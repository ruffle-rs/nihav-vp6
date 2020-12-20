use nihav_core::io::bitreader::*;
use nihav_core::io::codebook::*;
use nihav_core::formats;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_codec_support::codecs::{MV, ZIGZAG};
use nihav_codec_support::codecs::blockdsp;
use nihav_codec_support::codecs::h263::*;
use nihav_codec_support::codecs::h263::code::*;
use nihav_codec_support::codecs::h263::decoder::*;
use nihav_codec_support::codecs::h263::data::*;

#[allow(dead_code)]
struct Tables {
    intra_mcbpc_cb: Codebook<u8>,
    inter_mcbpc_cb: Codebook<u8>,
    cbpy_cb:        Codebook<u8>,
    rl_cb:          Codebook<H263RLSym>,
    aic_rl_cb:      Codebook<H263RLSym>,
    mv_cb:          Codebook<u8>,
}

struct VivoBlockDSP {}

impl VivoBlockDSP { fn new() -> Self { Self {} } }

#[allow(clippy::erasing_op)]
#[allow(clippy::identity_op)]
fn deblock_hor(buf: &mut [u8], off: usize, stride: usize, clip_tab: &[i16; 64]) {
    for x in 0..8 {
        let p1 = i16::from(buf[off - 2 * stride + x]);
        let p0 = i16::from(buf[off - 1 * stride + x]);
        let q0 = i16::from(buf[off + 0 * stride + x]);
        let q1 = i16::from(buf[off + 1 * stride + x]);
        let diff = (3 * (p1 - q1) + 8 * (q0 - p0)) >> 4;
        if (diff != 0) && (diff > -32) && (diff < 32) {
            let delta = clip_tab[(diff + 32) as usize];
            buf[off - 1 * stride + x] = (p0 + delta).max(0).min(255) as u8;
            buf[off + 0 * stride + x] = (q0 - delta).max(0).min(255) as u8;
        }
    }
}

#[allow(clippy::identity_op)]
fn deblock_ver(buf: &mut [u8], off: usize, stride: usize, clip_tab: &[i16; 64]) {
    for y in 0..8 {
        let p1 = i16::from(buf[off - 2 + y * stride]);
        let p0 = i16::from(buf[off - 1 + y * stride]);
        let q0 = i16::from(buf[off + 0 + y * stride]);
        let q1 = i16::from(buf[off + 1 + y * stride]);
        let diff = (3 * (p1 - q1) + 8 * (q0 - p0)) >> 4;
        if (diff != 0) && (diff > -32) && (diff < 32) {
            let delta = clip_tab[(diff + 32) as usize];
            buf[off - 1 + y * stride] = (p0 + delta).max(0).min(255) as u8;
            buf[off     + y * stride] = (q0 - delta).max(0).min(255) as u8;
        }
    }
}

fn gen_clip_tab(clip_tab: &mut [i16; 64], q: u8) {
    let q = i16::from(q);
    *clip_tab = [0; 64];
    let lim = (q + 2) >> 1;
    for i in 0..lim {
        clip_tab[(32 - i) as usize] = -i;
        clip_tab[(32 + i) as usize] =  i;
    }
    for i in lim..q {
        let val = q - i; 
        clip_tab[(32 - i) as usize] = -val;
        clip_tab[(32 + i) as usize] =  val;
    }
}

