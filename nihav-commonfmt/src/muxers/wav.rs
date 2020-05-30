use nihav_core::muxers::*;
use nihav_registry::register::*;

struct WAVMuxer<'a> {
    bw:         &'a mut ByteWriter<'a>,
    data_pos:   u64,
}

impl<'a> WAVMuxer<'a> {
    fn new(bw: &'a mut ByteWriter<'a>) -> Self {
        Self {
            bw,
            data_pos:   0,
        }
    }
}

fn patch_size(bw: &mut ByteWriter, pos: u64) -> MuxerResult<()> {
    let size = bw.tell() - pos;
    bw.seek(SeekFrom::Current(-((size + 4) as i64)))?;
    bw.write_u32le(size as u32)?;
    bw.seek(SeekFrom::End(0))?;
    Ok(())
}

impl<'a> MuxCore<'a> for WAVMuxer<'a> {
    fn create(&mut self, strmgr: &StreamManager) -> MuxerResult<()> {
        if strmgr.get_num_streams() != 1 {
            return Err(MuxerError::InvalidArgument);
        }

        let stream = strmgr.get_stream(0).unwrap();

        if stream.get_info().get_properties().get_audio_info().is_none() {
            return Err(MuxerError::InvalidArgument);
        }
        let ainfo = stream.get_info().get_properties().get_audio_info().unwrap();

        let edata_len = if let Some(ref buf) = stream.get_info().get_extradata() { buf.len() } else { 0 };
        if edata_len >= (1 << 16) {
            return Err(MuxerError::UnsupportedFormat);
        }

        let twocc = find_wav_twocc(stream.get_info().get_name());
        if twocc.is_none() {
            return Err(MuxerError::UnsupportedFormat);
        }
        let twocc = if stream.get_info().get_name() == "pcm" {
                if !ainfo.format.float { 0x0001 } else { 0x0003 }
            } else {
                twocc.unwrap_or(0)
            };
        let avg_bytes_per_sec = if stream.get_info().get_name() == "pcm" {
                u32::from(ainfo.channels) * ainfo.sample_rate * u32::from(ainfo.format.bits) >> 3
            } else {
                0
            };

        self.bw.write_buf(b"RIFF\0\0\0\0WAVEfmt ")?;
        self.bw.write_u32le(if edata_len == 0 { 16 } else { 18 + edata_len } as u32)?;
        self.bw.write_u16le(twocc)?;
        self.bw.write_u16le(ainfo.channels as u16)?;
        self.bw.write_u32le(ainfo.sample_rate)?;
        self.bw.write_u32le(avg_bytes_per_sec)?;
        self.bw.write_u16le(ainfo.block_len as u16)?;
        self.bw.write_u16le(ainfo.format.bits as u16)?;
        if let Some(ref buf) = stream.get_info().get_extradata() {
            self.bw.write_u16le(edata_len as u16)?;
            self.bw.write_buf(buf.as_slice())?;
        }
        self.bw.write_buf(b"data\0\0\0\0")?;
        self.data_pos = self.bw.tell();

        Ok(())
    }
    fn mux_frame(&mut self, _strmgr: &StreamManager, pkt: NAPacket) -> MuxerResult<()> {
        if self.data_pos == 0 {
            return Err(MuxerError::NotCreated);
        }

        let stream = pkt.get_stream();
        if stream.get_num() != 0 {
            return Err(MuxerError::UnsupportedFormat);
        }

        self.bw.write_buf(pkt.get_buffer().as_slice())?;
        Ok(())
    }
    fn flush(&mut self) -> MuxerResult<()> {
        Ok(())
    }
    fn end(&mut self) -> MuxerResult<()> {
        patch_size(&mut self.bw, self.data_pos)?;
        patch_size(&mut self.bw, 8)?;
        // todo patch avg_bytes_per_second if calculated
        // todo write fact value if calculated
        Ok(())
    }
}

pub struct WAVMuxerCreator {}

impl MuxerCreator for WAVMuxerCreator {
    fn new_muxer<'a>(&self, bw: &'a mut ByteWriter<'a>) -> Box<dyn MuxCore<'a> + 'a> {
        Box::new(WAVMuxer::new(bw))
    }
    fn get_name(&self) -> &'static str { "wav" }
    fn get_capabilities(&self) -> MuxerCapabilities { MuxerCapabilities::SingleAudio("any") }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use nihav_core::demuxers::*;
    use crate::demuxers::*;

    #[test]
    fn test_wav_muxer() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut file = File::open("assets/Indeo/laser05.avi").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let dmx_f = dmx_reg.find_demuxer("avi").unwrap();
        let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

        let mut out_sm = StreamManager::new();
        let mut out_streamno = 0;
        for stream in dmx.get_streams() {
            if stream.get_media_type() == StreamType::Audio {
                let mut stream = NAStream::clone(&stream);
                out_streamno = stream.id;
                stream.id = 0;
                out_sm.add_stream(stream);
            }
        }

        let ofile = File::create("assets/test_out/muxed.wav").unwrap();
        let mut fw = FileWriter::new_write(ofile);
        let mut bw = ByteWriter::new(&mut fw);
        let mut mux = WAVMuxer::new(&mut bw);

        mux.create(&out_sm).unwrap();

        loop {
            let pktres = dmx.get_frame();
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
                panic!("error");
            }
            let mut pkt = pktres.unwrap();
            println!("Got {}", pkt);
            let pkt_str = pkt.get_stream();
            if pkt_str.id == out_streamno {
                pkt.reassign(out_sm.get_stream(0).unwrap(), pkt.get_time_information());
                mux.mux_frame(&out_sm, pkt).unwrap();
            }
        }

        mux.end().unwrap();
panic!("end");
    }
}
