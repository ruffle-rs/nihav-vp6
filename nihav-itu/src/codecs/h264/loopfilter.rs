use nihav_core::frame::NASimpleVideoFrame;
use super::types::SliceState;
use super::dsp::*;

const ALPHA: [i16; 52] = [
      0,   0,   0,   0,  0,  0,  0,  0,  0,  0,   0,   0,   0,   0,   0,   0,
      4,   4,   5,   6,  7,  8,  9, 10, 12, 13,  15,  17,  20,  22,  25,  28,
     32,  36,  40,  45, 50, 56, 63, 71, 80, 90, 100, 113, 127, 144, 162, 182,
    203, 226, 255, 255
];
const BETA: [i16; 52] = [
     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,
     2,  2,  2,  3,  3,  3,  3,  4,  4,  4,  6,  6,  7,  7,  8,  8,
     9,  9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16,
    17, 17, 18, 18
];

const TC0: [[u8; 3]; 52] = [
    [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0],
    [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0],
    [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0],
    [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0], [ 0,  0,  0],
    [ 0,  0,  0], [ 0,  0,  1], [ 0,  0,  1], [ 0,  0,  1],
    [ 0,  0,  1], [ 0,  1,  1], [ 0,  1,  1], [ 1,  1,  1],
    [ 1,  1,  1], [ 1,  1,  1], [ 1,  1,  1], [ 1,  1,  2],
    [ 1,  1,  2], [ 1,  1,  2], [ 1,  1,  2], [ 1,  2,  3],
    [ 1,  2,  3], [ 2,  2,  3], [ 2,  2,  4], [ 2,  3,  4],
    [ 2,  3,  4], [ 3,  3,  5], [ 3,  4,  6], [ 3,  4,  6],
    [ 4,  5,  7], [ 4,  5,  8], [ 4,  6,  9], [ 5,  7, 10],
    [ 6,  8, 11], [ 6,  8, 13], [ 7, 10, 14], [ 8, 11, 16],
    [ 9, 12, 18], [10, 13, 20], [11, 15, 23], [13, 17, 25]
];

fn get_lf_idx(qp0: u8, qp1: u8, off: i8) -> usize {
    (i16::from((qp0 + qp1 + 1) >> 1) + i16::from(off)).max(0).min(51) as usize
}

fn filter_mb_row4_y(dst: &mut [u8], off: usize, stride: usize, dmodes: [u8; 4], quants: [u8; 3], alpha_off: i8, beta_off: i8) {
    let q = quants[0];
    let qleft = quants[1];
    let dmode = dmodes[0] & 0xF;
    if dmode != 0 {
        let index_a_y = get_lf_idx(q, qleft, alpha_off);
        let alpha_y = ALPHA[index_a_y];
        let beta_y = BETA[get_lf_idx(q, qleft, beta_off)];
        if dmode == 4 {
            loop_filter_lumaedge_v(dst, off, stride, alpha_y, beta_y);
        } else {
            let tc0 = i16::from(TC0[index_a_y][(dmode - 1) as usize]);
            loop_filter_lumanormal_v(dst, off, stride, alpha_y, beta_y, tc0);
        }
    }
    let index_a_y = get_lf_idx(q, q, alpha_off);
    let alpha_y = ALPHA[index_a_y];
    let beta_y = BETA[get_lf_idx(q, q, beta_off)];

    for i in 1..4 {
        let dmode = dmodes[i] & 0xF;
        if dmode != 0 {
            let tc0 = i16::from(TC0[index_a_y][(dmode - 1) as usize]);
            loop_filter_lumanormal_v(dst, off + i * 4, stride, alpha_y, beta_y, tc0);
        }
    }

    let qtop = quants[2];
    let index_a_y = get_lf_idx(q, qtop, alpha_off);
    let alpha_y = ALPHA[index_a_y];
    let beta_y = BETA[get_lf_idx(q, qtop, beta_off)];
    for i in 0..4 {
        let dmode = dmodes[i] >> 4;
        if dmode == 4 {
            loop_filter_lumaedge_h(dst, off + i * 4, stride, alpha_y, beta_y);
        } else if dmode != 0 {
            let tc0 = i16::from(TC0[index_a_y][(dmode - 1) as usize]);
            loop_filter_lumanormal_h(dst, off + i * 4, stride, alpha_y, beta_y, tc0);
        }
    }
}

