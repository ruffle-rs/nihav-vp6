use std::cmp::max;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
pub use crate::formats::*;
pub use crate::refs::*;

#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
pub struct NAAudioInfo {
    sample_rate: u32,
    channels:    u8,
    format:      NASoniton,
    block_len:   usize,
}

impl NAAudioInfo {
    pub fn new(sr: u32, ch: u8, fmt: NASoniton, bl: usize) -> Self {
        NAAudioInfo { sample_rate: sr, channels: ch, format: fmt, block_len: bl }
    }
    pub fn get_sample_rate(&self) -> u32 { self.sample_rate }
    pub fn get_channels(&self) -> u8 { self.channels }
    pub fn get_format(&self) -> NASoniton { self.format }
    pub fn get_block_len(&self) -> usize { self.block_len }
}

impl fmt::Display for NAAudioInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} Hz, {} ch", self.sample_rate, self.channels)
    }
}

#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
pub struct NAVideoInfo {
    width:      usize,
    height:     usize,
    flipped:    bool,
    format:     NAPixelFormaton,
}

impl NAVideoInfo {
    pub fn new(w: usize, h: usize, flip: bool, fmt: NAPixelFormaton) -> Self {
        NAVideoInfo { width: w, height: h, flipped: flip, format: fmt }
    }
    pub fn get_width(&self)  -> usize { self.width as usize }
    pub fn get_height(&self) -> usize { self.height as usize }
    pub fn is_flipped(&self) -> bool { self.flipped }
    pub fn get_format(&self) -> NAPixelFormaton { self.format }
    pub fn set_width(&mut self, w: usize)  { self.width  = w; }
    pub fn set_height(&mut self, h: usize) { self.height = h; }
}

impl fmt::Display for NAVideoInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Clone,Copy,PartialEq)]
pub enum NACodecTypeInfo {
    None,
    Audio(NAAudioInfo),
    Video(NAVideoInfo),
}

impl NACodecTypeInfo {
    pub fn get_video_info(&self) -> Option<NAVideoInfo> {
        match *self {
            NACodecTypeInfo::Video(vinfo) => Some(vinfo),
            _ => None,
        }
    }
    pub fn get_audio_info(&self) -> Option<NAAudioInfo> {
        match *self {
            NACodecTypeInfo::Audio(ainfo) => Some(ainfo),
            _ => None,
        }
    }
    pub fn is_video(&self) -> bool {
        match *self {
            NACodecTypeInfo::Video(_) => true,
            _ => false,
        }
    }
    pub fn is_audio(&self) -> bool {
        match *self {
            NACodecTypeInfo::Audio(_) => true,
            _ => false,
        }
    }
}

impl fmt::Display for NACodecTypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ret = match *self {
            NACodecTypeInfo::None       => "".to_string(),
            NACodecTypeInfo::Audio(fmt) => format!("{}", fmt),
            NACodecTypeInfo::Video(fmt) => format!("{}", fmt),
        };
        write!(f, "{}", ret)
    }
}

#[derive(Clone)]
pub struct NAVideoBuffer<T> {
    info:    NAVideoInfo,
    data:    NABufferRef<Vec<T>>,
    offs:    Vec<usize>,
    strides: Vec<usize>,
}

impl<T: Clone> NAVideoBuffer<T> {
    pub fn get_offset(&self, idx: usize) -> usize {
        if idx >= self.offs.len() { 0 }
        else { self.offs[idx] }
    }
    pub fn get_info(&self) -> NAVideoInfo { self.info }
    pub fn get_data(&self) -> &Vec<T> { self.data.as_ref() }
    pub fn get_data_mut(&mut self) -> Option<&mut Vec<T>> { self.data.as_mut() }
    pub fn copy_buffer(&mut self) -> Self {
        let mut data: Vec<T> = Vec::with_capacity(self.data.len());
        data.clone_from(self.data.as_ref());
        let mut offs: Vec<usize> = Vec::with_capacity(self.offs.len());
        offs.clone_from(&self.offs);
        let mut strides: Vec<usize> = Vec::with_capacity(self.strides.len());
        strides.clone_from(&self.strides);
        NAVideoBuffer { info: self.info, data: NABufferRef::new(data), offs, strides }
    }
    pub fn get_stride(&self, idx: usize) -> usize {
        if idx >= self.strides.len() { return 0; }
        self.strides[idx]
    }
    pub fn get_dimensions(&self, idx: usize) -> (usize, usize) {
        get_plane_size(&self.info, idx)
    }
    pub fn into_ref(self) -> NABufferRef<Self> {
        NABufferRef::new(self)
    }
}

