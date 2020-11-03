use nihav_core::frame::*;
use nihav_codec_support::codecs::blockdsp::*;
use nihav_codec_support::codecs::MV;

pub const CHROMA_QUANTS: [u8; 52] = [
     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 29, 30,
    31, 32, 32, 33, 34, 34, 35, 35, 36, 36, 37, 37, 37, 38, 38, 38,
    39, 39, 39, 39
];

pub const CHROMA_DC_SCAN: [usize; 4] = [ 0, 1, 2, 3];
pub const ZIGZAG: [usize; 16] = [
    0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15
];
pub const ZIGZAG1: [usize; 15] = [
    0, 3, 7, 4, 1, 2, 5, 8, 11, 12, 9, 6, 10, 13, 14
];
/*pub const IL_SCAN: [usize; 16] = [
    0, 4, 1, 8, 12, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15
];*/
pub const ZIGZAG8X8: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63
];

const LEVEL_SCALE: [[i16; 6]; 3] = [
    [ 10, 11, 13, 14, 16, 18 ],
    [ 16, 18, 20, 23, 25, 29 ],
    [ 13, 14, 16, 18, 20, 23 ]
];

pub fn chroma_dc_transform(blk: &mut [i16; 4], qp: u8) {
    let t0 = blk[0] + blk[2];
    let t1 = blk[0] - blk[2];
    let t2 = blk[1] + blk[3];
    let t3 = blk[1] - blk[3];
    blk[0] = t0 + t2;
    blk[1] = t0 - t2;
    blk[2] = t1 + t3;
    blk[3] = t1 - t3;
    if qp < 6 {
        let mul = i16::from(LEVEL_SCALE[0][qp as usize]);
        for el in blk.iter_mut() {
            *el = el.wrapping_mul(mul) >> 1;
        }
    } else {
        let mul = i16::from(LEVEL_SCALE[0][(qp % 6) as usize]);
        let shift = qp / 6 - 1;
        for el in blk.iter_mut() {
            *el = el.wrapping_mul(mul) << shift;
        }
    }
}

macro_rules! transform {
    (luma_dc; $a: expr, $b: expr, $c: expr, $d: expr) => ({
        let t0 = $a.wrapping_add($c);
        let t1 = $a.wrapping_sub($c);
        let t2 = $b.wrapping_add($d);
        let t3 = $b.wrapping_sub($d);
        $a = t0.wrapping_add(t2);
        $b = t1.wrapping_add(t3);
        $c = t1.wrapping_sub(t3);
        $d = t0.wrapping_sub(t2);
    });
    ($a: expr, $b: expr, $c: expr, $d: expr, $shift: expr) => ({
        let t0 = $a.wrapping_add($c);
        let t1 = $a.wrapping_sub($c);
        let t2 = ($b >> 1).wrapping_sub($d);
        let t3 = $b.wrapping_add($d >> 1);
        let bias = 1 << $shift >> 1;
        $a = t0.wrapping_add(t3).wrapping_add(bias) >> $shift;
        $b = t1.wrapping_add(t2).wrapping_add(bias) >> $shift;
        $c = t1.wrapping_sub(t2).wrapping_add(bias) >> $shift;
        $d = t0.wrapping_sub(t3).wrapping_add(bias) >> $shift;
    });
    ($a: expr, $b: expr, $c: expr, $d: expr, $e: expr, $f: expr, $g: expr, $h: expr) => {
        let e0 = $a + $e;
        let e1 = -$d + $f - $h - ($h >> 1);
        let e2 = $a - $e;
        let e3 = $b + $h - $d - ($d >> 1);
        let e4 = ($c >> 1) - $g;
        let e5 = -$b + $h + $f + ($f >> 1);
        let e6 = $c + ($g >> 1);
        let e7 = $d + $f + $b + ($b >> 1);

        let f0 = e0 + e6;
        let f1 = e1 + (e7 >> 2);
        let f2 = e2 + e4;
        let f3 = e3 + (e5 >> 2);
        let f4 = e2 - e4;
        let f5 = (e3 >> 2) - e5;
        let f6 = e0 - e6;
        let f7 = e7 - (e1 >> 2);

        $a = f0 + f7;
        $b = f2 + f5;
        $c = f4 + f3;
        $d = f6 + f1;
        $e = f6 - f1;
        $f = f4 - f3;
        $g = f2 - f5;
        $h = f0 - f7;
    };
}

pub fn idct_luma_dc(blk: &mut [i16; 16], qp: u8) {
    if qp < 12 {
        let mul = i16::from(LEVEL_SCALE[0][(qp % 6) as usize]);
        let shift = 2 - qp / 6;
        let bias = 1 << shift >> 1;
        for el in blk.iter_mut() {
            *el = el.wrapping_mul(mul).wrapping_add(bias) >> shift;
        }
    } else {
        let mul = i16::from(LEVEL_SCALE[0][(qp % 6) as usize]);
        let shift = qp / 6 - 2;
        for el in blk.iter_mut() {
            *el = el.wrapping_mul(mul) << shift;
        }
    }
    for i in 0..4 {
        transform!(luma_dc; blk[i], blk[i + 4], blk[i + 8], blk[i + 12]);
    }
    for row in blk.chunks_mut(4) {
        transform!(luma_dc; row[0], row[1], row[2], row[3]);
    }
}

pub fn idct(blk: &mut [i16; 16], qp: u8, quant_dc: bool) {
    const BLK_INDEX: [usize; 16] = [
        0, 2, 0, 2,
        2, 1, 2, 1,
        0, 2, 0, 2,
        2, 1, 2, 1
    ];
    let qidx = (qp % 6) as usize;
    let shift = qp / 6;
    let start = if quant_dc { 0 } else { 1 };
    for (el, &idx) in blk.iter_mut().zip(BLK_INDEX.iter()).skip(start) {
        *el = (*el * LEVEL_SCALE[idx][qidx]) << shift;
    }
    for i in 0..4 {
        transform!(blk[i], blk[i + 4], blk[i + 8], blk[i + 12], 0);
    }
    for row in blk.chunks_mut(4) {
        transform!(row[0], row[1], row[2], row[3], 6);
    }
}

pub fn idct_dc(blk: &mut [i16; 16], qp: u8, quant_dc: bool) {
    let dc = if quant_dc {
            (blk[0] * LEVEL_SCALE[0][(qp % 6) as usize]) << (qp / 6)
        } else {
            blk[0]
        };
    *blk  = [(dc + 0x20) >> 6; 16];
}

const QMAT_8X8: [[u8; 16]; 6] = [
  [
    20, 19, 25, 24,
    19, 18, 24, 18,
    25, 24, 32, 24,
    24, 18, 24, 18
  ], [
    22, 21, 28, 26,
    21, 19, 26, 19,
    28, 26, 35, 26,
    26, 19, 26, 19
  ], [
    26, 24, 33, 31,
    24, 23, 31, 23,
    33, 31, 42, 31,
    31, 23, 31, 23
  ], [
    28, 26, 35, 33,
    26, 25, 33, 25,
    35, 33, 45, 33,
    33, 25, 33, 25
  ], [
    32, 30, 40, 38,
    30, 28, 38, 28,
    40, 38, 51, 38,
    38, 28, 38, 28
  ], [
    36, 34, 46, 43,
    34, 32, 43, 32,
    46, 43, 58, 43,
    43, 32, 43, 32
  ]
];