impl BlockDSP for VivoBlockDSP {
    fn idct(&self, blk: &mut [i16; 64]) {
        h263_annex_w_idct(blk);
    }
    fn copy_blocks(&self, dst: &mut NAVideoBuffer<u8>, src: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, mv: MV) {
        let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;
        let cmode = (if (mv.x & 3) != 0 { 1 } else { 0 }) + (if (mv.y & 3) != 0 { 2 } else { 0 });

        let mut dst = NASimpleVideoFrame::from_video_buf(dst).unwrap();

        blockdsp::copy_block(&mut dst, src.clone(), 0, xpos, ypos, mv.x >> 1, mv.y >> 1, 16, 16, 0, 1, mode, H263_INTERP_FUNCS);
        blockdsp::copy_block(&mut dst, src.clone(), 1, xpos >> 1, ypos >> 1, mv.x >> 2, mv.y >> 2, 8, 8, 0, 1, cmode, H263_INTERP_FUNCS);
        blockdsp::copy_block(&mut dst, src.clone(), 2, xpos >> 1, ypos >> 1, mv.x >> 2, mv.y >> 2, 8, 8, 0, 1, cmode, H263_INTERP_FUNCS);
    }
    fn copy_blocks8x8(&self, dst: &mut NAVideoBuffer<u8>, src: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, mvs: &[MV; 4]) {
        let mut dst = NASimpleVideoFrame::from_video_buf(dst).unwrap();

        for i in 0..4 {
            let xadd = (i & 1) * 8;
            let yadd = (i & 2) * 4;
            let mode = ((mvs[i].x & 1) + (mvs[i].y & 1) * 2) as usize;

            blockdsp::copy_block(&mut dst, src.clone(), 0, xpos + xadd, ypos + yadd, mvs[i].x >> 1, mvs[i].y >> 1, 8, 8, 0, 1, mode, H263_INTERP_FUNCS);
        }

        let sum_mv = mvs[0] + mvs[1] + mvs[2] + mvs[3];
        let cmx = (sum_mv.x >> 3) + H263_CHROMA_ROUND[(sum_mv.x & 0xF) as usize];
        let cmy = (sum_mv.y >> 3) + H263_CHROMA_ROUND[(sum_mv.y & 0xF) as usize];
        let mode = ((cmx & 1) + (cmy & 1) * 2) as usize;
        for plane in 1..3 {
            blockdsp::copy_block(&mut dst, src.clone(), plane, xpos >> 1, ypos >> 1, cmx >> 1, cmy >> 1, 8, 8, 0, 1, mode, H263_INTERP_FUNCS);
        }
    }
    fn avg_blocks(&self, dst: &mut NAVideoBuffer<u8>, src: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, mv: MV) {
        let mode = ((mv.x & 1) + (mv.y & 1) * 2) as usize;
        let cmode = (if (mv.x & 3) != 0 { 1 } else { 0 }) + (if (mv.y & 3) != 0 { 2 } else { 0 });

        let mut dst = NASimpleVideoFrame::from_video_buf(dst).unwrap();

        blockdsp::copy_block(&mut dst, src.clone(), 0, xpos, ypos, mv.x >> 1, mv.y >> 1, 16, 16, 0, 1, mode, H263_INTERP_AVG_FUNCS);
        blockdsp::copy_block(&mut dst, src.clone(), 1, xpos >> 1, ypos >> 1, mv.x >> 2, mv.y >> 2, 8, 8, 0, 1, cmode, H263_INTERP_AVG_FUNCS);
        blockdsp::copy_block(&mut dst, src.clone(), 2, xpos >> 1, ypos >> 1, mv.x >> 2, mv.y >> 2, 8, 8, 0, 1, cmode, H263_INTERP_AVG_FUNCS);
    }
    fn avg_blocks8x8(&self, dst: &mut NAVideoBuffer<u8>, src: NAVideoBufferRef<u8>, xpos: usize, ypos: usize, mvs: &[MV; 4]) {
        let mut dst = NASimpleVideoFrame::from_video_buf(dst).unwrap();

        for i in 0..4 {
            let xadd = (i & 1) * 8;
            let yadd = (i & 2) * 4;
            let mode = ((mvs[i].x & 1) + (mvs[i].y & 1) * 2) as usize;

            blockdsp::copy_block(&mut dst, src.clone(), 0, xpos + xadd, ypos + yadd, mvs[i].x >> 1, mvs[i].y >> 1, 8, 8, 0, 1, mode, H263_INTERP_AVG_FUNCS);
        }

        let sum_mv = mvs[0] + mvs[1] + mvs[2] + mvs[3];
        let cmx = (sum_mv.x >> 3) + H263_CHROMA_ROUND[(sum_mv.x & 0xF) as usize];
        let cmy = (sum_mv.y >> 3) + H263_CHROMA_ROUND[(sum_mv.y & 0xF) as usize];
        let mode = ((cmx & 1) + (cmy & 1) * 2) as usize;
        for plane in 1..3 {
            blockdsp::copy_block(&mut dst, src.clone(), plane, xpos >> 1, ypos >> 1, cmx >> 1, cmy >> 1, 8, 8, 0, 1, mode, H263_INTERP_AVG_FUNCS);
        }
    }
    fn filter_row(&self, buf: &mut NAVideoBuffer<u8>, mb_y: usize, mb_w: usize, cbpi: &CBPInfo) {
        let ystride = buf.get_stride(0);
        let ustride = buf.get_stride(1);
        let vstride = buf.get_stride(2);
        let yoff = buf.get_offset(0) + mb_y * 16 * ystride;
        let uoff = buf.get_offset(1) + mb_y * 8 * ustride;
        let voff = buf.get_offset(2) + mb_y * 8 * vstride;
        let buf = buf.get_data_mut().unwrap();

        let mut clip_tab = [0i16; 64];
        let mut last_q = 0;
        let mut off = yoff;
        for mb_x in 0..mb_w {
            let coff = off;
            let coded0 = cbpi.is_coded(mb_x, 0);
            let coded1 = cbpi.is_coded(mb_x, 1);
            let q = cbpi.get_q(mb_w + mb_x);
            if q != last_q {
                gen_clip_tab(&mut clip_tab, q);
                last_q = q;
            }
            if mb_y != 0 {
                if coded0 && cbpi.is_coded_top(mb_x, 0) {
                    deblock_hor(buf, ystride, coff, &clip_tab);
                }
                if coded1 && cbpi.is_coded_top(mb_x, 1) {
                    deblock_hor(buf, ystride, coff + 8, &clip_tab);
                }
            }
            let coff = off + 8 * ystride;
            if cbpi.is_coded(mb_x, 2) && coded0 {
                deblock_hor(buf, ystride, coff, &clip_tab);
            }
            if cbpi.is_coded(mb_x, 3) && coded1 {
                deblock_hor(buf, ystride, coff + 8, &clip_tab);
            }
            off += 16;
        }
        let mut leftt = false;
        let mut leftc = false;
        let mut off = yoff;
        for mb_x in 0..mb_w {
            let ctop0 = cbpi.is_coded_top(mb_x, 0);
            let ctop1 = cbpi.is_coded_top(mb_x, 0);
            let ccur0 = cbpi.is_coded(mb_x, 0);
            let ccur1 = cbpi.is_coded(mb_x, 1);
            let q = cbpi.get_q(mb_w + mb_x);
            if q != last_q {
                gen_clip_tab(&mut clip_tab, q);
                last_q = q;
            }
            if mb_y != 0 {
                let coff = off - 8 * ystride;
                let qtop = cbpi.get_q(mb_x);
                if qtop != last_q {
                    gen_clip_tab(&mut clip_tab, qtop);
                    last_q = qtop;
                }
                if leftt && ctop0 {
                    deblock_ver(buf, ystride, coff, &clip_tab);
                }
                if ctop0 && ctop1 {
                    deblock_ver(buf, ystride, coff + 8, &clip_tab);
                }
            }
            if leftc && ccur0 {
                deblock_ver(buf, ystride, off, &clip_tab);
            }
            if ccur0 && ccur1 {
                deblock_ver(buf, ystride, off + 8, &clip_tab);
            }
            leftt = ctop1;
            leftc = ccur1;
            off += 16;
        }
        if mb_y != 0 {
            for mb_x in 0..mb_w {
                let ctu = cbpi.is_coded_top(mb_x, 4);
                let ccu = cbpi.is_coded(mb_x, 4);
                let ctv = cbpi.is_coded_top(mb_x, 5);
                let ccv = cbpi.is_coded(mb_x, 5);
                let q = cbpi.get_q(mb_w + mb_x);
                if q != last_q {
                    gen_clip_tab(&mut clip_tab, q);
                    last_q = q;
                }
                if ctu && ccu {
                    deblock_hor(buf, ustride, uoff + mb_x * 8, &clip_tab);
                }
                if ctv && ccv {
                    deblock_hor(buf, vstride, voff + mb_x * 8, &clip_tab);
                }
            }
            let mut leftu = false;
            let mut leftv = false;
            let offu = uoff - 8 * ustride;
            let offv = voff - 8 * vstride;
            for mb_x in 0..mb_w {
                let ctu = cbpi.is_coded_top(mb_x, 4);
                let ctv = cbpi.is_coded_top(mb_x, 5);
                let qt = cbpi.get_q(mb_x);
                if qt != last_q {
                    gen_clip_tab(&mut clip_tab, qt);
                    last_q = qt;
                }
                if leftu && ctu {
                    deblock_ver(buf, ustride, offu + mb_x * 8, &clip_tab);
                }
                if leftv && ctv {
                    deblock_ver(buf, vstride, offv + mb_x * 8, &clip_tab);
                }
                leftu = ctu;
                leftv = ctv;
            }
        }
    }
}

