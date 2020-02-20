use nihav_core::frame::*;
use nihav_codec_support::codecs::blockdsp::edge_emu;

fn clip_u8(val: i16) -> u8 {
    val.max(0).min(255) as u8
}

pub struct IPredContext {
    pub left:       [u8; 16],
    pub has_left:   bool,
    pub top:        [u8; 16],
    pub has_top:    bool,
    pub tl:         u8,
}

impl IPredContext {
    pub fn fill(&mut self, src: &[u8], off: usize, stride: usize, tsize: usize, lsize: usize) {
        if self.has_top {
            for i in 0..tsize {
                self.top[i] = src[off - stride + i];
            }
            for i in tsize..16 {
                self.top[i] = 0x80;
            }
        } else {
            self.top = [0x80; 16];
        }
        if self.has_left {
            for i in 0..lsize {
                self.left[i] = src[off - 1 + i * stride];
            }
            for i in lsize..16 {
                self.left[i] = 0x80;
            }
        } else {
            self.left = [0x80; 16];
        }
        if self.has_top && self.has_left {
            self.tl = src[off - stride - 1];
        } else {
            self.tl = 0x80;
        }
    }
}

impl Default for IPredContext {
    fn default() -> Self {
        Self {
            left:       [0x80; 16],
            top:        [0x80; 16],
            tl:         0x80,
            has_left:   false,
            has_top:    false,
        }
    }
}

const DCT_COEFFS: [i32; 16] = [
    23170,  23170,  23170,  23170,
    30274,  12540, -12540, -30274,
    23170, -23170, -23170,  23170,
    12540, -30274,  30274, -12540
];

pub fn idct4x4(coeffs: &mut [i16; 16]) {
    let mut tmp = [0i16; 16];
    for (src, dst) in coeffs.chunks(4).zip(tmp.chunks_mut(4)) {
        let s0 = src[0] as i32;
        let s1 = src[1] as i32;
        let s2 = src[2] as i32;
        let s3 = src[3] as i32;

        let t0 = (s0 + s2).wrapping_mul(23170);
        let t1 = (s0 - s2).wrapping_mul(23170);
        let t2 = s1.wrapping_mul(30274) + s3.wrapping_mul(12540);
        let t3 = s1.wrapping_mul(12540) - s3.wrapping_mul(30274);

        dst[0] = ((t0 + t2) >> 14) as i16;
        dst[1] = ((t1 + t3) >> 14) as i16;
        dst[2] = ((t1 - t3) >> 14) as i16;
        dst[3] = ((t0 - t2) >> 14) as i16;
    }
    for i in 0..4 {
        let s0 = tmp[i + 4 * 0] as i32;
        let s1 = tmp[i + 4 * 1] as i32;
        let s2 = tmp[i + 4 * 2] as i32;
        let s3 = tmp[i + 4 * 3] as i32;

        let t0 = (s0 + s2).wrapping_mul(23170) + 0x20000;
        let t1 = (s0 - s2).wrapping_mul(23170) + 0x20000;
        let t2 = s1.wrapping_mul(30274) + s3.wrapping_mul(12540);
        let t3 = s1.wrapping_mul(12540) - s3.wrapping_mul(30274);

        coeffs[i + 0 * 4] = ((t0 + t2) >> 18) as i16;
        coeffs[i + 1 * 4] = ((t1 + t3) >> 18) as i16;
        coeffs[i + 2 * 4] = ((t1 - t3) >> 18) as i16;
        coeffs[i + 3 * 4] = ((t0 - t2) >> 18) as i16;
    }
}

pub fn idct4x4_dc(coeffs: &mut [i16; 16]) {
    let dc = (((((coeffs[0] as i32) * DCT_COEFFS[0]) >> 14) * DCT_COEFFS[0] + 0x20000) >> 18) as i16;
    for el in coeffs.iter_mut() {
        *el = dc;
    }
}

pub fn add_coeffs4x4(dst: &mut [u8], off: usize, stride: usize, coeffs: &[i16; 16]) {
    let dst = &mut dst[off..];
    for (out, src) in dst.chunks_mut(stride).zip(coeffs.chunks(4)) {
        for (oel, iel) in out.iter_mut().take(4).zip(src.iter()) {
            *oel = clip_u8((*oel as i16) + *iel);
        }
    }
}
pub fn add_coeffs16x1(dst: &mut [u8], off: usize, coeffs: &[i16; 16]) {
    let dst = &mut dst[off..];
    for (oel, iel) in dst.iter_mut().take(16).zip(coeffs.iter()) {
        *oel = clip_u8((*oel as i16) + *iel);
    }
}

