use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use nihav_codec_support::vq::*;

#[derive(Default,Clone,Copy,PartialEq)]
struct Pixel16(u16);

impl Pixel16 {
    fn unpack(self) -> (u16, u16, u16) {
        ((self.0 >> 10) & 0x1F, (self.0 >> 5) & 0x1F, self.0 & 0x1F)
    }
    fn pack(r: u16, g: u16, b: u16) -> Self {
        Pixel16{ 0: (r << 10) | (g << 5) | b }
    }
    fn invalid() -> Self { Self(0x8000) }
    fn is_invalid(self) -> bool { self == Self::invalid() }
}
impl VQElement for Pixel16 {
    fn dist(&self, rval: Self) -> u32 {
        let (r0, g0, b0) = self.unpack();
        let (r1, g1, b1) = rval.unpack();
        let rd = i32::from(r0) - i32::from(r1);
        let gd = i32::from(g0) - i32::from(g1);
        let bd = i32::from(b0) - i32::from(b1);
        (rd * rd + gd * gd + bd * bd) as u32
    }
    fn min_cw() -> Self { Pixel16(0x0000) }
    fn max_cw() -> Self { Pixel16(0x7FFF) }
    fn min(&self, rval: Self) -> Self {
        let (r0, g0, b0) = self.unpack();
        let (r1, g1, b1) = rval.unpack();
        Self::pack(r0.min(r1), g0.min(g1), b0.min(b1))
    }
    fn max(&self, rval: Self) -> Self {
        let (r0, g0, b0) = self.unpack();
        let (r1, g1, b1) = rval.unpack();
        Self::pack(r0.max(r1), g0.max(g1), b0.max(b1))
    }
    fn num_components() -> usize { 3 }
    fn sort_by_component(arr: &mut [Self], component: usize) {
        let mut counts = [0; 32];
        for pix in arr.iter() {
            let (r, g, b) = pix.unpack();
            let idx = match component {
                    0 => r,
                    1 => g,
                    _ => b,
                } as usize;
            counts[idx] += 1;
        }
        let mut offs = [0; 32];
        for i in 0..31 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut dst = [Pixel16(0); 16];
        assert!(dst.len() >= arr.len());
        for pix in arr.iter() {
            let (r, g, b) = pix.unpack();
            let idx = match component {
                    0 => r,
                    1 => g,
                    _ => b,
                } as usize;
            dst[offs[idx]] = *pix;
            offs[idx] += 1;
        }
        let len = arr.len();
        arr.copy_from_slice(&dst[..len]);
    }
    fn max_dist_component(min: &Self, max: &Self) -> usize {
        let (r0, g0, b0) = max.unpack();
        let (r1, g1, b1) = min.unpack();
        let rd = u32::from(r0) - u32::from(r1);
        let gd = u32::from(g0) - u32::from(g1);
        let bd = u32::from(b0) - u32::from(b1);
        if rd > gd && rd > bd {
            0
        } else if bd > rd && bd > gd {
            2
        } else {
            1
        }
    }
}

struct Pixel16Sum {
    rsum: u16,
    gsum: u16,
    bsum: u16,
    count: u16,
}

impl VQElementSum<Pixel16> for Pixel16Sum {
    fn zero() -> Self { Pixel16Sum { rsum: 0, gsum: 0, bsum: 0, count: 0 } }
    fn add(&mut self, rval: Pixel16, count: u64) {
        let (r, g, b) = rval.unpack();
        let count = count as u16;
        self.rsum += r * count;
        self.gsum += g * count;
        self.bsum += b * count;
        self.count += count;
    }
    fn get_centroid(&self) -> Pixel16 {
        if self.count != 0 {
            let r = (self.rsum + self.count / 2) / self.count;
            let g = (self.gsum + self.count / 2) / self.count;
            let b = (self.bsum + self.count / 2) / self.count;
            Pixel16::pack(r, g, b)
        } else {
            Pixel16(0x0000)
        }
    }
}