struct VivoDecoder {
    info:       NACodecInfoRef,
    dec:        H263BaseDecoder,
    tables:     Tables,
    bdsp:       VivoBlockDSP,
    lastframe:  Option<NABufferType>,
    lastpts:    Option<u64>,
    width:      usize,
    height:     usize,
}

struct VivoBR<'a> {
    br:     BitReader<'a>,
    tables: &'a Tables,
    gob_no: usize,
    mb_w:   usize,
    is_pb:  bool,
    is_ipb: bool,
    ref_w:  usize,
    ref_h:  usize,
    aic:    bool,
}

fn check_marker<'a>(br: &mut BitReader<'a>) -> DecoderResult<()> {
    let mark = br.read(1)?;
    validate!(mark == 1);
    Ok(())
}

impl<'a> VivoBR<'a> {
    fn new(src: &'a [u8], tables: &'a Tables, ref_w: usize, ref_h: usize) -> Self {
        VivoBR {
            br:     BitReader::new(src, BitReaderMode::BE),
            tables,
            gob_no: 0,
            mb_w:   0,
            is_pb:  false,
            is_ipb: false,
            ref_w, ref_h,
            aic:    false,
        }
    }

    fn decode_block(&mut self, quant: u8, intra: bool, coded: bool, blk: &mut [i16; 64], _plane_no: usize, acpred: ACPredMode) -> DecoderResult<()> {
        let br = &mut self.br;
        let mut idx = 0;
        if !self.aic && intra {
            let mut dc = br.read(8)? as i16;
            if dc == 255 { dc = 128; }
            blk[0] = dc << 3;
            idx = 1;
        }
        if !coded { return Ok(()); }
        let scan = match acpred {
                    ACPredMode::Hor => H263_SCAN_V,
                    ACPredMode::Ver => H263_SCAN_H,
                    _               => &ZIGZAG,
                };

        let rl_cb = if self.aic && intra { &self.tables.aic_rl_cb } else { &self.tables.rl_cb };
        let q = i16::from(quant * 2);
        let q_add = if q == 0 || self.aic { 0i16 } else { (((q >> 1) - 1) | 1) as i16 };
        while idx < 64 {
            let code = br.read_cb(rl_cb)?;
            let run;
            let mut level;
            let last;
            if !code.is_escape() {
                run   = code.get_run();
                level = code.get_level();
                last  = code.is_last();
                if br.read_bool()? { level = -level; }
                if !intra || idx != 0 {
                    if level >= 0 {
                        level = (level * q) + q_add;
                    } else {
                        level = (level * q) - q_add;
                    }
                }
            } else {
                last  = br.read_bool()?;
                run   = br.read(6)? as u8;
                level = br.read_s(8)? as i16;
                if level == -128 {
                    let low = br.read(5)? as i16;
                    let top = br.read_s(6)? as i16;
                    level = (top << 5) | low;
                }
                if !intra || idx != 0 {
                    if level >= 0 {
                        level = (level * q) + q_add;
                    } else {
                        level = (level * q) - q_add;
                    }
                    if level < -2048 { level = -2048; }
                    if level >  2047 { level =  2047; }
                }
            }
            idx += run;
            validate!(idx < 64);
            let oidx = scan[idx as usize];
            blk[oidx] = level;
            idx += 1;
            if last { break; }
        }
        Ok(())
    }
}

