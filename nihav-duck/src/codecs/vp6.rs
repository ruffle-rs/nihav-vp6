use nihav_core::codecs::*;
use nihav_core::io::bitreader::*;
use nihav_codec_support::codecs::{MV, ZIGZAG};
use nihav_codec_support::codecs::blockdsp::edge_emu;
use super::vpcommon::*;
use super::vp56::*;

const VERSION_VP60: u8 = 6;
//const VERSION_VP61: u8 = 7;
const VERSION_VP62: u8 = 8;

const VP6_SIMPLE_PROFILE: u8    = 0;
const VP6_ADVANCED_PROFILE: u8  = 3;

#[derive(Default)]
struct VP6BR {
    vpversion:      u8,
    profile:        u8,
    interlaced:     bool,
    do_pm:          bool,
    loop_mode:      u8,
    autosel_pm:     bool,
    var_thresh:     u16,
    mv_thresh:      u8,
    bicubic:        bool,
    filter_alpha:   usize,
}

impl VP6BR {
    fn new() -> Self {
        Self::default()
    }
}

impl VP56Parser for VP6BR {
    fn parse_header(&mut self, bc: &mut BoolCoder) -> DecoderResult<VP56Header> {
        let mut hdr = VP56Header::default();
// horrible hack to match VP6 header parsing
        let src = bc.src;
        let mut br = BitReader::new(src, BitReaderMode::BE);

        hdr.is_intra                            = !br.read_bool()?;
        hdr.is_golden = hdr.is_intra;
        hdr.quant                               = br.read(6)? as u8;
        hdr.multistream                         = br.read_bool()?;
        if hdr.is_intra {
            hdr.version                         = br.read(5)? as u8;
            validate!((hdr.version >= VERSION_VP60) && (hdr.version <= VERSION_VP62));
            hdr.profile                         = br.read(2)? as u8;
            validate!((hdr.profile == VP6_SIMPLE_PROFILE) || (hdr.profile == VP6_ADVANCED_PROFILE));
            hdr.interlaced                      = br.read_bool()?;
        } else {
            hdr.version = self.vpversion;
            hdr.profile = self.profile;
            hdr.interlaced = self.interlaced;
        }
        if hdr.multistream || (hdr.profile == VP6_SIMPLE_PROFILE) {
            hdr.offset                          = br.read(16)? as u16;
            validate!(hdr.offset > if hdr.is_intra { 6 } else { 2 });
        }
        let bytes = br.tell() >> 3;
        std::mem::drop(br);
        bc.skip_bytes(bytes);
        self.loop_mode = 0;
        if hdr.is_intra {
            hdr.mb_h                            = bc.read_bits(8) as u8;
            hdr.mb_w                            = bc.read_bits(8) as u8;
            hdr.disp_h                          = bc.read_bits(8) as u8;
            hdr.disp_w                          = bc.read_bits(8) as u8;
            validate!((hdr.mb_h > 0) && (hdr.mb_w > 0) && (hdr.disp_w > 0) && (hdr.disp_h > 0));
            validate!((hdr.disp_w <= hdr.mb_w) && (hdr.disp_h <= hdr.mb_h));
            hdr.scale                           = bc.read_bits(2) as u8;
        } else {
            hdr.is_golden                       = bc.read_bool();
            if hdr.profile == VP6_ADVANCED_PROFILE {
                self.loop_mode                  = bc.read_bool() as u8;
                if self.loop_mode != 0 {
                    self.loop_mode             += bc.read_bool() as u8;
                    validate!(self.loop_mode <= 1);
                }
                if hdr.version == VERSION_VP62 {
                    self.do_pm                  = bc.read_bool();
                }
            }
        }

        if (hdr.profile == VP6_ADVANCED_PROFILE) && (hdr.is_intra || self.do_pm) {
            self.autosel_pm                     = bc.read_bool();
            if self.autosel_pm {
                self.var_thresh                 = bc.read_bits(5) as u16;
                if hdr.version != VERSION_VP62 {
                    self.var_thresh <<= 5;
                }
                self.mv_thresh                  = bc.read_bits(3) as u8;
            } else {
                self.bicubic                    = bc.read_bool();
            }
            if hdr.version == VERSION_VP62 {
                self.filter_alpha               = bc.read_bits(4) as usize;
            } else {
                self.filter_alpha = 16;
            }
        }

        hdr.use_huffman                         = bc.read_bool();

        self.vpversion  = hdr.version;
        self.profile    = hdr.profile;
        self.interlaced = hdr.interlaced;
        Ok(hdr)
    }
    fn decode_mv(&self, bc: &mut BoolCoder, model: &VP56MVModel) -> i16 {
        const LONG_VECTOR_ORDER: [usize; 7] = [ 0, 1, 2, 7, 6, 5, 4 ];

        let val = if !bc.read_prob(model.nz_prob) { // short vector
                vp_tree!(bc, model.tree_probs[0],
                         vp_tree!(bc, model.tree_probs[1],
                                  vp_tree!(bc, model.tree_probs[2], 0, 1),
                                  vp_tree!(bc, model.tree_probs[3], 2, 3)),
                         vp_tree!(bc, model.tree_probs[4],
                                  vp_tree!(bc, model.tree_probs[5], 4, 5),
                                  vp_tree!(bc, model.tree_probs[6], 6, 7)))
            } else {
                let mut raw = 0;
                for ord in LONG_VECTOR_ORDER.iter() {
                    raw                         |= (bc.read_prob(model.raw_probs[*ord]) as i16) << *ord;
                }
                if (raw & 0xF0) != 0 {
                    raw                         |= (bc.read_prob(model.raw_probs[3]) as i16) << 3;
                } else {
                    raw |= 1 << 3;
                }
                raw
            };
        if (val != 0) && bc.read_prob(model.sign_prob) {
            -val
        } else {
            val
        }
    }
    fn reset_models(&self, models: &mut VP56Models) {
        const NZ_PROBS: [u8; 2] = [ 162, 164 ];
        const RAW_PROBS: [[u8; 8]; 2] = [
            [ 247, 210, 135, 68, 138, 220, 239, 246 ],
            [ 244, 184, 201, 44, 173, 221, 239, 253 ]
        ];
        const TREE_PROBS: [[u8; 7]; 2] = [
            [ 225, 146, 172, 147, 214,  39, 156 ],
            [ 204, 170, 119, 235, 140, 230, 228 ]
        ];
        const ZERO_RUN_PROBS: [[u8; 14]; 2] = [
            [ 198, 197, 196, 146, 198, 204, 169, 142, 130, 136, 149, 149, 191, 249 ],
            [ 135, 201, 181, 154,  98, 117, 132, 126, 146, 169, 184, 240, 246, 254 ]
        ];

        for (i, mdl) in models.mv_models.iter_mut().enumerate() {
            mdl.nz_prob         = NZ_PROBS[i];
            mdl.sign_prob       = 128;
            mdl.raw_probs.copy_from_slice(&RAW_PROBS[i]);
            mdl.tree_probs.copy_from_slice(&TREE_PROBS[i]);
        }
        models.vp6models.zero_run_probs.copy_from_slice(&ZERO_RUN_PROBS);
        reset_scan(&mut models.vp6models, self.interlaced);
    }
    fn decode_mv_models(&self, bc: &mut BoolCoder, models: &mut [VP56MVModel; 2]) -> DecoderResult<()> {
        const HAS_NZ_PROB: [u8; 2] = [ 237, 231 ];
        const HAS_SIGN_PROB: [u8; 2] = [ 246, 243 ];
        const HAS_TREE_PROB: [[u8; 7]; 2] = [
            [ 253, 253, 254, 254, 254, 254, 254 ],
            [ 245, 253, 254, 254, 254, 254, 254 ]
        ];
        const HAS_RAW_PROB: [[u8; 8]; 2] = [
            [ 254, 254, 254, 254, 254, 250, 250, 252 ],
            [ 254, 254, 254, 254, 254, 251, 251, 254 ]
        ];

        for comp in 0..2 {
            if bc.read_prob(HAS_NZ_PROB[comp]) {
                models[comp].nz_prob            = bc.read_probability();
            }
            if bc.read_prob(HAS_SIGN_PROB[comp]) {
                models[comp].sign_prob          = bc.read_probability();
            }
        }
        for comp in 0..2 {
            for (i, prob) in HAS_TREE_PROB[comp].iter().enumerate() {
                if bc.read_prob(*prob) {
                    models[comp].tree_probs[i]  = bc.read_probability();
                }
            }
        }
        for comp in 0..2 {
            for (i, prob) in HAS_RAW_PROB[comp].iter().enumerate() {
                if bc.read_prob(*prob) {
                    models[comp].raw_probs[i]   = bc.read_probability();
                }
            }
        }
        Ok(())
    }
    fn decode_coeff_models(&self, bc: &mut BoolCoder, models: &mut VP56Models, is_intra: bool) -> DecoderResult<()> {
        const COEF_PROBS: [[u8; 11]; 2] = [
            [ 146, 255, 181, 207, 232, 243, 238, 251, 244, 250, 249 ],
            [ 179, 255, 214, 240, 250, 255, 244, 255, 255, 255, 255 ]
        ];
        const SCAN_UPD_PROBS: [u8; 64] = [
              0, 132, 132, 159, 153, 151, 161, 170,
            164, 162, 136, 110, 103, 114, 129, 118,
            124, 125, 132, 136, 114, 110, 142, 135,
            134, 123, 143, 126, 153, 183, 166, 161,
            171, 180, 179, 164, 203, 218, 225, 217,
            215, 206, 203, 217, 229, 241, 248, 243,
            253, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255
        ];
        const ZERO_RUN_PROBS: [[u8; 14]; 2] = [
            [ 219, 246, 238, 249, 232, 239, 249, 255, 248, 253, 239, 244, 241, 248 ],
            [ 198, 232, 251, 253, 219, 241, 253, 255, 248, 249, 244, 238, 251, 255 ]
        ];

        let mut def_prob = [128u8; 11];
        for plane in 0..2 {
            for i in 0..11 {
                if bc.read_prob(COEF_PROBS[plane][i]) {
                    def_prob[i]                 = bc.read_probability();
                    models.coeff_models[plane].dc_value_probs[i] = def_prob[i];
                } else if is_intra {
                    models.coeff_models[plane].dc_value_probs[i] = def_prob[i];
                }
            }
        }

        if bc.read_bool() {
            for i in 1..64 {
                if bc.read_prob(SCAN_UPD_PROBS[i]) {
                    models.vp6models.scan_order[i]  = bc.read_bits(4) as usize;
                }
            }
            update_scan(&mut models.vp6models);
        } else {
            reset_scan(&mut models.vp6models, self.interlaced);
        }

        for comp in 0..2 {
            for i in 0..14 {
                if bc.read_prob(ZERO_RUN_PROBS[comp][i]) {
                    models.vp6models.zero_run_probs[comp][i] = bc.read_probability();
                }
            }
        }

        for ctype in 0..3 {
            for plane in 0..2 {
                for group in 0..6 {
                    for i in 0..11 {
                        if bc.read_prob(VP6_AC_PROBS[ctype][plane][group][i]) {
                            def_prob[i]         = bc.read_probability();
                            models.coeff_models[plane].ac_val_probs[ctype][group][i] = def_prob[i];
                        } else if is_intra {
                            models.coeff_models[plane].ac_val_probs[ctype][group][i] = def_prob[i];
                        }
                    }
                }
            }
        }
        for plane in 0..2 {
            let mdl = &mut models.coeff_models[plane];
            for i in 0..3 {
                for k in 0..5 {
                    mdl.dc_token_probs[0][i][k] = rescale_prob(mdl.dc_value_probs[k], &VP6_DC_WEIGHTS[k][i], 255);
                }
            }
        }
        Ok(())
    }
    fn decode_block(&self, bc: &mut BoolCoder, coeffs: &mut [i16; 64], model: &VP56CoeffModel, vp6model: &VP6Models, fstate: &mut FrameState) -> DecoderResult<()> {
        let left_ctx = fstate.coeff_cat[fstate.ctx_idx][0] as usize;
        let top_ctx = fstate.top_ctx as usize;
        let dc_mode = top_ctx + left_ctx;
        let token = decode_token_bc(bc, &model.dc_token_probs[0][dc_mode], model.dc_value_probs[5], true, true);
        let val = expand_token_bc(bc, &model.dc_value_probs, token, 6);
        coeffs[0] = val;
        fstate.last_idx[fstate.ctx_idx] = 0;

        let mut idx = 1;
        let mut last_val = val;
        while idx < 64 {
            let ac_band = VP6_IDX_TO_AC_BAND[idx];
            let ac_mode = last_val.abs().min(2) as usize;
            let has_nnz = (idx == 1) || (last_val != 0);
            let token = decode_token_bc(bc, &model.ac_val_probs[ac_mode][ac_band], model.ac_val_probs[ac_mode][ac_band][5], false, has_nnz);
            if token == 42 { break; }
            let val = expand_token_bc(bc, &model.ac_val_probs[ac_mode][ac_band], token, 6);
            coeffs[vp6model.zigzag[idx]] = val.wrapping_mul(fstate.ac_quant);
            idx += 1;
            last_val = val;
            if val == 0 {
                idx += decode_zero_run_bc(bc, &vp6model.zero_run_probs[if idx >= 7 { 1 } else { 0 }]);
                validate!(idx <= 64);
            }
        }
        fstate.coeff_cat[fstate.ctx_idx][0] = if coeffs[0] != 0 { 1 } else { 0 };
        fstate.top_ctx = fstate.coeff_cat[fstate.ctx_idx][0];
        fstate.last_idx[fstate.ctx_idx] = idx;
        Ok(())
    }
    fn decode_block_huff(&self, br: &mut BitReader, coeffs: &mut [i16; 64], vp6model: &VP6Models, model: &VP6HuffModels, fstate: &mut FrameState) -> DecoderResult<()> {
        let plane = if (fstate.plane == 0) || (fstate.plane == 3) { 0 } else { 1 };
        let mut last_val;

        if fstate.dc_zero_run[plane] == 0 {
            let (val, eob) = decode_token_huff(br, &model.dc_token_tree[plane])?;
            if eob {
                return Ok(());
            }
            last_val = val;
            coeffs[0] = val;
            if val == 0 {
                fstate.dc_zero_run[plane] = decode_eob_run_huff(br)?;
            }
        } else {
            last_val = 0;
            fstate.dc_zero_run[plane] -= 1;
        }

        if fstate.ac_zero_run[plane] > 0 {
            fstate.ac_zero_run[plane] -= 1;
            fstate.last_idx[fstate.ctx_idx] = 0;
            return Ok(());
        }

        let mut idx = 1;
        while idx < 64 {
            let ac_band = VP6_IDX_TO_AC_BAND[idx].min(3);
            let ac_mode = last_val.abs().min(2) as usize;
            let (val, eob) = decode_token_huff(br, &model.ac_token_tree[plane][ac_mode][ac_band])?;
            if eob {
                if idx == 1 {
                    fstate.ac_zero_run[plane] = decode_eob_run_huff(br)?;
                }
                break;
            }
            coeffs[vp6model.zigzag[idx]] = val.wrapping_mul(fstate.ac_quant);
            idx += 1;
            last_val = val;
            if val == 0 {
                idx += decode_zero_run_huff(br, &model.zero_run_tree[if idx >= 7 { 1 } else { 0 }])?;
                validate!(idx <= 64);
            }
        }

        fstate.last_idx[fstate.ctx_idx] = idx;

        Ok(())
    }
    fn mc_block(&self, dst: &mut NASimpleVideoFrame<u8>, mut mc_buf: NAVideoBufferRef<u8>, src: NAVideoBufferRef<u8>, plane: usize, x: usize, y: usize, mv: MV, loop_str: i16) {
        let is_luma = (plane != 1) && (plane != 2);
        let (sx, sy, mx, my, msx, msy) = if is_luma {
                (mv.x >> 2, mv.y >> 2, (mv.x & 3) << 1, (mv.y & 3) << 1, mv.x / 4, mv.y / 4)
            } else {
                (mv.x >> 3, mv.y >> 3, mv.x & 7, mv.y & 7, mv.x / 8, mv.y / 8)
            };
        let tmp_blk = mc_buf.get_data_mut().unwrap();
        get_block(tmp_blk, 16, src.clone(), plane, x, y, sx, sy);
        if (msx & 7) != 0 {
            let foff = (8 - (sx & 7)) as usize;
            let off = 2 + foff;
            vp31_loop_filter(tmp_blk, off, 1, 16, 12, loop_str);
        }
        if (msy & 7) != 0 {
            let foff = (8 - (sy & 7)) as usize;
            let off = (2 + foff) * 16;
            vp31_loop_filter(tmp_blk, off, 16, 1, 12, loop_str);
        }
        let copy_mode = (mx == 0) && (my == 0);
        let mut bicubic = !copy_mode && is_luma && self.bicubic;
        if is_luma && !copy_mode && (self.profile == VP6_ADVANCED_PROFILE) {
            if !self.autosel_pm {
                bicubic = true;
            } else {
                let mv_limit = 1 << (self.mv_thresh + 1);
                if (mv.x.abs() <= mv_limit) && (mv.y.abs() <= mv_limit) {
                    let mut var_off = 16 * 2 + 2;
                    if mv.x < 0 { var_off += 1; }
                    if mv.y < 0 { var_off += 16; }
                    let var = calc_variance(&tmp_blk[var_off..], 16);
                    if var >= self.var_thresh {
                        bicubic = true;
                    }
                }
            }
        }
        let dstride = dst.stride[plane];
        let dbuf = &mut dst.data[dst.offset[plane] + x + y * dstride..];
        if copy_mode {
            let src = &tmp_blk[2 * 16 + 2..];
            for (dline, sline) in dbuf.chunks_mut(dst.stride[plane]).zip(src.chunks(16)).take(8) {
                for i in 0..8 { dline[i] = sline[i]; }
            }
        } else if bicubic {
            let coeff_h = &VP6_BICUBIC_COEFFS[self.filter_alpha][mx as usize];
            let coeff_v = &VP6_BICUBIC_COEFFS[self.filter_alpha][my as usize];
            mc_bicubic(dbuf, dstride, tmp_blk, 16 * 2 + 2, 16, coeff_h, coeff_v);
        } else {
            mc_bilinear(dbuf, dstride, tmp_blk, 16 * 2 + 2, 16, mx as u16, my as u16);
        }
    }
}

