use nihav_core::formats::*;
use nihav_core::codecs::*;

struct PCMDecoder { chmap: NAChannelMap }

impl PCMDecoder {
    fn new() -> Self {
        PCMDecoder { chmap: NAChannelMap::new() }
    }
}

const CHMAP_MONO: [NAChannelType; 1] = [NAChannelType::C];
const CHMAP_STEREO: [NAChannelType; 2] = [NAChannelType::L, NAChannelType::R];

fn get_default_chmap(nch: u8) -> NAChannelMap {
    let mut chmap = NAChannelMap::new();
    match nch {
        1 => chmap.add_channels(&CHMAP_MONO),
        2 => chmap.add_channels(&CHMAP_STEREO),
        _ => (),
    }
    chmap
}

fn get_duration(ainfo: &NAAudioInfo, duration: Option<u64>, data_size: usize) -> u64 {
    if duration == None {
        let size_bits = (data_size as u64) * 8;
        let blk_size = (ainfo.get_channels() as u64) * (ainfo.get_format().get_bits() as u64);
        size_bits / blk_size
    } else {
        duration.unwrap() as u64
    }
}

impl NADecoder for PCMDecoder {
    fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            self.chmap = get_default_chmap(ainfo.get_channels());
            if self.chmap.num_channels() == 0 { return Err(DecoderError::InvalidData); }
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            let duration = get_duration(&ainfo, pkt.get_duration(), pkt.get_buffer().len());
            let pktbuf = pkt.get_buffer();
            let abuf = NAAudioBuffer::new_from_buf(ainfo, pktbuf, self.chmap.clone());
            let mut frm = NAFrame::new_from_pkt(pkt, info, NABufferType::AudioPacked(abuf));
            frm.set_duration(Some(duration));
            frm.set_keyframe(true);
            Ok(frm.into_ref())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
}

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(PCMDecoder::new())
}
