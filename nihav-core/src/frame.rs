//! Packets and decoded frames functionality.
use std::cmp::max;
//use std::collections::HashMap;
use std::fmt;
pub use std::sync::Arc;
pub use crate::formats::*;
pub use crate::refs::*;
use std::str::FromStr;

/// Video stream information.
#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
pub struct NAVideoInfo {
    /// Picture width.
    pub width:      usize,
    /// Picture height.
    pub height:     usize,
    /// Picture is stored downside up.
    pub flipped:    bool,
    /// Picture pixel format.
    pub format:     NAPixelFormaton,
    /// Declared bits per sample.
    pub bits:       u8,
}

impl NAVideoInfo {
    /// Constructs a new `NAVideoInfo` instance.
    pub fn new(w: usize, h: usize, flip: bool, fmt: NAPixelFormaton) -> Self {
        let bits = fmt.get_total_depth();
        NAVideoInfo { width: w, height: h, flipped: flip, format: fmt, bits }
    }
    /// Returns picture width.
    pub fn get_width(&self)  -> usize { self.width as usize }
    /// Returns picture height.
    pub fn get_height(&self) -> usize { self.height as usize }
    /// Returns picture orientation.
    pub fn is_flipped(&self) -> bool { self.flipped }
    /// Returns picture pixel format.
    pub fn get_format(&self) -> NAPixelFormaton { self.format }
    /// Sets new picture width.
    pub fn set_width(&mut self, w: usize)  { self.width  = w; }
    /// Sets new picture height.
    pub fn set_height(&mut self, h: usize) { self.height = h; }
}

impl fmt::Display for NAVideoInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// A list of possible stream information types.
#[derive(Clone,Copy,PartialEq)]
pub enum NACodecTypeInfo {
    /// No codec present.
    None,
    /// Video codec information.
    Video(NAVideoInfo),
}

impl NACodecTypeInfo {
    /// Returns video stream information.
    pub fn get_video_info(&self) -> Option<NAVideoInfo> {
        match *self {
            NACodecTypeInfo::Video(vinfo) => Some(vinfo),
            _ => None,
        }
    }
    /// Reports whether the current stream is video stream.
    pub fn is_video(&self) -> bool {
        match *self {
            NACodecTypeInfo::Video(_) => true,
            _ => false,
        }
    }
}

impl fmt::Display for NACodecTypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ret = match *self {
            NACodecTypeInfo::None       => "".to_string(),
            NACodecTypeInfo::Video(fmt) => format!("{}", fmt),
        };
        write!(f, "{}", ret)
    }
}

/// Decoded video frame.
///
/// NihAV frames are stored in native type (8/16/32-bit elements) inside a single buffer.
/// In case of image with several components those components are stored sequentially and can be accessed in the buffer starting at corresponding component offset.
#[derive(Clone)]
pub struct NAVideoBuffer<T> {
    info:    NAVideoInfo,
    data:    NABufferRef<Vec<T>>,
    offs:    Vec<usize>,
    strides: Vec<usize>,
}

impl<T: Clone> NAVideoBuffer<T> {
    /// Returns the component offset (0 for all unavailable offsets).
    pub fn get_offset(&self, idx: usize) -> usize {
        if idx >= self.offs.len() { 0 }
        else { self.offs[idx] }
    }
    /// Returns picture info.
    pub fn get_info(&self) -> NAVideoInfo { self.info }
    /// Returns an immutable reference to the data.
    pub fn get_data(&self) -> &Vec<T> { self.data.as_ref() }
    /// Returns a mutable reference to the data.
    pub fn get_data_mut(&mut self) -> Option<&mut Vec<T>> { self.data.as_mut() }
    /// Returns the number of components in picture format.
    pub fn get_num_components(&self) -> usize { self.offs.len() }
    /// Creates a copy of current `NAVideoBuffer`.
    pub fn copy_buffer(&mut self) -> Self {
        let mut data: Vec<T> = Vec::with_capacity(self.data.len());
        data.clone_from(self.data.as_ref());
        let mut offs: Vec<usize> = Vec::with_capacity(self.offs.len());
        offs.clone_from(&self.offs);
        let mut strides: Vec<usize> = Vec::with_capacity(self.strides.len());
        strides.clone_from(&self.strides);
        NAVideoBuffer { info: self.info, data: NABufferRef::new(data), offs, strides }
    }
    /// Returns stride (distance between subsequent lines) for the requested component.
    pub fn get_stride(&self, idx: usize) -> usize {
        if idx >= self.strides.len() { return 0; }
        self.strides[idx]
    }
    /// Returns requested component dimensions.
    pub fn get_dimensions(&self, idx: usize) -> (usize, usize) {
        get_plane_size(&self.info, idx)
    }
    /// Converts current instance into buffer reference.
    pub fn into_ref(self) -> NABufferRef<Self> {
        NABufferRef::new(self)
    }