fn update_scan(model: &mut VP6Models) {
    let mut idx = 1;
    for band in 0..16 {
        for i in 1..64 {
            if model.scan_order[i] == band {
                model.scan[idx] = i;
                idx += 1;
            }
        }
    }
    for i in 1..64 {
        model.zigzag[i] = ZIGZAG[model.scan[i]];
    }
}

fn reset_scan(model: &mut VP6Models, interlaced: bool) {
    const VP6_DEFAULT_SCAN_ORDER: [usize; 64] = [
         0,  0,  1,  1,  1,  2,  2,  2,
         2,  2,  2,  3,  3,  4,  4,  4,
         5,  5,  5,  5,  6,  6,  7,  7,
         7,  7,  7,  8,  8,  9,  9,  9,
         9,  9,  9, 10, 10, 11, 11, 11,
        11, 11, 11, 12, 12, 12, 12, 12,
        12, 13, 13, 13, 13, 13, 14, 14,
        14, 14, 15, 15, 15, 15, 15, 15
    ];
    const VP6_INTERLACED_SCAN_ORDER: [usize; 64] = [
         0,  1,  0,  1,  1,  2,  5,  3,
         2,  2,  2,  2,  4,  7,  8, 10,
         9,  7,  5,  4,  2,  3,  5,  6,
         8,  9, 11, 12, 13, 12, 11, 10,
         9,  7,  5,  4,  6,  7,  9, 11,
        12, 12, 13, 13, 14, 12, 11,  9,
         7,  9, 11, 12, 14, 14, 14, 15,
        13, 11, 13, 15, 15, 15, 15, 15
    ];

    if !interlaced {
        model.scan_order.copy_from_slice(&VP6_DEFAULT_SCAN_ORDER);
    } else {
        model.scan_order.copy_from_slice(&VP6_INTERLACED_SCAN_ORDER);
    }
    for i in 0..64 { model.scan[i] = i; }
    model.zigzag.copy_from_slice(&ZIGZAG);
}

