use std::collections::HashMap;
use std::rc::Rc;

#[allow(dead_code)]
#[derive(Copy,Clone)]
pub struct NASoniton {
    bits:       u8,
    is_be:      bool,
    packed:     bool,
    planar:     bool,
    float:      bool,
    signed:     bool,
}

#[allow(dead_code)]
pub const SND_U8_FORMAT: NASoniton = NASoniton { bits: 8, is_be: false, packed: false, planar: false, float: false, signed: false };
#[allow(dead_code)]
pub const SND_S16_FORMAT: NASoniton = NASoniton { bits: 16, is_be: false, packed: false, planar: false, float: false, signed: true };

#[derive(Debug,Clone,Copy)]
pub enum NAChannelType {
    C, L, R, Ls, Rs, Lss, Rss, LFE, Lc, Rc, Lh, Rh, Ch, LFE2, Lw, Rw, Ov, Lhs, Rhs, Chr, Ll, Rl, Cl, Lt, Rt, Lo, Ro
}

impl NAChannelType {
    pub fn is_center(&self) -> bool {
        match *self {
            NAChannelType::C => true,   NAChannelType::Ch => true,
            NAChannelType::Cl => true,  NAChannelType::Ov => true,
            NAChannelType::LFE => true, NAChannelType::LFE2 => true,
            _ => false,
        }
    }
    pub fn is_left(&self) -> bool {
        match *self {
            NAChannelType::L   => true, NAChannelType::Ls => true,
            NAChannelType::Lss => true, NAChannelType::Lc => true,
            NAChannelType::Lh  => true, NAChannelType::Lw => true,
            NAChannelType::Lhs => true, NAChannelType::Ll => true,
            NAChannelType::Lt  => true, NAChannelType::Lo => true,
            _ => false,
        }
    }
    pub fn is_right(&self) -> bool {
        match *self {
            NAChannelType::R   => true, NAChannelType::Rs => true,
            NAChannelType::Rss => true, NAChannelType::Rc => true,
            NAChannelType::Rh  => true, NAChannelType::Rw => true,
            NAChannelType::Rhs => true, NAChannelType::Rl => true,
            NAChannelType::Rt  => true, NAChannelType::Ro => true,
            _ => false,
        }
    }
}

pub struct NAChannelMap {
    ids: Vec<NAChannelType>,
}

impl NAChannelMap {
    pub fn new() -> Self { NAChannelMap { ids: Vec::new() } }
    pub fn add_channel(&mut self, ch: NAChannelType) {
        self.ids.push(ch);
    }
    pub fn num_channels(&self) -> usize {
        self.ids.len()
    }
    pub fn get_channel(&self, idx: usize) -> NAChannelType {
        self.ids[idx]
    }
    pub fn find_channel_id(&self, t: NAChannelType) -> Option<u8> {
        for i in 0..self.ids.len() {
            if self.ids[i] as i32 == t as i32 { return Some(i as u8); }
        }
        None
    }
}

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

#[derive(Debug,Clone,Copy)]
pub enum ColorModel {
    RGB,
    YUV,
    CMYK,
    HSV,
    LAB,
}

#[allow(dead_code)]
#[derive(Clone,Copy)]
pub struct NAPixelChromaton {
    h_ss:           u8,
    v_ss:           u8,
    is_packed:      bool,
    depth:          u8,
    shift:          u8,
    comp_offs:      u8,
    next_elem:      u8,
}

#[allow(dead_code)]
#[derive(Clone,Copy)]
pub struct NAPixelFormaton {
    model:      ColorModel,
    components: u8,
    comp_info:  [Option<NAPixelChromaton>; 5],
    elem_size:  u8,
    has_alpha:  bool,
    is_palette: bool,
}

macro_rules! chromaton {
    ($hs: expr, $vs: expr, $pck: expr, $d: expr, $sh: expr, $co: expr, $ne: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, is_packed: $pck, depth: $d, shift: $sh, comp_offs: $co, next_elem: $ne })
    });
    (yuv8; $hs: expr, $vs: expr, $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, is_packed: false, depth: 8, shift: 0, comp_offs: $co, next_elem: 1 })
    });
    (pal8; $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, is_packed: true, depth: 8, shift: 0, comp_offs: $co, next_elem: 3 })
    });
}

#[allow(dead_code)]
pub const YUV420_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV, components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 1, 1, 1),
                                            chromaton!(yuv8; 1, 1, 2),
                                            None, None],
                                        elem_size: 0, has_alpha: false, is_palette: false };

#[allow(dead_code)]
pub const PAL8_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB, components: 3,
                                        comp_info: [
                                            chromaton!(pal8; 0),
                                            chromaton!(pal8; 1),
                                            chromaton!(pal8; 2),
                                            None, None],
                                        elem_size: 1, has_alpha: false, is_palette: true };


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

#[derive(Clone,Copy)]
pub enum NACodecTypeInfo {
    None,
    Audio(NAAudioInfo),
    Video(NAVideoInfo),
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
