//! Output frame reordering.
//!
//! NihAV decoders output frames in the same order as they are put in.
//! In result if you want to have frames in display order you might need some frame reorderer.
//! This module provides such functionality depending on codec type: audio codecs and video codecs without B-frames do not need any reorderer and can use `NoReorderer` if the common interface is required. Codecs with B-frames should use `IPBReorderer`. For codecs with very complex reordering rules like H.264 or H.256 `PictureIDReorderer` will be added eventually.
//!
//! You can find out required reorderer by quering codec properties using `nihav_core::register` module.
use std::mem::swap;
pub use crate::frame::{FrameType, NAFrameRef};

/// A trait for frame reorderer.
pub trait FrameReorderer {
    /// Stores a newly decoded frame.
    fn add_frame(&mut self, fref: NAFrameRef) -> bool;
    /// Gets the next frame to be displayed (or `None` if that is not possible).
    fn get_frame(&mut self) -> Option<NAFrameRef>;
    /// Clears all stored frames.
    fn flush(&mut self);
    /// Retrieves the last frames stored by the reorderer.
    fn get_last_frames(&mut self) -> Option<NAFrameRef>;
}

/// Zero reorderer.
pub struct NoReorderer {
    fref:   Option<NAFrameRef>,
}

impl NoReorderer {
    /// Constructs a new instance of `NoReorderer`.
    pub fn new() -> Self {
        Self { fref: None }
    }
}

impl FrameReorderer for NoReorderer {
    fn add_frame(&mut self, fref: NAFrameRef) -> bool {
        if self.fref.is_none() {
            self.fref = Some(fref);
            true
        } else {
            false
        }
    }
    fn get_frame(&mut self) -> Option<NAFrameRef> {
        let mut ret = None;
        swap(&mut ret, &mut self.fref);
        ret
    }
    fn flush(&mut self) { self.fref = None; }
    fn get_last_frames(&mut self) -> Option<NAFrameRef> { None }
}

/// Frame reorderer for codecs with I/P/B frames.
#[derive(Default)]
pub struct IPBReorderer {
    rframe:     Option<NAFrameRef>,
    bframe:     Option<NAFrameRef>,
}

impl IPBReorderer {
    /// Constructs a new instance of `IPBReorderer`.
    pub fn new() -> Self { Self::default() }
}

impl FrameReorderer for IPBReorderer {
    fn add_frame(&mut self, fref: NAFrameRef) -> bool {
        if self.rframe.is_some() && self.bframe.is_some() { return false; }
        let is_b = fref.get_frame_type() == FrameType::B;
        if is_b && self.bframe.is_some() { return false; }
        if is_b {
            self.bframe = Some(fref);
        } else {
            std::mem::swap(&mut self.bframe, &mut self.rframe);
            self.rframe = Some(fref);
        }
        true
    }
    fn get_frame(&mut self) -> Option<NAFrameRef> {
        let mut ret = None;
        if self.bframe.is_some() {
            std::mem::swap(&mut ret, &mut self.bframe);
        }
        ret
    }
    fn flush(&mut self) {
        self.rframe = None;
        self.bframe = None;
    }
    fn get_last_frames(&mut self) -> Option<NAFrameRef> {
        let mut ret = None;
        if self.bframe.is_some() {
            std::mem::swap(&mut ret, &mut self.bframe);
        } else if self.rframe.is_some() {
            std::mem::swap(&mut ret, &mut self.rframe);
        }
        ret
    }
}

