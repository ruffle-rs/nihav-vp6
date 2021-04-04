use nihav_core::codecs::DecoderResult;
use nihav_core::frame::{FrameType, NAVideoBufferRef};
use nihav_codec_support::codecs::MV;
use super::sets::SeqParameterSet;
use super::slice::*;
use super::types::*;

#[derive(Clone)]
pub struct PictureInfo {
    pub id:         u16,
    pub full_id:    u32,
    pub pic_type:   FrameType,
    pub buf:        NAVideoBufferRef<u8>,
    pub cur_mb:     usize,
    pub is_ref:     bool,
    pub long_term:  Option<usize>,

    pub mv_info:    FrameMV, //todo replace with refcounted index to a pool
}

#[derive(Clone,Copy,Default, Debug)]
pub struct FrameMBInfo {
    pub mb_type:    CompactMBType,
    pub ref_poc:    [[u16; 2]; 4],
    pub ref_idx:    [[PicRef; 2]; 4],
    pub mv:         [[MV; 2]; 16],
}

impl FrameMBInfo {
    pub fn new() -> Self { Self::default() }
}

#[derive(Clone)]
pub struct FrameMV {
    pub mbs:        Vec<FrameMBInfo>,
    pub mb_stride:  usize,
}

impl FrameMV {
    pub fn new(mb_w: usize, mb_h: usize) -> Self {
        Self {
            mbs:        vec![FrameMBInfo::default(); mb_w * mb_h],
            mb_stride:  mb_w,
        }
    }
}

pub struct FrameRefs {
    pub ref_pics:   Vec<PictureInfo>,
    pub ref_list0:  Vec<Option<PictureInfo>>,
    pub ref_list1:  Vec<Option<PictureInfo>>,
    pub long_term:  Vec<Option<PictureInfo>>,

    prev_poc_msb:       u32,
    prev_poc_lsb:       u16,
    prev_ref_poc_lsb:   u16,
    prev_frame_num:     u16,
    frame_num_offset:   u32,
}

