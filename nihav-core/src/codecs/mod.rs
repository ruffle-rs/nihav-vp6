//! Decoder interface definitions.
pub use crate::frame::*;
use crate::io::byteio::ByteIOError;
use crate::io::bitreader::BitReaderError;
use crate::io::codebook::CodebookError;

/// A list specifying general decoding errors.
#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DecoderError {
    /// No frame was provided.
    NoFrame,
    /// Allocation failed.
    AllocError,
    /// Operation requires repeating.
    TryAgain,
    /// Invalid input data was provided.
    InvalidData,
    /// Provided input turned out to be incomplete.
    ShortData,
    /// Decoder could not decode provided frame because it references some missing previous frame.
    MissingReference,
    /// Feature is not implemented.
    NotImplemented,
    /// Some bug in decoder. It should not happen yet it might.
    Bug,
}

/// A specialised `Result` type for decoding operations.
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

/// Auxiliary structure for storing data used by decoder but also controlled by the caller.
pub struct NADecoderSupport {
    /// Frame buffer pool for 8-bit or packed video frames.
    pub pool_u8:        NAVideoBufferPool<u8>,
    /// Frame buffer pool for 16-bit video frames.
    pub pool_u16:       NAVideoBufferPool<u16>,
    /// Frame buffer pool for 32-bit video frames.
    pub pool_u32:       NAVideoBufferPool<u32>,
}

impl NADecoderSupport {
    /// Constructs a new instance of `NADecoderSupport`.
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

/// Decoder trait.
pub trait NADecoder {
    /// Initialises the decoder.
    ///
    /// It takes [`NADecoderSupport`] allocated by the caller and `NACodecInfoRef` provided by demuxer.
    ///
    /// [`NADecoderSupport`]: ./struct.NADecoderSupport.html
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()>;
    /// Decodes a single frame.
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
    /// Tells decoder to clear internal state (e.g. after error or seeking).
    fn flush(&mut self);
}

/// Decoder information using during creating a decoder for requested codec.
#[derive(Clone,Copy)]
pub struct DecoderInfo {
    /// Short decoder name.
    pub name: &'static str,
    /// The function that creates a decoder instance.
    pub get_decoder: fn () -> Box<dyn NADecoder + Send>,
}

/// Structure for registering known decoders.
///
/// It is supposed to be filled using `register_all_codecs()` from some decoders crate and then it can be used to create decoders for the requested codecs.
#[derive(Default)]
pub struct RegisteredDecoders {
    decs:   Vec<DecoderInfo>,
}

impl RegisteredDecoders {
    /// Constructs a new instance of `RegisteredDecoders`.
    pub fn new() -> Self {
        Self { decs: Vec::new() }
    }
    /// Adds another decoder to the registry.
    pub fn add_decoder(&mut self, dec: DecoderInfo) {
        self.decs.push(dec);
    }
    /// Searches for the decoder for the provided name and returns a function for creating it on success.
    pub fn find_decoder(&self, name: &str) -> Option<fn () -> Box<dyn NADecoder + Send>> {
        for &dec in self.decs.iter() {
            if dec.name == name {
                return Some(dec.get_decoder);
            }
        }
        None
    }
    /// Provides an iterator over currently registered decoders.
    pub fn iter(&self) -> std::slice::Iter<DecoderInfo> {
        self.decs.iter()
    }
}