pub type NAVideoBufferRef<T> = NABufferRef<NAVideoBuffer<T>>;

#[derive(Clone)]
pub struct NAAudioBuffer<T> {
    info:   NAAudioInfo,
    data:   NABufferRef<Vec<T>>,
    offs:   Vec<usize>,
    chmap:  NAChannelMap,
    len:    usize,
}

impl<T: Clone> NAAudioBuffer<T> {
    pub fn get_offset(&self, idx: usize) -> usize {
        if idx >= self.offs.len() { 0 }
        else { self.offs[idx] }
    }
    pub fn get_info(&self) -> NAAudioInfo { self.info }
    pub fn get_chmap(&self) -> NAChannelMap { self.chmap.clone() }
    pub fn get_data(&self) -> &Vec<T> { self.data.as_ref() }
    pub fn get_data_mut(&mut self) -> Option<&mut Vec<T>> { self.data.as_mut() }
    pub fn copy_buffer(&mut self) -> Self {
        let mut data: Vec<T> = Vec::with_capacity(self.data.len());
        data.clone_from(self.data.as_ref());
        let mut offs: Vec<usize> = Vec::with_capacity(self.offs.len());
        offs.clone_from(&self.offs);
        NAAudioBuffer { info: self.info, data: NABufferRef::new(data), offs, chmap: self.get_chmap(), len: self.len }
    }
    pub fn get_length(&self) -> usize { self.len }
}

impl NAAudioBuffer<u8> {
    pub fn new_from_buf(info: NAAudioInfo, data: NABufferRef<Vec<u8>>, chmap: NAChannelMap) -> Self {
        let len = data.len();
        NAAudioBuffer { info, data, chmap, offs: Vec::new(), len }
    }
}

#[derive(Clone)]
pub enum NABufferType {
    Video      (NAVideoBufferRef<u8>),
    Video16    (NAVideoBufferRef<u16>),
    Video32    (NAVideoBufferRef<u32>),
    VideoPacked(NAVideoBufferRef<u8>),
    AudioU8    (NAAudioBuffer<u8>),
    AudioI16   (NAAudioBuffer<i16>),
    AudioI32   (NAAudioBuffer<i32>),
    AudioF32   (NAAudioBuffer<f32>),
    AudioPacked(NAAudioBuffer<u8>),
    Data       (NABufferRef<Vec<u8>>),
    None,
}

impl NABufferType {
    pub fn get_offset(&self, idx: usize) -> usize {
        match *self {
            NABufferType::Video(ref vb)       => vb.get_offset(idx),
            NABufferType::Video16(ref vb)     => vb.get_offset(idx),
            NABufferType::Video32(ref vb)     => vb.get_offset(idx),
            NABufferType::VideoPacked(ref vb) => vb.get_offset(idx),
            NABufferType::AudioU8(ref ab)     => ab.get_offset(idx),
            NABufferType::AudioI16(ref ab)    => ab.get_offset(idx),
            NABufferType::AudioF32(ref ab)    => ab.get_offset(idx),
            NABufferType::AudioPacked(ref ab) => ab.get_offset(idx),
            _ => 0,
        }
    }
    pub fn get_video_info(&self) -> Option<NAVideoInfo> {
        match *self {
            NABufferType::Video(ref vb)       => Some(vb.get_info()),
            NABufferType::Video16(ref vb)     => Some(vb.get_info()),
            NABufferType::Video32(ref vb)     => Some(vb.get_info()),
            NABufferType::VideoPacked(ref vb) => Some(vb.get_info()),
            _ => None,
        }
    }
    pub fn get_vbuf(&self) -> Option<NAVideoBufferRef<u8>> {
        match *self {
            NABufferType::Video(ref vb)       => Some(vb.clone()),
            NABufferType::VideoPacked(ref vb) => Some(vb.clone()),
            _ => None,
        }
    }
    pub fn get_vbuf16(&self) -> Option<NAVideoBufferRef<u16>> {
        match *self {
            NABufferType::Video16(ref vb)     => Some(vb.clone()),
            _ => None,
        }
    }
    pub fn get_vbuf32(&self) -> Option<NAVideoBufferRef<u32>> {
        match *self {
            NABufferType::Video32(ref vb)     => Some(vb.clone()),
            _ => None,
        }
    }
    pub fn get_abuf_u8(&self) -> Option<NAAudioBuffer<u8>> {
        match *self {
            NABufferType::AudioU8(ref ab) => Some(ab.clone()),
            NABufferType::AudioPacked(ref ab) => Some(ab.clone()),
            _ => None,
        }
    }
    pub fn get_abuf_i16(&self) -> Option<NAAudioBuffer<i16>> {
        match *self {
            NABufferType::AudioI16(ref ab) => Some(ab.clone()),
            _ => None,
        }
    }
    pub fn get_abuf_i32(&self) -> Option<NAAudioBuffer<i32>> {
        match *self {
            NABufferType::AudioI32(ref ab) => Some(ab.clone()),
            _ => None,
        }
    }
    pub fn get_abuf_f32(&self) -> Option<NAAudioBuffer<f32>> {
        match *self {
            NABufferType::AudioF32(ref ab) => Some(ab.clone()),
            _ => None,
        }
    }
}