fn decode_token_bc(bc: &mut BoolCoder, probs: &[u8], prob34: u8, is_dc: bool, has_nnz: bool) -> u8 {
    if has_nnz && !bc.read_prob(probs[0]) {
        if is_dc || bc.read_prob(probs[1]) {
            0
        } else {
            TOKEN_EOB
        }
    } else {
        vp_tree!(bc, probs[2],
                 1,
                 vp_tree!(bc, probs[3],
                          vp_tree!(bc, probs[4],
                                   2,
                                   vp_tree!(bc, prob34, 3, 4)),
                          TOKEN_LARGE))
    }
}

fn decode_zero_run_bc(bc: &mut BoolCoder, probs: &[u8; 14]) -> usize {
    let val = vp_tree!(bc, probs[0],
                    vp_tree!(bc, probs[1],
                        vp_tree!(bc, probs[2], 0, 1),
                        vp_tree!(bc, probs[3], 2, 3)),
                    vp_tree!(bc, probs[4],
                        vp_tree!(bc, probs[5],
                            vp_tree!(bc, probs[6], 4, 5),
                            vp_tree!(bc, probs[7], 6, 7)),
                        42));
    if val != 42 {
        val
    } else {
        let mut nval = 8;
        for i in 0..6 {
            nval                                += (bc.read_prob(probs[i + 8]) as usize) << i;
        }
        nval
    }
}