pub trait IntraPred {
    const SIZE: usize;
    fn ipred_dc(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let dc;
        if !ipred.has_left && !ipred.has_top {
            dc = 0x80;
        } else {
            let mut dcsum = 0;
            let mut dcshift = match Self::SIZE {
                    16 => 3,
                    _  => 2,
                };
            if ipred.has_left {
                for el in ipred.left.iter().take(Self::SIZE) {
                    dcsum += *el as u16;
                }
                dcshift += 1;
            }
            if ipred.has_top {
                for el in ipred.top.iter().take(Self::SIZE) {
                    dcsum += *el as u16;
                }
                dcshift += 1;
            }
            dc = ((dcsum + (1 << (dcshift - 1))) >> dcshift) as u8;
        }
        for _ in 0..Self::SIZE {
            let out = &mut dst[off..][..Self::SIZE];
            for el in out.iter_mut() {
                *el = dc;
            }
            off += stride;
        }
    }
    fn ipred_v(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        for _ in 0..Self::SIZE {
            let out = &mut dst[off..][..Self::SIZE];
            out.copy_from_slice(&ipred.top[0..Self::SIZE]);
            off += stride;
        }
    }
    fn ipred_h(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        for leftel in ipred.left.iter().take(Self::SIZE) {
            let out = &mut dst[off..][..Self::SIZE];
            for el in out.iter_mut() {
                *el = *leftel;
            }
            off += stride;
        }
    }
    fn ipred_tm(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let tl = ipred.tl as i16;
        for m in 0..Self::SIZE {
            for n in 0..Self::SIZE {
                dst[off + n] = clip_u8((ipred.left[m] as i16) + (ipred.top[n] as i16) - tl);
            }
            off += stride;
        }
    }
}

pub struct IPred16x16 {}
impl IntraPred for IPred16x16 { const SIZE: usize = 16; }

pub struct IPred8x8 {}
impl IntraPred for IPred8x8 { const SIZE: usize = 8; }

