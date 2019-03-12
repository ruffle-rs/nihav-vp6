use std::rc::Rc;
use std::cell::{Ref, RefCell};
use nihav_core::io::bitreader::*;
use nihav_core::formats;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use super::ivi::*;
use super::ivibr::*;

fn calc_quant(glob_q: u32, qd: i16) -> usize {
    let qq = (glob_q as i16) + (qd as i16);
    if qq < 0 {
        0
    } else if qq > 23 {
        23
    } else {
        qq as usize
    }
}

struct Indeo5Parser {
    mb_cb:          IVICodebook,

    width:          usize,
    height:         usize,
    tile_w:         usize,
    tile_h:         usize,
    luma_bands:     usize,
    chroma_bands:   usize,

    is_hpel:        [bool; 5],
    mb_size:        [usize; 5],
    blk_size:       [usize; 5],
}

impl Indeo5Parser {
    fn new() -> Self {
        Indeo5Parser {
            mb_cb:      IVI_CB_ZERO,

            width:          0,
            height:         0,
            tile_w:         0,
            tile_h:         0,
            luma_bands:     0,
            chroma_bands:   0,

            is_hpel:    [false; 5],
            mb_size:    [0; 5],
            blk_size:   [0; 5],
        }
    }
}

fn skip_extension(br: &mut BitReader) -> DecoderResult<()> {
    loop {
        let len             = br.read(8)?;
        if len == 0 { break; }
        br.skip(len * 8)?;
    }
    Ok(())
}

