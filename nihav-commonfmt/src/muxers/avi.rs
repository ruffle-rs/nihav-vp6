use nihav_core::muxers::*;
use nihav_registry::register::*;

#[derive(Clone,Copy)]
struct IdxEntry {
    stream:     u32,
    stype:      StreamType,
    key:        bool,
    pos:        u32,
    len:        u32,
}

#[derive(Clone,Copy)]
struct AVIStream {
    strh_pos:   u64,
    nframes:    u32,
    is_video:   bool,
    max_size:   u32,    
}

struct AVIMuxer<'a> {
    bw:             &'a mut ByteWriter<'a>,
    index:          Vec<IdxEntry>,
    video_str:      Option<usize>,
    video_id:       u32,
    data_pos:       u64,
    stream_info:    Vec<AVIStream>,
}

impl<'a> AVIMuxer<'a> {
    fn new(bw: &'a mut ByteWriter<'a>) -> Self {
        Self {
            bw,
            index:          Vec::new(),
            video_str:      None,
            video_id:       0,
            data_pos:       0,
            stream_info:    Vec::with_capacity(2),
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

fn write_chunk_hdr(bw: &mut ByteWriter, stype: StreamType, str_no: u32) -> MuxerResult<()> {
    bw.write_byte(b'0' + ((str_no / 10) as u8))?;
    bw.write_byte(b'0' + ((str_no % 10) as u8))?;
    match stype {
        StreamType::Video => { bw.write_buf(b"dc")?; },
        StreamType::Audio => { bw.write_buf(b"wb")?; },
        StreamType::Subtitles => { bw.write_buf(b"tx")?; },
        _ => return Err(MuxerError::UnsupportedFormat),
    };
    Ok(())
}

impl<'a> MuxCore<'a> for AVIMuxer<'a> {
    fn create(&mut self, strmgr: &StreamManager) -> MuxerResult<()> {
        if strmgr.get_num_streams() == 0 {
            return Err(MuxerError::InvalidArgument);
        }
        if strmgr.get_num_streams() > 99 {
            return Err(MuxerError::UnsupportedFormat);
        }
        for (str_no, str) in strmgr.iter().enumerate() {
            if str.get_media_type() == StreamType::Video {
                self.video_str = Some(str_no);
                self.video_id  = str.id;
                break;
            }
        }
        let (vinfo, tb_num, tb_den) = if let Some(str_id) = self.video_str {
                let vstr = strmgr.get_stream(str_id).unwrap();
                (vstr.get_info(), vstr.tb_num, vstr.tb_den)
            } else {
                (NACodecInfo::new_dummy(), 0, 1)
            };
        let hdrl_pos = self.bw.tell() + 20;
        self.bw.write_buf(b"RIFF\0\0\0\0AVI LIST\0\0\0\0hdrlavih")?;
        self.bw.write_u32le(56)?; // avih size
        let ms_per_frame = NATimeInfo::ts_to_time(1, 1000000, tb_num, tb_den);
        self.bw.write_u32le(ms_per_frame as u32)?;
        self.bw.write_u32le(0)?; // max transfer rate
        self.bw.write_u32le(0)?; // padding granularity
        self.bw.write_u32le(0)?; // flags
        self.bw.write_u32le(0)?; // total frames
        self.bw.write_u32le(0)?; // initial frames
        self.bw.write_u32le(strmgr.get_num_streams() as u32)?;
        self.bw.write_u32le(0)?; // suggested buffer size
        if let NACodecTypeInfo::Video(ref vinfo) = vinfo.get_properties() {
            self.bw.write_u32le(vinfo.width as u32)?;
            self.bw.write_u32le(vinfo.height as u32)?;
        } else {
            self.bw.write_u32le(0)?;
            self.bw.write_u32le(0)?;
        }
        self.bw.write_u32le(0)?; // reserved
        self.bw.write_u32le(0)?; // reserved
        self.bw.write_u32le(0)?; // reserved
        self.bw.write_u32le(0)?; // reserved

        for str in strmgr.iter() {
            let strl_pos = self.bw.tell() + 8;
            self.bw.write_buf(b"LIST\0\0\0\0strlstrh")?;
            self.bw.write_u32le(56)?; // strh size

            match str.get_media_type() {
                StreamType::Video => {
                    self.bw.write_buf(b"vids")?;
                    let fcc = find_avi_fourcc(str.get_info().get_name());
                    if fcc.is_none() {
                        return Err(MuxerError::UnsupportedFormat);
                    }
                    self.bw.write_buf(&fcc.unwrap_or([0; 4]))?;
                    let vinfo = str.get_info().get_properties().get_video_info().unwrap();
                    if vinfo.width >= (1 << 16) || vinfo.height >= (1 << 16) {
                        return Err(MuxerError::UnsupportedFormat);
                    }
                },
                StreamType::Audio => {
                    self.bw.write_buf(b"auds")?;
                    self.bw.write_u32le(0)?;
                },
                StreamType::Subtitles => {
                    self.bw.write_buf(b"txts")?;
                    self.bw.write_u32le(0)?;
                },
                _ => return Err(MuxerError::UnsupportedFormat),
            };
            self.stream_info.push(AVIStream {
                    strh_pos:   self.bw.tell(),
                    is_video:   str.get_media_type() == StreamType::Video,
                    nframes:    0,
                    max_size:   0,
                });

            self.bw.write_u32le(0)?; // flags
            self.bw.write_u16le(0)?; // priority
            self.bw.write_u16le(0)?; // language
            self.bw.write_u32le(0)?; // initial frames
            self.bw.write_u32le(str.tb_num)?;
            self.bw.write_u32le(str.tb_den)?;
            self.bw.write_u32le(0)?; // start
            self.bw.write_u32le(0)?; // length
            self.bw.write_u32le(0)?; // suggested buffer size
            self.bw.write_u32le(0)?; // quality
            self.bw.write_u32le(0)?; // sample_size
            self.bw.write_u16le(0)?; // x
            self.bw.write_u16le(0)?; // y
            self.bw.write_u16le(0)?; // w
            self.bw.write_u16le(0)?; // h

            self.bw.write_buf(b"strf")?;
            self.bw.write_u32le(0)?;
            let strf_pos = self.bw.tell();
            match str.get_media_type() {
                StreamType::Video => {
                    let vinfo = str.get_info().get_properties().get_video_info().unwrap();
                    let hdr_pos = self.bw.tell();
                    self.bw.write_u32le(0)?;
                    self.bw.write_u32le(vinfo.width as u32)?;
                    if vinfo.flipped {
                        self.bw.write_u32le((-(vinfo.height as i32)) as u32)?;
                    } else {
                        self.bw.write_u32le(vinfo.height as u32)?;
                    }
                    self.bw.write_u16le(vinfo.format.components as u16)?;
                    self.bw.write_u16le(vinfo.format.get_total_depth() as u16)?;
                    let fcc = find_avi_fourcc(str.get_info().get_name());
                    if fcc.is_none() {
                        return Err(MuxerError::UnsupportedFormat);
                    }
                    self.bw.write_buf(&fcc.unwrap_or([0; 4]))?;
                    self.bw.write_u32le(0)?; // image size
                    self.bw.write_u32le(0)?; // x dpi
                    self.bw.write_u32le(0)?; // y dpi
                    if vinfo.format.palette {
//                        unimplemented!();
                        self.bw.write_u32le(0)?; // total colors
                        self.bw.write_u32le(0)?; // important colors
                    } else {
                        self.bw.write_u32le(0)?; // total colors
                        self.bw.write_u32le(0)?; // important colors
                    }
                    if let Some(ref edata) = str.get_info().get_extradata() {
                        self.bw.write_buf(edata.as_slice())?;
                    }
                    let bisize = self.bw.tell() - hdr_pos;
                    self.bw.seek(SeekFrom::Current(-(bisize as i64)))?;
                    self.bw.write_u32le(bisize as u32)?;
                    self.bw.seek(SeekFrom::End(0))?;
                },
                StreamType::Audio => {
                    let ainfo = str.get_info().get_properties().get_audio_info().unwrap();
                    let twocc = find_wav_twocc(str.get_info().get_name());
                    if twocc.is_none() {
                        return Err(MuxerError::UnsupportedFormat);
                    }
                    self.bw.write_u16le(twocc.unwrap_or(0))?;
                    self.bw.write_u16le(ainfo.channels as u16)?;
                    self.bw.write_u32le(ainfo.sample_rate)?;
                    self.bw.write_u32le(0)?; // avg bytes per second
                    self.bw.write_u16le(ainfo.block_len as u16)?;
                    self.bw.write_u16le(ainfo.format.bits as u16)?;
                    if let Some(ref edata) = str.get_info().get_extradata() {
                        self.bw.write_buf(edata.as_slice())?;
                    }
                },
                StreamType::Subtitles => {
                    if let Some(ref edata) = str.get_info().get_extradata() {
                        self.bw.write_buf(edata.as_slice())?;
                    }
                },
                _ => unreachable!(),
            };
            patch_size(&mut self.bw, strf_pos)?;
            patch_size(&mut self.bw, strl_pos)?;
        }
        patch_size(&mut self.bw, hdrl_pos)?;

        self.data_pos = self.bw.tell() + 8;
        self.bw.write_buf(b"LIST\0\0\0\0movi")?;

        Ok(())
    }
    fn mux_frame(&mut self, _strmgr: &StreamManager, pkt: NAPacket) -> MuxerResult<()> {
        if self.data_pos == 0 {
            return Err(MuxerError::NotCreated);
        }
        let str = pkt.get_stream();
        let str_num = str.get_num();
        if str_num > 99 || str_num >= self.stream_info.len() {
            return Err(MuxerError::UnsupportedFormat);
        }

        let chunk_len = pkt.get_buffer().len() as u32;

        self.stream_info[str_num].nframes += 1;
        self.stream_info[str_num].max_size = self.stream_info[str_num].max_size.max(chunk_len);
// todo palchange
        self.index.push(IdxEntry {
                stream: str_num as u32,
                stype:  str.get_media_type(),
                key:    pkt.keyframe,
                pos:    self.bw.tell() as u32,
                len:    chunk_len });
        write_chunk_hdr(&mut self.bw, str.get_media_type(), str_num as u32)?;
        self.bw.write_u32le(chunk_len)?;
        self.bw.write_buf(pkt.get_buffer().as_slice())?;
        Ok(())
    }
    fn flush(&mut self) -> MuxerResult<()> {
        Ok(())
    }
    fn end(&mut self) -> MuxerResult<()> {
        patch_size(&mut self.bw, self.data_pos)?;
        if self.index.len() > 0 {
            self.bw.write_buf(b"idx1")?;
            self.bw.write_u32le((self.index.len() * 16) as u32)?;
            for item in self.index.iter() {
                write_chunk_hdr(&mut self.bw, item.stype, item.stream)?;
                self.bw.write_u32le(if item.key { 0x10 } else { 0 })?;
                self.bw.write_u32le(item.pos)?;
                self.bw.write_u32le(item.len)?;
            }
        }
        patch_size(&mut self.bw, 8)?;
        let mut max_frames = 0;
        let mut max_size = 0;
        for stri in self.stream_info.iter() {
            max_frames = max_frames.max(stri.nframes);
            max_size = max_size.max(stri.max_size);
            self.bw.seek(SeekFrom::Start(stri.strh_pos + 0x18))?;
            self.bw.write_u32le(if stri.is_video { stri.nframes } else { 0 })?;
            self.bw.write_u32le(stri.max_size)?;
        }
        self.bw.seek(SeekFrom::Start(0x30))?;
        self.bw.write_u32le(max_frames)?;
        self.bw.seek(SeekFrom::Current(8))?;
        self.bw.write_u32le(max_size)?;
        Ok(())
    }
}

pub struct AVIMuxerCreator {}

impl MuxerCreator for AVIMuxerCreator {
    fn new_muxer<'a>(&self, bw: &'a mut ByteWriter<'a>) -> Box<dyn MuxCore<'a> + 'a> {
        Box::new(AVIMuxer::new(bw))
    }
    fn get_name(&self) -> &'static str { "avi" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use nihav_core::demuxers::*;
    use crate::demuxers::*;

    #[test]
    fn test_avi_muxer() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut file = File::open("assets/Indeo/laser05.avi").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let dmx_f = dmx_reg.find_demuxer("avi").unwrap();
        let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

        let ofile = File::create("assets/test_out/muxed.avi").unwrap();
        let mut fw = FileWriter::new_write(ofile);
        let mut bw = ByteWriter::new(&mut fw);
        let mut mux = AVIMuxer::new(&mut bw);

        mux.create(dmx.get_stream_manager()).unwrap();

        loop {
            let pktres = dmx.get_frame();
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
                panic!("error");
            }
            let pkt = pktres.unwrap();
            println!("Got {}", pkt);
            mux.mux_frame(dmx.get_stream_manager(), pkt).unwrap();
        }

        mux.end().unwrap();
panic!("end");
    }
}
