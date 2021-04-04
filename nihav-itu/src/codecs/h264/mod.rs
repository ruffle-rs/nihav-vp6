/*
 known bugs and limitations:
  * weighted motion compensation is not implemented
  * wrong slice boundary filtering
  * not fully correct deblock strength selection for P/B-macroblocks
  * scaling lists for 4x4 blocks
*/
use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use nihav_core::io::bitreader::*;
use nihav_core::io::intcode::*;
use nihav_codec_support::codecs::{MV, ZERO_MV};

mod types;
pub use types::*;
mod pic_ref;
pub use pic_ref::*;
#[allow(clippy::identity_op)]
#[allow(clippy::erasing_op)]
#[allow(clippy::many_single_char_names)]
#[allow(clippy::range_plus_one)]
mod dsp;
use dsp::*;
mod cabac;
use cabac::*;
mod cabac_coder;
use cabac_coder::*;
mod cavlc;
use cavlc::*;
mod loopfilter;
use loopfilter::*;
mod sets;
use sets::*;
mod slice;
use slice::*;

trait ReadUE {
    fn read_ue(&mut self) -> DecoderResult<u32>;
    fn read_te(&mut self, range: u32) -> DecoderResult<u32>;
    fn read_ue_lim(&mut self, max_val: u32) -> DecoderResult<u32> {
        let val = self.read_ue()?;
        validate!(val <= max_val);
        Ok(val)
    }
    fn read_se(&mut self) -> DecoderResult<i32> {
        let val = self.read_ue()?;
        if (val & 1) != 0 {
            Ok (((val >> 1) as i32) + 1)
        } else {
            Ok (-((val >> 1) as i32))
        }
    }
}

impl<'a> ReadUE for BitReader<'a> {
    fn read_ue(&mut self) -> DecoderResult<u32> {
        Ok(self.read_code(UintCodeType::GammaP)? - 1)
    }
    fn read_te(&mut self, range: u32) -> DecoderResult<u32> {
        if range == 1 {
            if self.read_bool()? {
                Ok(0)
            } else {
                Ok(1)
            }
        } else {
            let val = self.read_ue()?;
            validate!(val <= range);
            Ok(val)
        }
    }
}

#[derive(Clone,Copy)]
pub struct Coeff8x8 {
    pub coeffs:     [i16; 64],
}

impl Coeff8x8 {
    fn clear(&mut self) {
        self.coeffs = [0; 64];
    }
}

impl Default for Coeff8x8 {
    fn default() -> Self {
        Self {
            coeffs: [0; 64],
        }
    }
}

#[derive(Clone,Copy,Default)]
pub struct CurrentMBInfo {
    pub mb_type:        MBType,
    pub sub_mb_type:    [SubMBType; 4],
    pub ipred:          [IntraPredMode; 16],
    pub chroma_ipred:   u8,
    pub luma_ipred:     [u8; 16],
    pub mv_l0:          [MV; 16],
    pub ref_l0:         [PicRef; 4],
    pub mv_l1:          [MV; 16],
    pub ref_l1:         [PicRef; 4],
    pub qp_y:           u8,
    pub cbpy:           u8,
    pub cbpc:           u8,
    pub coeffs:         [[i16; 16]; 25],
    pub coeffs8x8:      [Coeff8x8; 4],
    pub chroma_dc:      [[i16; 4]; 2],
    pub coded:          [bool; 25],
    pub transform_size_8x8: bool,
}

impl CurrentMBInfo {
    fn clear_coeffs8x8(&mut self) {
        for c in self.coeffs8x8.iter_mut() {
            c.clear();
        }
    }
    fn can_have_8x8_tx(&self, inference_flag: bool) -> bool {
        match self.mb_type {
            MBType::Intra4x4 | MBType::Intra8x8 | MBType::Intra16x16(_, _, _) | MBType::PCM => false,
            MBType::P8x8 | MBType::P8x8Ref0 | MBType::B8x8 => {
                for &sub_id in self.sub_mb_type.iter() {
                    match sub_id {
                        SubMBType::P8x8 |
                        SubMBType::B8x8(_)
                            => {},
                        SubMBType::Direct8x8
                            => if !inference_flag { return false; },
                        _ => return false,
                    };
                }
                true
            },
            MBType::Direct => inference_flag,
            _ => true,
        }
    }
}

fn get_long_term_id(is_idr: bool, slice_hdr: &SliceHeader) -> Option<usize> {
    if is_idr && !slice_hdr.long_term_reference {
        None
    } else {
        let marking = &slice_hdr.adaptive_ref_pic_marking;
        for (&op, &arg) in marking.memory_management_control_op.iter().zip(marking.operation_arg.iter()).take(marking.num_ops) {
            if op == 6 {
                return Some(arg as usize);
            }
        }
        None
    }
}

struct H264Decoder {
    info:       NACodecInfoRef,
    width:      usize,
    height:     usize,
    num_mbs:    usize,
    nal_len:    u8,
    sps:        Vec<SeqParameterSet>,
    cur_sps:    usize,
    pps:        Vec<PicParameterSet>,
    cur_pps:    usize,

    skip_mode:      FrameSkipMode,
    deblock_skip:   bool,

    is_mbaff:   bool,

    cavlc_cb:   CAVLCTables,

    sstate:     SliceState,

    cur_pic:    Option<PictureInfo>,
    cur_id:     u16,
    has_pic:    bool,
    frame_refs: FrameRefs,

    temporal_mv:    bool,
    deblock_mode:   u8,
    lf_alpha:       i8,
    lf_beta:        i8,
    is_s:           bool,

    ipcm_buf:   [u8; 256 + 64 + 64],

    avg_buf:    NAVideoBufferRef<u8>,

    transform_8x8_mode: bool,
}

fn unescape_nal(src: &[u8], dst: &mut Vec<u8>) -> usize {
    let mut off = 0;
    let mut zrun = 0;
    dst.truncate(0);
    dst.reserve(src.len());
    while off < src.len() {
        dst.push(src[off]);
        if src[off] != 0 {
            zrun = 0;
        } else {
            zrun += 1;
            if zrun == 2 && off + 1 < src.len() && src[off + 1] == 0x03 {
                zrun = 0;
                off += 1;
            }
            if zrun >= 3 && off + 1 < src.len() && src[off + 1] == 0x01 {
                off -= 3;
                dst.truncate(off);
                break;
            }
        }
        off += 1;
    }
    off
}