impl IndeoXParser for Indeo5Parser {
#[allow(unused_variables)]
#[allow(unused_assignments)]
    fn decode_picture_header(&mut self, br: &mut BitReader) -> DecoderResult<PictureHeader> {
        let sync                = br.read(5)?;
        validate!(sync == 0x1F);
        let ftype_idx           = br.read(3)?;
        validate!(ftype_idx < 5);
        let ftype               = INDEO5_FRAME_TYPE[ftype_idx as usize];
        let fnum                = br.read(8)?;
        if ftype == IVIFrameType::Intra {
            let gop_flags       = br.read(8)?;
            let hdr_size;
            if (gop_flags & 0x01) != 0 {
                hdr_size        = br.read(16)?;
            } else {
                hdr_size = 0;
            }
            if (gop_flags & 0x20) != 0 {
                br.skip(32)?; // lock word
            }
            self.tile_w = 0;
            self.tile_h = 0;
            if (gop_flags & 0x40) != 0 {
                self.tile_w     = 64 << br.read(2)?;
                self.tile_h = self.tile_w;
            }
            validate!(self.tile_w < 256);
            self.luma_bands     = (br.read(2)? * 3 + 1) as usize;
            self.chroma_bands   = (br.read(1)? * 3 + 1) as usize;
            validate!((self.luma_bands == 4) || (self.luma_bands == 1));
            validate!(self.chroma_bands == 1);
            let pic_size_idx    = br.read(4)? as usize;
            let w;
            let h;
            if pic_size_idx < 15 {
                w = INDEO5_PICTURE_SIZE_TAB[pic_size_idx][0];
                h = INDEO5_PICTURE_SIZE_TAB[pic_size_idx][1];
            } else {
                h               = br.read(13)? as usize;
                w               = br.read(13)? as usize;
            }
            validate!((w != 0) && (h != 0));
            self.width  = w;
            self.height = h;

            validate!((gop_flags & 0x02) == 0);
            if self.tile_w == 0 {
                self.tile_w = w;
                self.tile_h = h;
            }
            for b in 0..self.luma_bands+self.chroma_bands {
                self.is_hpel[b]     = br.read_bool()?;
                let mb_scale        = br.read(1)?;
                self.blk_size[b]    = 8 >> br.read(1)?;
                self.mb_size[b]     = self.blk_size[b] << (1 - mb_scale);
                let ext_tr          = br.read_bool()?;
                validate!(!ext_tr);
                let end_marker      = br.read(2)?;
                validate!(end_marker == 0);
            }
            if (gop_flags & 0x08) != 0 {
                let align       = br.read(3)?;
                validate!(align == 0);
                if br.read_bool()? {
                    br.skip(24)?; // transparency color
                }
            }
            br.align();
            br.skip(23)?;
            if br.read_bool()? { // gop extension
                loop {
                    let v       = br.read(16)?;
                    if (v & 0x8000) == 0 { break; }
                }
            }
            br.align();
        }
        if ftype.is_null() {
            br.align();
            return Ok(PictureHeader::new_null(ftype));
        }
        let flags               = br.read(8)?;
        let size;
        if (flags & 0x01) != 0 {
            size                = br.read(24)?;
        } else {
            size = 0;
        }
        let checksum;
        if (flags & 0x10) != 0 {
            checksum            = br.read(16)?;
        } else {
            checksum = 0;
        }
        if (flags & 0x20) != 0 {
            skip_extension(br)?;
        }
        let in_q = (flags & 0x08) != 0;
        self.mb_cb              = br.read_ivi_codebook_desc(true, (flags & 0x40) != 0)?;
        br.skip(3)?;
        br.align();

        Ok(PictureHeader::new(ftype, self.width, self.height, self.tile_w, self.tile_h, false, self.luma_bands, self.chroma_bands, in_q))
    }

#[allow(unused_variables)]
    fn decode_band_header(&mut self, br: &mut BitReader, pic_hdr: &PictureHeader, plane_no: usize, band_no: usize) -> DecoderResult<BandHeader> {
        let band_flags      = br.read(8)?;

        if (band_flags & 0x01) != 0 {
            br.align();
            return Ok(BandHeader::new_empty(plane_no, band_no));
        }
        let inherit_mv = (band_flags & 0x02) != 0;
        let has_qdelta = (band_flags & 0x04) != 0;
        let inherit_qd = ((band_flags & 0x08) != 0) || !has_qdelta;
        let data_size: usize;
        if (band_flags & 0x80) != 0 {
            data_size       = br.read(24)? as usize;
        } else {
            data_size = 0;
        }
        validate!(data_size <= ((br.left() / 8) as usize));

        let num_corr: usize;
        let mut corr_map: [u8; CORR_MAP_SIZE] = [0; CORR_MAP_SIZE];
        if (band_flags & 0x10) != 0 {
            num_corr = br.read(8)? as usize;
            validate!(num_corr*2 <= CORR_MAP_SIZE);
            for i in 0..num_corr*2 {
                corr_map[i] = br.read(8)? as u8;
            }
        } else {
            num_corr = 0;
        }
        let rvmap_idx;
        if (band_flags & 0x40) != 0 {
            rvmap_idx       = br.read(3)? as usize;
        } else {
            rvmap_idx = 8;
        }
        let blk_cb = br.read_ivi_codebook_desc(false, (band_flags & 0x80) != 0)?;
        if br.read_bool()? {
            br.skip(16)?; // checksum
        }
        let band_q          = br.read(5)?;
        if (band_flags & 0x20) != 0 {
            skip_extension(br)?;
        }
        br.align();

        let tr;
        let txtype;
        let band_id = if plane_no == 0 { band_no } else { self.luma_bands };
        match plane_no {
            0 => {
                    let scan = INDEO5_SCAN8X8[band_no];
                    let qintra;
                    let qinter;
                    validate!(self.blk_size[band_id] == 8);
                    match band_no {
                        0 => {
                                tr = IVITransformType::Slant(TSize::T8x8, TDir::TwoD);
                                if self.luma_bands == 1 {
                                    qintra = INDEO5_Q8_INTRA[0];
                                    qinter = INDEO5_Q8_INTER[0];
                                } else {
                                    qintra = INDEO5_Q8_INTRA[1];
                                    qinter = INDEO5_Q8_INTER[1];
                                }
                            },
                        1 => {
                                tr = IVITransformType::Slant(TSize::T8x8, TDir::Row);
                                qintra = INDEO5_Q8_INTRA[2];
                                qinter = INDEO5_Q8_INTER[2];
                            },
                        2 => {
                                tr = IVITransformType::Slant(TSize::T8x8, TDir::Col);
                                qintra = INDEO5_Q8_INTRA[3];
                                qinter = INDEO5_Q8_INTER[3];
                            },
                        3 => {
                                tr = IVITransformType::None(TSize::T8x8);
                                qintra = INDEO5_Q8_INTRA[4];
                                qinter = INDEO5_Q8_INTER[4];
                            },
                        _ => { unreachable!(); }
                    };
                    txtype = TxType::Transform8(TxParams8x8::new(qintra, qinter, scan));
                },
            1 | 2 => {
                    validate!(self.blk_size[band_id] == 4);
                    tr = IVITransformType::Slant(TSize::T4x4, TDir::TwoD);
                    let scan = INDEO5_SCAN4X4;
                    let qintra = INDEO5_Q4_INTRA;
                    let qinter = INDEO5_Q4_INTER;
                    txtype = TxType::Transform4(TxParams4x4::new(qintra, qinter, scan));
                },
            _ => { unreachable!(); }
        };

        Ok(BandHeader::new(plane_no, band_no, self.mb_size[band_id], self.blk_size[band_id], self.is_hpel[band_id], inherit_mv, has_qdelta, inherit_qd, band_q, rvmap_idx, num_corr, corr_map, blk_cb, tr, txtype))
    }

