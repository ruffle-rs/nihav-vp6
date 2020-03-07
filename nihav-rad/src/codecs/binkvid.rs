use std::f32::consts;
use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use nihav_core::io::bitreader::*;
use nihav_core::io::codebook::*;
use nihav_codec_support::codecs::{IPShuffler, HAMShuffler};

const SKIP_BLOCK: u8 = 0;
const SCALED_BLOCK: u8 = 1;
const MOTION_BLOCK: u8 = 2;
const RUN_BLOCK: u8 = 3;
const RESIDUE_BLOCK: u8 = 4;
const INTRA_BLOCK: u8 = 5;
const FILL_BLOCK: u8 = 6;
const INTER_BLOCK: u8 = 7;
const PATTERN_BLOCK: u8 = 8;
const RAW_BLOCK: u8 = 9;

#[derive(Default, Clone,Copy)]
struct Tree {
    id:     usize,
    syms:   [u8; 16],
}

impl Tree {
    fn read_desc(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.id                                 = br.read(4)? as usize;
        if self.id == 0 {
            for i in 0..16 { self.syms[i] = i as u8; }
        } else {
            if br.read_bool()? {
                let len                         = br.read(3)? as usize;
                let mut present: [bool; 16] = [false; 16];
                for i in 0..=len {
                    self.syms[i]                = br.read(4)? as u8;
                    present[self.syms[i] as usize] = true;
                }
                let mut idx = len + 1;
                for i in 0..16 {
                    if present[i] { continue; }
                    self.syms[idx] = i as u8;
                    idx += 1;
                }
            } else {
                let len                         = br.read(2)? as usize;
                let mut syms: [u8; 16] = [ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
                let mut tmp: [u8; 16] = [0; 16];
                for bits in 0..=len {
                    let size = 1 << bits;
                    for arr in syms.chunks_mut(size * 2) {
                        let mut ptr0 = 0;
                        let mut ptr1 = size;
                        let mut optr = 0;
                        while (ptr0 < size) && (ptr1 < size * 2) {
                            if !br.read_bool()? {
                                tmp[optr] = arr[ptr0];
                                ptr0 += 1;
                            } else {
                                tmp[optr] = arr[ptr1];
                                ptr1 += 1;
                            }
                            optr += 1;
                        }
                        while ptr0 < size {
                            tmp[optr] = arr[ptr0];
                            ptr0 += 1;
                            optr += 1;
                        }
                        while ptr1 < size * 2 {
                            tmp[optr] = arr[ptr1];
                            ptr1 += 1;
                            optr += 1;
                        }
                        arr.copy_from_slice(&tmp[0..size * 2]);
                    }
                }
                self.syms = syms;
            }
        }
        Ok(())
    }
    fn read_sym(&self, br: &mut BitReader, trees: &BinkTrees) -> DecoderResult<u8> {
        let idx                                 = br.read_cb(&trees.cb[self.id])?;
        Ok(self.syms[idx as usize])
    }
}

#[derive(Default)]
struct Bundle<T: Copy> {
    tree:       Tree,
    data:       Vec<T>,
    dec_pos:    usize,
    read_pos:   usize,
    bits:       u8,
}

impl<T:Copy> Bundle<T> {
    fn binkb_reset(&mut self, bits: u8) {
        self.bits       = bits;
        self.dec_pos    = 0;
        self.read_pos   = 0;
    }
    fn reset(&mut self) {
        self.dec_pos = 0;
        self.read_pos = 0;
    }
    fn read_desc(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.dec_pos = 0;
        self.read_pos = 0;
        self.tree.read_desc(br)?;
        Ok(())
    }
    fn read_len(&mut self, br: &mut BitReader) -> DecoderResult<usize> {
        if self.read_pos < self.dec_pos { return Ok(0); }
        let len                                 = br.read(self.bits)? as usize;
        if len == 0 {
            self.dec_pos = self.data.len();
            self.read_pos = self.data.len() - 1;
        }
        Ok(len)
    }
    fn read_len_binkb(&mut self, br: &mut BitReader) -> DecoderResult<usize> {
        if self.read_pos < self.dec_pos { return Ok(0); }
        let len                                 = br.read(13)? as usize;
        if len == 0 {
            self.dec_pos = self.data.len();
            self.read_pos = self.data.len() - 1;
        }
        Ok(len)
    }
    fn get_val(&mut self) -> DecoderResult<T> {
        validate!(self.read_pos < self.dec_pos);
        let val = self.data[self.read_pos];
        self.read_pos += 1;
        Ok(val)
    }
}

const BLOCK_TYPE_RUNS: [usize; 4] = [ 4, 8, 12, 32 ];
impl Bundle<u8> {
    fn read_binkb(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let len = self.read_len_binkb(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        for i in 0..len {
            self.data[self.dec_pos + i]         = br.read(self.bits)? as u8;
        }
        self.dec_pos += len;
        Ok(())
    }
    fn read_runs(&mut self, br: &mut BitReader, trees: &BinkTrees) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        if br.read_bool()? {
            let val                             = br.read(4)? as u8;
            for i in 0..len { self.data[self.dec_pos + i] = val; }
            self.dec_pos += len;
        } else {
            while self.dec_pos < end {
                self.data[self.dec_pos] = self.tree.read_sym(br, trees)?;
                self.dec_pos += 1;
            }
        }
        Ok(())
    }
    fn read_block_types(&mut self, br: &mut BitReader, trees: &BinkTrees) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        if br.read_bool()? {
            let val                             = br.read(4)? as u8;
            for i in 0..len { self.data[self.dec_pos + i] = val; }
            self.dec_pos += len;
        } else {
            let mut last = 0;
            while self.dec_pos < end {
                let val = self.tree.read_sym(br, trees)?;
                if val < 12 {
                    self.data[self.dec_pos] = val;
                    self.dec_pos += 1;
                    last = val;
                } else {
                    let run = BLOCK_TYPE_RUNS[(val - 12) as usize];
                    validate!(self.dec_pos + run <= end);
                    for i in 0..run {
                        self.data[self.dec_pos + i] = last;
                    }
                    self.dec_pos += run;
                }
            }
        }
        Ok(())
    }
    fn read_patterns(&mut self, br: &mut BitReader, trees: &BinkTrees) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        for i in 0..len {
            let pat_lo = self.tree.read_sym(br, trees)?;
            let pat_hi = self.tree.read_sym(br, trees)?;
            self.data[self.dec_pos + i] = pat_lo | (pat_hi << 4);
        }
        self.dec_pos += len;
        Ok(())
    }
    fn cvt_color(lo: u8, hi: u8, new_bink: bool) -> u8 {
        let val = lo | (hi << 4);
        if !new_bink {
            let sign = ((val as i8) >> 7) as u8;
            ((val & 0x7F) ^ sign).wrapping_sub(sign) ^ 0x80
        } else {
            val
        }
    }
    fn read_colors(&mut self, br: &mut BitReader, trees: &BinkTrees, col_hi: &[Tree; 16], col_last: &mut u8, new_bink: bool) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        let mut last = *col_last;
        if br.read_bool()? {
            last = col_hi[last as usize].read_sym(br, trees)?;
            let lo = self.tree.read_sym(br, trees)?;
            let val = Self::cvt_color(lo, last, new_bink);
            for i in 0..len { self.data[self.dec_pos + i] = val as u8; }
            self.dec_pos += len;
        } else {
            while self.dec_pos < end {
                last = col_hi[last as usize].read_sym(br, trees)?;
                let lo = self.tree.read_sym(br, trees)?;
                let val = Self::cvt_color(lo, last, new_bink);
                self.data[self.dec_pos] = val;
                self.dec_pos += 1;
            }
        }
        *col_last = last;
        Ok(())
    }
}

impl Bundle<i8> {
    fn read_binkb(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let len = self.read_len_binkb(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        let bias = 1 << (self.bits - 1);
        for i in 0..len {
            self.data[self.dec_pos + i]         = (br.read(self.bits)? as i8) - bias;
        }
        self.dec_pos += len;
        Ok(())
    }
    fn read_motion_values(&mut self, br: &mut BitReader, trees: &BinkTrees) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        if br.read_bool()? {
            let mut val                         = br.read(4)? as i8;
            if val != 0 && br.read_bool()? { val = -val; }
            for i in 0..len { self.data[self.dec_pos + i] = val; }
            self.dec_pos += len;
        } else {
            while self.dec_pos < end {
                self.data[self.dec_pos] = self.tree.read_sym(br, trees)? as i8;
                if self.data[self.dec_pos] != 0 && br.read_bool()? {
                    self.data[self.dec_pos] = -self.data[self.dec_pos];
                }
                self.dec_pos += 1;
            }
        }
        Ok(())
    }
}

const DC_START_BITS: u8 = 11;
impl Bundle<u16> {
    fn read_binkb(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let len = self.read_len_binkb(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        for i in 0..len {
            self.data[self.dec_pos + i]         = br.read(self.bits)? as u16;
        }
        self.dec_pos += len;
        Ok(())
    }
    fn read_dcs(&mut self, br: &mut BitReader, start_bits: u8) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        let mut val                             = br.read(start_bits)? as u16;
        self.data[self.dec_pos] = val;
        self.dec_pos += 1;
        for i in (1..len).step_by(8) {
            let seg_len = (len - i).min(8);
            let bits                            = br.read(4)? as u8;
            if bits != 0 {
                for _ in 0..seg_len {
                    let diff                    = br.read(bits)? as u16;
                    let res = if diff != 0 && br.read_bool()? {
                            val.checked_sub(diff)
                        } else {
                            val.checked_add(diff)
                        };
                    validate!(res.is_some());
                    val = res.unwrap();
                    self.data[self.dec_pos] = val;
                    self.dec_pos += 1;
                }
            } else {
                for _ in 0..seg_len {
                    self.data[self.dec_pos] = val;
                    self.dec_pos += 1;
                }
            }
        }
        Ok(())
    }
}

impl Bundle<i16> {
    fn read_binkb(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let len = self.read_len_binkb(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        let bias = 1 << (self.bits - 1);
        for i in 0..len {
            self.data[self.dec_pos + i]         = (br.read(self.bits)? as i16) - bias;
        }
        self.dec_pos += len;
        Ok(())
    }
    fn read_dcs(&mut self, br: &mut BitReader, start_bits: u8) -> DecoderResult<()> {
        let len = self.read_len(br)?;
        if len == 0 { return Ok(()); }
        let end = self.dec_pos + len;
        validate!(end <= self.data.len());
        let mut val                             = br.read(start_bits - 1)? as i16;
        if val != 0 && br.read_bool()? {
            val = -val;
        }
        self.data[self.dec_pos] = val;
        self.dec_pos += 1;
        for i in (1..len).step_by(8) {
            let seg_len = (len - i).min(8);
            let bits                            = br.read(4)? as u8;
            if bits != 0 {
                for _ in 0..seg_len {
                    let mut diff                = br.read(bits)? as i16;
                    if diff != 0 && br.read_bool()? {
                        diff = -diff;
                    }
                    let res = val.checked_add(diff);
                    validate!(res.is_some());
                    val = res.unwrap();
                    self.data[self.dec_pos] = val;
                    self.dec_pos += 1;
                }
            } else {
                for _ in 0..seg_len {
                    self.data[self.dec_pos] = val;
                    self.dec_pos += 1;
                }
            }
        }
        Ok(())
    }
}

struct BinkTrees {
    cb:         [Codebook<u8>; 16],
}

fn map_u8(idx: usize) -> u8 { idx as u8 }

impl Default for BinkTrees {
    fn default() -> Self {
        let mut cb: [Codebook<u8>; 16];
        unsafe {
            cb = std::mem::uninitialized();
            for i in 0..16 {
                let mut cr = TableCodebookDescReader::new(&BINK_TREE_CODES[i], &BINK_TREE_BITS[i], map_u8);
                std::ptr::write(&mut cb[i], Codebook::new(&mut cr, CodebookMode::LSB).unwrap());
            }
        }
        Self { cb }
    }
}

const A1: i32 =  2896;
const A2: i32 =  2217;
const A3: i32 =  3784;
const A4: i32 = -5352;

macro_rules! idct {
    ($src: expr, $sstep: expr, $dst: expr, $dstep: expr, $off: expr, $bias: expr, $shift: expr) => {
        let a0 = $src[$off + 0 * $sstep] + $src[$off + 4 * $sstep];
        let a1 = $src[$off + 0 * $sstep] - $src[$off + 4 * $sstep];
        let a2 = $src[$off + 2 * $sstep] + $src[$off + 6 * $sstep];
        let a3 = A1.wrapping_mul($src[$off + 2 * $sstep] - $src[$off + 6 * $sstep]) >> 11;
        let a4 = $src[$off + 5 * $sstep] + $src[$off + 3 * $sstep];
        let a5 = $src[$off + 5 * $sstep] - $src[$off + 3 * $sstep];
        let a6 = $src[$off + 1 * $sstep] + $src[$off + 7 * $sstep];
        let a7 = $src[$off + 1 * $sstep] - $src[$off + 7 * $sstep];
        let b0 = a4 + a6;
        let b1 = A3.wrapping_mul(a5 + a7) >> 11;
        let b2 = (A4.wrapping_mul(a5) >> 11) - b0 + b1;
        let b3 = (A1.wrapping_mul(a6 - a4) >> 11) - b2;
        let b4 = (A2.wrapping_mul(a7) >> 11) + b3 - b1;
        let c0 = a0 + a2;
        let c1 = a0 - a2;
        let c2 = a1 + (a3 - a2);
        let c3 = a1 - (a3 - a2);

        $dst[$off + 0 * $dstep] = (c0 + b0 + $bias) >> $shift;
        $dst[$off + 1 * $dstep] = (c2 + b2 + $bias) >> $shift;
        $dst[$off + 2 * $dstep] = (c3 + b3 + $bias) >> $shift;
        $dst[$off + 3 * $dstep] = (c1 - b4 + $bias) >> $shift;
        $dst[$off + 4 * $dstep] = (c1 + b4 + $bias) >> $shift;
        $dst[$off + 5 * $dstep] = (c3 - b3 + $bias) >> $shift;
        $dst[$off + 6 * $dstep] = (c2 - b2 + $bias) >> $shift;
        $dst[$off + 7 * $dstep] = (c0 - b0 + $bias) >> $shift;
    };
}

struct QuantMats {
    intra_qmat: [[i32; 64]; 16],
    inter_qmat: [[i32; 64]; 16],
}

impl QuantMats {
    fn calc_binkb_quants(&mut self) {
        let mut inv_scan: [usize; 64] = [0; 64];
        let mut mod_mat: [f32; 64] = [0.0; 64];
        let base = consts::PI / 16.0;

        for i in 0..64 { inv_scan[BINK_SCAN[i]] = i; }

        for j in 0..8 {
            let j_scale = if (j != 0) && (j != 4) { (base * (j as f32)).cos() * consts::SQRT_2 } else { 1.0 };
            for i in 0..8 {
                let i_scale = if (i != 0) && (i != 4) { (base * (i as f32)).cos() * consts::SQRT_2 } else { 1.0 };
                mod_mat[i + j * 8] = i_scale * j_scale;
            }
        }

        for q in 0..16 {
            let (num, den) = BINKB_REF_QUANTS[q];
            let quant = (num as f32) * ((1 << 12) as f32) / (den as f32);
            for c in 0..64 {
                let idx = inv_scan[c];
                self.intra_qmat[q][idx] = ((BINKB_REF_INTRA_Q[c] as f32) * mod_mat[c] * quant) as i32;
                self.inter_qmat[q][idx] = ((BINKB_REF_INTER_Q[c] as f32) * mod_mat[c] * quant) as i32;
            }
        }
    }
}

impl Default for QuantMats {
    fn default() -> Self {
        Self { intra_qmat: [[0; 64]; 16], inter_qmat: [[0; 64]; 16] }
    }
}

#[derive(Default)]
struct BinkDecoder {
    info:       NACodecInfoRef,
    ips:        IPShuffler,
    hams:       HAMShuffler,