const NA_SIMPLE_VFRAME_COMPONENTS: usize = 4;
pub struct NASimpleVideoFrame<'a, T: Copy> {
    pub width:      [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    pub height:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    pub flip:       bool,
    pub stride:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    pub offset:     [usize; NA_SIMPLE_VFRAME_COMPONENTS],
    pub components: usize,
    pub data:       &'a mut Vec<T>,
}

impl<'a, T:Copy> NASimpleVideoFrame<'a, T> {
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
            data: vbuf.data.as_mut().unwrap(),
            })
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum AllocatorError {
    TooLargeDimensions,
    FormatError,
}

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
            if !vinfo.is_flipped() {
                offs.push(new_size as usize);
            }
            let stride = chr.get_linesize(width);
            let cur_h = chr.get_height(height);
            let cur_sz = stride.checked_mul(cur_h);
            if cur_sz == None { return Err(AllocatorError::TooLargeDimensions); }
            let new_sz = new_size.checked_add(cur_sz.unwrap());
            if new_sz == None { return Err(AllocatorError::TooLargeDimensions); }
            new_size = new_sz.unwrap();
            if vinfo.is_flipped() {
                offs.push(new_size as usize);
            }
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

#[allow(clippy::collapsible_if)]
pub fn alloc_audio_buffer(ainfo: NAAudioInfo, nsamples: usize, chmap: NAChannelMap) -> Result<NABufferType, AllocatorError> {
    let mut offs: Vec<usize> = Vec::new();
    if ainfo.format.is_planar() || (ainfo.channels == 1 && (ainfo.format.get_bits() % 8) == 0) {
        let len = nsamples.checked_mul(ainfo.channels as usize);
        if len == None { return Err(AllocatorError::TooLargeDimensions); }
        let length = len.unwrap();
        for i in 0..ainfo.channels {
            offs.push((i as usize) * nsamples);
        }
        if ainfo.format.is_float() {
            if ainfo.format.get_bits() == 32 {
                let data: Vec<f32> = vec![0.0; length];
                let buf: NAAudioBuffer<f32> = NAAudioBuffer { data: NABufferRef::new(data), info: ainfo, offs, chmap, len: nsamples };
                Ok(NABufferType::AudioF32(buf))
            } else {
                Err(AllocatorError::TooLargeDimensions)
            }
        } else {
            if ainfo.format.get_bits() == 8 && !ainfo.format.is_signed() {
                let data: Vec<u8> = vec![0; length];
                let buf: NAAudioBuffer<u8> = NAAudioBuffer { data: NABufferRef::new(data), info: ainfo, offs, chmap, len: nsamples };
                Ok(NABufferType::AudioU8(buf))
            } else if ainfo.format.get_bits() == 16 && ainfo.format.is_signed() {
                let data: Vec<i16> = vec![0; length];
                let buf: NAAudioBuffer<i16> = NAAudioBuffer { data: NABufferRef::new(data), info: ainfo, offs, chmap, len: nsamples };
                Ok(NABufferType::AudioI16(buf))
            } else {
                Err(AllocatorError::TooLargeDimensions)
            }
        }
    } else {
        let len = nsamples.checked_mul(ainfo.channels as usize);
        if len == None { return Err(AllocatorError::TooLargeDimensions); }
        let length = ainfo.format.get_audio_size(len.unwrap() as u64);
        let data: Vec<u8> = vec![0; length];
        let buf: NAAudioBuffer<u8> = NAAudioBuffer { data: NABufferRef::new(data), info: ainfo, offs, chmap, len: nsamples };
        Ok(NABufferType::AudioPacked(buf))
    }
}