    fn decode_mb_info(&mut self, br: &mut BitReader, pic_hdr: &PictureHeader, band: &BandHeader, tile: &mut IVITile, ref_tile: Option<Ref<IVITile>>, mv_scale: u8) -> DecoderResult<()> {
        let mut mv_x = 0;
        let mut mv_y = 0;
        let band_id = if pic_hdr.luma_bands == 4 { band.band_no + 1 } else { 0 };
        let mut mb_idx = 0;
        for mb_y in 0..tile.mb_h {
            for mb_x in 0..tile.mb_w {
                let mut mb = MB::new(tile.pos_x + mb_x * band.mb_size, tile.pos_y + mb_y * band.mb_size);
                if !br.read_bool()? {
                    if pic_hdr.ftype.is_intra() {
                        mb.mtype = MBType::Intra;
                    } else if band.inherit_mv {
                        if let Some(ref tileref) = ref_tile {
                            mb.mtype = tileref.mb[mb_idx].mtype;
                        } else {
                            return Err(DecoderError::MissingReference);
                        }
                    } else {
                        mb.mtype = if br.read_bool()? { MBType::Inter } else { MBType::Intra };
                    }
                    if band.mb_size == band.blk_size {
                        mb.cbp = br.read(1)? as u8;
                    } else {
                        mb.cbp = br.read(4)? as u8;
                    }
                    let q;
                    if band.has_qdelta {
                        if band.inherit_qd {
                            if let Some(ref tileref) = ref_tile {
                                mb.qd = tileref.mb[mb_idx].qd;
                                q = calc_quant(band.quant, mb.qd);
                            } else {
                                return Err(DecoderError::MissingReference);
                            }
                        } else if (mb.cbp != 0) || ((band.plane_no == 0) && (band.band_no == 0) && pic_hdr.in_q) {
                            mb.qd = br.read_ivi_cb_s(&self.mb_cb)? as i16;
                            q = calc_quant(band.quant, mb.qd);
                        } else {
                            q = band.quant as usize;
                        }
                    } else {
                        q = band.quant as usize;
                    }

                    if mb.mtype == MBType::Intra {
                        if band.blk_size == 8 {
                            mb.q = INDEO5_QSCALE8_INTRA[band_id][q];
                        } else {
                            mb.q = INDEO5_QSCALE4_INTRA[q];
                        }
                    } else {
                        if band.blk_size == 8 {
                            mb.q = INDEO5_QSCALE8_INTER[band_id][q];
                        } else {
                            mb.q = INDEO5_QSCALE4_INTER[q];
                        }
                    }

                    if mb.mtype != MBType::Intra {
                        if band.inherit_mv {
                            if let Some(ref tileref) = ref_tile {
                                let mx = tileref.mb[mb_idx].mv_x;
                                let my = tileref.mb[mb_idx].mv_y;
                                if mv_scale == 0 {
                                    mb.mv_x = mx;
                                    mb.mv_y = my;
                                } else {
                                    mb.mv_x = scale_mv(mx, mv_scale);
                                    mb.mv_y = scale_mv(my, mv_scale);
                                }
                            }
                        } else {
                            mv_y += br.read_ivi_cb_s(&self.mb_cb)?;
                            mv_x += br.read_ivi_cb_s(&self.mb_cb)?;
                            mb.mv_x = mv_x;
                            mb.mv_y = mv_y;
                        }
                    }
                } else {
                    validate!(!pic_hdr.ftype.is_intra());
                    mb.mtype = MBType::Inter;
                    mb.cbp   = 0;
                    mb.qd    = 0;
                    if (band.plane_no == 0) && (band.band_no == 0) && pic_hdr.in_q {
                        mb.qd = br.read_ivi_cb_s(&self.mb_cb)? as i16;
                        let q = calc_quant(band.quant, mb.qd);
                        if mb.mtype == MBType::Intra {
                            if band.blk_size == 8 {
                                mb.q = INDEO5_QSCALE8_INTRA[band_id][q];
                            } else {
                                mb.q = INDEO5_QSCALE4_INTRA[q];
                            }
                        } else {
                            if band.blk_size == 8 {
                                mb.q = INDEO5_QSCALE8_INTER[band_id][q];
                            } else {
                                mb.q = INDEO5_QSCALE4_INTER[q];
                            }
                        }
                    }
                    if band.inherit_mv {
                        if let Some(ref tileref) = ref_tile {
                            let mx = tileref.mb[mb_idx].mv_x;
                            let my = tileref.mb[mb_idx].mv_y;
                            if mv_scale == 0 {
                                mb.mv_x = mx;
                                mb.mv_y = my;
                            } else {
                                mb.mv_x = scale_mv(mx, mv_scale);
                                mb.mv_y = scale_mv(my, mv_scale);
                            }
                        }
                    }
                }
                tile.mb[mb_idx] = mb;
                mb_idx += 1;
            }
        }
        br.align();
        Ok(())
    }

