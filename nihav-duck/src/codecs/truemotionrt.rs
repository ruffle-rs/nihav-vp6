use nihav_core::codecs::*;
use nihav_core::io::bitreader::*;

#[derive(Default)]
struct TMRTDecoder {
    info:       NACodecInfoRef,
}

const TMRT_DELTA_TAB: [&[i16]; 3] = [
    &[ 5, -7, 36, -36 ],
    &[ 2, -3, 8, -8, 18, -18, 36, -36 ],
    &[ 1, -1, 2, -3, 8, -8, 18, -18, 36, -36, 54, -54, 96, -96, 144, -144 ]
];

impl TMRTDecoder {
    fn new() -> Self { Self::default() }
    fn decode_plane(&self, br: &mut BitReader, dst: &mut [u8], mut off: usize, stride: usize, w: usize, h: usize, hscale: bool, dbits: u8, is_chroma: bool) -> DecoderResult<()> {
        let delta_tab = TMRT_DELTA_TAB[(dbits - 2) as usize];
        let step = if !hscale { 1 } else { 2 };
        for y in 0..h {
            let mut diff = 0;
            for x in (0..w).step_by(step) {
                let delta                       = delta_tab[br.read(dbits)? as usize];
                diff += delta;
                let pred = if y > 0 { dst[off + x - stride].into() } else if !is_chroma { 0 } else { 0x80 };
                dst[off + x] = (pred + diff).min(255).max(0) as u8;
                if hscale {
                    dst[off + x + 1] = dst[off + x];
                }
            }
            off += stride;
        }
        Ok(())
    }
}

impl NADecoder for TMRTDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), false, YUV410_FORMAT));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        validate!(src.len() > 10);
        let hdr_size = (src[0].rotate_left(3) & 0x7F) as usize;
        validate!(hdr_size >= 10 && hdr_size < src.len() - 4);
        let mut hdr: [u8; 127] = [0; 127];
        for i in 1..hdr_size {
            hdr[i - 1] = src[i] ^ src[i + 1];
        }
        let dbits  = hdr[1];
        validate!(dbits >= 2 && dbits <= 4);
        let hscale = hdr[3] != 0;
        let width  = (hdr[7] as usize) | ((hdr[8] as usize) << 8);
        let height = (hdr[5] as usize) | ((hdr[6] as usize) << 8);

        let myinfo = NAVideoInfo::new(width, height, false, YUV410_FORMAT);
        let bufret = alloc_video_buffer(myinfo, 2);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let bufinfo = bufret.unwrap();
        let mut buf = bufinfo.get_vbuf().unwrap();

        let mut br = BitReader::new(&src[hdr_size..], src.len() - hdr_size, BitReaderMode::LE);
        let size                                = br.read(32)? as usize;
        validate!(size <= src.len() - hdr_size);
        for plane in 0..3 {
            let (w, h)  = buf.get_dimensions(plane);
            let off     = buf.get_offset(plane);
            let stride  = buf.get_stride(plane);
            let data = buf.get_data_mut().unwrap();
            let dst = data.as_mut_slice();
            self.decode_plane(&mut br, dst, off, stride, w, h, hscale, dbits, plane > 0)?;
        }

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(true);
        frm.set_frame_type(FrameType::I);
        Ok(frm.into_ref())
    }
}

pub fn get_decoder() -> Box<dyn NADecoder> {
    Box::new(TMRTDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::duck_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;
    #[test]
    fn test_tmrt() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        test_file_decoding("avi", "assets/Duck/tr20_low.avi", Some(10), true, false, None/*Some("tmrt")*/, &dmx_reg, &dec_reg);
    }
}
