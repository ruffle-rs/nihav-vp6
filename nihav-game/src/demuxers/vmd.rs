use nihav_core::frame::*;
use nihav_core::demuxers::*;
use std::io::SeekFrom;

const HEADER_SIZE: usize = 0x330;
const FRAME_HDR_SIZE: usize = 10;

const CHTYPE_VIDEO: u8 = 0x02;
const CHTYPE_AUDIO: u8 = 0x01;

#[derive(Clone,Copy)]
struct FrameRec {
    chtype:     u8,
    size:       u32,
    off:        u32,
    hdr:        [u8; FRAME_HDR_SIZE],
    ts:         u32,
}

struct VMDDemuxer<'a> {
    src:        &'a mut ByteReader<'a>,
    vid_id:     usize,
    aud_id:     usize,
    fno:        usize,
    is_indeo:   bool,
    frames:     Vec<FrameRec>,
}

impl<'a> DemuxCore<'a> for VMDDemuxer<'a> {
    #[allow(unused_variables)]
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let src = &mut self.src;

        let mut header: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
                                                src.read_buf(&mut header)?;

        let mut width  = read_u16le(&header[12..])? as usize;
        let mut height = read_u16le(&header[14..])? as usize;
        self.is_indeo = &header[24..27] == b"iv3";
        if self.is_indeo && width > 320 {
            width  >>= 1;
            height >>= 1;
        }

        let nframes = read_u16le(&header[6..])? as usize;
        let fpb     = read_u16le(&header[18..])? as usize;
        validate!(nframes > 0 && fpb > 0);
        
        let mut edata: Vec<u8> = Vec::with_capacity(HEADER_SIZE);
        edata.extend_from_slice(&header);
        let vhdr = NAVideoInfo::new(width, height, false, PAL8_FORMAT);
        let vci = NACodecTypeInfo::Video(vhdr);
        let vinfo = NACodecInfo::new(if !self.is_indeo { "vmd-video" } else { "indeo3" }, vci, Some(edata));
        self.vid_id = strmgr.add_stream(NAStream::new(StreamType::Video, 0, vinfo, 1, 12)).unwrap();

        let srate = read_u16le(&header[804..])? as u32;
        let block_size;
        if srate > 0 {
            let bsize = read_u16le(&header[806..])? as usize;
            let channels = if (header[811] & 0x8F) != 0 { 2 } else { 1 };
            let is16bit;
            if (bsize & 0x8000) != 0 {
                is16bit = true;
                block_size = 0x10000 - bsize;
            } else {
                is16bit = false;
                block_size = bsize;
            }

            let ahdr = NAAudioInfo::new(srate, channels, if is16bit { SND_S16P_FORMAT } else { SND_U8_FORMAT }, block_size);
            let ainfo = NACodecInfo::new("vmd-audio", NACodecTypeInfo::Audio(ahdr), None);
            self.aud_id = strmgr.add_stream(NAStream::new(StreamType::Audio, 1, ainfo, 1, srate)).unwrap();
        } else {
            block_size = 0;
        }

        let adelay  = read_u16le(&header[808..])? as u32;
        let idx_off = read_u32le(&header[812..])? as u64;
                                                src.seek(SeekFrom::Start(idx_off))?;
        let mut offs: Vec<u32> = Vec::with_capacity(nframes);
        for i in 0..nframes {
            let _flags                          = src.read_u16le()?;
            let off                             = src.read_u32le()?;
            offs.push(off);
        }
        self.frames.reserve(nframes * fpb);
        let mut ats = adelay;
        for i in 0..nframes {
            let mut off = offs[i];
            for _ in 0..fpb {
                let chtype                      = src.read_byte()?;
                                                  src.read_skip(1)?;
                let mut size                    = src.read_u32le()?;
                let mut hdr: [u8; FRAME_HDR_SIZE] = [0; FRAME_HDR_SIZE];
                                                  src.read_buf(&mut hdr)?;
                if (i == 0) && (chtype == CHTYPE_AUDIO) && (size > 4) && ((size as usize) < block_size/2) {
                    size += 0x10000;
                }
                if (chtype == CHTYPE_VIDEO || chtype == CHTYPE_AUDIO) && (size > 0) {
                    let ts = if (i == 0) || (chtype != CHTYPE_AUDIO) {
                            i as u32
                        } else {
                            ats
                        };
                    self.frames.push(FrameRec { chtype, size, hdr, off, ts });
                }
                if i > 0 && chtype == CHTYPE_AUDIO {
                    ats += 1;
                }
                if chtype != 0 {
                    validate!(off.checked_add(size).is_some());
                    off += size;
                }
            }
        }

        self.fno = 0;
        Ok(())
    }

    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.fno >= self.frames.len() { return Err(DemuxerError::EOF); }
        let cur_frame = &self.frames[self.fno];
//println!("fno {} -> type {} size {} @ {:X} ts {}", self.fno, cur_frame.chtype, cur_frame.size, cur_frame.off, cur_frame.ts);
        let next_pos = cur_frame.off as u64;
        if self.src.tell() != next_pos {
            self.src.seek(SeekFrom::Start(next_pos))?;
        }

        let is_video = cur_frame.chtype == CHTYPE_VIDEO;
        let mut buf: Vec<u8> = Vec::with_capacity(FRAME_HDR_SIZE + (cur_frame.size as usize));
        if !self.is_indeo || !is_video {
            buf.extend_from_slice(&cur_frame.hdr);
            buf.resize(FRAME_HDR_SIZE + (cur_frame.size as usize), 0);
            self.src.read_buf(&mut buf[FRAME_HDR_SIZE..])?;
        } else {
            buf.resize(cur_frame.size as usize, 0);
            self.src.read_buf(&mut buf)?;
        }

        self.fno += 1;

        let str_id = if is_video { self.vid_id } else { self.aud_id };
        let str = strmgr.get_stream(str_id).unwrap();
        let (tb_num, tb_den) = str.get_timebase();
        let ts = NATimeInfo::new(Some(cur_frame.ts as u64), None, None, tb_num, tb_den);
        let pkt = NAPacket::new(str, ts, false, buf);

        Ok(pkt)
    }

    #[allow(unused_variables)]
    fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        Err(DemuxerError::NotImplemented)
    }
}

impl<'a> VMDDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        Self {
            src:        io,
            vid_id:     0,
            aud_id:     0,
            fno:        0,
            is_indeo:   false,
            frames:     Vec::new(),
        }
    }
}

pub struct VMDDemuxerCreator { }

impl DemuxerCreator for VMDDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a> {
        Box::new(VMDDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "vmd" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_vmd_demux() {
        let mut file = File::open("assets/Game/128.vmd").unwrap();
        //let mut file = File::open("assets/Game/1491.VMD").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = VMDDemuxer::new(&mut br);
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
