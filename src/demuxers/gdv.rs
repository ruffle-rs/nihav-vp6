use super::*;
//use io::byteio::*;
//use frame::*;
use formats::*;
//use std::collections::HashMap;

enum GDVState {
    NewFrame,
    AudioRead,
}

#[allow(dead_code)]
struct GremlinVideoDemuxer<'a> {
    src:        &'a mut ByteReader<'a>,
    frames:     u16,
    cur_frame:  u16,
    asize:      usize,
    apacked:    bool,
    state:      GDVState,
    pktdta:     Vec<u8>,
    a_id:       Option<usize>,
    v_id:       Option<usize>,
}

struct GDVFixedSizes {
    id:     u16,
    width:  u16,
    height: u16,
}
const GDV_SIZE_TABLE: &[GDVFixedSizes] = &[
    GDVFixedSizes { id:  0, width: 320, height: 200 },
    GDVFixedSizes { id:  1, width: 640, height: 200 },
    GDVFixedSizes { id:  2, width: 320, height: 167 },
    GDVFixedSizes { id:  3, width: 320, height: 180 },
    GDVFixedSizes { id:  4, width: 320, height: 400 },
    GDVFixedSizes { id:  5, width: 320, height: 170 },
    GDVFixedSizes { id:  6, width: 160, height:  85 },
    GDVFixedSizes { id:  7, width: 160, height:  83 },
    GDVFixedSizes { id:  8, width: 160, height:  90 },
    GDVFixedSizes { id:  9, width: 280, height: 128 },
    GDVFixedSizes { id: 10, width: 320, height: 240 },
    GDVFixedSizes { id: 11, width: 320, height: 201 },
    GDVFixedSizes { id: 16, width: 640, height: 400 },
    GDVFixedSizes { id: 17, width: 640, height: 200 },
    GDVFixedSizes { id: 18, width: 640, height: 180 },
    GDVFixedSizes { id: 19, width: 640, height: 167 },
    GDVFixedSizes { id: 20, width: 640, height: 170 },
    GDVFixedSizes { id: 21, width: 320, height: 240 },
];

impl<'a> DemuxCore<'a> for GremlinVideoDemuxer<'a> {
    #[allow(unused_variables)]
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let src = &mut self.src;
        let magic = src.read_u32le()?;
        if magic != 0x29111994 { return Err(DemuxerError::InvalidData); }
        let id = src.read_u16le()?;
        let frames = src.read_u16le()?;
        let fps = src.read_u16le()?;
        let aflags = src.read_u16le()?;
        let rate = src.read_u16le()?;
        let depth = src.read_u16le()?;
        let max_fs = src.read_u16le()?;
        src.read_skip(2)?;
        let mut width = src.read_u16le()?;
        let mut height = src.read_u16le()?;
        if (width == 0) && (height == 0) {
            for el in GDV_SIZE_TABLE {
                if el.id == id {
                    width  = el.width;
                    height = el.height;
                    break;
                }
            }
            if (width == 0) && (height == 0) { return Err(DemuxerError::InvalidData); }
        }
        if max_fs > 0 {
            let mut edata: Vec<u8> = Vec::with_capacity(768);
            if depth == 1 {
                edata.resize(768, 0);
                src.read_buf(edata.as_mut_slice())?;
            }
            let vhdr = NAVideoInfo::new(width as usize, height as usize, false, PAL8_FORMAT);
            let vci = NACodecTypeInfo::Video(vhdr);
            let vinfo = NACodecInfo::new("gdv-video", vci, if edata.len() == 0 { None } else { Some(edata) });
            self.v_id = strmgr.add_stream(NAStream::new(StreamType::Video, 0, vinfo, 1, fps as u32));
        }
        if (aflags & 1) != 0 {
            let channels = if (aflags & 2) != 0 { 2 } else { 1 };
            let packed   = if (aflags & 8) != 0 { 1 } else { 0 };
            let depth    = if (aflags & 4) != 0 { 16 } else { 8 };

            let ahdr = NAAudioInfo::new(rate as u32, channels as u8, if depth == 16 { SND_S16_FORMAT } else { SND_U8_FORMAT }, 2);
            let ainfo = NACodecInfo::new(if packed != 0 { "gdv-audio" } else { "pcm" },
                                         NACodecTypeInfo::Audio(ahdr), None);
            self.a_id = strmgr.add_stream(NAStream::new(StreamType::Audio, 1, ainfo, 1, rate as u32));

            self.asize = (((rate / fps) * channels * (depth / 8)) >> packed) as usize;
            self.apacked = (aflags & 8) != 0;
        }
        self.frames = frames;
        self.state = GDVState::NewFrame;
        Ok(())
    }

    #[allow(unused_variables)]
    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.cur_frame >= self.frames { return Err(DemuxerError::EOF); }
        match self.state {
            GDVState::NewFrame if self.asize > 0 => { self.read_achunk(strmgr) }
            _ => { self.read_vchunk(strmgr) }
        }
    }

    #[allow(unused_variables)]
    fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        Err(DemuxerError::NotImplemented)
    }
}
/*impl<'a> Drop for GremlinVideoDemuxer<'a> {
    #[allow(unused_variables)]
    fn drop(&mut self) {
    }
}*/
impl<'a> GremlinVideoDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        GremlinVideoDemuxer {
            cur_frame: 0,
            frames: 0,
            asize: 0,
            apacked: false,
            state: GDVState::NewFrame,
pktdta: Vec::new(),
            src: io,
            a_id: None,
            v_id: None,
        }
    }

    fn read_achunk(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        self.state = GDVState::AudioRead;
        let str = strmgr.get_stream(self.a_id.unwrap()).unwrap();
        let (tb_num, tb_den) = str.get_timebase();
        let ts = NATimeInfo::new(Some(self.cur_frame as u64), None, None, tb_num, tb_den);
        self.src.read_packet(str, ts, true, self.asize)
    }

    fn read_vchunk(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        let str = strmgr.get_stream(self.v_id.unwrap()).unwrap();
        let mut src = &mut self.src;
        let magic = src.read_u16be()?;
        if magic != 0x0513 { return Err(DemuxerError::InvalidData); }
        let size = (src.read_u16le()? as usize) + 4;
        let tmp = src.peek_u32le()?;
        let flags = (tmp & 0xFF) as usize;
        self.state = GDVState::NewFrame;
        self.cur_frame = self.cur_frame + 1;
        let (tb_num, tb_den) = str.get_timebase();
        let ts = NATimeInfo::new(Some((self.cur_frame - 1) as u64), None, None, tb_num, tb_den);
        src.read_packet(str, ts, if (flags & 64) != 0 { true } else { false }, size)
    }
}

pub struct GDVDemuxerCreator { }

impl DemuxerCreator for GDVDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<DemuxCore<'a> + 'a> {
        Box::new(GremlinVideoDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "gdv" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_gdv_demux() {
        let mut file = File::open("assets/intro1.gdv").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = GremlinVideoDemuxer::new(&mut br);
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
