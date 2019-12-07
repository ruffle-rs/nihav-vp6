use nihav_core::formats;
use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use std::io::SeekFrom;
use std::mem;

struct IviDeltaCB {
    quad_radix: u8,
    data: &'static [i8],
}

#[derive(Clone, Copy)]
struct MV {
    x: i8,
    y: i8
}

struct Buffers {
    width:      usize,
    height:     usize,
    cw:         usize,
    ch:         usize,
    sbuf:       Vec<u8>,
    dbuf:       Vec<u8>,
}

const DEFAULT_PIXEL: u8 = 0x40;

impl Buffers {
    fn new() -> Self { Buffers { width: 0, height: 0, cw: 0, ch: 0, sbuf: Vec::new(), dbuf: Vec::new() } }
    fn reset(&mut self) {
        self.width  = 0;
        self.height = 0;
        self.sbuf.truncate(0);
        self.dbuf.truncate(0);
    }
    fn alloc(&mut self, w: usize, h: usize) {
        self.width  = w;
        self.height = h;
        self.cw = ((w >> 2) + 3) & !3;
        self.ch = ((h >> 2) + 3) & !3;
        self.sbuf.resize(w * h + self.cw * self.ch * 2, DEFAULT_PIXEL);
        self.dbuf.resize(w * h + self.cw * self.ch * 2, DEFAULT_PIXEL);
    }
    fn flip(&mut self) { std::mem::swap(&mut self.sbuf, &mut self.dbuf); }
    fn get_stride(&mut self, planeno: usize) -> usize {
        if planeno == 0 { self.width } else { self.cw }
    }
    fn get_offset(&mut self, planeno: usize) -> usize {
        match planeno {
            1 => self.width * self.height,
            2 => self.width * self.height + self.cw * self.ch,
            _ => 0,
        }
    }
    fn fill_framebuf(&mut self, fbuf: &mut NAVideoBuffer<u8>) {
        for planeno in 0..3 {
            let mut soff = self.get_offset(planeno);
            let mut doff = fbuf.get_offset(planeno);
            let sstride = self.get_stride(planeno);
            let dstride = fbuf.get_stride(planeno);
            let width  = if planeno == 0 { self.width }  else { self.width >> 2 };
            let height = if planeno == 0 { self.height } else { self.height >> 2 };
            let src = self.dbuf.as_slice();
            let dst = fbuf.get_data_mut().unwrap();
            for _ in 0..height {
                for x in 0..width {
                    dst[doff + x] = src[soff + x] * 2;
                }
                soff += sstride;
                doff += dstride;
            }
        }
    }
    fn copy_block(&mut self, doff: usize, soff: usize, stride: usize, w: usize, h: usize) {
        let mut sidx = soff;
        let mut didx = doff;
        for _ in 0..h {
            for i in 0..w { self.dbuf[didx + i] = self.sbuf[sidx + i]; }
            sidx += stride;
            didx += stride;
        }
    }
    fn fill_block(&mut self, doff: usize, stride: usize, w: usize, h: usize, topline: bool) {
        let mut didx = doff;
        let mut buf: [u8; 8] = [0; 8];
        if topline {
            for _ in 0..h {
                for i in 0..w { self.dbuf[didx + i] = DEFAULT_PIXEL; }
                didx += stride;
            }
        } else {
            for i in 0..w { buf[i] = self.dbuf[didx - stride + i]; }
            for _ in 0..h {
                for i in 0..w { self.dbuf[didx + i] = buf[i]; }
                didx += stride;
            }
        }
    }
}

#[allow(unused_variables)]
fn apply_delta4x4(bufs: &mut Buffers, off: usize, stride: usize,
                  deltas: &[u8], topline: bool, first_line: bool) {
    let dst = &mut bufs.dbuf[off..][..4];
    for i in 0..4 { dst[i] = dst[i].wrapping_add(deltas[i]) & 0x7F; }
}

#[allow(unused_variables)]
fn apply_delta4x8(bufs: &mut Buffers, off: usize, stride: usize,
                  deltas: &[u8], topline: bool, first_line: bool) {
    let dst = &mut bufs.dbuf[off..][..stride + 4];
    for i in 0..4 { dst[i + stride] = dst[i].wrapping_add(deltas[i]) & 0x7F; }
    if !topline {
        for i in 0..4 { dst[i] = (dst[i + stride] + dst[i]) >> 1; }
    } else {
        for i in 0..4 { dst[i] =  dst[i + stride]; }
    }
}

#[allow(unused_variables)]
fn apply_delta4x8m11(bufs: &mut Buffers, off: usize, stride: usize,
                     deltas: &[u8], topline: bool, first_line: bool) {
    let dst = &mut bufs.dbuf[off..][..stride + 4];
    for i in 0..4 { dst[i]          = dst[i]         .wrapping_add(deltas[i]) & 0x7F; }
    for i in 0..4 { dst[i + stride] = dst[i + stride].wrapping_add(deltas[i]) & 0x7F; }
}

#[allow(unused_variables)]
fn apply_delta8x8p(bufs: &mut Buffers, off: usize, stride: usize,
                   deltas: &[u8], topline: bool, first_line: bool) {
    let dst = &mut bufs.dbuf[off..][..stride + 8];
    for i in 0..8 { dst[i]          = dst[i]         .wrapping_add(deltas[i >> 1]) & 0x7F; }
    for i in 0..8 { dst[i + stride] = dst[i + stride].wrapping_add(deltas[i >> 1]) & 0x7F; }
}

fn apply_delta8x8i(bufs: &mut Buffers, off: usize, stride: usize,
                   deltas: &[u8], topline: bool, firstline: bool) {
    let dst = &mut bufs.dbuf[off..][..stride + 8];
    if !firstline {
        for i in 0..8 { dst[i + stride] = dst[i     ].wrapping_add(deltas[i >> 1]) & 0x7F; }
    } else {
        for i in 0..8 { dst[i + stride] = dst[i & !1].wrapping_add(deltas[i >> 1]) & 0x7F; }
    }
    if !topline {
        for i in 0..8 { dst[i] = (dst[i + stride] + dst[i]) >> 1; }
    } else {
        for i in 0..8 { dst[i] =  dst[i + stride]; }
    }
}

fn copy_line_top(bufs: &mut Buffers, off: usize, stride: usize, bw: usize, topline: bool) {
    let mut buf: [u8; 8] = [0; 8];
    if !topline {
        let src = &bufs.dbuf[(off - stride)..(off - stride + bw)];
        for i in 0..bw { buf[i] = src[i]; }
    } else {
        for i in 0..bw { buf[i] = DEFAULT_PIXEL; }
    }
    let dst = &mut bufs.dbuf[off..][..bw];
    for i in 0..bw { dst[i] = buf[i]; }
}

fn copy_line_top4x4(bufs: &mut Buffers, off: usize, stride: usize, topline: bool) {
    copy_line_top(bufs, off, stride, 4, topline);
}

fn copy_line_top4x8(bufs: &mut Buffers, off: usize, stride: usize, topline: bool) {
    copy_line_top(bufs, off,          stride, 4, topline);
    copy_line_top(bufs, off + stride, stride, 4, false);
}

fn copy_line_top8x8(bufs: &mut Buffers, off: usize, stride: usize, topline: bool) {
    let mut buf: [u8; 8] = [0; 8];
    if !topline {
        let src = &bufs.dbuf[(off - stride)..(off - stride + 8)];
        for i in 0..8 { buf[i] = src[i & !1]; }
    } else {
        for i in 0..8 { buf[i] = DEFAULT_PIXEL; }
    }
    let dst = &mut bufs.dbuf[off..][..8];
    for i in 0..8 {dst[i] = buf[i]; }
}

fn fill_block8x8(bufs: &mut Buffers, doff: usize, stride: usize, h: usize, topline: bool, firstline: bool) {
    let mut didx = doff;
    let mut buf: [u8; 8] = [0; 8];
    if firstline {
        for i in 0..8 { buf[i] = DEFAULT_PIXEL; }
    } else {
        for i in 0..8 { buf[i] = bufs.dbuf[doff - stride + i]; }
    }
    if topline && !firstline {
        for i in 0..4 { buf[i * 2 + 1] = buf[i * 2]; }
        for i in 0..8 { bufs.dbuf[doff + i] = (bufs.dbuf[doff - stride + i] + buf[i]) >> 1; }
    }

    let start = if !topline { 0 } else { 1 };
    if topline {
        didx += stride;
    }
    for _ in start..h {
        for i in 0..8 { bufs.dbuf[didx + i] = buf[i]; }
        didx += stride;
    }
}

struct Indeo3Decoder {
    info:       NACodecInfoRef,
    bpos:       u8,
    bbuf:       u8,
    width:      u16,
    height:     u16,
    mvs:        Vec<MV>,
    altquant:   [u8; 16],
    vq_offset:  u8,
    bufs:       Buffers,
    requant_tab: [[u8; 128]; 8],
}

#[derive(Clone,Copy)]
struct IV3Cell {
    x:      u16,
    y:      u16,
    w:      u16,
    h:      u16,
    d:      u8,
    vqt:    bool,
    mv:     Option<MV>,
}

