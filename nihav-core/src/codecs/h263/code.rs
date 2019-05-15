use crate::frame::NAVideoBuffer;
use super::{BlockDSP, CBPInfo, MV};
use super::super::blockdsp;
//use super::h263data::*;

/*const W1: i32 = 22725;
const W2: i32 = 21407;
const W3: i32 = 19266;
const W4: i32 = 16383;
const W5: i32 = 12873;
const W6: i32 =  8867;
const W7: i32 =  4520;

const ROW_SHIFT: u8 = 11;
const COL_SHIFT: u8 = 20;

fn idct_row(row: &mut [i16]) {
    let in0 = row[0] as i32;
    let in1 = row[1] as i32;
    let in2 = row[2] as i32;
    let in3 = row[3] as i32;
    let in4 = row[4] as i32;
    let in5 = row[5] as i32;
    let in6 = row[6] as i32;
    let in7 = row[7] as i32;

    let mut a0 = in0 * W1 + (1 << (ROW_SHIFT - 1));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 += W2 * in2;
    a1 += W6 * in2;
    a2 -= W6 * in2;
    a3 -= W2 * in2;

    let mut b0 = W1 * in1 + W3 * in3;
    let mut b1 = W3 * in1 - W7 * in3;
    let mut b2 = W5 * in1 - W1 * in3;
    let mut b3 = W7 * in1 - W5 * in3;

    a0 += W4 * in4 + W6 * in6;
    a1 -= W4 * in4 + W2 * in6;
    a2 -= W4 * in4 - W2 * in6;
    a3 += W4 * in4 - W6 * in6;

    b0 += W5 * in5 + W7 * in7;
    b1 -= W1 * in5 + W5 * in7;
    b2 += W7 * in5 + W3 * in7;
    b3 += W3 * in5 - W1 * in7;

    row[0] = ((a0 + b0) >> ROW_SHIFT) as i16;
    row[7] = ((a0 - b0) >> ROW_SHIFT) as i16;
    row[1] = ((a1 + b1) >> ROW_SHIFT) as i16;
    row[6] = ((a1 - b1) >> ROW_SHIFT) as i16;
    row[2] = ((a2 + b2) >> ROW_SHIFT) as i16;
    row[5] = ((a2 - b2) >> ROW_SHIFT) as i16;
    row[3] = ((a3 + b3) >> ROW_SHIFT) as i16;
    row[4] = ((a3 - b3) >> ROW_SHIFT) as i16;
}

fn idct_col(blk: &mut [i16; 64], off: usize) {
    let in0 = blk[off + 0*8] as i32;
    let in1 = blk[off + 1*8] as i32;
    let in2 = blk[off + 2*8] as i32;
    let in3 = blk[off + 3*8] as i32;
    let in4 = blk[off + 4*8] as i32;
    let in5 = blk[off + 5*8] as i32;
    let in6 = blk[off + 6*8] as i32;
    let in7 = blk[off + 7*8] as i32;

    let mut a0 = in0 * W1 + (1 << (COL_SHIFT - 1));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 += W2 * in2;
    a1 += W6 * in2;
    a2 -= W6 * in2;
    a3 -= W2 * in2;

    let mut b0 = W1 * in1 + W3 * in3;
    let mut b1 = W3 * in1 - W7 * in3;
    let mut b2 = W5 * in1 - W1 * in3;
    let mut b3 = W7 * in1 - W5 * in3;

    a0 += W4 * in4 + W6 * in6;
    a1 -= W4 * in4 + W2 * in6;
    a2 -= W4 * in4 - W2 * in6;
    a3 += W4 * in4 - W6 * in6;

    b0 += W5 * in5 + W7 * in7;
    b1 -= W1 * in5 + W5 * in7;
    b2 += W7 * in5 + W3 * in7;
    b3 += W3 * in5 - W1 * in7;

    blk[off + 0*8] = ((a0 + b0) >> COL_SHIFT) as i16;
    blk[off + 7*8] = ((a0 - b0) >> COL_SHIFT) as i16;
    blk[off + 1*8] = ((a1 + b1) >> COL_SHIFT) as i16;
    blk[off + 6*8] = ((a1 - b1) >> COL_SHIFT) as i16;
    blk[off + 2*8] = ((a2 + b2) >> COL_SHIFT) as i16;
    blk[off + 5*8] = ((a2 - b2) >> COL_SHIFT) as i16;
    blk[off + 3*8] = ((a3 + b3) >> COL_SHIFT) as i16;
    blk[off + 4*8] = ((a3 - b3) >> COL_SHIFT) as i16;
}

#[allow(dead_code)]
pub fn h263_idct(blk: &mut [i16; 64]) {
    for i in 0..8 { idct_row(&mut blk[i*8..(i+1)*8]); }
    for i in 0..8 { idct_col(blk, i); }
}*/

