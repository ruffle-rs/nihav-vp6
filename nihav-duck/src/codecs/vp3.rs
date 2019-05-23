use std::mem;
use std::ptr;
use nihav_core::codecs::*;
use nihav_core::codecs::blockdsp::*;
use nihav_core::io::bitreader::*;
use nihav_core::io::codebook::*;
use nihav_core::io::intcode::*;
use super::vpcommon::*;

#[derive(Clone,Copy,Debug,PartialEq)]
enum SBState {
    Coded,
    Partial,
    Uncoded,
}

fn map_idx(idx: usize) -> u8 {
    idx as u8
}

struct VP31Codes {
    dc_cb:      [Codebook<u8>; 16],
    ac0_cb:     [Codebook<u8>; 16],
    ac1_cb:     [Codebook<u8>; 16],
    ac2_cb:     [Codebook<u8>; 16],
    ac3_cb:     [Codebook<u8>; 16],
}

impl VP31Codes {
    fn new() -> Self {
        let mut dc_cb: [Codebook<u8>; 16];
        let mut ac0_cb: [Codebook<u8>; 16];
        let mut ac1_cb: [Codebook<u8>; 16];
        let mut ac2_cb: [Codebook<u8>; 16];
        let mut ac3_cb: [Codebook<u8>; 16];
        unsafe {
            dc_cb = mem::uninitialized();
            ac0_cb = mem::uninitialized();
            ac1_cb = mem::uninitialized();
            ac2_cb = mem::uninitialized();
            ac3_cb = mem::uninitialized();
            for i in 0..16 {
                let mut cr = TableCodebookDescReader::new(&VP31_DC_CODES[i], &VP31_DC_BITS[i], map_idx);
                let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
                ptr::write(&mut dc_cb[i], cb);

                let mut cr = TableCodebookDescReader::new(&VP31_AC_CAT0_CODES[i], &VP31_AC_CAT0_BITS[i], map_idx);
                let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
                ptr::write(&mut ac0_cb[i], cb);
                let mut cr = TableCodebookDescReader::new(&VP31_AC_CAT1_CODES[i], &VP31_AC_CAT1_BITS[i], map_idx);
                let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
                ptr::write(&mut ac1_cb[i], cb);
                let mut cr = TableCodebookDescReader::new(&VP31_AC_CAT2_CODES[i], &VP31_AC_CAT2_BITS[i], map_idx);
                let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
                ptr::write(&mut ac2_cb[i], cb);
                let mut cr = TableCodebookDescReader::new(&VP31_AC_CAT3_CODES[i], &VP31_AC_CAT3_BITS[i], map_idx);
                let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
                ptr::write(&mut ac3_cb[i], cb);
            }
        }
        Self { dc_cb, ac0_cb, ac1_cb, ac2_cb, ac3_cb }
    }
}

#[derive(Clone)]
struct Block {
    btype:      VPMBType,
    coeffs:     [i16; 64],
    has_ac:     bool,
    idx:        usize,
    mv:         MV,
    coded:      bool,
}

impl Block {
    fn new() -> Self {
        Self {
            btype:      VPMBType::Intra,
            coeffs:     [0; 64],
            has_ac:     false,
            idx:        0,
            mv:         ZERO_MV,
            coded:      false,
        }
    }
}

type ReadRunFunc = fn (&mut BitReader) -> DecoderResult<usize>;

const VP31_LONG_RUN_BASE: [usize; 7] = [ 1, 2, 4, 6, 10, 18, 34 ];
const VP31_LONG_RUN_BITS: [u8;    7] = [ 0, 1, 1, 2,  3,  4, 12 ];
fn read_long_run(br: &mut BitReader) -> DecoderResult<usize> {
    let pfx                                     = br.read_code(UintCodeType::LimitedUnary(6, 0))? as usize;
    if pfx == 0 { return Ok(1); }
    Ok(VP31_LONG_RUN_BASE[pfx] + (br.read(VP31_LONG_RUN_BITS[pfx])? as usize))
}

const VP31_SHORT_RUN_BASE: [usize; 6] = [ 1, 3, 5, 7, 11, 15 ];
const VP31_SHORT_RUN_BITS: [u8;    6] = [ 1, 1, 1, 2,  2,  4 ];
fn read_short_run(br: &mut BitReader) -> DecoderResult<usize> {
    let pfx                                     = br.read_code(UintCodeType::LimitedUnary(5, 0))? as usize;
    Ok(VP31_SHORT_RUN_BASE[pfx] + (br.read(VP31_SHORT_RUN_BITS[pfx])? as usize))
}

struct BitRunDecoder {
    value:      bool,
    run:        usize,
    read_run:   ReadRunFunc,
}

impl BitRunDecoder {
    fn new(br: &mut BitReader, read_run: ReadRunFunc) -> DecoderResult<Self> {
        let value                               = !br.read_bool()?; // it will be flipped before run decoding
        Ok(Self { value, run: 0, read_run })
    }
    fn get_val(&mut self, br: &mut BitReader) -> DecoderResult<bool> {
        if self.run == 0 {
            self.value = !self.value;
            self.run = (self.read_run)(br)?;
        }
        self.run -= 1;
        Ok(self.value)        
    }
}

struct VP34Decoder {
    info:       NACodecInfoRef,
    width:      usize,
    height:     usize,
    mb_w:       usize,
    mb_h:       usize,
    version:    u8,
    is_intra:   bool,
    quant:      usize,
    shuf:       VPShuffler,
    codes:      VP31Codes,
    loop_str:   i16,

    blocks:     Vec<Block>,
    y_blocks:   usize,
    y_sbs:      usize,
    qmat_y:     [i16; 64],
    qmat_c:     [i16; 64],
    qmat_inter: [i16; 64],

    eob_run:    usize,
    last_dc:    [i16; 3],

    blk_addr:   Vec<usize>,
    sb_info:    Vec<SBState>,
    sb_blocks:  Vec<u8>,
}

fn read_mv_comp_packed(br: &mut BitReader) -> DecoderResult<i16> {
    let code                                    = br.read(3)?;
    match code {
        0 => Ok(0),
        1 => Ok(1),
        2 => Ok(-1),
        3 => if br.read_bool()? { Ok(-2) } else { Ok(2) },
        4 => if br.read_bool()? { Ok(-3) } else { Ok(3) },
        5 => {
            let val                             = (br.read(2)? as i16) + 4;
            if br.read_bool()? {
                Ok(-val)
            } else {
                Ok(val)
            }
        },
        6 => {
            let val                             = (br.read(3)? as i16) + 8;
            if br.read_bool()? {
                Ok(-val)
            } else {
                Ok(val)
            }
        },
        _ => {
            let val                             = (br.read(4)? as i16) + 16;
            if br.read_bool()? {
                Ok(-val)
            } else {
                Ok(val)
            }
        },
    }
}

fn read_mv_packed(br: &mut BitReader) -> DecoderResult<MV> {
    let x = read_mv_comp_packed(br)?;
    let y = read_mv_comp_packed(br)?;
    Ok(MV{ x, y })
}

fn read_mv_comp_raw(br: &mut BitReader) -> DecoderResult<i16> {
    let val                                     = br.read(5)? as i16;
    if br.read_bool()? {
        Ok(-val)
    } else {
        Ok(val)
    }
}

fn read_mv_raw(br: &mut BitReader) -> DecoderResult<MV> {
    let x = read_mv_comp_raw(br)?;
    let y = read_mv_comp_raw(br)?;
    Ok(MV{ x, y })
}

fn rescale_qmat(dst_qmat: &mut [i16; 64], base_qmat: &[i16; 64], dc_quant: i16, ac_quant: i16) {
    for (dst, src) in dst_qmat.iter_mut().zip(base_qmat.iter()) {
        *dst = (src.wrapping_mul(ac_quant) / 100).max(2) << 2;
    }
    dst_qmat[0] = (base_qmat[0] * dc_quant / 100).max(4) << 2;
}