macro_rules! load_pred4 {
    (topleft; $ipred: expr) => {{
        let tl = $ipred.tl as u16;
        let a0 = $ipred.top[0] as u16;
        let l0 = $ipred.left[0] as u16;
        ((l0 + tl * 2 + a0 + 2) >> 2) as u8
    }};
    (top; $ipred: expr) => {{
        let tl = $ipred.tl as u16;
        let a0 = $ipred.top[0] as u16;
        let a1 = $ipred.top[1] as u16;
        let a2 = $ipred.top[2] as u16;
        let a3 = $ipred.top[3] as u16;
        let a4 = $ipred.top[4] as u16;
        let p0 = ((tl + a0 * 2 + a1 + 2) >> 2) as u8;
        let p1 = ((a0 + a1 * 2 + a2 + 2) >> 2) as u8;
        let p2 = ((a1 + a2 * 2 + a3 + 2) >> 2) as u8;
        let p3 = ((a2 + a3 * 2 + a4 + 2) >> 2) as u8;
        (p0, p1, p2, p3)
    }};
    (top8; $ipred: expr) => {{
        let t3 = $ipred.top[3] as u16;
        let t4 = $ipred.top[4] as u16;
        let t5 = $ipred.top[5] as u16;
        let t6 = $ipred.top[6] as u16;
        let t7 = $ipred.top[7] as u16;
        let p4 = ((t3 + t4 * 2 + t5 + 2) >> 2) as u8;
        let p5 = ((t4 + t5 * 2 + t6 + 2) >> 2) as u8;
        let p6 = ((t5 + t6 * 2 + t7 + 2) >> 2) as u8;
        let p7 = ((t6 + t7 * 2 + t7 + 2) >> 2) as u8;
        (p4, p5, p6, p7)
    }};
    (topavg; $ipred: expr) => {{
        let tl = $ipred.tl as u16;
        let a0 = $ipred.top[0] as u16;
        let a1 = $ipred.top[1] as u16;
        let a2 = $ipred.top[2] as u16;
        let a3 = $ipred.top[3] as u16;
        let p0 = ((tl + a0 + 1) >> 1) as u8;
        let p1 = ((a0 + a1 + 1) >> 1) as u8;
        let p2 = ((a1 + a2 + 1) >> 1) as u8;
        let p3 = ((a2 + a3 + 1) >> 1) as u8;
        (p0, p1, p2, p3)
    }};
    (left; $ipred: expr) => {{
        let tl = $ipred.tl as u16;
        let l0 = $ipred.left[0] as u16;
        let l1 = $ipred.left[1] as u16;
        let l2 = $ipred.left[2] as u16;
        let l3 = $ipred.left[3] as u16;
        let l4 = $ipred.left[4] as u16;
        let p0 = ((tl + l0 * 2 + l1 + 2) >> 2) as u8;
        let p1 = ((l0 + l1 * 2 + l2 + 2) >> 2) as u8;
        let p2 = ((l1 + l2 * 2 + l3 + 2) >> 2) as u8;
        let p3 = ((l2 + l3 * 2 + l4 + 2) >> 2) as u8;
        (p0, p1, p2, p3)
    }};
    (left8; $ipred: expr) => {{
        let l3 = $ipred.left[3] as u16;
        let l4 = $ipred.left[4] as u16;
        let l5 = $ipred.left[5] as u16;
        let l6 = $ipred.left[6] as u16;
        let l7 = $ipred.left[7] as u16;
        let p4 = ((l3 + l4 * 2 + l5 + 2) >> 2) as u8;
        let p5 = ((l4 + l5 * 2 + l6 + 2) >> 2) as u8;
        let p6 = ((l5 + l6 * 2 + l7 + 2) >> 2) as u8;
        let p7 = ((l6 + l7 * 2 + l7 + 2) >> 2) as u8;
        (p4, p5, p6, p7)
    }};
    (leftavg; $ipred: expr) => {{
        let tl = $ipred.tl as u16;
        let l0 = $ipred.left[0] as u16;
        let l1 = $ipred.left[1] as u16;
        let l2 = $ipred.left[2] as u16;
        let l3 = $ipred.left[3] as u16;
        let p0 = ((tl + l0 + 1) >> 1) as u8;
        let p1 = ((l0 + l1 + 1) >> 1) as u8;
        let p2 = ((l1 + l2 + 1) >> 1) as u8;
        let p3 = ((l2 + l3 + 1) >> 1) as u8;
        (p0, p1, p2, p3)
    }};
}