const W1: i32 = 2841;
const W2: i32 = 2676;
const W3: i32 = 2408;
const W5: i32 = 1609;
const W6: i32 = 1108;
const W7: i32 =  565;
const W8: i32 =  181;

const ROW_SHIFT: u8 = 8;
const COL_SHIFT: u8 = 14;

#[allow(clippy::erasing_op)]
fn idct_row(row: &mut [i16]) {
    let in0 = ((i32::from(row[0])) << 11) + (1 << (ROW_SHIFT - 1));
    let in1 =  (i32::from(row[4])) << 11;
    let in2 =   i32::from(row[6]);
    let in3 =   i32::from(row[2]);
    let in4 =   i32::from(row[1]);
    let in5 =   i32::from(row[7]);
    let in6 =   i32::from(row[5]);
    let in7 =   i32::from(row[3]);

    let tmp = W7 * (in4 + in5);
    let a4 = tmp + (W1 - W7) * in4;
    let a5 = tmp - (W1 + W7) * in5;

    let tmp = W3 * (in6 + in7);
    let a6 = tmp - (W3 - W5) * in6;
    let a7 = tmp - (W3 + W5) * in7;

    let tmp = in0 + in1;

    let a0 = in0 - in1;
    let t1 = W6 * (in2 + in3);
    let a2 = t1 - (W2 + W6) * in2;
    let a3 = t1 + (W2 - W6) * in3;
    let b1 = a4 + a6;

    let b4 = a4 - a6;
    let t2 = a5 - a7;
    let b6 = a5 + a7;
    let b7 = tmp + a3;
    let b5 = tmp - a3;
    let b3 = a0 + a2;
    let b0 = a0 - a2;
    let b2 = (W8 * (b4 + t2) + 128) >> 8;
    let b4 = (W8 * (b4 - t2) + 128) >> 8;

    row[0] = ((b7 + b1) >> ROW_SHIFT) as i16;
    row[7] = ((b7 - b1) >> ROW_SHIFT) as i16;
    row[1] = ((b3 + b2) >> ROW_SHIFT) as i16;
    row[6] = ((b3 - b2) >> ROW_SHIFT) as i16;
    row[2] = ((b0 + b4) >> ROW_SHIFT) as i16;
    row[5] = ((b0 - b4) >> ROW_SHIFT) as i16;
    row[3] = ((b5 + b6) >> ROW_SHIFT) as i16;
    row[4] = ((b5 - b6) >> ROW_SHIFT) as i16;
}