impl H264Decoder {
    fn new() -> Self {
        let avg_vi = NAVideoInfo { width: 32, height: 32, flipped: false, format: YUV420_FORMAT, bits: 12 };
        let avg_buf = alloc_video_buffer(avg_vi, 4).unwrap().get_vbuf().unwrap();
        H264Decoder{
            info:       NACodecInfoRef::default(),
            width:      0,
            height:     0,
            num_mbs:    0,
            nal_len:    0,
            sps:        Vec::with_capacity(1),
            cur_sps:    0,
            pps:        Vec::with_capacity(3),
            cur_pps:    0,

            skip_mode:      FrameSkipMode::default(),
            deblock_skip:   false,

            is_mbaff:   false,

            cavlc_cb:   CAVLCTables::new(),

            sstate:     SliceState::new(),
            cur_pic:    None,
            cur_id:     0,
            has_pic:    false,
            frame_refs: FrameRefs::new(),

            temporal_mv:        false,
            deblock_mode:       0,
            lf_alpha:           0,
            lf_beta:            0,
            is_s:               false,

            ipcm_buf:   [0; 256 + 64 + 64],

            avg_buf,

            transform_8x8_mode: false,
        }
    }
    fn handle_nal(&mut self, src: &[u8], supp: &mut NADecoderSupport, skip_decoding: bool) -> DecoderResult<()> {
        validate!(!src.is_empty());
        validate!((src[0] & 0x80) == 0);
        let nal_ref_idc   = src[0] >> 5;
        let nal_unit_type = src[0] & 0x1F;

        let mut full_size = src.len() * 8;
        for &byte in src.iter().rev() {
            if byte == 0 {
                full_size -= 8;
            } else {
                full_size -= (byte.trailing_zeros() + 1) as usize;
                break;
            }
        }
        validate!(full_size > 0);
        match nal_unit_type {
             1 | 5 if !skip_decoding => {
                let is_idr = nal_unit_type == 5;
                let mut br = BitReader::new(&src[..(full_size + 7)/8], BitReaderMode::BE);
                                                    br.skip(8)?;

                let slice_hdr = parse_slice_header(&mut br, &self.sps, &self.pps, is_idr, nal_ref_idc)?;
                validate!(br.tell() < full_size);
                let full_id;
                if slice_hdr.first_mb_in_slice == 0 {
                    validate!(self.cur_pic.is_none());
                    for (i, pps) in self.pps.iter().enumerate() {
                        if pps.pic_parameter_set_id == slice_hdr.pic_parameter_set_id {
                            self.cur_pps = i;
                            break;
                        }
                    }
                    for (i, sps) in self.sps.iter().enumerate() {
                        if sps.seq_parameter_set_id == self.pps[self.cur_pps].seq_parameter_set_id {
                            self.cur_sps = i;
                            break;
                        }
                    }

                    full_id = self.frame_refs.calc_picture_num(&slice_hdr, is_idr, nal_ref_idc, &self.sps[self.cur_sps]);

                    let sps = &self.sps[self.cur_sps];
                    if sps.chroma_format_idc != 1 || sps.bit_depth_luma != 8 || sps.bit_depth_chroma != 8 {
println!(" chroma fmt {} bits {}/{}", sps.chroma_format_idc, sps.bit_depth_luma, sps.bit_depth_chroma);
                        return Err(DecoderError::NotImplemented);
                    }
                    //let pps = &self.pps[self.cur_pps];

                    if is_idr {
                        self.frame_refs.clear_refs();
                    }

                    self.width  = sps.pic_width_in_mbs  << 4;
                    self.height = sps.pic_height_in_mbs << 4;
                    self.num_mbs = sps.pic_width_in_mbs * sps.pic_height_in_mbs;

                    self.is_mbaff = sps.mb_adaptive_frame_field && !slice_hdr.field_pic;
                    if self.is_mbaff {
println!("MBAFF");
                        return Err(DecoderError::NotImplemented);
                    }
                    if !sps.frame_mbs_only {
println!("PAFF?");
                        return Err(DecoderError::NotImplemented);
                    }

//if slice_hdr.slice_type.is_b() { return Ok(()); }
                    self.cur_id = full_id as u16;
                } else {
                    if let Some(ref mut pic) = self.cur_pic {
                        validate!(pic.cur_mb == slice_hdr.first_mb_in_slice);
                        let new_type = slice_hdr.slice_type.to_frame_type();
                        pic.pic_type = match (pic.pic_type, new_type) {
                                (FrameType::I, _) => new_type,
                                (_, FrameType::B) => FrameType::B,
                                _ => pic.pic_type,
                            };
                        full_id = pic.full_id;
                    } else {
                        return Ok(());//Err(DecoderError::InvalidData);
                    }
                    validate!(self.cur_pps < self.pps.len() && self.pps[self.cur_pps].pic_parameter_set_id == slice_hdr.pic_parameter_set_id);
                }

                let sps = &self.sps[self.cur_sps];
                let pps = &self.pps[self.cur_pps];

                self.temporal_mv = !slice_hdr.direct_spatial_mv_pred;
                self.is_s = slice_hdr.slice_type == SliceType::SI || slice_hdr.slice_type == SliceType::SP;
                self.deblock_mode = slice_hdr.disable_deblocking_filter_idc;
                self.lf_alpha = slice_hdr.slice_alpha_c0_offset;
                self.lf_beta  = slice_hdr.slice_beta_offset;

                self.frame_refs.select_refs(sps, &slice_hdr, full_id);

                if slice_hdr.adaptive_ref_pic_marking_mode {
                    self.frame_refs.apply_adaptive_marking(&slice_hdr.adaptive_ref_pic_marking, slice_hdr.frame_num, 1 << self.sps[self.cur_sps].log2_max_frame_num)?;
                }
                if slice_hdr.first_mb_in_slice == 0 {
                    let ret = supp.pool_u8.get_free();
                    if ret.is_none() {
                        return Err(DecoderError::AllocError);
                    }
                    let tmp_vinfo = NAVideoInfo::new(self.width, self.height, false, YUV420_FORMAT);
                    let mut buf = ret.unwrap();
                    if buf.get_info() != tmp_vinfo {
                        supp.pool_u8.reset();
                        supp.pool_u8.prealloc_video(tmp_vinfo, 4)?;
                        let ret = supp.pool_u8.get_free();
                        if ret.is_none() {
                            return Err(DecoderError::AllocError);
                        }
                        buf = ret.unwrap();
                    }
                    self.cur_pic = Some(PictureInfo {
                            id: slice_hdr.frame_num,
                            full_id,
                            pic_type: slice_hdr.slice_type.to_frame_type(),
                            buf,
                            cur_mb: 0,
                            is_ref: nal_ref_idc != 0,
                            long_term: get_long_term_id(is_idr, &slice_hdr),
                            mv_info: FrameMV::new(sps.pic_width_in_mbs, sps.pic_height_in_mbs),
                        });
                }

                self.transform_8x8_mode = pps.transform_8x8_mode;

                self.sstate.reset(sps.pic_width_in_mbs, sps.pic_height_in_mbs, slice_hdr.first_mb_in_slice);
                if !pps.entropy_coding_mode {
                    self.has_pic = self.decode_slice_cavlc(&mut br, &slice_hdr, full_size)?;
                } else {
                    br.align();
                    let start = (br.tell() / 8) as usize;
                    let csrc = &src[start..];
                    validate!(csrc.len() >= 2);
                    let mut cabac = CABAC::new(csrc, slice_hdr.slice_type, slice_hdr.slice_qp, slice_hdr.cabac_init_idc as usize)?;
                    self.has_pic = self.decode_slice_cabac(&mut cabac, &slice_hdr)?;
                }
                if !self.deblock_skip && self.deblock_mode != 1 {
                    if let Some(ref mut pic) = self.cur_pic {
                        let mut frm = NASimpleVideoFrame::from_video_buf(&mut pic.buf).unwrap();
                        if self.sstate.mb_x != 0 {
                            loop_filter_row(&mut frm, &self.sstate, self.lf_alpha, self.lf_beta);
                        }
                        loop_filter_last(&mut frm, &self.sstate, self.lf_alpha, self.lf_beta);
                    }
                }
            },
             2 => { // slice data partition A
                //slice header
                //slice id = read_ue()
                //cat 2 slice data (all but MB layer residual)
                return Err(DecoderError::NotImplemented);
            },
             3 => { // slice data partition B
                //slice id = read_ue()
                //if pps.redundant_pic_cnt_present { redundant_pic_cnt = read_ue() }
                //cat 3 slice data (MB layer residual)
                return Err(DecoderError::NotImplemented);
            },
             4 => { // slice data partition C
                //slice id = read_ue()
                //if pps.redundant_pic_cnt_present { redundant_pic_cnt = read_ue() }
                //cat 4 slice data (MB layer residual)
                return Err(DecoderError::NotImplemented);
            },
             6 => {}, //SEI
             7 => {
                let sps = parse_sps(&src[1..])?;
                self.sps.push(sps);
            },
             8 => {
                validate!(full_size >= 8 + 16);
                let pps = parse_pps(&src[1..], &self.sps, full_size - 8)?;
                let mut found = false;
                for stored_pps in self.pps.iter_mut() {
                    if stored_pps.pic_parameter_set_id == pps.pic_parameter_set_id {
                        *stored_pps = pps.clone();
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.pps.push(pps);
                }
            },
             9 => { // access unit delimiter
            },
            10 => {}, //end of sequence
            11 => {}, //end of stream
            12 => {}, //filler
            _  => {},
        };

        Ok(())
    }
    fn pred_intra(frm: &mut NASimpleVideoFrame<u8>, sstate: &SliceState, mb_info: &CurrentMBInfo) {
        let yoff = frm.offset[0] + sstate.mb_x * 16 + sstate.mb_y * 16 * frm.stride[0];
        match mb_info.mb_type {
            MBType::Intra16x16(imode, _, _) => {
                let id = if imode != 2 || (sstate.has_top && sstate.has_left) {
                        imode as usize
                    } else if !sstate.has_top && !sstate.has_left {
                        IPRED8_DC128
                    } else if !sstate.has_left {
                        IPRED8_DC_TOP
                    } else {
                        IPRED8_DC_LEFT
                    };
                IPRED_FUNCS16X16[id](frm.data, yoff, frm.stride[0]);
            },
            MBType::Intra8x8 => {
                let mut ictx = IPred8Context::new();
                for part in 0..4 {
                    let x = (part & 1) * 2;
                    let y = part & 2;
                    let blk4 = x + y * 4;

                    let cur_yoff = yoff + x * 4 + y * 4 * frm.stride[0];
                    let has_top = y > 0 || sstate.has_top;
                    let has_left = x > 0 || sstate.has_left;
                    let imode = mb_info.ipred[blk4];
                    let id = if imode != IntraPredMode::DC || (has_top && has_left) {
                            let im_id: u8 = imode.into();
                            im_id as usize
                        } else if !has_top && !has_left {
                            IPRED4_DC128
                        } else if !has_left {
                            IPRED4_DC_TOP
                        } else {
                            IPRED4_DC_LEFT
                        };
                    let mb_idx = sstate.mb_x + sstate.mb_y * sstate.mb_w;
                    let noright = (y == 2 || sstate.mb_x == sstate.mb_w - 1 || mb_idx < sstate.mb_start + sstate.mb_w) && (x == 2);
                    let has_tl = (has_top && x > 0) || (has_left && y > 0) || (x == 0 && y == 0 && sstate.mb_x > 0 && mb_idx > sstate.mb_start + sstate.mb_w);
                    if id != IPRED4_DC128 {
                        ictx.fill(frm.data, cur_yoff, frm.stride[0], has_top, has_top && !noright, has_left, has_tl);
                    }
                    IPRED_FUNCS8X8_LUMA[id](&mut frm.data[cur_yoff..], frm.stride[0], &ictx);
                    if mb_info.coded[blk4] {
                        add_coeffs8(frm.data, cur_yoff, frm.stride[0], &mb_info.coeffs8x8[part].coeffs);
                    }
                }
            },
            MBType::Intra4x4 => {
                for &(x,y) in I4X4_SCAN.iter() {
                    let x = x as usize;
                    let y = y as usize;
                    let cur_yoff = yoff + x * 4 + y * 4 * frm.stride[0];
                    let has_top = y > 0 || sstate.has_top;
                    let has_left = x > 0 || sstate.has_left;
                    let imode = mb_info.ipred[x + y * 4];
                    let id = if imode != IntraPredMode::DC || (has_top && has_left) {
                            let im_id: u8 = imode.into();
                            im_id as usize
                        } else if !has_top && !has_left {
                            IPRED4_DC128
                        } else if !has_left {
                            IPRED4_DC_TOP
                        } else {
                            IPRED4_DC_LEFT
                        };
                    let noright = (sstate.mb_x == sstate.mb_w - 1 || sstate.mb_x + sstate.mb_y * sstate.mb_w < sstate.mb_start + sstate.mb_w) && (x == 3);
                    let tr: [u8; 4] = if y == 0 {
                            if has_top && !noright {
                                let i = cur_yoff - frm.stride[0];
                                [frm.data[i + 4], frm.data[i + 5], frm.data[i + 6], frm.data[i + 7]]
                            } else if has_top {
                                let i = cur_yoff - frm.stride[0];
                                [frm.data[i + 3], frm.data[i + 3], frm.data[i + 3], frm.data[i + 3]]
                            } else {
                                [0; 4]
                            }
                        } else if (x & 1) == 0 || (x == 1 && y == 2) {
                            let i = cur_yoff - frm.stride[0];
                            [frm.data[i + 4], frm.data[i + 5], frm.data[i + 6], frm.data[i + 7]]
                        } else {
                            let i = cur_yoff - frm.stride[0];
                            [frm.data[i + 3], frm.data[i + 3], frm.data[i + 3], frm.data[i + 3]]
                        };
                    IPRED_FUNCS4X4[id](frm.data, cur_yoff, frm.stride[0], &tr);
                    if mb_info.coded[x + y * 4] {
                        add_coeffs(frm.data, cur_yoff, frm.stride[0], &mb_info.coeffs[x + y * 4]);
                    }
                }
            },
            _ => unreachable!(),
        };
        let id = if mb_info.chroma_ipred != 0 || (sstate.has_top && sstate.has_left) {
                mb_info.chroma_ipred as usize
            } else if !sstate.has_top && !sstate.has_left {
                IPRED8_DC128
            } else if !sstate.has_left {
                IPRED8_DC_TOP
            } else {
                IPRED8_DC_LEFT
            };
        for chroma in 1..3 {
            let off = frm.offset[chroma] + sstate.mb_x * 8 + sstate.mb_y * 8 * frm.stride[chroma];
            IPRED_FUNCS8X8_CHROMA[id](frm.data, off, frm.stride[chroma]);
        }
    }
    fn add_luma(frm: &mut NASimpleVideoFrame<u8>, sstate: &SliceState, mb_info: &CurrentMBInfo) {
        let mut yoff = frm.offset[0] + sstate.mb_x * 16 + sstate.mb_y * 16 * frm.stride[0];
        if !mb_info.transform_size_8x8 {
            for y in 0..4 {
                for x in 0..4 {
                    if mb_info.coded[x + y * 4] {
                        add_coeffs(frm.data, yoff + x * 4, frm.stride[0], &mb_info.coeffs[x + y * 4]);
                    }
                }
                yoff += frm.stride[0] * 4;
            }
        } else {
            for y in 0..2 {
                for x in 0..2 {
                    if mb_info.coded[x * 2 + y * 2 * 4] {
                        add_coeffs8(frm.data, yoff + x * 8, frm.stride[0], &mb_info.coeffs8x8[x + y * 2].coeffs);
                    }
                }
                yoff += frm.stride[0] * 8;
            }
        }
    }
    fn add_chroma(frm: &mut NASimpleVideoFrame<u8>, sstate: &SliceState, mb_info: &CurrentMBInfo) {
        for chroma in 1..3 {
            let mut off = frm.offset[chroma] + sstate.mb_x * 8 + sstate.mb_y * 8 * frm.stride[chroma];
            for y in 0..2 {
                for x in 0..2 {
                    let blk_no = 16 + (chroma - 1) * 4 + x + y * 2;
                    if mb_info.coded[blk_no] || mb_info.coeffs[blk_no][0] != 0 {
                        add_coeffs(frm.data, off + x * 4, frm.stride[chroma], &mb_info.coeffs[blk_no]);
                    }
                }
                off += frm.stride[chroma] * 4;
            }
        }
    }
    fn pred_mv(sstate: &mut SliceState, frame_refs: &FrameRefs, mb_info: &mut CurrentMBInfo, cur_id: u16, temporal_mv: bool) {
        let mb_type = mb_info.mb_type;
        if !mb_type.is_4x4() {
            let (pw, ph) = mb_type.size();
            let mut xoff = 0;
            let mut yoff = 0;
            if mb_type == MBType::Direct || mb_type == MBType::BSkip {
                sstate.predict_direct_mb(frame_refs, temporal_mv, cur_id);
            }
            for part in 0..mb_type.num_parts() {
                if !mb_type.is_l1(part) {
                    match mb_type {
                        MBType::PSkip => sstate.predict_pskip(),
                        MBType::BSkip | MBType::Direct => {
                        },
                        _ => {
                            sstate.predict(xoff, yoff, pw, ph, 0,
 mb_info.mv_l0[part], mb_info.ref_l0[part]);
                        },
                    };
                }
                if !mb_type.is_l0(part) && mb_type != MBType::BSkip && mb_type != MBType::Direct {
                    sstate.predict(xoff, yoff, pw, ph, 1, mb_info.mv_l1[part], mb_info.ref_l1[part]);
                }
                if pw != 16 {
                    xoff += pw;
                } else {
                    yoff += ph;
                }
            }
        } else {
            for part in 0..4 {
                let sub_type = mb_info.sub_mb_type[part];
                let mut xoff = (part & 1) * 8;
                let mut yoff = (part & 2) * 4;
                let orig_x = xoff;
                let (pw, ph) = sub_type.size();
                for subpart in 0..sub_type.num_parts() {
                    if sub_type != SubMBType::Direct8x8 {
                        if !sub_type.is_l1() {
                            sstate.predict(xoff, yoff, pw, ph, 0, mb_info.mv_l0[part * 4 + subpart], mb_info.ref_l0[part]);
                        }
                        if !sub_type.is_l0() {
                            sstate.predict(xoff, yoff, pw, ph, 1, mb_info.mv_l1[part * 4 + subpart], mb_info.ref_l1[part]);
                        }
                    } else {
                        for sblk in 0..4 {
                            sstate.predict_direct_sub(frame_refs, temporal_mv, cur_id, (xoff / 4) + (sblk & 1) + (yoff / 4) * 4 + (sblk & 2) * 2);
                        }
                    }
                    xoff += pw;
                    if xoff == orig_x + 8 {
                        xoff -= 8;
                        yoff += ph;
                    }
                }
            }
        }
    }
    #[allow(clippy::cognitive_complexity)]
    fn handle_macroblock(&mut self, mb_info: &mut CurrentMBInfo) {
        let pps = &self.pps[self.cur_pps];

        let qp_y = mb_info.qp_y;
        let qpr = ((qp_y as i8) + pps.chroma_qp_index_offset).max(0).min(51) as usize;
        let qp_u = CHROMA_QUANTS[qpr];
        let qpb = ((qp_y as i8) + pps.second_chroma_qp_index_offset).max(0).min(51) as usize;
        let qp_v = CHROMA_QUANTS[qpb];

        let tx_bypass = qp_y == 0 && self.sps[self.cur_sps].qpprime_y_zero_transform_bypass;

        self.sstate.get_cur_mb().mb_type = mb_info.mb_type.into();
        if mb_info.mb_type != MBType::PCM {
            self.sstate.get_cur_mb().qp_y = qp_y;
            self.sstate.get_cur_mb().qp_u = qp_u;
            self.sstate.get_cur_mb().qp_v = qp_v;
            self.sstate.get_cur_mb().transform_8x8 = mb_info.transform_size_8x8;
        }
        let has_dc = mb_info.mb_type.is_intra16x16() && mb_info.coded[24];
        if has_dc {
            idct_luma_dc(&mut mb_info.coeffs[24], qp_y);
            for i in 0..16 {
                mb_info.coeffs[i][0] = mb_info.coeffs[24][i];
            }
        }
        if !mb_info.transform_size_8x8 {
            let quant_dc = !mb_info.mb_type.is_intra16x16();
            for i in 0..16 {
                if mb_info.coded[i] {
                    if !tx_bypass {
                        idct(&mut mb_info.coeffs[i], qp_y, quant_dc);
                    }
                } else if has_dc {
                    if !tx_bypass {
                        idct_dc(&mut mb_info.coeffs[i], qp_y, quant_dc);
                    }
                    mb_info.coded[i] = true;
                }
            }
        } else {
            for i in 0..4 {
                if mb_info.coded[(i & 1) * 2 + (i & 2) * 4] && !tx_bypass {
                    dequant8x8(&mut mb_info.coeffs8x8[i].coeffs, &pps.scaling_list_8x8[!mb_info.mb_type.is_intra() as usize]);
                    idct8x8(&mut mb_info.coeffs8x8[i].coeffs, qp_y);
                }
            }
        }
        for chroma in 0..2 {
            let qp_c = if chroma == 0 { qp_u } else { qp_v };
            if mb_info.cbpc != 0 {
                chroma_dc_transform(&mut mb_info.chroma_dc[chroma], qp_c);
            }
            for i in 0..4 {
                let blk_no = 16 + chroma * 4 + i;
                mb_info.coeffs[blk_no][0] = mb_info.chroma_dc[chroma][i];
                if mb_info.coded[blk_no] {
                    idct(&mut mb_info.coeffs[blk_no], qp_c, false);
                } else if mb_info.coeffs[blk_no][0] != 0 {
                    idct_dc(&mut mb_info.coeffs[blk_no], qp_c, false);
                    mb_info.coded[blk_no] = true;
                }
            }
        }
        if !pps.entropy_coding_mode || mb_info.mb_type.is_skip() || mb_info.mb_type.is_intra() {
            self.sstate.reset_mb_mv();
        }
        if !mb_info.mb_type.is_intra() {
            Self::pred_mv(&mut self.sstate, &self.frame_refs, mb_info, self.cur_id, self.temporal_mv);
        }
        if !pps.constrained_intra_pred && mb_info.mb_type != MBType::Intra4x4 && mb_info.mb_type != MBType::Intra8x8 {
            self.sstate.fill_ipred(IntraPredMode::DC);
        }

        let xpos = self.sstate.mb_x * 16;
        let ypos = self.sstate.mb_y * 16;
        if let Some(ref mut pic) = self.cur_pic {
            let mut frm = NASimpleVideoFrame::from_video_buf(&mut pic.buf).unwrap();
            match mb_info.mb_type {
                MBType::Intra16x16(_, _, _) => {
                    Self::pred_intra(&mut frm, &self.sstate, &mb_info);
                },
                MBType::Intra4x4 | MBType::Intra8x8 => {
                    Self::pred_intra(&mut frm, &self.sstate, &mb_info);
                },
                MBType::PCM => {},
                MBType::PSkip => {
                    let mv = self.sstate.get_cur_blk4(0).mv[0];
                    let rpic = self.frame_refs.select_ref_pic(0, 0);
                    Self::do_p_mc(&mut frm, xpos, ypos, 16, 16, mv, rpic);
                },
                MBType::P16x16 => {
                    let mv = self.sstate.get_cur_blk4(0).mv[0];
                    let rpic = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[0].index());
                    Self::do_p_mc(&mut frm, xpos, ypos, 16, 16, mv, rpic);
                },
                MBType::P16x8 | MBType::P8x16 => {
                    let (bw, bh, bx, by) = if mb_info.mb_type == MBType::P16x8 {
                            (16, 8, 0, 8)
                        } else {
                            (8, 16, 8, 0)
                        };
                    let mv = self.sstate.get_cur_blk4(0).mv[0];
                    let rpic = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[0].index());
                    Self::do_p_mc(&mut frm, xpos, ypos, bw, bh, mv, rpic);
                    let mv = self.sstate.get_cur_blk4(bx / 4 + by).mv[0];
                    let rpic = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[1].index());
                    Self::do_p_mc(&mut frm, xpos + bx, ypos + by, bw, bh, mv, rpic);
                },
                MBType::P8x8 | MBType::P8x8Ref0 => {
                    for part in 0..4 {
                        let bx = (part & 1) * 8;
                        let by = (part & 2) * 4;
                        if let Some(buf) = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[part].index()) {
                            let mv = self.sstate.get_cur_blk4(bx / 4 + by).mv[0];

                            match mb_info.sub_mb_type[part] {
                                SubMBType::P8x8 => {
                                    do_mc(&mut frm, buf, xpos + bx, ypos + by, 8, 8, mv);
                                },
                                SubMBType::P8x4 => {
                                    do_mc(&mut frm, buf.clone(), xpos + bx, ypos + by, 8, 4, mv);
                                    let mv = self.sstate.get_cur_blk4(bx / 4 + by + 4).mv[0];
                                    do_mc(&mut frm, buf, xpos + bx, ypos + by + 4, 8, 4, mv);
                                },
                                SubMBType::P4x8 => {
                                    do_mc(&mut frm, buf.clone(), xpos + bx, ypos + by, 4, 8, mv);
                                    let mv = self.sstate.get_cur_blk4(bx / 4 + by + 1).mv[0];
                                    do_mc(&mut frm, buf, xpos + bx + 4, ypos + by, 4, 8, mv);
                                },
                                SubMBType::P4x4 => {
                                    for sb_no in 0..4 {
                                        let sxpos = xpos + bx + (sb_no & 1) * 4;
                                        let sypos = ypos + by + (sb_no & 2) * 2;
                                        let sblk_no = (bx / 4 + (sb_no & 1)) + ((by / 4) + (sb_no >> 1)) * 4;
                                        let mv = self.sstate.get_cur_blk4(sblk_no).mv[0];
                                        do_mc(&mut frm, buf.clone(), sxpos, sypos, 4, 4, mv);
                                    }
                                },
                                _ => unreachable!(),
                            };
                        } else {
                            gray_block(&mut frm, xpos + bx, ypos + by, 8, 8);
                        }
                    }
                },
                MBType::B16x16(mode) => {
                    let mv0 = self.sstate.get_cur_blk4(0).mv[0];
                    let rpic0 = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[0].index());
                    let mv1 = self.sstate.get_cur_blk4(0).mv[1];
                    let rpic1 = self.frame_refs.select_ref_pic(1, mb_info.ref_l1[0].index());
                    Self::do_b_mc(&mut frm, mode, xpos, ypos, 16, 16, mv0, rpic0, mv1, rpic1, &mut self.avg_buf);
                },
                MBType::B16x8(mode0, mode1) | MBType::B8x16(mode0, mode1) => {
                    let (pw, ph) = mb_info.mb_type.size();
                    let (px, py) = (pw & 8, ph & 8);
                    let modes = [mode0, mode1];
                    let (mut bx, mut by) = (0, 0);
                    for part in 0..2 {
                        let blk = if part == 0 { 0 } else { (px / 4) + py };
                        let mv0 = self.sstate.get_cur_blk4(blk).mv[0];
                        let rpic0 = self.frame_refs.select_ref_pic(0, mb_info.ref_l0[part].index());
                        let mv1 = self.sstate.get_cur_blk4(blk).mv[1];
                        let rpic1 = self.frame_refs.select_ref_pic(1, mb_info.ref_l1[part].index());
                        Self::do_b_mc(&mut frm, modes[part], xpos + bx, ypos + by, pw, ph, mv0, rpic0, mv1, rpic1, &mut self.avg_buf);
                        bx += px;
                        by += py;
                    }
                },
                MBType::Direct | MBType::BSkip => {
                    let is_16x16 = self.frame_refs.get_colocated_info(self.sstate.mb_x, self.sstate.mb_y).0.mb_type.is_16x16();
                    if is_16x16 || !self.temporal_mv {
                        let mv = self.sstate.get_cur_blk4(0).mv;
                        let ref_idx = self.sstate.get_cur_blk8(0).ref_idx;
                        let rpic0 = self.frame_refs.select_ref_pic(0, ref_idx[0].index());
                        let rpic1 = self.frame_refs.select_ref_pic(1, ref_idx[1].index());
                        Self::do_b_mc(&mut frm, BMode::Bi, xpos, ypos, 16, 16, mv[0], rpic0, mv[1], rpic1, &mut self.avg_buf);
                    } else {
                        for blk4 in 0..16 {
                            let mv = self.sstate.get_cur_blk4(blk4).mv;
                            let ref_idx = self.sstate.get_cur_blk8(blk4_to_blk8(blk4)).ref_idx;
                            let rpic0 = self.frame_refs.select_ref_pic(0, ref_idx[0].index());
                            let rpic1 = self.frame_refs.select_ref_pic(1, ref_idx[1].index());
                            Self::do_b_mc(&mut frm, BMode::Bi, xpos + (blk4 & 3) * 4, ypos + (blk4 >> 2) * 4, 4, 4, mv[0], rpic0, mv[1], rpic1, &mut self.avg_buf);
                        }
                    }
                    self.sstate.apply_to_blk8(|blk8| { blk8.ref_idx[0].set_direct(); blk8.ref_idx[1].set_direct(); });
                },
                MBType::B8x8 => {
                    for part in 0..4 {
                        let ridx = self.sstate.get_cur_blk8(part).ref_idx;
                        let rpic0 = self.frame_refs.select_ref_pic(0, ridx[0].index());
                        let rpic1 = self.frame_refs.select_ref_pic(1, ridx[1].index());
                        let subtype = mb_info.sub_mb_type[part];
                        let blk8 = (part & 1) * 2 + (part & 2) * 4;
                        let mut bx = (part & 1) * 8;
                        let mut by = (part & 2) * 4;
                        match subtype {
                            SubMBType::Direct8x8 => {
                                for blk in 0..4 {
                                    let mv = self.sstate.get_cur_blk4(bx / 4 + (by / 4) * 4).mv;
                                    let ref_idx = self.sstate.get_cur_blk8(bx / 8 + (by / 8) * 2).ref_idx;
                                    let rpic0 = self.frame_refs.select_ref_pic(0, ref_idx[0].index());
                                    let rpic1 = self.frame_refs.select_ref_pic(1, ref_idx[1].index());
                                    Self::do_b_mc(&mut frm, BMode::Bi, xpos + bx, ypos + by, 4, 4, mv[0], rpic0, mv[1], rpic1, &mut self.avg_buf);
                                    bx += 4;
                                    if blk == 1 {
                                        bx -= 8;
                                        by += 4;
                                    }
                                }
                                self.sstate.get_cur_blk8(part).ref_idx[0].set_direct();
                                self.sstate.get_cur_blk8(part).ref_idx[1].set_direct();
                            },
                            SubMBType::B8x8(mode) => {
                                let mv = self.sstate.get_cur_blk4(blk8).mv;
                                Self::do_b_mc(&mut frm, mode, xpos + bx, ypos + by, 8, 8, mv[0], rpic0, mv[1], rpic1, &mut self.avg_buf);
                            },
                            SubMBType::B8x4(mode) | SubMBType::B4x8(mode) => {
                                let (pw, ph) = subtype.size();
                                let mv = self.sstate.get_cur_blk4(blk8).mv;
                                Self::do_b_mc(&mut frm, mode, xpos + bx, ypos + by, pw, ph, mv[0], rpic0.clone(), mv[1], rpic1.clone(), &mut self.avg_buf);
                                let addr2 = blk8 + (pw & 4) / 4 + (ph & 4);
                                let mv = self.sstate.get_cur_blk4(addr2).mv;
                                Self::do_b_mc(&mut frm, mode, xpos + bx + (pw & 4), ypos + by + (ph & 4), pw, ph, mv[0], rpic0, mv[1], rpic1, &mut self.avg_buf);
                            },
                            SubMBType::B4x4(mode) => {
                                for i in 0..4 {
                                    let addr2 = blk8 + (i & 1) + (i & 2) * 2;
                                    let mv = self.sstate.get_cur_blk4(addr2).mv;
                                    Self::do_b_mc(&mut frm, mode, xpos + bx, ypos + by, 4, 4, mv[0], rpic0.clone(), mv[1], rpic1.clone(), &mut self.avg_buf);
                                    bx += 4;
                                    if i == 1 {
                                        bx -= 8;
                                        by += 4;
                                    }
                                }
                            },
                            _ => unreachable!(),
                        };
                    }
                },
            };
            if mb_info.mb_type == MBType::PCM {
                for (dline, src) in frm.data[frm.offset[0] + xpos + ypos * frm.stride[0]..].chunks_mut(frm.stride[0]).take(16).zip(self.ipcm_buf.chunks(16)) {
                    dline[..16].copy_from_slice(src);
                }
                for (dline, src) in frm.data[frm.offset[1] + xpos/2 + ypos/2 * frm.stride[1]..].chunks_mut(frm.stride[1]).take(8).zip(self.ipcm_buf[256..].chunks(8)) {
                    dline[..8].copy_from_slice(src);
                }
                for (dline, src) in frm.data[frm.offset[2] + xpos/2 + ypos/2 * frm.stride[2]..].chunks_mut(frm.stride[2]).take(8).zip(self.ipcm_buf[256 + 64..].chunks(8)) {
                    dline[..8].copy_from_slice(src);
                }
            } else if !mb_info.mb_type.is_skip() {
                if mb_info.mb_type != MBType::Intra4x4 && mb_info.mb_type != MBType::Intra8x8 {
                    Self::add_luma(&mut frm, &self.sstate, &mb_info);
                }
                Self::add_chroma(&mut frm, &self.sstate, &mb_info);
            }