    is_ver_b:   bool,
    is_ver_i:   bool,
    has_alpha:  bool,
    is_gray:    bool,
    swap_uv:    bool,
    key_frame:  bool,

    cur_w:      usize,
    cur_h:      usize,
    cur_plane:  usize,

    colhi_tree: [Tree; 16],
    col_last:   u8,

    btype:      Bundle<u8>,
    sbtype:     Bundle<u8>,
    colors:     Bundle<u8>,
    pattern:    Bundle<u8>,
    xoff:       Bundle<i8>,
    yoff:       Bundle<i8>,
    intradc:    Bundle<u16>,
    interdc:    Bundle<i16>,
    intraq:     Bundle<u8>,
    interq:     Bundle<u8>,
    nresidues:  Bundle<u8>,
    run:        Bundle<u8>,

    trees:      BinkTrees,

    qmat_b:     QuantMats,
}

fn calc_len(size: usize) -> u8 {
    (32 - ((size + 511) as u32).leading_zeros()) as u8
}

impl BinkDecoder {
    fn new() -> Self {
        Self::default()
    }
    fn init_bundle_bufs(&mut self, bw: usize, bh: usize) {
        let size = bw * bh * 64;
        self.btype.data.resize(size, 0);
        self.sbtype.data.resize(size, 0);
        self.colors.data.resize(size, 0);
        self.pattern.data.resize(size, 0);
        self.xoff.data.resize(size, 0);
        self.yoff.data.resize(size, 0);
        self.intradc.data.resize(size, 0);
        self.interdc.data.resize(size, 0);
        self.intraq.data.resize(size, 0);
        self.interq.data.resize(size, 0);
        self.nresidues.data.resize(size, 0);
        self.run.data.resize(size, 0);
    }
    fn init_bundle_lengths(&mut self, w: usize, bw: usize) {
        let w = (w + 7) & !7;
        self.btype.bits     = calc_len(w >> 3);
        self.sbtype.bits    = calc_len(w >> 4);
        self.colors.bits    = calc_len(bw * 64);
        self.pattern.bits   = calc_len(bw * 8);
        self.xoff.bits      = calc_len(w >> 3);
        self.yoff.bits      = calc_len(w >> 3);
        self.intradc.bits   = calc_len(w >> 3);
        self.interdc.bits   = calc_len(w >> 3);
        self.run.bits       = calc_len(bw * 48);
    }
    fn init_bundle_lengths_binkb(&mut self) {
        self.btype.binkb_reset(4);
        self.colors.binkb_reset(8);
        self.pattern.binkb_reset(8);
        self.xoff.binkb_reset(5);
        self.yoff.binkb_reset(5);
        self.intradc.binkb_reset(11);
        self.interdc.binkb_reset(11);
        self.intraq.binkb_reset(4);
        self.interq.binkb_reset(4);
        self.nresidues.binkb_reset(7);
    }
    fn read_bundles_desc(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.btype.read_desc(br)?;
        self.sbtype.read_desc(br)?;
        for el in &mut self.colhi_tree {
            el.read_desc(br)?;
        }
        self.col_last = 0;
        self.colors.read_desc(br)?;
        self.pattern.read_desc(br)?;
        self.xoff.read_desc(br)?;
        self.yoff.read_desc(br)?;
        self.intradc.reset();
        self.interdc.reset();
        self.run.read_desc(br)?;
        Ok(())
    }
    fn read_bundles_binkb(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.btype.read_binkb(br)?;
        self.colors.read_binkb(br)?;
        self.pattern.read_binkb(br)?;
        self.xoff.read_binkb(br)?;
        self.yoff.read_binkb(br)?;
        self.intradc.read_binkb(br)?;
        self.interdc.read_binkb(br)?;
        self.intraq.read_binkb(br)?;
        self.interq.read_binkb(br)?;
        self.nresidues.read_binkb(br)?;
        Ok(())
    }
    fn read_bundles(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.btype.read_block_types(br, &self.trees)?;
        self.sbtype.read_block_types(br, &self.trees)?;
        self.colors.read_colors(br, &self.trees, &self.colhi_tree, &mut self.col_last, self.is_ver_i)?;
        self.pattern.read_patterns(br, &self.trees)?;
        self.xoff.read_motion_values(br, &self.trees)?;
        self.yoff.read_motion_values(br, &self.trees)?;
        self.intradc.read_dcs(br, DC_START_BITS)?;
        self.interdc.read_dcs(br, DC_START_BITS)?;
        self.run.read_runs(br, &self.trees)?;
        Ok(())
    }

    fn put_block(&self, block: &[u8; 64], dst: &mut [u8], mut off: usize, stride: usize, scaled: bool) {
        if !scaled {
            for src in block.chunks_exact(8) {
                let out = &mut dst[off..][..8];
                out.copy_from_slice(src);
                off += stride;
            }
        } else {
            for src in block.chunks_exact(8) {
                for i in 0..8 {
                    dst[off + i * 2 + 0] = src[i];
                    dst[off + i * 2 + 1] = src[i];
                }
                off += stride;
                for i in 0..8 {
                    dst[off + i * 2 + 0] = src[i];
                    dst[off + i * 2 + 1] = src[i];
                }
                off += stride;
            }
        }
    }
    fn copy_block(&mut self, dst: &mut [u8], mut off: usize, stride: usize, bx: usize, by: usize, xoff: i8, yoff: i8) -> DecoderResult<()> {
        if let Some(prev_buf) = self.ips.get_ref() {
            let xoff = ((bx * 8) as isize) + (xoff as isize);
            let yoff = ((by * 8) as isize) + (yoff as isize);
            validate!((xoff >= 0) && (xoff + 8 <= (self.cur_w as isize)));
            validate!((yoff >= 0) && (yoff + 8 <= (self.cur_h as isize)));
            let pstride = prev_buf.get_stride(self.cur_plane);
            let mut poff = prev_buf.get_offset(self.cur_plane) + (xoff as usize) + (yoff as usize) * pstride;
            let pdata = prev_buf.get_data();
            let ppix = pdata.as_slice();
            for _ in 0..8 {
                let src = &ppix[poff..][..8];
                let out = &mut dst[off..][..8];
                out.copy_from_slice(src);
                off += stride;
                poff += pstride;
            }
            Ok(())
        } else {
            Err(DecoderError::MissingReference)
        }
    }
    fn copy_overlapped(&mut self, dst: &mut [u8], mut off: usize, stride: usize, bx: usize, by: usize, xoff: i8, yoff1: i8) -> DecoderResult<()> {
        let ybias = if self.key_frame { -15 } else { 0 };
        let yoff = yoff1 + ybias;

        let xpos = ((bx * 8) as isize) + (xoff as isize);
        let ypos = ((by * 8) as isize) + (yoff as isize);
        validate!((xpos >= 0) && (xpos + 8 <= (self.cur_w as isize)));
        validate!((ypos >= 0) && (ypos + 8 <= (self.cur_h as isize)));

        let mut block: [u8; 64] = [0; 64];
        let mut ref_off = ((off as isize) + (xoff as isize) + (yoff as isize) * (stride as isize)) as usize;
        for row in block.chunks_exact_mut(8) {
            row.copy_from_slice(&dst[ref_off..][..8]);
            ref_off += stride;
        }
        for row in block.chunks_exact(8) {
            let out = &mut dst[off..][..8];
            out.copy_from_slice(row);
            off += stride;
        }

        Ok(())
    }
    fn add_block(&self, coeffs: &[i32; 64], dst: &mut [u8], mut off: usize, stride: usize) {
        for src in coeffs.chunks_exact(8) {
            for i in 0..8 {
                let v = (dst[off + i] as i32) + src[i];
                dst[off + i] = v as u8;
            }
            off += stride;
        }
    }
    fn idct_put(&self, coeffs: &[i32; 64], dst: &mut [u8], mut off: usize, stride: usize) {
        let mut tmp: [i32; 64] = [0; 64];
        let mut row: [i32; 8] = [0; 8];
        for i in 0..8 {
            idct!(coeffs, 8, tmp, 8, i, 0, 0);
        }
        for srow in tmp.chunks_exact(8) {
            idct!(srow, 1, row, 1, 0, 0x7F, 8);
            for i in 0..8 {
                dst[off + i] = row[i] as u8;
            }
            off += stride;
        }
    }
    fn idct_add(&self, coeffs: &[i32; 64], dst: &mut [u8], mut off: usize, stride: usize) {
        let mut tmp: [i32; 64] = [0; 64];
        let mut row: [i32; 8] = [0; 8];
        for i in 0..8 {
            idct!(coeffs, 8, tmp, 8, i, 0, 0);
        }
        for srow in tmp.chunks_exact(8) {
            idct!(srow, 1, row, 1, 0, 0x7F, 8);
            for i in 0..8 {
                let v = (dst[off + i] as i32) + row[i];
                dst[off + i] = v as u8;
            }
            off += stride;
        }
    }