pub fn dequant8x8(blk: &mut [i16; 64], slist: &[u8; 64]) {
    for (el, &scan) in blk.iter_mut().zip(ZIGZAG8X8.iter()) {
        if *el != 0 {
            *el = el.wrapping_mul(i16::from(slist[scan]));
        }
    }
}

pub fn idct8x8(blk: &mut [i16; 64], qp: u8) {
    let mut tmp = [0i32; 64];
    let qmat = &QMAT_8X8[(qp % 6) as usize];
    if qp >= 36 {
        let shift = qp / 6 - 6;
        for (i, (dst, &src)) in tmp.iter_mut().zip(blk.iter()).enumerate() {
            let x = i & 7;
            let y = i >> 3;
            let idx = (x & 3) + (y & 3) * 4;
            *dst = i32::from(src).wrapping_mul(i32::from(qmat[idx])) << shift;
        }
    } else {
        let shift = 6 - qp / 6;
        let bias = (1 << shift) >> 1;
        for (i, (dst, &src)) in tmp.iter_mut().zip(blk.iter()).enumerate() {
            let x = i & 7;
            let y = i >> 3;
            let idx = (x & 3) + (y & 3) * 4;
            *dst = i32::from(src).wrapping_mul(i32::from(qmat[idx])).wrapping_add(bias) >> shift;
        }
    }
    for row in tmp.chunks_mut(8) {
        transform!(row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7]);
    }
    for col in 0..8 {
        transform!(tmp[col], tmp[col + 8], tmp[col + 8 * 2], tmp[col + 8 * 3],
                   tmp[col + 8 * 4], tmp[col + 8 * 5], tmp[col + 8 * 6], tmp[col + 8 * 7]);
    }
    for (dst, &src) in blk.iter_mut().zip(tmp.iter()) {
        *dst = ((src + 0x20) >> 6) as i16;
    }
}

pub fn add_coeffs(dst: &mut [u8], offset: usize, stride: usize, coeffs: &[i16]) {
    let out = &mut dst[offset..][..stride * 3 + 4];
    for (line, src) in out.chunks_mut(stride).take(4).zip(coeffs.chunks(4)) {
        for (dst, src) in line.iter_mut().take(4).zip(src.iter()) {
            *dst = (i32::from(*dst) + i32::from(*src)).max(0).min(255) as u8;
        }
    }
}

pub fn add_coeffs8(dst: &mut [u8], offset: usize, stride: usize, coeffs: &[i16; 64]) {
    let out = &mut dst[offset..];
    for (line, src) in out.chunks_mut(stride).take(8).zip(coeffs.chunks(8)) {
        for (dst, src) in line.iter_mut().take(8).zip(src.iter()) {
            *dst = (i32::from(*dst) + i32::from(*src)).max(0).min(255) as u8;
        }
    }
}

pub fn avg(dst: &mut [u8], dstride: usize,
           src: &[u8], sstride: usize, bw: usize, bh: usize) {
   for (dline, sline) in dst.chunks_mut(dstride).zip(src.chunks(sstride)).take(bh) {
        for (dst, src) in dline.iter_mut().zip(sline.iter()).take(bw) {
            *dst = ((u16::from(*dst) + u16::from(*src) + 1) >> 1) as u8;
        }
    }
}

fn clip8(val: i16) -> u8 { val.max(0).min(255) as u8 }

fn ipred_dc128(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize) {
    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = 128; }
        idx += stride;
    }
}
fn ipred_ver(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize) {
    let oidx = idx - stride;
    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = buf[oidx + x]; }
        idx += stride;
    }
}
fn ipred_hor(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize) {
    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = buf[idx - 1]; }
        idx += stride;
    }
}
fn ipred_dc(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize, shift: u8) {
    let mut adc: u16 = 0;
    for i in 0..bsize { adc += u16::from(buf[idx - stride + i]); }
    for i in 0..bsize { adc += u16::from(buf[idx - 1 + i * stride]); }
    let dc = ((adc + (1 << (shift - 1))) >> shift) as u8;

    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = dc; }
        idx += stride;
    }
}
fn ipred_left_dc(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize, shift: u8) {
    let mut adc: u16 = 0;
    for i in 0..bsize { adc += u16::from(buf[idx - 1 + i * stride]); }
    let dc = ((adc + (1 << (shift - 1))) >> shift) as u8;

    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = dc; }
        idx += stride;
    }
}
fn ipred_top_dc(buf: &mut [u8], mut idx: usize, stride: usize, bsize: usize, shift: u8) {
    let mut adc: u16 = 0;
    for i in 0..bsize { adc += u16::from(buf[idx - stride + i]); }
    let dc = ((adc + (1 << (shift - 1))) >> shift) as u8;

    for _ in 0..bsize {
        for x in 0..bsize { buf[idx + x] = dc; }
        idx += stride;
    }
}

fn load_top(dst: &mut [u16], buf: &mut [u8], idx: usize, stride: usize, len: usize) {
    for i in 0..len { dst[i] = u16::from(buf[idx - stride + i]); }
}
fn load_left(dst: &mut [u16], buf: &mut [u8], idx: usize, stride: usize, len: usize) {
    for i in 0..len { dst[i] = u16::from(buf[idx - 1 + i * stride]); }
}

