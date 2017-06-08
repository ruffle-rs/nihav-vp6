use std::mem;
use std::ops::Add;
use super::*;
use super::blockdsp;
use super::h263code::*;
use formats;

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum Type {
    I, P, Skip, Special
}

#[allow(dead_code)]
#[derive(Debug,Clone,Copy)]
pub struct PicInfo {
    w:      usize,
    h:      usize,
    mode:   Type,
    quant:  u8,
    apm:    bool,
    umv:    bool,
    pb:     bool,
    ts:     u8,
}

#[allow(dead_code)]
impl PicInfo {
    pub fn new(w: usize, h: usize, mode: Type, quant: u8, apm: bool, umv: bool, pb: bool, ts: u8) -> Self {
        PicInfo{ w: w, h: h, mode: mode, quant: quant, apm: apm, umv: umv, pb: pb, ts: ts }
    }
    pub fn get_width(&self) -> usize { self.w }
    pub fn get_height(&self) -> usize { self.h }
    pub fn get_mode(&self) -> Type { self.mode }
    pub fn get_quant(&self) -> u8 { self.quant }
    pub fn get_apm(&self) -> bool { self.apm }
    pub fn get_umv(&self) -> bool { self.umv }
    pub fn is_pb(&self) -> bool { self.pb }
    pub fn get_ts(&self) -> u8 { self.ts }
}

#[derive(Debug,Clone,Copy)]
pub struct Slice {
    mb_x:   usize,
    mb_y:   usize,
    quant:  u8,
}

impl Slice {
    pub fn new(mb_x: usize, mb_y: usize, quant: u8) -> Self {
        Slice{ mb_x: mb_x, mb_y: mb_y, quant: quant }
    }
    pub fn get_default_slice(pinfo: &PicInfo) -> Self {
        Slice{ mb_x: 0, mb_y: 0, quant: pinfo.get_quant() }
    }
    pub fn get_quant(&self) -> u8 { self.quant }
}

#[derive(Debug,Clone,Copy)]
pub struct MV {
    x: i16,
    y: i16,
}

impl MV {
    pub fn new(x: i16, y: i16) -> Self { MV{ x: x, y: y } }
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
        MV { x: x, y: y }
    }
    fn add_umv(pred_mv: MV, add: MV, umv: bool) -> Self {
        let mut new_mv = pred_mv + add;
        if umv {
            if pred_mv.x >  32 && new_mv.x >  63 { new_mv.x -= 64; }
            if pred_mv.x < -31 && new_mv.x < -63 { new_mv.x += 64; }
            if pred_mv.y >  32 && new_mv.y >  63 { new_mv.y -= 64; }
            if pred_mv.y < -31 && new_mv.y < -63 { new_mv.y += 64; }
        } else {
            if      new_mv.x >  31 { new_mv.x -= 64; }
            else if new_mv.x < -32 { new_mv.x += 64; }
            if      new_mv.y >  31 { new_mv.y -= 64; }
            else if new_mv.y < -32 { new_mv.y += 64; }
        }
        new_mv
    }
}

pub const ZERO_MV: MV = MV { x: 0, y: 0 };

impl Add for MV {
    type Output = MV;
    fn add(self, other: MV) -> MV { MV { x: self.x + other.x, y: self.y + other.y } }
}

#[derive(Debug,Clone,Copy)]
pub struct BlockInfo {
    intra:   bool,
    skip:    bool,
    mode:    Type,
    cbp:     u8,
    q:       u8,
    mv:      [MV; 4],
    num_mv:  usize,
    bpart:   bool,
    b_cbp:   u8,
    mv2:     [MV; 4],
    num_mv2: usize,
}

#[allow(dead_code)]
impl BlockInfo {
    pub fn new(mode: Type, cbp: u8, q: u8) -> Self {
        BlockInfo {
            intra:   mode == Type::I,
            skip:    (cbp == 0) && (mode != Type::I),
            mode:    mode,
            cbp:     cbp,
            q:       q,
            mv:      [MV::new(0, 0), MV::new(0, 0), MV::new(0, 0), MV::new(0, 0)],
            num_mv:  0,
            bpart:   false,
            b_cbp:   0,
            mv2:     [MV::new(0, 0), MV::new(0, 0), MV::new(0, 0), MV::new(0, 0)],
            num_mv2: 0,
        }
    }
    pub fn is_intra(&self) -> bool { self.intra }
    pub fn is_skipped(&self) -> bool { self.skip }
    pub fn get_mode(&self) -> Type { self.mode }
    pub fn get_cbp(&self) -> u8 { self.cbp }
    pub fn get_q(&self) -> u8 { self.q }
    pub fn get_num_mvs(&self) -> usize { self.num_mv }
    pub fn get_mv(&self, idx: usize) -> MV { self.mv[idx] }
    pub fn has_b_part(&self) -> bool { self.bpart }
    pub fn get_cbp_b(&self) -> u8 { self.b_cbp }
    pub fn get_num_mvs2(&self) -> usize { self.num_mv2 }
    pub fn get_mv2(&self, idx: usize) -> MV { self.mv2[idx] }
    pub fn set_mv(&mut self, mvs: &[MV]) {
        if mvs.len() > 0 { self.skip = false; }
        self.bpart = true;
        let mut mv_arr: [MV; 4] = [MV::new(0, 0), MV::new(0, 0), MV::new(0, 0), MV::new(0, 0)];
        for i in 0..mvs.len() { mv_arr[i] = mvs[i]; }
        self.mv     = mv_arr;
        self.num_mv = mvs.len();
    }
    pub fn set_mv2(&mut self, cbp: u8, mvs: &[MV]) {
        self.bpart = true;
        self.b_cbp = cbp;
        let mut mv_arr: [MV; 4] = [MV::new(0, 0), MV::new(0, 0), MV::new(0, 0), MV::new(0, 0)];
        for i in 0..mvs.len() { mv_arr[i] = mvs[i]; }
        self.mv2     = mv_arr;
        self.num_mv2 = mvs.len();
    }
}