impl IV3Cell {
    fn new(w: u16, h: u16) -> Self {
        IV3Cell { x: 0, y: 0, w, h, d: 20, vqt: false, mv: None }
    }
    fn split_h(&self) -> (Self, Self) {
        let h1 = if self.h > 2 { ((self.h + 2) >> 2) << 1 } else { 1 };
        let h2 = self.h - h1;
        let mut cell1 = *self;
        cell1.h  = h1;
        cell1.d -= 1;
        let mut cell2 = *self;
        cell2.y += h1;
        cell2.h  = h2;
        cell2.d -= 1;
        (cell1, cell2)
    }
    fn split_w(&self, stripw: u16) -> (Self, Self) {
        let w1 = if self.w > stripw {
                if self.w > stripw * 2 { stripw * 2 } else { stripw }
            } else {
                if self.w > 2 { ((self.w + 2) >> 2) << 1 } else { 1 }
            };
        let w2 = self.w - w1;
        let mut cell1 = *self;
        cell1.w  = w1;
        cell1.d -= 1;
        let mut cell2 = *self;
        cell2.x += w1;
        cell2.w  = w2;
        cell2.d -= 1;
        (cell1, cell2)
    }
    fn no_mv(&self) -> bool { self.mv.is_none() }
}

struct CellDecParams {
    tab:    [usize; 2],
    bw:     u16,
    bh:     u16,
    swap_q: [bool; 2],
    hq:     bool,
    apply_delta:   fn (&mut Buffers, usize, usize, &[u8], bool, bool),
    copy_line_top: fn (&mut Buffers, usize, usize, bool),
}

const FRMH_TAG: u32 = ((b'F' as u32) << 24) | ((b'R' as u32) << 16)
                     | ((b'M' as u32) << 8) | (b'H' as u32);

const H_SPLIT: u8 = 0;
const V_SPLIT: u8 = 1;
const SKIP_OR_TREE: u8 = 2;

impl Indeo3Decoder {
    fn new() -> Self {
        const REQUANT_OFF: [i32; 8] = [ 0, 1, 0, 4, 4, 1, 0, 1 ];

        let dummy_info = NACodecInfo::new_dummy();

        let mut requant_tab = [[0u8; 128]; 8];
        for i in 0..8 {
            let step = (i as i32) + 2;
            let start = if (i == 3) || (i == 4) { -3 } else { step / 2 };
            let mut last = 0;
            for j in 0..128 {
                requant_tab[i][j] = (((j as i32) + start) / step * step + REQUANT_OFF[i]) as u8;
                if requant_tab[i][j] < 128 {
                    last = requant_tab[i][j];
                } else {
                    requant_tab[i][j] = last;
                }
            }
        }
        requant_tab[1][7]   =  10;
        requant_tab[1][119] = 118;
        requant_tab[1][120] = 118;
        requant_tab[4][8]   =  10;

        Indeo3Decoder { info: dummy_info, bpos: 0, bbuf: 0, width: 0, height: 0,
                        mvs: Vec::new(), altquant: [0; 16],
                        vq_offset: 0, bufs: Buffers::new(), requant_tab }
    }

    fn br_reset(&mut self) {
        self.bpos = 0;
        self.bbuf = 0;
    }

    fn get_2bits(&mut self, br: &mut ByteReader) -> DecoderResult<u8> {
        if self.bpos == 0 {
            self.bbuf = br.read_byte()?;
            self.bpos = 8;
        }
        self.bpos -= 2;
        Ok((self.bbuf >> self.bpos) & 0x3)
    }

    fn decode_cell_data(&mut self, br: &mut ByteReader, cell: IV3Cell,
                        off: usize, stride: usize, params: CellDecParams, vq_idx: u8) -> DecoderResult<()> {
        let blk_w = cell.w * 4 / params.bw;
        let blk_h = cell.h * 4 / params.bh;
        let scale: usize = if params.bh == 4 { 1 } else { 2 };

        validate!((((cell.w * 4) % params.bw) == 0) && (((cell.h * 4) % params.bh) == 0));

        let mut run_blocks = 0;
        let mut run_skip   = false;

        let mut didx: usize = ((cell.x*4) as usize) + ((cell.y * 4) as usize) * stride + off;
        let mut sidx: usize;

        if cell.no_mv() {
            sidx = 0;
        } else {
            let mv = cell.mv.unwrap();
            let mx = i16::from(mv.x);
            let my = i16::from(mv.y);
            let l = (cell.x as i16) * 4 + mx;
            let t = (cell.y as i16) * 4 + my;
            let r = ((cell.x + cell.w) as i16) * 4 + mx;
            let b = ((cell.y + cell.h) as i16) * 4 + my;
            validate!(l >= 0);
            validate!(t >= 0);
            validate!(r <= (self.width as i16));
            validate!(b <= (self.height as i16));
            sidx = (l as usize) + (t as usize) * stride + off;
        }
        if vq_idx >= 8 {
            let requant_tab = &self.requant_tab[(vq_idx & 7) as usize];
            if cell.no_mv() {
                if cell.y > 0 {
                    for x in 0..(cell.w as usize) * 4 {
                        self.bufs.dbuf[didx + x - stride] = requant_tab[self.bufs.dbuf[didx + x - stride] as usize];
                    }
                }
            } else {
                for x in 0..(cell.w as usize) * 4 {
                    self.bufs.sbuf[sidx + x] = requant_tab[self.bufs.sbuf[sidx + x] as usize];
                }
            }
        }
        for y in 0..blk_h {
            let mut xoff: usize = 0;
            for _ in 0..blk_w {
                if run_blocks > 0 {
                    if !run_skip || !cell.no_mv() {
                        if !(params.bw == 8 && cell.no_mv()) {
                            if !cell.no_mv() {
                                self.bufs.copy_block(didx + xoff, sidx + xoff, stride,
                                                     params.bw as usize, params.bh as usize);
                            } else {
                                self.bufs.fill_block(didx + xoff, stride,
                                                     params.bw as usize, params.bh as usize,
                                                     (cell.y == 0) && (y == 0));
                            }
                        } else {
                            fill_block8x8(&mut self.bufs,
                                          didx + xoff, stride, 8,
                                          y == 0, (cell.y == 0) && (y == 0));
                        }
                    }
                    run_blocks -= 1;
                } else {
                    let mut line: usize = 0;
                    while line < 4 {
                        let c = br.read_byte()?;
                        if c < 0xF8 {
                            let delta_tab = if params.hq {
                                                IVI3_DELTA_CBS[params.tab[line & 1]]
                                            } else {
                                                IVI3_DELTA_CBS[params.tab[1]]
                                            };
                            let mut idx1;
                            let mut idx2;
                            if (c as usize) < delta_tab.data.len()/2 {
                                idx1 = br.read_byte()? as usize;
                                validate!(idx1 < delta_tab.data.len() / 2);
                                idx2 = c as usize;
                            } else {
                                let tmp = (c as usize) - delta_tab.data.len()/2;
                                idx1 = tmp / (delta_tab.quad_radix as usize);
                                idx2 = tmp % (delta_tab.quad_radix as usize);
                                if params.swap_q[line & 1] {
                                    mem::swap(&mut idx1, &mut idx2);
                                }
                            }
                            let deltas: [u8; 4] = [delta_tab.data[idx1 * 2]     as u8,
                                                   delta_tab.data[idx1 * 2 + 1] as u8,
                                                   delta_tab.data[idx2 * 2 + 0]     as u8,
                                                   delta_tab.data[idx2 * 2 + 1] as u8];
                            let topline = (cell.y == 0) && (y == 0) && (line == 0);
                            let first_line = (y == 0) && (line == 0);
                            if cell.no_mv() {
                                (params.copy_line_top)(&mut self.bufs,
                                                       didx + xoff + line * scale * stride,
                                                       stride, topline);
                            } else {
                                self.bufs.copy_block(didx + xoff + line * scale * stride,
                                                     sidx + xoff + line * scale * stride,
                                                     stride, params.bw as usize, scale);
                            }
                            (params.apply_delta)(&mut self.bufs,
                                                 didx + xoff + line * scale * stride,
                                                 stride, &deltas, topline, first_line);
                            line += 1;
                        } else {
                            let mut tocopy: usize = 0;
                            let mut do_copy = true;
                            if c == 0xF8 { return Err(DecoderError::InvalidData); }
                            if c == 0xF9 {
                                run_blocks = 1;
                                run_skip   = true;
                                validate!(line == 0);
                                tocopy = 4;
                                do_copy = !cell.no_mv();
                            }
                            if c == 0xFA {
                                validate!(line == 0);
                                tocopy = 4;
                                do_copy = !cell.no_mv();
                            }
                            if c == 0xFB {
                                let c = br.read_byte()?;
                                validate!((c < 64) && ((c & 0x1F) != 0));
                                run_blocks = (c & 0x1F) - 1;
                                run_skip   = (c & 0x20) != 0;
                                tocopy = 4 - line;
                                if params.bw == 4 && cell.no_mv() && run_skip {
                                    do_copy = false;
                                }
                            }
                            if c == 0xFC {
                                run_skip = false;
                                run_blocks = 1;
                                tocopy = 4 - line;
                            }
                            if c >= 0xFD {
                                let nl = 257 - i16::from(c) - (line as i16);
                                validate!(nl > 0);
                                tocopy = nl as usize;
                            }
                            if do_copy {
                                if !(params.bh == 8 && cell.no_mv()) {
                                    if !cell.no_mv() {
                                        self.bufs.copy_block(didx + xoff + line * scale * stride,
                                                             sidx + xoff + line * scale * stride,
                                                             stride, params.bw as usize,
                                                             tocopy * scale);
                                    } else {
                                        self.bufs.fill_block(didx + xoff + line * scale * stride,
                                                             stride, params.bw as usize,
                                                             tocopy * scale,
                                                             (cell.y == 0) && (y == 0) && (line == 0));
                                    }
                                } else {
                                    fill_block8x8(&mut self.bufs,
                                                  didx + xoff + line * 2 * stride,
                                                  stride, tocopy * 2,
                                                  (y == 0) && (line == 0),
                                                  (cell.y == 0) && (y == 0) && (line == 0));
                                }
                            }
                            line += tocopy;
                        }
                    }
                }
                xoff += params.bw as usize;
            }
            didx += stride * (params.bh as usize);
            sidx += stride * (params.bh as usize);
        }
        Ok(())
    }