fn ipred_4x4_ver(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_ver(buf, idx, stride, 4);
}
fn ipred_4x4_hor(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_hor(buf, idx, stride, 4);
}
fn ipred_4x4_diag_down_left(buf: &mut [u8], idx: usize, stride: usize, tr: &[u8]) {
    let mut t: [u16; 9] = [0; 9];
    load_top(&mut t, buf, idx, stride, 4);
    for i in 0..4 {
        t[i + 4] = u16::from(tr[i]);
    }
    t[8] = t[7];

    let dst = &mut buf[idx..];
    for i in 0..4 {
        dst[i] = ((t[i]     + 2 * t[i + 1] + t[i + 2] + 2) >> 2) as u8;
    }
    let dst = &mut buf[idx + stride..];
    for i in 0..4 {
        dst[i] = ((t[i + 1] + 2 * t[i + 2] + t[i + 3] + 2) >> 2) as u8;
    }
    let dst = &mut buf[idx + stride * 2..];
    for i in 0..4 {
        dst[i] = ((t[i + 2] + 2 * t[i + 3] + t[i + 4] + 2) >> 2) as u8;
    }
    let dst = &mut buf[idx + stride * 3..];
    for i in 0..4 {
        dst[i] = ((t[i + 3] + 2 * t[i + 4] + t[i + 5] + 2) >> 2) as u8;
    }
}
fn ipred_4x4_diag_down_right(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    let mut t: [u16; 5] = [0; 5];
    let mut l: [u16; 5] = [0; 5];
    load_top(&mut t, buf, idx - 1, stride, 5);
    load_left(&mut l, buf, idx - stride, stride, 5);
    let dst = &mut buf[idx..];

    for j in 0..4 {
        for i in 0..j {
            dst[i + j * stride] = ((l[j - i - 1] + 2 * l[j - i] + l[j - i + 1] + 2) >> 2) as u8;
        }
        dst[j + j * stride] = ((l[1] + 2 * l[0] + t[1] + 2) >> 2) as u8;
        for i in (j+1)..4 {
            dst[i + j * stride] = ((t[i - j - 1] + 2 * t[i - j] + t[i - j + 1] + 2) >> 2) as u8;
        }
    }
}
fn ipred_4x4_ver_right(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    let mut t: [u16; 5] = [0; 5];
    let mut l: [u16; 5] = [0; 5];
    load_top(&mut t, buf, idx - 1, stride, 5);
    load_left(&mut l, buf, idx - stride, stride, 5);
    let dst = &mut buf[idx..];

    for j in 0..4 {
        for i in 0..4 {
            let zvr = ((2 * i) as i8) - (j as i8);
            let pix;
            if zvr >= 0 {
                if (zvr & 1) == 0 {
                    pix = (t[i - (j >> 1)] + t[i - (j >> 1) + 1] + 1) >> 1;
                } else {
                    pix = (t[i - (j >> 1) - 1] + 2 * t[i - (j >> 1)] + t[i - (j >> 1) + 1] + 2) >> 2;
                }
            } else {
                if zvr == -1 {
                    pix = (l[1] + 2 * l[0] + t[1] + 2) >> 2;
                } else {
                    pix = (l[j] + 2 * l[j - 1] + l[j - 2] + 2) >> 2;
                }
            }
            dst[i + j * stride] = pix as u8;
        }
    }
}
fn ipred_4x4_ver_left(buf: &mut [u8], idx: usize, stride: usize, tr: &[u8]) {
    let mut t: [u16; 8] = [0; 8];
    load_top(&mut t, buf, idx, stride, 4);
    for i in 0..4 { t[i + 4] = u16::from(tr[i]); }
    let dst = &mut buf[idx..];

    dst[0 + 0 * stride] = ((t[0] + t[1] + 1) >> 1) as u8;
    let pix = ((t[1] + t[2] + 1) >> 1) as u8;
    dst[1 + 0 * stride] = pix;
    dst[0 + 2 * stride] = pix;
    let pix = ((t[2] + t[3] + 1) >> 1) as u8;
    dst[2 + 0 * stride] = pix;
    dst[1 + 2 * stride] = pix;
    let pix = ((t[3] + t[4] + 1) >> 1) as u8;
    dst[3 + 0 * stride] = pix;
    dst[2 + 2 * stride] = pix;
    dst[3 + 2 * stride] = ((t[4] + t[5] + 1) >> 1) as u8;
    dst[0 + 1 * stride] = ((t[0] + 2*t[1] + t[2] + 2) >> 2) as u8;
    let pix = ((t[1] + 2*t[2] + t[3] + 2) >> 2) as u8;
    dst[1 + 1 * stride] = pix;
    dst[0 + 3 * stride] = pix;
    let pix = ((t[2] + 2*t[3] + t[4] + 2) >> 2) as u8;
    dst[2 + 1 * stride] = pix;
    dst[1 + 3 * stride] = pix;
    let pix = ((t[3] + 2*t[4] + t[5] + 2) >> 2) as u8;
    dst[3 + 1 * stride] = pix;
    dst[2 + 3 * stride] = pix;
    dst[3 + 3 * stride] = ((t[4] + 2*t[5] + t[6] + 2) >> 2) as u8;
}
fn ipred_4x4_hor_down(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    let mut t: [u16; 5] = [0; 5];
    let mut l: [u16; 5] = [0; 5];
    load_top(&mut t, buf, idx - 1, stride, 5);
    load_left(&mut l, buf, idx - stride, stride, 5);
    let dst = &mut buf[idx..];

    for j in 0..4 {
        for i in 0..4 {
            let zhd = ((2 * j) as i8) - (i as i8);
            let pix;
            if zhd >= 0 {
                if (zhd & 1) == 0 {
                    pix = (l[j - (i >> 1)] + l[j - (i >> 1) + 1] + 1) >> 1;
                } else {
                    pix = (l[j - (i >> 1) - 1] + 2 * l[j - (i >> 1)] + l[j - (i >> 1) + 1] + 2) >> 2;
                }
            } else {
                if zhd == -1 {
                    pix = (l[1] + 2 * l[0] + t[1] + 2) >> 2;
                } else {
                    pix = (t[i - 2] + 2 * t[i - 1] + t[i] + 2) >> 2;
                }
            }
            dst[i + j * stride] = pix as u8;
        }
    }
}
fn ipred_4x4_hor_up(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    let mut l: [u16; 8] = [0; 8];
    load_left(&mut l, buf, idx, stride, 8);
    let dst = &mut buf[idx..];

    dst[0 + 0 * stride] = ((l[0] + l[1] + 1) >> 1) as u8;
    dst[1 + 0 * stride] = ((l[0] + 2*l[1] + l[2] + 2) >> 2) as u8;
    let pix = ((l[1] + l[2] + 1) >> 1) as u8;
    dst[2 + 0 * stride] = pix;
    dst[0 + 1 * stride] = pix;
    let pix = ((l[1] + 2*l[2] + l[3] + 2) >> 2) as u8;
    dst[3 + 0 * stride] = pix;
    dst[1 + 1 * stride] = pix;
    let pix = ((l[2] + l[3] + 1) >> 1) as u8;
    dst[2 + 1 * stride] = pix;
    dst[0 + 2 * stride] = pix;
    let pix = ((l[2] + 3*l[3] + 2) >> 2) as u8;
    dst[3 + 1 * stride] = pix;
    dst[1 + 2 * stride] = pix;
    dst[3 + 2 * stride] = l[3] as u8;
    dst[1 + 3 * stride] = l[3] as u8;
    dst[0 + 3 * stride] = l[3] as u8;
    dst[2 + 2 * stride] = l[3] as u8;
    dst[2 + 3 * stride] = l[3] as u8;
    dst[3 + 3 * stride] = l[3] as u8;
}
fn ipred_4x4_dc(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_dc(buf, idx, stride, 4, 3);
}
fn ipred_4x4_left_dc(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_left_dc(buf, idx, stride, 4, 2);
}
fn ipred_4x4_top_dc(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_top_dc(buf, idx, stride, 4, 2);
}
fn ipred_4x4_dc128(buf: &mut [u8], idx: usize, stride: usize, _tr: &[u8]) {
    ipred_dc128(buf, idx, stride, 4);
}

pub struct IPred8Context {
    pub t:      [u8; 16],
    pub l:      [u8; 8],
    pub tl:     u8,
}

