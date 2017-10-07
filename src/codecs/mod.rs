use frame::*;
use std::rc::Rc;
use std::cell::RefCell;
use io::byteio::ByteIOError;
use io::bitreader::BitReaderError;
use io::codebook::CodebookError;

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

pub trait NADecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()>;
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
}

#[derive(Clone,Copy)]
pub struct DecoderInfo {
    name: &'static str,
    get_decoder: fn () -> Box<NADecoder>,
}

#[cfg(feature="h263")]
mod blockdsp;

#[cfg(feature="decoder_gdvvid")]
mod gremlinvideo;
#[cfg(any(feature="decoder_indeo2", feature="decoder_indeo3", feature="decoder_indeo4", feature="decoder_indeo5", feature="decoder_imc"))]
mod indeo;
#[cfg(feature="h263")]
mod h263;

#[cfg(feature="decoder_pcm")]
mod pcm;

const DECODERS: &[DecoderInfo] = &[
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

#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "pcm", get_decoder: pcm::get_decoder },
#[cfg(feature="decoder_imc")]
    DecoderInfo { name: "imc", get_decoder: indeo::imc::get_decoder_imc },
#[cfg(feature="decoder_imc")]
    DecoderInfo { name: "iac", get_decoder: indeo::imc::get_decoder_iac },
];

pub fn find_decoder(name: &str) -> Option<fn () -> Box<NADecoder>> {
    for &dec in DECODERS {
        if dec.name == name {
            return Some(dec.get_decoder);
        }
    }
    None
}