fn expand_token(blk: &mut Block, br: &mut BitReader, eob_run: &mut usize, coef_no: usize, token: u8) -> DecoderResult<()> {
    match token {
        // EOBs
        0 | 1 | 2 => { *eob_run = (token as usize) + 1; },
        3 | 4 | 5 => {
            let bits = token - 1;
            *eob_run                            = (br.read(bits)? as usize) + (1 << bits);
        },
        6 => { *eob_run                         = br.read(12)? as usize; },
        // zero runs
        7 | 8 => {
            let bits = if token == 7 { 3 } else { 6 };
            let run                             = (br.read(bits)? as usize) + 1;
            blk.idx += run;
            validate!(blk.idx <= 64);
        },
        // single coefficients
        9 | 10 | 11 | 12 => {
            let val = (i16::from(token) - 7) >> 1;
            if (token & 1) == 1 {
                blk.coeffs[ZIGZAG[blk.idx]] = val;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -val;
            }
            blk.idx += 1;
        },
        13 | 14 | 15 | 16 => {
            let val = i16::from(token) - 10;
            if !br.read_bool()? {
                blk.coeffs[ZIGZAG[blk.idx]] = val;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -val;
            }
            blk.idx += 1;
        },
        17 | 18 | 19 | 20 | 21 | 22 => {
            let add_bits = if token == 22 { 9 } else { token - 16 };
            let sign                            = br.read_bool()?;
            let val                             = (br.read(add_bits)? as i16) + VP3_LITERAL_BASE[(token - 17) as usize];
            if !sign {
                blk.coeffs[ZIGZAG[blk.idx]] = val;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -val;
            }
            blk.idx += 1;
        }
        // zero run plus coefficient
        23 | 24 | 25 | 26 | 27 => {
            blk.idx += (token - 22) as usize;
            validate!(blk.idx < 64);
            if !br.read_bool()? {
                blk.coeffs[ZIGZAG[blk.idx]] = 1;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -1;
            }
            blk.idx += 1;
        },
        28 | 29 => {
            let run_bits = token - 26;
            if token == 28 {
                blk.idx += 6;
            } else {
                blk.idx += 10;
            }
            let sign                            = br.read_bool()?;
            blk.idx                            += br.read(run_bits)? as usize;
            validate!(blk.idx < 64);
            if !sign {
                blk.coeffs[ZIGZAG[blk.idx]] = 1;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -1;
            }
            blk.idx += 1;
        },
        30 => {
            blk.idx += 1;
            validate!(blk.idx < 64);
            let sign                            = br.read_bool()?;
            let val                             = (br.read(1)? as i16) + 2;
            if !sign {
                blk.coeffs[ZIGZAG[blk.idx]] = val;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -val;
            }
            blk.idx += 1;
        },
        _ => {
            let sign                            = br.read_bool()?;
            let val                             = (br.read(1)? as i16) + 2;
            blk.idx                            += (br.read(1)? as usize) + 2;
            validate!(blk.idx < 64);
            if !sign {
                blk.coeffs[ZIGZAG[blk.idx]] = val;
            } else {
                blk.coeffs[ZIGZAG[blk.idx]] = -val;
            }
            blk.idx += 1;
        },
    };
    if *eob_run > 0 {
        blk.idx = 64;
        *eob_run -= 1;
    } else if coef_no > 0 {
        blk.has_ac = true;
    }
    Ok(())
}
macro_rules! fill_dc_pred {
    ($self: expr, $ref_id: expr, $pred: expr, $pp: expr, $bit: expr, $idx: expr) => {
        if $self.blocks[$idx].coded && $self.blocks[$idx].btype.get_ref_id() == $ref_id {
            $pred[$bit] = $self.blocks[$idx].coeffs[0] as i32;
            $pp |= 1 << $bit;
        }
    };
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

fn vp3_interp11(dst: &mut [u8], dstride: usize, src: &[u8], sstride: usize, bw: usize, bh: usize)
{
    let mut didx = 0;
    let mut sidx = 0;
    for _ in 0..bh {
        for x in 0..bw {
            dst[didx + x] = (((src[sidx + x] as u16) +
                              (src[sidx + x + 1] as u16) +
                              (src[sidx + x + sstride] as u16) +
                              (src[sidx + x + sstride + 1] as u16)) >> 2) as u8;
        }
        didx += dstride;
        sidx += sstride;
    }
}

fn vp31_loop_filter(data: &mut [u8], mut off: usize, step: usize, stride: usize, loop_str: i16) {
    for _ in 0..8 {
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

fn vp31_loop_filter_v(frm: &mut NASimpleVideoFrame<u8>, x: usize, y: usize, plane: usize, loop_str: i16) {
    let off = frm.offset[plane] + x + y * frm.stride[plane];
    vp31_loop_filter(frm.data, off, 1, frm.stride[plane], loop_str);
}

fn vp31_loop_filter_h(frm: &mut NASimpleVideoFrame<u8>, x: usize, y: usize, plane: usize, loop_str: i16) {
    let off = frm.offset[plane] + x + y * frm.stride[plane];
    vp31_loop_filter(frm.data, off, frm.stride[plane], 1, loop_str);
}

pub const VP3_INTERP_FUNCS: &[blockdsp::BlkInterpFunc] = &[ vp3_interp00, vp3_interp01, vp3_interp10, vp3_interp11 ];

impl VP34Decoder {
    fn new(version: u8) -> Self {
        Self {
            info:       NACodecInfoRef::default(),
            width:      0,
            height:     0,
            mb_w:       0,
            mb_h:       0,
            version,
            is_intra:   true,
            quant:      0,
            shuf:       VPShuffler::new(),
            codes:      VP31Codes::new(),
            loop_str:   0,

            blocks:     Vec::new(),
            y_blocks:   0,
            y_sbs:      0,

            qmat_y:     [0; 64],
            qmat_c:     [0; 64],
            qmat_inter: [0; 64],

            eob_run:    0,
            last_dc:    [0; 3],

            blk_addr:   Vec::new(),
            sb_info:    Vec::new(),
            sb_blocks:  Vec::new(),
        }
    }
    fn parse_header(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.is_intra                           = !br.read_bool()?;
                                                  br.skip(1)?;
        self.quant                              = br.read(6)? as usize;
        self.loop_str = VP31_LOOP_STRENGTH[self.quant];
println!("quant = {}", self.quant);
        if self.is_intra {
            if br.peek(8) != 0 {
                unimplemented!();
            }
            let version                         = br.read(13)?;
println!("intra, ver {} (self {})", version, self.version);
            validate!((self.version == 3 && version == 1) || (self.version == 4 && version == 3));
            let coding_type                     = br.read(1)?;
            validate!(coding_type == 0);
                                                  br.skip(2)?;
        }
        Ok(())
    }
    fn vp31_unpack_sb_info(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let mut has_uncoded = false;
        let mut has_partial = false;
        {
            let mut brun = BitRunDecoder::new(br, read_long_run)?;
            for sb in self.sb_info.iter_mut() {
                if brun.get_val(br)? {
                    *sb = SBState::Partial;
                    has_partial = true;
                } else {
                    *sb = SBState::Uncoded;
                    has_uncoded = true;
                }
            }
        }
        if has_uncoded {
            let mut brun = BitRunDecoder::new(br, read_long_run)?;
            let mut cur_blk = 0;
            for (sb, nblk) in self.sb_info.iter_mut().zip(self.sb_blocks.iter()) {
                let nblks = *nblk as usize;
                if *sb != SBState::Partial && brun.get_val(br)? {
                    *sb = SBState::Coded;
                    for _ in 0..nblks {
                        let blk_idx = self.blk_addr[cur_blk] >> 2;
                        self.blocks[blk_idx].coded = true;
                        cur_blk += 1;
                    }
                } else {
                    for _ in 0..nblks {
                        let blk_idx = self.blk_addr[cur_blk] >> 2;
                        self.blocks[blk_idx].coded = false;
                        cur_blk += 1;
                    }
                }
            }
        }
        if has_partial {
            let mut brun = BitRunDecoder::new(br, read_short_run)?;
            let mut cur_blk = 0;
            for (sb, nblk) in self.sb_info.iter_mut().zip(self.sb_blocks.iter()) {
                let nblks = *nblk as usize;
                if *sb == SBState::Partial {
                    for _ in 0..nblks {
                        let blk_idx = self.blk_addr[cur_blk] >> 2;
                        self.blocks[blk_idx].coded = brun.get_val(br)?;
                        cur_blk += 1;
                    }
                } else {
                    cur_blk += nblks;
                }
            }
        }
        Ok(())
    }
    fn vp31_unpack_mb_info(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let mut modes = [VPMBType::InterNoMV; 8];
        let alphabet                            = br.read(3)? as usize;
        let raw_modes = alphabet >= 7;
        if alphabet == 0 {
            for mode in VP31_DEFAULT_MB_MODES.iter() {
                modes[br.read(3)? as usize] = *mode;
            }
        } else if alphabet < 7 {
            modes.copy_from_slice(&VP31_MB_MODES[alphabet - 1]);
        }

        let mut cur_blk = 0;
        for (sb, nblk) in self.sb_info.iter_mut().zip(self.sb_blocks.iter()).take(self.y_sbs) {
            let nblks = *nblk as usize;
            if *sb == SBState::Uncoded {
                for _ in 0..nblks {
                    self.blocks[self.blk_addr[cur_blk] >> 2].btype = VPMBType::InterNoMV;
                    cur_blk += 1;
                }
            } else {
                for _ in 0..nblks/4 {
                    let mut coded = *sb == SBState::Coded;
                    if !coded {
                        for blk in 0..4 {
                            if self.blocks[self.blk_addr[cur_blk + blk] >> 2].coded {
                                coded = true;
                                break;
                            }
                        }
                    }
                    let mode = if !coded {
                            VPMBType::InterNoMV
                        } else if !raw_modes {
                            let code            = br.read_code(UintCodeType::LimitedUnary(7, 0))?;
                            modes[code as usize]
                        } else {
                                                VP31_DEFAULT_MB_MODES[br.read(3)? as usize]
                        };
                    for _ in 0..4 {
                        self.blocks[self.blk_addr[cur_blk] >> 2].btype = mode;
                        cur_blk += 1;
                    }
                }
            }
        }
        // replicate types for chroma
        let mut off_y = 0;
        let mut off_u = self.y_blocks;
        let mut off_v = off_u + self.mb_w * self.mb_h;
        for _blk_y in 0..self.mb_h {
            for blk_x in 0..self.mb_w {
                let btype = self.blocks[off_y + blk_x * 2].btype;
                self.blocks[off_u + blk_x].btype = btype;
                self.blocks[off_v + blk_x].btype = btype;
            }
            off_y += self.mb_w * 2 * 2;
            off_u += self.mb_w;
            off_v += self.mb_w;
        }
        Ok(())
    }
    fn vp31_unpack_mv_info(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        let mut last_mv = ZERO_MV;
        let mut last2_mv = ZERO_MV;
        let read_mv                             = if br.read_bool()? { read_mv_raw } else { read_mv_packed };

        let mut cur_blk = 0;
        for _ in 0..self.y_blocks/4 {
            if self.blocks[self.blk_addr[cur_blk] >> 2].btype == VPMBType::InterFourMV {
                for _ in 0..4 {
                    let blk = &mut self.blocks[self.blk_addr[cur_blk] >> 2];
                    if blk.coded {
                        blk.mv = (read_mv)(br)?;
                        last2_mv = last_mv;
                        last_mv = blk.mv;
                    }
                    cur_blk += 1;
                }
            } else {
                let cur_mv;
                match self.blocks[self.blk_addr[cur_blk] >> 2].btype {
                    VPMBType::Intra | VPMBType::InterNoMV | VPMBType::GoldenNoMV => {
                        cur_mv = ZERO_MV;
                    },
                    VPMBType::InterMV => {
                        cur_mv = (read_mv)(br)?;
                        last2_mv = last_mv;
                        last_mv = cur_mv;
                    },
                    VPMBType::InterNearest => {
                        cur_mv = last_mv;
                    },
                    VPMBType::InterNear => {
                        cur_mv = last2_mv;
                        std::mem::swap(&mut last_mv, &mut last2_mv);
                    },
                    _ => { // GoldenMV
                        cur_mv = (read_mv)(br)?;
                    },
                };
                for _ in 0..4 {
                    self.blocks[self.blk_addr[cur_blk] >> 2].mv = cur_mv;
                    cur_blk += 1;
                }
            }
        }
        Ok(())
    }
    fn vp31_unpack_coeffs(&mut self, br: &mut BitReader, coef_no: usize, table_y: usize, table_c: usize) -> DecoderResult<()> {
        let cbs = if coef_no == 0 {
                [&self.codes.dc_cb[table_y], &self.codes.dc_cb[table_c]]
            } else if coef_no < 6 {
                [&self.codes.ac0_cb[table_y], &self.codes.ac0_cb[table_c]]
            } else if coef_no < 15 {
                [&self.codes.ac1_cb[table_y], &self.codes.ac1_cb[table_c]]
            } else if coef_no < 28 {
                [&self.codes.ac2_cb[table_y], &self.codes.ac2_cb[table_c]]
            } else {
                [&self.codes.ac3_cb[table_y], &self.codes.ac3_cb[table_c]]
            };
        for blkaddr in self.blk_addr.iter() {
            let blk: &mut Block = &mut self.blocks[blkaddr >> 2];
            if !blk.coded || blk.idx != coef_no { continue; }
            if self.eob_run > 0 {
                blk.idx = 64;
                self.eob_run -= 1;
                continue;
            }
            let cb = if (blkaddr & 3) == 0 { cbs[0] } else { cbs[1] };
            let token                           = br.read_cb(cb)?;
            expand_token(blk, br, &mut self.eob_run, coef_no, token)?;
        }
        Ok(())
    }
    fn decode_vp31(&mut self, br: &mut BitReader, frm: &mut NASimpleVideoFrame<u8>) -> DecoderResult<()> {
        for blk in self.blocks.iter_mut() {
            blk.coeffs = [0; 64];
            blk.idx = 0;
            blk.coded = false;
            blk.has_ac = false;
        }
        if self.is_intra {
            for sb in self.sb_info.iter_mut() { *sb = SBState::Coded; }
            for blk in self.blocks.iter_mut() {
                blk.btype = VPMBType::Intra;
                blk.coded = true;
            }
        } else {
            if self.shuf.get_last().is_none() || self.shuf.get_golden().is_none() {
                return Err(DecoderError::MissingReference);
            }
            self.vp31_unpack_sb_info(br)?;
            self.vp31_unpack_mb_info(br)?;
            self.vp31_unpack_mv_info(br)?;
        }
        let dc_quant = VP31_DC_SCALES[self.quant];
        let ac_quant = VP31_AC_SCALES[self.quant];
        rescale_qmat(&mut self.qmat_y, VP3_QMAT_Y, dc_quant, ac_quant);
        rescale_qmat(&mut self.qmat_c, VP3_QMAT_C, dc_quant, ac_quant);
        rescale_qmat(&mut self.qmat_inter, VP3_QMAT_INTER, dc_quant, ac_quant);

        self.eob_run = 0;
        let dc_table_y                          = br.read(4)? as usize;
        let dc_table_c                          = br.read(4)? as usize;
        self.vp31_unpack_coeffs(br, 0, dc_table_y, dc_table_c)?;
        self.restore_dcs();

        let ac_table_y                          = br.read(4)? as usize;
        let ac_table_c                          = br.read(4)? as usize;
        for coef_no in 1..64 {
            self.vp31_unpack_coeffs(br, coef_no, ac_table_y, ac_table_c)?;
        }

        if self.is_intra {
            self.output_blocks_intra(frm);
        } else {
            self.output_blocks_inter(frm);
        }
        if self.loop_str > 0 {
            self.vp31_loop_filter(frm);
        }

        Ok(())
    }
    fn decode_vp4(&mut self) -> DecoderResult<()> {
unimplemented!();
    }
    fn predict_dc(&self, bx: usize, by: usize, bw: usize, blk_idx: usize) -> i16 {
        let mut preds = [0i32; 4];
        let mut pp: usize = 0;
        let ref_id = self.blocks[blk_idx].btype.get_ref_id();
        let is_right = bx == bw - 1;
        if bx > 0 {
            fill_dc_pred!(self, ref_id, preds, pp, 0, blk_idx - 1);
            if by > 0 {
                fill_dc_pred!(self, ref_id, preds, pp, 1, blk_idx - 1 - bw);
            }
        }
        if by > 0 {
            fill_dc_pred!(self, ref_id, preds, pp, 2, blk_idx - bw);
            if !is_right {
                fill_dc_pred!(self, ref_id, preds, pp, 3, blk_idx + 1 - bw);
            }
        }
        if pp == 0 { return self.last_dc[ref_id as usize]; }
        let mut pred = 0i32;
        for i in 0..4 {
            if (pp & (1 << i)) != 0 {
                pred += (preds[i] as i32) * (VP31_DC_WEIGHTS[pp][i] as i32);
            }
        }
        pred /= VP31_DC_WEIGHTS[pp][4] as i32;
        if (pp & 7) == 7 {
            if (pred - preds[2]).abs() > 128 { return preds[2] as i16; }
            if (pred - preds[0]).abs() > 128 { return preds[0] as i16; }
            if (pred - preds[1]).abs() > 128 { return preds[1] as i16; }
        }
        pred as i16
    }
    fn restore_dcs(&mut self) {
        let blk_stride = self.mb_w * 2;
        let mut blk_idx = 0;
        self.last_dc = [0; 3];
        for by in 0..self.mb_h*2 {
            for bx in 0..self.mb_w*2 {
                if !self.blocks[blk_idx + bx].coded { continue; }
                let dc = self.predict_dc(bx, by, self.mb_w*2, blk_idx + bx);
                self.blocks[blk_idx + bx].coeffs[0] += dc;
                self.last_dc[self.blocks[blk_idx + bx].btype.get_ref_id() as usize] = self.blocks[blk_idx + bx].coeffs[0];
            }
            blk_idx += blk_stride;
        }
        let blk_stride = self.mb_w;
        for _plane in 1..3 {
            self.last_dc = [0; 3];
            for by in 0..self.mb_h {
                for bx in 0..self.mb_w {
                    if !self.blocks[blk_idx + bx].coded { continue; }
                    let dc = self.predict_dc(bx, by, self.mb_w, blk_idx + bx);
                    self.blocks[blk_idx + bx].coeffs[0] += dc;
                    self.last_dc[self.blocks[blk_idx + bx].btype.get_ref_id() as usize] = self.blocks[blk_idx + bx].coeffs[0];
                }
                blk_idx += blk_stride;
            }
        }
    }
    fn output_blocks_intra(&mut self, frm: &mut NASimpleVideoFrame<u8>) {
        let mut biter = self.blocks.iter_mut();
        for by in 0..self.mb_h*2 {
            for bx in 0..self.mb_w*2 {
                let mut blk = biter.next().unwrap();
                let qmat = if blk.btype == VPMBType::Intra { &self.qmat_y } else { &self.qmat_inter };
                blk.coeffs[0] *= qmat[0];
                if blk.has_ac {
                    unquant(&mut blk.coeffs, qmat);
                    vp_put_block(&mut blk.coeffs, bx, by, 0, frm);
                } else {
                    vp_put_block_dc(&mut blk.coeffs, bx, by, 0, frm);
                }
            }
        }
        for plane in 1..3 {
            for by in 0..self.mb_h {
                for bx in 0..self.mb_w {
                    let mut blk = biter.next().unwrap();
                    let qmat = if blk.btype == VPMBType::Intra { &self.qmat_c } else { &self.qmat_inter };
                    blk.coeffs[0] *= qmat[0];
                    if blk.has_ac {
                        unquant(&mut blk.coeffs, qmat);
                        vp_put_block(&mut blk.coeffs, bx, by, plane, frm);
                    } else {
                        vp_put_block_dc(&mut blk.coeffs, bx, by, plane, frm);
                    }
                }
            }
        }
    }
    fn output_blocks_inter(&mut self, frm: &mut NASimpleVideoFrame<u8>) {
        let mut blk_idx = 0;
        let bstride = self.mb_w * 2;
        for by in (0..self.mb_h*2).step_by(2) {
            for bx in (0..self.mb_w*2).step_by(2) {
                if self.blocks[blk_idx + bx].btype != VPMBType::InterFourMV {
                    continue;
                }
                let mv_a = self.blocks[blk_idx + bx].mv;
                let mv_b = self.blocks[blk_idx + bx + 1].mv;
                let mv_c = self.blocks[blk_idx + bx     + bstride].mv;
                let mv_d = self.blocks[blk_idx + bx + 1 + bstride].mv;
                let mut mv_sum = mv_a + mv_b + mv_c + mv_d;
                mv_sum.x = (mv_sum.x + 2) >> 2;
                mv_sum.y = (mv_sum.y + 2) >> 2;

                let src = self.shuf.get_last().unwrap();
                let mode = ((mv_a.x & 1) + (mv_a.y & 1) * 2) as usize;
                copy_block(frm, src.clone(), 0, bx * 8, by * 8, mv_a.x >> 1, mv_a.y >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                let mode = ((mv_b.x & 1) + (mv_b.y & 1) * 2) as usize;
                copy_block(frm, src.clone(), 0, bx * 8 + 8, by * 8, mv_b.x >> 1, mv_b.y >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                let mode = ((mv_c.x & 1) + (mv_c.y & 1) * 2) as usize;
                copy_block(frm, src.clone(), 0, bx * 8, by * 8 + 8, mv_c.x >> 1, mv_c.y >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                let mode = ((mv_d.x & 1) + (mv_d.y & 1) * 2) as usize;
                copy_block(frm, src.clone(), 0, bx * 8 + 8, by * 8 + 8, mv_d.x >> 1, mv_d.y >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);

                let mx = (mv_sum.x >> 1) | (mv_sum.x & 1);
                let my = (mv_sum.y >> 1) | (mv_sum.y & 1);
                let mode = ((mx & 1) + (my & 1) * 2) as usize;
                copy_block(frm, src.clone(), 1, bx * 4, by * 4, mx >> 1, my >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                copy_block(frm, src.clone(), 2, bx * 4, by * 4, mx >> 1, my >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
            }
            blk_idx += bstride;
        }

        let mut biter = self.blocks.iter_mut();
        for by in 0..self.mb_h*2 {
            for bx in 0..self.mb_w*2 {
                let mut blk = biter.next().unwrap();
                // do MC for whole macroblock
                if !blk.btype.is_intra() && (((bx | by) & 1) == 0) && (blk.btype != VPMBType::InterFourMV) {
                    let src = if blk.btype.get_ref_id() == 1 {
                            self.shuf.get_last().unwrap()
                        } else {
                            self.shuf.get_golden().unwrap()
                        };
                    let mode = ((blk.mv.x & 1) + (blk.mv.y & 1) * 2) as usize;
                    copy_block(frm, src.clone(), 0, bx * 8, by * 8, blk.mv.x >> 1, blk.mv.y >> 1, 16, 16, 0, 1, mode, VP3_INTERP_FUNCS);
                    let mx = (blk.mv.x >> 1) | (blk.mv.x & 1);
                    let my = (blk.mv.y >> 1) | (blk.mv.y & 1);
                    let mode = ((mx & 1) + (my & 1) * 2) as usize;
                    copy_block(frm, src.clone(), 1, bx * 4, by * 4, mx >> 1, my >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                    copy_block(frm, src.clone(), 2, bx * 4, by * 4, mx >> 1, my >> 1, 8, 8, 0, 1, mode, VP3_INTERP_FUNCS);
                }
                let qmat = if blk.btype.is_intra() { &self.qmat_y } else { &self.qmat_inter };
                blk.coeffs[0] *= qmat[0];
                if blk.has_ac {
                    unquant(&mut blk.coeffs, qmat);
                }
                if blk.btype.is_intra() {
                    if !blk.coded {
                        copy_block(frm, self.shuf.get_last().unwrap(), 0, bx * 8, by * 8, 0, 0, 8, 8, 0, 1, 0, VP3_INTERP_FUNCS);
                    } else if blk.has_ac {
                        vp_put_block(&mut blk.coeffs, bx, by, 0, frm);
                    } else {
                        vp_put_block_dc(&mut blk.coeffs, bx, by, 0, frm);
                    }
                } else if blk.coded {
                    if blk.has_ac {
                        vp_add_block(&mut blk.coeffs, bx, by, 0, frm);
                    } else {
                        vp_add_block_dc(&mut blk.coeffs, bx, by, 0, frm);
                    }
                }
            }
        }
        for plane in 1..3 {
            for by in 0..self.mb_h {
                for bx in 0..self.mb_w {
                    let mut blk = biter.next().unwrap();
                    let qmat = if blk.btype.is_intra() { &self.qmat_c } else { &self.qmat_inter };
                    blk.coeffs[0] *= qmat[0];
                    if blk.has_ac {
                        unquant(&mut blk.coeffs, qmat);
                    }
                    if blk.btype.is_intra() {
                        if !blk.coded {
                            copy_block(frm, self.shuf.get_last().unwrap(), plane, bx * 8, by * 8, 0, 0, 8, 8, 0, 1, 0, VP3_INTERP_FUNCS);
                        } else if blk.has_ac {
                            vp_put_block(&mut blk.coeffs, bx, by, plane, frm);
                        } else {
                            vp_put_block_dc(&mut blk.coeffs, bx, by, plane, frm);
                        }
                    } else if blk.coded {
                        if blk.has_ac {
                            vp_add_block(&mut blk.coeffs, bx, by, plane, frm);
                        } else {
                            vp_add_block_dc(&mut blk.coeffs, bx, by, plane, frm);
                        }
                    }
                }
            }
        }
    }
    fn vp31_loop_filter(&mut self, frm: &mut NASimpleVideoFrame<u8>) {
        let mut blk_idx = 0;
        let blk_w = self.mb_w * 2;
        for by in 0..self.mb_h*2 {
            for bx in 0..blk_w {
                let blk = &self.blocks[blk_idx + bx];
                if (bx > 0) && blk.coded {
                    vp31_loop_filter_v(frm, bx * 8, by * 8, 0, self.loop_str);
                }
                if (by > 0) && blk.coded {
                    vp31_loop_filter_h(frm, bx * 8, by * 8, 0, self.loop_str);
                }
                if (bx < blk_w - 1) && !self.blocks[blk_idx + bx + 1].coded {
                    vp31_loop_filter_v(frm, bx * 8 + 8, by * 8, 0, self.loop_str);
                }
                if (by < self.mb_h * 2 - 1) && !self.blocks[blk_idx + bx + blk_w].coded {
                    vp31_loop_filter_h(frm, bx * 8, by * 8 + 8, 0, self.loop_str);
                }
            }
            blk_idx += blk_w;
        }
/*        for plane in 1..3 {
            for by in 0..self.mb_h {
                for bx in 0..self.mb_w {
                }
            }
        }*/
    }
    fn generate_block_addr(&mut self) {
        let sb_w_y = (self.width         + 31) >> 5;
        let sb_h_y = (self.height        + 31) >> 5;
        let sb_w_c = ((self.width  >> 1) + 31) >> 5;
        let sb_h_c = ((self.height >> 1) + 31) >> 5;
        self.y_sbs = sb_w_y * sb_h_y;
        let tot_sb = sb_w_y * sb_h_y + 2 * sb_w_c * sb_h_c;
        let bw = self.width >> 3;
        let bh = self.height >> 3;
        let tot_blk = bw * bh * 3 / 2;
        self.sb_info.resize(tot_sb, SBState::Uncoded);
        self.sb_blocks = Vec::with_capacity(tot_sb);
        self.blk_addr = Vec::with_capacity(tot_blk);
        self.y_blocks = bw * bh;
        let mut base_idx = 0;
        for plane in 0..3 {
            let w = if plane > 0 { self.width  >> 1 } else { self.width };
            let h = if plane > 0 { self.height >> 1 } else { self.height };
            let sb_w = (w + 31) >> 5;
            let sb_h = (h + 31) >> 5;
            let blk_w = w >> 3;
            let blk_h = h >> 3;
            for y in 0..sb_h {
                for x in 0..sb_w {
                    let mut nblocks = 0;
                    for blk_no in 0..16 {
                        let bx = x * 4 + HILBERT_ORDER[blk_no][0];
                        let by = y * 4 + HILBERT_ORDER[blk_no][1];
                        if (bx >= blk_w) || (by >= blk_h) { continue; }
                        let idx = base_idx + bx + by * blk_w;
                        self.blk_addr.push(idx * 4 + plane);
                        nblocks += 1;
                    }
                    self.sb_blocks.push(nblocks);
                }
            }
            base_idx += blk_w * blk_h;
        }
        self.blocks.resize(tot_blk, Block::new());
    }
}

impl NADecoder for VP34Decoder {
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let fmt = YUV420_FORMAT;
            self.width  = vinfo.get_width();
            self.height = vinfo.get_height();
            validate!(((self.width | self.height) & 15) == 0);
            self.mb_w   = self.width  >> 4;
            self.mb_h   = self.height >> 4;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), true, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            supp.pool_u8.set_dec_bufs(3);
            supp.pool_u8.prealloc_video(NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), false, fmt), 4)?;

            if self.version == 3 {
                self.generate_block_addr();
            }
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        validate!(src.len() > 0);
        let mut br = BitReader::new(&src, src.len(), BitReaderMode::BE);

        self.parse_header(&mut br)?;
        if self.is_intra {
            self.shuf.clear();
        }

        let ret = supp.pool_u8.get_free();
        if ret.is_none() {
            return Err(DecoderError::AllocError);
        }
        let mut buf = ret.unwrap();
        let mut dframe = NASimpleVideoFrame::from_video_buf(&mut buf).unwrap();
        if self.version == 3 {
            self.decode_vp31(&mut br, &mut dframe)?;
        } else {
            self.decode_vp4()?;
        }

        if self.is_intra {
            self.shuf.add_golden_frame(buf.clone());
        }
        self.shuf.add_frame(buf.clone());

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), NABufferType::Video(buf));
        frm.set_keyframe(self.is_intra);
        frm.set_frame_type(if self.is_intra { FrameType::I } else { FrameType::P });
        Ok(frm.into_ref())
    }
}

pub fn get_decoder_vp3() -> Box<NADecoder> {
    Box::new(VP34Decoder::new(3))
}

/*pub fn get_decoder_vp4() -> Box<NADecoder> {
    Box::new(VP34Decoder::new(4))
}*/

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::duck_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;

    #[test]
    fn test_vp3() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

//        let file = "assets/Duck/vp30-logo.avi";
        let file = "assets/Duck/vp31.avi";
//        let file = "assets/Duck/vp31_crash.avi";
//        let file = "assets/Duck/01-vp31-0500.avi";
        test_file_decoding("avi", file, Some(3), true, false, Some("vp3"), &dmx_reg, &dec_reg);
//panic!("end");
    }

    #[test]
    fn test_vp4() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        let file = "assets/Duck/ot171_vp40.avi";
        test_file_decoding("avi", file, Some(16), true, false, Some("vp4"), &dmx_reg, &dec_reg);
    }
}

