use std::mem;
use frame::*;
use super::super::*;
use super::super::blockdsp;
use super::*;
use super::code::*;
use formats;

#[allow(dead_code)]
struct MVInfo {
    mv:         Vec<MV>,
    mb_w:       usize,
    mb_stride:  usize,
    mb_start:   usize,
    top:        bool,
    mvmode:     MVMode,
}

impl MVInfo {
    fn new() -> Self { MVInfo{ mv: Vec::new(), mb_w: 0, mb_stride: 0, mb_start: 0, top: true, mvmode: MVMode::Old } }
    fn reset(&mut self, mb_w: usize, mb_start: usize, mvmode: MVMode) {
        self.mb_start  = mb_start;
        self.mb_w      = mb_w;
        self.mb_stride = mb_w * 2;
        self.top       = true;
        self.mv.resize(self.mb_stride * 3, ZERO_MV);
        self.mvmode    = mvmode;
    }
    fn update_row(&mut self) {
        self.mb_start = self.mb_w + 1;
        self.top      = false;
        for i in 0..self.mb_stride {
            self.mv[i] = self.mv[self.mb_stride * 2 + i];
        }
    }
    #[allow(non_snake_case)]
    fn predict(&mut self, mb_x: usize, blk_no: usize, use4: bool, diff: MV) -> MV {
        let A;
        let B;
        let C;
        let last = mb_x == self.mb_w - 1;
        match blk_no {
            0 => {
                    if mb_x != self.mb_start {
                        A = if mb_x != 0 { self.mv[self.mb_stride + mb_x * 2 - 1] } else { ZERO_MV };
                        B = if !self.top { self.mv[                 mb_x * 2] } else { A };
                        C = if !self.top && !last { self.mv[mb_x * 2 + 2] } else { ZERO_MV };
                    } else {
                        A = ZERO_MV; B = ZERO_MV; C = ZERO_MV;
                    }
                },
            1 => {
                    A = self.mv[self.mb_stride + mb_x * 2];
                    B = if !self.top { self.mv[mb_x * 2 + 1] } else { A };
                    C = if !self.top && !last { self.mv[mb_x * 2 + 2] } else { A };
                },
            2 => {
                    A = if mb_x != self.mb_start { self.mv[self.mb_stride * 2 + mb_x * 2 - 1] } else { ZERO_MV };
                    B = self.mv[self.mb_stride + mb_x * 2];
                    C = self.mv[self.mb_stride + mb_x * 2 + 1];
                },
            3 => {
                    A = self.mv[self.mb_stride * 2 + mb_x * 2];
                    B = self.mv[self.mb_stride * 1 + mb_x * 2 + 1];
                    C = self.mv[self.mb_stride * 1 + mb_x * 2];
                },
            _ => { return ZERO_MV; }
        }
        let pred_mv = MV::pred(A, B, C);
        let new_mv = MV::add_umv(pred_mv, diff, self.mvmode);
        if !use4 {
            self.mv[self.mb_stride * 1 + mb_x * 2 + 0] = new_mv;
            self.mv[self.mb_stride * 1 + mb_x * 2 + 1] = new_mv;
            self.mv[self.mb_stride * 2 + mb_x * 2 + 0] = new_mv;
            self.mv[self.mb_stride * 2 + mb_x * 2 + 1] = new_mv;
        } else {
            match blk_no {
                0 => { self.mv[self.mb_stride * 1 + mb_x * 2 + 0] = new_mv; },
                1 => { self.mv[self.mb_stride * 1 + mb_x * 2 + 1] = new_mv; },
                2 => { self.mv[self.mb_stride * 2 + mb_x * 2 + 0] = new_mv; },
                3 => { self.mv[self.mb_stride * 2 + mb_x * 2 + 1] = new_mv; },
                _ => {},
            };
        }
        
        new_mv
    }
    fn set_zero_mv(&mut self, mb_x: usize) {
        self.mv[self.mb_stride * 1 + mb_x * 2 + 0] = ZERO_MV;
        self.mv[self.mb_stride * 1 + mb_x * 2 + 1] = ZERO_MV;
        self.mv[self.mb_stride * 2 + mb_x * 2 + 0] = ZERO_MV;
        self.mv[self.mb_stride * 2 + mb_x * 2 + 1] = ZERO_MV;
    }
}