    fn copy_cell(&mut self, cell: IV3Cell, off: usize, stride: usize) -> DecoderResult<()> {
        if cell.no_mv() { return Err(DecoderError::InvalidData); }
        let mv = cell.mv.unwrap();
        let mx = i16::from(mv.x);
        let my = i16::from(mv.y);
        let l = (cell.x as i16) * 4 + mx;
        let t = (cell.y as i16) * 4 + my;
        let r = ((cell.x + cell.w) as i16) * 4 + mx;
        let b = ((cell.y + cell.h) as i16) * 4 + my;
        validate!(l >= 0);
        validate!(t >= 0);
        validate!(r <= (self.width as i16));
        validate!(b <= (self.height as i16));
        let sidx: usize = off + (l as usize) + (t as usize) * stride;
        let didx: usize = off + ((cell.x * 4) as usize) + ((cell.y * 4) as usize) * stride;
        self.bufs.copy_block(didx, sidx, stride, (cell.w * 4) as usize, (cell.h * 4) as usize);
        Ok(())
    }

    fn decode_cell(&mut self, br: &mut ByteReader, cell: IV3Cell, off: usize,
                   stride: usize, intra: bool) -> DecoderResult<()> {
        let code = br.read_byte()?;
        let mode   = code >> 4;
        let vq_idx = code & 0xF;

        let mut idx1: usize = vq_idx as usize;
        let mut idx2: usize = vq_idx as usize;
        if (mode == 1) || (mode == 4) {
            let c = self.altquant[vq_idx as usize];
            idx1 = (c >> 4) as usize;
            idx2 = (c & 0xF) as usize;
        } else {
            idx1 += self.vq_offset as usize;
            idx2 += self.vq_offset as usize;
        }
        validate!((idx1 < 24) && (idx2 < 24));

        let mut cp = CellDecParams {
                         tab: [idx2, idx1],
                         bw: 0, bh: 0,
                         swap_q: [idx2 >= 16, idx1 >= 16],
                         hq: false,
                         apply_delta:   apply_delta4x4,
                         copy_line_top: copy_line_top4x4,
                     };
        if (mode == 0) || (mode == 1) {
            cp.bw = 4;
            cp.bh = 4;
            cp.hq = true;
        } else if (mode == 3) || (mode == 4) {
            if !cell.no_mv() { return Err(DecoderError::InvalidData); }
            cp.bw = 4;
            cp.bh = 8;
            cp.hq = true;
            cp.apply_delta   = apply_delta4x8;
            cp.copy_line_top = copy_line_top4x8;
        } else if mode == 10 {
            if !cell.no_mv() {
                validate!(!intra);
                cp.apply_delta = apply_delta8x8p;
            } else {
                cp.apply_delta = apply_delta8x8i;
            }
            cp.bw = 8;
            cp.bh = 8;
            cp.copy_line_top = copy_line_top8x8;
        } else if mode == 11 {
            if cell.no_mv() { return Err(DecoderError::InvalidData); }
            validate!(!intra);
            cp.bw = 4;
            cp.bh = 8;
            cp.apply_delta = apply_delta4x8m11;
            cp.copy_line_top = copy_line_top4x8;
        } else {
            return Err(DecoderError::InvalidData);
        }
        self.decode_cell_data(br, cell, off, stride, cp, vq_idx)
    }

    fn parse_tree(&mut self, br: &mut ByteReader, cell: IV3Cell, off: usize,
                  stride: usize, stripw: u16, intra: bool) -> DecoderResult<()> {
        let op = self.get_2bits(br)?;
        if op == H_SPLIT {
            validate!(cell.h > 1);
            validate!(cell.d > 0);
            let (cell1, cell2) = cell.split_h();
            self.parse_tree(br, cell1, off, stride, stripw, intra)?;
            self.parse_tree(br, cell2, off, stride, stripw, intra)?;
            Ok(())
        } else if op == V_SPLIT {
            validate!(cell.w > 1);
            validate!(cell.d > 0);
            let (cell1, cell2) = cell.split_w(stripw);
            self.parse_tree(br, cell1, off, stride, stripw, intra)?;
            self.parse_tree(br, cell2, off, stride, stripw, intra)?;
            Ok(())
        } else if op == SKIP_OR_TREE {
            if !cell.vqt {
                let mut newcell = cell;
                newcell.vqt = true;
                newcell.d   -= 1;
                self.parse_tree(br, newcell, off, stride, stripw, intra)
            } else {
                validate!(!intra);
                let code = self.get_2bits(br)?;
                validate!(code < 2);
                if code == 1 { return Err(DecoderError::NotImplemented); }
                self.copy_cell(cell, off, stride)
            }
        } else {
            if !cell.vqt {
                let mut newcell = cell;
                newcell.vqt = true;
                newcell.d  -= 1;
                let mv_idx = br.read_byte()? as usize;
                validate!(mv_idx < self.mvs.len());
                newcell.mv = Some(self.mvs[mv_idx]);
                self.parse_tree(br, newcell, off, stride, stripw, intra)
            } else {
                self.decode_cell(br, cell, off, stride, intra)
            }
        }
    }

    fn decode_plane_intra(&mut self, br: &mut ByteReader, planeno: usize,
                          start: u64, end: u64) -> DecoderResult<()> {
        let offs   = self.bufs.get_offset(planeno);
        let stride = self.bufs.get_stride(planeno);
        br.seek(SeekFrom::Start(start))?;

        let nvec = br.read_u32le()?;
        validate!(nvec == 0); // for intra there should be no mc_vecs
        self.mvs.truncate(0);
        for _ in 0..nvec {
            let x = br.read_byte()? as i8;
            let y = br.read_byte()? as i8;
            self.mvs.push(MV{ x, y });
        }

        let (cellwidth, cellheight) = if planeno == 0 {
                (self.bufs.width >> 2, self.bufs.height >> 2)
            } else {
                (((self.bufs.width >> 2) + 3) >> 2, ((self.bufs.height >> 2) + 3) >> 2)
            };
        let cell = IV3Cell::new(cellwidth as u16, cellheight as u16);
        self.br_reset();
        self.parse_tree(br, cell, offs, stride, if planeno > 0 { 10 } else { 40 }, true)?;
        validate!(br.tell() <= end);
        Ok(())
    }

    fn decode_plane_inter(&mut self, br: &mut ByteReader, planeno: usize,
                          start: u64, end: u64) -> DecoderResult<()> {
        let offs   = self.bufs.get_offset(planeno);
        let stride = self.bufs.get_stride(planeno);
        br.seek(SeekFrom::Start(start))?;

        let nvec = br.read_u32le()?;
        validate!(nvec <= 256); // for intra there should be no mc_vecs
        self.mvs.truncate(0);
        for _ in 0..nvec {
            let y = br.read_byte()? as i8;
            let x = br.read_byte()? as i8;
            self.mvs.push(MV{ x, y });
        }

        let (cellwidth, cellheight) = if planeno == 0 {
                (self.bufs.width >> 2, self.bufs.height >> 2)
            } else {
                (((self.bufs.width >> 2) + 3) >> 2, ((self.bufs.height >> 2) + 3) >> 2)
            };
        let cell = IV3Cell::new(cellwidth as u16, cellheight as u16);
        self.br_reset();
        self.parse_tree(br, cell, offs, stride, if planeno > 0 { 10 } else { 40 }, false)?;
        validate!(br.tell() <= end);
        Ok(())
    }
}

const FLAG_KEYFRAME: u16 = 1 << 2;
const FLAG_NONREF:   u16 = 1 << 8;