const HILBERT_ORDER: [[usize; 2]; 16] = [
    [ 0, 0 ], [ 1, 0 ], [ 1, 1 ], [ 0, 1 ],
    [ 0, 2 ], [ 0, 3 ], [ 1, 3 ], [ 1, 2 ],
    [ 2, 2 ], [ 2, 3 ], [ 3, 3 ], [ 3, 2 ],
    [ 3, 1 ], [ 2, 1 ], [ 2, 0 ], [ 3, 0 ]
];

const VP31_LOOP_STRENGTH: [i16; 64] = [
    30, 25, 20, 20, 15, 15, 14, 14,
    13, 13, 12, 12, 11, 11, 10, 10,
     9,  9,  8,  8,  7,  7,  7,  7,
     6,  6,  6,  6,  5,  5,  5,  5,
     4,  4,  4,  4,  3,  3,  3,  3,
     2,  2,  2,  2,  2,  2,  2,  2,
     0,  0,  0,  0,  0,  0,  0,  0,
     0,  0,  0,  0,  0,  0,  0,  0
];

const VP31_DEFAULT_MB_MODES: [VPMBType; 8] = [
    VPMBType::InterNoMV,    VPMBType::Intra,        VPMBType::InterMV,      VPMBType::InterNearest,
    VPMBType::InterNear,    VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
];