    fn recombine_plane(&mut self, src: &[i16], sstride: usize, dst: &mut [u8], dstride: usize, w: usize, h: usize) {
        let mut idx0 = 0;
        let mut idx1 = w / 2;
        let mut idx2 = (h / 2) * sstride;
        let mut idx3 = idx2 + idx1;
        let mut bidx1 = idx1;
        let mut bidx3 = idx3;
        let mut oidx0 = 0;
        let mut oidx1 = dstride;
        let filt_lo = |a: i16, b: i16| a + b;
        let filt_hi = |a: i16, b: i16, c: i16| a - b * 6 + c;

        for _ in 0..(h/2) {
            let mut b0_1 = src[idx0];
            let mut b0_2 = src[idx0 + sstride];
            let mut b1_1 = src[bidx1];
            let mut b1_2 = src[idx1];
            let mut b1_3 = filt_hi(b1_1, b1_2, src[idx1 + sstride]);
            let mut b2_1;
            let mut b2_2 = src[idx2];
            let mut b2_3 = b2_2;
            let mut b2_4;
            let mut b2_5 = src[idx2 + sstride];
            let mut b2_6 = b2_5;
            let mut b3_1;
            let mut b3_2 = src[bidx3];
            let mut b3_3 = b3_2;
            let mut b3_4;
            let mut b3_5 = src[idx3];
            let mut b3_6 = b3_5;
            let mut b3_8 = filt_hi(b3_2, b3_5, src[idx3 + sstride]);
            let mut b3_9 = b3_8;
            let mut b3_7;

            for x in 0..(w/2) {
                b2_1 = b2_2;
                b2_2 = b2_3;
                b2_4 = b2_5;
                b2_5 = b2_6;
                b3_1 = b3_2;
                b3_2 = b3_3;
                b3_4 = b3_5;
                b3_5 = b3_6;
                b3_7 = b3_8;
                b3_8 = b3_9;

                let tmp0 = b0_1;
                let tmp1 = b0_2;
                b0_1 = src[idx0 + x + 1];
                b0_2 = src[idx0 + x + 1 + sstride];
                let mut p0 =  tmp0                       << 4;
                let mut p1 = (tmp0 + b0_1)               << 3;
                let mut p2 = (tmp0 + tmp1)               << 3;
                let mut p3 = (tmp0 + tmp1 + b0_1 + b0_2) << 2;

                let tmp0 = b1_1;
                let tmp1 = b1_2;
                let tmp2 = filt_lo(tmp0, tmp1);
                let tmp3 = filt_hi(tmp0, tmp1, b1_3);
                b1_2 = src[ idx1 + x + 1];
                b1_1 = src[bidx1 + x + 1];
                b1_3 = filt_hi(b1_1, b1_2, src[idx1 + x + 1 + sstride]);
                p0 +=  tmp2                << 3;
                p1 += (tmp2 + b1_1 + b1_2) << 2;
                p2 +=  tmp3                << 2;
                p3 += (tmp3 + b1_3)        << 1;

                b2_3 = src[idx2 + x + 1];
                b2_6 = src[idx2 + x + 1 + sstride];
                let tmp0 = filt_lo(b2_1, b2_2);
                let tmp1 = filt_hi(b2_1, b2_2, b2_3);
                p0 +=  tmp0                              << 3;
                p1 +=  tmp1                              << 2;
                p2 += (tmp0 + filt_lo(b2_4, b2_5))       << 2;
                p3 += (tmp1 + filt_hi(b2_4, b2_5, b2_6)) << 1;

                b3_6 = src[idx3 + x + 1];
                b3_3 = src[bidx3 + x + 1];
                b3_9 = filt_hi(b3_3, b3_6, src[idx3 + x + 1 + sstride]);
                let tmp0 = b3_1 + b3_4;
                let tmp1 = b3_2 + b3_5;
                let tmp2 = b3_3 + b3_6;
                p0 += filt_lo(tmp0, tmp1)       << 2;
                p1 += filt_hi(tmp0, tmp1, tmp2) << 1;
                p2 += filt_lo(b3_7, b3_8)       << 1;
                p3 += filt_hi(b3_7, b3_8, b3_9) << 0;

                dst[oidx0 + x * 2 + 0] = clip8((p0 >> 6) + 128);
                dst[oidx0 + x * 2 + 1] = clip8((p1 >> 6) + 128);
                dst[oidx1 + x * 2 + 0] = clip8((p2 >> 6) + 128);
                dst[oidx1 + x * 2 + 1] = clip8((p3 >> 6) + 128);
            }
            bidx1 = idx1;
            bidx3 = idx3;
            idx0 += sstride;
            idx1 += sstride;
            idx2 += sstride;
            idx3 += sstride;
            oidx0 += dstride * 2;
            oidx1 += dstride * 2;
        }
    }
}