impl FrameRefs {
    pub fn new() -> Self {
        Self {
            ref_pics:   Vec::with_capacity(16),
            ref_list0:  Vec::with_capacity(3),
            ref_list1:  Vec::with_capacity(3),
            long_term:  Vec::new(),

            prev_poc_msb:       0,
            prev_poc_lsb:       0,
            prev_ref_poc_lsb:   0,
            prev_frame_num:     0,
            frame_num_offset:   0,
        }
    }
    pub fn calc_picture_num(&mut self, slice_hdr: &SliceHeader, is_idr: bool, ref_id: u8, sps: &SeqParameterSet) -> u32 {
        match sps.pic_order_cnt_type {
            0 => {
                if is_idr {
                    //self.prev_poc_msb = 0;
                    self.prev_poc_lsb = 0;
                } else {
                    self.prev_poc_lsb = self.prev_ref_poc_lsb;
                }
                let max_poc_lsb = 1 << sps.log2_max_pic_order_cnt_lsb;
                let half_max_poc_lsb = 1 << (sps.log2_max_pic_order_cnt_lsb - 1);
                let cur_lsb = slice_hdr.pic_order_cnt_lsb;
                let poc_msb = if cur_lsb < self.prev_poc_lsb && (self.prev_poc_lsb - cur_lsb >= half_max_poc_lsb) {
                        self.prev_poc_msb + max_poc_lsb
                    } else if cur_lsb > self.prev_poc_lsb && (cur_lsb - self.prev_poc_lsb > half_max_poc_lsb) {
                        self.prev_poc_msb.wrapping_sub(max_poc_lsb)
                    } else {
                        self.prev_poc_msb
                    };
                let poc = poc_msb + u32::from(cur_lsb);
                if ref_id != 0 {
                    self.prev_ref_poc_lsb = slice_hdr.pic_order_cnt_lsb;
                    self.prev_poc_msb = poc_msb;
                }
                poc
            },
            1 => {
                let off = if self.prev_frame_num > slice_hdr.frame_num {
                        self.frame_num_offset + (1 << sps.log2_max_frame_num)
                    } else {
                        self.frame_num_offset
                    };
                let mut anum = if sps.num_ref_frames_in_pic_order_cnt_cycle != 0 {
                        (off as i32) + i32::from(slice_hdr.frame_num)
                    } else {
                        0
                    };
                if ref_id == 0 && anum > 0 {
                    anum -= 1;
                }
                let (poc_cycle_cnt, fno_in_poc_cycle) = if anum > 0 {
                        let nrf = sps.num_ref_frames_in_pic_order_cnt_cycle as i32;
                        ((anum - 1) / nrf, (anum - 1) % nrf)
                    } else {
                        (0, 0)
                    };
                let mut expected_delta = 0;
                for &offset in sps.offset_for_ref_frame[..sps.num_ref_frames_in_pic_order_cnt_cycle].iter() {
                    expected_delta += offset;
                }
                let mut expected_poc = if anum > 0 {
                        let mut sum = poc_cycle_cnt * expected_delta;
                        for &offset in sps.offset_for_ref_frame[..=fno_in_poc_cycle as usize].iter() {
                            sum += offset;
                        }
                        sum
                    } else {
                        0
                    };
                if ref_id == 0 {
                    expected_poc += sps.offset_for_non_ref_pic;
                }
                let (top_id, _bottom_id) = if !slice_hdr.field_pic {
                        let top_id = expected_poc + slice_hdr.delta_pic_order_cnt[0];
                        let bot_id = top_id + sps.offset_for_top_to_bottom_field + slice_hdr.delta_pic_order_cnt[1];
                        (top_id, bot_id)
                    } else if !slice_hdr.bottom_field {
                        (expected_poc + slice_hdr.delta_pic_order_cnt[0], 0)
                    } else {
                        (0, sps.offset_for_top_to_bottom_field + slice_hdr.delta_pic_order_cnt[1])
                    };
                self.prev_frame_num = slice_hdr.frame_num;
                self.frame_num_offset = off;
                top_id as u32
            },
            _ => {
                if slice_hdr.frame_num < self.prev_frame_num {
                    self.frame_num_offset   += 1 << sps.log2_max_frame_num;
                }
                self.prev_frame_num = slice_hdr.frame_num;
                self.frame_num_offset + u32::from(slice_hdr.frame_num)
            },
        }
    }
    pub fn apply_adaptive_marking(&mut self, marking: &AdaptiveMarking, cur_id: u16, max_id: u16) -> DecoderResult<()> {
        let all_ref_pics = self.ref_pics.clone();

        for (&op, (&arg1, &arg2)) in marking.memory_management_control_op.iter().zip(marking.operation_arg.iter().zip(marking.operation_arg2.iter())).take(marking.num_ops) {
            match op {
                1 => {
                    let src_id = cur_id.wrapping_sub(arg1) & (max_id - 1);
                    let mut found = false;
                    let mut idx = 0;
                    for (i, pic) in self.ref_pics.iter().enumerate() {
                        if pic.id == src_id {
                            found = true;
                            idx = i;
                            break;
                        }
                    }
                    if found {
                        self.ref_pics.remove(idx);
                    }
                },
                2 => { // mark long term picture as unused
                    let idx = arg1 as usize;
                    if idx < self.long_term.len() {
                        self.long_term[idx] = None;
                    }
                },
                3 => {
                    let src_id = cur_id.wrapping_sub(arg1) & (max_id - 1);

                    let didx = arg2 as usize;
                    for pic in all_ref_pics.iter() {
                        if pic.id == src_id {
                            if didx < self.long_term.len() {
                                self.long_term[didx] = Some(pic.clone());
                            }
                            break;
                        }
                    }
                },
                4 => {
                    self.long_term.resize(arg1 as usize, None);
                },
                5 => {
                    self.ref_pics.truncate(0);
                    self.long_term.truncate(0);
                },
                6 => {
                    // assign an long term index to current pic - done elsewhere
                },
                _ => {},
            };
        }
        Ok(())
    }
    pub fn clear_refs(&mut self) {
        self.ref_pics.truncate(0);
        self.long_term.truncate(0);
    }
    #[allow(clippy::cognitive_complexity)]
    pub fn select_refs(&mut self, sps: &SeqParameterSet, slice_hdr: &SliceHeader, cur_id: u32) {
        self.ref_list0.truncate(0);
        self.ref_list1.truncate(0);
        let pic_num_mask = if sps.log2_max_frame_num == 16 {
                0xFFFF
            } else {
                (1 << sps.log2_max_frame_num) - 1
            };
        if !slice_hdr.slice_type.is_intra() {
            let has_reordering = slice_hdr.ref_pic_list_reordering_l0;
            if !has_reordering {
                let num_ref = slice_hdr.num_ref_idx_l0_active;
                if slice_hdr.slice_type.is_p() {
                    if !self.ref_pics.is_empty() {
                        for pic in self.ref_pics.iter().rev().take(num_ref) {
                            self.ref_list0.push(Some(pic.clone()));
                        }
                    }
                } else {
                    let mut pivot = 0;
                    for (i, pic) in self.ref_pics.iter().enumerate() {
                        pivot = i;
                        if pic.full_id > cur_id {
                            break;
                        }
                    }
                    for pic in self.ref_pics[..pivot].iter().rev() {
                        if self.ref_list0.len() >= num_ref {
                            break;
                        }
                        self.ref_list0.push(Some(pic.clone()));
                    }
                    for pic in self.ref_pics.iter().skip(pivot) {
                        if self.ref_list0.len() >= num_ref {
                            break;
                        }
                        self.ref_list0.push(Some(pic.clone()));
                    }
                }
                if !self.long_term.is_empty() && self.ref_list0.len() < num_ref {
                    let copy_size = num_ref - self.ref_list0.len();
                    for ltpic in self.long_term.iter().take(copy_size) {
                        self.ref_list0.push(ltpic.clone());
                    }
                }
            } else {
                form_ref_list(&mut self.ref_list0,
                              &self.ref_pics, &self.long_term,
                              &slice_hdr.reordering_list_l0,
                              slice_hdr.frame_num, pic_num_mask);
            }
            if slice_hdr.slice_type.is_b() {
                let has_reordering = slice_hdr.ref_pic_list_reordering_l1;
                if !has_reordering {
                    let num_ref = slice_hdr.num_ref_idx_l1_active;
                    let mut pivot = 0;
                    for (i, pic) in self.ref_pics.iter().enumerate() {
                        pivot = i;
                        if pic.full_id > cur_id {
                            break;
                        }
                    }
                    for pic in self.ref_pics.iter().skip(pivot) {
                        if self.ref_list1.len() >= num_ref {
                            break;
                        }
                        self.ref_list1.push(Some(pic.clone()));
                    }
                    for pic in self.ref_pics[..pivot].iter().rev() {
                        if self.ref_list1.len() >= num_ref {
                            break;
                        }
                        self.ref_list1.push(Some(pic.clone()));
                    }
                    if !self.long_term.is_empty() && self.ref_list1.len() < num_ref {
                        let copy_size = num_ref - self.ref_list1.len();
                        for ltpic in self.long_term.iter().take(copy_size) {
                            self.ref_list1.push(ltpic.clone());
                        }
                    }
                    if self.ref_list1.len() > 1 && self.ref_list0.len() == self.ref_list1.len() {
                        let mut equal = true;
                        for (pic1, pic2) in self.ref_list0.iter().zip(self.ref_list1.iter()) {
                            match (pic1, pic2) {
                                (Some(p1), Some(p2)) => {
                                    if p1.full_id != p2.full_id {
                                        equal = false;
                                        break;
                                    }
                                },
                                (None, None) => {},
                                _ => {
                                    equal = false;
                                    break;
                                },
                            };
                        }
                        if equal {
                            self.ref_list1.swap(0, 1);
                        }
                    }
                } else {
                    form_ref_list(&mut self.ref_list1,
                                  &self.ref_pics, &self.long_term,
                                  &slice_hdr.reordering_list_l1,
                                  slice_hdr.frame_num, pic_num_mask);
                }
            }
        }
    }
    pub fn add_short_term(&mut self, cpic: PictureInfo, num_ref_frames: usize) {
        if !self.ref_pics.is_empty() && self.ref_pics.len() >= num_ref_frames {
            self.ref_pics.remove(0);
        }
        if self.ref_pics.is_empty() || self.ref_pics.last().unwrap().full_id < cpic.full_id {
            self.ref_pics.push(cpic);
        } else {
            let mut idx = 0;
            for (i, pic) in self.ref_pics.iter().enumerate() {
                if pic.full_id < cpic.full_id {
                    idx = i;
                } else {
                    break;
                }
            }
            self.ref_pics.insert(idx + 1, cpic);
        }
    }
    pub fn add_long_term(&mut self, lt_idx: usize, cpic: PictureInfo) {
        if lt_idx < self.long_term.len() {
            self.long_term[lt_idx] = Some(cpic);
        }
    }
    pub fn select_ref_pic(&self, list_id: u8, ref_id: usize) -> Option<NAVideoBufferRef<u8>> {
        let ref_list = if list_id == 0 { &self.ref_list0 } else { &self.ref_list1 };
        if ref_list.len() > ref_id {
            if let Some(ref pic) = ref_list[ref_id] {
                Some(pic.buf.clone())
            } else {
                None
            }
        } else {
            None
        }
    }
    pub fn get_colocated_info(&self, mb_x: usize, mb_y: usize) -> (FrameMBInfo, u16, bool) {
        if let Some(ref ref_pic) = &self.ref_list1[0] {
            let mv_info = &ref_pic.mv_info;
            let mb = mv_info.mbs[mb_x + mb_y * mv_info.mb_stride];
            (mb, ref_pic.full_id as u16, ref_pic.long_term.is_some())
        } else {
            (FrameMBInfo::default(), 0, false)
        }
    }
    pub fn map_ref0(&self, ref0_id: u16) -> (PicRef, bool) {
        let mut r0_idx = 0;
        let mut long = false;
        for (i, rpic0) in self.ref_list0.iter().enumerate() {
            if let Some(ref pic) = rpic0 {
                if (pic.full_id as u16) == ref0_id {
                    r0_idx = i as u8;
                    long = pic.long_term.is_some();
                    break;
                }
            }
        }
        (PicRef::new(r0_idx), long)
    }
    pub fn map_refs(&self, ref_idx: [PicRef; 2]) -> [u16; 2] {
        let r0 = ref_idx[0].index();
        let r1 = ref_idx[1].index();
        let ref0 = if r0 < self.ref_list0.len() {
                if let Some(ref pic) = self.ref_list0[r0] {
                    pic.full_id as u16
                } else {
                    MISSING_POC
                }
            } else {
                MISSING_POC
            };
        let ref1 = if r1 < self.ref_list1.len() {
                if let Some(ref pic) = self.ref_list1[r1] {
                    pic.full_id as u16
                } else {
                    MISSING_POC
                }
            } else {
                MISSING_POC
            };
        [ref0, ref1]
    }
}

fn form_ref_list(ref_list: &mut Vec<Option<PictureInfo>>, ref_pics: &[PictureInfo], long_term: &[Option<PictureInfo>], reord_info: &ReorderingInfo, cur_id: u16, pic_num_mask: u16) {
    let mut ref_pic_id = cur_id;
    for (&op, &num) in reord_info.reordering_of_pic_nums_idc.iter().zip(reord_info.abs_diff_or_num.iter()).take(reord_info.num_ops) {
        if op < 2 {
            if op == 0 {
                ref_pic_id = ref_pic_id.wrapping_sub(num) & pic_num_mask;
            } else {
                ref_pic_id = ref_pic_id.wrapping_add(num) & pic_num_mask;
            }
            let mut found = false;
            for pic in ref_pics.iter() {
                if pic.id == ref_pic_id {
                    ref_list.push(Some(pic.clone()));
                    found = true;
                    break;
                }
            }
            if !found {
                ref_list.push(None);
            }
        } else {
            let idx = num as usize;
            if idx < long_term.len() {
                ref_list.push(long_term[idx].clone());
            } else {
                ref_list.push(None);
            }
        }
    }
}