    fn decode_plane_binkb(&mut self, br: &mut BitReader, plane_no: usize, buf: &mut NAVideoBuffer<u8>) -> DecoderResult<()> {
        let stride = buf.get_stride(plane_no);
        let mut off = buf.get_offset(plane_no);
        let (width, height) = buf.get_dimensions(plane_no);
        let data = buf.get_data_mut().unwrap();
        let dst = data.as_mut_slice();
        let bw = (width  + 7) >> 3;
        let bh = (height + 7) >> 3;
        self.cur_w = (width + 7) & !7;
        self.cur_h = (height + 7) & !7;
        self.cur_plane = plane_no;
        self.init_bundle_lengths_binkb();
        for by in 0..bh {
            self.read_bundles_binkb(br)?;
            for bx in 0..bw {
                let mut coeffs: [i32; 64] = [0; 64];
                let btype = self.btype.get_val()?;
                match btype {
                    0 => { // skip
                        },
                    1 => { // run
                            let scan = BINK_PATTERNS[br.read(4)? as usize];
                            let mut idx = 0;
                            while idx < 63 {
                                let run         = br.read_bool()?;
                                let len         = (br.read(BINKB_RUN_BITS[idx])? as usize) + 1;
                                validate!(idx + len <= 64);
                                if run {
                                    let val = self.colors.get_val()?;
                                    for j in 0..len {
                                        let pos = scan[idx + j] as usize;
                                        dst[off + (pos >> 3) * stride + (pos & 7)] = val;
                                    }
                                    idx += len;
                                } else {
                                    for _ in 0..len {
                                        let pos = scan[idx] as usize;
                                        dst[off + (pos >> 3) * stride + (pos & 7)] = self.colors.get_val()?;
                                        idx += 1;
                                    }
                                }
                            }
                            if idx == 63 {
                                let pos = scan[idx] as usize;
                                dst[off + (pos >> 3) * stride + (pos & 7)] = self.colors.get_val()?;
                            }
                        },
                    2 => { // intra
                            coeffs[0] = self.intradc.get_val()? as i32;
                            let q = self.intraq.get_val()? as usize;
                            read_dct_coefficients(br, &mut coeffs, &BINK_SCAN, &self.qmat_b.intra_qmat, Some(q))?;
                            self.idct_put(&coeffs, dst, off, stride);
                        },
                    3 => { // residue
                            let xoff = self.xoff.get_val()?;
                            let yoff = self.yoff.get_val()?;
                            self.copy_overlapped(dst, off, stride, bx, by, xoff, yoff)?;
                            let nmasks = self.nresidues.get_val()? as usize;
                            read_residue(br, &mut coeffs, nmasks)?;
                            self.add_block(&coeffs, dst, off, stride);
                        },
                    4 => { // inter
                            let xoff = self.xoff.get_val()?;
                            let yoff = self.yoff.get_val()?;
                            self.copy_overlapped(dst, off, stride, bx, by, xoff, yoff)?;
                            coeffs[0] = self.interdc.get_val()? as i32;
                            let q = self.interq.get_val()? as usize;
                            read_dct_coefficients(br, &mut coeffs, &BINK_SCAN, &self.qmat_b.inter_qmat, Some(q))?;
                            self.idct_add(&coeffs, dst, off, stride);
                        },
                    5 => { // fill
                            let fill = self.colors.get_val()?;
                            for i in 0..8 {
                                for j in 0..8 { dst[off + i * stride + j] = fill; }
                            }
                        },
                    6 => { // pattern
                            let clr: [u8; 2] = [ self.colors.get_val()?, self.colors.get_val()? ];
                            for i in 0..8 {
                                let pattern = self.pattern.get_val()? as usize;
                                for j in 0..8 {
                                    dst[off + i * stride + j] = clr[(pattern >> j) & 1];
                                }
                            }
                        },
                    7 => { // motion block
                            let xoff = self.xoff.get_val()?;
                            let yoff = self.yoff.get_val()?;
                            self.copy_overlapped(dst, off, stride, bx, by, xoff, yoff)?;
                        },
                    8 => { // raw
                            for i in 0..8 {
                                for j in 0..8 {
                                    dst[off + i * stride + j] = self.colors.get_val()?;
                                }
                            }
                        },
                    _ => { return Err(DecoderError::InvalidData); },
                };
                off += 8;
            }
            off += stride * 8 - bw * 8;
        }
        if (br.tell() & 0x1F) != 0 {
            let skip = (32 - (br.tell() & 0x1F)) as u32;
            br.skip(skip)?;
        }
        Ok(())
    }
    fn handle_block(&mut self, br: &mut BitReader, bx: usize, by: usize,
                    dst: &mut [u8], off: usize, stride: usize, btype: u8, scaled: bool) -> DecoderResult<()> {
        let mut oblock: [u8; 64] = [0; 64];
        let mut coeffs: [i32; 64] = [0; 64];
        match btype {
            SKIP_BLOCK => {
                    validate!(!scaled);
                    self.copy_block(dst, off, stride, bx, by, 0, 0)?;
                },
            SCALED_BLOCK => {
                    validate!(!scaled);
                    let sbtype = self.sbtype.get_val()?;
                    self.handle_block(br, bx, by, dst, off, stride, sbtype, true)?;
                },
            MOTION_BLOCK => {
                    validate!(!scaled);
                    let xoff = self.xoff.get_val()?;
                    let yoff = self.yoff.get_val()?;
                    self.copy_block(dst, off, stride, bx, by, xoff, yoff)?;
                },
            RUN_BLOCK => {
                    let scan = BINK_PATTERNS[br.read(4)? as usize];
                    let mut idx = 0;
                    while idx < 63 {
                        let run = (self.run.get_val()? as usize) + 1;
                        validate!(idx + run <= 64);
                        if br.read_bool()? {
                            let val = self.colors.get_val()?;
                            for j in 0..run {
                                oblock[scan[idx + j] as usize] = val;
                            }
                            idx += run;
                        } else {
                            for _ in 0..run {
                                oblock[scan[idx] as usize] = self.colors.get_val()?;
                                idx += 1;
                            }
                        }
                    }
                    if idx == 63 { oblock[scan[63] as usize] = self.colors.get_val()?; }
                    self.put_block(&oblock, dst, off, stride, scaled);
                },
            RESIDUE_BLOCK => {
                    validate!(!scaled);
                    let xoff = self.xoff.get_val()?;
                    let yoff = self.yoff.get_val()?;
                    self.copy_block(dst, off, stride, bx, by, xoff, yoff)?;
                    let nmasks                  = br.read(7)? as usize;
                    read_residue(br, &mut coeffs, nmasks)?;
                    self.add_block(&coeffs, dst, off, stride);
                },
            INTRA_BLOCK => {
                    coeffs[0] = self.intradc.get_val()? as i32;
                    read_dct_coefficients(br, &mut coeffs, &BINK_SCAN, BINK_INTRA_QUANT, None)?;
                    if !scaled {
                        self.idct_put(&coeffs, dst, off, stride);
                    } else {
                        self.idct_put(&coeffs, &mut oblock, 0, 8);
                        self.put_block(&oblock, dst, off, stride, scaled);
                    }
                },
            FILL_BLOCK => {
                    let fill = self.colors.get_val()?;
                    oblock = [fill; 64];
                    self.put_block(&oblock, dst, off, stride, scaled);
                },
            INTER_BLOCK => {
                    validate!(!scaled);
                    let xoff = self.xoff.get_val()?;
                    let yoff = self.yoff.get_val()?;
                    self.copy_block(dst, off, stride, bx, by, xoff, yoff)?;
                    coeffs[0] = self.interdc.get_val()? as i32;
                    read_dct_coefficients(br, &mut coeffs, &BINK_SCAN, BINK_INTER_QUANT, None)?;
                    self.idct_add(&coeffs, dst, off, stride);
                },
            PATTERN_BLOCK => {
                    let clr: [u8; 2] = [ self.colors.get_val()?, self.colors.get_val()? ];
                    for i in 0..8 {
                        let pattern = self.pattern.get_val()? as usize;
                        for j in 0..8 {
                            oblock[i * 8 + j] = clr[(pattern >> j) & 1];
                        }
                    }
                    self.put_block(&oblock, dst, off, stride, scaled);
                },
            RAW_BLOCK => {
                    for i in 0..8 {
                        for j in 0..8 {
                            oblock[i * 8 + j] = self.colors.get_val()?;
                        }
                    }
                    self.put_block(&oblock, dst, off, stride, scaled);
                },
            _ => { return Err(DecoderError::InvalidData); },
        };
        Ok(())
    }
    fn decode_plane(&mut self, br: &mut BitReader, plane_no: usize, buf: &mut NAVideoBuffer<u8>) -> DecoderResult<()> {
        let stride = buf.get_stride(plane_no);
        let mut off = buf.get_offset(plane_no);
        let (width, height) = buf.get_dimensions(plane_no);
        let data = buf.get_data_mut().unwrap();
        let dst = data.as_mut_slice();
        let bw = (width  + 7) >> 3;
        let bh = (height + 7) >> 3;
        self.cur_w = (width + 7) & !7;
        self.cur_h = (height + 7) & !7;
        self.cur_plane = plane_no;
        self.init_bundle_lengths(width.max(8), bw);
        self.read_bundles_desc(br)?;
        for by in 0..bh {
            self.read_bundles(br)?;
            let mut bx = 0;
            while bx < bw {
                let btype = self.btype.get_val()?;
                if btype == SCALED_BLOCK && (by & 1) == 1 { // already decoded scaled block, skip
                    bx += 2;
                    continue;
                }
                self.handle_block(br, bx, by, dst, off + bx * 8, stride, btype, false)?;
                if btype == SCALED_BLOCK {
                    bx += 1;
                }
                bx += 1;
            }
            off += stride * 8;
        }
        if (br.tell() & 0x1F) != 0 {
            let skip = (32 - (br.tell() & 0x1F)) as u32;
            br.skip(skip)?;
        }
        Ok(())
    }
}

fn get_coef(br: &mut BitReader, bits1: u8) -> DecoderResult<i32> {
    let t;
    if bits1 == 1 {
        t = if br.read_bool()? { -1 } else { 1 };
    } else {
        let bits = bits1 - 1;
        let val             = (br.read(bits)? as i32) | (1 << bits);
        if br.read_bool()? {
            t = -val;
        } else {
            t = val;
        }
    }
    Ok(t)
}

fn read_dct_coefficients(br: &mut BitReader, block: &mut [i32; 64], scan: &[usize; 64],
                         quant_matrices: &[[i32; 64]; 16], q: Option<usize>) -> DecoderResult<()> {
    let mut coef_list: [i32; 128] = [0; 128];
    let mut mode_list: [u8; 128] = [0; 128];
    let mut list_start = 64;
    let mut list_end   = 64;
    let mut coef_idx: [usize; 64] = [0; 64];
    let mut coef_count = 0;

    coef_list[list_end] =  4;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] = 24;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] = 44;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] =  1;   mode_list[list_end] = 3;    list_end += 1;
    coef_list[list_end] =  2;   mode_list[list_end] = 3;    list_end += 1;
    coef_list[list_end] =  3;   mode_list[list_end] = 3;    list_end += 1;

    let mut bits1                               = br.read(4)? as u8;
    while bits1 >= 1 {
        let mut list_pos = list_start;
        while list_pos < list_end {
            let ccoef = coef_list[list_pos];
            let mode  = mode_list[list_pos];
            if (mode == 0 && ccoef == 0) || !br.read_bool()? {
                list_pos += 1;
                continue;
            }
            match mode {
                0 | 2 => {
                        if mode == 0 {
                            coef_list[list_pos] = ccoef + 4;
                            mode_list[list_pos] = 1;
                        } else {
                            coef_list[list_pos] = 0;
                            mode_list[list_pos] = 0;
                            list_pos += 1;
                        }
                        for i in 0..4 {
                            if br.read_bool()? {
                                list_start -= 1;
                                coef_list[list_start] = ccoef + i;
                                mode_list[list_start] = 3;
                            } else {
                                let idx = (ccoef + i) as usize;
                                block[scan[idx]] = get_coef(br, bits1)?;
                                coef_idx[coef_count] = idx;
                                coef_count += 1;
                            }
                        }
                    },
                1 => {
                        mode_list[list_pos] = 2;
                        for i in 0..3 {
                            coef_list[list_end] = ccoef + i * 4 + 4;
                            mode_list[list_end] = 2;
                            list_end += 1;
                        }
                    },
                3 => {
                        let idx = ccoef as usize;
                        block[scan[idx]] = get_coef(br, bits1)?;
                        coef_idx[coef_count] = idx;
                        coef_count += 1;
                        coef_list[list_pos] = 0;
                        mode_list[list_pos] = 0;
                        list_pos += 1;
                    },
                _ => unreachable!(),
            };
        }
        bits1 -= 1;
    }

    let q_index = if let Some(qidx) = q { qidx } else { br.read(4)? as usize };
    let qmat = &quant_matrices[q_index];
    block[0] = block[0].wrapping_mul(qmat[0]) >> 11;
    for idx in coef_idx.iter().take(coef_count) {
        block[scan[*idx]] = block[scan[*idx]].wrapping_mul(qmat[*idx]) >> 11;
    }

    Ok(())
}

