use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
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


#[allow(dead_code)]
#[derive(Clone)]
pub struct NABuffer {
    id:   u64,
    data: Rc<Vec<u8>>,
}

impl Drop for NABuffer {
    fn drop(&mut self) { }
}

impl NABuffer {
    pub fn get_data(&self) -> Rc<Vec<u8>> { self.data.clone() }
    pub fn get_data_mut(&mut self) -> Option<&mut Vec<u8>> { Rc::get_mut(&mut self.data) }
}

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

pub fn alloc_buf(info: &NACodecInfo) -> (Rc<NABuffer>, Vec<usize>) {
    let mut data: Vec<u8> = Vec::new();
    let mut offs: Vec<usize> = Vec::new();
    match info.properties {
        NACodecTypeInfo::Audio(ainfo) => alloc_audio_buf(ainfo, &mut data, &mut offs),
        NACodecTypeInfo::Video(vinfo) => alloc_video_buf(vinfo, &mut data, &mut offs),
        _ => (),
    }
    (Rc::new(NABuffer { id: 0, data: Rc::new(data) }), offs)
}

pub fn copy_buf(buf: &NABuffer) -> Rc<NABuffer> {
    let mut data: Vec<u8> = Vec::new();
    data.clone_from(buf.get_data().as_ref());
    Rc::new(NABuffer { id: 0, data: Rc::new(data) })
}

#[derive(Debug,Clone)]
pub enum NAValue {
    None,
    Int(i32),
    Long(i64),
    String(String),
    Data(Rc<Vec<u8>>),
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct NAFrame {
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         Rc<NABuffer>,
    info:           Rc<NACodecInfo>,
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
               info:           Rc<NACodecInfo>,
               options:        HashMap<String, NAValue>) -> Self {
        let (buf, offs) = alloc_buf(&info);
        NAFrame { pts: pts, dts: dts, duration: duration, buffer: buf, offsets: offs, info: info, options: options }
    }
    pub fn from_copy(src: &NAFrame) -> Self {
        let buf = copy_buf(src.get_buffer().as_ref());
        let mut offs: Vec<usize> = Vec::new();
        offs.clone_from(&src.offsets);
        NAFrame { pts: None, dts: None, duration: None, buffer: buf, offsets: offs, info: src.info.clone(), options: src.options.clone() }
    }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_dts(&self) -> Option<u64> { self.dts }
    pub fn get_duration(&self) -> Option<u64> { self.duration }
    pub fn set_pts(&mut self, pts: Option<u64>) { self.pts = pts; }
    pub fn set_dts(&mut self, dts: Option<u64>) { self.dts = dts; }
    pub fn set_duration(&mut self, dur: Option<u64>) { self.duration = dur; }

    pub fn get_offset(&self, idx: usize) -> usize { self.offsets[idx] }
    pub fn get_buffer(&self) -> Rc<NABuffer> { self.buffer.clone() }
    pub fn get_buffer_mut(&mut self) -> Option<&mut NABuffer> { Rc::get_mut(&mut self.buffer) }
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

#[allow(dead_code)]
pub struct NACodecContext<'a> {
    info:           &'a NACodecInfo,
}