pub fn alloc_data_buffer(size: usize) -> Result<NABufferType, AllocatorError> {
    let data: Vec<u8> = vec![0; size];
    let buf: NABufferRef<Vec<u8>> = NABufferRef::new(data);
    Ok(NABufferType::Data(buf))
}

pub fn copy_buffer(buf: NABufferType) -> NABufferType {
    buf.clone()
}

pub struct NAVideoBufferPool<T:Copy> {
    pool:       Vec<NAVideoBufferRef<T>>,
    max_len:    usize,
    add_len:    usize,
}

impl<T:Copy> NAVideoBufferPool<T> {
    pub fn new(max_len: usize) -> Self {
        Self {
            pool:       Vec::with_capacity(max_len),
            max_len,
            add_len: 0,
        }
    }
    pub fn set_dec_bufs(&mut self, add_len: usize) {
        self.add_len = add_len;
    }
    pub fn get_free(&mut self) -> Option<NAVideoBufferRef<T>> {
        for e in self.pool.iter() {
            if e.get_num_refs() == 1 {
                return Some(e.clone());
            }
        }
        None
    }
    pub fn get_copy(&mut self, rbuf: &NAVideoBufferRef<T>) -> Option<NAVideoBufferRef<T>> {
        let mut dbuf = self.get_free()?;
        dbuf.data.copy_from_slice(&rbuf.data);
        Some(dbuf)
    }
    pub fn reset(&mut self) {
        self.pool.truncate(0);
    }
}

