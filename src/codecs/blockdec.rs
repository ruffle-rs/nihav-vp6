use std::mem;
use std::ops::{Add, Sub};
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
pub struct PBInfo {
    trb:        u8,
    dbquant:    u8,
}

impl PBInfo {
    pub fn new(trb: u8, dbquant: u8) -> Self {
        PBInfo{ trb: trb, dbquant: dbquant }
    }
    pub fn get_trb(&self) -> u8 { self.trb }
    pub fn get_dbquant(&self) -> u8 { self.dbquant }
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
    pb:     Option<PBInfo>,
    ts:     u8,
    deblock: bool,
}

#[allow(dead_code)]
impl PicInfo {
    pub fn new(w: usize, h: usize, mode: Type, quant: u8, apm: bool, umv: bool, ts: u8, pb: Option<PBInfo>, deblock: bool) -> Self {
        PicInfo{ w: w, h: h, mode: mode, quant: quant, apm: apm, umv: umv, ts: ts, pb: pb, deblock: deblock }
    }
    pub fn get_width(&self) -> usize { self.w }
    pub fn get_height(&self) -> usize { self.h }
    pub fn get_mode(&self) -> Type { self.mode }
    pub fn get_quant(&self) -> u8 { self.quant }
    pub fn get_apm(&self) -> bool { self.apm }
    pub fn get_umv(&self) -> bool { self.umv }
    pub fn is_pb(&self) -> bool { self.pb.is_some() }
    pub fn get_ts(&self) -> u8 { self.ts }
    pub fn get_pbinfo(&self) -> PBInfo { self.pb.unwrap() }
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
    fn scale(&self, trb: u8, trd: u8) -> Self {
        if (trd == 0) || (trb == 0) {
            ZERO_MV
        } else {
            MV { x: (self.x * (trb as i16)) / (trd as i16), y: (self.y * (trb as i16)) / (trd as i16) }
        }
    }
    fn b_sub(pvec: MV, fwdvec: MV, bvec: MV, trb: u8, trd: u8) -> Self {
        let bscale = (trb as i16) - (trd as i16);
        let x = if bvec.x != 0 { fwdvec.x - pvec.x } else if trd != 0 { bscale * pvec.x / (trd as i16) } else { 0 };
        let y = if bvec.y != 0 { fwdvec.y - pvec.y } else if trd != 0 { bscale * pvec.y / (trd as i16) } else { 0 };
        MV { x: x, y: y }
    }
}

pub const ZERO_MV: MV = MV { x: 0, y: 0 };

impl Add for MV {
    type Output = MV;
    fn add(self, other: MV) -> MV { MV { x: self.x + other.x, y: self.y + other.y } }
}

impl Sub for MV {
    type Output = MV;
    fn sub(self, other: MV) -> MV { MV { x: self.x - other.x, y: self.y - other.y } }
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
    mv2:     [MV; 2],
    num_mv2: usize,
    fwd:     bool,
}

#[allow(dead_code)]
#[derive(Debug,Clone,Copy)]
pub struct BBlockInfo {
    present: bool,
    cbp:     u8,
    num_mv:  usize,
    fwd:     bool,
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
            mv2:     [ZERO_MV, ZERO_MV],
            num_mv2: 0,
            fwd:     false,
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
        let mut mv_arr: [MV; 4] = [MV::new(0, 0), MV::new(0, 0), MV::new(0, 0), MV::new(0, 0)];
        for i in 0..mvs.len() { mv_arr[i] = mvs[i]; }
        self.mv     = mv_arr;
        self.num_mv = mvs.len();
    }
    pub fn set_bpart(&mut self, bbinfo: BBlockInfo) {
        self.bpart = bbinfo.present;
        self.b_cbp = bbinfo.cbp;
        self.fwd   = bbinfo.fwd;
        self.num_mv2 = bbinfo.get_num_mv();
    }
    pub fn set_b_mv(&mut self, mvs: &[MV]) {
        if mvs.len() > 0 { self.skip = false; }
        let mut mv_arr: [MV; 2] = [ZERO_MV, ZERO_MV];
        for i in 0..mvs.len() { mv_arr[i] = mvs[i]; }
        self.mv2    = mv_arr;
    }
    pub fn is_b_fwd(&self) -> bool { self.fwd }
}

impl BBlockInfo {
    pub fn new(present: bool, cbp: u8, num_mv: usize, fwd: bool) -> Self {
        BBlockInfo {
            present: present,
            cbp:     cbp,
            num_mv:  num_mv,
            fwd:     fwd,
        }
    }
    pub fn get_num_mv(&self) -> usize { self.num_mv }
}