const VP31_MB_MODES: [[VPMBType; 8]; 6] = [
  [
    VPMBType::InterNearest, VPMBType::InterNear,    VPMBType::InterMV,      VPMBType::InterNoMV,
    VPMBType::Intra,        VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
  ], [
    VPMBType::InterNearest, VPMBType::InterNear,    VPMBType::InterNoMV,    VPMBType::InterMV,
    VPMBType::Intra,        VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
  ], [
    VPMBType::InterNearest, VPMBType::InterMV,      VPMBType::InterNear,    VPMBType::InterNoMV,
    VPMBType::Intra,        VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
  ], [
    VPMBType::InterNearest, VPMBType::InterMV,      VPMBType::InterNoMV,    VPMBType::InterNear,
    VPMBType::Intra,        VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
  ], [
    VPMBType::InterNoMV,    VPMBType::InterNearest, VPMBType::InterNear,    VPMBType::InterMV,
    VPMBType::Intra,        VPMBType::GoldenNoMV,   VPMBType::GoldenMV,     VPMBType::InterFourMV
  ], [
    VPMBType::InterNoMV,    VPMBType::GoldenNoMV,   VPMBType::InterNearest, VPMBType::InterNear,
    VPMBType::InterMV,      VPMBType::Intra,        VPMBType::GoldenMV,     VPMBType::InterFourMV
  ]
];

const VP3_LITERAL_BASE: [i16; 6] = [ 7, 9, 13, 21, 37, 69 ];

const VP31_AC_SCALES: [i16; 64] = [
    500, 450, 400, 370, 340, 310, 285, 265,
    245, 225, 210, 195, 185, 180, 170, 160,
    150, 145, 135, 130, 125, 115, 110, 107,
    100,  96,  93,  89,  85,  82,  75,  74,
     70,  68,  64,  60,  57,  56,  52,  50,
     49,  45,  44,  43,  40,  38,  37,  35,
     33,  32,  30,  29,  28,  25,  24,  22,
     21,  19,  18,  17,  15,  13,  12,  10
];

const VP31_DC_SCALES: [i16; 64] = [
    220, 200, 190, 180, 170, 170, 160, 160,
    150, 150, 140, 140, 130, 130, 120, 120,
    110, 110, 100, 100,  90,  90,  90,  80,
     80,  80,  70,  70,  70,  60,  60,  60,
     60,  50,  50,  50,  50,  40,  40,  40,
     40,  40,  30,  30,  30,  30,  30,  30,
     30,  20,  20,  20,  20,  20,  20,  20,
     20,  10,  10,  10,  10,  10,  10,  10
];

const VP3_QMAT_Y: &[i16; 64] = &[
    16,  11,  10,  16,  24,  40,  51,  61,
    12,  12,  14,  19,  26,  58,  60,  55,
    14,  13,  16,  24,  40,  57,  69,  56,
    14,  17,  22,  29,  51,  87,  80,  62,
    18,  22,  37,  58,  68, 109, 103,  77,
    24,  35,  55,  64,  81, 104, 113,  92,
    49,  64,  78,  87, 103, 121, 120, 101,
    72,  92,  95,  98, 112, 100, 103,  99
];

const VP3_QMAT_C: &[i16; 64] = &[
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99
];

const VP3_QMAT_INTER: &[i16; 64] = &[
    16,  16,  16,  20,  24,  28,  32,  40,
    16,  16,  20,  24,  28,  32,  40,  48,
    16,  20,  24,  28,  32,  40,  48,  64,
    20,  24,  28,  32,  40,  48,  64,  64,
    24,  28,  32,  40,  48,  64,  64,  64,
    28,  32,  40,  48,  64,  64,  64,  96,
    32,  40,  48,  64,  64,  64,  96, 128,
    40,  48,  64,  64,  64,  96, 128, 128
];

const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63
];

const VP31_DC_CODES: [[u16; 32]; 16] = [
  [
    0x002D, 0x0026, 0x0166, 0x004E, 0x02CE, 0x059E, 0x027D, 0x0008,
    0x04F9, 0x000F, 0x000E, 0x001B, 0x0006, 0x0008, 0x0005, 0x001A,
    0x0015, 0x0007, 0x000C, 0x0001, 0x0000, 0x0009, 0x0017, 0x0029,
    0x0028, 0x00B2, 0x04F8, 0x059F, 0x009E, 0x013F, 0x0012, 0x0058,
  ], [
    0x0010, 0x0047, 0x01FF, 0x008C, 0x03FC, 0x046A, 0x0469, 0x0022,
    0x11A1, 0x000E, 0x000D, 0x0004, 0x0005, 0x0009, 0x0006, 0x001E,
    0x0016, 0x0007, 0x000C, 0x0001, 0x0000, 0x000A, 0x0017, 0x007D,
    0x007E, 0x011B, 0x08D1, 0x03FD, 0x046B, 0x11A0, 0x007C, 0x00FE,
  ], [
    0x0016, 0x0020, 0x0086, 0x0087, 0x0367, 0x06CC, 0x06CB, 0x006E,
    0x366D, 0x000F, 0x000E, 0x0004, 0x0005, 0x000A, 0x0006, 0x001A,
    0x0011, 0x0007, 0x000C, 0x0001, 0x0000, 0x0009, 0x0017, 0x006F,
    0x006D, 0x0364, 0x0D9A, 0x06CA, 0x1B37, 0x366C, 0x0042, 0x00D8,
  ], [
    0x0000, 0x002D, 0x00F7, 0x0058, 0x0167, 0x02CB, 0x02CA, 0x000E,
    0x1661, 0x0003, 0x0002, 0x0008, 0x0009, 0x000D, 0x0002, 0x001F,
    0x0017, 0x0001, 0x000C, 0x000E, 0x000A, 0x0006, 0x0078, 0x000F,
    0x007A, 0x0164, 0x0599, 0x02CD, 0x0B31, 0x1660, 0x0079, 0x00F6,
  ], [
    0x0003, 0x003C, 0x000F, 0x007A, 0x001D, 0x0020, 0x0072, 0x0006,
    0x0399, 0x0004, 0x0005, 0x0005, 0x0006, 0x000E, 0x0004, 0x0000,
    0x0019, 0x0002, 0x000D, 0x0007, 0x001F, 0x0030, 0x0011, 0x0031,
    0x0005, 0x0021, 0x00E7, 0x0038, 0x01CD, 0x0398, 0x007B, 0x0009,
  ], [
    0x0009, 0x0002, 0x0074, 0x0007, 0x00EC, 0x00D1, 0x01A6, 0x0006,
    0x0D21, 0x0005, 0x0006, 0x0008, 0x0007, 0x000F, 0x0004, 0x0000,
    0x001C, 0x0002, 0x0005, 0x0003, 0x000C, 0x0035, 0x01A7, 0x001B,
    0x0077, 0x01A5, 0x0349, 0x00D0, 0x0691, 0x0D20, 0x0075, 0x00ED,
  ], [
    0x000A, 0x000C, 0x0012, 0x001B, 0x00B7, 0x016C, 0x0099, 0x005A,
    0x16D8, 0x0007, 0x0006, 0x0009, 0x0008, 0x0000, 0x0005, 0x0017,
    0x000E, 0x0002, 0x0003, 0x000F, 0x001A, 0x004D, 0x2DB3, 0x002C,
    0x0011, 0x02DA, 0x05B7, 0x0098, 0x0B6D, 0x2DB2, 0x0010, 0x0027,
  ], [
    0x000D, 0x000F, 0x001D, 0x0008, 0x0051, 0x0056, 0x00AF, 0x002A,
    0x148A, 0x0007, 0x0000, 0x0008, 0x0009, 0x000C, 0x0006, 0x0017,
    0x000B, 0x0016, 0x0015, 0x0009, 0x0050, 0x00AE, 0x2917, 0x001C,
    0x0014, 0x0290, 0x0523, 0x0149, 0x0A44, 0x2916, 0x0053, 0x00A5,
  ], [
    0x0001, 0x001D, 0x00F5, 0x00F4, 0x024D, 0x0499, 0x0498, 0x0001,
    0x0021, 0x0006, 0x0005, 0x0006, 0x0005, 0x0002, 0x0007, 0x0025,
    0x007B, 0x001C, 0x0020, 0x000D, 0x0048, 0x0092, 0x0127, 0x000E,
    0x0004, 0x0011, 0x000C, 0x003C, 0x000F, 0x0000, 0x001F, 0x0013,
  ], [
    0x0005, 0x003C, 0x0040, 0x000D, 0x0031, 0x0061, 0x0060, 0x0002,
    0x00F5, 0x0006, 0x0005, 0x0007, 0x0006, 0x0002, 0x0009, 0x0025,
    0x0007, 0x0021, 0x0024, 0x0010, 0x0041, 0x00F4, 0x0019, 0x000E,
    0x0003, 0x0011, 0x0011, 0x003F, 0x003E, 0x007B, 0x0000, 0x0013,
  ], [
    0x000A, 0x0007, 0x0001, 0x0009, 0x0131, 0x0261, 0x0260, 0x0015,
    0x0001, 0x0007, 0x0006, 0x0008, 0x0007, 0x0006, 0x0012, 0x002F,
    0x0014, 0x0027, 0x002D, 0x0016, 0x004D, 0x0099, 0x0000, 0x0004,
    0x0001, 0x0005, 0x0017, 0x002E, 0x002C, 0x0008, 0x0006, 0x0001,
  ], [
    0x0000, 0x000E, 0x0017, 0x002A, 0x0010, 0x00F9, 0x00F8, 0x001E,
    0x003F, 0x0007, 0x0006, 0x0009, 0x0008, 0x0006, 0x000F, 0x0005,
    0x0016, 0x0029, 0x002B, 0x0015, 0x0050, 0x0011, 0x007D, 0x0004,
    0x0017, 0x0006, 0x0014, 0x002C, 0x002D, 0x000E, 0x0009, 0x0051,
  ], [
    0x0002, 0x0018, 0x002F, 0x000D, 0x0053, 0x0295, 0x0294, 0x00A4,
    0x007C, 0x0000, 0x0007, 0x0009, 0x0008, 0x001B, 0x000C, 0x0028,
    0x006A, 0x001E, 0x001D, 0x0069, 0x00D7, 0x007D, 0x014B, 0x0019,
    0x0016, 0x002E, 0x001C, 0x002B, 0x002A, 0x0068, 0x003F, 0x00D6,
  ], [
    0x0002, 0x001B, 0x000C, 0x0018, 0x0029, 0x007F, 0x02F0, 0x0198,
    0x0179, 0x0000, 0x0007, 0x0009, 0x0008, 0x001A, 0x000D, 0x002A,
    0x0064, 0x001E, 0x0067, 0x005F, 0x00CD, 0x007E, 0x02F1, 0x0016,
    0x000E, 0x002E, 0x0065, 0x002B, 0x0028, 0x003E, 0x00BD, 0x0199,
  ], [
    0x0002, 0x0007, 0x0016, 0x0006, 0x0036, 0x005C, 0x015D, 0x015C,
    0x02BF, 0x0000, 0x0007, 0x0009, 0x0008, 0x0018, 0x0034, 0x002A,
    0x005E, 0x006A, 0x0064, 0x005D, 0x00CB, 0x00AD, 0x02BE, 0x0014,
    0x0033, 0x006E, 0x005F, 0x006F, 0x006B, 0x00CA, 0x00AC, 0x015E,
  ], [
    0x000F, 0x001D, 0x0018, 0x000B, 0x0019, 0x0029, 0x00D6, 0x0551,
    0x0AA1, 0x0001, 0x0000, 0x0009, 0x0008, 0x001B, 0x0038, 0x0028,
    0x0057, 0x006A, 0x0068, 0x0056, 0x00E5, 0x0155, 0x0AA0, 0x0073,
    0x0069, 0x00D7, 0x00AB, 0x00E4, 0x00A9, 0x0151, 0x0150, 0x02A9,
  ]
];