struct Indeo5Decoder {
    info:   Rc<NACodecInfo>,
    dec:    IVIDecoder,
    ip:     Indeo5Parser,
}

impl Indeo5Decoder {
    fn new() -> Self {
        Indeo5Decoder {
            info:   NACodecInfo::new_dummy(),
            dec:    IVIDecoder::new(),
            ip:     Indeo5Parser::new(),
        }
    }
}

impl NADecoder for Indeo5Decoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let f = vinfo.is_flipped();
            let fmt = formats::YUV410_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, f, fmt));
            self.info = Rc::new(NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()));
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        let mut br = BitReader::new(src.as_slice(), src.len(), BitReaderMode::LE);

        let bufinfo = self.dec.decode_frame(&mut self.ip, &mut br)?;
        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(self.dec.is_intra());
        frm.set_frame_type(self.dec.get_frame_type());
        Ok(Rc::new(RefCell::new(frm)))
    }
}

const INDEO5_PICTURE_SIZE_TAB: [[usize; 2]; 15] = [
    [640, 480], [320, 240], [160, 120], [704, 480], [352, 240], [352, 288], [176, 144],
    [240, 180], [640, 240], [704, 240], [80, 60], [88, 72], [0, 0], [0, 0], [0, 0]
];

const INDEO5_FRAME_TYPE: [IVIFrameType; 5] = [
    IVIFrameType::Intra, IVIFrameType::Inter, IVIFrameType::InterScal,
    IVIFrameType::InterDroppable, IVIFrameType::NULL,
];