impl NADecoder for Indeo3Decoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let fmt = formats::YUV410_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            self.bufs.reset();
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        let mut mr = MemoryReader::new_read(&src);
        let mut br = ByteReader::new(&mut mr);
        let frameno = br.read_u32le()?;
        let hdr_2   = br.read_u32le()?;
        let check   = br.read_u32le()?;
        let size    = br.read_u32le()?;

        let data_start = br.tell();

        if (frameno ^ hdr_2 ^ size ^ FRMH_TAG) != check {
            return Err(DecoderError::InvalidData);
        }
        if i64::from(size) > br.left() { return Err(DecoderError::InvalidData); }
        let ver     = br.read_u16le()?;
        if ver != 32 { return Err(DecoderError::NotImplemented); }
        let flags   = br.read_u16le()?;
        let size2   = br.read_u32le()?;
        if size2 == 0x80 {
            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), NABufferType::None);
            frm.set_keyframe(false);
            frm.set_frame_type(FrameType::Skip);
            return Ok(frm.into_ref());
        }
        validate!(((size2 + 7) >> 3) <= size);
        let cb      = br.read_byte()?;
        self.vq_offset = cb;
        br.read_skip(3)?;
        let height  = br.read_u16le()?;
        let width   = br.read_u16le()?;
        validate!((width  >= 16) && (width  <= 640));
        validate!((height >= 16) && (height <= 640));
        validate!(((width & 3) == 0) && ((height & 3) == 0));
        if (self.bufs.width != (width as usize)) || (self.bufs.height != (height as usize)) {
            self.bufs.alloc(width as usize, height as usize);
        }
        self.width  = width;
        self.height = height;

        let yoff    = br.read_u32le()?;
        let uoff    = br.read_u32le()?;
        let voff    = br.read_u32le()?;
        if yoff > size { return Err(DecoderError::InvalidData); }
        if uoff > size { return Err(DecoderError::InvalidData); }
        if voff > size { return Err(DecoderError::InvalidData); }

        br.read_skip(4)?;
        br.read_buf(&mut self.altquant)?;

        let mut yend = src.len() as u32;//size;
        if (uoff < yend) && (uoff > yoff) { yend = uoff; }
        if (voff < yend) && (voff > yoff) { yend = voff; }
        let mut uend = size;
        if (yoff < uend) && (yoff > uoff) { uend = yoff; }
        if (voff < uend) && (voff > uoff) { uend = voff; }
        let mut vend = size;
        if (yoff < vend) && (yoff > voff) { vend = yoff; }
        if (uoff < vend) && (uoff > voff) { vend = uoff; }

        let intraframe = (flags & FLAG_KEYFRAME) != 0;
        let vinfo = self.info.get_properties().get_video_info().unwrap();
        validate!((vinfo.get_width() & !3) == (self.width & !3).into());
        validate!((vinfo.get_height() & !3) == (self.height & !3).into());
        let bufinfo = alloc_video_buffer(vinfo, 4)?;
        let mut buf = bufinfo.get_vbuf().unwrap();
        let ystart  = data_start + u64::from(yoff);
        let ustart  = data_start + u64::from(uoff);
        let vstart  = data_start + u64::from(voff);
        let yendpos = data_start + u64::from(yend);
        let uendpos = data_start + u64::from(uend);
        let vendpos = data_start + u64::from(vend);
        if intraframe {
            self.decode_plane_intra(&mut br, 0, ystart, yendpos)?;
            self.decode_plane_intra(&mut br, 1, vstart, vendpos)?;
            self.decode_plane_intra(&mut br, 2, ustart, uendpos)?;
        } else {
            self.decode_plane_inter(&mut br, 0, ystart, yendpos)?;
            self.decode_plane_inter(&mut br, 1, vstart, vendpos)?;
            self.decode_plane_inter(&mut br, 2, ustart, uendpos)?;
        }
        self.bufs.fill_framebuf(&mut buf);
        if (flags & FLAG_NONREF) == 0 { self.bufs.flip(); }
        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(intraframe);
        frm.set_frame_type(if intraframe { FrameType::I } else { FrameType::P });
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.bufs.reset();
    }
}

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(Indeo3Decoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::indeo_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;
    #[test]
    fn test_indeo3() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        indeo_register_all_codecs(&mut dec_reg);

        test_decoding("avi", "indeo3", "assets/Indeo/iv32_example.avi", Some(10),
                      &dmx_reg, &dec_reg, ExpectedTestResult::MD5Frames(vec![
                            [0x90be698e, 0x326db071, 0x08e8c6a5, 0x39349acc],
                            [0x25d677fc, 0x63f96aaa, 0xd412ca98, 0x61416313],
                            [0xc4368250, 0x63e7b6bc, 0xffcff950, 0x11f13239],
                            [0x7e869758, 0x027abc2e, 0x25204bca, 0x93fbaa03],
                            [0x5a1e822c, 0x2b1a4cd5, 0x72059843, 0xe5689ad1],
                            [0x3a971cce, 0x5ec22135, 0x1a45f802, 0x0f5f9264],
                            [0x0a65f782, 0xd8767cf3, 0x878b4b8d, 0xfc94c88b],
                            [0x4ac70139, 0x3300eac1, 0xba84b068, 0x47f5ff29],
                            [0x3e8c8ec4, 0x9421b38c, 0x580abbbd, 0x92792d19],
                            [0x9096ee9b, 0x8dd9fb14, 0x981e31e3, 0x3ffd7d29],
                            [0x22dc71ec, 0x3d8f6f7e, 0x1a198982, 0x41d17ecc]]));
    }
}

const DT_1_1: IviDeltaCB = IviDeltaCB{ quad_radix: 7, data: &[
       0,    0,    2,    2,   -2,   -2,   -1,    3,
       1,   -3,    3,   -1,   -3,    1,    4,    4,
      -4,   -4,    1,    5,   -1,   -5,    5,    1,
      -5,   -1,   -4,    4,    4,   -4,   -2,    6,
       2,   -6,    6,   -2,   -6,    2,    4,    9,
      -4,   -9,    9,    4,   -9,   -4,    9,    9,
      -9,   -9,    1,   10,   -1,  -10,   10,    1,
     -10,   -1,   -5,    8,    5,   -8,    8,   -5,
      -8,    5,    9,   15,   -9,  -15,   15,    9,
     -15,   -9,   -3,   12,    3,  -12,   12,   -3,
     -12,    3,    4,   16,   -4,  -16,   16,    4,
     -16,   -4,   16,   16,  -16,  -16,    0,   18,
       0,  -18,   18,    0,  -18,    0,  -12,   12,
      12,  -12,   -9,   16,    9,  -16,   16,   -9,
     -16,    9,   11,   27,  -11,  -27,   27,   11,
     -27,  -11,   19,   28,  -19,  -28,   28,   19,
     -28,  -19,   -6,   22,    6,  -22,   22,   -6,
     -22,    6,    4,   29,   -4,  -29,   29,    4,
     -29,   -4,   30,   30,  -30,  -30,   -2,   33,
       2,  -33,   33,   -2,  -33,    2,  -18,   23,
      18,  -23,   23,  -18,  -23,   18,  -15,   30,
      15,  -30,   30,  -15,  -30,   15,   22,   46,
     -22,  -46,   46,   22,  -46,  -22,   13,   47,
     -13,  -47,   47,   13,  -47,  -13,   35,   49,
     -35,  -49,   49,   35,  -49,  -35,  -11,   41,
      11,  -41,   41,  -11,  -41,   11,    4,   51,
      -4,  -51,   51,    4,  -51,   -4,   54,   54,
     -54,  -54,  -34,   34,   34,  -34,  -29,   42,
      29,  -42,   42,  -29,  -42,   29,   -6,   60,
       6,  -60,   60,   -6,  -60,    6,   27,   76,
     -27,  -76,   76,   27,  -76,  -27,   43,   77,
     -43,  -77,   77,   43,  -77,  -43,  -24,   55,
      24,  -55,   55,  -24,  -55,   24,   14,   79,
     -14,  -79,   79,   14,  -79,  -14,   63,   83,
     -63,  -83,   83,   63,  -83,  -63,  -20,   74,
      20,  -74,   74,  -20,  -74,   20,    2,   88,
      -2,  -88,   88,    2,  -88,   -2,   93,   93,
     -93,  -93,  -52,   61,   52,  -61,   61,  -52,
     -61,   52,   52,  120,  -52, -120,  120,   52,
    -120,  -52,  -45,   75,   45,  -75,   75,  -45,
     -75,   45,   75,  125,  -75, -125,  125,   75,
    -125,  -75,   33,  122,  -33, -122,  122,   33,
    -122,  -33,  -13,  103,   13, -103,  103,  -13,
    -103,   13,  -40,   96,   40,  -96,   96,  -40,
     -96,   40,  -34,  127,   34, -127,  127,  -34,
    -127,   34,  -89,   89,   89,  -89,  -78,  105,
      78, -105,  105,  -78, -105,   78,   12,   12,
     -12,  -12,   23,   23,  -23,  -23,   42,   42,
     -42,  -42,   73,   73,  -73,  -73,
]};

