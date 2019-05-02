use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use std::str::FromStr;

const IMA_MAX_STEP: u8 = 88;
struct IMAState {
    predictor:  i32,
    step:       usize,
}

impl IMAState {
    fn new() -> Self {
        Self {
            predictor:  0,
            step:       0,
        }
    }
    fn reset(&mut self, predictor: i16, step: u8) {
        self.predictor  = predictor as i32;
        self.step       = step.min(IMA_MAX_STEP) as usize;
    }
    fn expand_sample(&mut self, nibble: u8) -> i16 {
        let istep = (self.step as isize) + (IMA_STEPS[nibble as usize] as isize);
        let sign = (nibble & 8) != 0;
        let diff = (((2 * (nibble & 7) + 1) as i32) * IMA_STEP_TABLE[self.step]) >> 3;
        let sample = if !sign { self.predictor + diff } else { self.predictor - diff };
        self.predictor = sample.max(std::i16::MIN as i32).min(std::i16::MAX as i32);
        self.step = istep.max(0).min(IMA_MAX_STEP as isize) as usize;
        self.predictor as i16
    }
}

struct DuckADPCMDecoder {
    ainfo:      NAAudioInfo,
    chmap:      NAChannelMap,
    is_dk3:     bool,
    ch_state:   [IMAState; 2],
    block_len:  usize,
}

impl DuckADPCMDecoder {
    fn new(is_dk3: bool) -> Self {
        Self {
            ainfo:      NAAudioInfo::new(0, 1, SND_S16P_FORMAT, 0),
            chmap:      NAChannelMap::new(),
            is_dk3,
            ch_state:   [IMAState::new(), IMAState::new()],
            block_len:  0,
        }
    }
}

