use nihav_core::demuxers::*;
use nihav_core::register;
use nihav_core::demuxers::DemuxerError::*;

macro_rules! mktag {
    ($a:expr, $b:expr, $c:expr, $d:expr) => ({
        (($a as u32) << 24) | (($b as u32) << 16) | (($c as u32) << 8) | ($d as u32)
    });
    ($arr:expr) => ({
        (($arr[0] as u32) << 24) | (($arr[1] as u32) << 16) | (($arr[2] as u32) << 8) | ($arr[3] as u32)
    });
}

struct StreamState {
    strm_no:    u8,
    got_strf:   bool,
    strm_type:  Option<StreamType>,
}

impl StreamState {
    fn new() -> Self {
        StreamState { strm_no: 0, got_strf: true, strm_type: None }
    }
    fn reset(&mut self) {
        self.strm_type = None;
        self.got_strf  = true;
        self.strm_no  += 1;
    }
    fn valid_state(&self) -> bool {
        match self.strm_type {
            None => self.got_strf,
            _    => false,
        }
    }
}

#[allow(dead_code)]
struct AVIDemuxer<'a> {
    src:            &'a mut ByteReader<'a>,
    cur_frame:      Vec<u64>,
    num_streams:    u8,
    size:           usize,
    movi_size:      usize,
    sstate:         StreamState,
    tb_num:         u32,
    tb_den:         u32,
}

#[derive(Debug,Clone,Copy,PartialEq)]
enum RIFFTag {
    Chunk(u32),
    List(u32,u32),
}

struct RIFFParser {
    tag:   RIFFTag,
    parse: fn(&mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize>,
}

impl<'a> DemuxCore<'a> for AVIDemuxer<'a> {
    #[allow(unused_variables)]
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        self.read_header(strmgr)?;
        Ok(())
    }

    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.movi_size == 0 { return Err(EOF); }
        let mut tag: [u8; 4] = [0; 4];
        loop {
            if (self.src.tell() & 1) == 1 {
                self.src.read_skip(1)?;
                self.movi_size -= 1;
                if self.movi_size == 0 { return Err(EOF); }
            }
            self.src.read_buf(&mut tag)?;
            let size = self.src.read_u32le()? as usize;
            if mktag!(tag) == mktag!(b"JUNK") {
                self.movi_size -= size + 8;
                self.src.read_skip(size)?;
                if self.movi_size == 0 { return Err(EOF); }
                continue;
            }
            if mktag!(tag) == mktag!(b"LIST") {
                self.movi_size -= 12;
                self.src.read_skip(4)?;
                if self.movi_size == 0 { return Err(EOF); }
                continue;
            }
            if tag[0] == b'i' && tag[1] == b'x' {
                return Err(EOF);
            }
            if tag[0] < b'0' || tag[0] > b'9' || tag[1] < b'0' || tag[1] > b'9' {
                return Err(InvalidData);
            }
            let stream_no = (tag[0] - b'0') * 10 + (tag[1] - b'0');
            let str = strmgr.get_stream(stream_no as usize);
            if str.is_none() { return Err(InvalidData); }
            let stream = str.unwrap();
            if size == 0 {
                self.movi_size -= 8;
                if self.movi_size == 0 { return Err(EOF); }
                continue;
            }
            let (tb_num, tb_den) = stream.get_timebase();
            let ts = NATimeInfo::new(Some(self.cur_frame[stream_no as usize]), None, None, tb_num, tb_den);
            let pkt = self.src.read_packet(stream, ts, false, size)?;
            self.cur_frame[stream_no as usize] += 1;
            self.movi_size -= size + 8;

            return Ok(pkt);
        }
    }

    #[allow(unused_variables)]
    fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        Err(NotImplemented)
    }
}