fn decode_mv_component(br: &mut BitReader, mv_cb: &Codebook<u8>) -> DecoderResult<i16> {
    let code = i16::from(br.read_cb(mv_cb)?);
    if code == 0 { return Ok(0) }
    if !br.read_bool()? {
        Ok(code)
    } else {
        Ok(-code)
    }
}

fn decode_mv(br: &mut BitReader, mv_cb: &Codebook<u8>) -> DecoderResult<MV> {
    let xval = decode_mv_component(br, mv_cb)?;
    let yval = decode_mv_component(br, mv_cb)?;
    Ok(MV::new(xval, yval))
}

fn decode_b_info(br: &mut BitReader, is_pb: bool, is_ipb: bool, is_intra: bool) -> DecoderResult<BBlockInfo> {
    if is_pb { // as improved pb
        if is_ipb {
            let pb_mv_add = if is_intra { 1 } else { 0 };
            if br.read_bool()?{
                if br.read_bool()? {
                    let pb_mv_count = 1 - (br.read(1)? as usize);
                    let cbpb = br.read(6)? as u8;
                    Ok(BBlockInfo::new(true, cbpb, pb_mv_count + pb_mv_add, pb_mv_count == 1))
                } else {
                    Ok(BBlockInfo::new(true, 0, 1 + pb_mv_add, true))
                }
            } else {
                Ok(BBlockInfo::new(true, 0, pb_mv_add, false))
            }
        } else {
            let mvdb = br.read_bool()?;
            let cbpb = if mvdb && br.read_bool()? { br.read(6)? as u8 } else { 0 };
            Ok(BBlockInfo::new(true, cbpb, if mvdb { 1 } else { 0 }, false))
        }
    } else {
        Ok(BBlockInfo::new(false, 0, 0, false))
    }
}