const DT_1_2: IviDeltaCB = IviDeltaCB{ quad_radix: 9, data: &[
       0,    0,    3,    3,   -3,   -3,   -1,    4,
       1,   -4,    4,   -1,   -4,    1,    7,    7,
      -7,   -7,    2,    8,   -2,   -8,    8,    2,
      -8,   -2,   -2,    9,    2,   -9,    9,   -2,
      -9,    2,   -6,    6,    6,   -6,    6,   13,
      -6,  -13,   13,    6,  -13,   -6,   13,   13,
     -13,  -13,    1,   14,   -1,  -14,   14,    1,
     -14,   -1,   -8,   12,    8,  -12,   12,   -8,
     -12,    8,   14,   23,  -14,  -23,   23,   14,
     -23,  -14,   -5,   18,    5,  -18,   18,   -5,
     -18,    5,    6,   24,   -6,  -24,   24,    6,
     -24,   -6,   24,   24,  -24,  -24,   -1,   27,
       1,  -27,   27,   -1,  -27,    1,  -17,   17,
      17,  -17,  -13,   23,   13,  -23,   23,  -13,
     -23,   13,   16,   40,  -16,  -40,   40,   16,
     -40,  -16,   28,   41,  -28,  -41,   41,   28,
     -41,  -28,   -9,   33,    9,  -33,   33,   -9,
     -33,    9,    6,   43,   -6,  -43,   43,    6,
     -43,   -6,   46,   46,  -46,  -46,   -4,   50,
       4,  -50,   50,   -4,  -50,    4,  -27,   34,
      27,  -34,   34,  -27,  -34,   27,  -22,   45,
      22,  -45,   45,  -22,  -45,   22,   34,   69,
     -34,  -69,   69,   34,  -69,  -34,   19,   70,
     -19,  -70,   70,   19,  -70,  -19,   53,   73,
     -53,  -73,   73,   53,  -73,  -53,  -17,   62,
      17,  -62,   62,  -17,  -62,   17,    5,   77,
      -5,  -77,   77,    5,  -77,   -5,   82,   82,
     -82,  -82,  -51,   51,   51,  -51,  -43,   64,
      43,  -64,   64,  -43,  -64,   43,  -10,   90,
      10,  -90,   90,  -10,  -90,   10,   41,  114,
     -41, -114,  114,   41, -114,  -41,   64,  116,
     -64, -116,  116,   64, -116,  -64,  -37,   82,
      37,  -82,   82,  -37,  -82,   37,   22,  119,
     -22, -119,  119,   22, -119,  -22,   95,  124,
     -95, -124,  124,   95, -124,  -95,  -30,  111,
      30, -111,  111,  -30, -111,   30,  -78,   92,
      78,  -92,   92,  -78,  -92,   78,  -68,  113,
      68, -113,  113,  -68, -113,   68,   18,   18,
     -18,  -18,   34,   34,  -34,  -34,   63,   63,
     -63,  -63,  109,  109, -109, -109,
]};

const DT_1_3: IviDeltaCB = IviDeltaCB{ quad_radix: 10, data: &[
       0,    0,    4,    4,   -4,   -4,   -1,    5,
       1,   -5,    5,   -1,   -5,    1,    3,   10,
      -3,  -10,   10,    3,  -10,   -3,    9,    9,
      -9,   -9,   -7,    7,    7,   -7,   -3,   12,
       3,  -12,   12,   -3,  -12,    3,    8,   17,
      -8,  -17,   17,    8,  -17,   -8,   17,   17,
     -17,  -17,    1,   19,   -1,  -19,   19,    1,
     -19,   -1,  -11,   16,   11,  -16,   16,  -11,
     -16,   11,   -6,   23,    6,  -23,   23,   -6,
     -23,    6,   18,   31,  -18,  -31,   31,   18,
     -31,  -18,    8,   32,   -8,  -32,   32,    8,
     -32,   -8,   33,   33,  -33,  -33,   -1,   36,
       1,  -36,   36,   -1,  -36,    1,  -23,   23,
      23,  -23,  -17,   31,   17,  -31,   31,  -17,
     -31,   17,   21,   54,  -21,  -54,   54,   21,
     -54,  -21,   37,   55,  -37,  -55,   55,   37,
     -55,  -37,  -12,   44,   12,  -44,   44,  -12,
     -44,   12,    8,   57,   -8,  -57,   57,    8,
     -57,   -8,   61,   61,  -61,  -61,   -5,   66,
       5,  -66,   66,   -5,  -66,    5,  -36,   45,
      36,  -45,   45,  -36,  -45,   36,  -29,   60,
      29,  -60,   60,  -29,  -60,   29,   45,   92,
     -45,  -92,   92,   45,  -92,  -45,   25,   93,
     -25,  -93,   93,   25,  -93,  -25,   71,   97,
     -71,  -97,   97,   71,  -97,  -71,  -22,   83,
      22,  -83,   83,  -22,  -83,   22,    7,  102,
      -7, -102,  102,    7, -102,   -7,  109,  109,
    -109, -109,  -68,   68,   68,  -68,  -57,   85,
      57,  -85,   85,  -57,  -85,   57,  -13,  120,
      13, -120,  120,  -13, -120,   13,  -49,  110,
      49, -110,  110,  -49, -110,   49, -104,  123,
     104, -123,  123, -104, -123,  104,   24,   24,
     -24,  -24,   46,   46,  -46,  -46,   84,   84,
     -84,  -84,
]};

const DT_1_4: IviDeltaCB = IviDeltaCB{ quad_radix: 11, data: &[
       0,    0,    5,    5,   -5,   -5,   -2,    7,
       2,   -7,    7,   -2,   -7,    2,   11,   11,
     -11,  -11,    3,   13,   -3,  -13,   13,    3,
     -13,   -3,   -9,    9,    9,   -9,   -4,   15,
       4,  -15,   15,   -4,  -15,    4,   11,   22,
     -11,  -22,   22,   11,  -22,  -11,   21,   21,
     -21,  -21,    2,   24,   -2,  -24,   24,    2,
     -24,   -2,  -14,   20,   14,  -20,   20,  -14,
     -20,   14,   23,   38,  -23,  -38,   38,   23,
     -38,  -23,   -8,   29,    8,  -29,   29,   -8,
     -29,    8,   11,   39,  -11,  -39,   39,   11,
     -39,  -11,   41,   41,  -41,  -41,   -1,   45,
       1,  -45,   45,   -1,  -45,    1,  -29,   29,
      29,  -29,  -22,   39,   22,  -39,   39,  -22,
     -39,   22,   27,   67,  -27,  -67,   67,   27,
     -67,  -27,   47,   69,  -47,  -69,   69,   47,
     -69,  -47,  -15,   56,   15,  -56,   56,  -15,
     -56,   15,   11,   71,  -11,  -71,   71,   11,
     -71,  -11,   76,   76,  -76,  -76,   -6,   83,
       6,  -83,   83,   -6,  -83,    6,  -45,   57,
      45,  -57,   57,  -45,  -57,   45,  -36,   75,
      36,  -75,   75,  -36,  -75,   36,   56,  115,
     -56, -115,  115,   56, -115,  -56,   31,  117,
     -31, -117,  117,   31, -117,  -31,   88,  122,
     -88, -122,  122,   88, -122,  -88,  -28,  104,
      28, -104,  104,  -28, -104,   28,  -85,   85,
      85,  -85,  -72,  106,   72, -106,  106,  -72,
    -106,   72,   30,   30,  -30,  -30,   58,   58,
     -58,  -58,  105,  105, -105, -105,
]};

const DT_1_5: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    6,    6,   -6,   -6,   -2,    8,
       2,   -8,    8,   -2,   -8,    2,   13,   13,
     -13,  -13,    4,   15,   -4,  -15,   15,    4,
     -15,   -4,  -11,   11,   11,  -11,   -5,   18,
       5,  -18,   18,   -5,  -18,    5,   13,   26,
     -13,  -26,   26,   13,  -26,  -13,   26,   26,
     -26,  -26,    2,   29,   -2,  -29,   29,    2,
     -29,   -2,  -16,   24,   16,  -24,   24,  -16,
     -24,   16,   28,   46,  -28,  -46,   46,   28,
     -46,  -28,   -9,   35,    9,  -35,   35,   -9,
     -35,    9,   13,   47,  -13,  -47,   47,   13,
     -47,  -13,   49,   49,  -49,  -49,   -1,   54,
       1,  -54,   54,   -1,  -54,    1,  -35,   35,
      35,  -35,  -26,   47,   26,  -47,   47,  -26,
     -47,   26,   32,   81,  -32,  -81,   81,   32,
     -81,  -32,   56,   83,  -56,  -83,   83,   56,
     -83,  -56,  -18,   67,   18,  -67,   67,  -18,
     -67,   18,   13,   86,  -13,  -86,   86,   13,
     -86,  -13,   91,   91,  -91,  -91,   -7,   99,
       7,  -99,   99,   -7,  -99,    7,  -54,   68,
      54,  -68,   68,  -54,  -68,   54,  -44,   90,
      44,  -90,   90,  -44,  -90,   44,  -33,  124,
      33, -124,  124,  -33, -124,   33, -103,  103,
     103, -103,  -86,  127,   86, -127,  127,  -86,
    -127,   86,   37,   37,  -37,  -37,   69,   69,
     -69,  -69,
]};

const DT_1_6: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    7,    7,   -7,   -7,   -3,   10,
       3,  -10,   10,   -3,  -10,    3,   16,   16,
     -16,  -16,    5,   18,   -5,  -18,   18,    5,
     -18,   -5,  -13,   13,   13,  -13,   -6,   21,
       6,  -21,   21,   -6,  -21,    6,   15,   30,
     -15,  -30,   30,   15,  -30,  -15,   30,   30,
     -30,  -30,    2,   34,   -2,  -34,   34,    2,
     -34,   -2,  -19,   28,   19,  -28,   28,  -19,
     -28,   19,   32,   54,  -32,  -54,   54,   32,
     -54,  -32,  -11,   41,   11,  -41,   41,  -11,
     -41,   11,   15,   55,  -15,  -55,   55,   15,
     -55,  -15,   57,   57,  -57,  -57,   -1,   63,
       1,  -63,   63,   -1,  -63,    1,  -40,   40,
      40,  -40,  -30,   55,   30,  -55,   55,  -30,
     -55,   30,   37,   94,  -37,  -94,   94,   37,
     -94,  -37,   65,   96,  -65,  -96,   96,   65,
     -96,  -65,  -21,   78,   21,  -78,   78,  -21,
     -78,   21,   15,  100,  -15, -100,  100,   15,
    -100,  -15,  106,  106, -106, -106,   -8,  116,
       8, -116,  116,   -8, -116,    8,  -63,   79,
      63,  -79,   79,  -63,  -79,   63,  -51,  105,
      51, -105,  105,  -51, -105,   51, -120,  120,
     120, -120,   43,   43,  -43,  -43,   80,   80,
     -80,  -80,
]};