    fn print_contents(&self, datatype: &str) {
        println!("{} video buffer size {}", datatype, self.data.len());
        println!(" format {}", self.info);
        print!(" offsets:");
        for off in self.offs.iter() {
            print!(" {}", *off);
        }
        println!();
        print!(" strides:");
        for stride in self.strides.iter() {
            print!(" {}", *stride);
        }
        println!();
    }
}

/// A specialised type for reference-counted `NAVideoBuffer`.
pub type NAVideoBufferRef<T> = NABufferRef<NAVideoBuffer<T>>;

/// A list of possible decoded frame types.
#[derive(Clone)]
pub enum NABufferType {
    /// 8-bit video buffer.
    Video      (NAVideoBufferRef<u8>),
    /// 16-bit video buffer (i.e. every component or packed pixel fits into 16 bits).
    Video16    (NAVideoBufferRef<u16>),
    /// 32-bit video buffer (i.e. every component or packed pixel fits into 32 bits).
    Video32    (NAVideoBufferRef<u32>),
    /// Packed video buffer.
    VideoPacked(NAVideoBufferRef<u8>),
    /// Buffer with generic data (e.g. subtitles).
    Data       (NABufferRef<Vec<u8>>),
    /// No data present.
    None,
}

impl NABufferType {
    /// Returns the offset to the requested component or channel.
    pub fn get_offset(&self, idx: usize) -> usize {
        match *self {
            NABufferType::Video(ref vb)       => vb.get_offset(idx),
            NABufferType::Video16(ref vb)     => vb.get_offset(idx),
            NABufferType::Video32(ref vb)     => vb.get_offset(idx),
            NABufferType::VideoPacked(ref vb) => vb.get_offset(idx),
            _ => 0,
        }
    }
    /// Returns information for video frames.
    pub fn get_video_info(&self) -> Option<NAVideoInfo> {
        match *self {
            NABufferType::Video(ref vb)       => Some(vb.get_info()),
            NABufferType::Video16(ref vb)     => Some(vb.get_info()),
            NABufferType::Video32(ref vb)     => Some(vb.get_info()),
            NABufferType::VideoPacked(ref vb) => Some(vb.get_info()),
            _ => None,
        }
    }
    /// Returns reference to 8-bit (or packed) video buffer.
    pub fn get_vbuf(&self) -> Option<NAVideoBufferRef<u8>> {
        match *self {
            NABufferType::Video(ref vb)       => Some(vb.clone()),
            NABufferType::VideoPacked(ref vb) => Some(vb.clone()),
            _ => None,
        }
    }
    /// Returns reference to 16-bit video buffer.
    pub fn get_vbuf16(&self) -> Option<NAVideoBufferRef<u16>> {
        match *self {
            NABufferType::Video16(ref vb)     => Some(vb.clone()),
            _ => None,
        }
    }
    /// Returns reference to 32-bit video buffer.
    pub fn get_vbuf32(&self) -> Option<NAVideoBufferRef<u32>> {
        match *self {
            NABufferType::Video32(ref vb)     => Some(vb.clone()),
            _ => None,
        }
    }
    /// Prints internal buffer layout.
    pub fn print_buffer_metadata(&self) {
        match *self {
            NABufferType::Video(ref buf)        => buf.print_contents("8-bit"),
            NABufferType::Video16(ref buf)      => buf.print_contents("16-bit"),
            NABufferType::Video32(ref buf)      => buf.print_contents("32-bit"),
            NABufferType::VideoPacked(ref buf)  => buf.print_contents("packed"),
            NABufferType::Data(ref buf) => { println!("Data buffer, len = {}", buf.len()); },
            NABufferType::None          => { println!("No buffer"); },
        };
    }
}

