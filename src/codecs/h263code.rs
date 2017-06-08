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

fn idct_row(row: &mut [i16]) {
    let in0 = ((row[0] as i32) << 11) + (1 << (ROW_SHIFT - 1));
    let in1 =  (row[4] as i32) << 11;
    let in2 =   row[6] as i32;
    let in3 =   row[2] as i32;
    let in4 =   row[1] as i32;
    let in5 =   row[7] as i32;
    let in6 =   row[5] as i32;
    let in7 =   row[3] as i32;

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

fn idct_col(blk: &mut [i16; 64], off: usize) {
    let in0 = ((blk[off + 0*8] as i32) << 8) + (1 << (COL_SHIFT - 1));
    let in1 =  (blk[off + 4*8] as i32) << 8;
    let in2 =   blk[off + 6*8] as i32;
    let in3 =   blk[off + 2*8] as i32;
    let in4 =   blk[off + 1*8] as i32;
    let in5 =   blk[off + 7*8] as i32;
    let in6 =   blk[off + 5*8] as i32;
    let in7 =   blk[off + 3*8] as i32;

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

pub const H263_INTERP_FUNCS: &[fn(&mut [u8], usize, &[u8], usize, usize, usize)] = &[
        h263_interp00, h263_interp01, h263_interp10, h263_interp11 ];