const DT_1_7: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    8,    8,   -8,   -8,   -3,   11,
       3,  -11,   11,   -3,  -11,    3,   18,   18,
     -18,  -18,    5,   20,   -5,  -20,   20,    5,
     -20,   -5,  -15,   15,   15,  -15,   -7,   24,
       7,  -24,   24,   -7,  -24,    7,   17,   35,
     -17,  -35,   35,   17,  -35,  -17,   34,   34,
     -34,  -34,    3,   38,   -3,  -38,   38,    3,
     -38,   -3,  -22,   32,   22,  -32,   32,  -22,
     -32,   22,   37,   61,  -37,  -61,   61,   37,
     -61,  -37,  -13,   47,   13,  -47,   47,  -13,
     -47,   13,   17,   63,  -17,  -63,   63,   17,
     -63,  -17,   65,   65,  -65,  -65,   -1,   72,
       1,  -72,   72,   -1,  -72,    1,  -46,   46,
      46,  -46,  -35,   63,   35,  -63,   63,  -35,
     -63,   35,   43,  107,  -43, -107,  107,   43,
    -107,  -43,   75,  110,  -75, -110,  110,   75,
    -110,  -75,  -24,   89,   24,  -89,   89,  -24,
     -89,   24,   17,  114,  -17, -114,  114,   17,
    -114,  -17,  121,  121, -121, -121,  -72,   91,
      72,  -91,   91,  -72,  -91,   72,  -58,  120,
      58, -120,  120,  -58, -120,   58,   49,   49,
     -49,  -49,   92,   92,  -92,  -92,
]};

const DT_1_8: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    9,    9,   -9,   -9,   -3,   12,
       3,  -12,   12,   -3,  -12,    3,   20,   20,
     -20,  -20,    6,   23,   -6,  -23,   23,    6,
     -23,   -6,  -17,   17,   17,  -17,   -7,   27,
       7,  -27,   27,   -7,  -27,    7,   19,   39,
     -19,  -39,   39,   19,  -39,  -19,   39,   39,
     -39,  -39,    3,   43,   -3,  -43,   43,    3,
     -43,   -3,  -24,   36,   24,  -36,   36,  -24,
     -36,   24,   42,   69,  -42,  -69,   69,   42,
     -69,  -42,  -14,   53,   14,  -53,   53,  -14,
     -53,   14,   19,   71,  -19,  -71,   71,   19,
     -71,  -19,   73,   73,  -73,  -73,   -2,   80,
       2,  -80,   80,   -2,  -80,    2,  -52,   52,
      52,  -52,  -39,   70,   39,  -70,   70,  -39,
     -70,   39,   48,  121,  -48, -121,  121,   48,
    -121,  -48,   84,  124,  -84, -124,  124,   84,
    -124,  -84,  -27,  100,   27, -100,  100,  -27,
    -100,   27,  -81,  102,   81, -102,  102,  -81,
    -102,   81,   55,   55,  -55,  -55,  104,  104,
    -104, -104,
]};

const DT_2_1: IviDeltaCB = IviDeltaCB{ quad_radix: 7, data: &[
       0,    0,    2,    2,   -2,   -2,    0,    2,
       0,   -2,    2,    0,   -2,    0,    4,    4,
      -4,   -4,    0,    4,    0,   -4,    4,    0,
      -4,    0,   -4,    4,    4,   -4,   -2,    6,
       2,   -6,    6,   -2,   -6,    2,    4,    8,
      -4,   -8,    8,    4,   -8,   -4,    8,    8,
      -8,   -8,    0,   10,    0,  -10,   10,    0,
     -10,    0,   -4,    8,    4,   -8,    8,   -4,
      -8,    4,    8,   14,   -8,  -14,   14,    8,
     -14,   -8,   -2,   12,    2,  -12,   12,   -2,
     -12,    2,    4,   16,   -4,  -16,   16,    4,
     -16,   -4,   16,   16,  -16,  -16,    0,   18,
       0,  -18,   18,    0,  -18,    0,  -12,   12,
      12,  -12,   -8,   16,    8,  -16,   16,   -8,
     -16,    8,   10,   26,  -10,  -26,   26,   10,
     -26,  -10,   18,   28,  -18,  -28,   28,   18,
     -28,  -18,   -6,   22,    6,  -22,   22,   -6,
     -22,    6,    4,   28,   -4,  -28,   28,    4,
     -28,   -4,   30,   30,  -30,  -30,   -2,   32,
       2,  -32,   32,   -2,  -32,    2,  -18,   22,
      18,  -22,   22,  -18,  -22,   18,  -14,   30,
      14,  -30,   30,  -14,  -30,   14,   22,   46,
     -22,  -46,   46,   22,  -46,  -22,   12,   46,
     -12,  -46,   46,   12,  -46,  -12,   34,   48,
     -34,  -48,   48,   34,  -48,  -34,  -10,   40,
      10,  -40,   40,  -10,  -40,   10,    4,   50,
      -4,  -50,   50,    4,  -50,   -4,   54,   54,
     -54,  -54,  -34,   34,   34,  -34,  -28,   42,
      28,  -42,   42,  -28,  -42,   28,   -6,   60,
       6,  -60,   60,   -6,  -60,    6,   26,   76,
     -26,  -76,   76,   26,  -76,  -26,   42,   76,
     -42,  -76,   76,   42,  -76,  -42,  -24,   54,
      24,  -54,   54,  -24,  -54,   24,   14,   78,
     -14,  -78,   78,   14,  -78,  -14,   62,   82,
     -62,  -82,   82,   62,  -82,  -62,  -20,   74,
      20,  -74,   74,  -20,  -74,   20,    2,   88,
      -2,  -88,   88,    2,  -88,   -2,   92,   92,
     -92,  -92,  -52,   60,   52,  -60,   60,  -52,
     -60,   52,   52,  118,  -52, -118,  118,   52,
    -118,  -52,  -44,   74,   44,  -74,   74,  -44,
     -74,   44,   74,  118,  -74, -118,  118,   74,
    -118,  -74,   32,  118,  -32, -118,  118,   32,
    -118,  -32,  -12,  102,   12, -102,  102,  -12,
    -102,   12,  -40,   96,   40,  -96,   96,  -40,
     -96,   40,  -34,  118,   34, -118,  118,  -34,
    -118,   34,  -88,   88,   88,  -88,  -78,  104,
      78, -104,  104,  -78, -104,   78,   12,   12,
     -12,  -12,   22,   22,  -22,  -22,   42,   42,
     -42,  -42,   72,   72,  -72,  -72,
]};

const DT_2_2: IviDeltaCB = IviDeltaCB{ quad_radix: 9, data: &[
       0,    0,    3,    3,   -3,   -3,    0,    3,
       0,   -3,    3,    0,   -3,    0,    6,    6,
      -6,   -6,    3,    9,   -3,   -9,    9,    3,
      -9,   -3,   -3,    9,    3,   -9,    9,   -3,
      -9,    3,   -6,    6,    6,   -6,    6,   12,
      -6,  -12,   12,    6,  -12,   -6,   12,   12,
     -12,  -12,    0,   15,    0,  -15,   15,    0,
     -15,    0,   -9,   12,    9,  -12,   12,   -9,
     -12,    9,   15,   24,  -15,  -24,   24,   15,
     -24,  -15,   -6,   18,    6,  -18,   18,   -6,
     -18,    6,    6,   24,   -6,  -24,   24,    6,
     -24,   -6,   24,   24,  -24,  -24,    0,   27,
       0,  -27,   27,    0,  -27,    0,  -18,   18,
      18,  -18,  -12,   24,   12,  -24,   24,  -12,
     -24,   12,   15,   39,  -15,  -39,   39,   15,
     -39,  -15,   27,   42,  -27,  -42,   42,   27,
     -42,  -27,   -9,   33,    9,  -33,   33,   -9,
     -33,    9,    6,   42,   -6,  -42,   42,    6,
     -42,   -6,   45,   45,  -45,  -45,   -3,   51,
       3,  -51,   51,   -3,  -51,    3,  -27,   33,
      27,  -33,   33,  -27,  -33,   27,  -21,   45,
      21,  -45,   45,  -21,  -45,   21,   33,   69,
     -33,  -69,   69,   33,  -69,  -33,   18,   69,
     -18,  -69,   69,   18,  -69,  -18,   54,   72,
     -54,  -72,   72,   54,  -72,  -54,  -18,   63,
      18,  -63,   63,  -18,  -63,   18,    6,   78,
      -6,  -78,   78,    6,  -78,   -6,   81,   81,
     -81,  -81,  -51,   51,   51,  -51,  -42,   63,
      42,  -63,   63,  -42,  -63,   42,   -9,   90,
       9,  -90,   90,   -9,  -90,    9,   42,  114,
     -42, -114,  114,   42, -114,  -42,   63,  117,
     -63, -117,  117,   63, -117,  -63,  -36,   81,
      36,  -81,   81,  -36,  -81,   36,   21,  120,
     -21, -120,  120,   21, -120,  -21,   96,  123,
     -96, -123,  123,   96, -123,  -96,  -30,  111,
      30, -111,  111,  -30, -111,   30,  -78,   93,
      78,  -93,   93,  -78,  -93,   78,  -69,  114,
      69, -114,  114,  -69, -114,   69,   18,   18,
     -18,  -18,   33,   33,  -33,  -33,   63,   63,
     -63,  -63,  108,  108, -108, -108,
]};