impl<'a> BlockDecoder for VivoBR<'a> {

#[allow(unused_variables)]
#[allow(clippy::unreadable_literal)]
    fn decode_pichdr(&mut self) -> DecoderResult<PicInfo> {
        let br = &mut self.br;
        let syncw = br.read(22)?;
        validate!(syncw == 0x000020);
        let tr = (br.read(8)? << 8) as u16;
        check_marker(br)?;
        let id = br.read(1)?;
        validate!(id == 0);
        br.read(1)?; // split screen indicator
        br.read(1)?; // document camera indicator
        br.read(1)?; // freeze picture release
        let mut sfmt = br.read(3)?;
        validate!(sfmt != 0b000);
        let is_intra = !br.read_bool()?;
        let umv = br.read_bool()?;
        br.read(1)?; // syntax arithmetic coding
        let apm = br.read_bool()?;
        self.is_pb = br.read_bool()?;
        let deblock;
        let pbplus;
        let aic;
        if sfmt == 0b110 {
            sfmt = br.read(3)?;
            validate!(sfmt != 0b000 && sfmt != 0b110);
            aic = br.read_bool()?;
            br.read(1)?; // umv mode
            deblock = br.read_bool()?;
            br.read(3)?; // unknown flags
            pbplus = br.read_bool()?;
            br.read(4)?; // unknown flags
        } else {
            aic = false;
            deblock = false;
            pbplus = false;
        }
        self.is_ipb = pbplus;
        let (w, h) = match sfmt {
                0b001 => ( 64,  48),
                0b011 => ( 88,  72),
                0b010 => (176, 144),
                0b100 => (352, 288),
                0b101 => (704, 576),
                0b111 => {
                    validate!((self.ref_w != 0) && (self.ref_h != 0));
                    ((self.ref_w + 15) & !15, (self.ref_h + 15) & !15)
                },
                _ => return Err(DecoderError::InvalidData),
            };
        let quant = br.read(5)?;
        let cpm = br.read_bool()?;
        validate!(!cpm);

        let pbinfo;
        if self.is_pb {
            let trb = br.read(3)?;
            let dbquant = br.read(2)?;
            pbinfo = Some(PBInfo::new(trb as u8, dbquant as u8, pbplus));
        } else {
            pbinfo = None;
        }
        while br.read_bool()? { // skip PEI
            br.read(8)?;
        }
        self.gob_no = 0;
        self.mb_w = (w + 15) >> 4;
        self.aic = aic;

        let ftype = if is_intra { Type::I } else { Type::P };
        let plusinfo = Some(PlusInfo::new(aic, deblock, false, false));
        let mvmode = if umv { MVMode::UMV } else { MVMode::Old };
        let picinfo = PicInfo::new(w, h, ftype, mvmode, umv, apm, quant as u8, tr, pbinfo, plusinfo);
        Ok(picinfo)
    }

