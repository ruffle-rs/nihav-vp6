use nihav_core::frame::*;
use nihav_core::demuxers::*;

struct BMVDemuxer<'a> {
    src:        &'a mut ByteReader<'a>,
    vid_id:     usize,
    aud_id:     usize,
    vpos:       u64,
    apos:       u64,
    pkt_buf:    Vec<NAPacket>,
}

impl<'a> DemuxCore<'a> for BMVDemuxer<'a> {
    #[allow(unused_variables)]
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let src = &mut self.src;

        let vhdr = NAVideoInfo::new(640, 429, false, PAL8_FORMAT);
        let vci = NACodecTypeInfo::Video(vhdr);
        let vinfo = NACodecInfo::new("bmv-video", vci, None);
        self.vid_id = strmgr.add_stream(NAStream::new(StreamType::Video, 0, vinfo, 1, 12)).unwrap();

        let ahdr = NAAudioInfo::new(22050, 2, SND_S16_FORMAT, 1);
        let ainfo = NACodecInfo::new("bmv-audio", NACodecTypeInfo::Audio(ahdr), None);
        self.aud_id = strmgr.add_stream(NAStream::new(StreamType::Audio, 1, ainfo, 1, 22050)).unwrap();

        self.vpos       = 0;
        self.apos       = 0;
        Ok(())
    }

    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.pkt_buf.len() > 0 {
            return Ok(self.pkt_buf.pop().unwrap());
        }

        loop {
            let ctype                   = self.src.read_byte()?;
            if ctype == 0 { // NOP chunk
                continue;
            }
            if ctype == 1 { return Err(DemuxerError::EOF); }
            let size                    = self.src.read_u24le()? as usize;
            validate!(size > 0);
            let asize;
            if (ctype & 0x20) != 0 {
                let nblocks             = self.src.peek_byte()?;
                asize = (nblocks as usize) * 65 + 1;
                validate!(asize < size);
                let str = strmgr.get_stream(self.aud_id).unwrap();
                let (tb_num, tb_den) = str.get_timebase();
                let ts = NATimeInfo::new(Some(self.apos), None, None, tb_num, tb_den);
                let apkt = self.src.read_packet(str, ts, false, asize)?;
                self.apos += (nblocks as u64) * 32;
                self.pkt_buf.push(apkt);
            } else {
                asize = 0;
            }
            let mut buf: Vec<u8> = Vec::with_capacity(size - asize + 1);
            buf.resize(size - asize + 1, 0);
            buf[0] = ctype;
            self.src.read_buf(&mut buf[1..])?;

            let str = strmgr.get_stream(self.vid_id).unwrap();
            let (tb_num, tb_den) = str.get_timebase();
            let ts = NATimeInfo::new(Some(self.vpos), None, None, tb_num, tb_den);
            let pkt = NAPacket::new(str, ts, (ctype & 3) == 3, buf);

            self.vpos += 1;
            return Ok(pkt);
        }
    }

    #[allow(unused_variables)]
    fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        Err(DemuxerError::NotImplemented)
    }
}

impl<'a> BMVDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        Self {
            src:        io,
            vid_id:     0,
            aud_id:     0,
            vpos:       0,
            apos:       0,
            pkt_buf:    Vec::with_capacity(1),
        }
    }
}

pub struct BMVDemuxerCreator { }

impl DemuxerCreator for BMVDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<DemuxCore<'a> + 'a> {
        Box::new(BMVDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "bmv" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_bmv_demux() {
        let mut file = File::open("assets/Game/DW2-MOUSE.BMV").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = BMVDemuxer::new(&mut br);
        let mut sm = StreamManager::new();
        dmx.open(&mut sm).unwrap();
        loop {
            let pktres = dmx.get_frame(&mut sm);
            if let Err(e) = pktres {
                if (e as i32) == (DemuxerError::EOF as i32) { break; }
                panic!("error");
            }
            let pkt = pktres.unwrap();
            println!("Got {}", pkt);
        }
    }
}