impl IPred8Context {
    pub fn new() -> Self {
        Self {
            t:      [128; 16],
            l:      [128; 8],
            tl:     128,
        }
    }
    pub fn fill(&mut self, buf: &mut [u8], idx: usize, stride: usize, has_t: bool, has_tr: bool, has_l: bool, has_tl: bool) {
        let mut t = [0x80u8; 19];
        let mut l = [0x80u8; 11];
        if has_t {
            t[1..8 + 1].copy_from_slice(&buf[idx - stride..][..8]);
        }
        if has_tr {
            t[8 + 1..16 + 1].copy_from_slice(&buf[idx - stride + 8..][..8]);
            t[16 + 1] = t[15 + 1];
            t[17 + 1] = t[15 + 1];
        } else {
            let (t0, t1) = t.split_at_mut(8 + 1);
            for el in t1.iter_mut() {
                *el = t0[7 + 1];
            }
        }
        if has_l {
            for i in 0..8 {
                l[i + 1] = buf[idx - 1 + stride * i];
            }
            l[8 + 1] = l[7 + 1];
            l[9 + 1] = l[7 + 1];
        }
        if has_tl {
            t[0] = buf[idx - 1 - stride];
            l[0] = buf[idx - 1 - stride];
        } else {
            t[0] = t[1];
            l[0] = l[1];
        }

        for i in 0..16 {
            self.t[i] = ((u16::from(t[i]) + 2 * u16::from(t[i + 1]) + u16::from(t[i + 2]) + 2) >> 2) as u8;
        }
        for i in 0..8 {
            self.l[i] = ((u16::from(l[i]) + 2 * u16::from(l[i + 1]) + u16::from(l[i + 2]) + 2) >> 2) as u8;
        }
        self.tl = if has_t && has_l {
                ((u16::from(t[1]) + 2 * u16::from(t[0]) + u16::from(l[1]) + 2) >> 2) as u8
            } else if has_t {
                ((3 * u16::from(t[0]) + u16::from(t[1]) + 2) >> 2) as u8
            } else if has_l {
                ((3 * u16::from(l[0]) + u16::from(l[1]) + 2) >> 2) as u8
            } else {
                t[0]
            };
    }
}

fn ipred_y_8x8_ver(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    for row in buf.chunks_mut(stride).take(8) {
        row[..8].copy_from_slice(&ctx.t[..8]);
    }
}
fn ipred_y_8x8_hor(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    for (row, &l) in buf.chunks_mut(stride).zip(ctx.l.iter()).take(8) {
        row[..8].copy_from_slice(&[l; 8]);
    }
}
fn ipred_y_8x8_diag_down_left(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut t = [0u16; 16];
    for (dt, &st) in t.iter_mut().zip(ctx.t.iter()) {
        *dt = u16::from(st);
    }

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            *pix = ((if (x != 7) || (y != 7) {
                    t[x + y] + 2 * t[x + y + 1] + t[x + y + 2]
                } else {
                    t[14] + 3 * t[15]
                } + 2) >> 2) as u8;
        }
    }
}
fn ipred_y_8x8_diag_down_right(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut t = [0u16; 9];
    t[0] = u16::from(ctx.tl);
    for (dt, &st) in t[1..].iter_mut().zip(ctx.t.iter()) {
        *dt = u16::from(st);
    }
    let mut l = [0u16; 9];
    l[0] = u16::from(ctx.tl);
    for (dl, &sl) in l[1..].iter_mut().zip(ctx.l.iter()) {
        *dl = u16::from(sl);
    }
    let diag = t[1] + 2 * t[0] + l[1];

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            *pix = ((if x > y {
                    t[x - y - 1] + 2 * t[x - y] + t[x - y + 1]
                } else if x < y {
                    l[y - x - 1] + 2 * l[y - x] + l[y - x + 1]
                } else {
                    diag
                } + 2) >> 2) as u8;
        }
    }
}
fn ipred_y_8x8_ver_right(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut t = [0u16; 9];
    t[0] = u16::from(ctx.tl);
    for (dt, &st) in t[1..].iter_mut().zip(ctx.t.iter()) {
        *dt = u16::from(st);
    }
    let mut l = [0u16; 9];
    l[0] = u16::from(ctx.tl);
    for (dl, &sl) in l[1..].iter_mut().zip(ctx.l.iter()) {
        *dl = u16::from(sl);
    }

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            let zvr = 2 * (x as i8) - (y as i8);
            *pix = if zvr >= 0 {
                    let ix = x - (y >> 1);
                    if (zvr & 1) == 0 {
                        (t[ix] + t[ix + 1] + 1) >> 1
                    } else {
                        (t[ix - 1] + 2 * t[ix] + t[ix + 1] + 2) >> 2
                    }
                } else if zvr == -1 {
                    (l[1] + 2 * l[0] + t[0] + 2) >> 2
                } else {
                    let ix = y - 2 * x;
                    (l[ix] + 2 * l[ix - 1] + l[ix - 2] + 2) >> 2
                } as u8;
        }
    }
}
fn ipred_y_8x8_ver_left(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut t = [0u16; 16];
    for (dt, &st) in t.iter_mut().zip(ctx.t.iter()) {
        *dt = u16::from(st);
    }

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            let ix = x + (y >> 1);
            *pix = if (y & 1) == 0 {
                    (t[ix] + t[ix + 1] + 1) >> 1
                } else {
                    (t[ix] + 2 * t[ix + 1] + t[ix + 2] + 2) >> 2
                } as u8;
        }
    }

}
fn ipred_y_8x8_hor_down(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut t = [0u16; 9];
    t[0] = u16::from(ctx.tl);
    for (dt, &st) in t[1..].iter_mut().zip(ctx.t.iter()) {
        *dt = u16::from(st);
    }
    let mut l = [0u16; 9];
    l[0] = u16::from(ctx.tl);
    for (dl, &sl) in l[1..].iter_mut().zip(ctx.l.iter()) {
        *dl = u16::from(sl);
    }

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            let zhd = 2 * (y as i8) - (x as i8);
            *pix = if zhd >= 0 {
                    let ix = y - (x >> 1);
                    if (zhd & 1) == 0 {
                        (l[ix] + l[ix + 1] + 1) >> 1
                    } else {
                        (l[ix - 1] + 2 * l[ix] + l[ix + 1] + 2) >> 2
                    }
                } else if zhd == -1 {
                    (l[1] + 2 * l[0] + t[0] + 2) >> 2
                } else {
                    let ix = x - 2 * y;
                    (t[ix] + 2 * t[ix - 1] + t[ix - 2] + 2) >> 2
                } as u8;
        }
    }
}
fn ipred_y_8x8_hor_up(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut l = [0u16; 8];
    for (dl, &sl) in l.iter_mut().zip(ctx.l.iter()) {
        *dl = u16::from(sl);
    }

    for (y, row) in buf.chunks_mut(stride).take(8).enumerate() {
        for (x, pix) in row.iter_mut().take(8).enumerate() {
            let zhu = x + 2 * y;
            let ix = y + (x >> 1);
            *pix = if zhu > 13 {
                    l[7]
                } else if zhu == 13 {
                    (l[6] + 3 * l[7] + 2) >> 2
                } else if (zhu & 1) != 0 {
                    (l[ix] + 2 * l[ix + 1] + l[ix + 2] + 2) >> 2
                } else {
                    (l[ix] + l[ix + 1] + 1) >> 1
                } as u8;
        }
    }
}
fn ipred_y_8x8_dc(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut sum = 0u16;
    for &t in ctx.t[..8].iter() {
        sum += u16::from(t);
    }
    for &l in ctx.l[..8].iter() {
        sum += u16::from(l);
    }
    let dc = ((sum + 8) >> 4) as u8;
    for row in buf.chunks_mut(stride).take(8) {
        for pix in row.iter_mut().take(8) {
            *pix = dc;
        }
    }
}
fn ipred_y_8x8_left_dc(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut sum = 0u16;
    for &l in ctx.l[..8].iter() {
        sum += u16::from(l);
    }
    let dc = ((sum + 4) >> 3) as u8;
    for row in buf.chunks_mut(stride).take(8) {
        for pix in row.iter_mut().take(8) {
            *pix = dc;
        }
    }
}
fn ipred_y_8x8_top_dc(buf: &mut [u8], stride: usize, ctx: &IPred8Context) {
    let mut sum = 0u16;
    for &t in ctx.t[..8].iter() {
        sum += u16::from(t);
    }
    let dc = ((sum + 4) >> 3) as u8;
    for row in buf.chunks_mut(stride).take(8) {
        for pix in row.iter_mut().take(8) {
            *pix = dc;
        }
    }
}
fn ipred_y_8x8_dc128(buf: &mut [u8], stride: usize, _ctx: &IPred8Context) {
    ipred_dc128(buf, 0, stride, 8);
}

