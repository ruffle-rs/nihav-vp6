use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use formats::*;

#[allow(dead_code)]
#[derive(Clone,Copy)]
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
}

impl fmt::Display for NAAudioInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} Hz, {} ch", self.sample_rate, self.channels)
    }
}

#[allow(dead_code)]
#[derive(Clone,Copy)]
pub struct NAVideoInfo {
    width:      u32,
    height:     u32,
    flipped:    bool,
    format:     NAPixelFormaton,
}

impl NAVideoInfo {
    pub fn new(w: u32, h: u32, flip: bool, fmt: NAPixelFormaton) -> Self {
        NAVideoInfo { width: w, height: h, flipped: flip, format: fmt }
    }
}

impl fmt::Display for NAVideoInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Clone,Copy)]
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


#[allow(dead_code)]
pub struct NABuffer<'a> {
    id:   u64,
    data: &'a mut [u8],
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct NACodecInfo {
    properties: NACodecTypeInfo,
    extradata:  Option<Rc<Vec<u8>>>,
}

impl NACodecInfo {
    pub fn new(p: NACodecTypeInfo, edata: Option<Vec<u8>>) -> Self {
        let extradata = match edata {
            None => None,
            Some(vec) => Some(Rc::new(vec)),
        };
        NACodecInfo { properties: p, extradata: extradata }
    }
    pub fn get_properties(&self) -> NACodecTypeInfo { self.properties }
    pub fn get_extradata(&self) -> Option<Rc<Vec<u8>>> {
        if let Some(ref vec) = self.extradata { return Some(vec.clone()); }
        None
    }
}

pub trait NABufferAllocator {
    fn alloc_buf(info: &NACodecInfo) -> NABuffer<'static>;
}

#[derive(Debug)]
pub enum NAValue<'a> {
    None,
    Int(i32),
    Long(i64),
    String(String),
    Data(&'a [u8]),
}

#[allow(dead_code)]
pub struct NAFrame<'a> {
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         &'a mut NABuffer<'a>,
    info:           &'a NACodecInfo,
    options:        HashMap<String, NAValue<'a>>,
}

#[allow(dead_code)]
pub struct NACodecContext<'a> {
    info:           &'a NACodecInfo,
}