fn decode_token_huff(br: &mut BitReader, huff: &VP6Huff) -> DecoderResult<(i16, bool)> {
    const COEF_ADD_BITS: [u8; 6] = [ 1, 2, 3, 4, 5, 11 ];
    let tok                                     = br.read_huff(huff)?;
    match tok {
        0   => Ok((0, false)),
        1 | 2 | 3 | 4 => {
            if !br.read_bool()? {
                Ok((tok as i16, false))
            } else {
                Ok((-(tok as i16), false))
            }
        },
        5 | 6 | 7 | 8 | 9 | 10 => {
            let base = (tok - 5) as usize;
            let add_bits                        = br.read(COEF_ADD_BITS[base])? as i16;
            let val = VP56_COEF_BASE[base] + add_bits;
            if !br.read_bool()? {
                Ok((val, false))
            } else {
                Ok((-val, false))
            }
        },
        _   => Ok((0, true)),
    }
}

fn decode_eob_run_huff(br: &mut BitReader) -> DecoderResult<usize> {
    let val                                     = br.read(2)?;
    match val {
        0 => Ok(0),
        1 => Ok(1),
        2 => {
            let val                             = br.read(2)?;
            Ok((val as usize) + 2)
        },
        _ => {
            if br.read_bool()? {
                Ok((br.read(6)? as usize) + 10)
            } else {
                Ok((br.read(2)? as usize) + 6)
            }
        },
    }
}