fn read_residue(br: &mut BitReader, block: &mut [i32; 64], mut masks_count: usize) -> DecoderResult<()> {
    let mut coef_list: [i32; 128] = [0; 128];
    let mut mode_list: [u8; 128] = [0; 128];
    let mut list_start = 64;
    let mut list_end   = 64;
    let mut nz_coef_idx: [usize; 64] = [0; 64];
    let mut nz_coef_count = 0;

    coef_list[list_end] =  4;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] = 24;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] = 44;   mode_list[list_end] = 0;    list_end += 1;
    coef_list[list_end] =  0;   mode_list[list_end] = 2;    list_end += 1;

    let mut mask                                = 1 << br.read(3)?;
    while mask > 0 {
        for i in 0..nz_coef_count {
            if !br.read_bool()? { continue; }
            let idx = nz_coef_idx[i];
            if block[idx] < 0 {
                block[idx] -= mask;
            } else {
                block[idx] += mask;
            }
            if masks_count == 0 {
                return Ok(());
            }
            masks_count -= 1;
        }
        let mut list_pos = list_start;
        while list_pos < list_end {
            let ccoef = coef_list[list_pos];
            let mode  = mode_list[list_pos];
            if (mode == 0 && ccoef == 0) || !br.read_bool()? {
                list_pos += 1;
                continue;
            }
            match mode {
                0 | 2 => {
                        if mode == 0 {
                            coef_list[list_pos] = ccoef + 4;
                            mode_list[list_pos] = 1;
                        } else {
                            coef_list[list_pos] = 0;
                            mode_list[list_pos] = 0;
                            list_pos += 1;
                        }
                        for i in 0..4 {
                            if br.read_bool()? {
                                list_start -= 1;
                                coef_list[list_start] = ccoef + i;
                                mode_list[list_start] = 3;
                            } else {
                                let idx = (ccoef + i) as usize;
                                nz_coef_idx[nz_coef_count] = BINK_SCAN[idx];
                                nz_coef_count += 1;
                                block[BINK_SCAN[idx]] = if br.read_bool()? { -mask } else { mask };
                                if masks_count == 0 {
                                    return Ok(());
                                }
                                masks_count -= 1;
                            }
                        }
                    },
                1 => {
                        mode_list[list_pos] = 2;
                        for i in 0..3 {
                            coef_list[list_end] = ccoef + i * 4 + 4;
                            mode_list[list_end] = 2;
                            list_end += 1;
                        }
                    },
                3 => {
                        let idx = ccoef as usize;
                        nz_coef_idx[nz_coef_count] = BINK_SCAN[idx];
                        nz_coef_count += 1;
                        block[BINK_SCAN[idx]] = if br.read_bool()? { -mask } else { mask };
                        coef_list[list_pos] = 0;
                        mode_list[list_pos] = 0;
                        list_pos += 1;
                        if masks_count == 0 {
                            return Ok(());
                        }
                        masks_count -= 1;
                    },
                _ => unreachable!(),
            };
        }
        mask >>= 1;
    }

    Ok(())
}

const BINK_FLAG_ALPHA:  u32 = 0x00100000;
const BINK_FLAG_GRAY:   u32 = 0x00020000;

impl NADecoder for BinkDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();

            let edata = info.get_extradata().unwrap();
            validate!(edata.len() >= 8);

            let mut mr = MemoryReader::new_read(&edata);
            let mut br = ByteReader::new(&mut mr);
            let magic                   = br.read_u32be()?;
            let flags                   = br.read_u32le()?;

            self.is_ver_b  = (magic & 0xFF) == (b'b' as u32);
            self.is_ver_i  = (magic & 0xFF) >= (b'i' as u32);
            self.has_alpha = (flags & BINK_FLAG_ALPHA) != 0;
            self.is_gray   = (flags & BINK_FLAG_GRAY) != 0;
            self.swap_uv   = (magic & 0xFF) >= (b'h' as u32);
            if self.has_alpha && self.is_gray { return Err(DecoderError::NotImplemented); }

            let aplane = if self.has_alpha { Some(NAPixelChromaton::new(0, 0, false, 8, 0, 3, 1)) } else { None };
            let fmt;
            if !self.is_gray {
                fmt = NAPixelFormaton::new(ColorModel::YUV(YUVSubmodel::YUVJ),
                                           Some(NAPixelChromaton::new(0, 0, false, 8, 0, 0, 1)),
                                           Some(NAPixelChromaton::new(1, 1, false, 8, 0, 1, 1)),
                                           Some(NAPixelChromaton::new(1, 1, false, 8, 0, 2, 1)),
                                           aplane, None,
                                           0, if self.has_alpha { 4 } else { 3 } );
            } else {
                fmt = NAPixelFormaton::new(ColorModel::YUV(YUVSubmodel::YUVJ),
                                           Some(NAPixelChromaton::new(0, 0, false, 8, 0, 0, 1)),
                                           None, None, None, None, 0, 1);
            }
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();

            //self.init_bundle_lengths(w.max(8), (w + 7) >> 3);
            self.init_bundle_bufs((w + 7) >> 3, (h + 7) >> 3);

            if self.is_ver_b {
                self.qmat_b.calc_binkb_quants();
            }

            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

        let mut br = BitReader::new(&src, BitReaderMode::LE);

        let mut buf;
        self.key_frame = pkt.is_keyframe();
        if self.is_ver_b {
            let bufret = self.hams.clone_ref();
            if let Some(bbuf) = bufret {
                buf = bbuf;
            } else {
                let bufinfo = alloc_video_buffer(self.info.get_properties().get_video_info().unwrap(), 4)?;
                buf = bufinfo.get_vbuf().unwrap();
                self.key_frame = true;
                self.hams.add_frame(buf);
                buf = self.hams.get_output_frame().unwrap();
            }
        } else {
            let bufinfo = alloc_video_buffer(self.info.get_properties().get_video_info().unwrap(), 4)?;
            buf = bufinfo.get_vbuf().unwrap();
        }

        let nplanes = if self.is_gray { 1 } else { 3 };
        if self.has_alpha {
            validate!(!self.is_ver_b);
            if self.is_ver_i {
                br.skip(32)?;
            }
            self.decode_plane(&mut br, nplanes, &mut buf)?;
        }
        if self.is_ver_i {
            br.skip(32)?;
        }
        for plane in 0..nplanes {
            if self.is_ver_b {
                self.decode_plane_binkb(&mut br, plane, &mut buf)?;
            } else {
                let plane_idx = if plane > 0 && self.swap_uv { plane ^ 3 } else { plane };
                self.decode_plane(&mut br, plane_idx, &mut buf)?;
            }
        }
        let bufinfo = NABufferType::Video(buf);
        if !self.is_ver_b {
            self.ips.add_frame(bufinfo.get_vbuf().unwrap());
        }

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_frame_type(FrameType::P);
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.ips.clear();
    }
}

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(BinkDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::rad_register_all_codecs;
    use crate::rad_register_all_demuxers;
    #[test]
    fn test_binkvid() {
        let mut dmx_reg = RegisteredDemuxers::new();
        rad_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        rad_register_all_codecs(&mut dec_reg);

        let file = "assets/RAD/NEW.BIK";
        //let file = "assets/RAD/NWCLOGO.BIK";
        test_file_decoding("bink", file, Some(42), true, false, None/*Some("bink-b")*/, &dmx_reg, &dec_reg);

        //let file = "assets/RAD/bink_dct.bik";
        //let file = "assets/RAD/day3-b.bik";
        let file = "assets/RAD/ActivisionLogo.bik";
        //let file = "assets/RAD/ATI-9700-Animusic-Movie-v1.0.bik";
        //let file = "assets/RAD/original.bik";
        test_file_decoding("bink", file, Some(42), true, false, None/*Some("bink-")*/, &dmx_reg, &dec_reg);
    }
}

const BINK_SCAN: [usize; 64] = [
     0,  1,  8,  9,  2,  3, 10, 11,
     4,  5, 12, 13,  6,  7, 14, 15,
    20, 21, 28, 29, 22, 23, 30, 31,
    16, 17, 24, 25, 32, 33, 40, 41,
    34, 35, 42, 43, 48, 49, 56, 57,
    50, 51, 58, 59, 18, 19, 26, 27,
    36, 37, 44, 45, 38, 39, 46, 47,
    52, 53, 60, 61, 54, 55, 62, 63
];

const BINK_TREE_CODES: [[u8; 16]; 16] = [
    [ 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F ],
    [ 0x00, 0x01, 0x03, 0x05, 0x07, 0x09, 0x0B, 0x0D, 0x0F, 0x13, 0x15, 0x17, 0x19, 0x1B, 0x1D, 0x1F ],
    [ 0x00, 0x02, 0x01, 0x09, 0x05, 0x15, 0x0D, 0x1D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x17, 0x0F, 0x1F ],
    [ 0x00, 0x02, 0x06, 0x01, 0x09, 0x05, 0x0D, 0x1D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x17, 0x0F, 0x1F ],
    [ 0x00, 0x04, 0x02, 0x06, 0x01, 0x09, 0x05, 0x0D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x17, 0x0F, 0x1F ],
    [ 0x00, 0x04, 0x02, 0x0A, 0x06, 0x0E, 0x01, 0x09, 0x05, 0x0D, 0x03, 0x0B, 0x07, 0x17, 0x0F, 0x1F ],
    [ 0x00, 0x02, 0x0A, 0x06, 0x0E, 0x01, 0x09, 0x05, 0x0D, 0x03, 0x0B, 0x1B, 0x07, 0x17, 0x0F, 0x1F ],
    [ 0x00, 0x01, 0x05, 0x03, 0x13, 0x0B, 0x1B, 0x3B, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x01, 0x03, 0x13, 0x0B, 0x2B, 0x1B, 0x3B, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x01, 0x05, 0x0D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x02, 0x01, 0x05, 0x0D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x01, 0x09, 0x05, 0x0D, 0x03, 0x13, 0x0B, 0x1B, 0x07, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x02, 0x01, 0x03, 0x13, 0x0B, 0x1B, 0x3B, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x2F, 0x1F, 0x3F ],
    [ 0x00, 0x01, 0x05, 0x03, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x4F, 0x2F, 0x6F, 0x1F, 0x5F, 0x3F, 0x7F ],
    [ 0x00, 0x01, 0x05, 0x03, 0x07, 0x17, 0x37, 0x77, 0x0F, 0x4F, 0x2F, 0x6F, 0x1F, 0x5F, 0x3F, 0x7F ],
    [ 0x00, 0x02, 0x01, 0x05, 0x03, 0x07, 0x27, 0x17, 0x37, 0x0F, 0x2F, 0x6F, 0x1F, 0x5F, 0x3F, 0x7F ]
];

const BINK_TREE_BITS: [[u8; 16]; 16] = [
    [ 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4 ],
    [ 1, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5 ],
    [ 2, 2, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5 ],
    [ 2, 3, 3, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5 ],
    [ 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5 ],
    [ 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5 ],
    [ 2, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5 ],
    [ 1, 3, 3, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6 ],
    [ 1, 2, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6 ],
    [ 1, 3, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6 ],
    [ 2, 2, 3, 4, 4, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6 ],
    [ 1, 4, 4, 4, 4, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6 ],
    [ 2, 2, 2, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6 ],
    [ 1, 3, 3, 3, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7 ],
    [ 1, 3, 3, 3, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7 ],
    [ 2, 2, 3, 3, 3, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7 ]
];

