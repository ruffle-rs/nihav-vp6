use formats::*;
use super::*;

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

fn get_duration(ainfo: &NAAudioInfo, duration: Option<u64>, data_size: usize) -> usize {
println!("pcm in {:?}, {}", duration, data_size);
    if duration == None {
        let size_bits = data_size * 8;
        let blk_size = (ainfo.get_channels() as usize) * (ainfo.get_format().get_bits() as usize);
        size_bits / blk_size
    } else {
        duration.unwrap() as usize
    }
}

impl NADecoder for PCMDecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
println!("got info {}", ainfo);
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
println!("duration = {}", duration);
            let pktbuf = pkt.get_buffer();
            let mut buf: Vec<u8> = Vec::with_capacity(pktbuf.len());
            buf.clone_from(&pktbuf);
            let abuf = NAAudioBuffer::new_from_buf(ainfo, Rc::new(RefCell::new(buf)), self.chmap.clone());
            let mut frm = NAFrame::new_from_pkt(pkt, info, NABufferType::AudioPacked(abuf));
            frm.set_keyframe(true);
            Ok(Rc::new(RefCell::new(frm)))
        } else {
            Err(DecoderError::InvalidData)
        }
    }
}

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(PCMDecoder::new())
}