fn copy_blocks(dst: &mut NAVideoBuffer<u8>, src: &NAVideoBuffer<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV) {
    let srcx = ((mv.x >> 1) as isize) + (xpos as isize);
    let srcy = ((mv.y >> 1) as isize) + (ypos as isize);
    let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;

    blockdsp::copy_blocks(dst, src, xpos, ypos, srcx, srcy, w, h, 0, 1, mode, H263_INTERP_FUNCS);
}

fn avg_blocks(dst: &mut NAVideoBuffer<u8>, src: &NAVideoBuffer<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV) {
    let srcx = ((mv.x >> 1) as isize) + (xpos as isize);
    let srcy = ((mv.y >> 1) as isize) + (ypos as isize);
    let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;

    blockdsp::copy_blocks(dst, src, xpos, ypos, srcx, srcy, w, h, 0, 1, mode, H263_INTERP_AVG_FUNCS);
}

#[allow(dead_code)]
#[derive(Clone,Copy)]
struct BMB {
    num_mv: usize,
    mv_f:   [MV; 4],
    mv_b:   [MV; 4],
    fwd:    bool,
    blk:    [[i16; 64]; 6],
    cbp:    u8,
}

impl BMB {
    fn new() -> Self { BMB {blk: [[0; 64]; 6], cbp: 0, fwd: false, mv_f: [ZERO_MV; 4], mv_b: [ZERO_MV; 4], num_mv: 0} }
}

pub struct H263BaseDecoder {
    w:          usize,
    h:          usize,
    mb_w:       usize,
    mb_h:       usize,
    num_mb:     usize,
    ftype:      Type,
    prev_frm:   Option<NAVideoBuffer<u8>>,
    cur_frm:    Option<NAVideoBuffer<u8>>,
    last_ts:    u8,
    has_b:      bool,
    b_data:     Vec<BMB>,
}

#[allow(dead_code)]
impl H263BaseDecoder {
    pub fn new() -> Self {
        H263BaseDecoder{
            w: 0, h: 0, mb_w: 0, mb_h: 0, num_mb: 0,
            ftype: Type::Special,
            prev_frm: None, cur_frm: None,
            last_ts: 0,
            has_b: false, b_data: Vec::new(),
        }
    }

    pub fn is_intra(&self) -> bool { self.ftype == Type::I }
    pub fn get_dimensions(&self) -> (usize, usize) { (self.w, self.h) }