const BINK_PATTERNS: [[u8; 64]; 16] = [
  [
    0x00, 0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38,
    0x39, 0x31, 0x29, 0x21, 0x19, 0x11, 0x09, 0x01,
    0x02, 0x0A, 0x12, 0x1A, 0x22, 0x2A, 0x32, 0x3A,
    0x3B, 0x33, 0x2B, 0x23, 0x1B, 0x13, 0x0B, 0x03,
    0x04, 0x0C, 0x14, 0x1C, 0x24, 0x2C, 0x34, 0x3C,
    0x3D, 0x35, 0x2D, 0x25, 0x1D, 0x15, 0x0D, 0x05,
    0x06, 0x0E, 0x16, 0x1E, 0x26, 0x2E, 0x36, 0x3E,
    0x3F, 0x37, 0x2F, 0x27, 0x1F, 0x17, 0x0F, 0x07,
  ], [
    0x3B, 0x3A, 0x39, 0x38, 0x30, 0x31, 0x32, 0x33,
    0x2B, 0x2A, 0x29, 0x28, 0x20, 0x21, 0x22, 0x23,
    0x1B, 0x1A, 0x19, 0x18, 0x10, 0x11, 0x12, 0x13,
    0x0B, 0x0A, 0x09, 0x08, 0x00, 0x01, 0x02, 0x03,
    0x04, 0x05, 0x06, 0x07, 0x0F, 0x0E, 0x0D, 0x0C,
    0x14, 0x15, 0x16, 0x17, 0x1F, 0x1E, 0x1D, 0x1C,
    0x24, 0x25, 0x26, 0x27, 0x2F, 0x2E, 0x2D, 0x2C,
    0x34, 0x35, 0x36, 0x37, 0x3F, 0x3E, 0x3D, 0x3C,
  ], [
    0x19, 0x11, 0x12, 0x1A, 0x1B, 0x13, 0x0B, 0x03,
    0x02, 0x0A, 0x09, 0x01, 0x00, 0x08, 0x10, 0x18,
    0x20, 0x28, 0x30, 0x38, 0x39, 0x31, 0x29, 0x2A,
    0x32, 0x3A, 0x3B, 0x33, 0x2B, 0x23, 0x22, 0x21,
    0x1D, 0x15, 0x16, 0x1E, 0x1F, 0x17, 0x0F, 0x07,
    0x06, 0x0E, 0x0D, 0x05, 0x04, 0x0C, 0x14, 0x1C,
    0x24, 0x2C, 0x34, 0x3C, 0x3D, 0x35, 0x2D, 0x2E,
    0x36, 0x3E, 0x3F, 0x37, 0x2F, 0x27, 0x26, 0x25,
  ], [
    0x03, 0x0B, 0x02, 0x0A, 0x01, 0x09, 0x00, 0x08,
    0x10, 0x18, 0x11, 0x19, 0x12, 0x1A, 0x13, 0x1B,
    0x23, 0x2B, 0x22, 0x2A, 0x21, 0x29, 0x20, 0x28,
    0x30, 0x38, 0x31, 0x39, 0x32, 0x3A, 0x33, 0x3B,
    0x3C, 0x34, 0x3D, 0x35, 0x3E, 0x36, 0x3F, 0x37,
    0x2F, 0x27, 0x2E, 0x26, 0x2D, 0x25, 0x2C, 0x24,
    0x1C, 0x14, 0x1D, 0x15, 0x1E, 0x16, 0x1F, 0x17,
    0x0F, 0x07, 0x0E, 0x06, 0x0D, 0x05, 0x0C, 0x04,
  ], [
    0x18, 0x19, 0x10, 0x11, 0x08, 0x09, 0x00, 0x01,
    0x02, 0x03, 0x0A, 0x0B, 0x12, 0x13, 0x1A, 0x1B,
    0x1C, 0x1D, 0x14, 0x15, 0x0C, 0x0D, 0x04, 0x05,
    0x06, 0x07, 0x0E, 0x0F, 0x16, 0x17, 0x1E, 0x1F,
    0x27, 0x26, 0x2F, 0x2E, 0x37, 0x36, 0x3F, 0x3E,
    0x3D, 0x3C, 0x35, 0x34, 0x2D, 0x2C, 0x25, 0x24,
    0x23, 0x22, 0x2B, 0x2A, 0x33, 0x32, 0x3B, 0x3A,
    0x39, 0x38, 0x31, 0x30, 0x29, 0x28, 0x21, 0x20,
  ], [
    0x00, 0x01, 0x02, 0x03, 0x08, 0x09, 0x0A, 0x0B,
    0x10, 0x11, 0x12, 0x13, 0x18, 0x19, 0x1A, 0x1B,
    0x20, 0x21, 0x22, 0x23, 0x28, 0x29, 0x2A, 0x2B,
    0x30, 0x31, 0x32, 0x33, 0x38, 0x39, 0x3A, 0x3B,
    0x04, 0x05, 0x06, 0x07, 0x0C, 0x0D, 0x0E, 0x0F,
    0x14, 0x15, 0x16, 0x17, 0x1C, 0x1D, 0x1E, 0x1F,
    0x24, 0x25, 0x26, 0x27, 0x2C, 0x2D, 0x2E, 0x2F,
    0x34, 0x35, 0x36, 0x37, 0x3C, 0x3D, 0x3E, 0x3F,
  ], [
    0x06, 0x07, 0x0F, 0x0E, 0x0D, 0x05, 0x0C, 0x04,
    0x03, 0x0B, 0x02, 0x0A, 0x09, 0x01, 0x00, 0x08,
    0x10, 0x18, 0x11, 0x19, 0x12, 0x1A, 0x13, 0x1B,
    0x14, 0x1C, 0x15, 0x1D, 0x16, 0x1E, 0x17, 0x1F,
    0x27, 0x2F, 0x26, 0x2E, 0x25, 0x2D, 0x24, 0x2C,
    0x23, 0x2B, 0x22, 0x2A, 0x21, 0x29, 0x20, 0x28,
    0x31, 0x30, 0x38, 0x39, 0x3A, 0x32, 0x3B, 0x33,
    0x3C, 0x34, 0x3D, 0x35, 0x36, 0x37, 0x3F, 0x3E,
  ], [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, 0x09, 0x08,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    0x1F, 0x1E, 0x1D, 0x1C, 0x1B, 0x1A, 0x19, 0x18,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x2F, 0x2E, 0x2D, 0x2C, 0x2B, 0x2A, 0x29, 0x28,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
    0x3F, 0x3E, 0x3D, 0x3C, 0x3B, 0x3A, 0x39, 0x38,
  ], [
    0x00, 0x08, 0x09, 0x01, 0x02, 0x03, 0x0B, 0x0A,
    0x12, 0x13, 0x1B, 0x1A, 0x19, 0x11, 0x10, 0x18,
    0x20, 0x28, 0x29, 0x21, 0x22, 0x23, 0x2B, 0x2A,
    0x32, 0x31, 0x30, 0x38, 0x39, 0x3A, 0x3B, 0x33,
    0x34, 0x3C, 0x3D, 0x3E, 0x3F, 0x37, 0x36, 0x35,
    0x2D, 0x2C, 0x24, 0x25, 0x26, 0x2E, 0x2F, 0x27,
    0x1F, 0x17, 0x16, 0x1E, 0x1D, 0x1C, 0x14, 0x15,
    0x0D, 0x0C, 0x04, 0x05, 0x06, 0x0E, 0x0F, 0x07,
  ], [
    0x18, 0x19, 0x10, 0x11, 0x08, 0x09, 0x00, 0x01,
    0x02, 0x03, 0x0A, 0x0B, 0x12, 0x13, 0x1A, 0x1B,
    0x1C, 0x1D, 0x14, 0x15, 0x0C, 0x0D, 0x04, 0x05,
    0x06, 0x07, 0x0E, 0x0F, 0x16, 0x17, 0x1E, 0x1F,
    0x26, 0x27, 0x2E, 0x2F, 0x36, 0x37, 0x3E, 0x3F,
    0x3C, 0x3D, 0x34, 0x35, 0x2C, 0x2D, 0x24, 0x25,
    0x22, 0x23, 0x2A, 0x2B, 0x32, 0x33, 0x3A, 0x3B,
    0x38, 0x39, 0x30, 0x31, 0x28, 0x29, 0x20, 0x21,
  ], [
    0x00, 0x08, 0x01, 0x09, 0x02, 0x0A, 0x03, 0x0B,
    0x13, 0x1B, 0x12, 0x1A, 0x11, 0x19, 0x10, 0x18,
    0x20, 0x28, 0x21, 0x29, 0x22, 0x2A, 0x23, 0x2B,
    0x33, 0x3B, 0x32, 0x3A, 0x31, 0x39, 0x30, 0x38,
    0x3C, 0x34, 0x3D, 0x35, 0x3E, 0x36, 0x3F, 0x37,
    0x2F, 0x27, 0x2E, 0x26, 0x2D, 0x25, 0x2C, 0x24,
    0x1F, 0x17, 0x1E, 0x16, 0x1D, 0x15, 0x1C, 0x14,
    0x0C, 0x04, 0x0D, 0x05, 0x0E, 0x06, 0x0F, 0x07,
  ], [
    0x00, 0x08, 0x10, 0x18, 0x19, 0x1A, 0x1B, 0x13,
    0x0B, 0x03, 0x02, 0x01, 0x09, 0x11, 0x12, 0x0A,
    0x04, 0x0C, 0x14, 0x1C, 0x1D, 0x1E, 0x1F, 0x17,
    0x0F, 0x07, 0x06, 0x05, 0x0D, 0x15, 0x16, 0x0E,
    0x24, 0x2C, 0x34, 0x3C, 0x3D, 0x3E, 0x3F, 0x37,
    0x2F, 0x27, 0x26, 0x25, 0x2D, 0x35, 0x36, 0x2E,
    0x20, 0x28, 0x30, 0x38, 0x39, 0x3A, 0x3B, 0x33,
    0x2B, 0x23, 0x22, 0x21, 0x29, 0x31, 0x32, 0x2A,
  ], [
    0x00, 0x08, 0x09, 0x01, 0x02, 0x03, 0x0B, 0x0A,
    0x13, 0x1B, 0x1A, 0x12, 0x11, 0x10, 0x18, 0x19,
    0x21, 0x20, 0x28, 0x29, 0x2A, 0x22, 0x23, 0x2B,
    0x33, 0x3B, 0x3A, 0x32, 0x31, 0x39, 0x38, 0x30,
    0x34, 0x3C, 0x3D, 0x35, 0x36, 0x3E, 0x3F, 0x37,
    0x2F, 0x27, 0x26, 0x2E, 0x2D, 0x2C, 0x24, 0x25,
    0x1D, 0x1C, 0x14, 0x15, 0x16, 0x1E, 0x1F, 0x17,
    0x0E, 0x0F, 0x07, 0x06, 0x05, 0x0D, 0x0C, 0x04,
  ], [
    0x18, 0x10, 0x08, 0x00, 0x01, 0x02, 0x03, 0x0B,
    0x13, 0x1B, 0x1A, 0x19, 0x11, 0x0A, 0x09, 0x12,
    0x1C, 0x14, 0x0C, 0x04, 0x05, 0x06, 0x07, 0x0F,
    0x17, 0x1F, 0x1E, 0x1D, 0x15, 0x0E, 0x0D, 0x16,
    0x3C, 0x34, 0x2C, 0x24, 0x25, 0x26, 0x27, 0x2F,
    0x37, 0x3F, 0x3E, 0x3D, 0x35, 0x2E, 0x2D, 0x36,
    0x38, 0x30, 0x28, 0x20, 0x21, 0x22, 0x23, 0x2B,
    0x33, 0x3B, 0x3A, 0x39, 0x31, 0x2A, 0x29, 0x32,
  ], [
    0x00, 0x08, 0x09, 0x01, 0x02, 0x0A, 0x12, 0x11,
    0x10, 0x18, 0x19, 0x1A, 0x1B, 0x13, 0x0B, 0x03,
    0x07, 0x06, 0x0E, 0x0F, 0x17, 0x16, 0x15, 0x0D,
    0x05, 0x04, 0x0C, 0x14, 0x1C, 0x1D, 0x1E, 0x1F,
    0x3F, 0x3E, 0x36, 0x37, 0x2F, 0x2E, 0x2D, 0x35,
    0x3D, 0x3C, 0x34, 0x2C, 0x24, 0x25, 0x26, 0x27,
    0x38, 0x30, 0x31, 0x39, 0x3A, 0x32, 0x2A, 0x29,
    0x28, 0x20, 0x21, 0x22, 0x23, 0x2B, 0x33, 0x3B,
  ], [
    0x00, 0x01, 0x08, 0x09, 0x10, 0x11, 0x18, 0x19,
    0x20, 0x21, 0x28, 0x29, 0x30, 0x31, 0x38, 0x39,
    0x3A, 0x3B, 0x32, 0x33, 0x2A, 0x2B, 0x22, 0x23,
    0x1A, 0x1B, 0x12, 0x13, 0x0A, 0x0B, 0x02, 0x03,
    0x04, 0x05, 0x0C, 0x0D, 0x14, 0x15, 0x1C, 0x1D,
    0x24, 0x25, 0x2C, 0x2D, 0x34, 0x35, 0x3C, 0x3D,
    0x3E, 0x3F, 0x36, 0x37, 0x2E, 0x2F, 0x26, 0x27,
    0x1E, 0x1F, 0x16, 0x17, 0x0E, 0x0F, 0x06, 0x07,
  ]
];