const NA_SIMPLE_VFRAME_COMPONENTS: usize = 4;
/// Simplified decoded frame data.
pub struct NASimpleVideoFrame<'a, T: Copy> {
    /// Widths of each picture component.
    pub width:      [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    /// Heights of each picture component.
    pub height:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    /// Orientation (upside-down or downside-up) flag.
    pub flip:       bool,
    /// Strides for each component.
    pub stride:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    /// Start of each component.
    pub offset:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    /// Number of components.
    pub components: usize,
    /// Pointer to the picture pixel data.
    pub data:       &'a mut [T],
}

impl<'a, T:Copy> NASimpleVideoFrame<'a, T> {
    /// Constructs a new instance of `NASimpleVideoFrame` from `NAVideoBuffer`.
    pub fn from_video_buf(vbuf: &'a mut NAVideoBuffer<T>) -> Option<Self> {
        let vinfo = vbuf.get_info();
        let components = vinfo.format.components as usize;
        if components > NA_SIMPLE_VFRAME_COMPONENTS {
            return None;
        }
        let mut w: [usize; NA_SIMPLE_VFRAME_COMPONENTS] = [0; NA_SIMPLE_VFRAME_COMPONENTS];
        let mut h: [usize; NA_SIMPLE_VFRAME_COMPONENTS] = [0; NA_SIMPLE_VFRAME_COMPONENTS];
        let mut s: [usize; NA_SIMPLE_VFRAME_COMPONENTS] = [0; NA_SIMPLE_VFRAME_COMPONENTS];
        let mut o: [usize; NA_SIMPLE_VFRAME_COMPONENTS] = [0; NA_SIMPLE_VFRAME_COMPONENTS];
        for comp in 0..components {
            let (width, height) = vbuf.get_dimensions(comp);
            w[comp] = width;
            h[comp] = height;
            s[comp] = vbuf.get_stride(comp);
            o[comp] = vbuf.get_offset(comp);
        }
        let flip = vinfo.flipped;
        Some(NASimpleVideoFrame {
            width:  w,
            height: h,
            flip,
            stride: s,
            offset: o,
            components,
            data: vbuf.data.as_mut_slice(),
            })
    }
}

/// A list of possible frame allocator errors.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum AllocatorError {
    /// Requested picture dimensions are too large.
    TooLargeDimensions,
    /// Invalid input format.
    FormatError,
}