pub trait BlockDecoder {
    fn decode_pichdr(&mut self) -> DecoderResult<PicInfo>;
    fn decode_slice_header(&mut self, pinfo: &PicInfo) -> DecoderResult<Slice>;
    fn decode_block_header(&mut self, pinfo: &PicInfo, sinfo: &Slice) -> DecoderResult<BlockInfo>;
    fn decode_block_intra(&mut self, info: &BlockInfo, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()>;
    fn decode_block_inter(&mut self, info: &BlockInfo, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()>;
    fn calc_mv(&mut self, vec: MV);
    fn is_slice_end(&mut self) -> bool;
}

#[allow(dead_code)]
struct MVInfo {
    mv:         Vec<MV>,
    mb_w:       usize,
    mb_stride:  usize,
    mb_start:   usize,
    top:        bool,
    umv:        bool,
}

impl MVInfo {
    fn new() -> Self { MVInfo{ mv: Vec::new(), mb_w: 0, mb_stride: 0, mb_start: 0, top: true, umv: false } }
    fn reset(&mut self, mb_w: usize, mb_start: usize, umv: bool) {
        self.mb_start  = mb_start;
        self.mb_w      = mb_w;
        self.mb_stride = mb_w * 2;
        self.top       = true;
        self.mv.resize(self.mb_stride * 3, ZERO_MV);
        self.umv       = umv;
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
//println!("  pred from {}.{} blk {}.{}/{} top {}", diff.x, diff.y, mb_x, blk_no,self.mb_start,self.top);
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
//println!("     A = {}.{}  B = {}.{}  C = {}.{}", A.x,A.y,B.x,B.y,C.x,C.y);
        let pred_mv = MV::pred(A, B, C);
        let new_mv = MV::add_umv(pred_mv, diff, self.umv);
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

pub struct DCT8x8VideoDecoder {
    w:          usize,
    h:          usize,
    mb_w:       usize,
    mb_h:       usize,
    num_mb:     usize,
    ftype:      Type,
    prev_frm:   Option<NAVideoBuffer<u8>>,
    cur_frm:    Option<NAVideoBuffer<u8>>,
}

#[allow(dead_code)]
impl DCT8x8VideoDecoder {
    pub fn new() -> Self {
        DCT8x8VideoDecoder{
            w: 0, h: 0, mb_w: 0, mb_h: 0, num_mb: 0,
            ftype: Type::Special,
            prev_frm: None, cur_frm: None,
        }
    }

    pub fn is_intra(&self) -> bool { self.ftype == Type::I }
    pub fn get_dimensions(&self) -> (usize, usize) { (self.w, self.h) }

    pub fn parse_frame(&mut self, bd: &mut BlockDecoder) -> DecoderResult<NABufferType> {
        let pinfo = bd.decode_pichdr()?;
        let mut mvi  = MVInfo::new();

//todo handle res change
        self.w = pinfo.w;
        self.h = pinfo.h;
        self.mb_w = (pinfo.w + 15) >> 4;
        self.mb_h = (pinfo.h + 15) >> 4;
        self.num_mb = self.mb_w * self.mb_h;
        self.ftype = pinfo.mode;

        mem::swap(&mut self.cur_frm, &mut self.prev_frm);
//        if self.ftype == Type::I && !pinfo.is_pb() { self.prev_frm = None; }

        let fmt = formats::YUV420_FORMAT;
        let vinfo = NAVideoInfo::new(self.w, self.h, false, fmt);
        let bufret = alloc_video_buffer(vinfo, 4);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let mut bufinfo = bufret.unwrap();
        let mut buf = bufinfo.get_vbuf().unwrap();

        let mut bbuf;

        if self.prev_frm.is_some() && pinfo.is_pb() {
            let bufret = alloc_video_buffer(vinfo, 4);
            if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
            let mut bbufinfo = bufret.unwrap();
            bbuf = Some(bbufinfo.get_vbuf().unwrap());
        } else {
            bbuf = None;
        }

        let mut slice = Slice::get_default_slice(&pinfo);
        mvi.reset(self.mb_w, 0, pinfo.get_umv());

        let mut blk: [[i16; 64]; 6] = [[0; 64]; 6];
        for mb_y in 0..self.mb_h {
            for mb_x in 0..self.mb_w {
                for i in 0..6 { for j in 0..64 { blk[i][j] = 0; } }

                if  ((mb_x != 0) || (mb_y != 0)) && bd.is_slice_end() {
//println!("new slice @{}.{}!",mb_x,mb_y);
                    slice = bd.decode_slice_header(&pinfo)?;
                    mvi.reset(self.mb_w, mb_x, pinfo.get_umv());
                }

                let binfo = bd.decode_block_header(&pinfo, &slice)?;
                let cbp = binfo.get_cbp();
//println!("mb {}.{} CBP {:X} type {:?}, {} mvs skip {}", mb_x,mb_y, cbp, binfo.get_mode(), binfo.get_num_mvs(),binfo.is_skipped());
                if binfo.is_intra() {
                    for i in 0..6 {
                        bd.decode_block_intra(&binfo, i, (cbp & (1 << (5 - i))) != 0, &mut blk[i])?;
                        h263_idct(&mut blk[i]);
                    }
                    blockdsp::put_blocks(&mut buf, mb_x, mb_y, &blk);
                    mvi.set_zero_mv(mb_x);
                } else if !binfo.is_skipped() {
                    if binfo.get_num_mvs() == 1 {
                        let mv = mvi.predict(mb_x, 0, false, binfo.get_mv(0));
//println!(" 1MV {}.{}", mv.x, mv.y);
                        if let Some(ref srcbuf) = self.prev_frm {
                            copy_blocks(&mut buf, srcbuf, mb_x * 16, mb_y * 16, 16, 16, mv);
                        }
                    } else {
                        for blk_no in 0..4 {
                            let mv = mvi.predict(mb_x, blk_no, true, binfo.get_mv(blk_no));
//print!(" MV {}.{}", mv.x, mv.y);
                            if let Some(ref srcbuf) = self.prev_frm {
                                copy_blocks(&mut buf, srcbuf,
                                            mb_x * 16 + (blk_no & 1) * 8,
                                            mb_y * 16 + (blk_no & 2) * 4, 8, 8, mv);
                            }
                        }
//println!("");
                    }
                    for i in 0..6 {
                        bd.decode_block_inter(&binfo, i, ((cbp >> (5 - i)) & 1) != 0, &mut blk[i])?;
                        h263_idct(&mut blk[i]);
                    }
                    blockdsp::add_blocks(&mut buf, mb_x, mb_y, &blk);
                } else {
                    mvi.set_zero_mv(mb_x);
                    if let Some(ref srcbuf) = self.prev_frm {
                        copy_blocks(&mut buf, srcbuf, mb_x * 16, mb_y * 16, 16, 16, ZERO_MV);
                    }
                }
                if pinfo.is_pb() && binfo.has_b_part() {
                    let mut blk: [[i16; 64]; 6] = [[0; 64]; 6];
                    let cbp = binfo.get_cbp_b();
                    for i in 0..6 {
                        bd.decode_block_inter(&binfo, i, (cbp & (1 << (5 - i))) != 0, &mut blk[i])?;
                        h263_idct(&mut blk[i]);
                    }
                    if let Some(ref mut b_buf) = bbuf {
/*                        let is_fwd = false;
                        if binfo.get_num_mvs() == 1 { //todo scale
                            let mv_f = MV::add_umv(binfo.get_mv(0), binfo.get_mv2(0), pinfo.get_umv());
                            let mv_b = ZERO_MV//if component = 0 then scaled else mv_f - component
                        } else {
                        }*/
                        if let Some(ref srcbuf) = self.prev_frm {
                            copy_blocks(b_buf, srcbuf, mb_x * 16, mb_y * 16, 16, 16, ZERO_MV);
                            blockdsp::add_blocks(b_buf, mb_x, mb_y, &blk);
                        }
                    }
                }

            }
            mvi.update_row();
        }
        self.cur_frm = Some(buf);
        if pinfo.is_pb() {
            return Ok(NABufferType::Video(bbuf.unwrap()));
        } 
println!("unpacked all");
        Ok(bufinfo)
    }

    pub fn get_stored_pframe(&mut self) -> DecoderResult<NABufferType> {
        if let Some(_) = self.cur_frm {
            let buf = self.cur_frm.clone().unwrap();
            Ok(NABufferType::Video(buf))
        } else {
            Err(DecoderError::MissingReference)
        }
    }
}