const DT_2_3: IviDeltaCB = IviDeltaCB{ quad_radix: 10, data: &[
       0,    0,    4,    4,   -4,   -4,    0,    4,
       0,   -4,    4,    0,   -4,    0,    4,    8,
      -4,   -8,    8,    4,   -8,   -4,    8,    8,
      -8,   -8,   -8,    8,    8,   -8,   -4,   12,
       4,  -12,   12,   -4,  -12,    4,    8,   16,
      -8,  -16,   16,    8,  -16,   -8,   16,   16,
     -16,  -16,    0,   20,    0,  -20,   20,    0,
     -20,    0,  -12,   16,   12,  -16,   16,  -12,
     -16,   12,   -4,   24,    4,  -24,   24,   -4,
     -24,    4,   16,   32,  -16,  -32,   32,   16,
     -32,  -16,    8,   32,   -8,  -32,   32,    8,
     -32,   -8,   32,   32,  -32,  -32,    0,   36,
       0,  -36,   36,    0,  -36,    0,  -24,   24,
      24,  -24,  -16,   32,   16,  -32,   32,  -16,
     -32,   16,   20,   52,  -20,  -52,   52,   20,
     -52,  -20,   36,   56,  -36,  -56,   56,   36,
     -56,  -36,  -12,   44,   12,  -44,   44,  -12,
     -44,   12,    8,   56,   -8,  -56,   56,    8,
     -56,   -8,   60,   60,  -60,  -60,   -4,   64,
       4,  -64,   64,   -4,  -64,    4,  -36,   44,
      36,  -44,   44,  -36,  -44,   36,  -28,   60,
      28,  -60,   60,  -28,  -60,   28,   44,   92,
     -44,  -92,   92,   44,  -92,  -44,   24,   92,
     -24,  -92,   92,   24,  -92,  -24,   72,   96,
     -72,  -96,   96,   72,  -96,  -72,  -20,   84,
      20,  -84,   84,  -20,  -84,   20,    8,  100,
      -8, -100,  100,    8, -100,   -8,  108,  108,
    -108, -108,  -68,   68,   68,  -68,  -56,   84,
      56,  -84,   84,  -56,  -84,   56,  -12,  120,
      12, -120,  120,  -12, -120,   12,  -48,  108,
      48, -108,  108,  -48, -108,   48, -104,  124,
     104, -124,  124, -104, -124,  104,   24,   24,
     -24,  -24,   44,   44,  -44,  -44,   84,   84,
     -84,  -84,
]};

const DT_2_4: IviDeltaCB = IviDeltaCB{ quad_radix: 11, data: &[
       0,    0,    5,    5,   -5,   -5,    0,    5,
       0,   -5,    5,    0,   -5,    0,   10,   10,
     -10,  -10,    5,   15,   -5,  -15,   15,    5,
     -15,   -5,  -10,   10,   10,  -10,   -5,   15,
       5,  -15,   15,   -5,  -15,    5,   10,   20,
     -10,  -20,   20,   10,  -20,  -10,   20,   20,
     -20,  -20,    0,   25,    0,  -25,   25,    0,
     -25,    0,  -15,   20,   15,  -20,   20,  -15,
     -20,   15,   25,   40,  -25,  -40,   40,   25,
     -40,  -25,  -10,   30,   10,  -30,   30,  -10,
     -30,   10,   10,   40,  -10,  -40,   40,   10,
     -40,  -10,   40,   40,  -40,  -40,    0,   45,
       0,  -45,   45,    0,  -45,    0,  -30,   30,
      30,  -30,  -20,   40,   20,  -40,   40,  -20,
     -40,   20,   25,   65,  -25,  -65,   65,   25,
     -65,  -25,   45,   70,  -45,  -70,   70,   45,
     -70,  -45,  -15,   55,   15,  -55,   55,  -15,
     -55,   15,   10,   70,  -10,  -70,   70,   10,
     -70,  -10,   75,   75,  -75,  -75,   -5,   85,
       5,  -85,   85,   -5,  -85,    5,  -45,   55,
      45,  -55,   55,  -45,  -55,   45,  -35,   75,
      35,  -75,   75,  -35,  -75,   35,   55,  115,
     -55, -115,  115,   55, -115,  -55,   30,  115,
     -30, -115,  115,   30, -115,  -30,   90,  120,
     -90, -120,  120,   90, -120,  -90,  -30,  105,
      30, -105,  105,  -30, -105,   30,  -85,   85,
      85,  -85,  -70,  105,   70, -105,  105,  -70,
    -105,   70,   30,   30,  -30,  -30,   60,   60,
     -60,  -60,  105,  105, -105, -105,
]};

const DT_2_5: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    6,    6,   -6,   -6,    0,    6,
       0,   -6,    6,    0,   -6,    0,   12,   12,
     -12,  -12,    6,   12,   -6,  -12,   12,    6,
     -12,   -6,  -12,   12,   12,  -12,   -6,   18,
       6,  -18,   18,   -6,  -18,    6,   12,   24,
     -12,  -24,   24,   12,  -24,  -12,   24,   24,
     -24,  -24,    0,   30,    0,  -30,   30,    0,
     -30,    0,  -18,   24,   18,  -24,   24,  -18,
     -24,   18,   30,   48,  -30,  -48,   48,   30,
     -48,  -30,   -6,   36,    6,  -36,   36,   -6,
     -36,    6,   12,   48,  -12,  -48,   48,   12,
     -48,  -12,   48,   48,  -48,  -48,    0,   54,
       0,  -54,   54,    0,  -54,    0,  -36,   36,
      36,  -36,  -24,   48,   24,  -48,   48,  -24,
     -48,   24,   30,   78,  -30,  -78,   78,   30,
     -78,  -30,   54,   84,  -54,  -84,   84,   54,
     -84,  -54,  -18,   66,   18,  -66,   66,  -18,
     -66,   18,   12,   84,  -12,  -84,   84,   12,
     -84,  -12,   90,   90,  -90,  -90,   -6,   96,
       6,  -96,   96,   -6,  -96,    6,  -54,   66,
      54,  -66,   66,  -54,  -66,   54,  -42,   90,
      42,  -90,   90,  -42,  -90,   42,  -30,  126,
      30, -126,  126,  -30, -126,   30, -102,  102,
     102, -102,  -84,  126,   84, -126,  126,  -84,
    -126,   84,   36,   36,  -36,  -36,   66,   66,
     -66,  -66,
]};

const DT_2_6: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    7,    7,   -7,   -7,    0,    7,
       0,   -7,    7,    0,   -7,    0,   14,   14,
     -14,  -14,    7,   21,   -7,  -21,   21,    7,
     -21,   -7,  -14,   14,   14,  -14,   -7,   21,
       7,  -21,   21,   -7,  -21,    7,   14,   28,
     -14,  -28,   28,   14,  -28,  -14,   28,   28,
     -28,  -28,    0,   35,    0,  -35,   35,    0,
     -35,    0,  -21,   28,   21,  -28,   28,  -21,
     -28,   21,   35,   56,  -35,  -56,   56,   35,
     -56,  -35,  -14,   42,   14,  -42,   42,  -14,
     -42,   14,   14,   56,  -14,  -56,   56,   14,
     -56,  -14,   56,   56,  -56,  -56,    0,   63,
       0,  -63,   63,    0,  -63,    0,  -42,   42,
      42,  -42,  -28,   56,   28,  -56,   56,  -28,
     -56,   28,   35,   91,  -35,  -91,   91,   35,
     -91,  -35,   63,   98,  -63,  -98,   98,   63,
     -98,  -63,  -21,   77,   21,  -77,   77,  -21,
     -77,   21,   14,   98,  -14,  -98,   98,   14,
     -98,  -14,  105,  105, -105, -105,   -7,  119,
       7, -119,  119,   -7, -119,    7,  -63,   77,
      63,  -77,   77,  -63,  -77,   63,  -49,  105,
      49, -105,  105,  -49, -105,   49, -119,  119,
     119, -119,   42,   42,  -42,  -42,   77,   77,
     -77,  -77,
]};

const DT_2_7: IviDeltaCB = IviDeltaCB{ quad_radix: 12, data: &[
       0,    0,    8,    8,   -8,   -8,    0,    8,
       0,   -8,    8,    0,   -8,    0,   16,   16,
     -16,  -16,    8,   16,   -8,  -16,   16,    8,
     -16,   -8,  -16,   16,   16,  -16,   -8,   24,
       8,  -24,   24,   -8,  -24,    8,   16,   32,
     -16,  -32,   32,   16,  -32,  -16,   32,   32,
     -32,  -32,    0,   40,    0,  -40,   40,    0,
     -40,    0,  -24,   32,   24,  -32,   32,  -24,
     -32,   24,   40,   64,  -40,  -64,   64,   40,
     -64,  -40,  -16,   48,   16,  -48,   48,  -16,
     -48,   16,   16,   64,  -16,  -64,   64,   16,
     -64,  -16,   64,   64,  -64,  -64,    0,   72,
       0,  -72,   72,    0,  -72,    0,  -48,   48,
      48,  -48,  -32,   64,   32,  -64,   64,  -32,
     -64,   32,   40,  104,  -40, -104,  104,   40,
    -104,  -40,   72,  112,  -72, -112,  112,   72,
    -112,  -72,  -24,   88,   24,  -88,   88,  -24,
     -88,   24,   16,  112,  -16, -112,  112,   16,
    -112,  -16,  120,  120, -120, -120,  -72,   88,
      72,  -88,   88,  -72,  -88,   72,  -56,  120,
      56, -120,  120,  -56, -120,   56,   48,   48,
     -48,  -48,   88,   88,  -88,  -88,
]};