#[derive(Default)]
struct BlockState {
    fill_dist:  u32,
    fill_val:   Pixel16,
    clr2_dist:  u32,
    clr2_flags: u16,
    clr2:       [Pixel16; 2],
    clr8_dist:  u32,
    clr8_flags: u16,
    clr8:       [[Pixel16; 2]; 4],
}

impl BlockState {
    fn calc_clrs(buf: &[Pixel16; 16]) -> Option<(Pixel16, Pixel16)> {
        let     clr0 = buf[0];
        let mut clr1 = Pixel16::invalid();
        for &pix in buf[1..].iter() {
            if pix != clr0 && pix != clr1 {
                if clr1.is_invalid() {
                    clr1 = pix;
                } else {
                    return None;
                }
            }
        }
        Some((clr0, clr1))
    }
    fn calc_stats(&mut self, buf: &[Pixel16; 16]) {
        let mut filled = false;
        let mut two_clr = false;
        if let Some((clr0, clr1)) = Self::calc_clrs(buf) {
            self.clr2[0] = clr0;
            self.clr2[1] = if !clr1.is_invalid() { clr1 } else { clr0 };
            if clr0 == clr1 {
                self.fill_val = Pixel16 { 0: buf[0].0 & !0x400 };
                filled = true;
            }
            two_clr = true;
        }
        self.fill_dist = 0;
        if !filled {
            let mut avg = Pixel16Sum::zero();
            for pix in buf.iter() {
                avg.add(*pix, 1);
            }
            self.fill_val = Pixel16 { 0: avg.get_centroid().0 & !0x400 };
            for pix in buf.iter() {
                self.fill_dist += pix.dist(self.fill_val);
            }
        }
        if self.fill_dist == 0 {
            self.clr2_dist = std::u32::MAX;
            self.clr8_dist = std::u32::MAX;
            return;
        }

        self.clr2_flags = 0u16;
        if two_clr {
            let mut mask = 1;
            self.clr2_dist = 0;
            for &pix in buf.iter() {
                if pix == self.clr2[0] {
                    self.clr2_flags |= mask;
                } else {
                }
                mask <<= 1;
            }
            if (self.clr2_flags & 0x8000) != 0 {
                self.clr2_flags = !self.clr2_flags;
                self.clr2.swap(0, 1);
            }
        } else if quantise_median_cut::<Pixel16, Pixel16Sum>(buf, &mut self.clr2) == 2 {
            let mut mask = 1;
            self.clr2_dist = 0;
            for pix in buf.iter() {
                let dist0 = pix.dist(self.clr2[0]);
                let dist1 = pix.dist(self.clr2[1]);
                if dist0 < dist1 {
                    self.clr2_flags |= mask;
                    self.clr2_dist += dist0;
                } else {
                    self.clr2_dist += dist1;
                }
                mask <<= 1;
            }
            if (self.clr2_flags & 0x8000) != 0 {
                self.clr2_flags = !self.clr2_flags;
                self.clr2.swap(0, 1);
            }
        } else {
            self.clr2_dist = self.fill_dist;
            self.clr2 = [self.fill_val; 2];
        }
        if self.clr2_dist == 0 {
            self.clr8_dist = std::u32::MAX;
            return;
        }

        self.clr8 = [[Pixel16 { 0: 0}; 2]; 4];
        self.clr8_flags = 0;
        self.clr8_dist = 0;
        let mut mask = 1;
        for i in 0..4 {
            let off = (i & 1) * 2 + (i & 2) * 4;
            let src2 = [buf[off], buf[off + 1], buf[off + 4], buf[off + 5]];
            let nc = quantise_median_cut::<Pixel16, Pixel16Sum>(&src2, &mut self.clr8[i]);
            if nc < 2 {
                self.clr8[i][1] = self.clr8[i][0];
            }
            for j in 0..4 {
                let dist0 = src2[j].dist(self.clr8[i][0]);
                let dist1 = src2[j].dist(self.clr8[i][1]);
                if dist0 < dist1 {
                    self.clr8_flags |= mask;
                    self.clr8_dist += dist0;
                } else {
                    self.clr8_dist += dist1;
                }
                mask <<= 1;
            }
        }
        if (self.clr8_flags & 0x8000) != 0 {
            self.clr8_flags ^= 0xF000;
            self.clr8[3].swap(0, 1);
        }
    }
    fn put_fill(&self, dst: &mut [u16], dstride: usize) {
        for line in dst.chunks_mut(dstride) {
            for i in 0..4 {
                line[i] = self.fill_val.0;
            }
        }
    }
    fn put_clr2(&self, dst: &mut [u16], dstride: usize) {
        for j in 0..4 {
            for i in 0..4 {
                if (self.clr2_flags & (1 << (i + j * 4))) == 0 {
                    dst[i + j * dstride] = self.clr2[0].0;
                } else {
                    dst[i + j * dstride] = self.clr2[1].0;
                }
            }
        }
    }
    fn put_clr8(&self, dst: &mut [u16], dstride: usize) {
        for i in 0..4 {
            let off = (i & 1) * 2 + (i & 2) * dstride;
            let cur_flg = (self.clr8_flags >> (i * 4)) & 0xF;
            dst[off]               = self.clr8[i][( !cur_flg       & 1) as usize].0;
            dst[off + 1]           = self.clr8[i][((!cur_flg >> 1) & 1) as usize].0;
            dst[off +     dstride] = self.clr8[i][((!cur_flg >> 2) & 1) as usize].0;
            dst[off + 1 + dstride] = self.clr8[i][((!cur_flg >> 3) & 1) as usize].0;
        }
    }
    fn write_fill(&self, bw: &mut ByteWriter) -> EncoderResult<()> {
        bw.write_u16le(self.fill_val.0 | 0x8000)?;
        Ok(())
    }
    fn write_clr2(&self, bw: &mut ByteWriter) -> EncoderResult<()> {
        bw.write_u16le(self.clr2_flags)?;
        bw.write_u16le(self.clr2[0].0)?;
        bw.write_u16le(self.clr2[1].0)?;
        Ok(())
    }
    fn write_clr8(&self, bw: &mut ByteWriter) -> EncoderResult<()> {
        bw.write_u16le(self.clr8_flags)?;
        bw.write_u16le(self.clr8[0][0].0 | 0x8000)?;
        bw.write_u16le(self.clr8[0][1].0)?;
        bw.write_u16le(self.clr8[1][0].0)?;
        bw.write_u16le(self.clr8[1][1].0)?;
        bw.write_u16le(self.clr8[2][0].0)?;
        bw.write_u16le(self.clr8[2][1].0)?;
        bw.write_u16le(self.clr8[3][0].0)?;
        bw.write_u16le(self.clr8[3][1].0)?;
        Ok(())
    }
}

