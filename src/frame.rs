use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::cell::*;
use formats::*;

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

impl fmt::Display for NACodecTypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ret = match *self {
            NACodecTypeInfo::None       => format!(""),
            NACodecTypeInfo::Audio(fmt) => format!("{}", fmt),
            NACodecTypeInfo::Video(fmt) => format!("{}", fmt),
        };
        write!(f, "{}", ret)
    }
}

pub type BufferRef = Rc<RefCell<Vec<u8>>>;

#[allow(dead_code)]
#[derive(Clone)]
pub struct NABuffer {
    id:   u64,
    data: BufferRef,
}

impl Drop for NABuffer {
    fn drop(&mut self) { }
}

impl NABuffer {
    pub fn get_data(&self) -> Ref<Vec<u8>> { self.data.borrow() }
    pub fn get_data_mut(&mut self) -> RefMut<Vec<u8>> { self.data.borrow_mut() }
}

pub type NABufferRef = Rc<RefCell<NABuffer>>;

#[allow(dead_code)]
#[derive(Clone)]
pub struct NACodecInfo {
    name:       &'static str,
    properties: NACodecTypeInfo,
    extradata:  Option<Rc<Vec<u8>>>,
}

impl NACodecInfo {
    pub fn new(name: &'static str, p: NACodecTypeInfo, edata: Option<Vec<u8>>) -> Self {
        let extradata = match edata {
            None => None,
            Some(vec) => Some(Rc::new(vec)),
        };
        NACodecInfo { name: name, properties: p, extradata: extradata }
    }
    pub fn new_ref(name: &'static str, p: NACodecTypeInfo, edata: Option<Rc<Vec<u8>>>) -> Self {
        NACodecInfo { name: name, properties: p, extradata: edata }
    }
    pub fn get_properties(&self) -> NACodecTypeInfo { self.properties }
    pub fn get_extradata(&self) -> Option<Rc<Vec<u8>>> {
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
}

impl fmt::Display for NACodecInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let edata = match self.extradata.clone() {
            None => format!("no extradata"),
            Some(v) => format!("{} byte(s) of extradata", v.len()),
        };
        write!(f, "{}: {} {}", self.name, self.properties, edata)
    }
}

pub const DUMMY_CODEC_INFO: NACodecInfo = NACodecInfo {
                                name: "none",
                                properties: NACodecTypeInfo::None,
                                extradata: None };

fn alloc_video_buf(vinfo: NAVideoInfo, data: &mut Vec<u8>, offs: &mut Vec<usize>) {
//todo use overflow detection mul
    let width = vinfo.width as usize;
    let height = vinfo.height as usize;
    let fmt = &vinfo.format;
    let mut new_size = 0;
    for i in 0..fmt.get_num_comp() {
        let chr = fmt.get_chromaton(i).unwrap();
        if !vinfo.is_flipped() {
            offs.push(new_size as usize);
        }
        new_size += chr.get_data_size(width, height);
        if vinfo.is_flipped() {
            offs.push(new_size as usize);
        }
    }
    data.resize(new_size, 0);
}

fn alloc_audio_buf(ainfo: NAAudioInfo, data: &mut Vec<u8>, offs: &mut Vec<usize>) {
//todo better alloc
    let length = ((ainfo.sample_rate as usize) * (ainfo.format.get_bits() as usize)) >> 3;
    let new_size: usize = length * (ainfo.channels as usize);
    data.resize(new_size, 0);
    for i in 0..ainfo.channels {
        if ainfo.format.is_planar() {
            offs.push((i as usize) * length);
        } else {
            offs.push(((i * ainfo.format.get_bits()) >> 3) as usize);
        }
    }
}

pub fn alloc_buf(info: &NACodecInfo) -> (NABufferRef, Vec<usize>) {
    let mut data: Vec<u8> = Vec::new();
    let mut offs: Vec<usize> = Vec::new();
    match info.properties {
        NACodecTypeInfo::Audio(ainfo) => alloc_audio_buf(ainfo, &mut data, &mut offs),
        NACodecTypeInfo::Video(vinfo) => alloc_video_buf(vinfo, &mut data, &mut offs),
        _ => (),
    }
    (Rc::new(RefCell::new(NABuffer { id: 0, data: Rc::new(RefCell::new(data)) })), offs)
}

pub fn copy_buf(buf: &NABuffer) -> NABufferRef {
    let mut data: Vec<u8> = Vec::new();
    data.clone_from(buf.get_data().as_ref());
    Rc::new(RefCell::new(NABuffer { id: 0, data: Rc::new(RefCell::new(data)) }))
}