impl<'a> AVIDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        AVIDemuxer {
            cur_frame: Vec::new(),
            num_streams: 0,
            src: io,
            size: 0,
            movi_size: 0,
            sstate: StreamState::new(),
            tb_num: 0,
            tb_den: 0,
        }
    }

    fn parse_chunk(&mut self, strmgr: &mut StreamManager, end_tag: RIFFTag, csize: usize, depth: u16) -> DemuxerResult<(usize, bool)> {
        if csize < 8 { return Err(InvalidData); }
        if depth > 42 { return Err(InvalidData); }

        let tag  = self.src.read_u32be()?;
        let size = self.src.read_u32le()? as usize;
        if size > csize { return Err(InvalidData); }
        if RIFFTag::Chunk(tag) == end_tag {
            return Ok((size, true));
        }
        let is_list = is_list_tag(tag);
        let ltag = if is_list { self.src.read_u32be()? } else { 0 };
        if RIFFTag::List(tag, ltag) == end_tag {
            return Ok((size, true));
        }

        for chunk in CHUNKS.iter() {
            if RIFFTag::Chunk(tag) == chunk.tag {
                let psize = (chunk.parse)(self, strmgr, size)?;
                if psize != size { return Err(InvalidData); }
                if (psize & 1) == 1 { self.src.read_skip(1)?; }
                return Ok((size + 8, false));
            }
            if RIFFTag::List(tag, ltag) == chunk.tag {
                let mut rest_size = size - 4;
                let psize = (chunk.parse)(self, strmgr, rest_size)?;
                if psize > rest_size { return Err(InvalidData); }
                rest_size -= psize;
                while rest_size > 0 {
                    let (psize, _) = self.parse_chunk(strmgr, end_tag, rest_size, depth+1)?;
                    if psize > rest_size { return Err(InvalidData); }
                    rest_size -= psize;
                    if ((psize & 1) == 1) && (rest_size > 0) {
                        rest_size -= 1;
                    }
                }

                return Ok((size + 8, false));
            }
        }
        if !is_list {
            self.src.read_skip(size)?;
        } else {
            if size < 4 { return Err(InvalidData); }
            self.src.read_skip(size - 4)?;
        }
        if (size & 1) == 1 { self.src.read_skip(1)?; }
        Ok((size + 8, false))
    }

    fn read_header(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let riff_tag = self.src.read_u32be()?;
        let size     = self.src.read_u32le()? as usize;
        let avi_tag  = self.src.read_u32be()?;
        let mut matches = false;
        for rt in RIFF_TAGS.iter() {
            if rt[0] == riff_tag && rt[1] == avi_tag {
                matches = true;
                break;
            }
        }
        if !matches {
            return Err(InvalidData);
        }
        self.size = size;
        let mut rest_size = size;
        loop {
            let (csz, end) = self.parse_chunk(strmgr, RIFFTag::List(mktag!(b"LIST"), mktag!(b"movi")), rest_size,0)?;
            if end { self.movi_size = csz - 4; break; }
            rest_size -= csz;
        }
        if !self.sstate.valid_state() || self.sstate.strm_no != self.num_streams {
            return Err(InvalidData);
        }
        Ok(())
    }

    fn read_extradata(&mut self, size: usize) -> DemuxerResult<Option<Vec<u8>>> {
        if size == 0 { return Ok(None); }
        let mut edvec: Vec<u8> = vec![0; size];
        self.src.read_buf(&mut edvec)?;
        Ok(Some(edvec))
    }
}

const RIFF_TAGS: &[[u32; 2]] = &[
    [ mktag!(b"RIFF"), mktag!(b"AVI ") ],
    [ mktag!(b"RIFF"), mktag!(b"AVIX") ],
    [ mktag!(b"ON2 "), mktag!(b"ON2f") ],
];

