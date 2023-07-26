//! Decoder support functions and definitions.
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign, Neg};

pub use nihav_core::frame::*;

/// Motion vector data type.
///
/// # Examples
///
/// ```
/// use nihav_codec_support::codecs::MV;
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
#[allow(clippy::collapsible_else_if)]
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

impl Neg for MV {
    type Output = MV;
    fn neg(self) -> Self::Output {
        MV { x: -self.x, y: -self.y }
    }
}

impl fmt::Display for MV {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{},{}", self.x, self.y)
    }
}

pub mod blockdsp;

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