    #[allow(unused_variables)]
    fn decode_slice_header(&mut self, info: &PicInfo) -> DecoderResult<SliceInfo> {
        let br = &mut self.br;
        let gbsc = br.read(17)?;
        validate!(gbsc == 1);
        let gn = br.read(5)?;
        let gfid = br.read(2)?;
        let gquant = br.read(5)?;
        let ret = SliceInfo::new_gob(0, self.gob_no, gquant as u8);
        self.gob_no += 1;
        Ok(ret)
    }

    #[allow(unused_variables)]
    fn decode_block_header(&mut self, info: &PicInfo, slice: &SliceInfo, sstate: &SliceState) -> DecoderResult<BlockInfo> {
        let br = &mut self.br;
        let mut q = slice.get_quant();
        match info.get_mode() {
            Type::I => {
                    let mut cbpc = br.read_cb(&self.tables.intra_mcbpc_cb)?;
                    while cbpc == 8 { cbpc = br.read_cb(&self.tables.intra_mcbpc_cb)?; }
                    let mut acpred = ACPredMode::None;
                    if let Some(ref pi) = info.plusinfo {
                        if pi.aic {
                            let acpp = br.read_bool()?;
                            acpred = ACPredMode::DC;
                            if acpp {
                                acpred = if !br.read_bool()? { ACPredMode::Hor } else { ACPredMode::Ver };
                            }
                        }
                    }
                    let cbpy = br.read_cb(&self.tables.cbpy_cb)?;
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    let dquant = (cbpc & 4) != 0;
                    if dquant {
                        let idx = br.read(2)? as usize;
                        q = (i16::from(q) + i16::from(H263_DQUANT_TAB[idx])) as u8;
                    }
                    let mut binfo = BlockInfo::new(Type::I, cbp, q);
                    binfo.set_acpred(acpred);
                    Ok(binfo)
                },
            Type::P => {
                    if br.read_bool()? { return Ok(BlockInfo::new(Type::Skip, 0, info.get_quant())); }
                    let mut cbpc = br.read_cb(&self.tables.inter_mcbpc_cb)?;
                    while cbpc == 20 { cbpc = br.read_cb(&self.tables.inter_mcbpc_cb)?; }
                    let is_intra = (cbpc & 0x04) != 0;
                    let dquant   = (cbpc & 0x08) != 0;
                    let is_4x4   = (cbpc & 0x10) != 0;
                    if is_intra {
                        let mut acpred = ACPredMode::None;
                        if let Some(ref pi) = info.plusinfo {
                            if pi.aic {
                                let acpp = br.read_bool()?;
                                acpred = ACPredMode::DC;
                                if acpp {
                                    acpred = if !br.read_bool()? { ACPredMode::Hor } else { ACPredMode::Ver };
                                }
                            }
                        }
                        let mut mvec: Vec<MV> = Vec::new();
                        let bbinfo = decode_b_info(br, self.is_pb, self.is_ipb, true)?;
                        let cbpy = br.read_cb(&self.tables.cbpy_cb)?;
                        let cbp = (cbpy << 2) | (cbpc & 3);
                        if dquant {
                            let idx = br.read(2)? as usize;
                            q = (i16::from(q) + i16::from(H263_DQUANT_TAB[idx])) as u8;
                        }
                        let mut binfo = BlockInfo::new(Type::I, cbp, q);
                        binfo.set_bpart(bbinfo);
                        binfo.set_acpred(acpred);
                        if self.is_pb {
                            for _ in 0..bbinfo.get_num_mv() {
                                mvec.push(decode_mv(br, &self.tables.mv_cb)?);
                            }
                            binfo.set_b_mv(mvec.as_slice());
                        }
                        return Ok(binfo);
                    }

                    let bbinfo = decode_b_info(br, self.is_pb, self.is_ipb, false)?;
                    let mut cbpy = br.read_cb(&self.tables.cbpy_cb)?;
//                    if /* !aiv && */(cbpc & 3) != 3 {
                        cbpy ^= 0xF;
//                    }
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    if dquant {
                        let idx = br.read(2)? as usize;
                        q = (i16::from(q) + i16::from(H263_DQUANT_TAB[idx])) as u8;
                    }
                    let mut binfo = BlockInfo::new(Type::P, cbp, q);
                    binfo.set_bpart(bbinfo);
                    if !is_4x4 {
                        let mvec: [MV; 1] = [decode_mv(br, &self.tables.mv_cb)?];
                        binfo.set_mv(&mvec);
                    } else {
                        let mvec: [MV; 4] = [
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?
                            ];
                        binfo.set_mv(&mvec);
                    }
                    if self.is_pb {
                        let mut mvec: Vec<MV> = Vec::with_capacity(bbinfo.get_num_mv());
                        for _ in 0..bbinfo.get_num_mv() {
                            let mv = decode_mv(br, &self.tables.mv_cb)?;
                            mvec.push(mv);
                        }
                        binfo.set_b_mv(mvec.as_slice());
                    }
                    Ok(binfo)
                },
            _ => Err(DecoderError::InvalidData),
        }
    }

