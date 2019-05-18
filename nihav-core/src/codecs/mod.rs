use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

pub use crate::frame::*;
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

#[allow(dead_code)]
pub struct HAMShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
}

impl HAMShuffler {
    #[allow(dead_code)]
    pub fn new() -> Self { HAMShuffler { lastframe: None } }
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    pub fn clone_ref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref mut frm) = self.lastframe {
            let newfrm = frm.copy_buffer();
            *frm = newfrm.clone().into_ref();
            Some(newfrm.into_ref())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn get_output_frame(&mut self) -> Option<NAVideoBufferRef<u8>> {
        match self.lastframe {
            Some(ref frm) => Some(frm.clone()),
            None => None,
        }
    }
}

impl Default for HAMShuffler {
    fn default() -> Self { Self { lastframe: None } }
}

#[allow(dead_code)]
pub struct IPShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
}

impl IPShuffler {
    #[allow(dead_code)]
    pub fn new() -> Self { IPShuffler { lastframe: None } }
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    pub fn get_ref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
}

impl Default for IPShuffler {
    fn default() -> Self { Self { lastframe: None } }
}

#[allow(dead_code)]
pub struct IPBShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
    nextframe: Option<NAVideoBufferRef<u8>>,
}

impl IPBShuffler {
    #[allow(dead_code)]
    pub fn new() -> Self { IPBShuffler { lastframe: None, nextframe: None } }
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; self.nextframe = None; }
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        mem::swap(&mut self.lastframe, &mut self.nextframe);
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    pub fn get_lastref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn get_nextref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn get_b_fwdref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    #[allow(dead_code)]
    pub fn get_b_bwdref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
}

impl Default for IPBShuffler {
    fn default() -> Self { Self { lastframe: None, nextframe: None } }
}

#[derive(Debug,Clone,Copy,Default,PartialEq)]
pub struct MV {
    pub x: i16,
    pub y: i16,
}

#[allow(clippy::many_single_char_names)]
#[allow(clippy::collapsible_if)]
impl MV {
    pub fn new(x: i16, y: i16) -> Self { MV{ x, y } }
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
        MV { x, y }
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

pub struct NADecoderSupport {
    pub pool_u8:        NAVideoBufferPool<u8>,
    pub pool_u16:       NAVideoBufferPool<u16>,
    pub pool_u32:       NAVideoBufferPool<u32>,
}

impl NADecoderSupport {
    pub fn new() -> Self {
        Self {
            pool_u8:        NAVideoBufferPool::new(0),
            pool_u16:       NAVideoBufferPool::new(0),
            pool_u32:       NAVideoBufferPool::new(0),
        }
    }
}

impl Default for NADecoderSupport {
    fn default() -> Self { Self::new() }
}


pub trait NADecoder {
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()>;
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
}

#[derive(Clone,Copy)]
pub struct DecoderInfo {
    pub name: &'static str,
    pub get_decoder: fn () -> Box<dyn NADecoder>,
}

#[cfg(any(feature="blockdsp"))]
pub mod blockdsp;

#[cfg(feature="h263")]
pub mod h263;

#[derive(Default)]
pub struct RegisteredDecoders {
    decs:   Vec<DecoderInfo>,
}

impl RegisteredDecoders {
    pub fn new() -> Self {
        Self { decs: Vec::new() }
    }
    pub fn add_decoder(&mut self, dec: DecoderInfo) {
        self.decs.push(dec);
    }
    pub fn find_decoder(&self, name: &str) -> Option<fn () -> Box<dyn NADecoder>> {
        for &dec in self.decs.iter() {
            if dec.name == name {
                return Some(dec.get_decoder);
            }
        }
        None
    }
    pub fn iter(&self) -> std::slice::Iter<DecoderInfo> {
        self.decs.iter()
    }
}