const INDEO5_QUANT8X8_INTRA: [[u16; 64]; 5] = [
  [
    0x1a, 0x2e, 0x36, 0x42, 0x46, 0x4a, 0x4e, 0x5a,
    0x2e, 0x32, 0x3e, 0x42, 0x46, 0x4e, 0x56, 0x6a,
    0x36, 0x3e, 0x3e, 0x44, 0x4a, 0x54, 0x66, 0x72,
    0x42, 0x42, 0x44, 0x4a, 0x52, 0x62, 0x6c, 0x7a,
    0x46, 0x46, 0x4a, 0x52, 0x5e, 0x66, 0x72, 0x8e,
    0x4a, 0x4e, 0x54, 0x62, 0x66, 0x6e, 0x86, 0xa6,
    0x4e, 0x56, 0x66, 0x6c, 0x72, 0x86, 0x9a, 0xca,
    0x5a, 0x6a, 0x72, 0x7a, 0x8e, 0xa6, 0xca, 0xfe,
  ], [
    0x26, 0x3a, 0x3e, 0x46, 0x4a, 0x4e, 0x52, 0x5a,
    0x3a, 0x3e, 0x42, 0x46, 0x4a, 0x4e, 0x56, 0x5e,
    0x3e, 0x42, 0x46, 0x48, 0x4c, 0x52, 0x5a, 0x62,
    0x46, 0x46, 0x48, 0x4a, 0x4e, 0x56, 0x5e, 0x66,
    0x4a, 0x4a, 0x4c, 0x4e, 0x52, 0x5a, 0x62, 0x6a,
    0x4e, 0x4e, 0x52, 0x56, 0x5a, 0x5e, 0x66, 0x6e,
    0x52, 0x56, 0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x72,
    0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x6e, 0x72, 0x76,
  ], [
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
  ], [
    0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e,
    0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
    0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2,
    0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4,
    0xde, 0xde, 0xde, 0xde, 0xde, 0xde, 0xde, 0xde,
    0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2,
    0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6,
    0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2,
  ], [
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
  ]
];
const INDEO5_QUANT8X8_INTER: [[u16; 64]; 5] = [
  [
    0x26, 0x3a, 0x3e, 0x46, 0x4a, 0x4e, 0x52, 0x5a,
    0x3a, 0x3e, 0x42, 0x46, 0x4a, 0x4e, 0x56, 0x5e,
    0x3e, 0x42, 0x46, 0x48, 0x4c, 0x52, 0x5a, 0x62,
    0x46, 0x46, 0x48, 0x4a, 0x4e, 0x56, 0x5e, 0x66,
    0x4a, 0x4a, 0x4c, 0x4e, 0x52, 0x5a, 0x62, 0x6a,
    0x4e, 0x4e, 0x52, 0x56, 0x5a, 0x5e, 0x66, 0x6e,
    0x52, 0x56, 0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x72,
    0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x6e, 0x72, 0x76,
  ], [
    0x26, 0x3a, 0x3e, 0x46, 0x4a, 0x4e, 0x52, 0x5a,
    0x3a, 0x3e, 0x42, 0x46, 0x4a, 0x4e, 0x56, 0x5e,
    0x3e, 0x42, 0x46, 0x48, 0x4c, 0x52, 0x5a, 0x62,
    0x46, 0x46, 0x48, 0x4a, 0x4e, 0x56, 0x5e, 0x66,
    0x4a, 0x4a, 0x4c, 0x4e, 0x52, 0x5a, 0x62, 0x6a,
    0x4e, 0x4e, 0x52, 0x56, 0x5a, 0x5e, 0x66, 0x6e,
    0x52, 0x56, 0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x72,
    0x5a, 0x5e, 0x62, 0x66, 0x6a, 0x6e, 0x72, 0x76,
  ], [
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
    0x4e, 0xaa, 0xf2, 0xd4, 0xde, 0xc2, 0xd6, 0xc2,
  ], [
    0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e, 0x4e,
    0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
    0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2, 0xf2,
    0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4, 0xd4,
    0xde, 0xde, 0xde, 0xde, 0xde, 0xde, 0xde, 0xde,
    0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2,
    0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6, 0xd6,
    0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2, 0xc2,
  ], [
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
    0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e, 0x5e,
  ]
];
const INDEO5_QUANT4X4_INTRA: [u16; 16] = [
    0x1e, 0x3e, 0x4a, 0x52,
    0x3e, 0x4a, 0x52, 0x5e,
    0x4a, 0x52, 0x5e, 0x7a,
    0x52, 0x5e, 0x7a, 0x92
];
const INDEO5_QUANT4X4_INTER: [u16; 16] = [
    0x1e, 0x3e, 0x4a, 0x52,
    0x3e, 0x4a, 0x52, 0x56,
    0x4a, 0x52, 0x56, 0x5e,
    0x52, 0x56, 0x5e, 0x66
];
const INDEO5_Q8_INTRA: [&[u16; 64]; 5] = [
    &INDEO5_QUANT8X8_INTRA[0], &INDEO5_QUANT8X8_INTRA[1], &INDEO5_QUANT8X8_INTRA[2],
    &INDEO5_QUANT8X8_INTRA[3], &INDEO5_QUANT8X8_INTRA[4],
];
const INDEO5_Q8_INTER: [&[u16; 64]; 5] = [
    &INDEO5_QUANT8X8_INTER[0], &INDEO5_QUANT8X8_INTER[1], &INDEO5_QUANT8X8_INTER[2],
    &INDEO5_QUANT8X8_INTER[3], &INDEO5_QUANT8X8_INTER[4],
];
const INDEO5_Q4_INTRA: &[u16; 16] = &INDEO5_QUANT4X4_INTRA;
const INDEO5_Q4_INTER: &[u16; 16] = &INDEO5_QUANT4X4_INTER;