/// Constructs a new video buffer with requested format.
///
/// `align` is power of two alignment for image. E.g. the value of 5 means that frame dimensions will be padded to be multiple of 32.
pub fn alloc_video_buffer(vinfo: NAVideoInfo, align: u8) -> Result<NABufferType, AllocatorError> {
    let fmt = &vinfo.format;
    let mut new_size: usize = 0;
    let mut offs:    Vec<usize> = Vec::new();
    let mut strides: Vec<usize> = Vec::new();

    for i in 0..fmt.get_num_comp() {
        if fmt.get_chromaton(i) == None { return Err(AllocatorError::FormatError); }
    }

    let align_mod = ((1 << align) as usize) - 1;
    let width  = ((vinfo.width  as usize) + align_mod) & !align_mod;
    let height = ((vinfo.height as usize) + align_mod) & !align_mod;
    let mut max_depth = 0;
    let mut all_packed = true;
    let mut all_bytealigned = true;
    for i in 0..fmt.get_num_comp() {
        let ochr = fmt.get_chromaton(i);
        if ochr.is_none() { continue; }
        let chr = ochr.unwrap();
        if !chr.is_packed() {
            all_packed = false;
        } else if ((chr.get_shift() + chr.get_depth()) & 7) != 0 {
            all_bytealigned = false;
        }
        max_depth = max(max_depth, chr.get_depth());
    }
    let unfit_elem_size = match fmt.get_elem_size() {
            2 | 4 => false,
            _ => true,
        };

//todo semi-packed like NV12
    if fmt.is_paletted() {
//todo various-sized palettes?
        let stride = vinfo.get_format().get_chromaton(0).unwrap().get_linesize(width);
        let pic_sz = stride.checked_mul(height);
        if pic_sz == None { return Err(AllocatorError::TooLargeDimensions); }
        let pal_size = 256 * (fmt.get_elem_size() as usize);
        let new_size = pic_sz.unwrap().checked_add(pal_size);
        if new_size == None { return Err(AllocatorError::TooLargeDimensions); }
        offs.push(0);
        offs.push(stride * height);
        strides.push(stride);
        let data: Vec<u8> = vec![0; new_size.unwrap()];
        let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
        Ok(NABufferType::Video(buf.into_ref()))
    } else if !all_packed {
        for i in 0..fmt.get_num_comp() {
            let ochr = fmt.get_chromaton(i);
            if ochr.is_none() { continue; }
            let chr = ochr.unwrap();
            offs.push(new_size as usize);
            let stride = chr.get_linesize(width);
            let cur_h = chr.get_height(height);
            let cur_sz = stride.checked_mul(cur_h);
            if cur_sz == None { return Err(AllocatorError::TooLargeDimensions); }
            let new_sz = new_size.checked_add(cur_sz.unwrap());
            if new_sz == None { return Err(AllocatorError::TooLargeDimensions); }
            new_size = new_sz.unwrap();
            strides.push(stride);
        }
        if max_depth <= 8 {
            let data: Vec<u8> = vec![0; new_size];
            let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video(buf.into_ref()))
        } else if max_depth <= 16 {
            let data: Vec<u16> = vec![0; new_size];
            let buf: NAVideoBuffer<u16> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video16(buf.into_ref()))
        } else {
            let data: Vec<u32> = vec![0; new_size];
            let buf: NAVideoBuffer<u32> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video32(buf.into_ref()))
        }
    } else if all_bytealigned || unfit_elem_size {
        let elem_sz = fmt.get_elem_size();
        let line_sz = width.checked_mul(elem_sz as usize);
        if line_sz == None { return Err(AllocatorError::TooLargeDimensions); }
        let new_sz = line_sz.unwrap().checked_mul(height);
        if new_sz == None { return Err(AllocatorError::TooLargeDimensions); }
        new_size = new_sz.unwrap();
        let data: Vec<u8> = vec![0; new_size];
        strides.push(line_sz.unwrap());
        let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
        Ok(NABufferType::VideoPacked(buf.into_ref()))
    } else {
        let elem_sz = fmt.get_elem_size();
        let new_sz = width.checked_mul(height);
        if new_sz == None { return Err(AllocatorError::TooLargeDimensions); }
        new_size = new_sz.unwrap();
        match elem_sz {
            2 => {
                    let data: Vec<u16> = vec![0; new_size];
                    strides.push(width);
                    let buf: NAVideoBuffer<u16> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
                    Ok(NABufferType::Video16(buf.into_ref()))
                },
            4 => {
                    let data: Vec<u32> = vec![0; new_size];
                    strides.push(width);
                    let buf: NAVideoBuffer<u32> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
                    Ok(NABufferType::Video32(buf.into_ref()))
                },
            _ => unreachable!(),
        }
    }
}