fn decode_zero_run_huff(br: &mut BitReader, huff: &VP6Huff) -> DecoderResult<usize> {
    let val                                     = br.read_huff(huff)?;
    if val < 8 {
        Ok(val as usize)
    } else {
        Ok((br.read(6)? as usize) + 8)
    }
}


fn get_block(dst: &mut [u8], dstride: usize, src: NAVideoBufferRef<u8>, comp: usize,
             dx: usize, dy: usize, mv_x: i16, mv_y: i16)
{
    let (w, h) = src.get_dimensions(comp);
    let sx = (dx as isize) + (mv_x as isize);
    let sy = (dy as isize) + (mv_y as isize);

    if (sx - 2 < 0) || (sx + 8 + 2 > (w as isize)) ||
       (sy - 2 < 0) || (sy + 8 + 2 > (h as isize)) {
        edge_emu(&src, sx - 2, sy - 2, 8 + 2 + 2, 8 + 2 + 2,
                 dst, dstride, comp, 0);
    } else {
        let sstride = src.get_stride(comp);
        let soff    = src.get_offset(comp);
        let sdta    = src.get_data();
        let sbuf: &[u8] = sdta.as_slice();
        let saddr = soff + ((sx - 2) as usize) + ((sy - 2) as usize) * sstride;
        let src = &sbuf[saddr..];
        for (dline, sline) in dst.chunks_mut(dstride).zip(src.chunks(sstride)).take(12) {
            for i in 0..12 {
                dline[i] = sline[i];
            }
        }
    }
}