    #[allow(unused_variables)]
    fn decode_block_intra(&mut self, info: &BlockInfo, _sstate: &SliceState, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, true, coded, blk, if no < 4 { 0 } else { no - 3 }, info.get_acpred())
    }

    #[allow(unused_variables)]
    fn decode_block_inter(&mut self, info: &BlockInfo, _sstate: &SliceState, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, false, coded, blk, if no < 4 { 0 } else { no - 3 }, ACPredMode::None)
    }

    fn is_slice_end(&mut self) -> bool { self.br.peek(16) == 0 }
}

impl VivoDecoder {
    fn new() -> Self {
        let mut coderead = H263ShortCodeReader::new(H263_INTRA_MCBPC);
        let intra_mcbpc_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_INTER_MCBPC);
        let inter_mcbpc_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_CBPY);
        let cbpy_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263RLCodeReader::new(H263_RL_CODES);
        let rl_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263RLCodeReader::new(H263_RL_CODES_AIC);
        let aic_rl_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_MV);
        let mv_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let tables = Tables {
            intra_mcbpc_cb,
            inter_mcbpc_cb,
            cbpy_cb,
            rl_cb,
            aic_rl_cb,
            mv_cb,
        };

        VivoDecoder{
            info:           NACodecInfo::new_dummy(),
            dec:            H263BaseDecoder::new_with_opts(H263DEC_OPT_SLICE_RESET | H263DEC_OPT_USES_GOB | H263DEC_OPT_PRED_QUANT),
            tables,
            bdsp:           VivoBlockDSP::new(),
            lastframe:      None,
            lastpts:        None,
            width:          0,
            height:         0,
        }
    }
}