struct MSVideo1Encoder {
    stream:     Option<NAStreamRef>,
    pkt:        Option<NAPacket>,
    pool:       NAVideoBufferPool<u16>,
    lastfrm:    Option<NAVideoBufferRef<u16>>,
    quality:    u8,
    frmcount:   u8,
    key_int:    u8,
}

impl MSVideo1Encoder {
    fn new() -> Self {
        Self {
            stream:     None,
            pkt:        None,
            pool:       NAVideoBufferPool::new(2),
            lastfrm:    None,
            quality:    0,
            frmcount:   0,
            key_int:    25,
        }
    }
    fn get_block(src: &[u16], sstride: usize, buf: &mut [Pixel16; 16]) {
        for (line, dst) in src.chunks(sstride).zip(buf.chunks_mut(4)) {
            for i in 0..4 {
                dst[i] = Pixel16 { 0: line[i] };
            }
        }
    }
    fn write_skips(bw: &mut ByteWriter, skips: usize) -> EncoderResult<()> {
        bw.write_u16le((skips as u16) | 0x8400)?;
        Ok(())
    }
    fn encode_inter(bw: &mut ByteWriter, cur_frm: &mut NAVideoBuffer<u16>, in_frm: &NAVideoBuffer<u16>, prev_frm: &NAVideoBuffer<u16>, _quality: u8) -> EncoderResult<bool> {
        let mut is_intra = true;
        let src = in_frm.get_data();
        let sstride = in_frm.get_stride(0);
        let soff = in_frm.get_offset(0);
        let (w, h) = in_frm.get_dimensions(0);
        let rsrc = prev_frm.get_data();
        let rstride = prev_frm.get_stride(0);
        let roff = prev_frm.get_offset(0);
        let dstride = cur_frm.get_stride(0);
        let doff = cur_frm.get_offset(0);
        let dst = cur_frm.get_data_mut().unwrap();
        let mut skip_run = 0;
        for ((sstrip, rstrip), dstrip) in (&src[soff..]).chunks(sstride * 4).take(h / 4).zip((&rsrc[roff..]).chunks(rstride * 4)).zip((&mut dst[doff..]).chunks_mut(dstride * 4)) {
            for x in (0..w).step_by(4) {
                let mut buf = [Pixel16::min_cw(); 16];
                let mut refbuf = [Pixel16::min_cw(); 16];
                Self::get_block(&sstrip[x..], sstride, &mut buf);
                Self::get_block(&rstrip[x..], rstride, &mut refbuf);

                let mut skip_dist = 0;
                for (pix, rpix) in buf.iter().zip(refbuf.iter()) {
                    skip_dist += pix.dist(*rpix);
                }
                if skip_dist == 0 {
                    skip_run += 1;
                    is_intra = false;
                    if skip_run == 1023 {
                        Self::write_skips(bw, skip_run)?;
                        skip_run = 0;
                    }
                    continue;
                }

                let mut bstate = BlockState::default();
                bstate.calc_stats(&buf);

                let dst = &mut dstrip[x..];
                if skip_dist <= bstate.fill_dist {
                    skip_run += 1;
                    is_intra = false;
                    if skip_run == 1023 {
                        Self::write_skips(bw, skip_run)?;
                        skip_run = 0;
                    }
                } else if bstate.fill_dist <= bstate.clr2_dist {
                    bstate.put_fill(dst, dstride);
                    if skip_run != 0 {
                        Self::write_skips(bw, skip_run)?;
                        skip_run = 0;
                    }
                    bstate.write_fill(bw)?;
                } else if bstate.clr8_dist < bstate.clr2_dist {
                    bstate.put_clr8(dst, dstride);
                    if skip_run != 0 {
                        Self::write_skips(bw, skip_run)?;
                        skip_run = 0;
                    }
                    bstate.write_clr8(bw)?;
                } else {
                    bstate.put_clr2(dst, dstride);
                    if skip_run != 0 {
                        Self::write_skips(bw, skip_run)?;
                        skip_run = 0;
                    }
                    bstate.write_clr2(bw)?;
                }
            }
        }
        if skip_run != 0 {
            Self::write_skips(bw, skip_run)?;
        }
        if is_intra {
            bw.write_u16le(0)?;
        } //xxx: something for inter?
        Ok(is_intra)
    }
    fn encode_intra(bw: &mut ByteWriter, cur_frm: &mut NAVideoBuffer<u16>, in_frm: &NAVideoBuffer<u16>, _quality: u8) -> EncoderResult<bool> {
        let src = in_frm.get_data();
        let sstride = in_frm.get_stride(0);
        let soff = in_frm.get_offset(0);
        let (w, h) = in_frm.get_dimensions(0);
        let dstride = cur_frm.get_stride(0);
        let doff = cur_frm.get_offset(0);
        let dst = cur_frm.get_data_mut().unwrap();
        for (sstrip, dstrip) in (&src[soff..]).chunks(sstride * 4).take(h / 4).zip((&mut dst[doff..]).chunks_mut(dstride * 4)) {
            for x in (0..w).step_by(4) {
                let mut buf = [Pixel16::min_cw(); 16];
                Self::get_block(&sstrip[x..], sstride, &mut buf);
                let mut bstate = BlockState::default();
                bstate.calc_stats(&buf);

                let dst = &mut dstrip[x..];
                if bstate.fill_dist <= bstate.clr2_dist {
                    bstate.put_fill(dst, dstride);
                    bstate.write_fill(bw)?;
                } else if bstate.clr8_dist < bstate.clr2_dist {
                    bstate.put_clr8(dst, dstride);
                    bstate.write_clr8(bw)?;
                } else {
                    bstate.put_clr2(dst, dstride);
                    bstate.write_clr2(bw)?;
                }
            }
        }
        bw.write_u16le(0)?;
        Ok(true)
    }
}