pub struct IPred4x4 {}
impl IPred4x4 {
    pub fn ipred_dc(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let dc;
        let mut dcsum = 0;
        for el in ipred.left.iter().take(4) {
            dcsum += *el as u16;
        }
        for el in ipred.top.iter().take(4) {
            dcsum += *el as u16;
        }
        dc = ((dcsum + (1 << 2)) >> 3) as u8;
        for _ in 0..4 {
            let out = &mut dst[off..][..4];
            for el in out.iter_mut() {
                *el = dc;
            }
            off += stride;
        }
    }
    pub fn ipred_tm(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let tl = ipred.tl as i16;
        for m in 0..4 {
            for n in 0..4 {
                dst[off + n] = clip_u8((ipred.left[m] as i16) + (ipred.top[n] as i16) - tl);
            }
            off += stride;
        }
    }
    pub fn ipred_ve(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let (v0, v1, v2, v3) = load_pred4!(top; ipred);
        let vert_pred = [v0, v1, v2, v3];
        for _ in 0..4 {
            let out = &mut dst[off..][..4];
            out.copy_from_slice(&vert_pred);
            off += stride;
        }
    }
    pub fn ipred_he(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let (p0, p1, p2, _) = load_pred4!(left; ipred);
        let p3 = (((ipred.left[2] as u16) + (ipred.left[3] as u16) * 3 + 2) >> 2) as u8;
        let hor_pred = [p0, p1, p2, p3];
        for m in 0..4 {
            for n in 0..4 {
                dst[off + n] = hor_pred[m];
            }
            off += stride;
        }
    }
    pub fn ipred_ld(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let (_,  p0, p1, p2) = load_pred4!(top;  ipred);
        let (p3, p4, p5, p6) = load_pred4!(top8; ipred);

        dst[off + 0] = p0; dst[off + 1] = p1; dst[off + 2] = p2; dst[off + 3] = p3;
        off += stride;
        dst[off + 0] = p1; dst[off + 1] = p2; dst[off + 2] = p3; dst[off + 3] = p4;
        off += stride;
        dst[off + 0] = p2; dst[off + 1] = p3; dst[off + 2] = p4; dst[off + 3] = p5;
        off += stride;
        dst[off + 0] = p3; dst[off + 1] = p4; dst[off + 2] = p5; dst[off + 3] = p6;
    }
    pub fn ipred_rd(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let tl              = load_pred4!(topleft;  ipred);
        let (l0, l1, l2, _) = load_pred4!(left;     ipred);
        let (t0, t1, t2, _) = load_pred4!(top;      ipred);

        dst[off + 0] = tl; dst[off + 1] = t0; dst[off + 2] = t1; dst[off + 3] = t2;
        off += stride;
        dst[off + 0] = l0; dst[off + 1] = tl; dst[off + 2] = t0; dst[off + 3] = t1;
        off += stride;
        dst[off + 0] = l1; dst[off + 1] = l0; dst[off + 2] = tl; dst[off + 3] = t0;
        off += stride;
        dst[off + 0] = l2; dst[off + 1] = l1; dst[off + 2] = l0; dst[off + 3] = tl;
    }
    pub fn ipred_vr(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let tl               = load_pred4!(topleft; ipred);
        let (l0, l1, _,  _)  = load_pred4!(left;    ipred);
        let (t0, t1, t2, _)  = load_pred4!(top;     ipred);
        let (m0, m1, m2, m3) = load_pred4!(topavg;  ipred);

        dst[off + 0] = m0; dst[off + 1] = m1; dst[off + 2] = m2; dst[off + 3] = m3;
        off += stride;
        dst[off + 0] = tl; dst[off + 1] = t0; dst[off + 2] = t1; dst[off + 3] = t2;
        off += stride;
        dst[off + 0] = l0; dst[off + 1] = m0; dst[off + 2] = m1; dst[off + 3] = m2;
        off += stride;
        dst[off + 0] = l1; dst[off + 1] = tl; dst[off + 2] = t0; dst[off + 3] = t1;
    }
    pub fn ipred_vl(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let (_,  t1, t2, t3) = load_pred4!(top;     ipred);
        let (t4, t5, t6, _)  = load_pred4!(top8;    ipred);
        let (_,  m1, m2, m3) = load_pred4!(topavg;  ipred);
        let m4 = (((ipred.top[3] as u16) + (ipred.top[4] as u16) + 1) >> 1) as u8;

        dst[off + 0] = m1; dst[off + 1] = m2; dst[off + 2] = m3; dst[off + 3] = m4;
        off += stride;
        dst[off + 0] = t1; dst[off + 1] = t2; dst[off + 2] = t3; dst[off + 3] = t4;
        off += stride;
        dst[off + 0] = m2; dst[off + 1] = m3; dst[off + 2] = m4; dst[off + 3] = t5;
        off += stride;
        dst[off + 0] = t2; dst[off + 1] = t3; dst[off + 2] = t4; dst[off + 3] = t6;
    }
    pub fn ipred_hd(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let tl               = load_pred4!(topleft; ipred);
        let (l0, l1, l2, _)  = load_pred4!(left;    ipred);
        let (m0, m1, m2, m3) = load_pred4!(leftavg; ipred);
        let (t0, t1, _,  _)  = load_pred4!(top;     ipred);

        dst[off + 0] = m0; dst[off + 1] = tl; dst[off + 2] = t0; dst[off + 3] = t1;
        off += stride;
        dst[off + 0] = m1; dst[off + 1] = l0; dst[off + 2] = m0; dst[off + 3] = tl;
        off += stride;
        dst[off + 0] = m2; dst[off + 1] = l1; dst[off + 2] = m1; dst[off + 3] = l0;
        off += stride;
        dst[off + 0] = m3; dst[off + 1] = l2; dst[off + 2] = m2; dst[off + 3] = l1;
    }
    pub fn ipred_hu(dst: &mut [u8], mut off: usize, stride: usize, ipred: &IPredContext) {
        let (_, m1, m2, m3) = load_pred4!(leftavg; ipred);
        let (_, l1, l2, _)  = load_pred4!(left;    ipred);
        let l3 = (((ipred.left[2] as u16) + (ipred.left[3] as u16) * 3 + 2) >> 2) as u8;
        let p3 = ipred.left[3];

        dst[off + 0] = m1; dst[off + 1] = l1; dst[off + 2] = m2; dst[off + 3] = l2;
        off += stride;
        dst[off + 0] = m2; dst[off + 1] = l2; dst[off + 2] = m3; dst[off + 3] = l3;
        off += stride;
        dst[off + 0] = m3; dst[off + 1] = l3; dst[off + 2] = p3; dst[off + 3] = p3;
        off += stride;
        dst[off + 0] = p3; dst[off + 1] = p3; dst[off + 2] = p3; dst[off + 3] = p3;
    }
}

