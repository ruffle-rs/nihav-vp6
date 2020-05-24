use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use std::str::FromStr;

const ADAPT_TABLE: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 
    768, 614, 512, 409, 307, 230, 230, 230
];
const ADAPT_COEFFS: [[i32; 2]; 7] = [
    [ 256, 0 ], [ 512, -256 ], [ 0, 0 ], [ 192, 64 ],
    [ 240, 0 ], [ 460, -208 ], [ 392, -232 ]
];

#[derive(Default)]
struct Predictor {
    sample1:    i32,
    sample2:    i32,
    delta:      i32,
    coef1:      i32,
    coef2:      i32,
}

impl Predictor {
    fn expand_nibble(&mut self, nibble: u8) -> i16 {
        let mul = if (nibble & 8) == 0 { i32::from(nibble) } else { i32::from(nibble) - 16 };
        let pred = ((self.sample1.wrapping_mul(self.coef1) + self.sample2.wrapping_mul(self.coef2)) >> 8) + self.delta.wrapping_mul(mul);
        self.sample2 = self.sample1;
        self.sample1 = pred.max(-0x8000).min(0x7FFF);
        self.delta = (ADAPT_TABLE[nibble as usize].wrapping_mul(self.delta) >> 8).max(16);
        self.sample1 as i16
    }
}

struct MSADPCMDecoder {
    ainfo:          NAAudioInfo,
    chmap:          NAChannelMap,
    adapt_coeffs:   Vec<[i32; 2]>,
    block_len:      usize,
    block_samps:    usize,
}

impl MSADPCMDecoder {
    fn new() -> Self {
        Self {
            ainfo:          NAAudioInfo::new(0, 1, SND_S16P_FORMAT, 0),
            chmap:          NAChannelMap::new(),
            adapt_coeffs:   Vec::with_capacity(7),
            block_len:      0,
            block_samps:    0,
        }
    }
}

impl NADecoder for MSADPCMDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            self.block_len = ainfo.get_block_len();
            let channels = ainfo.get_channels() as usize;
            validate!(channels == 2 || channels == 1);
            validate!(self.block_len >= 7 * channels + 1);
            self.block_samps = (self.block_len / channels - 7) * 2 + 2;
            self.ainfo = NAAudioInfo::new(ainfo.get_sample_rate(), channels as u8, SND_S16P_FORMAT, self.block_samps);
            self.chmap = NAChannelMap::from_str(if channels == 1 { "C" } else { "L,R" }).unwrap();
            self.adapt_coeffs.truncate(0);
            if let Some(ref buf) = info.get_extradata() {
                validate!(buf.len() >= 6);
                validate!((buf.len() & 3) == 0);
                let mut mr = MemoryReader::new_read(buf.as_slice());
                let mut br = ByteReader::new(&mut mr);
                let _smth               = br.read_u16le()?;
                let ncoeffs             = br.read_u16le()? as usize;
                validate!(buf.len() == ncoeffs * 4 + 4);

                for _ in 0..ncoeffs {
                    let pair = [
                        i32::from(br.read_u16le()? as i16),
                        i32::from(br.read_u16le()? as i16)];
                    self.adapt_coeffs.push(pair);
                }
            } else {
                self.adapt_coeffs.extend_from_slice(&ADAPT_COEFFS);
            }
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        if let NACodecTypeInfo::Audio(_) = info.get_properties() {
            let pktbuf = pkt.get_buffer();
            let channels = self.chmap.num_channels();
            validate!(pktbuf.len() > 0 && (pktbuf.len() % self.block_len) == 0);
            let nblocks = pktbuf.len() / self.block_len;
            let nsamples = nblocks * self.block_samps;
            let abuf = alloc_audio_buffer(self.ainfo, nsamples, self.chmap.clone())?;
            let mut adata = abuf.get_abuf_i16().unwrap();
            let mut off = [adata.get_offset(0), adata.get_offset(1)];
            let dst = adata.get_data_mut().unwrap();

            let mut pred = [Predictor::default(), Predictor::default()];

            for blk in pktbuf.chunks(self.block_len) {
                let mut mr = MemoryReader::new_read(blk);
                let mut br = ByteReader::new(&mut mr);
                for ch in 0..channels {
                    let coef_idx                = br.read_byte()? as usize;
                    validate!(coef_idx < self.adapt_coeffs.len());
                    pred[ch].coef1 = self.adapt_coeffs[coef_idx][0];
                    pred[ch].coef2 = self.adapt_coeffs[coef_idx][1];
                }
                for ch in 0..channels {
                    pred[ch].delta              = i32::from(br.read_u16le()?);
                }
                for ch in 0..channels {
                    let samp                    = br.read_u16le()? as i16;
                    pred[ch].sample1            = i32::from(samp);
                    dst[off[ch]] = samp;
                    off[ch] += 1;
                }
                for ch in 0..channels {
                    let samp                    = br.read_u16le()? as i16;
                    pred[ch].sample2            = i32::from(samp);
                    dst[off[ch]] = samp;
                    off[ch] += 1;
                }
                if channels == 1 {
                    while br.left() > 0 {
                        let idx                 = br.read_byte()?;
                        dst[off[0]] = pred[0].expand_nibble(idx >> 4);
                        off[0] += 1;
                        dst[off[0]] = pred[0].expand_nibble(idx & 0xF);
                        off[0] += 1;
                    }
                } else {
                    while br.left() > 0 {
                        let idx                 = br.read_byte()?;
                        dst[off[0]] = pred[0].expand_nibble(idx >> 4);
                        off[0] += 1;
                        dst[off[1]] = pred[1].expand_nibble(idx & 0xF);
                        off[1] += 1;
                    }
                }
            }
            let mut frm = NAFrame::new_from_pkt(pkt, info.replace_info(NACodecTypeInfo::Audio(self.ainfo)), abuf);
            frm.set_duration(Some(nsamples as u64));
            frm.set_keyframe(false);
            Ok(frm.into_ref())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn flush(&mut self) {
    }
}

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(MSADPCMDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::ms_register_all_codecs;
    use nihav_commonfmt::generic_register_all_demuxers;
    #[test]
    fn test_ms_adpcm() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        ms_register_all_codecs(&mut dec_reg);

        test_decoding("avi", "ms-adpcm", "assets/MS/dance.avi", None, &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5([0x9d6619e1, 0x60d83560, 0xfe5c1fb7, 0xad5d130d]));
    }
}
