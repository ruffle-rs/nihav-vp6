use nihav_core::codecs::*;
use nihav_core::codecs::blockdsp::*;

#[derive(Clone,Copy,Debug,PartialEq)]
#[allow(dead_code)]
pub enum VPMBType {
    Intra,
    InterNoMV,
    InterMV,
    InterNearest,
    InterNear,
    InterFourMV,
    GoldenNoMV,
    GoldenMV,
    GoldenNearest,
    GoldenNear,
}

pub const VP_REF_INTER: u8 = 1;
pub const VP_REF_GOLDEN: u8 = 2;

#[allow(dead_code)]
impl VPMBType {
    pub fn is_intra(self) -> bool { self == VPMBType::Intra }
    pub fn get_ref_id(self) -> u8 {
        match self {
            VPMBType::Intra         => 0,
            VPMBType::InterNoMV     |
            VPMBType::InterMV       |
            VPMBType::InterNearest  |
            VPMBType::InterNear     |
            VPMBType::InterFourMV   => VP_REF_INTER,
            _                       => VP_REF_GOLDEN,
        }
    }
}

impl Default for VPMBType {
    fn default() -> Self { VPMBType::Intra }
}

#[derive(Default)]
pub struct VPShuffler {
    lastframe: Option<NAVideoBufferRef<u8>>,
    goldframe: Option<NAVideoBufferRef<u8>>,
}

impl VPShuffler {
    pub fn new() -> Self { VPShuffler { lastframe: None, goldframe: None } }
    pub fn clear(&mut self) { self.lastframe = None; self.goldframe = None; }
    pub fn add_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.lastframe = Some(buf);
    }
    pub fn add_golden_frame(&mut self, buf: NAVideoBufferRef<u8>) {
        self.goldframe = Some(buf);
    }
    pub fn get_last(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    pub fn get_golden(&mut self) -> Option<NAVideoBufferRef<u8>> {
        if let Some(ref frm) = self.goldframe {
            Some(frm.clone())
        } else {
            None
        }
    }
    pub fn has_refs(&self) -> bool {
        self.lastframe.is_some()
    }
}

pub const VP56_COEF_BASE: [i16; 6] = [ 5, 7, 11, 19, 35, 67 ];
pub const VP56_COEF_ADD_PROBS: [[u8; 12]; 6] = [
    [ 159, 128,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0 ],
    [ 165, 145, 128,   0,   0,   0,   0,   0,   0,   0,   0,   0 ],
    [ 173, 148, 140, 128,   0,   0,   0,   0,   0,   0,   0,   0 ],
    [ 176, 155, 140, 135, 128,   0,   0,   0,   0,   0,   0,   0 ],
    [ 180, 157, 141, 134, 130, 128,   0,   0,   0,   0,   0,   0 ],
    [ 254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129, 128 ],
];

#[allow(dead_code)]
pub struct BoolCoder<'a> {
    pub src:    &'a [u8],
    pos:    usize,
    value:  u32,
    range:  u32,
    bits:   i32,
}