const CHUNKS: [RIFFParser; 7] = [
    RIFFParser { tag: RIFFTag::List(mktag!(b"LIST"), mktag!(b"hdrl")), parse: parse_hdrl },
    RIFFParser { tag: RIFFTag::List(mktag!(b"LIST"), mktag!(b"strl")), parse: parse_strl },
    RIFFParser { tag: RIFFTag::Chunk(mktag!(b"avih")), parse: parse_avih },
    RIFFParser { tag: RIFFTag::Chunk(mktag!(b"ON2h")), parse: parse_avih },
    RIFFParser { tag: RIFFTag::Chunk(mktag!(b"strf")), parse: parse_strf },
    RIFFParser { tag: RIFFTag::Chunk(mktag!(b"strh")), parse: parse_strh },
    RIFFParser { tag: RIFFTag::Chunk(mktag!(b"JUNK")), parse: parse_junk },
];

fn is_list_tag(tag: u32) -> bool {
    for chunk in CHUNKS.iter() {
        if let RIFFTag::List(ltag, _) = chunk.tag {
            if tag == ltag {
                return true;
            }
        }
    }
    false
}

#[allow(unused_variables)]
fn parse_hdrl(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    Ok(0)
}

#[allow(unused_variables)]
fn parse_strl(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    Ok(0)
}

#[allow(unused_variables)]
fn parse_strh(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    if size < 0x38 { return Err(InvalidData); }
    let tag = dmx.src.read_u32be()?; //stream type
    let fcc = dmx.src.read_u32be()?; //handler(fourcc)
    dmx.src.read_u32le()?; //flags
    dmx.src.read_skip(2)?; //priority
    dmx.src.read_skip(2)?; //language
    dmx.src.read_skip(4)?; //initial frames
    dmx.tb_num = dmx.src.read_u32le()?; //scale
    dmx.tb_den = dmx.src.read_u32le()?; //rate
    dmx.src.read_skip(4)?; //start
    dmx.src.read_skip(4)?; //length
    dmx.src.read_skip(4)?; //buf size
    dmx.src.read_skip(4)?; //quality
    dmx.src.read_skip(4)?; //sample size
    let a = dmx.src.read_u16le()?;
    let b = dmx.src.read_u16le()?;
    let c = dmx.src.read_u16le()?;
    let d = dmx.src.read_u16le()?;

    dmx.src.read_skip(size - 0x38)?;

    if !dmx.sstate.valid_state() || dmx.sstate.strm_no >= dmx.num_streams {
        return Err(InvalidData);
    }
    if tag == mktag!(b"vids") {
        dmx.sstate.strm_type = Some(StreamType::Video);
    } else if tag == mktag!(b"auds") {
        dmx.sstate.strm_type = Some(StreamType::Audio);
    } else {
        dmx.sstate.strm_type = Some(StreamType::Data);
    }
    dmx.sstate.got_strf = false;

    Ok(size)
}

fn parse_strf(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    if dmx.sstate.strm_type.is_none() { return Err(InvalidData); }
    match dmx.sstate.strm_type.unwrap() {
        StreamType::Video    => parse_strf_vids(dmx, strmgr, size),
        StreamType::Audio    => parse_strf_auds(dmx, strmgr, size),
        _                    => parse_strf_xxxx(dmx, strmgr, size),
    }
}

#[allow(unused_variables)]
fn parse_strf_vids(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    if size < 40 { return Err(InvalidData); }
    let bi_size         = dmx.src.read_u32le()?;
    if (bi_size as usize) > size { return Err(InvalidData); }
    let width           = dmx.src.read_u32le()?;
    let height          = dmx.src.read_u32le()? as i32;
    let planes          = dmx.src.read_u16le()?;
    let bitcount        = dmx.src.read_u16le()?;
    let mut compression: [u8; 4] = [0; 4];
                          dmx.src.read_buf(&mut compression)?;
    let img_size        = dmx.src.read_u32le()?;
    let xdpi            = dmx.src.read_u32le()?;
    let ydpi            = dmx.src.read_u32le()?;
    let colors          = dmx.src.read_u32le()?;
    let imp_colors      = dmx.src.read_u32le()?;

    let flip = height < 0;
    let format = if bitcount > 8 { RGB24_FORMAT } else { PAL8_FORMAT };
    let vhdr = NAVideoInfo::new(width as usize, if flip { -height as usize } else { height as usize}, flip, PAL8_FORMAT);
    let vci = NACodecTypeInfo::Video(vhdr);
    let edata = dmx.read_extradata(size - 40)?;
    let cname = match register::find_codec_from_avi_fourcc(&compression) {
                    None => "unknown",
                    Some(name) => name,
                };
    let vinfo = NACodecInfo::new(cname, vci, edata);
    let res = strmgr.add_stream(NAStream::new(StreamType::Video, u32::from(dmx.sstate.strm_no), vinfo, dmx.tb_num, dmx.tb_den));
    if res.is_none() { return Err(MemoryError); }
    dmx.sstate.reset();
    Ok(size)
}