fn calc_variance(src: &[u8], stride: usize) -> u16 {
    let mut sum = 0;
    let mut ssum = 0;
    for line in src.chunks(stride * 2).take(4) {
        for el in line.iter().take(8).step_by(2) {
            let pix = *el as u32;
            sum += pix;
            ssum += pix * pix;
        }
    }
    ((ssum * 16 - sum * sum) >> 8) as u16
}

macro_rules! mc_filter {
    (bilinear; $a: expr, $b: expr, $c: expr) => {
        ((($a as u16) * (8 - $c) + ($b as u16) * $c + 4) >> 3) as u8
    };
    (bicubic; $src: expr, $off: expr, $step: expr, $coeffs: expr) => {
        ((($src[$off - $step]     as i32) * ($coeffs[0] as i32) +
          ($src[$off]             as i32) * ($coeffs[1] as i32) +
          ($src[$off + $step]     as i32) * ($coeffs[2] as i32) +
          ($src[$off + $step * 2] as i32) * ($coeffs[3] as i32) + 64) >> 7).min(255).max(0) as u8
    }
}

//#[allow(snake_case)]
fn mc_bilinear(dst: &mut [u8], dstride: usize, src: &[u8], mut soff: usize, sstride: usize, mx: u16, my: u16) {
    if my == 0 {
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                dline[i] = mc_filter!(bilinear; src[soff + i], src[soff + i + 1], mx);
            }
            soff += sstride;
        }
    } else if mx == 0 {
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                dline[i] = mc_filter!(bilinear; src[soff + i], src[soff + i + sstride], my);
            }
            soff += sstride;
        }
    } else {
        let mut tmp = [0u8; 8];
        for i in 0..8 {
            tmp[i] = mc_filter!(bilinear; src[soff + i], src[soff + i + 1], mx);
        }
        soff += sstride;
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                let cur = mc_filter!(bilinear; src[soff + i], src[soff + i + 1], mx);
                dline[i] = mc_filter!(bilinear; tmp[i], cur, my);
                tmp[i] = cur;
            }
            soff += sstride;
        }
    }
}

fn mc_bicubic(dst: &mut [u8], dstride: usize, src: &[u8], mut soff: usize, sstride: usize, coeffs_w: &[i16; 4], coeffs_h: &[i16; 4]) {
    if coeffs_h[1] == 128 {
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                dline[i] = mc_filter!(bicubic; src, soff + i, 1, coeffs_w);
            }
            soff += sstride;
        }
    } else if coeffs_w[1] == 128 { // horizontal-only interpolation
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                dline[i] = mc_filter!(bicubic; src, soff + i, sstride, coeffs_h);
            }
            soff += sstride;
        }
    } else {
        let mut buf = [0u8; 16 * 11];
        soff -= sstride;
        for dline in buf.chunks_mut(16) {
            for i in 0..8 {
                dline[i] = mc_filter!(bicubic; src, soff + i, 1, coeffs_w);
            }
            soff += sstride;
        }
        let mut soff = 16;
        for dline in dst.chunks_mut(dstride).take(8) {
            for i in 0..8 {
                dline[i] = mc_filter!(bicubic; buf, soff + i, 16, coeffs_h);
            }
            soff += 16;
        }
    }
}

struct VP6Decoder {
    dec:        VP56Decoder,
    info:       NACodecInfoRef,
    br:         VP6BR,
    has_alpha:  bool,
}

impl VP6Decoder {
    fn new(has_alpha: bool) -> Self {
        Self {
            dec:        VP56Decoder::new(6, has_alpha, true),
            info:       NACodecInfoRef::default(),
            br:         VP6BR::new(),
            has_alpha,
        }
    }
}

impl NADecoder for VP6Decoder {
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let fmt = if !self.has_alpha {
                    YUV420_FORMAT
                } else {
                    VP_YUVA420_FORMAT
                };
            let myvinfo = NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), false, fmt);
            let myinfo = NACodecTypeInfo::Video(myvinfo.clone());
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            self.dec.init(supp, myvinfo)?;
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

        let (bufinfo, ftype) = self.dec.decode_frame(supp, src.as_slice(), &mut self.br)?;

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(ftype == FrameType::I);
        frm.set_frame_type(ftype);
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.dec.flush();
    }
}