const DT_2_8: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    9,    9,   -9,   -9,    0,    9,
       0,   -9,    9,    0,   -9,    0,   18,   18,
     -18,  -18,    9,   27,   -9,  -27,   27,    9,
     -27,   -9,  -18,   18,   18,  -18,   -9,   27,
       9,  -27,   27,   -9,  -27,    9,   18,   36,
     -18,  -36,   36,   18,  -36,  -18,   36,   36,
     -36,  -36,    0,   45,    0,  -45,   45,    0,
     -45,    0,  -27,   36,   27,  -36,   36,  -27,
     -36,   27,   45,   72,  -45,  -72,   72,   45,
     -72,  -45,  -18,   54,   18,  -54,   54,  -18,
     -54,   18,   18,   72,  -18,  -72,   72,   18,
     -72,  -18,   72,   72,  -72,  -72,    0,   81,
       0,  -81,   81,    0,  -81,    0,  -54,   54,
      54,  -54,  -36,   72,   36,  -72,   72,  -36,
     -72,   36,   45,  117,  -45, -117,  117,   45,
    -117,  -45,   81,  126,  -81, -126,  126,   81,
    -126,  -81,  -27,   99,   27,  -99,   99,  -27,
     -99,   27,  -81,   99,   81,  -99,   99,  -81,
     -99,   81,   54,   54,  -54,  -54,  108,  108,
    -108, -108,
]};

const DT_3_1: IviDeltaCB = IviDeltaCB{ quad_radix: 11, data: &[
       0,    0,    2,    2,   -2,   -2,    0,    3,
       0,   -3,    3,    0,   -3,    0,    6,    6,
      -6,   -6,    0,    7,    0,   -7,    7,    0,
      -7,    0,   -5,    5,    5,   -5,    5,   -5,
      -5,    5,    6,   11,   -6,  -11,   11,    6,
     -11,   -6,    0,    8,    0,   -8,    8,    0,
      -8,    0,   11,   11,  -11,  -11,    0,   12,
       0,  -12,   12,    0,  -12,    0,   12,   17,
     -12,  -17,   17,   12,  -17,  -12,   17,   17,
     -17,  -17,    6,   18,   -6,  -18,   18,    6,
     -18,   -6,   -8,   11,    8,  -11,   11,   -8,
     -11,    8,    0,   15,    0,  -15,   15,    0,
     -15,    0,    0,   20,    0,  -20,   20,    0,
     -20,    0,   18,   25,  -18,  -25,   25,   18,
     -25,  -18,   11,   25,  -11,  -25,   25,   11,
     -25,  -11,   25,   25,  -25,  -25,  -14,   14,
      14,  -14,   14,  -14,  -14,   14,    0,   26,
       0,  -26,   26,    0,  -26,    0,  -11,   18,
      11,  -18,   18,  -11,  -18,   11,   -7,   22,
       7,  -22,   22,   -7,  -22,    7,   26,   34,
     -26,  -34,   34,   26,  -34,  -26,   18,   34,
     -18,  -34,   34,   18,  -34,  -18,   34,   34,
     -34,  -34,   11,   35,  -11,  -35,   35,   11,
     -35,  -11,    0,   29,    0,  -29,   29,    0,
     -29,    0,  -19,   22,   19,  -22,   22,  -19,
     -22,   19,  -15,   26,   15,  -26,   26,  -15,
     -26,   15,    0,   37,    0,  -37,   37,    0,
     -37,    0,   27,   44,  -27,  -44,   44,   27,
     -44,  -27,   36,   44,  -36,  -44,   44,   36,
     -44,  -36,   18,   44,  -18,  -44,   44,   18,
     -44,  -18,  -10,   33,   10,  -33,   33,  -10,
     -33,   10,   45,   45,  -45,  -45,    0,    0,
]};

const DT_3_2: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    0,    2,    0,   -2,    2,    0,
      -2,    0,    2,    2,   -2,   -2,    6,    6,
      -6,   -6,    0,    6,    0,   -6,    6,    0,
      -6,    0,   -4,    4,    4,   -4,   10,   -6,
     -10,    6,    0,  -12,    0,   12,   -6,  -12,
       6,  -12,   -6,   12,    6,   12,  -14,    0,
      14,    0,   12,   12,  -12,  -12,    0,  -18,
       0,   18,   14,  -12,  -14,   12,  -18,   -6,
      18,   -6,  -18,    6,   18,    6,  -10,  -18,
      10,  -18,  -10,   18,   10,   18,  -22,    0,
      22,    0,    0,  -24,    0,   24,  -22,  -12,
      22,  -12,  -22,   12,   22,   12,   -8,  -24,
       8,  -24,   -8,   24,    8,   24,  -26,   -6,
      26,   -6,  -26,    6,   26,    6,  -28,    0,
      28,    0,   20,   20,  -20,  -20,  -14,  -26,
      14,   26,  -30,  -12,   30,   12,  -10,  -32,
      10,   32,  -18,  -32,   18,   32,  -26,  -26,
      26,   26,  -34,  -20,   34,   20,  -38,  -12,
      38,   12,  -32,  -32,   32,   32,   32,   32,
     -22,  -40,  -34,  -34,   34,   34,
]};

const DT_3_3: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    0,    2,    0,   -2,    2,    0,
      -2,    0,    4,    4,   -4,   -4,   10,   10,
     -10,  -10,    0,   10,    0,  -10,   10,    0,
     -10,    0,   -6,    6,    6,   -6,   14,   -8,
     -14,    8,  -18,    0,   18,    0,   10,  -16,
     -10,   16,    0,  -24,    0,   24,  -24,   -8,
      24,   -8,  -24,    8,   24,    8,   18,   18,
     -18,  -18,   20,  -16,  -20,   16,  -14,  -26,
      14,  -26,  -14,   26,   14,   26,  -30,    0,
      30,    0,    0,  -34,    0,   34,  -34,   -8,
      34,   -8,  -34,    8,   34,    8,  -30,  -18,
      30,  -18,  -30,   18,   30,   18,  -10,  -34,
      10,  -34,  -10,   34,   10,   34,  -20,  -34,
      20,   34,  -40,    0,   40,    0,   30,   30,
     -30,  -30,  -40,  -18,   40,   18,    0,  -44,
       0,   44,  -16,  -44,   16,   44,  -36,  -36,
     -36,  -36,   36,   36,  -26,  -44,   26,   44,
     -46,  -26,   46,   26,  -52,  -18,   52,   18,
     -20,  -54,  -44,  -44,   44,   44,  -32,  -54,
     -46,  -46,  -46,  -46,   46,   46,
]};

const DT_3_4: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    0,    4,    0,   -4,    4,    0,
      -4,    0,    4,    4,   -4,   -4,   12,   12,
     -12,  -12,    0,   12,    0,  -12,   12,    0,
     -12,    0,   -8,    8,    8,   -8,    8,  -16,
      -8,   16,    0,  -24,    0,   24,  -24,   -8,
      24,   -8,  -24,    8,   24,    8,   20,  -16,
     -20,   16,  -28,    0,   28,    0,  -16,  -24,
      16,  -24,  -16,   24,   16,   24,    0,  -32,
       0,   32,  -28,  -16,   28,  -16,  -28,   16,
      28,   16,   -8,  -32,    8,  -32,  -32,   -8,
      32,   -8,  -32,    8,   32,    8,   -8,   32,
       8,   32,   24,   24,  -24,  -24,   24,  -24,
     -24,   24,  -20,  -32,   20,   32,  -40,    0,
      40,    0,  -40,  -16,   40,   16,    0,  -44,
       0,  -44,  -44,    0,   44,    0,    0,   44,
       0,   44,  -32,  -32,   32,   32,  -16,  -44,
      16,   44,  -24,  -44,  -44,  -24,   44,   24,
      24,   44,  -48,  -16,   48,   16,  -36,  -36,
     -36,  -36,   36,   36,   36,   36,  -20,  -52,
      40,   40,  -40,  -40,  -32,  -52,
]};

const DT_3_5: IviDeltaCB = IviDeltaCB{ quad_radix: 13, data: &[
       0,    0,    2,    2,   -2,   -2,    6,    6,
      -6,   -6,   12,   12,  -12,  -12,   20,   20,
     -20,  -20,   32,   32,  -32,  -32,   46,   46,
     -46,  -46,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,
]};

const IVI3_DELTA_CBS: [&IviDeltaCB; 24] = [
    &DT_1_1, &DT_1_2, &DT_1_3, &DT_1_4, &DT_1_5, &DT_1_6, &DT_1_7, &DT_1_8,
    &DT_2_1, &DT_2_2, &DT_2_3, &DT_2_4, &DT_2_5, &DT_2_6, &DT_2_7, &DT_2_8,
    &DT_3_1, &DT_3_2, &DT_3_3, &DT_3_4, &DT_3_5, &DT_3_5, &DT_3_5, &DT_3_5
];