const VP31_DC_BITS: [[u8; 32]; 16] = [
  [
     6,  7,  9,  8, 10, 11, 11,  5, 12,  4,  4,  5,  4,  4,  4,  5,
     5,  4,  4,  3,  3,  4,  5,  6,  6,  8, 12, 11,  9, 10,  6,  7,
  ], [
     5,  7,  9,  8, 10, 11, 11,  6, 13,  4,  4,  4,  4,  4,  4,  5,
     5,  4,  4,  3,  3,  4,  5,  7,  7,  9, 12, 10, 11, 13,  7,  8,
  ], [
     5,  6,  8,  8, 10, 11, 11,  7, 14,  4,  4,  4,  4,  4,  4,  5,
     5,  4,  4,  3,  3,  4,  5,  7,  7, 10, 12, 11, 13, 14,  7,  8,
  ], [
     4,  6,  8,  7,  9, 10, 10,  6, 13,  3,  3,  4,  4,  4,  4,  5,
     5,  4,  4,  4,  4,  5,  7,  6,  7,  9, 11, 10, 12, 13,  7,  8,
  ], [
     4,  6,  7,  7,  8,  9, 10,  6, 13,  3,  3,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  5,  6,  8,  6,  6,  9, 11,  9, 12, 13,  7,  7,
  ], [
     4,  5,  7,  6,  8,  9, 10,  6, 13,  3,  3,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  5,  7, 10,  6,  7, 10, 11,  9, 12, 13,  7,  8,
  ], [
     4,  5,  6,  6,  8,  9,  9,  7, 13,  3,  3,  4,  4,  3,  4,  5,
     5,  4,  4,  5,  6,  8, 14,  6,  6, 10, 11,  9, 12, 14,  6,  7,
  ], [
     4,  5,  6,  5,  7,  8,  9,  7, 13,  3,  2,  4,  4,  4,  4,  5,
     5,  5,  5,  5,  7,  9, 14,  6,  6, 10, 11,  9, 12, 14,  7,  8,
  ], [
     4,  6,  8,  8, 10, 11, 11,  5,  6,  3,  3,  4,  4,  4,  5,  6,
     7,  6,  6,  6,  7,  8,  9,  4,  4,  5,  6,  6,  5,  5,  5,  5,
  ], [
     4,  6,  7,  7,  9, 10, 10,  5,  8,  3,  3,  4,  4,  4,  5,  6,
     6,  6,  6,  6,  7,  8,  8,  4,  4,  5,  6,  6,  6,  7,  4,  5,
  ], [
     4,  5,  6,  6,  9, 10, 10,  6,  7,  3,  3,  4,  4,  4,  5,  6,
     6,  6,  6,  6,  7,  8,  7,  4,  4,  5,  6,  6,  6,  6,  5,  5,
  ], [
     3,  5,  6,  6,  7, 10, 10,  7,  8,  3,  3,  4,  4,  4,  5,  5,
     6,  6,  6,  6,  7,  7,  9,  4,  5,  5,  6,  6,  6,  6,  6,  7,
  ], [
     3,  5,  6,  5,  7, 10, 10,  8,  8,  2,  3,  4,  4,  5,  5,  6,
     7,  6,  6,  7,  8,  8,  9,  5,  5,  6,  6,  6,  6,  7,  7,  8,
  ], [
     3,  5,  5,  5,  6,  8, 10,  9,  9,  2,  3,  4,  4,  5,  5,  6,
     7,  6,  7,  7,  8,  8, 10,  5,  5,  6,  7,  6,  6,  7,  8,  9,
  ], [
     3,  4,  5,  4,  6,  7,  9,  9, 10,  2,  3,  4,  4,  5,  6,  6,
     7,  7,  7,  7,  8,  8, 10,  5,  6,  7,  7,  7,  7,  8,  8,  9,
  ], [
     4,  5,  5,  4,  5,  6,  8, 11, 12,  2,  2,  4,  4,  5,  6,  6,
     7,  7,  7,  7,  8,  9, 12,  7,  7,  8,  8,  8,  8,  9,  9, 10,
  ]
];

const VP31_AC_CAT0_CODES: [[u16; 32]; 16] = [
  [
    0x0008, 0x0025, 0x017A, 0x02F7, 0x0BDB, 0x17B4, 0x2F6B, 0x001D,
    0x2F6A, 0x0008, 0x0007, 0x0001, 0x0002, 0x000A, 0x0006, 0x0000,
    0x001C, 0x0009, 0x000D, 0x000F, 0x000C, 0x0003, 0x000A, 0x0016,
    0x0013, 0x005D, 0x0024, 0x00BC, 0x005C, 0x05EC, 0x000B, 0x005F,
  ], [
    0x000F, 0x0010, 0x004B, 0x00C6, 0x031D, 0x0C71, 0x0C70, 0x0001,
    0x0C73, 0x0008, 0x0009, 0x0002, 0x0003, 0x000B, 0x0006, 0x0000,
    0x001C, 0x0005, 0x000D, 0x000F, 0x000A, 0x0019, 0x0013, 0x001D,
    0x0030, 0x0062, 0x0024, 0x004A, 0x018F, 0x0C72, 0x000E, 0x0011,
  ], [
    0x001B, 0x0003, 0x008D, 0x0040, 0x0239, 0x0471, 0x08E0, 0x0003,
    0x11C3, 0x000A, 0x0009, 0x0004, 0x0005, 0x000E, 0x0007, 0x0001,
    0x001E, 0x0006, 0x000C, 0x000B, 0x0002, 0x0000, 0x0041, 0x001F,
    0x0022, 0x0002, 0x008F, 0x008C, 0x011D, 0x11C2, 0x001A, 0x0021,
  ], [
    0x001F, 0x0003, 0x0003, 0x0043, 0x000B, 0x0015, 0x0051, 0x0003,
    0x0050, 0x000D, 0x000C, 0x0004, 0x0006, 0x000E, 0x000A, 0x0001,
    0x001E, 0x0005, 0x0009, 0x0007, 0x0011, 0x0002, 0x0004, 0x0002,
    0x002D, 0x0020, 0x0042, 0x0001, 0x0000, 0x0029, 0x0017, 0x002C,
  ], [
    0x0003, 0x001F, 0x003A, 0x005D, 0x0173, 0x02E4, 0x172D, 0x0004,
    0x172C, 0x000F, 0x000E, 0x0009, 0x0008, 0x000C, 0x000A, 0x0001,
    0x0016, 0x0002, 0x0005, 0x001A, 0x002F, 0x0038, 0x05CA, 0x0006,
    0x0037, 0x001E, 0x003B, 0x0039, 0x00B8, 0x0B97, 0x0000, 0x0036,
  ], [
    0x0006, 0x0037, 0x005D, 0x000C, 0x00B9, 0x02E3, 0x05C4, 0x0004,
    0x1715, 0x0000, 0x000F, 0x0008, 0x0007, 0x000C, 0x0009, 0x001D,
    0x0016, 0x001C, 0x001A, 0x000B, 0x005E, 0x0170, 0x1714, 0x000A,
    0x000A, 0x0036, 0x005F, 0x001B, 0x001A, 0x0B8B, 0x0002, 0x0007,
  ], [
    0x000C, 0x000B, 0x0079, 0x0022, 0x00F0, 0x0119, 0x0230, 0x001D,
    0x08C4, 0x0001, 0x0000, 0x000A, 0x0009, 0x000B, 0x0007, 0x001C,
    0x003D, 0x000D, 0x0008, 0x0015, 0x008D, 0x118B, 0x118A, 0x000D,
    0x0010, 0x0009, 0x0014, 0x0047, 0x00F1, 0x0463, 0x001F, 0x000C,
  ], [
    0x0000, 0x001A, 0x0033, 0x000C, 0x0046, 0x01E3, 0x03C5, 0x0017,
    0x1E21, 0x0002, 0x0001, 0x0009, 0x000A, 0x0007, 0x001B, 0x003D,
    0x001B, 0x0022, 0x0079, 0x00F0, 0x1E20, 0x1E23, 0x1E22, 0x000E,
    0x0016, 0x0018, 0x0032, 0x001A, 0x0047, 0x0789, 0x001F, 0x0010,
  ], [
    0x001D, 0x0061, 0x004E, 0x009E, 0x027C, 0x09F5, 0x09F4, 0x0003,
    0x0060, 0x0000, 0x000F, 0x000B, 0x000A, 0x0009, 0x0005, 0x000D,
    0x0031, 0x0008, 0x0038, 0x0012, 0x0026, 0x013F, 0x04FB, 0x000D,
    0x0002, 0x000C, 0x0039, 0x001C, 0x000F, 0x001D, 0x0008, 0x0019,
  ], [
    0x0007, 0x0019, 0x00AB, 0x00AA, 0x0119, 0x0461, 0x0460, 0x001B,
    0x0047, 0x0001, 0x0000, 0x000C, 0x000B, 0x0009, 0x0005, 0x000D,
    0x0035, 0x003D, 0x003C, 0x0018, 0x0022, 0x008D, 0x0231, 0x000E,
    0x001F, 0x0009, 0x002B, 0x0010, 0x0034, 0x0054, 0x0008, 0x0014,
  ], [
    0x000C, 0x0005, 0x0008, 0x005B, 0x004D, 0x0131, 0x0261, 0x001A,
    0x0012, 0x0000, 0x000F, 0x000A, 0x0009, 0x0006, 0x001B, 0x0006,
    0x001C, 0x002C, 0x0015, 0x005A, 0x0027, 0x0099, 0x0260, 0x000E,
    0x0004, 0x000F, 0x0007, 0x001D, 0x000B, 0x0014, 0x0008, 0x0017,
  ], [
    0x000F, 0x0013, 0x0075, 0x0024, 0x0095, 0x0251, 0x04A0, 0x0010,
    0x00C8, 0x0002, 0x0001, 0x0001, 0x0000, 0x001A, 0x0011, 0x002C,
    0x0065, 0x0074, 0x004B, 0x00C9, 0x0129, 0x0943, 0x0942, 0x0003,
    0x000A, 0x001C, 0x0018, 0x0033, 0x0017, 0x002D, 0x001B, 0x003B,
  ], [
    0x0003, 0x001A, 0x002D, 0x0038, 0x0028, 0x0395, 0x0E51, 0x0037,
    0x00E4, 0x0001, 0x0000, 0x001F, 0x001E, 0x0017, 0x003A, 0x0073,
    0x002A, 0x002B, 0x0029, 0x01CB, 0x0729, 0x1CA1, 0x1CA0, 0x0004,
    0x000A, 0x0004, 0x0018, 0x0036, 0x000B, 0x002C, 0x0019, 0x003B,
  ], [
    0x0004, 0x0004, 0x003F, 0x0017, 0x0075, 0x01F5, 0x07D1, 0x0017,
    0x01F6, 0x0001, 0x0000, 0x001B, 0x001A, 0x000A, 0x0032, 0x0074,
    0x00F8, 0x00F9, 0x01F7, 0x03E9, 0x0FA0, 0x1F43, 0x1F42, 0x0003,
    0x000A, 0x001E, 0x001C, 0x003B, 0x0018, 0x0016, 0x0016, 0x0033,
  ], [
    0x0004, 0x0007, 0x0018, 0x001E, 0x0036, 0x0031, 0x0177, 0x0077,
    0x0176, 0x0001, 0x0000, 0x001A, 0x0019, 0x003A, 0x0019, 0x005C,
    0x00BA, 0x0061, 0x00C1, 0x0180, 0x0302, 0x0607, 0x0606, 0x0002,
    0x000A, 0x001F, 0x001C, 0x0037, 0x0016, 0x0076, 0x000D, 0x002F,
  ], [
    0x0000, 0x000A, 0x001A, 0x000C, 0x001D, 0x0039, 0x0078, 0x005E,
    0x0393, 0x0002, 0x0001, 0x0016, 0x000F, 0x002E, 0x005F, 0x0073,
    0x00E5, 0x01C8, 0x0E4A, 0x1C97, 0x1C96, 0x0E49, 0x0E48, 0x0004,
    0x0006, 0x001F, 0x001B, 0x001D, 0x0038, 0x0038, 0x003D, 0x0079,
  ]
];

const VP31_AC_CAT0_BITS: [[u8; 32]; 16] = [
  [
     5,  7,  9, 10, 12, 13, 14,  5, 14,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  4,  4,  5,  5,  6,  7,  7,  8,  7, 11,  5,  7,
  ], [
     5,  6,  8,  8, 10, 12, 12,  4, 12,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  4,  5,  6,  5,  6,  7,  7,  8,  9, 12,  5,  6,
  ], [
     5,  6,  8,  7, 10, 11, 12,  4, 13,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  4,  5,  7,  5,  6,  6,  8,  8,  9, 13,  5,  6,
  ], [
     5,  6,  7,  7,  9, 10, 12,  4, 12,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  5,  6,  8,  4,  6,  6,  7,  7,  7, 11,  5,  6,
  ], [
     4,  6,  7,  7,  9, 10, 13,  4, 13,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  5,  6,  7, 11,  4,  6,  6,  7,  7,  8, 12,  4,  6,
  ], [
     4,  6,  7,  6,  8, 10, 11,  4, 13,  3,  4,  4,  4,  4,  4,  5,
     5,  5,  5,  5,  7,  9, 13,  4,  5,  6,  7,  7,  7, 12,  4,  5,
  ], [
     4,  5,  7,  6,  8,  9, 10,  5, 12,  3,  3,  4,  4,  4,  4,  5,
     6,  5,  5,  6,  8, 13, 13,  4,  5,  5,  6,  7,  8, 11,  5,  5,
  ], [
     3,  5,  6,  5,  7,  9, 10,  5, 13,  3,  3,  4,  4,  4,  5,  6,
     6,  6,  7,  8, 13, 13, 13,  4,  5,  5,  6,  6,  7, 11,  5,  5,
  ], [
     5,  7,  8,  9, 11, 13, 13,  4,  7,  3,  4,  4,  4,  4,  4,  5,
     6,  5,  6,  6,  7, 10, 12,  4,  4,  5,  6,  6,  5,  6,  4,  5,
  ], [
     4,  6,  8,  8, 10, 12, 12,  5,  8,  3,  3,  4,  4,  4,  4,  5,
     6,  6,  6,  6,  7,  9, 11,  4,  5,  5,  6,  6,  6,  7,  4,  5,
  ], [
     4,  5,  6,  7,  9, 11, 12,  5,  7,  3,  4,  4,  4,  4,  5,  5,
     6,  6,  6,  7,  8, 10, 12,  4,  4,  5,  5,  6,  5,  6,  4,  5,
  ], [
     4,  5,  7,  6,  8, 10, 11,  5,  8,  3,  3,  4,  4,  5,  5,  6,
     7,  7,  7,  8,  9, 12, 12,  3,  4,  5,  5,  6,  5,  6,  5,  6,
  ], [
     3,  5,  6,  6,  7, 10, 12,  6,  8,  3,  3,  5,  5,  5,  6,  7,
     7,  7,  7,  9, 11, 13, 13,  3,  4,  4,  5,  6,  5,  6,  5,  6,
  ], [
     3,  4,  6,  5,  7,  9, 11,  6,  9,  3,  3,  5,  5,  5,  6,  7,
     8,  8,  9, 10, 12, 13, 13,  3,  4,  5,  5,  6,  5,  6,  5,  6,
  ], [
     3,  4,  5,  5,  6,  7,  9,  7,  9,  3,  3,  5,  5,  6,  6,  7,
     8,  8,  9, 10, 11, 12, 12,  3,  4,  5,  5,  6,  5,  7,  5,  6,
  ], [
     3,  4,  5,  4,  5,  6,  7,  7, 11,  3,  3,  5,  5,  6,  7,  8,
     9, 10, 13, 14, 14, 13, 13,  3,  4,  5,  5,  6,  6,  7,  6,  7,
  ]
];