/*match mb_info.mb_type {
MBType::BSkip | MBType::Direct | MBType::B16x16(_) | MBType::B16x8(_, _) | MBType::B8x16(_, _) | MBType::B8x8 => {
 let dstride = frm.stride[0];
 let dst = &mut frm.data[frm.offset[0] + self.sstate.mb_x * 16 + self.sstate.mb_y * 16 * dstride..];
 for el in dst[..16].iter_mut() { *el = 255; }
 for row in dst.chunks_mut(dstride).skip(1).take(15) {
  row[0] = 255;
 }
},
_ => {},
};*/
        }
        if let Some(ref mut pic) = self.cur_pic {
            let mv_info = &mut pic.mv_info;
            let mb_pos = self.sstate.mb_x + self.sstate.mb_y * mv_info.mb_stride;
            let mut mb = FrameMBInfo::new();
            mb.mb_type = mb_info.mb_type.into();
            for blk4 in 0..16 {
                mb.mv[blk4] = self.sstate.get_cur_blk4(blk4).mv;
            }
            for blk8 in 0..4 {
                mb.ref_poc[blk8] = self.frame_refs.map_refs(self.sstate.get_cur_blk8(blk8).ref_idx);
                mb.ref_idx[blk8] = self.sstate.get_cur_blk8(blk8).ref_idx;
            }
            mv_info.mbs[mb_pos] = mb;
        }
        self.sstate.fill_deblock(self.deblock_mode, self.is_s);
        if !self.deblock_skip && self.sstate.mb_x + 1 == self.sstate.mb_w && self.deblock_mode != 1 {
            if let Some(ref mut pic) = self.cur_pic {
                let mut frm = NASimpleVideoFrame::from_video_buf(&mut pic.buf).unwrap();
                loop_filter_row(&mut frm, &self.sstate, self.lf_alpha, self.lf_beta);
            }
        }
        self.sstate.next_mb();
    }
    fn do_p_mc(frm: &mut NASimpleVideoFrame<u8>, xpos: usize, ypos: usize, w: usize, h: usize, mv: MV, ref_pic: Option<NAVideoBufferRef<u8>>) {
        if let Some(buf) = ref_pic {
            do_mc(frm, buf, xpos, ypos, w, h, mv);
        } else {
            gray_block(frm, xpos, ypos, w, h);
        }
    }
    fn do_b_mc(frm: &mut NASimpleVideoFrame<u8>, mode: BMode, xpos: usize, ypos: usize, w: usize, h: usize, mv0: MV, ref_pic0: Option<NAVideoBufferRef<u8>>, mv1: MV, ref_pic1: Option<NAVideoBufferRef<u8>>, avg_buf: &mut NAVideoBufferRef<u8>) {
        match mode {
            BMode::L0 => {
                if let Some(buf) = ref_pic0 {
                    do_mc(frm, buf, xpos, ypos, w, h, mv0);
                } else {
                    gray_block(frm, xpos, ypos, w, h);
                }
            },
            BMode::L1 => {
                if let Some(buf) = ref_pic1 {
                    do_mc(frm, buf, xpos, ypos, w, h, mv1);
                } else {
                    gray_block(frm, xpos, ypos, w, h);
                }
            },
            BMode::Bi => {
                match (ref_pic0, ref_pic1) {
                    (Some(buf0), Some(buf1)) => {
                        do_mc(frm, buf0, xpos, ypos, w, h, mv0);
                        do_mc_avg(frm, buf1, xpos, ypos, w, h, mv1, avg_buf);
                    },
                    (Some(buf0), None) => {
                        do_mc(frm, buf0, xpos, ypos, w, h, mv0);
                    },
                    (None, Some(buf1)) => {
                        do_mc(frm, buf1, xpos, ypos, w, h, mv1);
                    },
                    (None, None) => {
                        gray_block(frm, xpos, ypos, w, h);
                    },
                };
            },
        };
    }
    fn decode_slice_cavlc(&mut self, br: &mut BitReader, slice_hdr: &SliceHeader, full_size: usize) -> DecoderResult<bool> {
        const INTRA_CBP: [u8; 48] = [
            47, 31, 15,  0, 23, 27, 29, 30,  7, 11, 13, 14, 39, 43, 45, 46,
            16,  3,  5, 10, 12, 19, 21, 26, 28, 35, 37, 42, 44,  1,  2,  4,
             8, 17, 18, 20, 24,  6,  9, 22, 25, 32, 33, 34, 36, 40, 38, 41
        ];
        const INTER_CBP: [u8; 48] = [
             0, 16,  1,  2,  4,  8, 32,  3,  5, 10, 12, 15, 47,  7, 11, 13,
            14,  6,  9, 31, 35, 37, 42, 44, 33, 34, 36, 40, 39, 43, 45, 46,
            17, 18, 20, 24, 19, 21, 26, 28, 23, 27, 29, 30, 22, 25, 38, 41
        ];

        let mut mb_idx = slice_hdr.first_mb_in_slice as usize;
        let mut mb_info = CurrentMBInfo::default();
        mb_info.qp_y = slice_hdr.slice_qp;
        let skip_type = if slice_hdr.slice_type.is_p() { MBType::PSkip } else { MBType::BSkip };
        while br.tell() < full_size && mb_idx < self.num_mbs {
            mb_info.coded = [false; 25];
            mb_info.ref_l0 = [ZERO_REF; 4];
            mb_info.ref_l1 = [ZERO_REF; 4];
            mb_info.mv_l0 = [ZERO_MV; 16];
            mb_info.mv_l1 = [ZERO_MV; 16];
            mb_info.chroma_dc = [[0; 4]; 2];
            mb_info.cbpy = 0;
            mb_info.cbpc = 0;

            if !slice_hdr.slice_type.is_intra() {
                let mb_skip_run                     = br.read_ue()? as usize;
                validate!(mb_idx + mb_skip_run <= self.num_mbs);
                mb_info.mb_type = skip_type;
                for _ in 0..mb_skip_run {
                    self.handle_macroblock(&mut mb_info);
                    mb_idx += 1;
                }
                if mb_idx == self.num_mbs || br.tell() >= full_size {
                    break;
                }
            }
            if br.tell() < full_size {
                if self.is_mbaff && ((mb_idx & 1) == 0) {
                    let _mb_field_decoding          = br.read_bool()?;
                }
                let mut mb_type = decode_mb_type_cavlc(br, slice_hdr)?;
                mb_info.mb_type = mb_type;
                mb_info.transform_size_8x8 = false;
                if mb_type == MBType::PCM {
                                                      br.align();
                    for pix in self.ipcm_buf[..256 + 64 + 64].iter_mut() {
                        *pix                        = br.read(8)? as u8;
                    }
                    self.sstate.fill_ncoded(16);
                } else {
                    if self.transform_8x8_mode && mb_type == MBType::Intra4x4 {
                        mb_info.transform_size_8x8  = br.read_bool()?;
                        if mb_info.transform_size_8x8 {
                            mb_type = MBType::Intra8x8;
                            mb_info.mb_type = MBType::Intra8x8;
                        }
                    }
                    decode_mb_pred_cavlc(br, slice_hdr, mb_type, &mut self.sstate, &mut mb_info)?;
                    let (cbpy, cbpc) = if let MBType::Intra16x16(_, cbpy, cbpc) = mb_type {
                            (cbpy, cbpc)
                        } else {
                            let cbp_id              = br.read_ue()? as usize;
                            validate!(cbp_id < INTRA_CBP.len());
                            let cbp = if mb_type == MBType::Intra4x4 || mb_type == MBType::Intra8x8 {
                                    INTRA_CBP[cbp_id]
                                } else {
                                    INTER_CBP[cbp_id]
                                };
                            if self.transform_8x8_mode && (cbp & 0xF) != 0 && mb_info.can_have_8x8_tx(self.sps[self.cur_sps].direct_8x8_inference) {
                                mb_info.transform_size_8x8 = br.read_bool()?;
                            }
                            ((cbp & 0xF), (cbp >> 4))
                        };
                    mb_info.cbpy = cbpy;
                    mb_info.cbpc = cbpc;
                    self.sstate.get_cur_mb().cbp = (cbpc << 4) | cbpy;
                    if cbpy != 0 || cbpc != 0 || mb_type.is_intra16x16() {
                        let mb_qp_delta             = br.read_se()?;
                        validate!(mb_qp_delta >= -26 && mb_qp_delta <= 25);
                        let new_qp = mb_qp_delta + i32::from(mb_info.qp_y);
                        mb_info.qp_y = if new_qp < 0 {
                                (new_qp + 52) as u8
                            } else if new_qp >= 52 {
                                (new_qp - 52) as u8
                            } else {
                                new_qp as u8
                            };
                        mb_info.coeffs = [[0; 16]; 25];
                        if self.transform_8x8_mode {
                            mb_info.clear_coeffs8x8();
                        }
                        mb_info.chroma_dc = [[0; 4]; 2];
                        decode_residual_cavlc(br, &mut self.sstate, &mut mb_info, &self.cavlc_cb)?;
                    }
                }
                self.handle_macroblock(&mut mb_info);
            }
            mb_idx += 1;
        }
        if let Some(ref mut pic) = self.cur_pic {
            pic.cur_mb = mb_idx;
        }
        Ok(mb_idx == self.num_mbs)
    }
    fn decode_slice_cabac(&mut self, cabac: &mut CABAC, slice_hdr: &SliceHeader) -> DecoderResult<bool> {
        let mut mb_idx = slice_hdr.first_mb_in_slice as usize;
        let mut prev_mb_skipped = false;
        let skip_type = if slice_hdr.slice_type.is_p() { MBType::PSkip } else { MBType::BSkip };
        let mut last_qp_diff = false;

        let mut mb_info = CurrentMBInfo::default();
        mb_info.qp_y = slice_hdr.slice_qp;

        while mb_idx < self.num_mbs {
            mb_info.coded = [false; 25];
            mb_info.ref_l0 = [ZERO_REF; 4];
            mb_info.ref_l1 = [ZERO_REF; 4];
            mb_info.mv_l0 = [ZERO_MV; 16];
            mb_info.mv_l1 = [ZERO_MV; 16];
            mb_info.chroma_dc = [[0; 4]; 2];
            mb_info.cbpy = 0;
            mb_info.cbpc = 0;
            let mb_skip = cabac_decode_mbskip(cabac, &self.sstate, slice_hdr);
            if !mb_skip {
                if self.is_mbaff && (((mb_idx & 1) == 0) || (prev_mb_skipped && ((mb_idx & 1) == 1))) {
                    let _mb_field_decoding          = cabac.decode_bit(70);
                }
                let mut mb_type                     = cabac_decode_mb_type(cabac, &slice_hdr, &self.sstate);
                mb_info.mb_type = mb_type;
                mb_info.transform_size_8x8 = false;
                if mb_type == MBType::PCM {
                    let ipcm_size = 256 + 64 + 64;
                    validate!(cabac.pos + ipcm_size <= cabac.src.len());
                    self.ipcm_buf[..ipcm_size].copy_from_slice(&cabac.src[cabac.pos..][..ipcm_size]);
                    cabac.pos += ipcm_size;
                    cabac.reinit()?;
                    last_qp_diff = false;
                } else {
                    if self.transform_8x8_mode && mb_type == MBType::Intra4x4 {
                        let mut ctx = 0;
                        if self.sstate.get_top_mb().transform_8x8 {
                            ctx += 1;
                        }
                        if self.sstate.get_left_mb().transform_8x8 {
                            ctx += 1;
                        }
                        mb_info.transform_size_8x8  = cabac.decode_bit(399 + ctx);
                        if mb_info.transform_size_8x8 {
                            mb_type = MBType::Intra8x8;
                            mb_info.mb_type = MBType::Intra8x8;
                        }
                    }
                    decode_mb_pred_cabac(cabac, slice_hdr, mb_type, &mut self.sstate, &mut mb_info);
                    let (cbpy, cbpc) = if let MBType::Intra16x16(_, cbpy, cbpc) = mb_type {
                            (cbpy, cbpc)
                        } else {
                            decode_cbp_cabac(cabac, &self.sstate)
                        };
                    if self.transform_8x8_mode && cbpy != 0 && mb_info.can_have_8x8_tx(self.sps[self.cur_sps].direct_8x8_inference) {
                        let mut ctx = 0;
                        if self.sstate.get_top_mb().transform_8x8 {
                            ctx += 1;
                        }
                        if self.sstate.get_left_mb().transform_8x8 {
                            ctx += 1;
                        }
                        mb_info.transform_size_8x8  = cabac.decode_bit(399 + ctx);
                    }
                    if mb_type.is_intra() {
                        self.sstate.get_cur_mb().cmode = mb_info.chroma_ipred;
                    }
                    mb_info.cbpy = cbpy;
                    mb_info.cbpc = cbpc;
                    self.sstate.get_cur_mb().cbp = (cbpc << 4) | cbpy;
                    if cbpy != 0 || cbpc != 0 || mb_type.is_intra16x16() {
                        let mb_qp_delta = decode_mb_qp_delta_cabac(cabac, last_qp_diff as usize);
                        validate!(mb_qp_delta >= -26 && mb_qp_delta <= 25);
                        last_qp_diff = mb_qp_delta != 0;
                        let new_qp = mb_qp_delta + i32::from(mb_info.qp_y);
                        mb_info.qp_y = if new_qp < 0 {
                                (new_qp + 52) as u8
                            } else if new_qp >= 52 {
                                (new_qp - 52) as u8
                            } else {
                                new_qp as u8
                            };
                        mb_info.coeffs = [[0; 16]; 25];
                        if self.transform_8x8_mode {
                            mb_info.clear_coeffs8x8();
                        }
                        mb_info.chroma_dc = [[0; 4]; 2];
                        decode_residual_cabac(cabac, &mut self.sstate, &mut mb_info);
                    } else {
                        last_qp_diff = false;
                    }
                }
            } else {
                mb_info.mb_type = skip_type;
                mb_info.transform_size_8x8 = false;
                last_qp_diff = false;
            }
            self.handle_macroblock(&mut mb_info);
            prev_mb_skipped = mb_skip;
            if !(self.is_mbaff && ((mb_idx & 1) == 0)) && cabac.decode_terminate() {
                if let Some(ref mut pic) = self.cur_pic {
                    pic.cur_mb = mb_idx + 1;
                }
                return Ok(mb_idx + 1 == self.num_mbs);
            }
            mb_idx += 1;
        }
        Err(DecoderError::InvalidData)
    }
}