fn ipred_8x8_ver(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_ver(buf, idx, stride, 8);
}
fn ipred_8x8_hor(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_hor(buf, idx, stride, 8);
}
fn ipred_8x8_dc(buf: &mut [u8], idx: usize, stride: usize) {
    let mut t: [u16; 8] = [0; 8];
    load_top(&mut t, buf, idx, stride, 8);
    let mut l: [u16; 8] = [0; 8];
    load_left(&mut l, buf, idx, stride, 8);

    let dc0 = ((t[0] + t[1] + t[2] + t[3] + l[0] + l[1] + l[2] + l[3] + 4) >> 3) as u8;
    let sum1 = t[4] + t[5] + t[6] + t[7];
    let dc1 = ((sum1 + 2) >> 2) as u8;
    let sum2 = l[4] + l[5] + l[6] + l[7];
    let dc2 = ((sum2 + 2) >> 2) as u8;
    let dc3 = ((sum1 + sum2 + 4) >> 3) as u8;

    let dst = &mut buf[idx..];
    for row in dst.chunks_mut(stride).take(4) {
        row[..4].copy_from_slice(&[dc0; 4]);
        row[4..8].copy_from_slice(&[dc1; 4]);
    }
    for row in dst.chunks_mut(stride).skip(4).take(4) {
        row[..4].copy_from_slice(&[dc2; 4]);
        row[4..8].copy_from_slice(&[dc3; 4]);
    }
}
fn ipred_8x8_left_dc(buf: &mut [u8], idx: usize, stride: usize) {
    let mut left_dc0 = 0;
    let mut left_dc1 = 0;
    for row in buf[idx - 1..].chunks(stride).take(4) {
        left_dc0 += u16::from(row[0]);
    }
    for row in buf[idx - 1..].chunks(stride).skip(4).take(4) {
        left_dc1 += u16::from(row[0]);
    }
    let dc0 = ((left_dc0 + 2) >> 2) as u8;
    let dc2 = ((left_dc1 + 2) >> 2) as u8;
    for row in buf[idx..].chunks_mut(stride).take(4) {
        row[..8].copy_from_slice(&[dc0; 8]);
    }
    for row in buf[idx..].chunks_mut(stride).skip(4).take(4) {
        row[..8].copy_from_slice(&[dc2; 8]);
    }
}
fn ipred_8x8_top_dc(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_top_dc(buf, idx, stride, 4, 2);
    ipred_top_dc(buf, idx + 4, stride, 4, 2);
    ipred_top_dc(buf, idx + 4 * stride, stride, 4, 2);
    ipred_top_dc(buf, idx + 4 + 4 * stride, stride, 4, 2);
}
fn ipred_8x8_dc128(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_dc128(buf, idx, stride, 8);
}
fn ipred_8x8_plane(buf: &mut [u8], idx: usize, stride: usize) {
    let mut h: i32 = 0;
    let mut v: i32 = 0;
    let     idx0 = idx + 3 - stride;
    let mut idx1 = idx + 4 * stride - 1;
    let mut idx2 = idx + 2 * stride - 1;
    for i in 0..4 {
        let i1 = (i + 1) as i32;
        h += i1 * (i32::from(buf[idx0 + i + 1]) - i32::from(buf[idx0 - i - 1]));
        v += i1 * (i32::from(buf[idx1]) - i32::from(buf[idx2]));
        idx1 += stride;
        idx2 -= stride;
    }
    let b = (17 * h + 16) >> 5;
    let c = (17 * v + 16) >> 5;
    let mut a = 16 * (i32::from(buf[idx - 1 + 7 * stride]) + i32::from(buf[idx + 7 - stride])) - 3 * (b + c) + 16;
    for line in buf[idx..].chunks_mut(stride).take(8) {
        let mut acc = a;
        for el in line.iter_mut().take(8) {
            *el = clip8((acc >> 5) as i16);
            acc += b;
        }
        a += c;
    }
}

fn ipred_16x16_ver(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_ver(buf, idx, stride, 16);
}
fn ipred_16x16_hor(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_hor(buf, idx, stride, 16);
}
fn ipred_16x16_dc(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_dc(buf, idx, stride, 16, 5);
}
fn ipred_16x16_left_dc(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_left_dc(buf, idx, stride, 16, 4);
}
fn ipred_16x16_top_dc(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_top_dc(buf, idx, stride, 16, 4);
}
fn ipred_16x16_dc128(buf: &mut [u8], idx: usize, stride: usize) {
    ipred_dc128(buf, idx, stride, 16);
}
fn ipred_16x16_plane(buf: &mut [u8], idx: usize, stride: usize) {
    let     idx0 = idx + 7 - stride;
    let mut idx1 = idx + 8 * stride - 1;
    let mut idx2 = idx1 - 2 * stride;

    let mut h = i32::from(buf[idx0 + 1]) - i32::from(buf[idx0 - 1]);
    let mut v = i32::from(buf[idx1])     - i32::from(buf[idx2]);

    for k in 2..9 {
        idx1 += stride;
        idx2 -= stride;
        h += (k as i32) * (i32::from(buf[idx0 + k]) - i32::from(buf[idx0 - k]));
        v += (k as i32) * (i32::from(buf[idx1])     - i32::from(buf[idx2]));
    }
    h = (5 * h + 32) >> 6;
    v = (5 * v + 32) >> 6;

    let mut a = 16 * (i32::from(buf[idx - 1 + 15 * stride]) + i32::from(buf[idx + 15 - stride]) + 1) - 7 * (v + h);

    for row in buf[idx..].chunks_mut(stride).take(16) {
        let mut b = a;
        a += v;

        for dst in row.chunks_exact_mut(4).take(4) {
            dst[0] = clip8(((b      ) >> 5) as i16);
            dst[1] = clip8(((b +   h) >> 5) as i16);
            dst[2] = clip8(((b + 2*h) >> 5) as i16);
            dst[3] = clip8(((b + 3*h) >> 5) as i16);
            b += h * 4;
        }
    }
}