fn filter_mb_row4_c(dst: &mut [u8], off: usize, stride: usize, dmodes: [u8; 4], quants: [u8; 3], alpha_off: i8, beta_off: i8) {
    let q = quants[0];
    let qleft = quants[1];

    let dmode = dmodes[0] & 0xF;
    if dmode != 0 {
        let index_a_c = get_lf_idx(q, qleft, alpha_off);
        let alpha_c = ALPHA[index_a_c];
        let beta_c = BETA[get_lf_idx(q, qleft, beta_off)];
        if dmode == 4 {
            loop_filter_chromaedge_v(dst, off, stride, alpha_c, beta_c);
        } else {
            let tc0 = i16::from(TC0[index_a_c][(dmode - 1) as usize]);
            loop_filter_chromanormal_v(dst, off, stride, alpha_c, beta_c, tc0);
        }
    }
    let dmode = dmodes[2] & 0xF;
    if dmode != 0 {
        let index_a_c = get_lf_idx(q, q, alpha_off);
        let alpha_c = ALPHA[index_a_c];
        let beta_c = BETA[get_lf_idx(q, q, beta_off)];
        let tc0 = i16::from(TC0[index_a_c][(dmode - 1) as usize]);
        loop_filter_chromanormal_v(dst, off + 4, stride, alpha_c, beta_c, tc0);
    }

    let qtop = quants[2];
    let index_a_c = get_lf_idx(q, qtop, alpha_off);
    let alpha_c = ALPHA[index_a_c];
    let beta_c = BETA[get_lf_idx(q, qtop, beta_off)];
    for i in 0..2 {
        let dmode = dmodes[i * 2] >> 4;
        if dmode == 4 {
            loop_filter_chromaedge_h(dst, off + i * 4, stride, alpha_c, beta_c);
        } else if dmode != 0 {
            let tc0 = i16::from(TC0[index_a_c][(dmode - 1) as usize]);
            loop_filter_chromanormal_h(dst, off + i * 4, stride, alpha_c, beta_c, tc0);
        }
    }
}