#[allow(unused_variables)]
fn parse_strf_auds(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    if size < 16 { return Err(InvalidData); }
    let w_format_tag        = dmx.src.read_u16le()?;
    let channels            = dmx.src.read_u16le()?;
    let samplespersec       = dmx.src.read_u32le()?;
    let avgbytespersec      = dmx.src.read_u32le()?;
    let block_align         = dmx.src.read_u16le()?;
    let bits_per_sample     = dmx.src.read_u16le()? * 8;

    let soniton = NASoniton::new(bits_per_sample as u8, SONITON_FLAG_SIGNED);
    let ahdr = NAAudioInfo::new(samplespersec, channels as u8, soniton, block_align as usize);
    let edata = dmx.read_extradata(size - 16)?;
    let cname = match register::find_codec_from_wav_twocc(w_format_tag) {
                    None => "unknown",
                    Some(name) => name,
                };
    let ainfo = NACodecInfo::new(cname, NACodecTypeInfo::Audio(ahdr), edata);
    let res = strmgr.add_stream(NAStream::new(StreamType::Audio, u32::from(dmx.sstate.strm_no), ainfo, dmx.tb_num, dmx.tb_den));
    if res.is_none() { return Err(MemoryError); }
    dmx.sstate.reset();
    Ok(size)
}

fn parse_strf_xxxx(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    let edata = dmx.read_extradata(size)?;
    let info = NACodecInfo::new("unknown", NACodecTypeInfo::None, edata);
    let res = strmgr.add_stream(NAStream::new(StreamType::Data, u32::from(dmx.sstate.strm_no), info, dmx.tb_num, dmx.tb_den));
    if res.is_none() { return Err(MemoryError); }
    dmx.sstate.reset();
    Ok(size)
}

#[allow(unused_variables)]
fn parse_avih(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    if size < 0x38 { return Err(InvalidData); }
    let timebase = dmx.src.read_u32le()?; //microsec per frame
    dmx.src.read_skip(4)?; //max frame size
    dmx.src.read_skip(4)?; //padding
    dmx.src.read_u32le()?; //flags
    let frames = dmx.src.read_u32le()?; //frames
    dmx.src.read_skip(4)?; //initial frames
    let streams = dmx.src.read_u32le()?; //streams
    if streams > 100 { return Err(InvalidData); }
    dmx.num_streams = streams as u8;

    dmx.src.read_skip(4)?; //buf size
    let width = dmx.src.read_u32le()?; //width
    let height = dmx.src.read_u32le()? as i32; //height
    dmx.src.read_skip(16)?; //reserved

    dmx.cur_frame.resize(streams as usize, 0);
    dmx.src.read_skip(size - 0x38)?;
    Ok(size)
}

#[allow(unused_variables)]
fn parse_junk(dmx: &mut AVIDemuxer, strmgr: &mut StreamManager, size: usize) -> DemuxerResult<usize> {
    dmx.src.read_skip(size)?;
    Ok(size)
}

pub struct AVIDemuxerCreator { }

impl DemuxerCreator for AVIDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a> {
        Box::new(AVIDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "avi" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_avi_demux() {
        let mut file = File::open("assets/Indeo/laser05.avi").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = AVIDemuxer::new(&mut br);
        let mut sm = StreamManager::new();
        dmx.open(&mut sm).unwrap();

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