pub trait BlockDecoder {
    fn decode_pichdr(&mut self) -> DecoderResult<PicInfo>;
    fn decode_slice_header(&mut self, pinfo: &PicInfo) -> DecoderResult<Slice>;
    fn decode_block_header(&mut self, pinfo: &PicInfo, sinfo: &Slice) -> DecoderResult<BlockInfo>;
    fn decode_block_intra(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()>;
    fn decode_block_inter(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()>;
    fn is_slice_end(&mut self) -> bool;

    fn filter_row(&mut self, buf: &mut NAVideoBuffer<u8>, mb_y: usize, mb_w: usize, cbpi: &CBPInfo);
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

#[allow(dead_code)]
pub struct CBPInfo {
    cbp:        Vec<u8>,
    q:          Vec<u8>,
    mb_w:       usize,
}

impl CBPInfo {
    fn new() -> Self { CBPInfo{ cbp: Vec::new(), q: Vec::new(), mb_w: 0 } }
    fn reset(&mut self, mb_w: usize) {
        self.mb_w = mb_w;
        self.cbp.truncate(0);
        self.cbp.resize(self.mb_w * 2, 0);
        self.q.truncate(0);
        self.q.resize(self.mb_w * 2, 0);
    }
    fn update_row(&mut self) {
        for i in 0..self.mb_w {
            self.cbp[i] = self.cbp[self.mb_w + i];
            self.q[i]   = self.q[self.mb_w + i];
        }
    }
    fn set_cbp(&mut self, mb_x: usize, cbp: u8) {
        self.cbp[self.mb_w + mb_x] = cbp;
    }
    fn set_q(&mut self, mb_x: usize, q: u8) {
        self.q[self.mb_w + mb_x] = q;
    }
    pub fn get_q(&self, mb_x: usize) -> u8 { self.q[mb_x] }
    pub fn is_coded(&self, mb_x: usize, blk_no: usize) -> bool {
        (self.cbp[self.mb_w + mb_x] & (1 << (5 - blk_no))) != 0
    }
    pub fn is_coded_top(&self, mb_x: usize, blk_no: usize) -> bool {
        let cbp     = self.cbp[self.mb_w + mb_x];
        let cbp_top = self.cbp[mb_x];
        match blk_no {
            0 => { (cbp_top & 0b001000) != 0 },
            1 => { (cbp_top & 0b000100) != 0 },
            2 => { (cbp     & 0b100000) != 0 },
            3 => { (cbp     & 0b010000) != 0 },
            4 => { (cbp_top & 0b000010) != 0 },
            _ => { (cbp_top & 0b000001) != 0 },
        }
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

pub struct DCT8x8VideoDecoder {
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
impl DCT8x8VideoDecoder {
    pub fn new() -> Self {
        DCT8x8VideoDecoder{
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
        mvi.reset(self.mb_w, 0, pinfo.get_umv());
        cbpi.reset(self.mb_w);

        let mut blk: [[i16; 64]; 6] = [[0; 64]; 6];
        for mb_y in 0..self.mb_h {
            for mb_x in 0..self.mb_w {
                for i in 0..6 { for j in 0..64 { blk[i][j] = 0; } }

                if  ((mb_x != 0) || (mb_y != 0)) && bd.is_slice_end() {
//println!("new slice @{}.{}!",mb_x,mb_y);
                    slice = bd.decode_slice_header(&pinfo)?;
                    mvi.reset(self.mb_w, mb_x, pinfo.get_umv());
                    //cbpi.reset(self.mb_w);
                }

                let binfo = bd.decode_block_header(&pinfo, &slice)?;
                let cbp = binfo.get_cbp();
//println!("mb {}.{} CBP {:X} type {:?}, {} mvs skip {}", mb_x,mb_y, cbp, binfo.get_mode(), binfo.get_num_mvs(),binfo.is_skipped());
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

                    let is_fwd = binfo.is_b_fwd();
                    b_mb.fwd = is_fwd;
                    b_mb.num_mv = binfo.get_num_mvs();
                    if binfo.get_num_mvs() == 0 {
                        b_mb.num_mv = 1;
                        b_mb.mv_f[0] = binfo.get_mv2(1);
                        b_mb.mv_b[0] = binfo.get_mv2(0);
                    } if binfo.get_num_mvs() == 1 {
                        let src_mv = if is_fwd { ZERO_MV } else { binfo.get_mv(0).scale(bsdiff, tsdiff) };
                        let mv_f = MV::add_umv(src_mv, binfo.get_mv2(0), pinfo.get_umv());
                        let mv_b = MV::b_sub(binfo.get_mv(0), mv_f, binfo.get_mv2(0), bsdiff, tsdiff);
                        b_mb.mv_f[0] = mv_f;
                        b_mb.mv_b[0] = mv_b;
                    } else {
                        for blk_no in 0..4 {
                            let src_mv = if is_fwd { ZERO_MV } else { binfo.get_mv(blk_no).scale(bsdiff, tsdiff) };
                            let mv_f = MV::add_umv(src_mv, binfo.get_mv2(0), pinfo.get_umv());
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
//println!("unpacked all");
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
                copy_blocks(b_buf, bck_buf, mb_x * 16, mb_y * 16, 16, 16, b_data[cur_mb].mv_f[0]);
                if !is_fwd {
                    avg_blocks(b_buf, fwd_buf, mb_x * 16, mb_y * 16, 16, 16, b_data[cur_mb].mv_b[0]);
                }
            } else {
                for blk_no in 0..4 {
                    let xpos = mb_x * 16 + (blk_no & 1) * 8;
                    let ypos = mb_y * 16 + (blk_no & 2) * 4;
                    copy_blocks(b_buf, bck_buf, xpos, ypos, 8, 8, b_data[cur_mb].mv_f[blk_no]);
                    if !is_fwd {
                        avg_blocks(b_buf, fwd_buf, xpos, ypos, 8, 8, b_data[cur_mb].mv_b[blk_no]);
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