    pub fn parse_frame(&mut self, bd: &mut BlockDecoder) -> DecoderResult<NABufferType> {
        let pinfo = bd.decode_pichdr()?;
        let mut mvi = MVInfo::new();
        let mut cbpi = CBPInfo::new();

//todo handle res change
        self.w = pinfo.w;
        self.h = pinfo.h;
        self.mb_w = (pinfo.w + 15) >> 4;
        self.mb_h = (pinfo.h + 15) >> 4;
        self.num_mb = self.mb_w * self.mb_h;
        self.ftype = pinfo.mode;
        self.has_b = pinfo.is_pb();

        if self.has_b {
            self.b_data.truncate(0);
        }

        mem::swap(&mut self.cur_frm, &mut self.prev_frm);

        let tsdiff = pinfo.ts.wrapping_sub(self.last_ts);
        let bsdiff = if pinfo.is_pb() { pinfo.get_pbinfo().get_trb() } else { 0 };

        let fmt = formats::YUV420_FORMAT;
        let vinfo = NAVideoInfo::new(self.w, self.h, false, fmt);
        let bufret = alloc_video_buffer(vinfo, 4);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let mut bufinfo = bufret.unwrap();
        let mut buf = bufinfo.get_vbuf().unwrap();

        let mut slice = Slice::get_default_slice(&pinfo);
        mvi.reset(self.mb_w, 0, pinfo.get_mvmode());
        cbpi.reset(self.mb_w);

        let mut blk: [[i16; 64]; 6] = [[0; 64]; 6];
        for mb_y in 0..self.mb_h {
            for mb_x in 0..self.mb_w {
                for i in 0..6 { for j in 0..64 { blk[i][j] = 0; } }

                if  ((mb_x != 0) || (mb_y != 0)) && bd.is_slice_end() {
                    slice = bd.decode_slice_header(&pinfo)?;
                    //mvi.reset(self.mb_w, mb_x, pinfo.get_mvmode());
                    //cbpi.reset(self.mb_w);
                }

                let binfo = bd.decode_block_header(&pinfo, &slice)?;
                let cbp = binfo.get_cbp();
                cbpi.set_cbp(mb_x, cbp);
                cbpi.set_q(mb_x, binfo.get_q());
                if binfo.is_intra() {
                    for i in 0..6 {
                        bd.decode_block_intra(&binfo, binfo.get_q(), i, (cbp & (1 << (5 - i))) != 0, &mut blk[i])?;
                        h263_idct(&mut blk[i]);
                    }
                    blockdsp::put_blocks(&mut buf, mb_x, mb_y, &blk);
                    mvi.set_zero_mv(mb_x);
                } else if !binfo.is_skipped() {
                    if binfo.get_num_mvs() == 1 {
                        let mv = mvi.predict(mb_x, 0, false, binfo.get_mv(0));
                        if let Some(ref srcbuf) = self.prev_frm {
                            copy_blocks(&mut buf, srcbuf, mb_x * 16, mb_y * 16, 16, 16, mv);
                        }
                    } else {
                        for blk_no in 0..4 {
                            let mv = mvi.predict(mb_x, blk_no, true, binfo.get_mv(blk_no));
                            if let Some(ref srcbuf) = self.prev_frm {
                                copy_blocks(&mut buf, srcbuf,
                                            mb_x * 16 + (blk_no & 1) * 8,
                                            mb_y * 16 + (blk_no & 2) * 4, 8, 8, mv);
                            }
                        }
                    }
                    for i in 0..6 {
                        bd.decode_block_inter(&binfo, binfo.get_q(), i, ((cbp >> (5 - i)) & 1) != 0, &mut blk[i])?;
                        h263_idct(&mut blk[i]);
                    }
                    blockdsp::add_blocks(&mut buf, mb_x, mb_y, &blk);
                } else {
                    mvi.set_zero_mv(mb_x);
                    if let Some(ref srcbuf) = self.prev_frm {
                        copy_blocks(&mut buf, srcbuf, mb_x * 16, mb_y * 16, 16, 16, ZERO_MV);
                    }
                }
                if pinfo.is_pb() {
                    let mut b_mb = BMB::new();
                    let cbp = binfo.get_cbp_b();
                    let bq = (((pinfo.get_pbinfo().get_dbquant() + 5) as u16) * (binfo.get_q() as u16)) >> 2;
                    let bquant;
                    if bq < 1 { bquant = 1; }
                    else if bq > 31 { bquant = 31; }
                    else { bquant = bq as u8; }

                    b_mb.cbp = cbp;
                    for i in 0..6 {
                        bd.decode_block_inter(&binfo, bquant, i, (cbp & (1 << (5 - i))) != 0, &mut b_mb.blk[i])?;
                        h263_idct(&mut b_mb.blk[i]);
                    }

                    let is_fwd = !binfo.is_b_fwd();
                    b_mb.fwd = is_fwd;
                    b_mb.num_mv = binfo.get_num_mvs();
                    if binfo.get_num_mvs() == 0 {
                        b_mb.num_mv = 1;
                        b_mb.mv_f[0] = binfo.get_mv2(1);
                        b_mb.mv_b[0] = binfo.get_mv2(0);
                    } else if binfo.get_num_mvs() == 1 {
                        let src_mv = binfo.get_mv(0).scale(bsdiff, tsdiff);
                        let mv_f = MV::add_umv(src_mv, binfo.get_mv2(0), pinfo.get_mvmode());
                        let mv_b = MV::b_sub(binfo.get_mv(0), mv_f, binfo.get_mv2(0), bsdiff, tsdiff);
                        b_mb.mv_f[0] = mv_f;
                        b_mb.mv_b[0] = mv_b;
                    } else {
                        for blk_no in 0..4 {
                            let src_mv = binfo.get_mv(blk_no).scale(bsdiff, tsdiff);
                            let mv_f = MV::add_umv(src_mv, binfo.get_mv2(0), pinfo.get_mvmode());
                            let mv_b = MV::b_sub(binfo.get_mv(blk_no), mv_f, binfo.get_mv2(0), bsdiff, tsdiff);
                            b_mb.mv_f[blk_no] = mv_f;
                            b_mb.mv_b[blk_no] = mv_b;
                        }
                    }
                    self.b_data.push(b_mb);
                }
            }
            if pinfo.deblock {
                bd.filter_row(&mut buf, mb_y, self.mb_w, &cbpi);
            }
            mvi.update_row();
            cbpi.update_row();
        }
        
        self.cur_frm = Some(buf);
        self.last_ts = pinfo.ts;
        Ok(bufinfo)
    }