/// Constructs a new buffer for generic data.
pub fn alloc_data_buffer(size: usize) -> Result<NABufferType, AllocatorError> {
    let data: Vec<u8> = vec![0; size];
    let buf: NABufferRef<Vec<u8>> = NABufferRef::new(data);
    Ok(NABufferType::Data(buf))
}

/// Creates a clone of current buffer.
pub fn copy_buffer(buf: &NABufferType) -> NABufferType {
    buf.clone()
}

/// Video frame pool.
///
/// This structure allows codec to effectively reuse old frames instead of allocating and de-allocating frames every time.
/// Caller can also reserve some frames for its own purposes e.g. display queue.
pub struct NAVideoBufferPool<T:Copy> {
    pool:       Vec<NAVideoBufferRef<T>>,
    max_len:    usize,
    add_len:    usize,
}

impl<T:Copy> NAVideoBufferPool<T> {
    /// Constructs a new `NAVideoBufferPool` instance.
    pub fn new(max_len: usize) -> Self {
        Self {
            pool:       Vec::with_capacity(max_len),
            max_len,
            add_len: 0,
        }
    }
    /// Sets the number of buffers reserved for the user.
    pub fn set_dec_bufs(&mut self, add_len: usize) {
        self.add_len = add_len;
    }
    /// Returns an unused buffer from the pool.
    pub fn get_free(&mut self) -> Option<NAVideoBufferRef<T>> {
        for e in self.pool.iter() {
            if e.get_num_refs() == 1 {
                return Some(e.clone());
            }
        }
        None
    }
    /// Clones provided frame data into a free pool frame.
    pub fn get_copy(&mut self, rbuf: &NAVideoBufferRef<T>) -> Option<NAVideoBufferRef<T>> {
        let mut dbuf = self.get_free()?;
        dbuf.data.copy_from_slice(&rbuf.data);
        Some(dbuf)
    }
    /// Clears the pool from all frames.
    pub fn reset(&mut self) {
        self.pool.truncate(0);
    }
}

impl NAVideoBufferPool<u8> {
    /// Allocates the target amount of video frames using [`alloc_video_buffer`].
    ///
    /// [`alloc_video_buffer`]: ./fn.alloc_video_buffer.html
    pub fn prealloc_video(&mut self, vinfo: NAVideoInfo, align: u8) -> Result<(), AllocatorError> {
        let nbufs = self.max_len + self.add_len - self.pool.len();
        for _ in 0..nbufs {
            let vbuf = alloc_video_buffer(vinfo, align)?;
            if let NABufferType::Video(buf) = vbuf {
                self.pool.push(buf);
            } else if let NABufferType::VideoPacked(buf) = vbuf {
                self.pool.push(buf);
            } else {
                return Err(AllocatorError::FormatError);
            }
        }
        Ok(())
    }
}

impl NAVideoBufferPool<u16> {
    /// Allocates the target amount of video frames using [`alloc_video_buffer`].
    ///
    /// [`alloc_video_buffer`]: ./fn.alloc_video_buffer.html
    pub fn prealloc_video(&mut self, vinfo: NAVideoInfo, align: u8) -> Result<(), AllocatorError> {
        let nbufs = self.max_len + self.add_len - self.pool.len();
        for _ in 0..nbufs {
            let vbuf = alloc_video_buffer(vinfo, align)?;
            if let NABufferType::Video16(buf) = vbuf {
                self.pool.push(buf);
            } else {
                return Err(AllocatorError::FormatError);
            }
        }
        Ok(())
    }
}

impl NAVideoBufferPool<u32> {
    /// Allocates the target amount of video frames using [`alloc_video_buffer`].
    ///
    /// [`alloc_video_buffer`]: ./fn.alloc_video_buffer.html
    pub fn prealloc_video(&mut self, vinfo: NAVideoInfo, align: u8) -> Result<(), AllocatorError> {
        let nbufs = self.max_len + self.add_len - self.pool.len();
        for _ in 0..nbufs {
            let vbuf = alloc_video_buffer(vinfo, align)?;
            if let NABufferType::Video32(buf) = vbuf {
                self.pool.push(buf);
            } else {
                return Err(AllocatorError::FormatError);
            }
        }
        Ok(())
    }
}