#[allow(clippy::erasing_op)]
fn idct_col(blk: &mut [i16; 64], off: usize) {
    let in0 = ((i32::from(blk[off + 0*8])) << 8) + (1 << (COL_SHIFT - 1));
    let in1 =  (i32::from(blk[off + 4*8])) << 8;
    let in2 =   i32::from(blk[off + 6*8]);
    let in3 =   i32::from(blk[off + 2*8]);
    let in4 =   i32::from(blk[off + 1*8]);
    let in5 =   i32::from(blk[off + 7*8]);
    let in6 =   i32::from(blk[off + 5*8]);
    let in7 =   i32::from(blk[off + 3*8]);

    let tmp = W7 * (in4 + in5);
    let a4 = (tmp + (W1 - W7) * in4) >> 3;
    let a5 = (tmp - (W1 + W7) * in5) >> 3;

    let tmp = W3 * (in6 + in7);
    let a6 = (tmp - (W3 - W5) * in6) >> 3;
    let a7 = (tmp - (W3 + W5) * in7) >> 3;

    let tmp = in0 + in1;

    let a0 = in0 - in1;
    let t1 = W6 * (in2 + in3);
    let a2 = (t1 - (W2 + W6) * in2) >> 3;
    let a3 = (t1 + (W2 - W6) * in3) >> 3;
    let b1 = a4 + a6;

    let b4 = a4 - a6;
    let t2 = a5 - a7;
    let b6 = a5 + a7;
    let b7 = tmp + a3;
    let b5 = tmp - a3;
    let b3 = a0 + a2;
    let b0 = a0 - a2;
    let b2 = (W8 * (b4 + t2) + 128) >> 8;
    let b4 = (W8 * (b4 - t2) + 128) >> 8;

    blk[off + 0*8] = ((b7 + b1) >> COL_SHIFT) as i16;
    blk[off + 7*8] = ((b7 - b1) >> COL_SHIFT) as i16;
    blk[off + 1*8] = ((b3 + b2) >> COL_SHIFT) as i16;
    blk[off + 6*8] = ((b3 - b2) >> COL_SHIFT) as i16;
    blk[off + 2*8] = ((b0 + b4) >> COL_SHIFT) as i16;
    blk[off + 5*8] = ((b0 - b4) >> COL_SHIFT) as i16;
    blk[off + 3*8] = ((b5 + b6) >> COL_SHIFT) as i16;
    blk[off + 4*8] = ((b5 - b6) >> COL_SHIFT) as i16;
}

#[allow(dead_code)]
pub fn h263_idct(blk: &mut [i16; 64]) {
    for i in 0..8 { idct_row(&mut blk[i*8..(i+1)*8]); }
    for i in 0..8 { idct_col(blk, i); }
}