pub fn loop_filter_row(frm: &mut NASimpleVideoFrame<u8>, sstate: &SliceState, alpha_off: i8, beta_off: i8) {
    let mut db_idx = sstate.deblock.xpos - sstate.deblock.stride;
    let mut yoff = frm.offset[0] + sstate.mb_y * 16 * frm.stride[0];
    let mut uoff = frm.offset[1] + sstate.mb_y *  8 * frm.stride[1];
    let mut voff = frm.offset[2] + sstate.mb_y *  8 * frm.stride[2];
    let mut tlq = [0; 3];
    let mut lq  = [0; 3];
    let mut mb_idx = sstate.mb.xpos;
    for _mb_x in 0..sstate.mb_w {
        let mut tqy = sstate.mb.data[mb_idx - sstate.mb.stride].qp_y;
        let     tqu = sstate.mb.data[mb_idx - sstate.mb.stride].qp_u;
        let     tqv = sstate.mb.data[mb_idx - sstate.mb.stride].qp_v;
        if sstate.mb_y > 0 {
            let dmodes = [sstate.deblock.data[db_idx],
                          sstate.deblock.data[db_idx + 1],
                          sstate.deblock.data[db_idx + 2],
                          sstate.deblock.data[db_idx + 3]];

            filter_mb_row4_y(frm.data, yoff - frm.stride[0] * 4, frm.stride[0], dmodes, [tqy, tlq[0], tqy], alpha_off, beta_off);
            filter_mb_row4_c(frm.data, uoff - frm.stride[1] * 4, frm.stride[1], dmodes, [tqu, tlq[1], tqu], alpha_off, beta_off);
            filter_mb_row4_c(frm.data, voff - frm.stride[2] * 4, frm.stride[2], dmodes, [tqv, tlq[2], tqv], alpha_off, beta_off);

            tlq = [tqy, tqu, tqv];
        }

        let qy = sstate.mb.data[mb_idx].qp_y;
        let qu = sstate.mb.data[mb_idx].qp_u;
        let qv = sstate.mb.data[mb_idx].qp_v;

        for y in 0..3 {
            db_idx += sstate.deblock.stride;
            let dmodes = [sstate.deblock.data[db_idx],
                          sstate.deblock.data[db_idx + 1],
                          sstate.deblock.data[db_idx + 2],
                          sstate.deblock.data[db_idx + 3]];

            filter_mb_row4_y(frm.data, yoff + frm.stride[0] * 4 * y, frm.stride[0], dmodes, [qy, lq[0], tqy], alpha_off, beta_off);
            if y == 0 {
                filter_mb_row4_c(frm.data, uoff + frm.stride[1] * 2 * y, frm.stride[1], dmodes, [qu, lq[1], tqu], alpha_off, beta_off);
                filter_mb_row4_c(frm.data, voff + frm.stride[2] * 2 * y, frm.stride[2], dmodes, [qv, lq[2], tqv], alpha_off, beta_off);
            }
            tqy = qy;
        }
        db_idx -= sstate.deblock.stride * 3;
        lq = [qy, qu, qv];

        mb_idx += 1;
        db_idx += 4;
        yoff += 16;
        uoff += 8;
        voff += 8;
    }
}
pub fn loop_filter_last(frm: &mut NASimpleVideoFrame<u8>, sstate: &SliceState, alpha_off: i8, beta_off: i8) {
    let mut db_idx = sstate.deblock.xpos + 3 * sstate.deblock.stride;
    let mut yoff = frm.offset[0] + (sstate.mb_y * 16 + 12) * frm.stride[0];
    let mut uoff = frm.offset[1] + (sstate.mb_y *  8 +  4) * frm.stride[1];
    let mut voff = frm.offset[2] + (sstate.mb_y *  8 +  4) * frm.stride[2];

    let mut lq = [0; 3];
    let mut mb_idx = sstate.mb.xpos;
    if sstate.mb_y != 0 && sstate.mb_x == 0 {
        db_idx -= 4 * sstate.deblock.stride;
        mb_idx -= sstate.mb.stride;
        yoff -= 16 * frm.stride[0];
        uoff -=  8 * frm.stride[1];
        voff -=  8 * frm.stride[2];
    }
    for _mb_x in 0..sstate.mb_w {
        let qy = sstate.mb.data[mb_idx].qp_y;
        let qu = sstate.mb.data[mb_idx].qp_u;
        let qv = sstate.mb.data[mb_idx].qp_v;

        let dmodes = [sstate.deblock.data[db_idx],
                      sstate.deblock.data[db_idx + 1],
                      sstate.deblock.data[db_idx + 2],
                      sstate.deblock.data[db_idx + 3]];

        filter_mb_row4_y(frm.data, yoff, frm.stride[0], dmodes, [qy, lq[0], qy], alpha_off, beta_off);
        filter_mb_row4_c(frm.data, uoff, frm.stride[1], dmodes, [qu, lq[1], qu], alpha_off, beta_off);
        filter_mb_row4_c(frm.data, voff, frm.stride[2], dmodes, [qv, lq[2], qv], alpha_off, beta_off);

        lq = [qy, qu, qv];
        mb_idx += 1;
        db_idx += 4;
        yoff += 16;
        uoff += 8;
        voff += 8;
    }
}