const VP31_AC_CAT1_CODES: [[u16; 32]; 16] = [
  [
    0x000B, 0x002B, 0x0054, 0x01B7, 0x06D9, 0x0DB1, 0x0DB0, 0x0002,
    0x00AB, 0x0009, 0x000A, 0x0007, 0x0008, 0x000F, 0x000C, 0x0003,
    0x001D, 0x0004, 0x000B, 0x0006, 0x001A, 0x0003, 0x00AA, 0x0001,
    0x0000, 0x0014, 0x006C, 0x00DA, 0x0002, 0x036D, 0x001C, 0x0037,
  ], [
    0x001D, 0x0004, 0x00B6, 0x006A, 0x05B9, 0x16E1, 0x16E0, 0x0007,
    0x016F, 0x000C, 0x000D, 0x0009, 0x0008, 0x000F, 0x000A, 0x0003,
    0x0017, 0x0002, 0x0004, 0x001C, 0x002C, 0x006B, 0x0B71, 0x0005,
    0x0003, 0x001B, 0x005A, 0x0034, 0x0005, 0x02DD, 0x0000, 0x000C,
  ], [
    0x0003, 0x007F, 0x00A1, 0x00A0, 0x020C, 0x0834, 0x106B, 0x0007,
    0x0082, 0x000E, 0x000D, 0x000B, 0x000C, 0x0000, 0x0009, 0x0002,
    0x0011, 0x001E, 0x0015, 0x003E, 0x0040, 0x041B, 0x106A, 0x0006,
    0x000A, 0x0029, 0x007E, 0x0051, 0x0021, 0x0107, 0x0004, 0x000B,
  ], [
    0x0007, 0x001B, 0x00F6, 0x00E9, 0x03A1, 0x0740, 0x0E82, 0x001F,
    0x01EF, 0x0001, 0x0002, 0x000B, 0x000C, 0x000D, 0x0008, 0x001C,
    0x0003, 0x0012, 0x0002, 0x0075, 0x01D1, 0x1D07, 0x1D06, 0x000A,
    0x0013, 0x003B, 0x001A, 0x007A, 0x003C, 0x01EE, 0x0000, 0x000C,
  ], [
    0x000D, 0x003D, 0x0042, 0x0037, 0x00D9, 0x0362, 0x06C6, 0x001F,
    0x0086, 0x0001, 0x0002, 0x000C, 0x000B, 0x000A, 0x0001, 0x000F,
    0x0025, 0x003C, 0x001A, 0x0087, 0x01B0, 0x0D8F, 0x0D8E, 0x000E,
    0x0013, 0x000C, 0x0024, 0x0020, 0x0011, 0x006D, 0x0000, 0x000E,
  ], [
    0x0000, 0x0012, 0x0076, 0x0077, 0x014D, 0x0533, 0x14C9, 0x0013,
    0x00A5, 0x0002, 0x0003, 0x000B, 0x000C, 0x0008, 0x001A, 0x002B,
    0x0075, 0x0074, 0x00A7, 0x0298, 0x14C8, 0x14CB, 0x14CA, 0x000F,
    0x001C, 0x0007, 0x002A, 0x0028, 0x001B, 0x00A4, 0x0002, 0x0006,
  ], [
    0x0002, 0x001A, 0x002B, 0x003A, 0x00ED, 0x0283, 0x0A0A, 0x0004,
    0x00A1, 0x0004, 0x0003, 0x000B, 0x000C, 0x001F, 0x0006, 0x0077,
    0x00A3, 0x00A2, 0x0140, 0x1417, 0x1416, 0x0A09, 0x0A08, 0x0000,
    0x001E, 0x0007, 0x002A, 0x0029, 0x001C, 0x00EC, 0x001B, 0x0005,
  ], [
    0x0002, 0x0002, 0x0018, 0x001D, 0x0035, 0x00E4, 0x01CF, 0x001D,
    0x0072, 0x0004, 0x0005, 0x0006, 0x0007, 0x0006, 0x0073, 0x0038,
    0x01CE, 0x039B, 0x0398, 0x0733, 0x0732, 0x0735, 0x0734, 0x0000,
    0x001F, 0x001B, 0x0034, 0x000F, 0x001E, 0x00E5, 0x0019, 0x0038,
  ], [
    0x0016, 0x0050, 0x0172, 0x02E7, 0x1732, 0x2E67, 0x2E66, 0x0006,
    0x0051, 0x0001, 0x0000, 0x000D, 0x000C, 0x0009, 0x001C, 0x0009,
    0x001C, 0x001D, 0x005D, 0x00B8, 0x05CD, 0x1731, 0x1730, 0x000F,
    0x0005, 0x000F, 0x0008, 0x0029, 0x001D, 0x002F, 0x0008, 0x0015,
  ], [
    0x0009, 0x0021, 0x0040, 0x00AD, 0x02B0, 0x1589, 0x1588, 0x001C,
    0x005F, 0x0000, 0x000F, 0x000D, 0x000C, 0x0006, 0x0011, 0x002A,
    0x0057, 0x005E, 0x0041, 0x0159, 0x0563, 0x158B, 0x158A, 0x0001,
    0x0005, 0x0014, 0x003B, 0x002E, 0x0004, 0x003A, 0x0007, 0x0016,
  ], [
    0x000E, 0x0007, 0x0046, 0x0045, 0x0064, 0x032A, 0x0657, 0x0018,
    0x000D, 0x0000, 0x000F, 0x000A, 0x000B, 0x001A, 0x0036, 0x0047,
    0x0044, 0x0018, 0x0033, 0x00CB, 0x0656, 0x0329, 0x0328, 0x0002,
    0x0006, 0x0019, 0x000E, 0x0037, 0x0009, 0x000F, 0x0002, 0x0010,
  ], [
    0x0003, 0x0018, 0x0023, 0x0077, 0x0194, 0x1956, 0x32AF, 0x003A,
    0x0076, 0x0002, 0x0001, 0x001F, 0x001E, 0x0014, 0x0022, 0x0064,
    0x0197, 0x0196, 0x032B, 0x0654, 0x32AE, 0x1955, 0x1954, 0x0000,
    0x0009, 0x001C, 0x0015, 0x0010, 0x000D, 0x0017, 0x0016, 0x0033,
  ], [
    0x0005, 0x0006, 0x003E, 0x0010, 0x0048, 0x093F, 0x24FA, 0x0032,
    0x0067, 0x0002, 0x0001, 0x001B, 0x001E, 0x0034, 0x0066, 0x0092,
    0x0126, 0x024E, 0x049E, 0x49F7, 0x49F6, 0x24F9, 0x24F8, 0x0000,
    0x0007, 0x0018, 0x0011, 0x003F, 0x000E, 0x0013, 0x0035, 0x0025,
  ], [
    0x0005, 0x0008, 0x0012, 0x001C, 0x001C, 0x00EA, 0x1D75, 0x001E,
    0x0066, 0x0001, 0x0002, 0x001B, 0x001A, 0x001F, 0x003B, 0x0074,
    0x01D6, 0x03AF, 0x1D74, 0x1D77, 0x1D76, 0x0EB9, 0x0EB8, 0x000F,
    0x0006, 0x0013, 0x003B, 0x003A, 0x0000, 0x0018, 0x0032, 0x0067,
  ], [
    0x0004, 0x000A, 0x001B, 0x000C, 0x000D, 0x00E6, 0x0684, 0x0072,
    0x00E7, 0x0002, 0x0001, 0x0017, 0x0016, 0x0018, 0x00D1, 0x01A0,
    0x0686, 0x0D0F, 0x0D0A, 0x1A17, 0x1A16, 0x1A1D, 0x1A1C, 0x000F,
    0x001D, 0x000E, 0x0035, 0x0038, 0x0000, 0x000F, 0x0019, 0x0069,
  ], [
    0x0003, 0x000C, 0x001B, 0x0000, 0x0003, 0x002E, 0x0051, 0x00BC,
    0x0053, 0x0004, 0x0002, 0x0016, 0x0015, 0x0015, 0x0050, 0x00A4,
    0x0294, 0x052B, 0x052A, 0x052D, 0x052C, 0x052F, 0x052E, 0x000E,
    0x001A, 0x0004, 0x0028, 0x0029, 0x000F, 0x000B, 0x005F, 0x00BD,
  ]
];

const VP31_AC_CAT1_BITS: [[u8; 32]; 16] = [
  [
     5,  7,  8,  9, 11, 12, 12,  4,  9,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  4,  5,  6,  9,  4,  5,  6,  7,  8,  6, 10,  5,  6,
  ], [
     5,  6,  8,  8, 11, 13, 13,  4,  9,  4,  4,  4,  4,  4,  4,  4,
     5,  4,  4,  5,  6,  8, 12,  4,  5,  6,  7,  7,  6, 10,  4,  5,
  ], [
     4,  7,  8,  8, 10, 12, 13,  4,  8,  4,  4,  4,  4,  3,  4,  4,
     5,  5,  5,  6,  7, 11, 13,  4,  5,  6,  7,  7,  6,  9,  4,  5,
  ], [
     4,  6,  8,  8, 10, 11, 12,  5,  9,  3,  3,  4,  4,  4,  4,  5,
     5,  5,  5,  7,  9, 13, 13,  4,  5,  6,  6,  7,  6,  9,  4,  5,
  ], [
     4,  6,  7,  7,  9, 11, 12,  5,  8,  3,  3,  4,  4,  4,  4,  5,
     6,  6,  6,  8, 10, 13, 13,  4,  5,  5,  6,  6,  5,  8,  4,  5,
  ], [
     3,  5,  7,  7,  9, 11, 13,  5,  8,  3,  3,  4,  4,  4,  5,  6,
     7,  7,  8, 10, 13, 13, 13,  4,  5,  5,  6,  6,  5,  8,  4,  5,
  ], [
     3,  5,  6,  6,  8, 10, 12,  5,  8,  3,  3,  4,  4,  5,  5,  7,
     8,  8,  9, 13, 13, 12, 12,  3,  5,  5,  6,  6,  5,  8,  5,  5,
  ], [
     3,  4,  5,  5,  6,  8, 11,  7,  9,  3,  3,  4,  4,  5,  7,  8,
    11, 12, 12, 13, 13, 13, 13,  3,  5,  5,  6,  6,  5,  8,  5,  6,
  ], [
     5,  7,  9, 10, 13, 14, 14,  4,  7,  3,  3,  4,  4,  4,  5,  5,
     6,  6,  7,  8, 11, 13, 13,  4,  4,  5,  5,  6,  5,  6,  4,  5,
  ], [
     4,  6,  7,  8, 10, 13, 13,  5,  7,  3,  4,  4,  4,  4,  5,  6,
     7,  7,  7,  9, 11, 13, 13,  3,  4,  5,  6,  6,  4,  6,  4,  5,
  ], [
     4,  5,  7,  7,  9, 12, 13,  5,  6,  3,  4,  4,  4,  5,  6,  7,
     7,  7,  8, 10, 13, 12, 12,  3,  4,  5,  5,  6,  4,  5,  4,  5,
  ], [
     3,  5,  6,  7,  9, 13, 14,  6,  7,  3,  3,  5,  5,  5,  6,  7,
     9,  9, 10, 11, 14, 13, 13,  3,  4,  5,  5,  5,  4,  5,  5,  6,
  ], [
     3,  4,  6,  5,  7, 12, 14,  6,  7,  3,  3,  5,  5,  6,  7,  8,
     9, 10, 11, 15, 15, 14, 14,  3,  4,  5,  5,  6,  4,  5,  6,  6,
  ], [
     3,  4,  5,  5,  6,  9, 14,  6,  7,  3,  3,  5,  5,  6,  7,  8,
    10, 11, 14, 14, 14, 13, 13,  4,  4,  5,  6,  6,  3,  5,  6,  7,
  ], [
     3,  4,  5,  4,  5,  8, 11,  7,  8,  3,  3,  5,  5,  6,  8,  9,
    11, 12, 12, 13, 13, 13, 13,  4,  5,  5,  6,  6,  3,  5,  6,  7,
  ], [
     3,  4,  5,  3,  4,  6,  9,  8,  9,  3,  3,  5,  5,  7,  9, 10,
    12, 13, 13, 13, 13, 13, 13,  4,  5,  5,  6,  6,  4,  6,  7,  8,
  ]
];

