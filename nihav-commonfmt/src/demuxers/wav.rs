use nihav_core::demuxers::*;
use nihav_registry::register;
use nihav_core::demuxers::DemuxerError::*;

macro_rules! mktag {
    ($a:expr, $b:expr, $c:expr, $d:expr) => {
        (($a as u32) << 24) | (($b as u32) << 16) | (($c as u32) << 8) | ($d as u32)
    };
    ($arr:expr) => {
        (($arr[0] as u32) << 24) | (($arr[1] as u32) << 16) | (($arr[2] as u32) << 8) | ($arr[3] as u32)
    };
}

struct WAVDemuxer<'a> {
    src:            &'a mut ByteReader<'a>,
    data_pos:       u64,
    data_end:       u64,
    srate:          u32,
    block_size:     usize,
    is_pcm:         bool,
    avg_bytes:      u32,
}

impl<'a> DemuxCore<'a> for WAVDemuxer<'a> {
    fn open(&mut self, strmgr: &mut StreamManager, seek_index: &mut SeekIndex) -> DemuxerResult<()> {
        let riff                        = self.src.read_u32be()?;
        let riff_size                   = self.src.read_u32le()? as usize;
        let riff_end = self.src.tell() + if riff_size > 0 { (riff_size as u64) } else { u64::from(std::u32::MAX) };
        let wave                        = self.src.read_u32be()?;
        validate!(riff == mktag!(b"RIFF"));
        validate!(wave == mktag!(b"WAVE"));

        seek_index.mode = SeekIndexMode::Automatic;

        let mut fmt_parsed = false;
        let mut _duration = 0;
        while self.src.tell() < riff_end {
            let ctype                   = self.src.read_tag()?;
            let csize                   = self.src.read_u32le()? as usize;
            match &ctype {
                b"fmt " => {
                    validate!(!fmt_parsed);
                    self.parse_fmt(strmgr, csize)?;
                    fmt_parsed = true;
                },
                b"fact" => {
                    validate!(csize == 4);
                    _duration           = self.src.read_u32le()? as usize;
                },
                b"data" => {
                    validate!(fmt_parsed);
                    self.data_pos = self.src.tell();
                    self.data_end = self.data_pos + (csize as u64);
                    return Ok(());
                },
                _ => {
                                          self.src.read_skip(csize)?;
                },
            };
        }
        Err(DemuxerError::InvalidData)
    }

    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.src.tell() >= self.data_end {
            return Err(DemuxerError::EOF);
        }
        let str = strmgr.get_stream(0);
        if str.is_none() { return Err(InvalidData); }
        let stream = str.unwrap();
        let ts = NATimeInfo::new(None, None, None, 1, self.srate);
        if self.is_pcm {
            let mut bsize = self.block_size;
            while bsize < 256 {
                bsize <<= 1;
            }
            let mut buf = vec![0; bsize];
            let size                    = self.src.read_buf_some(buf.as_mut_slice())?;
            buf.truncate(size);
            Ok(NAPacket::new(stream, ts, true, buf))
        } else {
            self.src.read_packet(stream, ts, true, self.block_size)
        }
    }

    fn seek(&mut self, time: u64, _seek_index: &SeekIndex) -> DemuxerResult<()> {
        if self.block_size != 0 && self.avg_bytes != 0 {
            let seek_dst = u64::from(self.avg_bytes) * time / 1000;
            let seek_off = seek_dst / (self.block_size as u64) * (self.block_size as u64);
            self.src.seek(SeekFrom::Start(self.data_pos + seek_off))?;
            Ok(())
        } else {
            Err(DemuxerError::NotImplemented)
        }
    }
}

impl<'a> NAOptionHandler for WAVDemuxer<'a> {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { &[] }
    fn set_options(&mut self, _options: &[NAOption]) { }
    fn query_option_value(&self, _name: &str) -> Option<NAValue> { None }
}

impl<'a> WAVDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        WAVDemuxer {
            src:        io,
            data_pos:   0,
            data_end:   0,
            srate:      0,
            block_size: 0,
            is_pcm:     false,
            avg_bytes:  0,
        }
    }
    fn parse_fmt(&mut self, strmgr: &mut StreamManager, csize: usize) -> DemuxerResult<()> {
        validate!(csize >= 14);
        let format_tag                  = self.src.read_u16le()?;
        let channels                    = self.src.read_u16le()?;
        validate!(channels < 256);
        let samples_per_sec             = self.src.read_u32le()?;
        let avg_bytes_per_sec           = self.src.read_u32le()?;
        let block_align                 = self.src.read_u16le()? as usize;
        if block_align == 0 {
            return Err(DemuxerError::NotImplemented);
        }
        let bits_per_sample             = if csize >= 16 { self.src.read_u16le()? } else { 8 };
        validate!(channels < 256);

        let edata;
        if csize > 16 {
            validate!(csize >= 18);
            let cb_size                 = self.src.read_u16le()? as usize;
            let mut buf = vec![0; cb_size];
                                          self.src.read_buf(buf.as_mut_slice())?;
            edata = Some(buf);
        } else {
            edata = None;
        }

        let cname = register::find_codec_from_wav_twocc(format_tag).unwrap_or("unknown");
        let soniton = if cname == "pcm" {
                if format_tag != 0x0003 {
                    if bits_per_sample == 8 {
                        NASoniton::new(8, 0)
                    } else {
                        NASoniton::new(bits_per_sample as u8, SONITON_FLAG_SIGNED)
                    }
                } else {
                    NASoniton::new(bits_per_sample as u8, SONITON_FLAG_FLOAT)
                }
            } else {
                NASoniton::new(bits_per_sample as u8, SONITON_FLAG_SIGNED)
            };
        let ahdr = NAAudioInfo::new(samples_per_sec, channels as u8, soniton, block_align);
        let ainfo = NACodecInfo::new(cname, NACodecTypeInfo::Audio(ahdr), edata);
        let res = strmgr.add_stream(NAStream::new(StreamType::Audio, 0, ainfo, 1, samples_per_sec));
        if res.is_none() { return Err(MemoryError); }

        self.srate = samples_per_sec;
        self.block_size = block_align;
        self.avg_bytes = avg_bytes_per_sec;
        self.is_pcm = cname == "pcm";

        Ok(())
    }
}

pub struct WAVDemuxerCreator { }

impl DemuxerCreator for WAVDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a> {
        Box::new(WAVDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "wav" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_wav_demux() {
        let mut file = File::open("assets/MS/scatter.wav").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = WAVDemuxer::new(&mut br);
        let mut sm = StreamManager::new();
        let mut si = SeekIndex::new();
        dmx.open(&mut sm, &mut si).unwrap();

        loop {
            let pktres = dmx.get_frame(&mut sm);
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
                panic!("error");
            }
            let pkt = pktres.unwrap();
            println!("Got {}", pkt);
        }
    }
}