    pub fn get_bframe(&mut self) -> DecoderResult<NABufferType> {
        if !self.has_b || !self.cur_frm.is_some() || !self.prev_frm.is_some() {
            return Err(DecoderError::MissingReference);
        }
        self.has_b = false;

        let fmt = formats::YUV420_FORMAT;
        let vinfo = NAVideoInfo::new(self.w, self.h, false, fmt);
        let bufret = alloc_video_buffer(vinfo, 4);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let mut bufinfo = bufret.unwrap();
        let mut b_buf = bufinfo.get_vbuf().unwrap();

        if let Some(ref bck_buf) = self.prev_frm {
            if let Some(ref fwd_buf) = self.cur_frm {
                recon_b_frame(&mut b_buf, bck_buf, fwd_buf, self.mb_w, self.mb_h, &self.b_data);
            }
        }

        self.b_data.truncate(0);
        Ok(bufinfo)
    }
}

fn recon_b_frame(b_buf: &mut NAVideoBuffer<u8>, bck_buf: &NAVideoBuffer<u8>, fwd_buf: &NAVideoBuffer<u8>,
                 mb_w: usize, mb_h: usize, b_data: &Vec<BMB>) {
    let mut cbpi = CBPInfo::new();
    let mut cur_mb = 0;
    cbpi.reset(mb_w);
    for mb_y in 0..mb_h {
        for mb_x in 0..mb_w {
            let num_mv = b_data[cur_mb].num_mv;
            let is_fwd = b_data[cur_mb].fwd;
            let cbp    = b_data[cur_mb].cbp;
            cbpi.set_cbp(mb_x, cbp);
            if num_mv == 1 {
                copy_blocks(b_buf, fwd_buf, mb_x * 16, mb_y * 16, 16, 16, b_data[cur_mb].mv_b[0]);
                if !is_fwd {
                    avg_blocks(b_buf, bck_buf, mb_x * 16, mb_y * 16, 16, 16, b_data[cur_mb].mv_f[0]);
                }
            } else {
                for blk_no in 0..4 {
                    let xpos = mb_x * 16 + (blk_no & 1) * 8;
                    let ypos = mb_y * 16 + (blk_no & 2) * 4;
                    copy_blocks(b_buf, fwd_buf, xpos, ypos, 8, 8, b_data[cur_mb].mv_b[blk_no]);
                    if !is_fwd {
                        avg_blocks(b_buf, bck_buf, xpos, ypos, 8, 8, b_data[cur_mb].mv_f[blk_no]);
                    }
                }
            }
            if cbp != 0 {
                blockdsp::add_blocks(b_buf, mb_x, mb_y, &b_data[cur_mb].blk);
            }
            cur_mb += 1;
        }
        cbpi.update_row();
    }
}
