//! Decoder interface definitions.
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

pub use crate::frame::*;
use std::mem;
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

/// Frame manager for hold-and-modify codecs.
///
/// This frame manager simplifies frame management for the case when codec decodes new frame by updating parts of the previous frame.
///
/// # Examples
///
/// ````norun
/// let mut frame = if is_intra_frame {
///         allocate_video_frame()
///     } else {
///         let ret = shuffler.clone_ref();
///         if ret.is_none() {
///             return Err(DecodingError::MissingReference);
///         }
///         ret.unwrap()
///     };
/// // output data into the frame
/// shuffler.add_frame(frame.clone()); // tells frame manager to use the frame as the next reference
/// ````
#[allow(dead_code)]
pub struct HAMShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
}

impl HAMShuffler {
    /// Constructs a new instance of frame manager.
    #[allow(dead_code)]
    pub fn new() -> Self { HAMShuffler { lastframe: None } }
    /// Clears the reference.
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; }
    /// Sets a new frame reference.
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.lastframe = Some(buf);
    }
    /// Provides a copy of the reference frame if present or `None` if it is not.
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
    /// Returns the original saved reference frame or `None` if it is not present.
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

/// Frame manager for codecs with intra and inter frames.
///
/// This frame manager simplifies frame management for the case when codec decodes new frame using previous frame as source of some data.
///
/// # Examples
///
/// ````norun
/// let mut frame = allocate_video_frame();
/// if is_inter_frame {
///     let ret = shuffler.get_ref();
///     if ret.is_none() {
///         return Err(DecodingError::MissingReference);
///     }
///     let ref_frame = ret.unwrap();
///     // keep decoding using data from ref_frame
/// }
/// shuffler.add_frame(frame.clone()); // tells frame manager to use the frame as the next reference
/// ````
#[allow(dead_code)]
pub struct IPShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
}

impl IPShuffler {
    /// Constructs a new instance of frame manager.
    #[allow(dead_code)]
    pub fn new() -> Self { IPShuffler { lastframe: None } }
    /// Clears the reference.
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; }
    /// Sets a new frame reference.
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.lastframe = Some(buf);
    }
    /// Returns the original saved reference frame or `None` if it is not present.
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

/// Frame manager for codecs with I-, P- and B-frames.
///
/// This frame manager simplifies frame management for the case when codec uses I/P/B frame scheme.
///
/// # Examples
///
/// ````norun
/// let mut frame = allocate_video_frame();
/// for mb in all_macroblocks {
///     // decode macroblock type
///     match mb_type {
///         MBType::Inter => {
///             do_mc(&mut frame, shuffler.get_lastref().unwrap());
///         },
///         MBType::BForward => {
///             do_mc(&mut frame, shuffler.get_b_fwdref().unwrap());
///         },
///         MBType::BBackward => {
///             do_mc(&mut frame, shuffler.get_b_bwdref().unwrap());
///         },
///         // handle the rest of cases
///     };
/// if is_random_access_frame {
///     shuffler.clear(); // remove all saved references
/// }
/// if is_intra_frame || is_p_frame {
///     shuffler.add_frame(frame.clone()); // tells frame manager to use the frame as the next reference
/// }
/// ````
#[allow(dead_code)]
pub struct IPBShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
    nextframe: Option<NAVideoBufferRef<u8>>,
}

impl IPBShuffler {
    /// Constructs a new instance of frame manager.
    #[allow(dead_code)]
    pub fn new() -> Self { IPBShuffler { lastframe: None, nextframe: None } }
    /// Clears the reference.
    #[allow(dead_code)]
    pub fn clear(&mut self) { self.lastframe = None; self.nextframe = None; }
    /// Sets a new frame reference.
    #[allow(dead_code)]
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        mem::swap(&mut self.lastframe, &mut self.nextframe);
        self.lastframe = Some(buf);
    }
    /// Returns the previous reference frame or `None` if it is not present.
    #[allow(dead_code)]
    pub fn get_lastref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    /// Returns second last reference frame or `None` if it is not present.
    #[allow(dead_code)]
    pub fn get_nextref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    /// Returns the temporally following reference for B-frame or `None` if it is not present.
    #[allow(dead_code)]
    pub fn get_b_fwdref(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.nextframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    /// Returns the temporally preceeding reference for B-frame or `None` if it is not present.
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

/// Motion vector data type.
///
/// # Examples
///
/// ```
/// use nihav_core::codecs::MV;
///
/// let mv0 = MV::new(1, 3);
/// let mv1 = MV { x: 2, y: 3 }; // choose whatever style you prefer
/// let mv2 = mv1 - mv0;
/// let mv_pred = MV::pred(mv0, mv1, mv2); // get median prediction for the vectors (1, 0)
/// ```
#[derive(Debug,Clone,Copy,Default,PartialEq)]
pub struct MV {
    /// X coordinate of the vector.
    pub x: i16,
    /// Y coordinate of the vector.
    pub y: i16,
}

#[allow(clippy::many_single_char_names)]
#[allow(clippy::collapsible_if)]
impl MV {
    /// Creates a new motion vector instance.
    pub fn new(x: i16, y: i16) -> Self { MV{ x, y } }
    /// Predicts median from provided motion vectors.
    ///
    /// Each component of the vector is predicted as the median of corresponding input vector components.
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

/// Zero motion vector.
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

#[cfg(any(feature="blockdsp"))]
pub mod blockdsp;

#[cfg(feature="h263")]
pub mod h263;

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

/// The common 8x8 zigzag scan.
pub const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63
];