fn delta(p1: i16, p0: i16, q0: i16, q1: i16) -> i16 {
    (p1 - q1) + 3 * (q0 - p0)
}

pub type LoopFilterFunc = fn(buf: &mut [u8], off: usize, step: usize, stride: usize, len: usize, thr: i16, thr_inner: i16, thr_hev: i16);

pub fn simple_loop_filter(buf: &mut [u8], mut off: usize, step: usize, stride: usize, len: usize, thr: i16, _thr_inner: i16, _thr_hev: i16) {
    for _ in 0..len {
        let p1 = buf[off - step * 2] as i16;
        let p0 = buf[off - step * 1] as i16;
        let q0 = buf[off + step * 0] as i16;
        let q1 = buf[off + step * 1] as i16;
        let dpq = p0 - q0;
        if dpq.abs() < thr {
            let diff = delta(p1, p0, q0, q1);
            let diffq0 = (diff.min(127) + 4) >> 3;
            let diffp0 = diffq0 - if (diff & 7) == 4 { 1 } else { 0 };
            buf[off - step * 1] = clip_u8(p0 + diffp0);
            buf[off + step * 0] = clip_u8(q0 - diffq0);
        }
        off += stride;
    }
}

fn normal_loop_filter(buf: &mut [u8], mut off: usize, step: usize, stride: usize, len: usize, thr: i16, thr_inner: i16, thr_hev: i16, edge: bool) {
    for _ in 0..len {
        let p0 = buf[off - step * 1] as i16;
        let q0 = buf[off + step * 0] as i16;
        let dpq = p0 - q0;
        if dpq.abs() <= thr {
            let p3 = buf[off - step * 4] as i16;
            let p2 = buf[off - step * 3] as i16;
            let p1 = buf[off - step * 2] as i16;
            let q1 = buf[off + step * 1] as i16;
            let q2 = buf[off + step * 2] as i16;
            let q3 = buf[off + step * 3] as i16;
            let dp2 = p3 - p2;
            let dp1 = p2 - p1;
            let dp0 = p1 - p0;
            let dq0 = q1 - q0;
            let dq1 = q2 - q1;
            let dq2 = q3 - q2;
            if (dp0.abs() <= thr_inner) && (dp1.abs() <= thr_inner) &&
               (dp2.abs() <= thr_inner) && (dq0.abs() <= thr_inner) &&
               (dq1.abs() <= thr_inner) && (dq2.abs() <= thr_inner) {
                let high_edge_variation = (dp0.abs() > thr_hev) || (dq0.abs() > thr_hev);
                if high_edge_variation {
                    let diff = delta(p1, p0, q0, q1);
                    let diffq0 = (diff.min(127) + 4) >> 3;
                    let diffp0 = diffq0 - if (diff & 7) == 4 { 1 } else { 0 };
                    buf[off - step * 1] = clip_u8(p0 + diffp0);
                    buf[off + step * 0] = clip_u8(q0 - diffq0);
                } else if edge {
                    let d = delta(p1, p0, q0, q1);
                    let diff0 = (d * 27 + 63) >> 7;
                    buf[off - step * 1] = clip_u8(p0 + diff0);
                    buf[off + step * 0] = clip_u8(q0 - diff0);
                    let diff1 = (d * 18 + 63) >> 7;
                    buf[off - step * 2] = clip_u8(p1 + diff1);
                    buf[off + step * 1] = clip_u8(q1 - diff1);
                    let diff2 = (d * 9 + 63) >> 7;
                    buf[off - step * 3] = clip_u8(p2 + diff2);
                    buf[off + step * 2] = clip_u8(q2 - diff2);
                } else {
                    let diff = 3 * (q0 - p0);
                    let diffq0 = (diff.min(127) + 4) >> 3;
                    let diffp0 = diffq0 - if (diff & 7) == 4 { 1 } else { 0 };
                    buf[off - step * 1] = clip_u8(p0 + diffp0);
                    buf[off + step * 0] = clip_u8(q0 - diffq0);
                    let diff2 = (diffq0 + 1) >> 1;
                    buf[off - step * 2] = clip_u8(p1 + diff2);
                    buf[off + step * 1] = clip_u8(q1 - diff2);
                }
            }
        }
        off += stride;
    }
}

