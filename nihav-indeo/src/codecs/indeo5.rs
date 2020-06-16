use nihav_core::io::bitreader::*;
use nihav_core::formats;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_codec_support::codecs::ZIGZAG;
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

    #[allow(clippy::cyclomatic_complexity)]
    fn decode_mb_info(&mut self, br: &mut BitReader, pic_hdr: &PictureHeader, band: &BandHeader, tile: &mut IVITile, ref_tile: Option<&IVITile>, mv_scale: u8) -> DecoderResult<()> {
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
    info:   NACodecInfoRef,
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
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let f = vinfo.is_flipped();
            let fmt = formats::YUV410_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, f, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        let mut br = BitReader::new(src.as_slice(), BitReaderMode::LE);

        let bufinfo = self.dec.decode_frame(&mut self.ip, &mut br)?;
        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(self.dec.is_intra());
        frm.set_frame_type(self.dec.get_frame_type());
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.dec.flush();
    }
}

impl NAOptionHandler for Indeo5Decoder {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { &[] }
    fn set_options(&mut self, _options: &[NAOption]) { }
    fn query_option_value(&self, _name: &str) -> Option<NAValue> { None }
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
    &ZIGZAG, &IVI_SCAN_8X8_VER, &IVI_SCAN_8X8_HOR, &IVI_SCAN_8X8_HOR
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

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(Indeo5Decoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::indeo_register_all_codecs;
    use nihav_commonfmt::generic_register_all_demuxers;
    #[test]
    fn test_indeo5() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        indeo_register_all_codecs(&mut dec_reg);

        test_decoding("avi", "indeo5", "assets/Indeo/IV5/sample.avi", Some(100),
                      &dmx_reg, &dec_reg, ExpectedTestResult::MD5Frames(vec![
                           [0xd73ef6e2, 0x099dc18f, 0x46450af9, 0x1b390a48],
                           [0xbe3295d6, 0xf4afd9fd, 0x820d35e8, 0x4b593c9a],
                           [0x415e5aed, 0x33afb9a2, 0x14ae9308, 0x53e906d3],
                           [0x7fa94dd3, 0x58582fc3, 0xe39977bc, 0xd88036d5],
                           [0x8eef68f7, 0xace88c0c, 0x3f6e4388, 0xfcd82f46],
                           [0xfe22fcc6, 0x8c4666ab, 0xd9888786, 0x7d9adbc8],
                           [0x37f8b6bc, 0xaea9e94a, 0x05a98f2e, 0x2dce51fa],
                           [0x37492cbd, 0x8fd516fa, 0x48a0bcd1, 0x5eb6584f],
                           [0x6f464159, 0xa2af785b, 0xb440493b, 0x86b21911],
                           [0x3a52de08, 0x9f5762b0, 0xe58a6979, 0x0abb295e],
                           [0xe8f56414, 0x36e76d76, 0xd0927365, 0x15dc5327],
                           [0x0fc17e06, 0x8cb6581c, 0x86eb730d, 0x9bedf951],
                           [0x54fb3627, 0xc02bffc6, 0x87748ee5, 0x8b12d57d],
                           [0x8e4fd3a5, 0x3a7b9cd7, 0x0a4ba1a0, 0x48e10237],
                           [0xce87ea8b, 0x1ec40c98, 0x12c9a682, 0x57d02bf0],
                           [0x7024e691, 0x6bc493ba, 0x617a7a91, 0x65997b4c],
                           [0xb8d53b7c, 0x132ffec9, 0x827cf176, 0x68e97292],
                           [0x12ed76a9, 0x11eced60, 0x473a364f, 0x1e197803],
                           [0x6c23ba3a, 0x12e2f7e3, 0x8fc0c2bc, 0x20726bb2],
                           [0x3307e5e6, 0x3e4fa871, 0x55df1d59, 0xbe055301],
                           [0x8198ee6c, 0x82a33414, 0x9fd8c430, 0x1fca7b93],
                           [0x557662c2, 0xeb3226fc, 0x2a125be4, 0xd475ffa9],
                           [0x850c0326, 0x7a0105e5, 0x37799945, 0x927d1237],
                           [0xe770097e, 0xabd460f4, 0x3d9260e0, 0x5a8132e2],
                           [0xdb6644e7, 0xde6986eb, 0x12cc4916, 0x977d2177],
                           [0xd58ced6c, 0x91c0e7b6, 0x8c5926fc, 0x2dbf3117],
                           [0x6e76dd5f, 0x088884f0, 0x8f94451f, 0xc8df4daf],
                           [0x726b2f8f, 0xd44af9ba, 0x1e188962, 0xd37c1a38],
                           [0x84035565, 0xd2370a8c, 0x8ecb4a3f, 0xd6758196],
                           [0xa1e75a16, 0xc9e230ed, 0x23de50f3, 0x2366967a],
                           [0x690a2a91, 0xfa4acef1, 0xd3de6dd0, 0x973031d9],
                           [0xb392e62a, 0x22b0d3f2, 0x0e975a86, 0x14d6dcb3],
                           [0x5e002202, 0xc80e236e, 0x0b484e02, 0x00035f47],
                           [0x4fc0f301, 0x8ec0d33d, 0xe71a12dd, 0xe799731f],
                           [0x278c9096, 0xec7fa833, 0x2094d81f, 0x52e21165],
                           [0xd55238a8, 0xf040101a, 0x1152b6fe, 0x661c9e64],
                           [0x3699d16e, 0x89d9f2d7, 0x9ad59597, 0x7361ee21],
                           [0x1419c93c, 0x91b75784, 0x18f7121d, 0xec2c6b78],
                           [0x07c435da, 0x05f18557, 0xf28ce1e0, 0x43cadcba],
                           [0x2015269d, 0x52cad948, 0xd6485611, 0x06fe33d7],
                           [0x0cea56f3, 0x82c30841, 0x9b2a8cab, 0x8a6f07cb],
                           [0x81f82aa9, 0x233060d5, 0x00f4171e, 0xe14c0c2a],
                           [0x9b2f8b08, 0x7d091eac, 0x09dcb2c3, 0xa7670405],
                           [0x99c97f75, 0xf91c6b12, 0xfbad7705, 0x1c6e6f27],
                           [0xc762b89c, 0xbf44a194, 0xb2a54dc2, 0xae2103e4],
                           [0xba4f52ed, 0xe35aff77, 0x50d8c9d3, 0xeb382d32],
                           [0x9bc9d9a0, 0x7cb4c594, 0xbc1af6f4, 0x1f718229],
                           [0x5f19eea2, 0x6260982e, 0x393fb360, 0x71abe746],
                           [0xd13f2fcc, 0x88a6a714, 0xf4f53d55, 0xf42b11ba],
                           [0x4208b476, 0xaf06ffce, 0x38e59bfe, 0x588567a2],
                           [0xbedfb7b7, 0x8300a39d, 0x964a3c0f, 0x577d52d7],
                           [0x18e5a6f2, 0x7ec85996, 0x27694f30, 0x7717748a],
                           [0xb5e6d70f, 0xc43261bb, 0xd4e6ae7c, 0xcc11f79c],
                           [0xc808cba7, 0xbb042416, 0x2f01ebe1, 0x7d176a38],
                           [0x03353805, 0x4b6e9d66, 0x25933123, 0x4213aaf7],
                           [0x189a6da5, 0x04a4cbe6, 0xea3c9d09, 0x153fdee2],
                           [0x41f8ac6b, 0xb476356b, 0xc70b67d0, 0x28caf359],
                           [0x4514b6a4, 0x788545ff, 0x4ee9139b, 0xa45bedf9],
                           [0x2a39be04, 0xac9921cb, 0x685c1bf9, 0x904bdab2],
                           [0x2c18f3ef, 0x416c0335, 0x0face768, 0x1b9d5cd2],
                           [0x898cd63f, 0x60af727f, 0x6bdf1be6, 0x0df05cfe],
                           [0x8a06787b, 0x7cee2f8b, 0xdc8aac77, 0x2e0e740a],
                           [0x3d340571, 0xbf1c8d4c, 0xddc23f69, 0xd1903942],
                           [0x7d179e85, 0x54048c4d, 0xba047d33, 0x2e9e5edb],
                           [0x65e26600, 0x87c8421d, 0xa77e2c6c, 0x32b4971a],
                           [0x69041052, 0xa4858c7b, 0x904d84f7, 0xb4ad3dcf],
                           [0x3ea0246d, 0x533e752d, 0x1d55798a, 0x30e17e72],
                           [0x4254a700, 0x07365f23, 0x0f9da313, 0xaecd38ce],
                           [0xa5756d9d, 0x79f31387, 0x0ded3654, 0xa7299663],
                           [0x4ef027c9, 0xeebb1383, 0x26a55289, 0x3746969d],
                           [0xdc6acadf, 0x23e1b6e1, 0x07fcdc26, 0x9914b684],
                           [0x52bb8b80, 0x1a5688ae, 0xd429662d, 0x1cc1485d],
                           [0x76b35f59, 0x24b64e5b, 0xbcbeaee7, 0xf568a832],
                           [0x0756d15f, 0x9cc288bf, 0x9f882a3c, 0xfe7c7161],
                           [0x0503113a, 0x95e716ff, 0x304cf65e, 0x490725e8],
                           [0x7db7ba62, 0x08e4e77d, 0xc9db6413, 0xea3f1a39],
                           [0x7cef6d67, 0xc94867e6, 0x5c674de6, 0x5eb74081],
                           [0x7573b799, 0x069d4f03, 0x63b537a1, 0xdfe25db6],
                           [0xc401e705, 0x834828bc, 0xd99da4a1, 0xd0f3bee8],
                           [0x02817844, 0xada6433e, 0x31761e98, 0x901ccf68],
                           [0x8f9432b4, 0x9f860957, 0xcba54c86, 0x8beb8209],
                           [0x6a46e58c, 0x7d299228, 0x5c001d12, 0xd8db2a00],
                           [0x0c12586d, 0x866d8ca9, 0x849bbb17, 0x5af63ea2],
                           [0xe48671b6, 0xc4377063, 0xc4d03c02, 0x621bd894],
                           [0x5f7f82eb, 0xcdb5abf5, 0x325f2d9d, 0x24a5d200],
                           [0xec6b6fe7, 0x347316c4, 0x6241904a, 0x4e2497a5],
                           [0xf661b7fd, 0xa00e2fc7, 0x90e11456, 0x507fef21],
                           [0x77c7addd, 0x67148dce, 0x1cd27059, 0xefbf4abf],
                           [0x11270d9c, 0xb352779d, 0x81f21055, 0xae93a8b6],
                           [0x3d1f0aaf, 0x3b4aa6d8, 0xca1c160c, 0x6fe4f2bd],
                           [0x17c6bec4, 0x54b568cd, 0xd19c78d6, 0x9a3d897a],
                           [0xc4ab4ca6, 0xbf3b2573, 0xb4d837dd, 0x4dfab799],
                           [0x6fd5645d, 0xa34978b2, 0x6696dd1a, 0x665ca09b],
                           [0x87984bb9, 0xd4d3bc30, 0x7f8bb7a8, 0x2d83b303],
                           [0x21fb5d58, 0x1ee47d1a, 0x97200d83, 0x1d596a88],
                           [0x2656f329, 0x497693be, 0xca971ddf, 0x410d4092],
                           [0xd285c512, 0xfc1ed632, 0x63c43ec2, 0xac5766d1],
                           [0x46fb80ee, 0xcfeecdaa, 0x7237a433, 0x5708ff56],
                           [0x4fccd9c8, 0x7b1a4f31, 0x51516a80, 0x27bf3cae],
                           [0xd649d2f5, 0xebadf1f7, 0x6b34e8ce, 0xb87e82f1],
                           [0x6eb0aec6, 0xfbe9cb51, 0x39e695b4, 0xa6e46e70]]));
    }
}