impl NADecoder for VivoDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let fmt = formats::YUV420_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            self.width  = w;
            self.height = h;
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

        if src.len() == 0 {
            let buftype;
            let ftype;
            if self.lastframe.is_none() {
                buftype = NABufferType::None;
                ftype = FrameType::Skip;
            } else {
                let mut buf = None;
                std::mem::swap(&mut self.lastframe, &mut buf);
                buftype = buf.unwrap();
                ftype = FrameType::B;
            }
            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), buftype);
            frm.set_keyframe(false);
            frm.set_frame_type(ftype);
            if self.lastpts.is_some() {
                frm.set_pts(self.lastpts);
                self.lastpts = None;
            }
            return Ok(frm.into_ref());
        }
        let mut ibr = VivoBR::new(&src, &self.tables, self.width, self.height);

        let bufinfo = self.dec.parse_frame(&mut ibr, &self.bdsp)?;

        let mut cur_pts = pkt.get_pts();
        if !self.dec.is_intra() {
            let bret = self.dec.get_bframe(&self.bdsp);
            if let Ok(b_buf) = bret {
                self.lastframe = Some(b_buf);
                self.lastpts = pkt.get_pts();
                if let Some(pts) = pkt.get_pts() {
                    cur_pts = Some(pts + 1);
                }
            }
        }

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(self.dec.is_intra());
        frm.set_frame_type(if self.dec.is_intra() { FrameType::I } else { FrameType::P });
        frm.set_pts(cur_pts);
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.dec.flush();
    }
}

impl NAOptionHandler for VivoDecoder {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { &[] }
    fn set_options(&mut self, _options: &[NAOption]) { }
    fn query_option_value(&self, _name: &str) -> Option<NAValue> { None }
}


pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(VivoDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::vivo_register_all_decoders;
    use crate::vivo_register_all_demuxers;
    #[test]
    fn test_vivo1() {
        let mut dmx_reg = RegisteredDemuxers::new();
        vivo_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        vivo_register_all_decoders(&mut dec_reg);

test_file_decoding("vivo", "assets/Misc/gr_al.viv", Some(16), true, false, Some("viv1"), &dmx_reg, &dec_reg);
//        test_decoding("vivo", "vivo1", "assets/Misc/gr_al.viv", Some(16),
//                      &dmx_reg, &dec_reg, ExpectedTestResult::GenerateMD5Frames));
    }
    #[test]
    fn test_vivo2() {
        let mut dmx_reg = RegisteredDemuxers::new();
        vivo_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        vivo_register_all_decoders(&mut dec_reg);

test_file_decoding("vivo", "assets/Misc/02-KimagureOrangeRoad.viv", Some(50), true, false, Some("viv2"), &dmx_reg, &dec_reg);
panic!("end");
//        test_decoding("vivo", "vivo2", "assets/Misc/greetings.viv", Some(16),
//                      &dmx_reg, &dec_reg, ExpectedTestResult::GenerateMD5Frames));
    }
}