#[derive(Debug,Clone)]
pub enum NAValue {
    None,
    Int(i32),
    Long(i64),
    String(String),
    Data(Rc<Vec<u8>>),
}

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum FrameType {
    I,
    P,
    B,
    Other,
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FrameType::I => write!(f, "I"),
            FrameType::P => write!(f, "P"),
            FrameType::B => write!(f, "B"),
            FrameType::Other => write!(f, "x"),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct NAFrame {
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         NABufferRef,
    info:           Rc<NACodecInfo>,
    ftype:          FrameType,
    key:            bool,
    offsets:        Vec<usize>,
    options:        HashMap<String, NAValue>,
}

fn get_plane_size(info: &NAVideoInfo, idx: usize) -> (usize, usize) {
    let chromaton = info.get_format().get_chromaton(idx);
    if let None = chromaton { return (0, 0); }
    let (hs, vs) = chromaton.unwrap().get_subsampling();
    let w = (info.get_width()  + ((1 << hs) - 1)) >> hs;
    let h = (info.get_height() + ((1 << vs) - 1)) >> vs;
    (w, h)
}

impl NAFrame {
    pub fn new(pts:            Option<u64>,
               dts:            Option<u64>,
               duration:       Option<u64>,
               ftype:          FrameType,
               keyframe:       bool,
               info:           Rc<NACodecInfo>,
               options:        HashMap<String, NAValue>) -> Self {
        let (buf, offs) = alloc_buf(&info);
        NAFrame { pts: pts, dts: dts, duration: duration, buffer: buf, offsets: offs, info: info, ftype: ftype, key: keyframe, options: options }
    }
    pub fn from_copy(src: &NAFrame) -> Self {
        let buf = copy_buf(&src.get_buffer());
        let mut offs: Vec<usize> = Vec::new();
        offs.clone_from(&src.offsets);
        NAFrame { pts: None, dts: None, duration: None, buffer: buf, offsets: offs, info: src.info.clone(), ftype: src.ftype, key: src.key, options: src.options.clone() }
    }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_dts(&self) -> Option<u64> { self.dts }
    pub fn get_duration(&self) -> Option<u64> { self.duration }
    pub fn get_frame_type(&self) -> FrameType { self.ftype }
    pub fn is_keyframe(&self) -> bool { self.key }
    pub fn set_pts(&mut self, pts: Option<u64>) { self.pts = pts; }
    pub fn set_dts(&mut self, dts: Option<u64>) { self.dts = dts; }
    pub fn set_duration(&mut self, dur: Option<u64>) { self.duration = dur; }
    pub fn set_frame_type(&mut self, ftype: FrameType) { self.ftype = ftype; }
    pub fn set_keyframe(&mut self, key: bool) { self.key = key; }

    pub fn get_offset(&self, idx: usize) -> usize { self.offsets[idx] }
    pub fn get_buffer(&self) -> Ref<NABuffer> { self.buffer.borrow() }
    pub fn get_buffer_mut(&mut self) -> RefMut<NABuffer> { self.buffer.borrow_mut() }
    pub fn get_stride(&self, idx: usize) -> usize {
        if let NACodecTypeInfo::Video(vinfo) = self.info.get_properties() {
            if idx >= vinfo.get_format().get_num_comp() { return 0; }
            vinfo.get_format().get_chromaton(idx).unwrap().get_linesize(vinfo.get_width())
        } else {
            0
        }
    }
    pub fn get_dimensions(&self, idx: usize) -> (usize, usize) {
        match self.info.get_properties() {
            NACodecTypeInfo::Video(ref vinfo) => get_plane_size(vinfo, idx),
            _ => (0, 0),
        }
    }
}

pub type NAFrameRef = Rc<RefCell<NAFrame>>;

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
    info:           Rc<NACodecInfo>,
}

impl NAStream {
    pub fn new(mt: StreamType, id: u32, info: NACodecInfo) -> Self {
        NAStream { media_type: mt, id: id, num: 0, info: Rc::new(info) }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_num(&self) -> usize { self.num }
    pub fn set_num(&mut self, num: usize) { self.num = num; }
    pub fn get_info(&self) -> Rc<NACodecInfo> { self.info.clone() }
}

impl fmt::Display for NAStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}#{} - {})", self.media_type, self.id, self.info.get_properties())
    }
}

#[allow(dead_code)]
pub struct NAPacket {
    stream:         Rc<NAStream>,
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         Rc<Vec<u8>>,
    keyframe:       bool,
//    options:        HashMap<String, NAValue<'a>>,
}

impl NAPacket {
    pub fn new(str: Rc<NAStream>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, kf: bool, vec: Vec<u8>) -> Self {
//        let mut vec: Vec<u8> = Vec::new();
//        vec.resize(size, 0);
        NAPacket { stream: str, pts: pts, dts: dts, duration: dur, keyframe: kf, buffer: Rc::new(vec) }
    }
    pub fn get_stream(&self) -> Rc<NAStream> { self.stream.clone() }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_dts(&self) -> Option<u64> { self.dts }
    pub fn get_duration(&self) -> Option<u64> { self.duration }
    pub fn is_keyframe(&self) -> bool { self.keyframe }
    pub fn get_buffer(&self) -> Rc<Vec<u8>> { self.buffer.clone() }
}

impl Drop for NAPacket {
    fn drop(&mut self) {}
}

impl fmt::Display for NAPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut foo = format!("[pkt for {} size {}", self.stream, self.buffer.len());
        if let Some(pts) = self.pts { foo = format!("{} pts {}", foo, pts); }
        if let Some(dts) = self.dts { foo = format!("{} dts {}", foo, dts); }
        if let Some(dur) = self.duration { foo = format!("{} duration {}", foo, dur); }
        if self.keyframe { foo = format!("{} kf", foo); }
        foo = foo + "]";
        write!(f, "{}", foo)
    }
}

pub trait FrameFromPacket {
    fn new_from_pkt(pkt: &NAPacket, info: Rc<NACodecInfo>) -> NAFrame;
    fn fill_timestamps(&mut self, pkt: &NAPacket);
}

impl FrameFromPacket for NAFrame {
    fn new_from_pkt(pkt: &NAPacket, info: Rc<NACodecInfo>) -> NAFrame {
        NAFrame::new(pkt.pts, pkt.dts, pkt.duration, FrameType::Other, pkt.keyframe, info, HashMap::new())
    }
    fn fill_timestamps(&mut self, pkt: &NAPacket) {
        self.set_pts(pkt.pts);
        self.set_dts(pkt.dts);
        self.set_duration(pkt.duration);
    }
}