impl NADecoder for DuckADPCMDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            validate!(ainfo.get_block_len() > 16);
            self.block_len = ainfo.get_block_len();
            let channels = ainfo.get_channels();
            validate!(channels == 2 || (!self.is_dk3 && channels == 1));
            let len = if self.is_dk3 {
                    ((self.block_len - 16) * 2 / 3) * 2
                } else {
                    (self.block_len - 4 * (channels as usize)) * 2 / (channels as usize)
                };
            self.ainfo = NAAudioInfo::new(ainfo.get_sample_rate(), channels, SND_S16P_FORMAT, len);
            self.chmap = NAChannelMap::from_str(if channels == 1 { "C" } else { "L,R" }).unwrap();
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        if let NACodecTypeInfo::Audio(_) = info.get_properties() {
            let pktbuf = pkt.get_buffer();
            validate!(pktbuf.len() > (if self.is_dk3 { 16 } else { 4 * self.chmap.num_channels() }));
            let nblocks = pktbuf.len() / self.block_len;
            let out_block_len = self.ainfo.get_block_len();
            let duration = out_block_len * nblocks;
            let abuf = alloc_audio_buffer(self.ainfo, duration, self.chmap.clone())?;
            let mut adata = abuf.get_abuf_i16().unwrap();
            let mut off0 = adata.get_offset(0);
            let mut off1 = adata.get_offset(1);
            let dst = adata.get_data_mut().unwrap();

            for blk in pktbuf.chunks_exact(self.block_len) {
                let mut mr = MemoryReader::new_read(blk);
                let mut br = ByteReader::new(&mut mr);
                if self.is_dk3 {
                    let _typeid                 = br.read_byte()?;
                    let _version                = br.read_byte()?;
                    let _srate                  = br.read_u32le()?;
                    let samples                 = br.read_u32le()? as usize;
                    let sumpred                 = br.read_u16le()? as i16;
                    let diffpred                = br.read_u16le()? as i16;
                    let sumstep                 = br.read_byte()?;
                    let diffstep                = br.read_byte()?;
                    validate!(sumstep <= IMA_MAX_STEP && diffstep <= IMA_MAX_STEP);
                    validate!(samples <= out_block_len);
                    self.ch_state[0].reset(sumpred,  sumstep);
                    self.ch_state[1].reset(diffpred, diffstep);
                    let mut last_nib = 0;
                    let mut diff_val: i32 = diffpred as i32;
                    for x in (0..out_block_len).step_by(2) {
                        let nib0;
                        let nib1;
                        let nib2;
                        if (x & 2) == 0 {
                            let b0              = br.read_byte()?;
                            let b1              = br.read_byte()?;
                            nib0 = b0 & 0xF;
                            nib1 = b0 >> 4;
                            nib2 = b1 & 0xF;
                            last_nib = b1 >> 4;
                        } else {
                            let b0              = br.read_byte()?;
                            nib0 = last_nib;
                            nib1 = b0 & 0xF;
                            nib2 = b0 >> 4;
                        }
                        let sum0 = self.ch_state[0].expand_sample(nib0) as i32;
                        let diff = self.ch_state[1].expand_sample(nib1) as i32;
                        let sum1 = self.ch_state[0].expand_sample(nib2) as i32;
                        diff_val = (diff_val + diff) >> 1;
                        dst[off0 + x + 0] = (sum0 + diff_val) as i16;
                        dst[off1 + x + 0] = (sum0 - diff_val) as i16;
                        diff_val = (diff_val + diff) >> 1;
                        dst[off0 + x + 1] = (sum1 + diff_val) as i16;
                        dst[off1 + x + 1] = (sum1 - diff_val) as i16;
                        diff_val = diff;
                    }
                } else {
                    let nchannels = self.chmap.num_channels();
                    for ch in 0..nchannels {
                        let pred                = br.read_u16le()? as i16;
                        let step                = br.read_byte()?;
                                                  br.read_skip(1)?;
                        validate!(step <= IMA_MAX_STEP);
                        self.ch_state[ch].reset(pred, step);
                    }
                    if nchannels == 2 {
                        for x in 0..out_block_len {
                            let b               = br.read_byte()?;
                            dst[off0 + x] = self.ch_state[0].expand_sample(b >> 4);
                            dst[off1 + x] = self.ch_state[1].expand_sample(b & 0xF);
                        }
                    } else {
                        for x in (0..out_block_len).step_by(2) {
                            let b               = br.read_byte()?;
                            dst[off0 + x + 0] = self.ch_state[0].expand_sample(b >> 4);
                            dst[off0 + x + 1] = self.ch_state[0].expand_sample(b & 0xF);
                        }
                    }
                }
                off0 += out_block_len;
                off1 += out_block_len;
            }
            let mut frm = NAFrame::new_from_pkt(pkt, info, abuf);
            frm.set_duration(Some(duration as u64));
            frm.set_keyframe(false);
            Ok(frm.into_ref())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
}

pub fn get_decoder_dk3() -> Box<NADecoder> {
    Box::new(DuckADPCMDecoder::new(true))
}

pub fn get_decoder_dk4() -> Box<NADecoder> {
    Box::new(DuckADPCMDecoder::new(false))
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::duck_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;
    #[test]
    fn test_dk3() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        let file = "assets/Duck/AVI-DUCK-dk3.duk";
        test_decode_audio("avi", file, Some(100), "dk3", &dmx_reg, &dec_reg);
    }
    #[test]
    fn test_dk4() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        let file = "assets/Duck/virtuafighter2-opening1.avi";
        test_decode_audio("avi", file, Some(100), "dk4", &dmx_reg, &dec_reg);
    }
}

const IMA_STEPS: [i8; 16] = [
    -1, -1, -1, -1, 2, 4, 6, 8,
    -1, -1, -1, -1, 2, 4, 6, 8
];

const IMA_STEP_TABLE: [i32; 89] = [
        7,     8,     9,    10,    11,    12,    13,    14,
       16,    17,    19,    21,    23,    25,    28,    31,
       34,    37,    41,    45,    50,    55,    60,    66,
       73,    80,    88,    97,   107,   118,   130,   143,
      157,   173,   190,   209,   230,   253,   279,   307,
      337,   371,   408,   449,   494,   544,   598,   658,
      724,   796,   876,   963,  1060,  1166,  1282,  1411,
     1552,  1707,  1878,  2066,  2272,  2499,  2749,  3024,
     3327,  3660,  4026,  4428,  4871,  5358,  5894,  6484,
     7132,  7845,  8630,  9493, 10442, 11487, 12635, 13899,
    15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767
];