impl NADecoder for H264Decoder {
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let fmt = YUV420_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(0, 0, false, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();

            let edata = info.get_extradata().unwrap();
//print!("edata:"); for &el in edata.iter() { print!(" {:02X}", el); } println!();
            if edata.len() > 11 && &edata[0..4] == b"avcC" {
                let mut mr = MemoryReader::new_read(edata.as_slice());
                let mut br = ByteReader::new(&mut mr);
                let mut nal_buf = Vec::new();

                                          br.read_skip(4)?;
                let version             = br.read_byte()?;
                validate!(version == 1);
                let profile             = br.read_byte()?;
                let _compatibility      = br.read_byte()?;
                let _level              = br.read_byte()?;
                let b                   = br.read_byte()?;
                validate!((b & 0xFC) == 0xFC);
                self.nal_len            = (b & 3) + 1;
                let b                   = br.read_byte()?;
                validate!((b & 0xE0) == 0xE0);
                let num_sps = (b & 0x1F) as usize;
                for _ in 0..num_sps {
                    let len             = br.read_u16be()? as usize;
                    let offset = br.tell() as usize;
                    validate!((br.peek_byte()? & 0x1F) == 7);
                    let _size = unescape_nal(&edata[offset..][..len], &mut nal_buf);
                    self.handle_nal(&nal_buf, supp, true)?;
                                          br.read_skip(len)?;
                }
                let num_pps             = br.read_byte()? as usize;
                for _ in 0..num_pps {
                    let len             = br.read_u16be()? as usize;
                    let offset = br.tell() as usize;
                    validate!((br.peek_byte()? & 0x1F) == 8);
                    let _size = unescape_nal(&edata[offset..][..len], &mut nal_buf);
                    self.handle_nal(&nal_buf, supp, true)?;
                                          br.read_skip(len)?;
                }
                if br.left() > 0 {
                    match profile {
                        100 | 110 | 122 | 144 => {
                            let b       = br.read_byte()?;
                            validate!((b & 0xFC) == 0xFC);
                            // b & 3 -> chroma format
                            let b       = br.read_byte()?;
                            validate!((b & 0xF8) == 0xF8);
                            // b & 7 -> luma depth minus 8
                            let b       = br.read_byte()?;
                            validate!((b & 0xF8) == 0xF8);
                            // b & 7 -> chroma depth minus 8
                            let num_spsext  = br.read_byte()? as usize;
                            for _ in 0..num_spsext {
                                let len = br.read_u16be()? as usize;
                                // parse spsext
                                          br.read_skip(len)?;
                            }
                        },
                        _ => {},
                    };
                }
            } else {
                return Err(DecoderError::NotImplemented);
            }

            self.width  = vinfo.get_width();
            self.height = vinfo.get_height();

            if (self.width == 0 || self.height == 0) && !self.sps.is_empty() {
                self.width  = self.sps[0].pic_width_in_mbs  * 16;
                self.height = self.sps[0].pic_height_in_mbs * 16;
            }

            let num_bufs = if !self.sps.is_empty() {
                    self.sps[0].num_ref_frames as usize + 1
                } else {
                    3
                }.max(16 + 1);
            supp.pool_u8.set_dec_bufs(num_bufs);
            supp.pool_u8.prealloc_video(NAVideoInfo::new(self.width, self.height, false, fmt), 4)?;

            Ok(())
        } else {
println!("???");
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

        let mut mr = MemoryReader::new_read(&src);
        let mut br = ByteReader::new(&mut mr);
        let mut nal_buf = Vec::with_capacity(src.len());
        if self.nal_len > 0 {
            let mut skip_decoding = false;
            if self.skip_mode != FrameSkipMode::None {
                let mut pic_type = FrameType::I;
                let mut is_ref = false;
                while br.left() > 0 {
                    let size = match self.nal_len {
                            1 => br.read_byte()? as usize,
                            2 => br.read_u16be()? as usize,
                            3 => br.read_u24be()? as usize,
                            4 => br.read_u32be()? as usize,
                            _ => unreachable!(),
                        };
                    validate!(br.left() >= (size as i64));
                    let offset = br.tell() as usize;
                    let size = unescape_nal(&src[offset..][..size], &mut nal_buf);
                    validate!(size > 0);
                    let nal_ref_idc   = nal_buf[0] >> 5;
                    let nal_unit_type = nal_buf[0] & 0x1F;
                    if nal_unit_type == 1 || nal_unit_type == 5 {
                        let mut bitr = BitReader::new(&nal_buf[1..], BitReaderMode::BE);
                        let (first_mb, slice_type) = parse_slice_header_minimal(&mut bitr)?;
                        if first_mb == 0 && nal_ref_idc != 0 {
                            is_ref = true;
                        }
                        let new_type = slice_type.to_frame_type();
                        pic_type = match (pic_type, new_type) {
                                         (FrameType::I, _) => new_type,
                                         (_, FrameType::B) => FrameType::B,
                                         _ => pic_type,
                                     };
                    }
                    br.read_skip(size)?;
                }
                match self.skip_mode {
                    FrameSkipMode::IntraOnly => {
                        skip_decoding = pic_type != FrameType::I;
                    },
                    FrameSkipMode::KeyframesOnly => {
                        if !is_ref {
                            skip_decoding = true;
                        }
                    },
                    _ => {},
                };
                br.seek(SeekFrom::Start(0))?;
            }
            while br.left() > 0 {
                let size = match self.nal_len {
                        1 => br.read_byte()? as usize,
                        2 => br.read_u16be()? as usize,
                        3 => br.read_u24be()? as usize,
                        4 => br.read_u32be()? as usize,
                        _ => unreachable!(),
                    };
                validate!(br.left() >= (size as i64));
                let offset = br.tell() as usize;
                let _size = unescape_nal(&src[offset..][..size], &mut nal_buf);
                self.handle_nal(nal_buf.as_slice(), supp, skip_decoding)?;
                br.read_skip(size)?;
            }
        } else {
//todo NAL detection
            unimplemented!();
        }

        let (bufinfo, ftype, dts) = if self.has_pic && self.cur_pic.is_some() {
                let mut npic = None;
                std::mem::swap(&mut self.cur_pic, &mut npic);
                let cpic = npic.unwrap();
                let ret = (NABufferType::Video(cpic.buf.clone()), cpic.pic_type, Some(u64::from(cpic.full_id)));
                if cpic.is_ref {
                    self.frame_refs.add_short_term(cpic.clone(), self.sps[self.cur_sps].num_ref_frames);
                }
                if let Some(lt_idx) = cpic.long_term {
                    self.frame_refs.add_long_term(lt_idx, cpic);
                }
                ret
            } else {
                (NABufferType::None, FrameType::Skip, None)
            };

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(ftype == FrameType::I);
        if let (Some(mydts), None) = (dts, frm.get_dts()) {
            frm.set_dts(Some(mydts));
        }
        if let Some(dts) = dts {
            frm.set_id(dts as i64);
        }
        frm.set_frame_type(ftype);
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
    }
}