#[allow(dead_code)]
impl<'a> BoolCoder<'a> {
    pub fn new(src: &'a [u8]) -> DecoderResult<Self> {
        if src.len() < 3 { return Err(DecoderError::ShortData); }
        let value = ((src[0] as u32) << 24) | ((src[1] as u32) << 16) | ((src[2] as u32) << 8) | (src[3] as u32);
        Ok(Self { src, pos: 4, value, range: 255, bits: 8 })
    }
    pub fn read_bool(&mut self) -> bool {
        self.read_prob(128)
    }
    pub fn read_prob(&mut self, prob: u8) -> bool {
        self.renorm();
        let split = 1 + (((self.range - 1) * (prob as u32)) >> 8);
        let bit;
        if self.value < (split << 24) {
            self.range = split;
            bit = false;
        } else {
            self.range -= split;
            self.value -= split << 24;
            bit = true;
        }
        bit
    }
    pub fn read_bits(&mut self, bits: u8) -> u32 {
        let mut val = 0u32;
        for _ in 0..bits {
            val = (val << 1) | (self.read_prob(128) as u32);
        }
        val
    }
    pub fn read_byte(&mut self) -> u8 {
        let mut val = 0u8;
        for _ in 0..8 {
            val = (val << 1) | (self.read_prob(128) as u8);
        }
        val
    }
    pub fn read_sbits(&mut self, bits: u8) -> i32 {
        let mut val = if self.read_prob(128) { -1i32 } else { 0i32 };
        for _ in 1..bits {
            val = (val << 1) | (self.read_prob(128) as i32);
        }
        val
    }
    pub fn read_probability(&mut self) -> u8 {
        let val = self.read_bits(7) as u8;
        if val == 0 {
            1
        } else {
            val << 1
        }
    }
    fn renorm(&mut self) {
        let shift = self.range.leading_zeros() & 7;
        self.range <<= shift;
        self.value <<= shift;
        self.bits   -= shift as i32;
        if (self.bits <= 0) && (self.pos < self.src.len()) {
            self.value |= (self.src[self.pos] as u32) << (-self.bits as u8);
            self.pos += 1;
            self.bits += 8;
        }
/*        while self.range < 0x80 {
            self.range <<= 1;
            self.value <<= 1;
            self.bits   -= 1;
            if (self.bits <= 0) && (self.pos < self.src.len()) {
                self.value |= self.src[self.pos] as u32;
                self.pos += 1;
                self.bits = 8;
            }
        }*/
    }
    pub fn skip_bytes(&mut self, nbytes: usize) {
        for _ in 0..nbytes {
            self.value <<= 8;
            if self.pos < self.src.len() {
                self.value |= self.src[self.pos] as u32;
                self.pos += 1;
            }
        }
    }
}

#[allow(dead_code)]
pub fn rescale_prob(prob: u8, weights: &[i16; 2], maxval: i32) -> u8 {
    ((((prob as i32) * (weights[0] as i32) + 128) >> 8) + (weights[1] as i32)).min(maxval).max(1) as u8
}

#[macro_export]
macro_rules! vp_tree {
    ($bc: expr, $prob: expr, $node1: expr, $node2: expr) => {
        if !$bc.read_prob($prob) {
            $node1
        } else {
            $node2
        }
    };
    ($leaf: expr) => { $leaf }
}

const C1S7: i32 = 64277;
const C2S6: i32 = 60547;
const C3S5: i32 = 54491;
const C4S4: i32 = 46341;
const C5S3: i32 = 36410;
const C6S2: i32 = 25080;
const C7S1: i32 = 12785;

fn mul16(a: i32, b: i32) -> i32 {
    (a * b) >> 16
}

macro_rules! idct_step {
    ($s0:expr, $s1:expr, $s2:expr, $s3:expr, $s4:expr, $s5:expr, $s6:expr, $s7:expr,
     $d0:expr, $d1:expr, $d2:expr, $d3:expr, $d4:expr, $d5:expr, $d6:expr, $d7:expr,
     $bias:expr, $shift:expr, $otype:ty) => {
        let t_a  = mul16(C1S7, i32::from($s1)) + mul16(C7S1, i32::from($s7));
        let t_b  = mul16(C7S1, i32::from($s1)) - mul16(C1S7, i32::from($s7));
        let t_c  = mul16(C3S5, i32::from($s3)) + mul16(C5S3, i32::from($s5));
        let t_d  = mul16(C3S5, i32::from($s5)) - mul16(C5S3, i32::from($s3));
        let t_a1 = mul16(C4S4, t_a - t_c);
        let t_b1 = mul16(C4S4, t_b - t_d);
        let t_c  = t_a + t_c;
        let t_d  = t_b + t_d;
        let t_e  = mul16(C4S4, i32::from($s0 + $s4)) + $bias;
        let t_f  = mul16(C4S4, i32::from($s0 - $s4)) + $bias;
        let t_g  = mul16(C2S6, i32::from($s2)) + mul16(C6S2, i32::from($s6));
        let t_h  = mul16(C6S2, i32::from($s2)) - mul16(C2S6, i32::from($s6));
        let t_e1 = t_e  - t_g;
        let t_g  = t_e  + t_g;
        let t_a  = t_f  + t_a1;
        let t_f  = t_f  - t_a1;
        let t_b  = t_b1 - t_h;
        let t_h  = t_b1 + t_h;

        $d0 = ((t_g  + t_c) >> $shift) as $otype;
        $d7 = ((t_g  - t_c) >> $shift) as $otype;
        $d1 = ((t_a  + t_h) >> $shift) as $otype;
        $d2 = ((t_a  - t_h) >> $shift) as $otype;
        $d3 = ((t_e1 + t_d) >> $shift) as $otype;
        $d4 = ((t_e1 - t_d) >> $shift) as $otype;
        $d5 = ((t_f  + t_b) >> $shift) as $otype;
        $d6 = ((t_f  - t_b) >> $shift) as $otype;
    }
}