pub type IPred4x4Func = fn(buf: &mut [u8], off: usize, stride: usize, tr: &[u8]);
pub type IPred8x8Func = fn(buf: &mut [u8], off: usize, stride: usize);
pub type IPred8x8LumaFunc = fn(buf: &mut [u8], stride: usize, ctx: &IPred8Context);

pub const IPRED4_DC128: usize = 11;
pub const IPRED4_DC_TOP: usize = 10;
pub const IPRED4_DC_LEFT: usize = 9;
pub const IPRED8_DC128: usize = 6;
pub const IPRED8_DC_TOP: usize = 5;
pub const IPRED8_DC_LEFT: usize = 4;

pub const IPRED_FUNCS4X4: [IPred4x4Func; 12] = [
    ipred_4x4_ver, ipred_4x4_hor, ipred_4x4_dc,
    ipred_4x4_diag_down_left, ipred_4x4_diag_down_right,
    ipred_4x4_ver_right, ipred_4x4_hor_down, ipred_4x4_ver_left, ipred_4x4_hor_up,
    ipred_4x4_left_dc, ipred_4x4_top_dc, ipred_4x4_dc128
];

pub const IPRED_FUNCS8X8_LUMA: [IPred8x8LumaFunc; 12] = [
    ipred_y_8x8_ver, ipred_y_8x8_hor, ipred_y_8x8_dc,
    ipred_y_8x8_diag_down_left, ipred_y_8x8_diag_down_right,
    ipred_y_8x8_ver_right, ipred_y_8x8_hor_down,
    ipred_y_8x8_ver_left, ipred_y_8x8_hor_up,
    ipred_y_8x8_left_dc, ipred_y_8x8_top_dc, ipred_y_8x8_dc128
];

pub const IPRED_FUNCS8X8_CHROMA: [IPred8x8Func; 7] = [
    ipred_8x8_dc, ipred_8x8_hor, ipred_8x8_ver, ipred_8x8_plane,
    ipred_8x8_left_dc, ipred_8x8_top_dc, ipred_8x8_dc128
];

pub const IPRED_FUNCS16X16: [IPred8x8Func; 7] = [
    ipred_16x16_ver, ipred_16x16_hor, ipred_16x16_dc, ipred_16x16_plane,
    ipred_16x16_left_dc, ipred_16x16_top_dc, ipred_16x16_dc128
];

fn clip_u8(val: i16) -> u8 { val.max(0).min(255) as u8 }

const TMP_BUF_STRIDE: usize = 32;

fn interp_block1(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize, hor: bool, avg0: bool) {
    let step = if hor { 1 } else { sstride };
    let mut idx = 0;
    let avgidx = if avg0 { step * 2 } else { step * 3 };
    for dline in dst.chunks_mut(dstride).take(h) {
        for (x, pix) in dline.iter_mut().take(w).enumerate() {
            let t = clip_u8((       i16::from(src[idx + x])
                             - 5  * i16::from(src[idx + x + step])
                             + 20 * i16::from(src[idx + x + step * 2])
                             + 20 * i16::from(src[idx + x + step * 3])
                             - 5  * i16::from(src[idx + x + step * 4])
                             +      i16::from(src[idx + x + step * 5])
                             + 16) >> 5);
            *pix = ((u16::from(t) + u16::from(src[idx + x + avgidx]) + 1) >> 1) as u8;
        }
        idx += sstride;
    }
}

fn interp_block2(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize, hor: bool) {
    let step = if hor { 1 } else { sstride };
    let mut idx = 0;
    for dline in dst.chunks_mut(dstride).take(h) {
        for (x, pix) in dline.iter_mut().take(w).enumerate() {
            *pix = clip_u8((       i16::from(src[idx + x])
                            - 5  * i16::from(src[idx + x + step])
                            + 20 * i16::from(src[idx + x + step * 2])
                            + 20 * i16::from(src[idx + x + step * 3])
                            - 5  * i16::from(src[idx + x + step * 4])
                            +      i16::from(src[idx + x + step * 5])
                            + 16) >> 5);
        }
        idx += sstride;
    }
}

fn mc_avg_tmp(dst: &mut [u8], dstride: usize, w: usize, h: usize, tmp: &[u8], tmp2: &[u8]) {
    for (dline, (sline0, sline1)) in dst.chunks_mut(dstride).zip(tmp.chunks(TMP_BUF_STRIDE).zip(tmp2.chunks(TMP_BUF_STRIDE))).take(h) {
        for (pix, (&a, &b)) in dline.iter_mut().zip(sline0.iter().zip(sline1.iter())).take(w) {
            *pix = ((u16::from(a) + u16::from(b) + 1) >> 1) as u8;
        }
    }
}

fn h264_mc00(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    for (dline, sline) in dst.chunks_mut(dstride).zip(src.chunks(sstride)).take(h) {
        dline[..w].copy_from_slice(&sline[..w]);
    }
}

fn h264_mc01(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block1(dst, dstride, &src[sstride * 2..], sstride, w, h, true, true);
}

fn h264_mc02(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block2(dst, dstride, &src[sstride * 2..], sstride, w, h, true);
}

fn h264_mc03(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block1(dst, dstride, &src[sstride * 2..], sstride, w, h, true, false);
}

fn h264_mc10(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block1(dst, dstride, &src[2..], sstride, w, h, false, true);
}