const BINK_INTRA_QUANT: &[[i32; 64]; 16] = &[
  [
    0x010000, 0x016315, 0x01E83D, 0x02A535, 0x014E7B, 0x016577, 0x02F1E6, 0x02724C,
    0x010000, 0x00EEDA, 0x024102, 0x017F9B, 0x00BE80, 0x00611E, 0x01083C, 0x00A552,
    0x021F88, 0x01DC53, 0x027FAD, 0x01F697, 0x014819, 0x00A743, 0x015A31, 0x009688,
    0x02346F, 0x030EE5, 0x01FBFA, 0x02C096, 0x01D000, 0x028396, 0x019247, 0x01F9AA,
    0x02346F, 0x01FBFA, 0x01DC53, 0x0231B8, 0x012F12, 0x01E06C, 0x00CB10, 0x0119A8,
    0x01C48C, 0x019748, 0x014E86, 0x0122AF, 0x02C628, 0x027F20, 0x0297B5, 0x023F32,
    0x025000, 0x01AB6B, 0x01D122, 0x0159B3, 0x012669, 0x008D43, 0x00EE1F, 0x0075ED,
    0x01490C, 0x010288, 0x00F735, 0x00EF51, 0x00E0F1, 0x0072AD, 0x00A4D8, 0x006517,
  ], [
    0x015555, 0x01D971, 0x028AFC, 0x0386F1, 0x01BDF9, 0x01DC9F, 0x03ED33, 0x034311,
    0x015555, 0x013E78, 0x030158, 0x01FF7A, 0x00FE00, 0x00817D, 0x01604F, 0x00DC6D,
    0x02D4B5, 0x027B19, 0x0354E7, 0x029E1F, 0x01B577, 0x00DF04, 0x01CD96, 0x00C8B6,
    0x02F095, 0x0413DC, 0x02A54E, 0x03AB73, 0x026AAB, 0x035A1E, 0x02185E, 0x02A238,
    0x02F095, 0x02A54E, 0x027B19, 0x02ECF5, 0x019418, 0x028090, 0x010EC0, 0x01778A,
    0x025B66, 0x021F0B, 0x01BE09, 0x018394, 0x03B2E0, 0x03542A, 0x0374F1, 0x02FEEE,
    0x031555, 0x0239E4, 0x026C2D, 0x01CCEE, 0x01888C, 0x00BC59, 0x013D7E, 0x009D3C,
    0x01B6BB, 0x0158B5, 0x01499C, 0x013F17, 0x012BEC, 0x0098E6, 0x00DBCB, 0x0086C9,
  ], [
    0x01AAAB, 0x024FCE, 0x032DBB, 0x0468AD, 0x022D78, 0x0253C7, 0x04E87F, 0x0413D5,
    0x01AAAB, 0x018E16, 0x03C1AE, 0x027F58, 0x013D80, 0x00A1DC, 0x01B863, 0x011388,
    0x0389E2, 0x0319DF, 0x042A21, 0x0345A7, 0x0222D4, 0x0116C5, 0x0240FC, 0x00FAE3,
    0x03ACBA, 0x0518D3, 0x034EA1, 0x04964F, 0x030555, 0x0430A5, 0x029E76, 0x034AC5,
    0x03ACBA, 0x034EA1, 0x0319DF, 0x03A833, 0x01F91E, 0x0320B4, 0x015270, 0x01D56D,
    0x02F23F, 0x02A6CE, 0x022D8B, 0x01E479, 0x049F98, 0x042935, 0x04522D, 0x03BEA9,
    0x03DAAB, 0x02C85D, 0x030738, 0x02402A, 0x01EAAF, 0x00EB6F, 0x018CDE, 0x00C48A,
    0x022469, 0x01AEE2, 0x019C02, 0x018EDD, 0x0176E7, 0x00BF20, 0x0112BE, 0x00A87B,
  ], [
    0x020000, 0x02C62A, 0x03D07A, 0x054A69, 0x029CF6, 0x02CAEF, 0x05E3CC, 0x04E499,
    0x020000, 0x01DDB4, 0x048204, 0x02FF36, 0x017D01, 0x00C23C, 0x021077, 0x014AA3,
    0x043F0F, 0x03B8A6, 0x04FF5A, 0x03ED2E, 0x029032, 0x014E86, 0x02B461, 0x012D11,
    0x0468DF, 0x061DCA, 0x03F7F5, 0x05812C, 0x03A000, 0x05072C, 0x03248D, 0x03F353,
    0x0468DF, 0x03F7F5, 0x03B8A6, 0x046370, 0x025E24, 0x03C0D8, 0x019620, 0x02334F,
    0x038919, 0x032E91, 0x029D0D, 0x02455E, 0x058C50, 0x04FE3F, 0x052F69, 0x047E65,
    0x04A000, 0x0356D6, 0x03A243, 0x02B365, 0x024CD2, 0x011A85, 0x01DC3E, 0x00EBD9,
    0x029218, 0x020510, 0x01EE69, 0x01DEA2, 0x01C1E2, 0x00E559, 0x0149B0, 0x00CA2D,
  ], [
    0x02AAAB, 0x03B2E3, 0x0515F8, 0x070DE2, 0x037BF2, 0x03B93E, 0x07DA65, 0x068621,
    0x02AAAB, 0x027CF0, 0x0602B1, 0x03FEF3, 0x01FC01, 0x0102FA, 0x02C09F, 0x01B8DA,
    0x05A96A, 0x04F632, 0x06A9CE, 0x053C3E, 0x036AED, 0x01BE09, 0x039B2D, 0x01916B,
    0x05E129, 0x0827B8, 0x054A9C, 0x0756E5, 0x04D555, 0x06B43B, 0x0430BC, 0x05446F,
    0x05E129, 0x054A9C, 0x04F632, 0x05D9EB, 0x032830, 0x050121, 0x021D80, 0x02EF14,
    0x04B6CC, 0x043E16, 0x037C11, 0x030728, 0x0765C0, 0x06A855, 0x06E9E2, 0x05FDDB,
    0x062AAB, 0x0473C8, 0x04D85A, 0x0399DC, 0x031118, 0x0178B2, 0x027AFD, 0x013A77,
    0x036D76, 0x02B16A, 0x029337, 0x027E2E, 0x0257D8, 0x0131CC, 0x01B796, 0x010D91,
  ], [
    0x038000, 0x04DACA, 0x06ACD5, 0x094238, 0x0492AE, 0x04E322, 0x0A4EA5, 0x08900C,
    0x038000, 0x0343FB, 0x07E388, 0x053E9F, 0x029AC1, 0x0153E8, 0x039CD0, 0x02429E,
    0x076E5B, 0x068322, 0x08BEDE, 0x06DF11, 0x047C57, 0x02496B, 0x04BBAB, 0x020EDD,
    0x07B786, 0x0AB421, 0x06F1ED, 0x09A20D, 0x065800, 0x08CC8E, 0x057FF7, 0x06E9D2,
    0x07B786, 0x06F1ED, 0x068322, 0x07AE04, 0x0424BF, 0x06917B, 0x02C6B8, 0x03D9CB,
    0x062FEB, 0x05917D, 0x0492D7, 0x03F964, 0x09B58C, 0x08BCEF, 0x0912F8, 0x07DD30,
    0x081800, 0x05D7F7, 0x065BF6, 0x04B9F1, 0x040670, 0x01EE69, 0x03416C, 0x019CBC,
    0x047FAA, 0x0388DC, 0x036138, 0x03459C, 0x03134C, 0x01915C, 0x0240F5, 0x0161CF,
  ], [
    0x040000, 0x058C54, 0x07A0F4, 0x0A94D3, 0x0539EC, 0x0595DD, 0x0BC798, 0x09C932,
    0x040000, 0x03BB68, 0x090409, 0x05FE6D, 0x02FA01, 0x018477, 0x0420EE, 0x029547,
    0x087E1F, 0x07714C, 0x09FEB5, 0x07DA5D, 0x052064, 0x029D0D, 0x0568C3, 0x025A21,
    0x08D1BE, 0x0C3B94, 0x07EFEA, 0x0B0258, 0x074000, 0x0A0E59, 0x06491A, 0x07E6A7,
    0x08D1BE, 0x07EFEA, 0x07714C, 0x08C6E0, 0x04BC48, 0x0781B1, 0x032C3F, 0x04669F,
    0x071232, 0x065D22, 0x053A1A, 0x048ABC, 0x0B18A0, 0x09FC7F, 0x0A5ED3, 0x08FCC9,
    0x094000, 0x06ADAC, 0x074487, 0x0566CA, 0x0499A5, 0x02350B, 0x03B87B, 0x01D7B3,
    0x052430, 0x040A20, 0x03DCD3, 0x03BD45, 0x0383C5, 0x01CAB3, 0x029361, 0x01945A,
  ], [
    0x050000, 0x06EF69, 0x098931, 0x0D3A07, 0x068867, 0x06FB55, 0x0EB97E, 0x0C3B7E,
    0x050000, 0x04AA42, 0x0B450B, 0x077E08, 0x03B881, 0x01E595, 0x05292A, 0x033A99,
    0x0A9DA7, 0x094D9F, 0x0C7E62, 0x09D0F4, 0x06687D, 0x034450, 0x06C2F4, 0x02F0AA,
    0x0B062D, 0x0F4A78, 0x09EBE4, 0x0DC2EE, 0x091000, 0x0C91EF, 0x07DB61, 0x09E050,
    0x0B062D, 0x09EBE4, 0x094D9F, 0x0AF898, 0x05EB59, 0x09621D, 0x03F74F, 0x058046,
    0x08D6BE, 0x07F46A, 0x0688A0, 0x05AD6B, 0x0DDEC8, 0x0C7B9F, 0x0CF687, 0x0B3BFB,
    0x0B9000, 0x085917, 0x0915A8, 0x06C07D, 0x05C00E, 0x02C24D, 0x04A69A, 0x024D9F,
    0x066D3C, 0x050CA7, 0x04D407, 0x04AC96, 0x0464B6, 0x023D5F, 0x033839, 0x01F971,
  ], [
    0x060000, 0x08527E, 0x0B716E, 0x0FDF3C, 0x07D6E1, 0x0860CC, 0x11AB63, 0x0EADCB,
    0x060000, 0x05991C, 0x0D860D, 0x08FDA3, 0x047702, 0x0246B3, 0x063165, 0x03DFEA,
    0x0CBD2E, 0x0B29F1, 0x0EFE0F, 0x0BC78B, 0x07B096, 0x03EB93, 0x081D24, 0x038732,
    0x0D3A9C, 0x12595D, 0x0BE7DF, 0x108384, 0x0AE000, 0x0F1585, 0x096DA8, 0x0BD9FA,
    0x0D3A9C, 0x0BE7DF, 0x0B29F1, 0x0D2A50, 0x071A6B, 0x0B4289, 0x04C25F, 0x0699EE,
    0x0A9B4A, 0x098BB2, 0x07D727, 0x06D01A, 0x10A4F0, 0x0EFABE, 0x0F8E3C, 0x0D7B2E,
    0x0DE000, 0x0A0482, 0x0AE6CA, 0x081A2F, 0x06E677, 0x034F90, 0x0594B9, 0x02C38C,
    0x07B649, 0x060F2F, 0x05CB3C, 0x059BE7, 0x0545A7, 0x02B00C, 0x03DD11, 0x025E87,
  ], [
    0x080000, 0x0B18A8, 0x0F41E8, 0x1529A5, 0x0A73D7, 0x0B2BBB, 0x178F2F, 0x139264,
    0x080000, 0x0776CF, 0x120812, 0x0BFCD9, 0x05F402, 0x0308EF, 0x0841DC, 0x052A8E,
    0x10FC3E, 0x0EE297, 0x13FD69, 0x0FB4B9, 0x0A40C8, 0x053A1A, 0x0AD186, 0x04B442,
    0x11A37B, 0x187727, 0x0FDFD4, 0x1604B0, 0x0E8000, 0x141CB1, 0x0C9235, 0x0FCD4D,
    0x11A37B, 0x0FDFD4, 0x0EE297, 0x118DC0, 0x09788F, 0x0F0362, 0x06587F, 0x08CD3D,
    0x0E2463, 0x0CBA43, 0x0A7434, 0x091577, 0x163140, 0x13F8FE, 0x14BDA5, 0x11F992,
    0x128000, 0x0D5B58, 0x0E890D, 0x0ACD94, 0x093349, 0x046A15, 0x0770F7, 0x03AF65,
    0x0A4861, 0x08143F, 0x07B9A6, 0x077A89, 0x070789, 0x039565, 0x0526C2, 0x0328B4,
  ], [
    0x0C0000, 0x10A4FD, 0x16E2DB, 0x1FBE78, 0x0FADC3, 0x10C198, 0x2356C7, 0x1D5B96,
    0x0C0000, 0x0B3237, 0x1B0C1A, 0x11FB46, 0x08EE03, 0x048D66, 0x0C62CA, 0x07BFD5,
    0x197A5D, 0x1653E3, 0x1DFC1E, 0x178F16, 0x0F612C, 0x07D727, 0x103A49, 0x070E64,
    0x1A7539, 0x24B2BB, 0x17CFBD, 0x210709, 0x15C000, 0x1E2B0A, 0x12DB4F, 0x17B3F4,
    0x1A7539, 0x17CFBD, 0x1653E3, 0x1A54A0, 0x0E34D7, 0x168513, 0x0984BE, 0x0D33DC,
    0x153695, 0x131765, 0x0FAE4E, 0x0DA033, 0x2149E1, 0x1DF57D, 0x1F1C78, 0x1AF65B,
    0x1BC000, 0x140904, 0x15CD94, 0x10345E, 0x0DCCEE, 0x069F20, 0x0B2972, 0x058718,
    0x0F6C91, 0x0C1E5E, 0x0B9678, 0x0B37CE, 0x0A8B4E, 0x056018, 0x07BA22, 0x04BD0E,
  ], [
    0x110000, 0x179466, 0x206C0C, 0x2CF87F, 0x16362A, 0x17BCED, 0x321044, 0x299714,
    0x110000, 0x0FDC79, 0x265125, 0x19794E, 0x0CA685, 0x0672FB, 0x118BF4, 0x0AFA6D,
    0x241804, 0x1FA181, 0x2A7A80, 0x21600A, 0x15C9A9, 0x0B1B77, 0x16FD3C, 0x09FF0D,
    0x257B66, 0x33FD33, 0x21BBA2, 0x2EC9F7, 0x1ED000, 0x2ABCF9, 0x1AB6B0, 0x219444,
    0x257B66, 0x21BBA2, 0x1FA181, 0x254D38, 0x142030, 0x1FE730, 0x0D7C0E, 0x12B423,
    0x1E0D52, 0x1B0BCF, 0x1636EE, 0x134D9E, 0x2F28A9, 0x2A711B, 0x2C12FF, 0x263256,
    0x275000, 0x1C621B, 0x1EE33C, 0x16F4DB, 0x138CFB, 0x09616E, 0x0FD00C, 0x07D4B7,
    0x15D9CE, 0x112B06, 0x106A80, 0x0FE464, 0x0EF004, 0x079D77, 0x0AF25B, 0x06B67F,
  ], [
    0x160000, 0x1E83CF, 0x29F53D, 0x3A3286, 0x1CBE90, 0x1EB842, 0x40C9C2, 0x35D293,
    0x160000, 0x1486BA, 0x319630, 0x20F756, 0x105F06, 0x085891, 0x16B51E, 0x0E3506,
    0x2EB5AA, 0x28EF20, 0x36F8E1, 0x2B30FE, 0x1C3225, 0x0E5FC7, 0x1DC030, 0x0CEFB7,
    0x308193, 0x4347AC, 0x2BA786, 0x3C8CE5, 0x27E000, 0x374EE7, 0x229212, 0x2B7494,
    0x308193, 0x2BA786, 0x28EF20, 0x3045D0, 0x1A0B89, 0x29494D, 0x11735D, 0x183469,
    0x26E410, 0x230039, 0x1CBF8F, 0x18FB09, 0x3D0771, 0x36ECBA, 0x390986, 0x316E52,
    0x32E000, 0x24BB33, 0x27F8E4, 0x1DB557, 0x194D09, 0x0C23BB, 0x1476A6, 0x0A2256,
    0x1C470A, 0x1637AD, 0x153E87, 0x1490FA, 0x1354B9, 0x09DAD6, 0x0E2A94, 0x08AFF0,
  ], [
    0x1C0000, 0x26D64D, 0x3566AA, 0x4A11C2, 0x249572, 0x27190E, 0x527525, 0x44805E,
    0x1C0000, 0x1A1FD6, 0x3F1C3E, 0x29F4F9, 0x14D607, 0x0A9F44, 0x1CE683, 0x1214F0,
    0x3B72D9, 0x341911, 0x45F6F0, 0x36F889, 0x23E2BB, 0x124B5B, 0x25DD54, 0x1076E9,
    0x3DBC30, 0x55A109, 0x378F64, 0x4D1069, 0x32C000, 0x46646C, 0x2BFFB9, 0x374E8E,
    0x3DBC30, 0x378F64, 0x341911, 0x3D7020, 0x2125F5, 0x348BD6, 0x1635BC, 0x1ECE57,
    0x317F5B, 0x2C8BEB, 0x2496B6, 0x1FCB22, 0x4DAC61, 0x45E778, 0x4897C2, 0x3EE97F,
    0x40C000, 0x2EBFB5, 0x32DFAE, 0x25CF86, 0x203380, 0x0F734B, 0x1A0B5F, 0x0CE5E2,
    0x23FD53, 0x1C46DC, 0x1B09C4, 0x1A2CE1, 0x189A60, 0x0C8AE2, 0x1207A5, 0x0B0E77,
  ], [
    0x220000, 0x2F28CC, 0x40D818, 0x59F0FE, 0x2C6C53, 0x2F79DA, 0x642089, 0x532E29,
    0x220000, 0x1FB8F1, 0x4CA24B, 0x32F29C, 0x194D09, 0x0CE5F7, 0x2317E8, 0x15F4DB,
    0x483007, 0x3F4303, 0x54F4FF, 0x42C014, 0x2B9351, 0x1636EE, 0x2DFA79, 0x13FE1A,
    0x4AF6CC, 0x67FA67, 0x437743, 0x5D93EE, 0x3DA000, 0x5579F1, 0x356D61, 0x432888,
    0x4AF6CC, 0x437743, 0x3F4303, 0x4A9A70, 0x284060, 0x3FCE60, 0x1AF81B, 0x256845,
    0x3C1AA5, 0x36179D, 0x2C6DDD, 0x269B3C, 0x5E5152, 0x54E237, 0x5825FE, 0x4C64AD,
    0x4EA000, 0x38C437, 0x3DC678, 0x2DE9B5, 0x2719F7, 0x12C2DB, 0x1FA018, 0x0FA96E,
    0x2BB39B, 0x22560C, 0x20D500, 0x1FC8C8, 0x1DE007, 0x0F3AEE, 0x15E4B7, 0x0D6CFE,
  ], [
    0x2C0000, 0x3D079E, 0x53EA79, 0x74650C, 0x397D20, 0x3D7083, 0x819383, 0x6BA525,
    0x2C0000, 0x290D75, 0x632C61, 0x41EEAC, 0x20BE0C, 0x10B121, 0x2D6A3B, 0x1C6A0C,
    0x5D6B54, 0x51DE40, 0x6DF1C2, 0x5661FB, 0x38644B, 0x1CBF8F, 0x3B8060, 0x19DF6D,
    0x610326, 0x868F57, 0x574F0B, 0x7919CA, 0x4FC000, 0x6E9DCE, 0x452423, 0x56E928,
    0x610326, 0x574F0B, 0x51DE40, 0x608BA0, 0x341713, 0x52929A, 0x22E6BA, 0x3068D2,
    0x4DC821, 0x460071, 0x397F1E, 0x31F611, 0x7A0EE2, 0x6DD974, 0x72130C, 0x62DCA3,
    0x65C000, 0x497665, 0x4FF1C9, 0x3B6AAE, 0x329A12, 0x184776, 0x28ED4D, 0x1444AC,
    0x388E14, 0x2C6F5A, 0x2A7D0F, 0x2921F4, 0x26A973, 0x13B5AD, 0x1C5528, 0x115FDF,
  ]
];