pub fn get_decoder_vp6() -> Box<NADecoder + Send> {
    Box::new(VP6Decoder::new(false))
}

pub fn get_decoder_vp6_alpha() -> Box<NADecoder + Send> {
    Box::new(VP6Decoder::new(true))
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::duck_register_all_codecs;
    use nihav_commonfmt::generic_register_all_demuxers;

    #[test]
    fn test_vp6() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        test_decoding("avi", "vp6", "assets/Duck/selection_720x576_300kBit_vp60i.avi", Some(16),
                      &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5([0x042c3e96, 0x8a9b26a2, 0x4dcbaf66, 0x1b788d03]));
    }
    #[test]
    fn test_vp6_huff() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        test_decoding("avi", "vp6", "assets/Duck/vp6_crash.avi", Some(4),
                      &dmx_reg, &dec_reg, ExpectedTestResult::MD5Frames(vec![
                            [0xdcd70fa0, 0x0d075ce2, 0xc9e65077, 0xb003a92e],
                            [0x334abf96, 0x3a004c7a, 0x5781cd5c, 0x25c3ae5c],
                            [0x6164b851, 0x528cd8de, 0xecab7328, 0x4b49708a],
                            [0x11b048ac, 0xedb3e471, 0xd04e9399, 0x64e623e3],
                            [0x182871b1, 0x2146893a, 0x2912210e, 0x6dd592e8]]));
    }
    // todo find good sample for vp6a test
}