impl NAVideoBufferPool<u8> {
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

#[allow(dead_code)]
#[derive(Clone)]
pub struct NACodecInfo {
    name:       &'static str,
    properties: NACodecTypeInfo,
    extradata:  Option<Arc<Vec<u8>>>,
}

pub type NACodecInfoRef = Arc<NACodecInfo>;

impl NACodecInfo {
    pub fn new(name: &'static str, p: NACodecTypeInfo, edata: Option<Vec<u8>>) -> Self {
        let extradata = match edata {
            None => None,
            Some(vec) => Some(Arc::new(vec)),
        };
        NACodecInfo { name, properties: p, extradata }
    }
    pub fn new_ref(name: &'static str, p: NACodecTypeInfo, edata: Option<Arc<Vec<u8>>>) -> Self {
        NACodecInfo { name, properties: p, extradata: edata }
    }
    pub fn into_ref(self) -> NACodecInfoRef { Arc::new(self) }
    pub fn get_properties(&self) -> NACodecTypeInfo { self.properties }
    pub fn get_extradata(&self) -> Option<Arc<Vec<u8>>> {
        if let Some(ref vec) = self.extradata { return Some(vec.clone()); }
        None
    }
    pub fn get_name(&self) -> &'static str { self.name }
    pub fn is_video(&self) -> bool {
        if let NACodecTypeInfo::Video(_) = self.properties { return true; }
        false
    }
    pub fn is_audio(&self) -> bool {
        if let NACodecTypeInfo::Audio(_) = self.properties { return true; }
        false
    }
    pub fn new_dummy() -> Arc<Self> {
        Arc::new(DUMMY_CODEC_INFO)
    }
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

pub const DUMMY_CODEC_INFO: NACodecInfo = NACodecInfo {
                                name: "none",
                                properties: NACodecTypeInfo::None,
                                extradata: None };

#[derive(Debug,Clone)]
pub enum NAValue {
    None,
    Int(i32),
    Long(i64),
    String(String),
    Data(Arc<Vec<u8>>),
}

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum FrameType {
    I,
    P,
    B,
    Skip,
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

#[derive(Debug,Clone,Copy)]
pub struct NATimeInfo {
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    tb_num:         u32,
    tb_den:         u32,
}

impl NATimeInfo {
    pub fn new(pts: Option<u64>, dts: Option<u64>, duration: Option<u64>, tb_num: u32, tb_den: u32) -> Self {
        NATimeInfo { pts, dts, duration, tb_num, tb_den }
    }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_dts(&self) -> Option<u64> { self.dts }
    pub fn get_duration(&self) -> Option<u64> { self.duration }
    pub fn set_pts(&mut self, pts: Option<u64>) { self.pts = pts; }
    pub fn set_dts(&mut self, dts: Option<u64>) { self.dts = dts; }
    pub fn set_duration(&mut self, dur: Option<u64>) { self.duration = dur; }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct NAFrame {
    ts:             NATimeInfo,
    buffer:         NABufferType,
    info:           NACodecInfoRef,
    ftype:          FrameType,
    key:            bool,
    options:        HashMap<String, NAValue>,
}

pub type NAFrameRef = Arc<NAFrame>;

fn get_plane_size(info: &NAVideoInfo, idx: usize) -> (usize, usize) {
    let chromaton = info.get_format().get_chromaton(idx);
    if chromaton.is_none() { return (0, 0); }
    let (hs, vs) = chromaton.unwrap().get_subsampling();
    let w = (info.get_width()  + ((1 << hs) - 1)) >> hs;
    let h = (info.get_height() + ((1 << vs) - 1)) >> vs;
    (w, h)
}

impl NAFrame {
    pub fn new(ts:             NATimeInfo,
               ftype:          FrameType,
               keyframe:       bool,
               info:           NACodecInfoRef,
               options:        HashMap<String, NAValue>,
               buffer:         NABufferType) -> Self {
        NAFrame { ts, buffer, info, ftype, key: keyframe, options }
    }
    pub fn get_info(&self) -> NACodecInfoRef { self.info.clone() }
    pub fn get_frame_type(&self) -> FrameType { self.ftype }
    pub fn is_keyframe(&self) -> bool { self.key }
    pub fn set_frame_type(&mut self, ftype: FrameType) { self.ftype = ftype; }
    pub fn set_keyframe(&mut self, key: bool) { self.key = key; }
    pub fn get_time_information(&self) -> NATimeInfo { self.ts }
    pub fn get_pts(&self) -> Option<u64> { self.ts.get_pts() }
    pub fn get_dts(&self) -> Option<u64> { self.ts.get_dts() }
    pub fn get_duration(&self) -> Option<u64> { self.ts.get_duration() }
    pub fn set_pts(&mut self, pts: Option<u64>) { self.ts.set_pts(pts); }
    pub fn set_dts(&mut self, dts: Option<u64>) { self.ts.set_dts(dts); }
    pub fn set_duration(&mut self, dur: Option<u64>) { self.ts.set_duration(dur); }

    pub fn get_buffer(&self) -> NABufferType { self.buffer.clone() }

    pub fn into_ref(self) -> NAFrameRef { Arc::new(self) }
}

impl fmt::Display for NAFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut ostr = format!("frame type {}", self.ftype);
        if let Some(pts) = self.ts.pts { ostr = format!("{} pts {}", ostr, pts); }
        if let Some(dts) = self.ts.dts { ostr = format!("{} dts {}", ostr, dts); }
        if let Some(dur) = self.ts.duration { ostr = format!("{} duration {}", ostr, dur); }
        if self.key { ostr = format!("{} kf", ostr); }
        write!(f, "[{}]", ostr)
    }
}

/// Possible stream types.
#[derive(Debug,Clone,Copy)]
#[allow(dead_code)]
pub enum StreamType {
    /// video stream
    Video,
    /// audio stream
    Audio,
    /// subtitles
    Subtitles,
    /// any data stream (or might be an unrecognized audio/video stream)
    Data,
    /// nonexistent stream
    None,
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            StreamType::Video => write!(f, "Video"),
            StreamType::Audio => write!(f, "Audio"),
            StreamType::Subtitles => write!(f, "Subtitles"),
            StreamType::Data => write!(f, "Data"),
            StreamType::None => write!(f, "-"),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct NAStream {
    media_type:     StreamType,
    id:             u32,
    num:            usize,
    info:           NACodecInfoRef,
    tb_num:         u32,
    tb_den:         u32,
}

pub type NAStreamRef = Arc<NAStream>;

pub fn reduce_timebase(tb_num: u32, tb_den: u32) -> (u32, u32) {
    if tb_num == 0 { return (tb_num, tb_den); }
    if (tb_den % tb_num) == 0 { return (1, tb_den / tb_num); }

    let mut a = tb_num;
    let mut b = tb_den;

    while a != b {
        if a > b { a -= b; }
        else if b > a { b -= a; }
    }

    (tb_num / a, tb_den / a)
}

impl NAStream {
    pub fn new(mt: StreamType, id: u32, info: NACodecInfo, tb_num: u32, tb_den: u32) -> Self {
        let (n, d) = reduce_timebase(tb_num, tb_den);
        NAStream { media_type: mt, id, num: 0, info: info.into_ref(), tb_num: n, tb_den: d }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_num(&self) -> usize { self.num }
    pub fn set_num(&mut self, num: usize) { self.num = num; }
    pub fn get_info(&self) -> NACodecInfoRef { self.info.clone() }
    pub fn get_timebase(&self) -> (u32, u32) { (self.tb_num, self.tb_den) }
    pub fn set_timebase(&mut self, tb_num: u32, tb_den: u32) {
        let (n, d) = reduce_timebase(tb_num, tb_den);
        self.tb_num = n;
        self.tb_den = d;
    }
    pub fn into_ref(self) -> NAStreamRef { Arc::new(self) }
}

impl fmt::Display for NAStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}#{} @ {}/{} - {})", self.media_type, self.id, self.tb_num, self.tb_den, self.info.get_properties())
    }
}

#[allow(dead_code)]
pub struct NAPacket {
    stream:         NAStreamRef,
    ts:             NATimeInfo,
    buffer:         NABufferRef<Vec<u8>>,
    keyframe:       bool,
//    options:        HashMap<String, NAValue<'a>>,
}

impl NAPacket {
    pub fn new(str: NAStreamRef, ts: NATimeInfo, kf: bool, vec: Vec<u8>) -> Self {
//        let mut vec: Vec<u8> = Vec::new();
//        vec.resize(size, 0);
        NAPacket { stream: str, ts, keyframe: kf, buffer: NABufferRef::new(vec) }
    }
    pub fn get_stream(&self) -> NAStreamRef { self.stream.clone() }
    pub fn get_time_information(&self) -> NATimeInfo { self.ts }
    pub fn get_pts(&self) -> Option<u64> { self.ts.get_pts() }
    pub fn get_dts(&self) -> Option<u64> { self.ts.get_dts() }
    pub fn get_duration(&self) -> Option<u64> { self.ts.get_duration() }
    pub fn is_keyframe(&self) -> bool { self.keyframe }
    pub fn get_buffer(&self) -> NABufferRef<Vec<u8>> { self.buffer.clone() }
}

impl Drop for NAPacket {
    fn drop(&mut self) {}
}

impl fmt::Display for NAPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut ostr = format!("[pkt for {} size {}", self.stream, self.buffer.len());
        if let Some(pts) = self.ts.pts { ostr = format!("{} pts {}", ostr, pts); }
        if let Some(dts) = self.ts.dts { ostr = format!("{} dts {}", ostr, dts); }
        if let Some(dur) = self.ts.duration { ostr = format!("{} duration {}", ostr, dur); }
        if self.keyframe { ostr = format!("{} kf", ostr); }
        ostr += "]";
        write!(f, "{}", ostr)
    }
}

pub trait FrameFromPacket {
    fn new_from_pkt(pkt: &NAPacket, info: NACodecInfoRef, buf: NABufferType) -> NAFrame;
    fn fill_timestamps(&mut self, pkt: &NAPacket);
}

impl FrameFromPacket for NAFrame {
    fn new_from_pkt(pkt: &NAPacket, info: NACodecInfoRef, buf: NABufferType) -> NAFrame {
        NAFrame::new(pkt.ts, FrameType::Other, pkt.keyframe, info, HashMap::new(), buf)
    }
    fn fill_timestamps(&mut self, pkt: &NAPacket) {
        self.ts = pkt.get_time_information();
    }
}