const DEBLOCK_SKIP_OPTION: &str = "skip_deblock";

const DECODER_OPTIONS: &[NAOptionDefinition] = &[
    NAOptionDefinition {
        name: FRAME_SKIP_OPTION, description: FRAME_SKIP_OPTION_DESC,
        opt_type: NAOptionDefinitionType::Bool },
    NAOptionDefinition {
        name: DEBLOCK_SKIP_OPTION, description: "Loop filter skipping mode",
        opt_type: NAOptionDefinitionType::String(Some(&[
                FRAME_SKIP_OPTION_VAL_NONE,
                FRAME_SKIP_OPTION_VAL_KEYFRAME,
                FRAME_SKIP_OPTION_VAL_INTRA
            ])) },
];

impl NAOptionHandler for H264Decoder {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { DECODER_OPTIONS }
    fn set_options(&mut self, options: &[NAOption]) {
        for option in options.iter() {
            for opt_def in DECODER_OPTIONS.iter() {
                if opt_def.check(option).is_ok() {
                    match (option.name, &option.value) {
                        (FRAME_SKIP_OPTION, NAValue::String(ref str)) => {
                            if let Ok(smode) = FrameSkipMode::from_str(str) {
                                self.skip_mode = smode;
                            }
                        },
                        (DEBLOCK_SKIP_OPTION, NAValue::Bool(val)) => {
                            self.deblock_skip = *val;
                        },
                        _ => {},
                    }
                }
            }
        }
    }
    fn query_option_value(&self, name: &str) -> Option<NAValue> {
        match name {
            FRAME_SKIP_OPTION => Some(NAValue::String(self.skip_mode.to_string())),
            DEBLOCK_SKIP_OPTION => Some(NAValue::Bool(self.deblock_skip)),
            _ => None,
        }
    }
}

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(H264Decoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::itu_register_all_decoders;
    use nihav_commonfmt::generic_register_all_demuxers;

    mod raw_demux;
    mod conformance;
    use self::raw_demux::RawH264DemuxerCreator;

    #[test]
    fn test_h264_perframe() {
        let mut dmx_reg = RegisteredDemuxers::new();
        dmx_reg.add_demuxer(&RawH264DemuxerCreator{});
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        itu_register_all_decoders(&mut dec_reg);

        test_decoding("rawh264", "h264",
                      "assets/ITU/h264-conformance/CABAST3_Sony_E.jsv",
                      None, &dmx_reg, &dec_reg, ExpectedTestResult::MD5Frames(vec![
                        [0x85fc4b44, 0xc9aefdc9, 0x568d0592, 0x2eccf9a0],
                        [0xbd8d11bc, 0x97acf592, 0x45a3cdbb, 0xa254a882],
                        [0xbda0e0b9, 0x9fbe1974, 0x1540b244, 0x46a050ca],
                        [0x471f0057, 0x125ef3b4, 0x4a87515f, 0xba254bbb],
                        [0x466a7df2, 0xb392c2a4, 0xed66b68b, 0xfdaad2da],
                        [0x96334b41, 0x41bac7ef, 0xe87154f1, 0xa5fc3551],
                        [0x0fd4e9b8, 0x4269bbec, 0x00a1978f, 0xe6224851],
                        [0x68be82af, 0x856615a7, 0x387a253d, 0x8473e6b9],
                        [0xc4bed119, 0x14ba7fe0, 0x447cb680, 0x555da4c5],
                        [0x85d127d6, 0x04b85928, 0x26740281, 0x4d848db5],
                        [0xe44fe461, 0x0d0b64ce, 0xf191179b, 0xabdab686],
                        [0x347c8edb, 0x847ad11f, 0x8f16b84e, 0xdc915d75],
                        [0xeb1364a6, 0x91c9d99d, 0x324f5427, 0xcc9f11a2],
                        [0x7aeb5a3f, 0xebc9c4dd, 0x8f12c8e4, 0x37a2db97],
                        [0xa11e5c33, 0x656df4c0, 0x1e8b98d8, 0x1736722f],
                        [0x239f2ef2, 0xe32b0603, 0x448366bb, 0x9331051c],
                        [0x1815a1b1, 0xfb7e7cf0, 0xd5c7dd5b, 0x0135a8fb],
                        [0xea3b85dd, 0xa96e7015, 0xa91c576d, 0x5c127ca1],
                        [0x1c49148f, 0x6d9e7045, 0x093f0b7c, 0x42c2ebaa],
                        [0x4b4c2863, 0x95709d8c, 0xeb72e251, 0x096632dc],
                        [0x727418e5, 0x2c015383, 0x59580212, 0x0302dd99],
                        [0xbe57dfa4, 0xf2aa7d70, 0xa068ee62, 0x77372861],
                        [0x2faef43a, 0x73da6654, 0xb9d9c22e, 0xc59520bc],
                        [0x138cff40, 0x3e6c108a, 0xa981e654, 0x903da85b],
                        [0xa90454f5, 0x7875d5db, 0xbab234bd, 0xe6ce1193]]));
    }

    #[test]
    fn test_h264_real1() {
        let mut dmx_reg = RegisteredDemuxers::new();
        dmx_reg.add_demuxer(&RawH264DemuxerCreator{});
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        itu_register_all_decoders(&mut dec_reg);

        test_decoding("mov", "h264", "assets/ITU/1.mp4",
                      Some(60), &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5Frames(vec![
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0x9dbac04a, 0xc49ca8c1, 0x09bb9182, 0xc7928970],
                            [0xf1c88c12, 0x7da871f5, 0xdaf3153f, 0x66e72d72],
                            [0x3d4765f1, 0x8ac472f6, 0x7ffd13a6, 0xc7a45dae],
                            [0x60e5e13a, 0xd2d7f239, 0x1a793d71, 0x19f8c190],
                            [0xdd80c3e4, 0xb1500149, 0x43925280, 0x9e5f3230],
                            [0x2adf6e64, 0x39012d45, 0x7a776cb5, 0x3df76e84],
                            [0x44319007, 0xbc837dd2, 0x486b2703, 0x451d0651],
                            [0x922386ef, 0xaf101e9b, 0xf2094a40, 0xc8c454c0],
                            [0x0d81e398, 0x04192a56, 0xa31f39d0, 0x5e0a2deb],
                            [0xcdd144b3, 0xd1c7743e, 0x5753b0f4, 0xc070efa9],
                            [0xe1c67e39, 0x6065ddaf, 0x576bf9f1, 0x8e6825aa],
                            [0xaf817b0d, 0xdc6c345a, 0xf7f289c7, 0x6cc482d8],
                            [0x81dc4bcb, 0xee4fc5e5, 0x9b87c7ef, 0xdd0fb034],
                            [0x8b07f523, 0x4168799f, 0x990cb270, 0x858b9f2c],
                            [0x31855a80, 0xadbcc562, 0xe60628be, 0x5f04be09],
                            [0x9c614acc, 0xbd08a3f4, 0x91b02c45, 0x41899a83],
                            [0x89d09064, 0xbff810a3, 0x9c674179, 0x305225a6],
                            [0xba1fc8d3, 0x15d34fae, 0x565d363b, 0x4f4d0604],
                            [0x1cb15a1b, 0xa0be111e, 0x45cc801f, 0x01a2c691],
                            [0xd898be48, 0xd19bf58d, 0xe22fe44f, 0x6a2914fb],
                            [0xec6712af, 0xa13b55c0, 0x2915a746, 0xb29a5c48],
                            [0x745798fa, 0x4ef0f882, 0x59335c08, 0xb1d9dbb4],
                            [0x4045b495, 0xdb3d969c, 0x1f0d9220, 0x5a34067b],
                            [0x94fee093, 0x78ad89b3, 0xf20e882b, 0x941425db],
                            [0xc3eb1a78, 0x4b4e098a, 0xcbcf9bb4, 0xfd5b5426]]));
    }
    #[test]
    fn test_h264_real2() {
        let mut dmx_reg = RegisteredDemuxers::new();
        dmx_reg.add_demuxer(&RawH264DemuxerCreator{});
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        itu_register_all_decoders(&mut dec_reg);
        test_decoding("mov", "h264", "assets/ITU/DimpledSpanishCuckoo-mobile.mp4",
                      Some(10), &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5Frames(vec![
                            [0x1addcb8e, 0xde58b857, 0x17222c32, 0x75455fa8],
                            [0xae63141a, 0x79435b2e, 0xfe606c48, 0xf676da66],
                            [0xfdb80404, 0x6a288e23, 0x45cc4106, 0xdd5eb57c],
                            [0xd603a3ff, 0x872dcb9b, 0x43f7a71c, 0x2ad4eecc],
                            [0x639ed6a5, 0xbb1cfec6, 0x0ee5443a, 0x1694772a],
                            [0xf8ef3f48, 0x152de238, 0xb1995f9a, 0xf82ad1d5],
                            [0x604f6265, 0xb9d82f56, 0x21f00cf4, 0xc69c18a7],
                            [0xd932c16e, 0x25cbf060, 0xcb66543b, 0xfe8a5019],
                            [0xf2a3dac0, 0x0f4678dd, 0xf64c8228, 0x47f14676],
                            [0x267041ee, 0x3b6b8b64, 0x8bfe1697, 0x1fba508b],
                            [0x9f917e72, 0x75d882a9, 0xa5e3e684, 0x4ed87eff]]));
    }
}

pub const I4X4_SCAN: [(u8, u8); 16] = [
    (0,0), (1,0), (0,1), (1,1), (2,0), (3,0), (2,1), (3,1),
    (0,2), (1,2), (0,3), (1,3), (2,2), (3,2), (2,3), (3,3)
];