pub fn vp_idct(coeffs: &mut [i16; 64]) {
    let mut tmp = [0i32; 64];
    for (src, dst) in coeffs.chunks(8).zip(tmp.chunks_mut(8)) {
        idct_step!(src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
                   dst[0], dst[1], dst[2], dst[3], dst[4], dst[5], dst[6], dst[7], 0, 0, i32);
    }
    let src = &tmp;
    let dst = coeffs;
    for i in 0..8 {
        idct_step!(src[0 * 8 + i], src[1 * 8 + i], src[2 * 8 + i], src[3 * 8 + i],
                   src[4 * 8 + i], src[5 * 8 + i], src[6 * 8 + i], src[7 * 8 + i],
                   dst[0 * 8 + i], dst[1 * 8 + i], dst[2 * 8 + i], dst[3 * 8 + i],
                   dst[4 * 8 + i], dst[5 * 8 + i], dst[6 * 8 + i], dst[7 * 8 + i], 8, 4, i16);
    }
}

pub fn vp_idct_dc(coeffs: &mut [i16; 64]) {
    let dc = ((mul16(C4S4, mul16(C4S4, i32::from(coeffs[0]))) + 8) >> 4) as i16;
    for i in 0..64 {
        coeffs[i] = dc;
    }
}

pub fn unquant(coeffs: &mut [i16; 64], qmat: &[i16; 64]) {
    for i in 1..64 {
        coeffs[i] = coeffs[i].wrapping_mul(qmat[i]);
    }
}

pub fn vp_put_block(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct(coeffs);
    let mut off = frm.offset[plane] + bx * 8 + by * 8 * frm.stride[plane];
    for y in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = (coeffs[x + y * 8] + 128).min(255).max(0) as u8;
        }
        off += frm.stride[plane];
    }
}

pub fn vp_put_block_ilace(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct(coeffs);
    let mut off = frm.offset[plane] + bx * 8 + ((by & !1) * 8 + (by & 1)) * frm.stride[plane];
    for y in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = (coeffs[x + y * 8] + 128).min(255).max(0) as u8;
        }
        off += frm.stride[plane] * 2;
    }
}

pub fn vp_put_block_dc(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct_dc(coeffs);
    let dc = (coeffs[0] + 128).min(255).max(0) as u8;
    let mut off = frm.offset[plane] + bx * 8 + by * 8 * frm.stride[plane];
    for _ in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = dc;
        }
        off += frm.stride[plane];
    }
}

pub fn vp_add_block(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct(coeffs);
    let mut off = frm.offset[plane] + bx * 8 + by * 8 * frm.stride[plane];
    for y in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = (coeffs[x + y * 8] + (frm.data[off + x] as i16)).min(255).max(0) as u8;
        }
        off += frm.stride[plane];
    }
}

pub fn vp_add_block_ilace(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct(coeffs);
    let mut off = frm.offset[plane] + bx * 8 + ((by & !1) * 8 + (by & 1)) * frm.stride[plane];
    for y in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = (coeffs[x + y * 8] + (frm.data[off + x] as i16)).min(255).max(0) as u8;
        }
        off += frm.stride[plane] * 2;
    }
}

pub fn vp_add_block_dc(coeffs: &mut [i16; 64], bx: usize, by: usize, plane: usize, frm: &mut NASimpleVideoFrame<u8>) {
    vp_idct_dc(coeffs);
    let dc = coeffs[0];
    let mut off = frm.offset[plane] + bx * 8 + by * 8 * frm.stride[plane];
    for _ in 0..8 {
        for x in 0..8 {
            frm.data[off + x] = (dc + (frm.data[off + x] as i16)).min(255).max(0) as u8;
        }
        off += frm.stride[plane];
    }
}

