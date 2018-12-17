use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::frame::*;
use std::rc::Rc;
use std::cell::RefCell;
use std::mem;
use crate::io::byteio::ByteIOError;
use crate::io::bitreader::BitReaderError;
use crate::io::codebook::CodebookError;

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DecoderError {
    NoFrame,
    AllocError,
    TryAgain,
    InvalidData,
    ShortData,
    MissingReference,
    NotImplemented,
    Bug,
}

pub type DecoderResult<T> = Result<T, DecoderError>;

impl From<ByteIOError> for DecoderError {
    fn from(_: ByteIOError) -> Self { DecoderError::ShortData }
}

impl From<BitReaderError> for DecoderError {
    fn from(e: BitReaderError) -> Self {
        match e {
            BitReaderError::BitstreamEnd => DecoderError::ShortData,
            _ => DecoderError::InvalidData,
        }
    }
}

impl From<CodebookError> for DecoderError {
    fn from(_: CodebookError) -> Self { DecoderError::InvalidData }
}

impl From<AllocatorError> for DecoderError {
    fn from(_: AllocatorError) -> Self { DecoderError::AllocError }
}

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[allow(dead_code)]
struct HAMShuffler {
    lastframe: Option<NAVideoBuffer<u8>>,
}

impl HAMShuffler {
    #[allow(dead_code)]
    fn new() -> Self { HAMShuffler { lastframe: None } }
    #[allow(dead_code)]
    fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    fn add_frame(&mut self, buf: NAVideoBuffer<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    fn clone_ref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref mut frm) = self.lastframe {
            let newfrm = frm.copy_buffer();
            *frm = newfrm.clone();
            Some(newfrm)
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_output_frame(&mut self) -> Option<NAVideoBuffer<u8>> {
        match self.lastframe {
            Some(ref frm) => Some(frm.clone()),
            None => None,
        }
    }
}

#[allow(dead_code)]
struct IPShuffler {
    lastframe: Option<NAVideoBuffer<u8>>,
}

impl IPShuffler {
    #[allow(dead_code)]
    fn new() -> Self { IPShuffler { lastframe: None } }
    #[allow(dead_code)]
    fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    fn add_frame(&mut self, buf: NAVideoBuffer<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    fn get_ref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
}

#[allow(dead_code)]
struct IPBShuffler {
    lastframe: Option<NAVideoBuffer<u8>>,
    nextframe: Option<NAVideoBuffer<u8>>,
}

impl IPBShuffler {
    #[allow(dead_code)]
    fn new() -> Self { IPBShuffler { lastframe: None, nextframe: None } }
    #[allow(dead_code)]
    fn clear(&mut self) { self.lastframe = None; self.nextframe = None; }
    #[allow(dead_code)]
    fn add_frame(&mut self, buf: NAVideoBuffer<u8>) {
        mem::swap(&mut self.lastframe, &mut self.nextframe);
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    fn get_lastref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_nextref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_b_fwdref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_b_bwdref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct MV {
    pub x: i16,
    pub y: i16,
}

impl MV {
    pub fn new(x: i16, y: i16) -> Self { MV{ x: x, y: y } }
    pub fn pred(a: MV, b: MV, c: MV) -> Self {
        let x;
        if a.x < b.x {
            if b.x < c.x {
                x = b.x;
            } else {
                if a.x < c.x { x = c.x; } else { x = a.x; }
            }
        } else {
            if b.x < c.x {
                if a.x < c.x { x = a.x; } else { x = c.x; }
            } else {
                x = b.x;
            }
        }
        let y;
        if a.y < b.y {
            if b.y < c.y {
                y = b.y;
            } else {
                if a.y < c.y { y = c.y; } else { y = a.y; }
            }
        } else {
            if b.y < c.y {
                if a.y < c.y { y = a.y; } else { y = c.y; }
            } else {
                y = b.y;
            }
        }
        MV { x: x, y: y }
    }
}

pub const ZERO_MV: MV = MV { x: 0, y: 0 };

impl Add for MV {
    type Output = MV;
    fn add(self, other: MV) -> MV { MV { x: self.x + other.x, y: self.y + other.y } }
}

impl AddAssign for MV {
    fn add_assign(&mut self, other: MV) { self.x += other.x; self.y += other.y; }
}

impl Sub for MV {
    type Output = MV;
    fn sub(self, other: MV) -> MV { MV { x: self.x - other.x, y: self.y - other.y } }
}

impl SubAssign for MV {
    fn sub_assign(&mut self, other: MV) { self.x -= other.x; self.y -= other.y; }
}

impl fmt::Display for MV {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{},{}", self.x, self.y)
    }
}


pub trait NADecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()>;
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
}

#[derive(Clone,Copy)]
pub struct DecoderInfo {
    name: &'static str,
    get_decoder: fn () -> Box<NADecoder>,
}

#[cfg(any(feature="h263", feature="decoder_realvideo3", feature="decoder_realvideo4"))]
mod blockdsp;

#[cfg(feature="decoder_clearvideo")]
mod clearvideo;
#[cfg(feature="decoder_gdvvid")]
mod gremlinvideo;
#[cfg(any(feature="decoder_indeo2", feature="decoder_indeo3", feature="decoder_indeo4", feature="decoder_indeo5", feature="decoder_imc"))]
mod indeo;
#[cfg(feature="h263")]
mod h263;
#[cfg(any(feature="decoder_realvideo3", feature="decoder_realvideo4", feature="decoder_realvideo6", feature="decoder_realaudio144", feature="decoder_realaudio288", feature="decoder_cook", feature="decoder_ralf"))]
mod real;

#[cfg(feature="decoder_aac")]
mod aac;
#[cfg(feature="decoder_atrac3")]
mod atrac3;
#[cfg(feature="decoder_pcm")]
mod pcm;
#[cfg(feature="decoder_sipro")]
mod sipro;
#[cfg(feature="decoder_ts102366")]
mod ts102366;

const DECODERS: &[DecoderInfo] = &[
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo", get_decoder: clearvideo::get_decoder },
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo_rm", get_decoder: clearvideo::get_decoder_rm },
#[cfg(feature="decoder_gdvvid")]
    DecoderInfo { name: "gdv-video", get_decoder: gremlinvideo::get_decoder },
#[cfg(feature="decoder_indeo2")]
    DecoderInfo { name: "indeo2", get_decoder: indeo::indeo2::get_decoder },
#[cfg(feature="decoder_indeo3")]
    DecoderInfo { name: "indeo3", get_decoder: indeo::indeo3::get_decoder },
#[cfg(feature="decoder_indeo4")]
    DecoderInfo { name: "indeo4", get_decoder: indeo::indeo4::get_decoder },
#[cfg(feature="decoder_indeo5")]
    DecoderInfo { name: "indeo5", get_decoder: indeo::indeo5::get_decoder },
#[cfg(feature="decoder_intel263")]
    DecoderInfo { name: "intel263", get_decoder: h263::intel263::get_decoder },
#[cfg(feature="decoder_realvideo1")]
    DecoderInfo { name: "realvideo1", get_decoder: h263::rv10::get_decoder },
#[cfg(feature="decoder_realvideo2")]
    DecoderInfo { name: "realvideo2", get_decoder: h263::rv20::get_decoder },
#[cfg(feature="decoder_realvideo3")]
    DecoderInfo { name: "realvideo3", get_decoder: real::rv30::get_decoder },
#[cfg(feature="decoder_realvideo4")]
    DecoderInfo { name: "realvideo4", get_decoder: real::rv40::get_decoder },
#[cfg(feature="decoder_realvideo6")]
    DecoderInfo { name: "realvideo6", get_decoder: real::rv60::get_decoder },

#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "pcm", get_decoder: pcm::get_decoder },
#[cfg(feature="decoder_imc")]
    DecoderInfo { name: "imc", get_decoder: indeo::imc::get_decoder_imc },
#[cfg(feature="decoder_imc")]
    DecoderInfo { name: "iac", get_decoder: indeo::imc::get_decoder_iac },
#[cfg(feature="decoder_realaudio144")]
    DecoderInfo { name: "ra14.4", get_decoder: real::ra144::get_decoder },
#[cfg(feature="decoder_realaudio288")]
    DecoderInfo { name: "ra28.8", get_decoder: real::ra288::get_decoder },
#[cfg(feature="decoder_sipro")]
    DecoderInfo { name: "sipro", get_decoder: sipro::get_decoder },
#[cfg(feature="decoder_ts102366")]
    DecoderInfo { name: "ac3", get_decoder: ts102366::get_decoder },
#[cfg(feature="decoder_cook")]
    DecoderInfo { name: "cook", get_decoder: real::cook::get_decoder },
#[cfg(feature="decoder_atrac3")]
    DecoderInfo { name: "atrac3", get_decoder: atrac3::get_decoder },
#[cfg(feature="decoder_aac")]
    DecoderInfo { name: "aac", get_decoder: aac::get_decoder },
#[cfg(feature="decoder_ralf")]
    DecoderInfo { name: "ralf", get_decoder: real::ralf::get_decoder },
];

pub fn find_decoder(name: &str) -> Option<fn () -> Box<NADecoder>> {
    for &dec in DECODERS {
        if dec.name == name {
            return Some(dec.get_decoder);
        }
    }
    None
}