pub fn normal_loop_filter_inner(buf: &mut [u8], off: usize, step: usize, stride: usize, len: usize, thr: i16, thr_inner: i16, thr_hev: i16) {
    normal_loop_filter(buf, off, step, stride, len, thr, thr_inner, thr_hev, false);
}

pub fn normal_loop_filter_edge(buf: &mut [u8], off: usize, step: usize, stride: usize, len: usize, thr: i16, thr_inner: i16, thr_hev: i16) {
    normal_loop_filter(buf, off, step, stride, len, thr, thr_inner, thr_hev, true);
}

const VP7_BICUBIC_FILTERS: [[i16; 6]; 8] = [
    [ 0,   0, 128,   0,   0, 0 ],
    [ 0,  -6, 123,  12,  -1, 0 ],
    [ 2, -11, 108,  36,  -8, 1 ],
    [ 0,  -9,  93,  50,  -6, 0 ],
    [ 3, -16,  77,  77, -16, 3 ],
    [ 0,  -6,  50,  93,  -9, 0 ],
    [ 1,  -8,  36, 108, -11, 2 ],
    [ 0,  -1,  12, 123,  -6, 0 ]
];

macro_rules! interpolate {
    ($src: expr, $off: expr, $step: expr, $mode: expr) => {{
        let s0 = $src[$off + 0 * $step] as i32;
        let s1 = $src[$off + 1 * $step] as i32;
        let s2 = $src[$off + 2 * $step] as i32;
        let s3 = $src[$off + 3 * $step] as i32;
        let s4 = $src[$off + 4 * $step] as i32;
        let s5 = $src[$off + 5 * $step] as i32;
        let filt = &VP7_BICUBIC_FILTERS[$mode];
        let src = [s0, s1, s2, s3, s4, s5];
        let mut val = 64;
        for (s, c) in src.iter().zip(filt.iter()) {
            val += s * (*c as i32);
        }
        clip_u8((val >> 7) as i16)
    }}
}

const EDGE_PRE: usize = 2;
const EDGE_POST: usize = 4;
const TMP_STRIDE: usize = 16;