/// Information about codec contained in a stream.
#[allow(dead_code)]
#[derive(Clone)]
pub struct NACodecInfo {
    name:       &'static str,
    properties: NACodecTypeInfo,
    extradata:  Option<Arc<Vec<u8>>>,
}

/// A specialised type for reference-counted `NACodecInfo`.
pub type NACodecInfoRef = Arc<NACodecInfo>;

impl NACodecInfo {
    /// Constructs a new instance of `NACodecInfo`.
    pub fn new(name: &'static str, p: NACodecTypeInfo, edata: Option<Vec<u8>>) -> Self {
        let extradata = match edata {
            None => None,
            Some(vec) => Some(Arc::new(vec)),
        };
        NACodecInfo { name, properties: p, extradata }
    }
    /// Constructs a new reference-counted instance of `NACodecInfo`.
    pub fn new_ref(name: &'static str, p: NACodecTypeInfo, edata: Option<Arc<Vec<u8>>>) -> Self {
        NACodecInfo { name, properties: p, extradata: edata }
    }
    /// Converts current instance into a reference-counted one.
    pub fn into_ref(self) -> NACodecInfoRef { Arc::new(self) }
    /// Returns codec information.
    pub fn get_properties(&self) -> NACodecTypeInfo { self.properties }
    /// Returns additional initialisation data required by the codec.
    pub fn get_extradata(&self) -> Option<Arc<Vec<u8>>> {
        if let Some(ref vec) = self.extradata { return Some(vec.clone()); }
        None
    }
    /// Returns codec name.
    pub fn get_name(&self) -> &'static str { self.name }
    /// Reports whether it is a video codec.
    pub fn is_video(&self) -> bool {
        if let NACodecTypeInfo::Video(_) = self.properties { return true; }
        false
    }
    /// Constructs a new empty reference-counted instance of `NACodecInfo`.
    pub fn new_dummy() -> Arc<Self> {
        Arc::new(DUMMY_CODEC_INFO)
    }
    /// Updates codec infomation.
    pub fn replace_info(&self, p: NACodecTypeInfo) -> Arc<Self> {
        Arc::new(NACodecInfo { name: self.name, properties: p, extradata: self.extradata.clone() })
    }
}

impl Default for NACodecInfo {
    fn default() -> Self { DUMMY_CODEC_INFO }
}

impl fmt::Display for NACodecInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let edata = match self.extradata.clone() {
            None => "no extradata".to_string(),
            Some(v) => format!("{} byte(s) of extradata", v.len()),
        };
        write!(f, "{}: {} {}", self.name, self.properties, edata)
    }
}

/// Default empty codec information.
pub const DUMMY_CODEC_INFO: NACodecInfo = NACodecInfo {
                                name: "none",
                                properties: NACodecTypeInfo::None,
                                extradata: None };

/// A list of recognized frame types.
#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum FrameType {
    /// Intra frame type.
    I,
    /// Inter frame type.
    P,
    /// Bidirectionally predicted frame.
    B,
    /// Skip frame.
    ///
    /// When such frame is encountered then last frame should be used again if it is needed.
    Skip,
    /// Some other frame type.
    Other,
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FrameType::I => write!(f, "I"),
            FrameType::P => write!(f, "P"),
            FrameType::B => write!(f, "B"),
            FrameType::Skip => write!(f, "skip"),
            FrameType::Other => write!(f, "x"),
        }
    }
}

fn get_plane_size(info: &NAVideoInfo, idx: usize) -> (usize, usize) {
    let chromaton = info.get_format().get_chromaton(idx);
    if chromaton.is_none() { return (0, 0); }
    let (hs, vs) = chromaton.unwrap().get_subsampling();
    let w = (info.get_width()  + ((1 << hs) - 1)) >> hs;
    let h = (info.get_height() + ((1 << vs) - 1)) >> vs;
    (w, h)
}