pub fn vp31_loop_filter(data: &mut [u8], mut off: usize, step: usize, stride: usize,
                        len: usize, loop_str: i16) {
    for _ in 0..len {
        let a = data[off - step * 2] as i16;
        let b = data[off - step] as i16;
        let c = data[off] as i16;
        let d = data[off + step] as i16;
        let mut diff = ((a - d) + 3 * (c - b) + 4) >> 3;
        if diff.abs() >= 2 * loop_str {
            diff = 0;
        } else if diff.abs() >= loop_str {
            if diff < 0 {
                diff = -diff - 2 * loop_str;
            } else {
                diff = -diff + 2 * loop_str;
            }
        }
        if diff != 0 {
            data[off - step] = (b + diff).max(0).min(255) as u8;
            data[off]        = (c - diff).max(0).min(255) as u8;
        }

        off += stride;
    }
}

pub fn vp_copy_block(dst: &mut NASimpleVideoFrame<u8>, src: NAVideoBufferRef<u8>, comp: usize,
                     dx: usize, dy: usize, mv_x: i16, mv_y: i16,
                     preborder: usize, postborder: usize, loop_str: i16,
                     mode: usize, interp: &[BlkInterpFunc], mut mc_buf: NAVideoBufferRef<u8>)
{
    let sx = (dx as isize) + (mv_x as isize);
    let sy = (dy as isize) + (mv_y as isize);
    if ((sx | sy) & 7) == 0 {
        copy_block(dst, src, comp, dx, dy, mv_x, mv_y, 8, 8, preborder, postborder, mode, interp);
        return;
    }
    let pre = preborder.max(2);
    let post = postborder.max(1);
    let bsize = 8 + pre + post;
    let src_x = sx - (pre as isize);
    let src_y = sy - (pre as isize);
    {
        let mut tmp_buf = NASimpleVideoFrame::from_video_buf(&mut mc_buf).unwrap();
        copy_block(&mut tmp_buf, src, comp, 0, 0, src_x as i16, src_y as i16,
                   bsize, bsize, 0, 0, 0, interp);
        if (sy & 7) != 0 {
            let foff = (8 - (sy & 7)) as usize;
            let off = (pre + foff) * tmp_buf.stride[comp];
            vp31_loop_filter(tmp_buf.data, off, tmp_buf.stride[comp], 1, bsize, loop_str);
        }
        if (sx & 7) != 0 {
            let foff = (8 - (sx & 7)) as usize;
            let off = pre + foff;
            vp31_loop_filter(tmp_buf.data, off, 1, tmp_buf.stride[comp], bsize, loop_str);
        }
    }
    let dxoff = (pre as i16) - (dx as i16);
    let dyoff = (pre as i16) - (dy as i16);
    copy_block(dst, mc_buf, comp, dx, dy, dxoff, dyoff, 8, 8, preborder, postborder, 0/* mode*/, interp);
}

fn vp3_interp00(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = src[sidx + x]; }
        didx += dstride;
        sidx += sstride;
    }
}

fn vp3_interp01(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = (((src[sidx + x] as u16) + (src[sidx + x + 1] as u16)) >> 1) as u8; }
        didx += dstride;
        sidx += sstride;
    }
}

fn vp3_interp10(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw { dst[didx + x] = (((src[sidx + x] as u16) + (src[sidx + x + sstride] as u16)) >> 1) as u8; }
        didx += dstride;
        sidx += sstride;
    }
}

fn vp3_interp1x(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            dst[didx + x] = (((src[sidx + x] as u16) +
                              (src[sidx + x + sstride + 1] as u16)) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

fn vp3_interp1y(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            dst[didx + x] = (((src[sidx + x + 1] as u16) +
                              (src[sidx + x + sstride] as u16)) >> 1) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

pub const VP3_INTERP_FUNCS: &[blockdsp::BlkInterpFunc] = &[ vp3_interp00, vp3_interp01, vp3_interp10, vp3_interp1x, vp3_interp1y ];