fn h264_mc11(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc02(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc20(&mut tmp2, TMP_BUF_STRIDE, src, sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc12(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc02(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc22(&mut tmp2, TMP_BUF_STRIDE, src, sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc13(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc02(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc20(&mut tmp2, TMP_BUF_STRIDE, &src[1..], sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc20(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block2(dst, dstride, &src[2..], sstride, w, h, false);
}

fn h264_mc21(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc22(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc20(&mut tmp2, TMP_BUF_STRIDE, src, sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc22(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp = [0i32; TMP_BUF_STRIDE * 16];
    let mut idx = 0;
    for dline in tmp.chunks_mut(TMP_BUF_STRIDE).take(h) {
        for (x, pix) in dline.iter_mut().take(w + 5).enumerate() {
            *pix =        i32::from(src[idx + x])
                   - 5  * i32::from(src[idx + x + sstride])
                   + 20 * i32::from(src[idx + x + sstride * 2])
                   + 20 * i32::from(src[idx + x + sstride * 3])
                   - 5  * i32::from(src[idx + x + sstride * 4])
                   +      i32::from(src[idx + x + sstride * 5]);
        }
        idx += sstride;
    }
    for (dline, sline) in dst.chunks_mut(dstride).zip(tmp.chunks(TMP_BUF_STRIDE)).take(h) {
        for (x, pix) in dline.iter_mut().take(w).enumerate() {
            *pix = clip8(((sline[x] - 5 * sline[x + 1] + 20 * sline[x + 2] + 20 * sline[x + 3] - 5 * sline[x + 4] + sline[x + 5] + 512) >> 10) as i16);
        }
    }
}

fn h264_mc23(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc22(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc20(&mut tmp2, TMP_BUF_STRIDE, &src[1..], sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc30(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    interp_block1(dst, dstride, &src[2..], sstride, w, h, false, false);
}

fn h264_mc31(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc20(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc02(&mut tmp2, TMP_BUF_STRIDE, &src[sstride..], sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc32(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc22(&mut tmp,  TMP_BUF_STRIDE, src, sstride, w, h);
    h264_mc02(&mut tmp2, TMP_BUF_STRIDE, &src[sstride..], sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}

fn h264_mc33(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, w: usize, h: usize) {
    let mut tmp  = [0u8; TMP_BUF_STRIDE * 16];
    let mut tmp2 = [0u8; TMP_BUF_STRIDE * 16];
    h264_mc20(&mut tmp,  TMP_BUF_STRIDE, &src[1..], sstride, w, h);
    h264_mc02(&mut tmp2, TMP_BUF_STRIDE, &src[sstride..], sstride, w, h);
    mc_avg_tmp(dst, dstride, w, h, &tmp, &tmp2);
}


fn chroma_interp(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, dx: u16, dy: u16, w: usize, h: usize) {
    let a0 = 8 - dx;
    let a1 = dx;
    let b0 = 8 - dy;
    let b1 = dy;

    let src1 = &src[sstride..];
    for (drow, (line0, line1)) in dst.chunks_mut(dstride).zip(src.chunks(sstride).zip(src1.chunks(sstride))).take(h) {
        let mut a = line0[0];
        let mut c = line1[0];
        for (pix, (&b, &d)) in drow.iter_mut().take(w).zip(line0[1..].iter().zip(line1[1..].iter())) {
            *pix = ((u16::from(a) * a0 * b0 + u16::from(b) * a1 * b0 + u16::from(c) * a0 * b1 + u16::from(d) * a1 * b1 + 0x20) >> 6) as u8;
            a = b;
            c = d;
        }
    }
}

const H264_LUMA_INTERP: &[BlkInterpFunc] = &[
    h264_mc00, h264_mc01, h264_mc02, h264_mc03,
    h264_mc10, h264_mc11, h264_mc12, h264_mc13,
    h264_mc20, h264_mc21, h264_mc22, h264_mc23,
    h264_mc30, h264_mc31, h264_mc32, h264_mc33
];

pub fn do_mc(frm: &mut NASimpleVideoFrame<u8>, refpic: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV) {
    let mode = ((mv.x & 3) + (mv.y & 3) * 4) as usize;
    copy_block(frm, refpic.clone(), 0, xpos, ypos, mv.x >> 2, mv.y >> 2, w, h, 2, 3, mode, H264_LUMA_INTERP);

    let (cw, ch) = refpic.get_dimensions(1);
    let mvx = mv.x >> 3;
    let mvy = mv.y >> 3;
    let dx = (mv.x & 7) as u16;
    let dy = (mv.y & 7) as u16;
    let mut ebuf = [0u8; 18 * 9];
    let src_x = ((xpos >> 1) as isize) + (mvx as isize);
    let src_y = ((ypos >> 1) as isize) + (mvy as isize);
    let suoff = refpic.get_offset(1);
    let svoff = refpic.get_offset(2);
    let sustride = refpic.get_stride(1);
    let svstride = refpic.get_stride(2);
    let src = refpic.get_data();
    let cbw = w / 2;
    let cbh = h / 2;
    let (csrc, cstride) = if (src_x < 0) || (src_x + (cbw as isize) + 1 > (cw as isize)) || (src_y < 0) || (src_y + (cbh as isize) + 1 > (ch as isize)) {
            edge_emu(&refpic, src_x, src_y, cbw+1, cbh+1, &mut ebuf,      18, 1, 4);
            edge_emu(&refpic, src_x, src_y, cbw+1, cbh+1, &mut ebuf[9..], 18, 2, 4);
            ([&ebuf, &ebuf[9..]], [18, 18])
        } else {
            ([&src[suoff + (src_x as usize) + (src_y as usize) * sustride..],
             &src[svoff + (src_x as usize) + (src_y as usize) * svstride..]],
             [sustride, svstride])
        };
    for chroma in 1..3 {
        let off = frm.offset[chroma] + xpos / 2 + (ypos / 2) * frm.stride[chroma];
        chroma_interp(&mut frm.data[off..], frm.stride[chroma], csrc[chroma - 1], cstride[chroma - 1], dx, dy, cbw, cbh);
    }
}

pub fn gray_block(frm: &mut NASimpleVideoFrame<u8>, x: usize, y: usize, w: usize, h: usize) {
    let yoff = frm.offset[0] + x + y * frm.stride[0];
    let coff = [frm.offset[1] + x / 2 + y / 2 * frm.stride[1],
                frm.offset[2] + x / 2 + y / 2 * frm.stride[2]];
    if w == 16 && h == 16 {
        IPRED_FUNCS16X16[IPRED8_DC128](frm.data, yoff, frm.stride[0]);
        for chroma in 1..2 {
            IPRED_FUNCS8X8_CHROMA[IPRED8_DC128](frm.data, coff[chroma - 1], frm.stride[chroma]);
        }
    } else if w == 8 && h == 8 {
        IPRED_FUNCS8X8_CHROMA[IPRED8_DC128](frm.data, yoff, frm.stride[0]);
        for chroma in 1..2 {
            IPRED_FUNCS4X4[IPRED4_DC128](frm.data, coff[chroma - 1], frm.stride[chroma], &[128; 4]);
        }
    } else {
        for row in frm.data[yoff..].chunks_mut(frm.stride[0]).take(h) {
            for el in row[..w].iter_mut() {
                *el = 128;
            }
        }
        for chroma in 0..2 {
            for row in frm.data[coff[chroma]..].chunks_mut(frm.stride[chroma + 1]).take(h / 2) {
                for el in row[..w / 2].iter_mut() {
                    *el = 128;
                }
            }
        }
    }
}

pub fn do_mc_avg(frm: &mut NASimpleVideoFrame<u8>, refpic: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV, avg_buf: &mut NAVideoBufferRef<u8>) {
    let mut afrm = NASimpleVideoFrame::from_video_buf(avg_buf).unwrap();
    let amv = MV { x: mv.x + (xpos as i16) * 4, y: mv.y + (ypos as i16) * 4 };
    do_mc(&mut afrm, refpic, 0, 0, w, h, amv);
    for comp in 0..3 {
        let shift = if comp == 0 { 0 } else { 1 };
        avg(&mut frm.data[frm.offset[comp] + (xpos >> shift) + (ypos >> shift) * frm.stride[comp]..], frm.stride[comp], &afrm.data[afrm.offset[comp]..], afrm.stride[comp], w >> shift, h >> shift);
    }
}

macro_rules! loop_filter {
    (lumaedge; $buf: expr, $off: expr, $step: expr, $alpha: expr, $beta: expr) => {
        let p2 = i16::from($buf[$off - $step * 3]);
        let p1 = i16::from($buf[$off - $step * 2]);
        let p0 = i16::from($buf[$off - $step]);
        let q0 = i16::from($buf[$off]);
        let q1 = i16::from($buf[$off + $step]);
        let q2 = i16::from($buf[$off + $step * 2]);
        let a_p = (p2 - p0).abs() < $beta;
        let a_q = (q2 - q0).abs() < $beta;
        if a_p && (p0 - q0).abs() < (($alpha >> 2) + 2) {
            let p3 = i16::from($buf[$off - $step * 4]);
            $buf[$off - $step * 3] = ((2 * p3 + 3 * p2 + p1 + p0 + q0 + 4) >> 3) as u8;
            $buf[$off - $step * 2] = ((p2 + p1 + p0 + q0 + 2) >> 2) as u8;
            $buf[$off - $step] = ((p2 + 2 * p1 + 2 * p0 + 2 * q0 + q1 + 4) >> 3) as u8;
        } else {
            $buf[$off - $step] = ((2 * p1 + p0 + q1 + 2) >> 2) as u8;
        }
        if a_q && (p0 - q0).abs() < (($alpha >> 2) + 2) {
            let q3 = i16::from($buf[$off + $step * 3]);
            $buf[$off]             = ((p1 + 2 * p0 + 2 * q0 + 2 * q1 + q2 + 4) >> 3) as u8;
            $buf[$off + $step]     = ((p0 + q0 + q1 + q2 + 2) >> 2) as u8;
            $buf[$off + $step * 2] = ((2 * q3 + 3 * q2 + q1 + q0 + p0 + 4) >> 3) as u8;
        } else {
            $buf[$off] = ((2 * q1 + q0 + p1 + 2) >> 2) as u8;
        }
    };
    (chromaedge; $buf: expr, $off: expr, $step: expr) => {
        let p1 = i16::from($buf[$off - $step * 2]);
        let p0 = i16::from($buf[$off - $step]);
        let q0 = i16::from($buf[$off]);
        let q1 = i16::from($buf[$off + $step]);
        $buf[$off - $step] = ((2 * p1 + p0 + q1 + 2) >> 2) as u8;
        $buf[$off]         = ((2 * q1 + q0 + p1 + 2) >> 2) as u8;
    };
    (lumanormal; $buf: expr, $off: expr, $step: expr, $tc0: expr, $beta: expr) => {
        let p2 = i16::from($buf[$off - $step * 3]);
        let p1 = i16::from($buf[$off - $step * 2]);
        let p0 = i16::from($buf[$off - $step]);
        let q0 = i16::from($buf[$off]);
        let q1 = i16::from($buf[$off + $step]);
        let q2 = i16::from($buf[$off + $step * 2]);
        let a_p = (p2 - p0).abs() < $beta;
        let a_q = (q2 - q0).abs() < $beta;
        let tc = $tc0 + (a_p as i16) + (a_q as i16);
        let delta = (((q0 - p0) * 4 + (p1 - q1) + 4) >> 3).max(-tc).min(tc);
        if a_p && ($tc0 > 0) {
            $buf[$off - $step * 2] = clip8(p1 + ((p2 + ((p0 + q0 + 1) >> 1) - p1 * 2) >> 1).max(-$tc0).min($tc0));
        }
        $buf[$off - $step] = clip8(p0 + delta);
        $buf[$off]         = clip8(q0 - delta);
        if a_q && ($tc0 > 0) {
            $buf[$off + $step] = clip8(q1 + ((q2 + ((p0 + q0 + 1) >> 1) - q1 * 2) >> 1).max(-$tc0).min($tc0));
        }
    };
    (chromanormal; $buf: expr, $off: expr, $step: expr, $tc0: expr) => {
        let p1 = i16::from($buf[$off - $step * 2]);
        let p0 = i16::from($buf[$off - $step]);
        let q0 = i16::from($buf[$off]);
        let q1 = i16::from($buf[$off + $step]);
        let tc = $tc0 + 1;
        let delta = (((q0 - p0) * 4 + (p1 - q1) + 4) >> 3).max(-tc).min(tc);
        $buf[$off - $step] = clip8(p0 + delta);
        $buf[$off]         = clip8(q0 - delta);
    }
}

fn check_filter(buf: &[u8], off: usize, step: usize, alpha: i16, beta: i16) -> bool {
    let p1 = i16::from(buf[off - step * 2]);
    let p0 = i16::from(buf[off - step]);
    let q0 = i16::from(buf[off]);
    let q1 = i16::from(buf[off + step]);
    (p0 - q0).abs() < alpha && (p1 - p0).abs() < beta && (q1 - q0).abs() < beta
}

pub fn loop_filter_lumaedge_v(dst: &mut [u8], mut off: usize, stride: usize, alpha: i16, beta: i16) {
    for _ in 0..4 {
        if check_filter(dst, off, 1, alpha, beta) {
            loop_filter!(lumaedge; dst, off, 1, alpha, beta);
        }
        off += stride;
    }
}
pub fn loop_filter_lumaedge_h(dst: &mut [u8], off: usize, stride: usize, alpha: i16, beta: i16) {
    for x in 0..4 {
        if check_filter(dst, off + x, stride, alpha, beta) {
            loop_filter!(lumaedge; dst, off + x, stride, alpha, beta);
        }
    }
}
pub fn loop_filter_lumanormal_v(dst: &mut [u8], mut off: usize, stride: usize, alpha: i16, beta: i16, tc0: i16) {
    for _ in 0..4 {
        if check_filter(dst, off, 1, alpha, beta) {
            loop_filter!(lumanormal; dst, off, 1, tc0, beta);
        }
        off += stride;
    }
}
pub fn loop_filter_lumanormal_h(dst: &mut [u8], off: usize, stride: usize, alpha: i16, beta: i16, tc0: i16) {
    for x in 0..4 {
        if check_filter(dst, off + x, stride, alpha, beta) {
            loop_filter!(lumanormal; dst, off + x, stride, tc0, beta);
        }
    }
}
pub fn loop_filter_chromaedge_v(dst: &mut [u8], mut off: usize, stride: usize, alpha: i16, beta: i16) {
    for _ in 0..4 {
        if check_filter(dst, off, 1, alpha, beta) {
            loop_filter!(chromaedge; dst, off, 1);
        }
        off += stride;
    }
}
pub fn loop_filter_chromaedge_h(dst: &mut [u8], off: usize, stride: usize, alpha: i16, beta: i16) {
    for x in 0..4 {
        if check_filter(dst, off + x, stride, alpha, beta) {
            loop_filter!(chromaedge; dst, off + x, stride);
        }
    }
}
pub fn loop_filter_chromanormal_v(dst: &mut [u8], mut off: usize, stride: usize, alpha: i16, beta: i16, tc0: i16) {
    for _ in 0..4 {
        if check_filter(dst, off, 1, alpha, beta) {
            loop_filter!(chromanormal; dst, off, 1, tc0);
        }
        off += stride;
    }
}
pub fn loop_filter_chromanormal_h(dst: &mut [u8], off: usize, stride: usize, alpha: i16, beta: i16, tc0: i16) {
    for x in 0..4 {
        if check_filter(dst, off + x, stride, alpha, beta) {
            loop_filter!(chromanormal; dst, off + x, stride, tc0);
        }
    }
}