const RGB555_FORMAT: NAPixelFormaton = NAPixelFormaton {
        model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
        comp_info: [
            Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: 5, shift: 10, comp_offs: 0, next_elem: 2 }),
            Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: 5, shift:  5, comp_offs: 0, next_elem: 2 }),
            Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: 5, shift:  0, comp_offs: 0, next_elem: 2 }),
            None, None],
        elem_size: 2, be: false, alpha: false, palette: false };

impl NAEncoder for MSVideo1Encoder {
    fn negotiate_format(&self, encinfo: &EncodeParameters) -> EncoderResult<EncodeParameters> {
        match encinfo.format {
            NACodecTypeInfo::None => {
                let mut ofmt = EncodeParameters::default();
                ofmt.format = NACodecTypeInfo::Video(NAVideoInfo::new(0, 0, true, RGB555_FORMAT));
                Ok(ofmt)
            },
            NACodecTypeInfo::Audio(_) => Err(EncoderError::FormatError),
            NACodecTypeInfo::Video(vinfo) => {
                let outinfo = NAVideoInfo::new((vinfo.width + 3) & !3, (vinfo.height + 3) & !3, true, RGB555_FORMAT);
                let mut ofmt = *encinfo;
                ofmt.format = NACodecTypeInfo::Video(outinfo);
                Ok(ofmt)
            }
        }
    }
    fn init(&mut self, stream_id: u32, encinfo: EncodeParameters) -> EncoderResult<NAStreamRef> {
        match encinfo.format {
            NACodecTypeInfo::None => Err(EncoderError::FormatError),
            NACodecTypeInfo::Audio(_) => Err(EncoderError::FormatError),
            NACodecTypeInfo::Video(vinfo) => {
                if vinfo.format != RGB555_FORMAT {
                    return Err(EncoderError::FormatError);
                }
                if ((vinfo.width | vinfo.height) & 3) != 0 {
                    return Err(EncoderError::FormatError);
                }

                let out_info = NAVideoInfo::new(vinfo.width, vinfo.height, true, RGB555_FORMAT);
                let info = NACodecInfo::new("msvideo1", NACodecTypeInfo::Video(out_info), None);
                let mut stream = NAStream::new(StreamType::Video, stream_id, info, encinfo.tb_num, encinfo.tb_den, 0);
                stream.set_num(stream_id as usize);
                let stream = stream.into_ref();
                if self.pool.prealloc_video(out_info, 2).is_err() {
                    return Err(EncoderError::AllocError);
                }

                self.stream = Some(stream.clone());
                self.quality = encinfo.quality;

                Ok(stream)
            },
        }
    }
    fn encode(&mut self, frm: &NAFrame) -> EncoderResult<()> {
        let buf = frm.get_buffer();
        if let Some(ref vbuf) = buf.get_vbuf16() {
            let mut cur_frm = self.pool.get_free().unwrap();
            let mut dbuf = Vec::with_capacity(4);
            let mut gw   = GrowableMemoryWriter::new_write(&mut dbuf);
            let mut bw   = ByteWriter::new(&mut gw);
            if self.frmcount == 0 {
                self.lastfrm = None;
            }
            let is_intra = if let Some(ref prev_buf) = self.lastfrm {
                    Self::encode_inter(&mut bw, &mut cur_frm, vbuf, prev_buf, self.quality)?
                } else {
                    Self::encode_intra(&mut bw, &mut cur_frm, vbuf, self.quality)?
                };
            self.lastfrm = Some(cur_frm);
            self.pkt = Some(NAPacket::new(self.stream.clone().unwrap(), frm.ts, is_intra, dbuf));
            self.frmcount += 1;
            if self.frmcount == self.key_int {
                self.frmcount = 0;
            }
            Ok(())
        } else {
            Err(EncoderError::InvalidParameters)
        }
    }
    fn get_packet(&mut self) -> EncoderResult<Option<NAPacket>> {
        let mut npkt = None;
        std::mem::swap(&mut self.pkt, &mut npkt);
        Ok(npkt)
    }
    fn flush(&mut self) -> EncoderResult<()> {
        self.frmcount = 0;
        Ok(())
    }
}