fn h263_interp00(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = src[sidx + x]; }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp01(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = (((src[sidx + x] as u16) + (src[sidx + x + 1] as u16) + 1) >> 1) as u8; }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp10(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = (((src[sidx + x] as u16) + (src[sidx + x + sstride] as u16) + 1) >> 1) as u8; }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp11(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            dst[didx + x] = (((src[sidx + x] as u16) +
                              (src[sidx + x + 1] as u16) +
                              (src[sidx + x + sstride] as u16) +
                              (src[sidx + x + sstride + 1] as u16) + 2) >> 2) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

pub const H263_INTERP_FUNCS: &[blockdsp::BlkInterpFunc] = &[
        h263_interp00, h263_interp01, h263_interp10, h263_interp11 ];

fn h263_interp00_avg(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            let a = dst[didx + x] as u16;
            let b = src[sidx + x] as u16;
            dst[didx + x] = ((a + b + 1) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp01_avg(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            let a = dst[didx + x] as u16;
            let b = ((src[sidx + x] as u16) + (src[sidx + x + 1] as u16) + 1) >> 1;
            dst[didx + x] = ((a + b + 1) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp10_avg(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            let a = dst[didx + x] as u16;
            let b = ((src[sidx + x] as u16) + (src[sidx + x + sstride] as u16) + 1) >> 1;
            dst[didx + x] = ((a + b + 1) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

fn h263_interp11_avg(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            let a = dst[didx + x] as u16;
            let b = ((src[sidx + x] as u16) +
                     (src[sidx + x + 1] as u16) +
                     (src[sidx + x + sstride] as u16) +
                     (src[sidx + x + sstride + 1] as u16) + 2) >> 2;
            dst[didx + x] = ((a + b + 1) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

pub const H263_INTERP_AVG_FUNCS: &[blockdsp::BlkInterpFunc] = &[
        h263_interp00_avg, h263_interp01_avg, h263_interp10_avg, h263_interp11_avg ];

pub struct H263BlockDSP { }

impl H263BlockDSP {
    pub fn new() -> Self {
        H263BlockDSP { }
    }
}

#[allow(clippy::erasing_op)]
fn deblock_hor(buf: &mut NAVideoBuffer<u8>, comp: usize, q: u8, off: usize) {
    let stride = buf.get_stride(comp);
    let dptr = buf.get_data_mut().unwrap();
    let buf = dptr.as_mut_slice();
    for x in 0..8 {
        let a = buf[off - 2 * stride + x] as i16;
        let b = buf[off - 1 * stride + x] as i16;
        let c = buf[off + 0 * stride + x] as i16;
        let d = buf[off + 1 * stride + x] as i16;
        let diff = ((a - d) * 3 + (c - b) * 8) >> 4;
        if (diff != 0) && (diff >= -32) && (diff < 32) {
            let d0 = diff.abs() * 2 - (q as i16);
            let d1 = if d0 < 0 { 0 } else { d0 };
            let d2 = diff.abs() - d1;
            let d3 = if d2 < 0 { 0 } else { d2 };

            let delta = if diff < 0 { -d3 } else { d3 };

            let b1 = b + delta;
            if      b1 < 0   { buf[off - 1 * stride + x] = 0; }
            else if b1 > 255 { buf[off - 1 * stride + x] = 0xFF; }
            else             { buf[off - 1 * stride + x] = b1 as u8; }
            let c1 = c - delta;
            if      c1 < 0   { buf[off + x] = 0; }
            else if c1 > 255 { buf[off + x] = 0xFF; }
            else             { buf[off + x] = c1 as u8; }
        }
    }
}

fn deblock_ver(buf: &mut NAVideoBuffer<u8>, comp: usize, q: u8, off: usize) {
    let stride = buf.get_stride(comp);
    let dptr = buf.get_data_mut().unwrap();
    let buf = dptr.as_mut_slice();
    for y in 0..8 {
        let a = buf[off - 2 + y * stride] as i16;
        let b = buf[off - 1 + y * stride] as i16;
        let c = buf[off + 0 + y * stride] as i16;
        let d = buf[off + 1 + y * stride] as i16;
        let diff = ((a - d) * 3 + (c - b) * 8) >> 4;
        if (diff != 0) && (diff >= -32) && (diff < 32) {
            let d0 = diff.abs() * 2 - (q as i16);
            let d1 = if d0 < 0 { 0 } else { d0 };
            let d2 = diff.abs() - d1;
            let d3 = if d2 < 0 { 0 } else { d2 };

            let delta = if diff < 0 { -d3 } else { d3 };

            let b1 = b + delta;
            if      b1 < 0   { buf[off - 1 + y * stride] = 0; }
            else if b1 > 255 { buf[off - 1 + y * stride] = 0xFF; }
            else             { buf[off - 1 + y * stride] = b1 as u8; }
            let c1 = c - delta;
            if      c1 < 0   { buf[off + y * stride] = 0; }
            else if c1 > 255 { buf[off + y * stride] = 0xFF; }
            else             { buf[off + y * stride] = c1 as u8; }
        }
    }
}

pub fn h263_filter_row(buf: &mut NAVideoBuffer<u8>, mb_y: usize, mb_w: usize, cbpi: &CBPInfo) {
    let stride  = buf.get_stride(0);
    let mut off = buf.get_offset(0) + mb_y * 16 * stride;
    for mb_x in 0..mb_w {
        let coff = off;
        let coded0 = cbpi.is_coded(mb_x, 0);
        let coded1 = cbpi.is_coded(mb_x, 1);
        let q = cbpi.get_q(mb_w + mb_x);
        if mb_y != 0 {
	        if coded0 && cbpi.is_coded_top(mb_x, 0) { deblock_hor(buf, 0, q, coff); }
	        if coded1 && cbpi.is_coded_top(mb_x, 1) { deblock_hor(buf, 0, q, coff + 8); }
        }
        let coff = off + 8 * stride;
        if cbpi.is_coded(mb_x, 2) && coded0 { deblock_hor(buf, 0, q, coff); }
        if cbpi.is_coded(mb_x, 3) && coded1 { deblock_hor(buf, 0, q, coff + 8); }
        off += 16;
    }
    let mut leftt = false;
    let mut leftc = false;
    let mut off = buf.get_offset(0) + mb_y * 16 * stride;
    for mb_x in 0..mb_w {
        let ctop0 = cbpi.is_coded_top(mb_x, 0);
        let ctop1 = cbpi.is_coded_top(mb_x, 0);
        let ccur0 = cbpi.is_coded(mb_x, 0);
        let ccur1 = cbpi.is_coded(mb_x, 1);
        let q = cbpi.get_q(mb_w + mb_x);
        if mb_y != 0 {
	        let coff = off - 8 * stride;
	        let qtop = cbpi.get_q(mb_x);
	        if leftt && ctop0 { deblock_ver(buf, 0, qtop, coff); }
        	if ctop0 && ctop1 { deblock_ver(buf, 0, qtop, coff + 8); }
        }
        if leftc && ccur0 { deblock_ver(buf, 0, q, off); }
        if ccur0 && ccur1 { deblock_ver(buf, 0, q, off + 8); }
        leftt = ctop1;
        leftc = ccur1;
        off += 16;
    }
    let strideu = buf.get_stride(1);
    let stridev = buf.get_stride(2);
    let offu = buf.get_offset(1) + mb_y * 8 * strideu;
    let offv = buf.get_offset(2) + mb_y * 8 * stridev;
    if mb_y != 0 {
        for mb_x in 0..mb_w {
	        let ctu = cbpi.is_coded_top(mb_x, 4);
        	let ccu = cbpi.is_coded(mb_x, 4);
        	let ctv = cbpi.is_coded_top(mb_x, 5);
        	let ccv = cbpi.is_coded(mb_x, 5);
        	let q = cbpi.get_q(mb_w + mb_x);
        	if ctu && ccu { deblock_hor(buf, 1, q, offu + mb_x * 8); }
        	if ctv && ccv { deblock_hor(buf, 2, q, offv + mb_x * 8); }
        }
        let mut leftu = false;
        let mut leftv = false;
        let offu = buf.get_offset(1) + (mb_y - 1) * 8 * strideu;
        let offv = buf.get_offset(2) + (mb_y - 1) * 8 * stridev;
        for mb_x in 0..mb_w {
	        let ctu = cbpi.is_coded_top(mb_x, 4);
	        let ctv = cbpi.is_coded_top(mb_x, 5);
	        let qt = cbpi.get_q(mb_x);
        	if leftu && ctu { deblock_ver(buf, 1, qt, offu + mb_x * 8); }
        	if leftv && ctv { deblock_ver(buf, 2, qt, offv + mb_x * 8); }
        	leftu = ctu;
        	leftv = ctv;
        }
    }
}

impl BlockDSP for H263BlockDSP {
    fn idct(&self, blk: &mut [i16; 64]) {
        h263_idct(blk)
    }
    fn copy_blocks(&self, dst: &mut NAVideoBuffer<u8>, src: &NAVideoBuffer<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV) {
        let srcx = ((mv.x >> 1) as isize) + (xpos as isize);
        let srcy = ((mv.y >> 1) as isize) + (ypos as isize);
        let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;

        blockdsp::copy_blocks(dst, src, xpos, ypos, srcx, srcy, w, h, 0, 1, mode, H263_INTERP_FUNCS);
    }
    fn avg_blocks(&self, dst: &mut NAVideoBuffer<u8>, src: &NAVideoBuffer<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV) {
        let srcx = ((mv.x >> 1) as isize) + (xpos as isize);
        let srcy = ((mv.y >> 1) as isize) + (ypos as isize);
        let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;

        blockdsp::copy_blocks(dst, src, xpos, ypos, srcx, srcy, w, h, 0, 1, mode, H263_INTERP_AVG_FUNCS);
    }
    fn filter_row(&self, buf: &mut NAVideoBuffer<u8>, mb_y: usize, mb_w: usize, cbpi: &CBPInfo) {
        h263_filter_row(buf, mb_y, mb_w, cbpi)
    }
}
