#[cfg(feature="decoder_indeo2")]
pub mod indeo2;

use std::rc::Rc;
use frame::*;
use io::byteio::ByteIOError;
use io::bitreader::BitReaderError;
use io::codebook::CodebookError;

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DecoderError {
    InvalidData,
    ShortData,
    MissingReference,
    NotImplemented,
    Bug,
}

type DecoderResult<T> = Result<T, DecoderError>;

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

pub trait NADecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()>;
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<Rc<NAFrame>>;
}

#[derive(Clone,Copy)]
pub struct DecoderInfo {
    name: &'static str,
    get_decoder: fn () -> Box<NADecoder>,
}

const DECODERS: &[DecoderInfo] = &[
#[cfg(feature="decoder_indeo2")]
    DecoderInfo { name: "indeo2", get_decoder: indeo2::get_decoder },
];

pub fn find_decoder(name: &str) -> Option<fn () -> Box<NADecoder>> {
    for &dec in DECODERS {
        if dec.name == name {
            return Some(dec.get_decoder);
        }
    }
    None
}