const ENCODER_OPTS: &[NAOptionDefinition] = &[
    NAOptionDefinition {
        name: KEYFRAME_OPTION, description: KEYFRAME_OPTION_DESC,
        opt_type: NAOptionDefinitionType::Int(Some(0), Some(128)) },
];

impl NAOptionHandler for MSVideo1Encoder {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { ENCODER_OPTS }
    fn set_options(&mut self, options: &[NAOption]) {
        for option in options.iter() {
            for opt_def in ENCODER_OPTS.iter() {
                if opt_def.check(option).is_ok() {
                    match option.name {
                        KEYFRAME_OPTION => {
                            if let NAValue::Int(intval) = option.value {
                                self.key_int = intval as u8;
                            }
                        },
                        _ => {},
                    };
                }
            }
        }
    }
    fn query_option_value(&self, name: &str) -> Option<NAValue> {
        match name {
            KEYFRAME_OPTION => Some(NAValue::Int(i64::from(self.key_int))),
            _ => None,
        }
    }
}

pub fn get_encoder() -> Box<dyn NAEncoder + Send> {
    Box::new(MSVideo1Encoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::*;
    use nihav_core::demuxers::*;
    use nihav_core::muxers::*;
    use crate::*;
    use nihav_commonfmt::*;
    use nihav_codec_support::test::enc_video::*;
    use super::RGB555_FORMAT;

    #[test]
    fn test_ms_video1_encoder() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        generic_register_all_decoders(&mut dec_reg);
        ms_register_all_decoders(&mut dec_reg);
        let mut mux_reg = RegisteredMuxers::new();
        generic_register_all_muxers(&mut mux_reg);
        let mut enc_reg = RegisteredEncoders::new();
        ms_register_all_encoders(&mut enc_reg);

        let dec_config = DecoderTestParams {
                demuxer:        "avi",
                in_name:        "assets/Misc/TalkingHead_352x288.avi",
                stream_type:    StreamType::Video,
                limit:          Some(3),
                dmx_reg, dec_reg,
            };
        let enc_config = EncoderTestParams {
                muxer:          "avi",
                enc_name:       "msvideo1",
                out_name:       "msvideo1.avi",
                mux_reg, enc_reg,
            };
        let dst_vinfo = NAVideoInfo {
                width:   0,
                height:  0,
                format:  RGB555_FORMAT,
                flipped: true,
                bits:    16,
            };
        let enc_params = EncodeParameters {
                format:  NACodecTypeInfo::Video(dst_vinfo),
                quality: 0,
                bitrate: 0,
                tb_num:  0,
                tb_den:  0,
                flags:   0,
            };
        //test_encoding_to_file(&dec_config, &enc_config, enc_params, &[]);
        test_encoding_md5(&dec_config, &enc_config, enc_params, &[],
                          &[0x4339421d, 0x6393f1b6, 0x653d6cd2, 0x3a184382]);
    }
}