const BINK_INTER_QUANT: &[[i32; 64]; 16] = &[
  [
    0x010000, 0x017946, 0x01A5A9, 0x0248DC, 0x016363, 0x0152A7, 0x0243EC, 0x0209EA,
    0x012000, 0x00E248, 0x01BBDA, 0x015CBC, 0x00A486, 0x0053E0, 0x00F036, 0x008095,
    0x01B701, 0x016959, 0x01B0B9, 0x0153FD, 0x00F8E7, 0x007EE4, 0x00EA30, 0x007763,
    0x01B701, 0x0260EB, 0x019DE9, 0x023E1B, 0x017000, 0x01FE6E, 0x012DB5, 0x01A27B,
    0x01E0D1, 0x01B0B9, 0x018A33, 0x01718D, 0x00D87A, 0x014449, 0x007B9A, 0x00AB71,
    0x013178, 0x0112EA, 0x00AD08, 0x009BB9, 0x023D97, 0x020437, 0x021CCC, 0x01E6B4,
    0x018000, 0x012DB5, 0x0146D9, 0x0100CE, 0x00CFD2, 0x006E5C, 0x00B0E4, 0x005A2D,
    0x00E9CC, 0x00B7B1, 0x00846F, 0x006B85, 0x008337, 0x0042E5, 0x004A10, 0x002831,
  ], [
    0x015555, 0x01F708, 0x023237, 0x030BD0, 0x01D9D9, 0x01C389, 0x03053B, 0x02B7E3,
    0x018000, 0x012DB5, 0x024FCE, 0x01D0FA, 0x00DB5D, 0x006FD5, 0x014048, 0x00AB71,
    0x024957, 0x01E1CC, 0x0240F7, 0x01C551, 0x014BDE, 0x00A92F, 0x013840, 0x009F2F,
    0x024957, 0x032BE4, 0x0227E1, 0x02FD7A, 0x01EAAB, 0x02A893, 0x019247, 0x022DF9,
    0x028116, 0x0240F7, 0x020D99, 0x01ECBC, 0x0120A3, 0x01B061, 0x00A4CE, 0x00E497,
    0x01974B, 0x016E8E, 0x00E6B5, 0x00CFA2, 0x02FCC9, 0x02B04A, 0x02D110, 0x0288F1,
    0x020000, 0x019247, 0x01B3CC, 0x015668, 0x011518, 0x009325, 0x00EBDA, 0x00783D,
    0x0137BB, 0x00F4ED, 0x00B093, 0x008F5C, 0x00AEF4, 0x005931, 0x0062BF, 0x003597,
  ], [
    0x01AAAB, 0x0274CB, 0x02BEC4, 0x03CEC4, 0x02504F, 0x02346C, 0x03C689, 0x0365DC,
    0x01E000, 0x017922, 0x02E3C1, 0x024539, 0x011235, 0x008BCA, 0x01905A, 0x00D64D,
    0x02DBAD, 0x025A40, 0x02D134, 0x0236A5, 0x019ED6, 0x00D37B, 0x018650, 0x00C6FB,
    0x02DBAD, 0x03F6DD, 0x02B1D9, 0x03BCD8, 0x026555, 0x0352B8, 0x01F6D8, 0x02B977,
    0x03215C, 0x02D134, 0x029100, 0x0267EB, 0x0168CC, 0x021C7A, 0x00CE01, 0x011DBD,
    0x01FD1E, 0x01CA31, 0x012062, 0x01038A, 0x03BBFB, 0x035C5C, 0x038554, 0x032B2D,
    0x028000, 0x01F6D8, 0x0220C0, 0x01AC02, 0x015A5E, 0x00B7EF, 0x0126D1, 0x00964C,
    0x0185A9, 0x013228, 0x00DCB8, 0x00B333, 0x00DAB2, 0x006F7D, 0x007B6F, 0x0042FC,
  ], [
    0x020000, 0x02F28D, 0x034B52, 0x0491B8, 0x02C6C5, 0x02A54E, 0x0487D8, 0x0413D5,
    0x024000, 0x01C48F, 0x0377B5, 0x02B977, 0x01490C, 0x00A7BF, 0x01E06C, 0x01012A,
    0x036E03, 0x02D2B3, 0x036172, 0x02A7FA, 0x01F1CE, 0x00FDC7, 0x01D460, 0x00EEC7,
    0x036E03, 0x04C1D6, 0x033BD1, 0x047C37, 0x02E000, 0x03FCDD, 0x025B6A, 0x0344F5,
    0x03C1A1, 0x036172, 0x031466, 0x02E31B, 0x01B0F5, 0x028892, 0x00F735, 0x0156E2,
    0x0262F1, 0x0225D5, 0x015A10, 0x013772, 0x047B2D, 0x04086E, 0x043998, 0x03CD69,
    0x030000, 0x025B6A, 0x028DB3, 0x02019B, 0x019FA3, 0x00DCB8, 0x0161C7, 0x00B45B,
    0x01D398, 0x016F63, 0x0108DD, 0x00D70A, 0x01066F, 0x0085C9, 0x00941F, 0x005062,
  ], [
    0x02AAAB, 0x03EE11, 0x04646D, 0x0617A0, 0x03B3B2, 0x038713, 0x060A75, 0x056FC6,
    0x030000, 0x025B6A, 0x049F9B, 0x03A1F4, 0x01B6BB, 0x00DFAA, 0x028090, 0x0156E2,
    0x0492AE, 0x03C399, 0x0481ED, 0x038AA2, 0x0297BD, 0x01525F, 0x027080, 0x013E5E,
    0x0492AE, 0x0657C8, 0x044FC1, 0x05FAF4, 0x03D555, 0x055126, 0x03248D, 0x045BF2,
    0x05022D, 0x0481ED, 0x041B33, 0x03D979, 0x024147, 0x0360C3, 0x01499C, 0x01C92E,
    0x032E96, 0x02DD1C, 0x01CD6A, 0x019F43, 0x05F991, 0x056093, 0x05A220, 0x0511E1,
    0x040000, 0x03248D, 0x036799, 0x02ACCF, 0x022A2F, 0x01264B, 0x01D7B5, 0x00F079,
    0x026F75, 0x01E9D9, 0x016127, 0x011EB8, 0x015DE9, 0x00B262, 0x00C57F, 0x006B2D,
  ], [
    0x038000, 0x052876, 0x05C3CF, 0x07FF02, 0x04DBD9, 0x04A148, 0x07EDBA, 0x0722B4,
    0x03F000, 0x0317FB, 0x06117C, 0x04C491, 0x023FD5, 0x01258F, 0x0348BD, 0x01C209,
    0x060085, 0x04F0B9, 0x05EA87, 0x04A5F5, 0x036728, 0x01BC1C, 0x0333A8, 0x01A1DB,
    0x060085, 0x085336, 0x05A8AE, 0x07D960, 0x050800, 0x06FA82, 0x041FF9, 0x05B8AE,
    0x0692DA, 0x05EA87, 0x0563B2, 0x050D6E, 0x02F5AD, 0x046F00, 0x01B09C, 0x02580C,
    0x042D25, 0x03C235, 0x025D9B, 0x022108, 0x07D78F, 0x070EC1, 0x0764CA, 0x06A777,
    0x054000, 0x041FF9, 0x0477F9, 0x0382D0, 0x02D75E, 0x018242, 0x026B1D, 0x013B9F,
    0x03324A, 0x0282ED, 0x01CF83, 0x017851, 0x01CB42, 0x00EA21, 0x010336, 0x008CAC,
  ], [
    0x040000, 0x05E519, 0x0696A4, 0x092370, 0x058D8A, 0x054A9C, 0x090FB0, 0x0827AA,
    0x048000, 0x03891F, 0x06EF69, 0x0572EE, 0x029218, 0x014F7E, 0x03C0D8, 0x020254,
    0x06DC05, 0x05A565, 0x06C2E4, 0x054FF3, 0x03E39B, 0x01FB8E, 0x03A8C0, 0x01DD8D,
    0x06DC05, 0x0983AC, 0x0677A2, 0x08F86E, 0x05C000, 0x07F9B9, 0x04B6D4, 0x0689EB,
    0x078343, 0x06C2E4, 0x0628CC, 0x05C635, 0x0361EA, 0x051124, 0x01EE69, 0x02ADC5,
    0x04C5E1, 0x044BAA, 0x02B41F, 0x026EE5, 0x08F65A, 0x0810DD, 0x087330, 0x079AD1,
    0x060000, 0x04B6D4, 0x051B65, 0x040337, 0x033F47, 0x01B970, 0x02C38F, 0x0168B6,
    0x03A730, 0x02DEC6, 0x0211BA, 0x01AE14, 0x020CDD, 0x010B93, 0x01283E, 0x00A0C4,
  ], [
    0x050000, 0x075E60, 0x083C4D, 0x0B6C4C, 0x06F0ED, 0x069D43, 0x0B539C, 0x0A3194,
    0x05A000, 0x046B67, 0x08AB44, 0x06CFAA, 0x03369E, 0x01A35E, 0x04B10F, 0x0282E8,
    0x089307, 0x070EBF, 0x08739C, 0x06A3F0, 0x04DC82, 0x027A72, 0x0492F0, 0x0254F0,
    0x089307, 0x0BE497, 0x08158B, 0x0B3689, 0x073000, 0x09F827, 0x05E489, 0x082C66,
    0x096413, 0x08739C, 0x07B2FF, 0x0737C2, 0x043A64, 0x06556D, 0x026A04, 0x035936,
    0x05F75A, 0x055E94, 0x036127, 0x030A9E, 0x0B33F1, 0x0A1514, 0x0A8FFC, 0x098186,
    0x078000, 0x05E489, 0x06623F, 0x050405, 0x040F19, 0x0227CC, 0x037473, 0x01C2E3,
    0x0490FC, 0x039677, 0x029629, 0x021999, 0x029015, 0x014E78, 0x01724E, 0x00C8F5,
  ], [
    0x060000, 0x08D7A6, 0x09E1F6, 0x0DB528, 0x085450, 0x07EFEA, 0x0D9788, 0x0C3B7E,
    0x06C000, 0x054DAE, 0x0A671E, 0x082C66, 0x03DB24, 0x01F73E, 0x05A145, 0x03037D,
    0x0A4A08, 0x087818, 0x0A2455, 0x07F7ED, 0x05D569, 0x02F955, 0x057D20, 0x02CC54,
    0x0A4A08, 0x0E4582, 0x09B373, 0x0D74A5, 0x08A000, 0x0BF696, 0x07123E, 0x09CEE0,
    0x0B44E4, 0x0A2455, 0x093D32, 0x08A950, 0x0512DF, 0x0799B6, 0x02E59E, 0x0404A7,
    0x0728D2, 0x06717F, 0x040E2F, 0x03A657, 0x0D7187, 0x0C194B, 0x0CACC8, 0x0B683A,
    0x090000, 0x07123E, 0x07A918, 0x0604D2, 0x04DEEA, 0x029629, 0x042556, 0x021D11,
    0x057AC8, 0x044E28, 0x031A97, 0x02851E, 0x03134C, 0x01915C, 0x01BC5D, 0x00F126,
  ], [
    0x080000, 0x0BCA33, 0x0D2D48, 0x1246E0, 0x0B1B15, 0x0A9538, 0x121F5F, 0x104F53,
    0x090000, 0x07123E, 0x0DDED2, 0x0AE5DD, 0x052430, 0x029EFD, 0x0781B1, 0x0404A7,
    0x0DB80B, 0x0B4ACB, 0x0D85C7, 0x0A9FE7, 0x07C736, 0x03F71D, 0x075180, 0x03BB1A,
    0x0DB80B, 0x130757, 0x0CEF44, 0x11F0DC, 0x0B8000, 0x0FF372, 0x096DA8, 0x0D13D6,
    0x0F0686, 0x0D85C7, 0x0C5198, 0x0B8C6A, 0x06C3D4, 0x0A2248, 0x03DCD3, 0x055B8A,
    0x098BC3, 0x089754, 0x05683E, 0x04DDC9, 0x11ECB4, 0x1021B9, 0x10E661, 0x0F35A3,
    0x0C0000, 0x096DA8, 0x0A36CB, 0x08066E, 0x067E8E, 0x0372E1, 0x05871E, 0x02D16B,
    0x074E60, 0x05BD8B, 0x042374, 0x035C28, 0x0419BB, 0x021726, 0x02507C, 0x014188,
  ], [
    0x0C0000, 0x11AF4C, 0x13C3EC, 0x1B6A50, 0x10A89F, 0x0FDFD4, 0x1B2F0F, 0x1876FD,
    0x0D8000, 0x0A9B5D, 0x14CE3C, 0x1058CB, 0x07B649, 0x03EE7B, 0x0B4289, 0x0606FB,
    0x149410, 0x10F030, 0x1448AB, 0x0FEFDA, 0x0BAAD2, 0x05F2AB, 0x0AFA40, 0x0598A7,
    0x149410, 0x1C8B03, 0x1366E6, 0x1AE949, 0x114000, 0x17ED2B, 0x0E247C, 0x139DC1,
    0x1689C8, 0x1448AB, 0x127A63, 0x11529F, 0x0A25BE, 0x0F336D, 0x05CB3C, 0x08094E,
    0x0E51A4, 0x0CE2FE, 0x081C5D, 0x074CAE, 0x1AE30E, 0x183296, 0x195991, 0x16D074,
    0x120000, 0x0E247C, 0x0F5230, 0x0C09A5, 0x09BDD5, 0x052C51, 0x084AAC, 0x043A21,
    0x0AF590, 0x089C51, 0x06352E, 0x050A3B, 0x062698, 0x0322B9, 0x0378BA, 0x01E24D,
  ], [
    0x110000, 0x190DAC, 0x1C0039, 0x26D69C, 0x17998C, 0x167D16, 0x2682AB, 0x22A891,
    0x132000, 0x0F06C3, 0x1D797F, 0x172876, 0x0AECE7, 0x0591D9, 0x0FF398, 0x0889E3,
    0x1D2717, 0x17FEEF, 0x1CBC47, 0x1693CA, 0x108754, 0x086D1D, 0x0F8D30, 0x07ED98,
    0x1D2717, 0x286F9A, 0x1B7C71, 0x261FD3, 0x187000, 0x21E552, 0x140904, 0x1BCA27,
    0x1FEDDC, 0x1CBC47, 0x1A2D62, 0x188A62, 0x0E6022, 0x1588DA, 0x083540, 0x0B6284,
    0x1448FE, 0x124192, 0x0B7D84, 0x0A574B, 0x2616FF, 0x2247AA, 0x23E98D, 0x2051FA,
    0x198000, 0x140904, 0x15B46F, 0x110DAA, 0x0DCCEE, 0x07541E, 0x0BBF1F, 0x05FD04,
    0x0F868B, 0x0C32C8, 0x08CB57, 0x0723D4, 0x08B6AD, 0x047130, 0x04EB08, 0x02AB42,
  ], [
    0x160000, 0x206C0C, 0x243C86, 0x3242E8, 0x1E8A79, 0x1D1A59, 0x31D646, 0x2CDA25,
    0x18C000, 0x13722A, 0x2624C3, 0x1DF820, 0x0E2385, 0x073537, 0x14A4A7, 0x0B0CCC,
    0x25BA1D, 0x1F0DAE, 0x252FE4, 0x1D37BB, 0x1563D6, 0x0AE78E, 0x142021, 0x0A4288,
    0x25BA1D, 0x345430, 0x2391FB, 0x31565C, 0x1FA000, 0x2BDD7A, 0x19ED8D, 0x23F68C,
    0x2951EF, 0x252FE4, 0x21E061, 0x1FC224, 0x129A87, 0x1BDE47, 0x0A9F44, 0x0EBBBA,
    0x1A4058, 0x17A026, 0x0EDEAB, 0x0D61E9, 0x314AEF, 0x2C5CBE, 0x2E798A, 0x29D380,
    0x210000, 0x19ED8D, 0x1C16AE, 0x1611AE, 0x11DC06, 0x097BEA, 0x0F3391, 0x07BFE7,
    0x141787, 0x0FC93E, 0x0B617F, 0x093D6D, 0x0B46C1, 0x05BFA8, 0x065D55, 0x037437,
  ], [
    0x1C0000, 0x2943B2, 0x2E1E7C, 0x3FF810, 0x26DEC9, 0x250A43, 0x3F6DCE, 0x3915A3,
    0x1F8000, 0x18BFD8, 0x308BE1, 0x262485, 0x11FEA9, 0x092C75, 0x1A45EB, 0x0E1049,
    0x300425, 0x2785C6, 0x2F5439, 0x252FA8, 0x1B393F, 0x0DE0E4, 0x199D41, 0x0D0EDC,
    0x300425, 0x4299B2, 0x2D456E, 0x3ECB00, 0x284000, 0x37D40F, 0x20FFCB, 0x2DC56D,
    0x3496D3, 0x2F5439, 0x2B1D93, 0x286B74, 0x17AD66, 0x2377FE, 0x0D84E2, 0x12C062,
    0x21692A, 0x1E11A5, 0x12ECDA, 0x110840, 0x3EBC76, 0x387608, 0x3B2652, 0x353BBA,
    0x2A0000, 0x20FFCB, 0x23BFC6, 0x1C1681, 0x16BAF1, 0x0C1213, 0x1358E8, 0x09DCF8,
    0x19924F, 0x141767, 0x0E7C16, 0x0BC28A, 0x0E5A0D, 0x075104, 0x0819B2, 0x04655D,
  ], [
    0x220000, 0x321B58, 0x380072, 0x4DAD38, 0x2F3318, 0x2CFA2D, 0x4D0556, 0x455122,
    0x264000, 0x1E0D86, 0x3AF2FE, 0x2E50EB, 0x15D9CE, 0x0B23B2, 0x1FE730, 0x1113C7,
    0x3A4E2D, 0x2FFDDF, 0x39788E, 0x2D2795, 0x210EA8, 0x10DA39, 0x1F1A61, 0x0FDB2F,
    0x3A4E2D, 0x50DF33, 0x36F8E1, 0x4C3FA5, 0x30E000, 0x43CAA5, 0x281209, 0x37944D,
    0x3FDBB7, 0x39788E, 0x345AC4, 0x3114C3, 0x1CC044, 0x2B11B4, 0x106A80, 0x16C509,
    0x2891FC, 0x248324, 0x16FB08, 0x14AE97, 0x4C2DFD, 0x448F54, 0x47D31B, 0x40A3F5,
    0x330000, 0x281209, 0x2B68DF, 0x221B53, 0x1B99DB, 0x0EA83B, 0x177E3E, 0x0BFA09,
    0x1F0D17, 0x18658F, 0x1196AE, 0x0E47A8, 0x116D5A, 0x08E260, 0x09D60F, 0x055684,
  ], [
    0x2C0000, 0x40D818, 0x48790C, 0x6485D0, 0x3D14F2, 0x3A34B2, 0x63AC8D, 0x59B44A,
    0x318000, 0x26E454, 0x4C4986, 0x3BF03F, 0x1C470A, 0x0E6A6E, 0x29494D, 0x161998,
    0x4B743A, 0x3E1B5C, 0x4A5FC7, 0x3A6F75, 0x2AC7AC, 0x15CF1D, 0x284041, 0x148510,
    0x4B743A, 0x68A861, 0x4723F6, 0x62ACB8, 0x3F4000, 0x57BAF3, 0x33DB1A, 0x47ED19,
    0x52A3DE, 0x4A5FC7, 0x43C0C2, 0x3F8448, 0x25350D, 0x37BC8E, 0x153E87, 0x1D7775,
    0x3480B0, 0x2F404C, 0x1DBD56, 0x1AC3D2, 0x6295DE, 0x58B97B, 0x5CF313, 0x53A701,
    0x420000, 0x33DB1A, 0x382D5C, 0x2C235D, 0x23B80D, 0x12F7D4, 0x1E6723, 0x0F7FCF,
    0x282F0E, 0x1F927D, 0x16C2FF, 0x127AD9, 0x168D83, 0x0B7F50, 0x0CBAAA, 0x06E86E,
  ]
];