const VP31_AC_CAT2_CODES: [[u16; 32]; 16] = [
  [
    0x0003, 0x0009, 0x00D0, 0x01A3, 0x0344, 0x0D14, 0x1A2B, 0x0004,
    0x0015, 0x0000, 0x000F, 0x000B, 0x000C, 0x000E, 0x0009, 0x001B,
    0x000A, 0x0014, 0x000D, 0x002A, 0x0014, 0x068B, 0x1A2A, 0x0008,
    0x000B, 0x002B, 0x000B, 0x0069, 0x0035, 0x0008, 0x0007, 0x000C,
  ], [
    0x000A, 0x003C, 0x0032, 0x0030, 0x00C5, 0x0621, 0x0620, 0x001F,
    0x0033, 0x0001, 0x0000, 0x000E, 0x000D, 0x000C, 0x0004, 0x000D,
    0x0026, 0x0027, 0x0014, 0x0063, 0x0189, 0x0623, 0x0622, 0x000B,
    0x0012, 0x003D, 0x0022, 0x0015, 0x000B, 0x0023, 0x0007, 0x0010,
  ], [
    0x000F, 0x000C, 0x0043, 0x0010, 0x0044, 0x0114, 0x0455, 0x0018,
    0x0023, 0x0001, 0x0000, 0x000E, 0x000D, 0x0009, 0x0019, 0x0009,
    0x0017, 0x0016, 0x0042, 0x008B, 0x0454, 0x0457, 0x0456, 0x000B,
    0x0015, 0x000A, 0x0029, 0x0020, 0x000D, 0x0028, 0x0007, 0x0011,
  ], [
    0x0001, 0x001A, 0x0029, 0x002A, 0x00A0, 0x0285, 0x1425, 0x0002,
    0x0000, 0x0002, 0x0003, 0x000C, 0x000B, 0x0008, 0x0012, 0x0001,
    0x0051, 0x0001, 0x0143, 0x0508, 0x1424, 0x1427, 0x1426, 0x000F,
    0x001C, 0x0003, 0x0037, 0x002B, 0x0013, 0x0036, 0x001D, 0x0001,
  ], [
    0x0004, 0x001F, 0x003D, 0x0006, 0x0016, 0x0053, 0x014A, 0x0034,
    0x002A, 0x0002, 0x0003, 0x000B, 0x000C, 0x001C, 0x0037, 0x0017,
    0x002B, 0x0028, 0x00A4, 0x052D, 0x052C, 0x052F, 0x052E, 0x0000,
    0x001D, 0x0007, 0x0004, 0x0035, 0x0014, 0x0036, 0x0015, 0x003C,
  ], [
    0x0004, 0x000A, 0x0007, 0x001D, 0x0009, 0x01F3, 0x07C7, 0x0008,
    0x01F0, 0x0003, 0x0002, 0x000D, 0x000C, 0x0017, 0x007D, 0x01F2,
    0x07C6, 0x07C5, 0x1F12, 0x3E27, 0x3E26, 0x1F11, 0x1F10, 0x0000,
    0x001E, 0x0006, 0x0039, 0x0038, 0x003F, 0x002C, 0x0005, 0x002D,
  ], [
    0x0002, 0x0007, 0x0018, 0x0003, 0x0005, 0x0035, 0x004F, 0x0012,
    0x04E5, 0x0005, 0x0004, 0x000D, 0x000E, 0x0033, 0x0026, 0x009D,
    0x04E4, 0x04E7, 0x04E6, 0x04E1, 0x04E0, 0x04E3, 0x04E2, 0x0000,
    0x001F, 0x000C, 0x003D, 0x003C, 0x0032, 0x0034, 0x001B, 0x0008,
  ], [
    0x0000, 0x0004, 0x001C, 0x000F, 0x0002, 0x0007, 0x0075, 0x00E8,
    0x1D2A, 0x0005, 0x0004, 0x000D, 0x000C, 0x0077, 0x0E96, 0x3A57,
    0x3A56, 0x3A5D, 0x3A5C, 0x3A5F, 0x3A5E, 0x1D29, 0x1D28, 0x0003,
    0x0006, 0x000A, 0x002C, 0x0017, 0x0076, 0x01D3, 0x03A4, 0x002D,
  ], [
    0x000A, 0x0024, 0x00BF, 0x0085, 0x0211, 0x0842, 0x1087, 0x0018,
    0x0020, 0x0001, 0x0002, 0x000E, 0x000D, 0x0007, 0x0013, 0x0025,
    0x005E, 0x0043, 0x00BE, 0x0109, 0x1086, 0x0841, 0x0840, 0x000F,
    0x0001, 0x0011, 0x0000, 0x002E, 0x0019, 0x0001, 0x0006, 0x0016,
  ], [
    0x0002, 0x000F, 0x006F, 0x0061, 0x0374, 0x1BA8, 0x3753, 0x0012,
    0x0036, 0x0000, 0x0001, 0x000A, 0x000B, 0x001A, 0x0031, 0x0060,
    0x00DC, 0x01BB, 0x06EB, 0x1BAB, 0x3752, 0x3755, 0x3754, 0x000E,
    0x0006, 0x0013, 0x000E, 0x003E, 0x0008, 0x001E, 0x0019, 0x003F,
  ], [
    0x0003, 0x001C, 0x0025, 0x0024, 0x01DA, 0x1DBD, 0x3B7C, 0x003C,
    0x003D, 0x0000, 0x0001, 0x000B, 0x000A, 0x000B, 0x0077, 0x00EC,
    0x03B6, 0x076E, 0x1DBF, 0x76FB, 0x76FA, 0x3B79, 0x3B78, 0x000D,
    0x001F, 0x0013, 0x000A, 0x0008, 0x000C, 0x0008, 0x0009, 0x003A,
  ], [
    0x0005, 0x0003, 0x0004, 0x0010, 0x008F, 0x0475, 0x11D1, 0x0079,
    0x0027, 0x0002, 0x0003, 0x0001, 0x0000, 0x0026, 0x0046, 0x011C,
    0x0477, 0x08ED, 0x11D0, 0x11D3, 0x11D2, 0x11D9, 0x11D8, 0x000D,
    0x001F, 0x0012, 0x0005, 0x003D, 0x000C, 0x000E, 0x0022, 0x0078,
  ], [
    0x0005, 0x000C, 0x001B, 0x0000, 0x0006, 0x03E2, 0x3E3D, 0x000F,
    0x0034, 0x0003, 0x0002, 0x001E, 0x001D, 0x007D, 0x01F0, 0x07C6,
    0x3E3C, 0x3E3F, 0x3E3E, 0x3E39, 0x3E38, 0x3E3B, 0x3E3A, 0x0008,
    0x001C, 0x0002, 0x003F, 0x0035, 0x0009, 0x0001, 0x000E, 0x00F9,
  ], [
    0x0004, 0x000B, 0x0001, 0x000A, 0x001E, 0x00E0, 0x0E1E, 0x0071,
    0x0039, 0x0007, 0x0006, 0x000D, 0x000C, 0x0020, 0x01C2, 0x1C3F,
    0x1C3E, 0x0E19, 0x0E18, 0x0E1B, 0x0E1A, 0x0E1D, 0x0E1C, 0x0000,
    0x0009, 0x001D, 0x001F, 0x0011, 0x0005, 0x0001, 0x0043, 0x0042,
  ], [
    0x0004, 0x000D, 0x0007, 0x0002, 0x0014, 0x016C, 0x16D1, 0x02DF,
    0x016E, 0x0000, 0x0007, 0x002C, 0x002B, 0x02DE, 0x16D0, 0x16D3,
    0x16D2, 0x2DB5, 0x2DB4, 0x2DB7, 0x2DB6, 0x16D9, 0x16D8, 0x000C,
    0x002A, 0x005A, 0x001B, 0x001A, 0x0017, 0x000C, 0x05B7, 0x05B5,
  ], [
    0x0002, 0x000F, 0x001C, 0x000C, 0x003B, 0x01AC, 0x1AD8, 0x35B3,
    0x35B2, 0x0001, 0x0000, 0x0069, 0x0068, 0x35BD, 0x35BC, 0x35BF,
    0x35BE, 0x35B9, 0x35B8, 0x35BB, 0x35BA, 0x35B5, 0x35B4, 0x01A9,
    0x01A8, 0x035A, 0x00D7, 0x00D5, 0x003A, 0x001B, 0x35B7, 0x35B6,
  ]
];

const VP31_AC_CAT2_BITS: [[u8; 32]; 16] = [
  [
     4,  6,  8,  9, 10, 12, 13,  4,  7,  3,  4,  4,  4,  4,  4,  5,
     5,  5,  5,  6,  7, 11, 13,  4,  5,  6,  6,  7,  6,  6,  4,  5,
  ], [
     4,  6,  7,  7,  9, 12, 12,  5,  7,  3,  3,  4,  4,  4,  4,  5,
     6,  6,  6,  8, 10, 12, 12,  4,  5,  6,  6,  6,  5,  6,  4,  5,
  ], [
     4,  5,  7,  6,  8, 10, 12,  5,  7,  3,  3,  4,  4,  4,  5,  5,
     6,  6,  7,  9, 12, 12, 12,  4,  5,  5,  6,  6,  5,  6,  4,  5,
  ], [
     3,  5,  6,  6,  8, 10, 13,  5,  7,  3,  3,  4,  4,  4,  5,  6,
     7,  7,  9, 11, 13, 13, 13,  4,  5,  5,  6,  6,  5,  6,  5,  5,
  ], [
     3,  5,  6,  5,  7,  9, 11,  6,  8,  3,  3,  4,  4,  5,  6,  7,
     8,  8, 10, 13, 13, 13, 13,  3,  5,  5,  5,  6,  5,  6,  5,  6,
  ], [
     3,  4,  5,  5,  6,  9, 11,  6,  9,  3,  3,  4,  4,  5,  7,  9,
    11, 11, 13, 14, 14, 13, 13,  3,  5,  5,  6,  6,  6,  6,  5,  6,
  ], [
     3,  4,  5,  4,  5,  7,  9,  7, 13,  3,  3,  4,  4,  6,  8, 10,
    13, 13, 13, 13, 13, 13, 13,  3,  5,  5,  6,  6,  6,  7,  6,  6,
  ], [
     3,  4,  5,  4,  4,  5,  7,  8, 13,  3,  3,  4,  4,  7, 12, 14,
    14, 14, 14, 14, 14, 13, 13,  3,  5,  5,  7,  6,  7,  9, 10,  7,
  ], [
     4,  6,  8,  8, 10, 12, 13,  5,  6,  3,  3,  4,  4,  4,  5,  6,
     7,  7,  8,  9, 13, 12, 12,  4,  4,  5,  5,  6,  5,  5,  4,  5,
  ], [
     3,  5,  7,  7, 10, 13, 14,  5,  6,  3,  3,  4,  4,  5,  6,  7,
     8,  9, 11, 13, 14, 14, 14,  4,  4,  5,  5,  6,  4,  5,  5,  6,
  ], [
     3,  5,  6,  6,  9, 13, 14,  6,  6,  3,  3,  4,  4,  5,  7,  8,
    10, 11, 13, 15, 15, 14, 14,  4,  5,  5,  5,  5,  4,  4,  5,  6,
  ], [
     3,  4,  5,  5,  8, 11, 13,  7,  6,  3,  3,  4,  4,  6,  7,  9,
    11, 12, 13, 13, 13, 13, 13,  4,  5,  5,  5,  6,  4,  4,  6,  7,
  ], [
     3,  4,  5,  4,  6, 10, 14,  7,  6,  3,  3,  5,  5,  7,  9, 11,
    14, 14, 14, 14, 14, 14, 14,  4,  5,  5,  6,  6,  4,  3,  7,  8,
  ], [
     3,  4,  4,  4,  6,  9, 13,  8,  7,  3,  3,  5,  5,  7, 10, 14,
    14, 13, 13, 13, 13, 13, 13,  4,  5,  6,  6,  6,  4,  3,  8,  8,
  ], [
     3,  4,  4,  3,  5,  9, 13, 10,  9,  2,  3,  6,  6, 10, 13, 13,
    13, 14, 14, 14, 14, 13, 13,  5,  6,  7,  6,  6,  5,  4, 11, 11,
  ], [
     2,  4,  5,  4,  6,  9, 13, 14, 14,  2,  2,  7,  7, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14,  9,  9, 10,  8,  8,  6,  5, 14, 14,
  ]
];