const VP6_AC_PROBS: [[[[u8; 11]; 6]; 2]; 3] = [
  [
    [
      [ 227, 246, 230, 247, 244, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 209, 231, 231, 249, 249, 253, 255, 255, 255 ],
      [ 255, 255, 225, 242, 241, 251, 253, 255, 255, 255, 255 ],
      [ 255, 255, 241, 253, 252, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 248, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ], [
      [ 240, 255, 248, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 240, 253, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ]
  ], [
    [
      [ 206, 203, 227, 239, 247, 255, 253, 255, 255, 255, 255 ],
      [ 207, 199, 220, 236, 243, 252, 252, 255, 255, 255, 255 ],
      [ 212, 219, 230, 243, 244, 253, 252, 255, 255, 255, 255 ],
      [ 236, 237, 247, 252, 253, 255, 255, 255, 255, 255, 255 ],
      [ 240, 240, 248, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ], [
      [ 230, 233, 249, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 238, 238, 250, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 248, 251, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ]
  ], [
    [
      [ 225, 239, 227, 231, 244, 253, 243, 255, 255, 253, 255 ],
      [ 232, 234, 224, 228, 242, 249, 242, 252, 251, 251, 255 ],
      [ 235, 249, 238, 240, 251, 255, 249, 255, 253, 253, 255 ],
      [ 249, 253, 251, 250, 255, 255, 255, 255, 255, 255, 255 ],
      [ 251, 250, 249, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ], [
      [ 243, 244, 250, 250, 255, 255, 255, 255, 255, 255, 255 ],
      [ 249, 248, 250, 253, 255, 255, 255, 255, 255, 255, 255 ],
      [ 253, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ],
      [ 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255 ]
    ]
  ]
];

const VP6_DC_WEIGHTS: [[[i16; 2]; 3]; 5] = [
    [ [ 122, 133 ], [ 133,  51 ], [ 142, -16 ] ],
    [ [   0,   1 ], [   0,   1 ], [   0,   1 ] ],
    [ [  78, 171 ], [ 169,  71 ], [ 221, -30 ] ],
    [ [ 139, 117 ], [ 214,  44 ], [ 246,  -3 ] ],
    [ [ 168,  79 ], [ 210,  38 ], [ 203,  17 ] ]
];

const VP6_IDX_TO_AC_BAND: [usize; 64] = [
    0, 0, 1, 1, 1, 2, 2, 2,
    2, 2, 2, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5
];

const VP6_BICUBIC_COEFFS: [[[i16; 4]; 8]; 17] = [
  [
    [   0, 128,   0,   0 ],
    [  -3, 122,   9,   0 ],
    [  -4, 109,  24,  -1 ],
    [  -5,  91,  45,  -3 ],
    [  -4,  68,  68,  -4 ],
    [  -3,  45,  91,  -5 ],
    [  -1,  24, 109,  -4 ],
    [   0,   9, 122,  -3 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -4, 124,   9,  -1 ],
    [  -5, 110,  25,  -2 ],
    [  -6,  91,  46,  -3 ],
    [  -5,  69,  69,  -5 ],
    [  -3,  46,  91,  -6 ],
    [  -2,  25, 110,  -5 ],
    [  -1,   9, 124,  -4 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -4, 123,  10,  -1 ],
    [  -6, 110,  26,  -2 ],
    [  -7,  92,  47,  -4 ],
    [  -6,  70,  70,  -6 ],
    [  -4,  47,  92,  -7 ],
    [  -2,  26, 110,  -6 ],
    [  -1,  10, 123,  -4 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -5, 124,  10,  -1 ],
    [  -7, 110,  27,  -2 ],
    [  -7,  91,  48,  -4 ],
    [  -6,  70,  70,  -6 ],
    [  -4,  48,  92,  -8 ],
    [  -2,  27, 110,  -7 ],
    [  -1,  10, 124,  -5 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -6, 124,  11,  -1 ],
    [  -8, 111,  28,  -3 ],
    [  -8,  92,  49,  -5 ],
    [  -7,  71,  71,  -7 ],
    [  -5,  49,  92,  -8 ],
    [  -3,  28, 111,  -8 ],
    [  -1,  11, 124,  -6 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -6, 123,  12,  -1 ],
    [  -9, 111,  29,  -3 ],
    [  -9,  93,  50,  -6 ],
    [  -8,  72,  72,  -8 ],
    [  -6,  50,  93,  -9 ],
    [  -3,  29, 111,  -9 ],
    [  -1,  12, 123,  -6 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -7, 124,  12,  -1 ],
    [ -10, 111,  30,  -3 ],
    [ -10,  93,  51,  -6 ],
    [  -9,  73,  73,  -9 ],
    [  -6,  51,  93, -10 ],
    [  -3,  30, 111, -10 ],
    [  -1,  12, 124,  -7 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -7, 123,  13,  -1 ],
    [ -11, 112,  31,  -4 ],
    [ -11,  94,  52,  -7 ],
    [ -10,  74,  74, -10 ],
    [  -7,  52,  94, -11 ],
    [  -4,  31, 112, -11 ],
    [  -1,  13, 123,  -7 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -8, 124,  13,  -1 ],
    [ -12, 112,  32,  -4 ],
    [ -12,  94,  53,  -7 ],
    [ -10,  74,  74, -10 ],
    [  -7,  53,  94, -12 ],
    [  -4,  32, 112, -12 ],
    [  -1,  13, 124,  -8 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -9, 124,  14,  -1 ],
    [ -13, 112,  33,  -4 ],
    [ -13,  95,  54,  -8 ],
    [ -11,  75,  75, -11 ],
    [  -8,  54,  95, -13 ],
    [  -4,  33, 112, -13 ],
    [  -1,  14, 124,  -9 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -9, 123,  15,  -1 ],
    [ -14, 113,  34,  -5 ],
    [ -14,  95,  55,  -8 ],
    [ -12,  76,  76, -12 ],
    [  -8,  55,  95, -14 ],
    [  -5,  34, 112, -13 ],
    [  -1,  15, 123,  -9 ]
  ], [
    [   0, 128,   0,   0 ],
    [ -10, 124,  15,  -1 ],
    [ -14, 113,  34,  -5 ],
    [ -15,  96,  56,  -9 ],
    [ -13,  77,  77, -13 ],
    [  -9,  56,  96, -15 ],
    [  -5,  34, 113, -14 ],
    [  -1,  15, 124, -10 ]
  ], [
    [   0, 128,   0,   0 ],
    [ -10, 123,  16,  -1 ],
    [ -15, 113,  35,  -5 ],
    [ -16,  98,  56, -10 ],
    [ -14,  78,  78, -14 ],
    [ -10,  56,  98, -16 ],
    [  -5,  35, 113, -15 ],
    [  -1,  16, 123, -10 ]
  ], [
    [   0, 128,   0,   0 ],
    [ -11, 124,  17,  -2 ],
    [ -16, 113,  36,  -5 ],
    [ -17,  98,  57, -10 ],
    [ -14,  78,  78, -14 ],
    [ -10,  57,  98, -17 ],
    [  -5,  36, 113, -16 ],
    [  -2,  17, 124, -11 ]
  ], [
    [   0, 128,   0,   0 ],
    [ -12, 125,  17,  -2 ],
    [ -17, 114,  37,  -6 ],
    [ -18,  99,  58, -11 ],
    [ -15,  79,  79, -15 ],
    [ -11,  58,  99, -18 ],
    [  -6,  37, 114, -17 ],
    [  -2,  17, 125, -12 ]
  ], [
    [   0, 128,   0,   0 ],
    [ -12, 124,  18,  -2 ],
    [ -18, 114,  38,  -6 ],
    [ -19,  99,  59, -11 ],
    [ -16,  80,  80, -16 ],
    [ -11,  59,  99, -19 ],
    [  -6,  38, 114, -18 ],
    [  -2,  18, 124, -12 ]
  ], [
    [   0, 128,   0,   0 ],
    [  -4, 118,  16,  -2 ],
    [  -7, 106,  34,  -5 ],
    [  -8,  90,  53,  -7 ],
    [  -8,  72,  72,  -8 ],
    [  -7,  53,  90,  -8 ],
    [  -5,  34, 106,  -7 ],
    [  -2,  16, 118,  -4 ]
  ]
];