const INDEO5_SCAN8X8: [&[usize; 64]; 4] = [
    &IVI_ZIGZAG, &IVI_SCAN_8X8_VER, &IVI_SCAN_8X8_HOR, &IVI_SCAN_8X8_HOR
];
const INDEO5_SCAN4X4: &[usize; 16] = &IVI_SCAN_4X4;

const INDEO5_QSCALE8_INTRA: [[u8; 24]; 5] = [
  [
    0x0b, 0x0e, 0x10, 0x12, 0x14, 0x16, 0x17, 0x18, 0x1a, 0x1c, 0x1e, 0x20,
    0x22, 0x24, 0x27, 0x28, 0x2a, 0x2d, 0x2f, 0x31, 0x34, 0x37, 0x39, 0x3c,
  ], [
    0x01, 0x10, 0x12, 0x14, 0x16, 0x18, 0x1b, 0x1e, 0x22, 0x25, 0x28, 0x2c,
    0x30, 0x34, 0x38, 0x3d, 0x42, 0x47, 0x4c, 0x52, 0x58, 0x5e, 0x65, 0x6c,
  ], [
    0x13, 0x22, 0x27, 0x2a, 0x2d, 0x33, 0x36, 0x3c, 0x41, 0x45, 0x49, 0x4e,
    0x53, 0x58, 0x5d, 0x63, 0x69, 0x6f, 0x75, 0x7c, 0x82, 0x88, 0x8e, 0x95,
  ], [
    0x13, 0x1f, 0x21, 0x24, 0x27, 0x29, 0x2d, 0x2f, 0x34, 0x37, 0x3a, 0x3d,
    0x40, 0x44, 0x48, 0x4c, 0x4f, 0x52, 0x56, 0x5a, 0x5e, 0x62, 0x66, 0x6b,
  ], [
    0x31, 0x42, 0x47, 0x47, 0x4d, 0x52, 0x58, 0x58, 0x5d, 0x63, 0x67, 0x6b,
    0x6f, 0x73, 0x78, 0x7c, 0x80, 0x84, 0x89, 0x8e, 0x93, 0x98, 0x9d, 0xa4,
  ]
];
const INDEO5_QSCALE8_INTER: [[u8; 24]; 5] = [
  [
    0x0b, 0x11, 0x13, 0x14, 0x15, 0x16, 0x18, 0x1a, 0x1b, 0x1d, 0x20, 0x22,
    0x23, 0x25, 0x28, 0x2a, 0x2e, 0x32, 0x35, 0x39, 0x3d, 0x41, 0x44, 0x4a,
  ], [
    0x07, 0x14, 0x16, 0x18, 0x1b, 0x1e, 0x22, 0x25, 0x29, 0x2d, 0x31, 0x35,
    0x3a, 0x3f, 0x44, 0x4a, 0x50, 0x56, 0x5c, 0x63, 0x6a, 0x71, 0x78, 0x7e,
  ], [
    0x15, 0x25, 0x28, 0x2d, 0x30, 0x34, 0x3a, 0x3d, 0x42, 0x48, 0x4c, 0x51,
    0x56, 0x5b, 0x60, 0x65, 0x6b, 0x70, 0x76, 0x7c, 0x82, 0x88, 0x8f, 0x97,
  ], [
    0x13, 0x1f, 0x20, 0x22, 0x25, 0x28, 0x2b, 0x2d, 0x30, 0x33, 0x36, 0x39,
    0x3c, 0x3f, 0x42, 0x45, 0x48, 0x4b, 0x4e, 0x52, 0x56, 0x5a, 0x5e, 0x62,
  ], [
    0x3c, 0x52, 0x58, 0x5d, 0x63, 0x68, 0x68, 0x6d, 0x73, 0x78, 0x7c, 0x80,
    0x84, 0x89, 0x8e, 0x93, 0x98, 0x9d, 0xa3, 0xa9, 0xad, 0xb1, 0xb5, 0xba
  ]
];
const INDEO5_QSCALE4_INTRA: [u8; 24] = [
    0x01, 0x0b, 0x0b, 0x0d, 0x0d, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x13, 0x14,
    0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20
];
const INDEO5_QSCALE4_INTER: [u8; 24] = [
    0x0b, 0x0d, 0x0d, 0x0e, 0x11, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23
];

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(Indeo5Decoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::indeo_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;
    #[test]
    fn test_indeo5() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        indeo_register_all_codecs(&mut dec_reg);

        test_file_decoding("avi", "assets/Indeo/IV5/sample.avi", /*None*/Some(2), true, false, None, &dmx_reg, &dec_reg);
//         test_file_decoding("avi", "assets/Indeo/IV5/W32mdl_1.avi", None/*Some(2)*/, true, false, Some("iv5"));
//panic!("the end");
    }
}
