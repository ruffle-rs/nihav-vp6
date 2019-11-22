use std::mem::swap;
pub use crate::frame::{FrameType, NAFrameRef};

pub trait FrameReorderer {
    fn add_frame(&mut self, fref: NAFrameRef) -> bool;
    fn get_frame(&mut self) -> Option<NAFrameRef>;
    fn flush(&mut self);
    fn get_last_frames(&mut self) -> Option<NAFrameRef>;
}

pub struct NoReorderer {
    fref:   Option<NAFrameRef>,
}

impl NoReorderer {
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

#[derive(Default)]
pub struct IPBReorderer {
    rframe:     Option<NAFrameRef>,
    bframe:     Option<NAFrameRef>,
}

impl IPBReorderer {
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