const BINKB_RUN_BITS: [u8; 64] = [
    6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6,
    5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5,
    4, 4, 4, 4, 4, 4, 4, 4,
    3, 3, 3, 3, 2, 2, 1, 0
];

const BINKB_REF_INTRA_Q: [u8; 64] = [
    16, 16, 16, 19, 16, 19, 22, 22,
    22, 22, 26, 24, 26, 22, 22, 27,
    27, 27, 26, 26, 26, 29, 29, 29,
    27, 27, 27, 26, 34, 34, 34, 29,
    29, 29, 27, 27, 37, 34, 34, 32,
    32, 29, 29, 38, 37, 35, 35, 34,
    35, 40, 40, 40, 38, 38, 48, 48,
    46, 46, 58, 56, 56, 69, 69, 83
];

const BINKB_REF_INTER_Q: [u8; 64] = [
    16, 17, 17, 18, 18, 18, 19, 19,
    19, 19, 20, 20, 20, 20, 20, 21,
    21, 21, 21, 21, 21, 22, 22, 22,
    22, 22, 22, 22, 23, 23, 23, 23,
    23, 23, 23, 23, 24, 24, 24, 25,
    24, 24, 24, 25, 26, 26, 26, 26,
    25, 27, 27, 27, 27, 27, 28, 28,
    28, 28, 30, 30, 30, 31, 31, 33
];

const BINKB_REF_QUANTS: [(u8, u8); 16] = [
    (1, 1), (4, 3), (5, 3), (2, 1), (7, 3), (8, 3), (3, 1), (7, 2),
    (4, 1), (9, 2), (5, 1), (6, 1), (7, 1), (8, 1), (9, 1), (10, 1)
];