fn mc_block_common(dst: &mut [u8], mut doff: usize, dstride: usize, src: &[u8], sstride: usize, size: usize, mx: usize, my: usize) {
    if (mx == 0) && (my == 0) {
        let dst = &mut dst[doff..];
        let src = &src[EDGE_PRE + EDGE_PRE * sstride..];
        for (out, src) in dst.chunks_mut(dstride).take(size).zip(src.chunks(sstride)) {
            (&mut out[0..size]).copy_from_slice(&src[0..size]);
        }
    } else if my == 0 {
        let src = &src[EDGE_PRE * sstride..];
        for src in src.chunks(sstride).take(size) {
            for x in 0..size {
                dst[doff + x] = interpolate!(src, x, 1, mx);
            }
            doff += dstride;
        }
    } else if mx == 0 {
        let src = &src[EDGE_PRE..];
        for y in 0..size {
            for x in 0..size {
                dst[doff + x] = interpolate!(src, x + y * sstride, sstride, my);
            }
            doff += dstride;
        }
    } else {
        let mut tmp = [0u8; TMP_STRIDE * (16 + EDGE_PRE + EDGE_POST)];
        for (y, dst) in tmp.chunks_mut(TMP_STRIDE).take(size + EDGE_PRE + EDGE_POST).enumerate() {
            for x in 0..size {
                dst[x] = interpolate!(src, x + y * sstride, 1, mx);
            }
        }
        for y in 0..size {
            for x in 0..size {
                dst[doff + x] = interpolate!(tmp, x + y * TMP_STRIDE, TMP_STRIDE, my);
            }
            doff += dstride;
        }
    }
}
fn mc_block(dst: &mut [u8], doff: usize, dstride: usize, xpos: usize, ypos: usize,
            mvx: i16, mvy: i16, reffrm: NAVideoBufferRef<u8>, plane: usize,
            mc_buf: &mut [u8], size: usize) {
    if (mvx == 0) && (mvy == 0) {
        let dst = &mut dst[doff..];
        let sstride = reffrm.get_stride(plane);
        let srcoff = reffrm.get_offset(plane) + xpos + ypos * sstride;
        let src = &reffrm.get_data();
        let src = &src[srcoff..];
        for (out, src) in dst.chunks_mut(dstride).take(size).zip(src.chunks(sstride)) {
            (&mut out[0..size]).copy_from_slice(&src[0..size]);
        }
        return;
    }
    let (w, h) = reffrm.get_dimensions(plane);
    let wa = if plane == 0 { ((w + 15) & !15) } else { ((w + 7) & !7) } as isize;
    let ha = if plane == 0 { ((h + 15) & !15) } else { ((h + 7) & !7) } as isize;
    let bsize = (size as isize) + (EDGE_PRE as isize) + (EDGE_POST as isize);
    let ref_x = (xpos as isize) + ((mvx >> 3) as isize) - (EDGE_PRE as isize);
    let ref_y = (ypos as isize) + ((mvy >> 3) as isize) - (EDGE_PRE as isize);

    let (src, sstride) = if (ref_x < 0) || (ref_x + bsize > wa) || (ref_y < 0) || (ref_y + bsize > ha) {
            edge_emu(&reffrm, ref_x, ref_y, bsize as usize, bsize as usize, mc_buf, 32, plane);
            (mc_buf as &[u8], 32)
        } else {
            let off     = reffrm.get_offset(plane);
            let stride  = reffrm.get_stride(plane);
            let data    = reffrm.get_data();
            (&data[off + (ref_x as usize) + (ref_y as usize) * stride..], stride)
        };
    let mx = (mvx & 7) as usize;
    let my = (mvy & 7) as usize;
    mc_block_common(dst, doff, dstride, src, sstride, size, mx, my);
}
pub fn mc_block16x16(dst: &mut [u8], doff: usize, dstride: usize, xpos: usize, ypos: usize,
                     mvx: i16, mvy: i16, src: NAVideoBufferRef<u8>, plane: usize, mc_buf: &mut [u8]) {
    mc_block(dst, doff, dstride, xpos, ypos, mvx, mvy, src, plane, mc_buf, 16);
}
pub fn mc_block8x8(dst: &mut [u8], doff: usize, dstride: usize, xpos: usize, ypos: usize,
                   mvx: i16, mvy: i16, src: NAVideoBufferRef<u8>, plane: usize, mc_buf: &mut [u8]) {
    mc_block(dst, doff, dstride, xpos, ypos, mvx, mvy, src, plane, mc_buf, 8);
}
pub fn mc_block4x4(dst: &mut [u8], doff: usize, dstride: usize, xpos: usize, ypos: usize,
                   mvx: i16, mvy: i16, src: NAVideoBufferRef<u8>, plane: usize, mc_buf: &mut [u8]) {
    mc_block(dst, doff, dstride, xpos, ypos, mvx, mvy, src, plane, mc_buf, 4);
}
pub fn mc_block_special(dst: &mut [u8], doff: usize, dstride: usize, xpos: usize, ypos: usize,
                        mvx: i16, mvy: i16, reffrm: NAVideoBufferRef<u8>, plane: usize,
                        mc_buf: &mut [u8], size: usize, pitch_mode: u8) {
    const Y_MUL: [isize; 8] = [ 1, 0, 2, 4, 1,  1, 2,  2 ];
    const Y_OFF: [isize; 8] = [ 0, 4, 0, 0, 1, -1, 1, -1 ];
    const ILACE_CHROMA: [bool; 8] = [ false, false, true, true, false, false, true, true ]; // mode&2 != 0

    let pitch_mode = (pitch_mode & 7) as usize;
    let (xstep, ymul) = if plane == 0 {
            (Y_OFF[pitch_mode], Y_MUL[pitch_mode])
        } else {
            (0, if ILACE_CHROMA[pitch_mode] { 2 } else { 1 })
        };

    let (w, h) = reffrm.get_dimensions(plane);
    let wa = if plane == 0 { ((w + 15) & !15) } else { ((w + 7) & !7) } as isize;
    let ha = if plane == 0 { ((h + 15) & !15) } else { ((h + 7) & !7) } as isize;
    let mut start_x = (xpos as isize) + ((mvx >> 3) as isize) - (EDGE_PRE as isize);
    let mut end_x   = (xpos as isize) + ((mvx >> 3) as isize) + ((size + EDGE_POST) as isize);
    if xstep < 0 {
        start_x -= (size + EDGE_POST) as isize;
    } else if xstep > 0 {
        end_x += (size as isize) * xstep;
    }
    let mut start_y = (ypos as isize) + ((mvy >> 3) as isize) - (EDGE_PRE as isize) * ymul;
    let mut end_y   = (ypos as isize) + ((mvy >> 3) as isize) + ((size + EDGE_POST) as isize) * ymul;
    if ymul == 0 {
        start_y -= EDGE_PRE as isize;
        end_y   += (EDGE_POST + 1) as isize;
    }
    let off     = reffrm.get_offset(plane);
    let stride  = reffrm.get_stride(plane);
    let (src, sstride) = if (start_x >= 0) && (end_x <= wa) && (start_y >= 0) && (end_y <= ha) {
            let data    = reffrm.get_data();
            (&data[off + (start_x as usize) + (start_y as usize) * stride..],
             ((stride as isize) + xstep) as usize)
        } else {
            let add = (size + EDGE_PRE + EDGE_POST) * (xstep.abs() as usize);
            let bw = size + EDGE_PRE + EDGE_POST + add;
            let bh = (end_y - start_y) as usize;
            let bo = if xstep >= 0 { 0 } else { add };
            edge_emu(&reffrm, start_x + (bo as isize), start_y, bw, bh, mc_buf, 128, plane);
            (&mc_buf[bo..], (128 + xstep) as usize)
        };
    let mx = (mvx & 7) as usize;
    let my = (mvy & 7) as usize;
    match ymul {
        0 => unimplemented!(),
        1 => mc_block_common(dst, doff, dstride, src, sstride, size, mx, my),
        2 => {
            let hsize = size / 2;
            for y in 0..2 {
                for x in 0..2 {
                    mc_block_common(dst, doff + x * hsize + y * hsize * dstride, dstride,
                                    &src[x * hsize + y * sstride..], sstride * 2, hsize, mx, my);
                }
            }
        },
        4 => {
            let qsize = size / 4;
            for y in 0..4 {
                for x in 0..4 {
                    mc_block_common(dst, doff + x * qsize + y * qsize * dstride, dstride,
                                    &src[x * qsize + y * sstride..], sstride * 4, qsize, mx, my);
                }
            }
        },
        _ => unreachable!(),
    };
}

