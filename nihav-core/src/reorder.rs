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

pub struct IPBReorderer {
    frames:     Vec<NAFrameRef>,
    max_depth:  usize,
    last_ft:    FrameType,
}

impl IPBReorderer {
    pub fn new(max_depth: usize) -> Self {
        Self {
            frames:     Vec::with_capacity(max_depth),
            max_depth,
            last_ft:    FrameType::Other,
        }
    }
}

impl FrameReorderer for IPBReorderer {
    fn add_frame(&mut self, fref: NAFrameRef) -> bool {
        if self.frames.len() < self.max_depth {
            let cur_ft = fref.get_frame_type();
            if cur_ft != FrameType::B {
                self.frames.push(fref);
                self.last_ft = cur_ft;
            } else {
                let pframe = self.frames.pop();
                if pframe.is_some() {
                    self.frames.push(fref);
                    self.frames.push(pframe.unwrap());
                } else {
                    self.last_ft = cur_ft;
                }
            }
            true
        } else {
            false
        }
    }
    fn get_frame(&mut self) -> Option<NAFrameRef> {
        if !self.frames.is_empty() {
            Some(self.frames.remove(0))
        } else {
            None
        }
    }
    fn flush(&mut self) {
        self.frames.clear();
        self.last_ft = FrameType::Other;
    }
    fn get_last_frames(&mut self) -> Option<NAFrameRef> {
        self.get_frame()
    }
}