const VP31_AC_CAT3_CODES: [[u16; 32]; 16] = [
  [
    0x0000, 0x0010, 0x0072, 0x0071, 0x0154, 0x0AAB, 0x0AA8, 0x0014,
    0x0070, 0x0002, 0x0003, 0x000C, 0x000B, 0x0003, 0x0011, 0x0073,
    0x0054, 0x00AB, 0x02AB, 0x1553, 0x1552, 0x1555, 0x1554, 0x000D,
    0x001E, 0x0012, 0x003E, 0x002B, 0x0002, 0x003F, 0x001D, 0x0013,
  ], [
    0x0003, 0x001F, 0x0029, 0x003D, 0x000C, 0x0069, 0x0345, 0x0002,
    0x0028, 0x0002, 0x0001, 0x000E, 0x000C, 0x0015, 0x0007, 0x001B,
    0x006B, 0x006A, 0x0344, 0x0347, 0x0346, 0x01A1, 0x01A0, 0x000B,
    0x001A, 0x0012, 0x0000, 0x003C, 0x0008, 0x001B, 0x0013, 0x0001,
  ], [
    0x0004, 0x0004, 0x003F, 0x0014, 0x0056, 0x015C, 0x15D5, 0x003C,
    0x002A, 0x0000, 0x0001, 0x000E, 0x000D, 0x000C, 0x00AF, 0x02BB,
    0x15D4, 0x15D7, 0x15D6, 0x15D1, 0x15D0, 0x15D3, 0x15D2, 0x000B,
    0x0019, 0x000D, 0x003E, 0x0031, 0x0007, 0x0005, 0x003D, 0x0030,
  ], [
    0x0005, 0x0008, 0x001A, 0x0000, 0x0036, 0x0011, 0x0106, 0x000A,
    0x006E, 0x0002, 0x0003, 0x0003, 0x0002, 0x006F, 0x0021, 0x020F,
    0x020E, 0x0101, 0x0100, 0x0103, 0x0102, 0x0105, 0x0104, 0x000C,
    0x001E, 0x0003, 0x003E, 0x003F, 0x0009, 0x000E, 0x000B, 0x0009,
  ], [
    0x0002, 0x000E, 0x001E, 0x000C, 0x001F, 0x006E, 0x00AD, 0x00AF,
    0x0014, 0x0004, 0x0003, 0x001A, 0x0017, 0x002A, 0x0576, 0x0AEF,
    0x0AEE, 0x0571, 0x0570, 0x0573, 0x0572, 0x0575, 0x0574, 0x0003,
    0x0016, 0x0004, 0x0036, 0x000B, 0x000A, 0x0000, 0x006F, 0x00AC,
  ], [
    0x0004, 0x0005, 0x0003, 0x0001, 0x0004, 0x002F, 0x0526, 0x1495,
    0x00A6, 0x0007, 0x0006, 0x002D, 0x002C, 0x1494, 0x1497, 0x1496,
    0x1491, 0x1490, 0x1493, 0x1492, 0x293D, 0x293C, 0x293F, 0x0000,
    0x0028, 0x00A5, 0x0148, 0x00A7, 0x002E, 0x0015, 0x0A4E, 0x293E,
  ], [
    0x0004, 0x0005, 0x0003, 0x0001, 0x0004, 0x002F, 0x0526, 0x1495,
    0x00A6, 0x0007, 0x0006, 0x002D, 0x002C, 0x1494, 0x1497, 0x1496,
    0x1491, 0x1490, 0x1493, 0x1492, 0x293D, 0x293C, 0x293F, 0x0000,
    0x0028, 0x00A5, 0x0148, 0x00A7, 0x002E, 0x0015, 0x0A4E, 0x293E,
  ], [
    0x0004, 0x0005, 0x0003, 0x0001, 0x0004, 0x002F, 0x0526, 0x1495,
    0x00A6, 0x0007, 0x0006, 0x002D, 0x002C, 0x1494, 0x1497, 0x1496,
    0x1491, 0x1490, 0x1493, 0x1492, 0x293D, 0x293C, 0x293F, 0x0000,
    0x0028, 0x00A5, 0x0148, 0x00A7, 0x002E, 0x0015, 0x0A4E, 0x293E,
  ], [
    0x0003, 0x0011, 0x0020, 0x0074, 0x010D, 0x0863, 0x0860, 0x000A,
    0x0075, 0x0001, 0x0000, 0x000B, 0x000A, 0x0018, 0x0038, 0x0042,
    0x010F, 0x010E, 0x0219, 0x10C3, 0x10C2, 0x10C5, 0x10C4, 0x000F,
    0x0004, 0x0019, 0x000B, 0x0039, 0x0009, 0x001B, 0x001A, 0x003B,
  ], [
    0x0005, 0x0001, 0x003E, 0x0001, 0x00E2, 0x1C6F, 0x38D9, 0x0039,
    0x001F, 0x0002, 0x0001, 0x0009, 0x0008, 0x0000, 0x0070, 0x01C7,
    0x038C, 0x071A, 0x38D8, 0x38DB, 0x38DA, 0x38DD, 0x38DC, 0x000D,
    0x001D, 0x000E, 0x003F, 0x003C, 0x000C, 0x0006, 0x003D, 0x001E,
  ], [
    0x0006, 0x000B, 0x0011, 0x001E, 0x0074, 0x03AA, 0x1D5C, 0x0001,
    0x0021, 0x0001, 0x0002, 0x0007, 0x0006, 0x003E, 0x00EB, 0x01D4,
    0x0EAF, 0x3ABB, 0x3ABA, 0x1D59, 0x1D58, 0x1D5B, 0x1D5A, 0x000A,
    0x001C, 0x0001, 0x003F, 0x003B, 0x0001, 0x0009, 0x0020, 0x0000,
  ], [
    0x0004, 0x000A, 0x0017, 0x0004, 0x0016, 0x016A, 0x16B1, 0x0017,
    0x005B, 0x0006, 0x0007, 0x0001, 0x0000, 0x000A, 0x02D7, 0x0B5A,
    0x16B0, 0x16B3, 0x16B2, 0x2D6D, 0x2D6C, 0x2D6F, 0x2D6E, 0x0006,
    0x000A, 0x0004, 0x002C, 0x0017, 0x0003, 0x0007, 0x0016, 0x00B4,
  ], [
    0x0005, 0x000D, 0x0005, 0x0009, 0x0033, 0x0193, 0x192C, 0x0061,
    0x0031, 0x0000, 0x0007, 0x0010, 0x0011, 0x00C8, 0x192F, 0x325B,
    0x325A, 0x1929, 0x1928, 0x192B, 0x192A, 0x325D, 0x325C, 0x0018,
    0x001A, 0x001B, 0x0065, 0x0019, 0x0004, 0x0007, 0x0060, 0x0324,
  ], [
    0x0006, 0x0000, 0x0002, 0x000F, 0x0039, 0x01D9, 0x1D82, 0x0761,
    0x03BE, 0x0001, 0x0002, 0x000F, 0x000E, 0x0762, 0x3B07, 0x3B06,
    0x3B1D, 0x3B1C, 0x3B1F, 0x3B1E, 0x3B19, 0x3B18, 0x3B1B, 0x0038,
    0x01DE, 0x00ED, 0x03BF, 0x00EE, 0x003A, 0x0006, 0x0EC0, 0x3B1A,
  ], [
    0x0000, 0x0002, 0x000F, 0x0006, 0x001C, 0x01D0, 0x0E8C, 0x1D1B,
    0x1D1A, 0x0003, 0x0002, 0x00EA, 0x00E9, 0x0E89, 0x0E88, 0x0E8B,
    0x0E8A, 0x1D65, 0x1D64, 0x1D67, 0x1D66, 0x1D61, 0x1D60, 0x03AD,
    0x1D63, 0x1D62, 0x1D1D, 0x1D1C, 0x003B, 0x01D7, 0x1D1F, 0x1D1E,
  ], [
    0x0002, 0x000F, 0x001C, 0x000C, 0x003B, 0x01AC, 0x1AD8, 0x35B3,
    0x35B2, 0x0001, 0x0000, 0x0069, 0x0068, 0x35BD, 0x35BC, 0x35BF,
    0x35BE, 0x35B9, 0x35B8, 0x35BB, 0x35BA, 0x35B5, 0x35B4, 0x01A9,
    0x01A8, 0x035A, 0x00D7, 0x00D5, 0x003A, 0x001B, 0x35B7, 0x35B6,
  ]
];

const VP31_AC_CAT3_BITS: [[u8; 32]; 16] = [
  [
     3,  5,  7,  7,  9, 12, 12,  5,  7,  3,  3,  4,  4,  4,  5,  7,
     7,  8, 10, 13, 13, 13, 13,  4,  5,  5,  6,  6,  4,  6,  5,  5,
  ], [
     3,  5,  6,  6,  7, 10, 13,  5,  6,  3,  3,  4,  4,  5,  6,  8,
    10, 10, 13, 13, 13, 12, 12,  4,  5,  5,  5,  6,  4,  5,  5,  5,
  ], [
     3,  4,  6,  5,  7,  9, 13,  6,  6,  3,  3,  4,  4,  5,  8, 10,
    13, 13, 13, 13, 13, 13, 13,  4,  5,  5,  6,  6,  4,  4,  6,  6,
  ], [
     3,  4,  5,  4,  6,  8, 12,  7,  7,  3,  3,  4,  4,  7,  9, 13,
    13, 12, 12, 12, 12, 12, 12,  4,  5,  5,  6,  6,  4,  4,  7,  7,
  ], [
     3,  4,  5,  4,  5,  7, 10, 10,  7,  3,  3,  5,  5,  8, 13, 14,
    14, 13, 13, 13, 13, 13, 13,  4,  5,  5,  6,  6,  4,  3,  7, 10,
  ], [
     3,  4,  3,  3,  4,  6, 11, 13,  8,  3,  3,  6,  6, 13, 13, 13,
    13, 13, 13, 13, 14, 14, 14,  3,  6,  8,  9,  8,  6,  5, 12, 14,
  ], [
     3,  4,  3,  3,  4,  6, 11, 13,  8,  3,  3,  6,  6, 13, 13, 13,
    13, 13, 13, 13, 14, 14, 14,  3,  6,  8,  9,  8,  6,  5, 12, 14,
  ], [
     3,  4,  3,  3,  4,  6, 11, 13,  8,  3,  3,  6,  6, 13, 13, 13,
    13, 13, 13, 13, 14, 14, 14,  3,  6,  8,  9,  8,  6,  5, 12, 14,
  ], [
     3,  5,  6,  7,  9, 12, 12,  5,  7,  3,  3,  4,  4,  5,  6,  7,
     9,  9, 10, 13, 13, 13, 13,  4,  4,  5,  5,  6,  4,  5,  5,  6,
  ], [
     3,  4,  6,  5,  8, 13, 14,  6,  6,  3,  3,  4,  4,  5,  7,  9,
    10, 11, 14, 14, 14, 14, 14,  4,  5,  5,  6,  6,  4,  4,  6,  6,
  ], [
     3,  4,  5,  5,  7, 10, 13,  6,  6,  3,  3,  4,  4,  6,  8,  9,
    12, 14, 14, 13, 13, 13, 13,  4,  5,  5,  6,  6,  4,  4,  6,  6,
  ], [
     3,  4,  5,  4,  6,  9, 13,  7,  7,  3,  3,  4,  4,  6, 10, 12,
    13, 13, 13, 14, 14, 14, 14,  4,  5,  5,  6,  6,  4,  4,  7,  8,
  ], [
     3,  4,  4,  4,  6,  9, 13,  8,  7,  2,  3,  5,  5,  8, 13, 14,
    14, 13, 13, 13, 13, 14, 14,  5,  6,  6,  7,  6,  4,  4,  8, 10,
  ], [
     3,  3,  4,  4,  6,  9, 13, 11, 10,  2,  2,  6,  6, 11, 14, 14,
    14, 14, 14, 14, 14, 14, 14,  6,  9,  8, 10,  8,  6,  5, 12, 14,
  ], [
     2,  3,  5,  4,  6, 10, 13, 14, 14,  2,  2,  9,  9, 13, 13, 13,
    13, 14, 14, 14, 14, 14, 14, 11, 14, 14, 14, 14,  7, 10, 14, 14,
  ], [
     2,  4,  5,  4,  6,  9, 13, 14, 14,  2,  2,  7,  7, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14,  9,  9, 10,  8,  8,  6,  5, 14, 14,
  ]
];

const VP31_DC_WEIGHTS: [[i16; 5]; 16] = [
    [  0,   0,  0,  0,   0 ],
    [  1,   0,  0,  0,   1 ],
    [  0,   1,  0,  0,   1 ],
    [  1,   0,  0,  0,   1 ],

    [  0,   0,  1,  0,   1 ],
    [  1,   0,  1,  0,   2 ],
    [  0,   0,  1,  0,   1 ],
    [ 29, -26, 29,  0,  32 ],

    [  0,   0,  0,  1,   1 ],
    [ 75,   0,  0, 53, 128 ],
    [  0,   1,  0,  1,   2 ],
    [ 75,   0,  0, 53, 128 ],

    [  0,   0,  1,  0,   1 ],
    [ 75,   0,  0, 53, 128 ],
    [  0,   3, 10,  3,  16 ],
    [ 29, -26, 29,  0,  32 ],
];