pub fn fade_frame(srcfrm: NAVideoBufferRef<u8>, dstfrm: &mut NASimpleVideoFrame<u8>, alpha: u16, beta: u16) {
    let mut fade_lut = [0u8; 256];
    for (i, el) in fade_lut.iter_mut().enumerate() {
        let y = i as u16;
        *el = (y + ((y * beta) >> 8) + alpha).max(0).min(255) as u8;
    }

    let (w, h)  = srcfrm.get_dimensions(0);
    let (wa, ha) = ((w + 15) & !15, (h + 15) & !15);
    let soff    = srcfrm.get_offset(0);
    let sstride = srcfrm.get_stride(0);
    let sdata   = srcfrm.get_data();
    let src = &sdata[soff..];
    let dstride = dstfrm.stride[0];
    let dst = &mut dstfrm.data[dstfrm.offset[0]..];
    for (src, dst) in src.chunks(sstride).zip(dst.chunks_mut(dstride)).take(ha) {
        for (s, d) in src.iter().zip(dst.iter_mut()).take(wa) {
            *d = fade_lut[*s as usize];
        }
    }

    for plane in 1..3 {
        let (w, h)  = srcfrm.get_dimensions(plane);
        let (wa, ha) = ((w + 7) & !7, (h + 7) & !7);
        let soff    = srcfrm.get_offset(plane);
        let sstride = srcfrm.get_stride(plane);
        let sdata   = srcfrm.get_data();
        let src = &sdata[soff..];
        let dstride = dstfrm.stride[plane];
        let dst = &mut dstfrm.data[dstfrm.offset[plane]..];
        for (src, dst) in src.chunks(sstride).zip(dst.chunks_mut(dstride)).take(ha) {
            (&mut dst[0..wa]).copy_from_slice(&src[0..wa]);
        }
    }
}
