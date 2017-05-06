pub mod gdv;

use std::fmt;
use std::rc::Rc;
use frame::*;
//use std::collections::HashMap;
use io::byteio::*;

#[derive(Debug)]
#[allow(dead_code)]
pub enum StreamType {
    Video,
    Audio,
    Subtitles,
    Data,
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            StreamType::Video => write!(f, "Video"),
            StreamType::Audio => write!(f, "Audio"),
            StreamType::Subtitles => write!(f, "Subtitles"),
            StreamType::Data => write!(f, "Data"),
        }
    }
}


#[allow(dead_code)]
pub struct NAStream<'a> {
    media_type:     StreamType,
    id:             u32,
    info:           Rc<NACodecInfo<'a>>,
}

impl<'a> NAStream<'a> {
    pub fn new(mt: StreamType, id: u32, info: NACodecInfo<'a>) -> Self {
        NAStream { media_type: mt, id: id, info: Rc::new(info) }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_info(&self) -> Rc<NACodecInfo<'a>> { self.info.clone() }
}

impl<'a> fmt::Display for NAStream<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}#{})", self.media_type, self.id)
    }
}

#[allow(dead_code)]
pub struct NAPacket<'a> {
    stream:         Rc<NAStream<'a>>,
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         Rc<Vec<u8>>,
    keyframe:       bool,
//    options:        HashMap<String, NAValue<'a>>,
}

impl<'a> NAPacket<'a> {
    pub fn new(str: Rc<NAStream<'a>>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, kf: bool, vec: Vec<u8>) -> Self {
//        let mut vec: Vec<u8> = Vec::new();
//        vec.resize(size, 0);
        NAPacket { stream: str, pts: pts, dts: dts, duration: dur, keyframe: kf, buffer: Rc::new(vec) }
    }
    pub fn get_stream(&self) -> Rc<NAStream<'a>> { self.stream.clone() }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_buffer(&self) -> Rc<Vec<u8>> { self.buffer.clone() }
}

impl<'a> fmt::Display for NAPacket<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut foo = format!("[pkt for {} size {}", self.stream, self.buffer.len());
        if let Some(pts) = self.pts { foo = format!("{} pts {}", foo, pts); }
        if let Some(dts) = self.dts { foo = format!("{} dts {}", foo, dts); }
        if let Some(dur) = self.duration { foo = format!("{} duration {}", foo, dur); }
        if self.keyframe { foo = format!("{} kf", foo); }
        foo = foo + "]";
        write!(f, "{}", foo)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum DemuxerError {
    EOF,
    NoSuchInput,
    InvalidData,
    IOError,
    NotImplemented,
    MemoryError,
}

type DemuxerResult<T> = Result<T, DemuxerError>;

pub trait NADemuxer<'a> {
    fn open(&mut self) -> DemuxerResult<()>;
    fn get_frame(&mut self) -> DemuxerResult<NAPacket>;
    fn seek(&mut self, time: u64) -> DemuxerResult<()>;
}

pub trait NAPacketReader<'a> {
    fn read_packet(&mut self, str: Rc<NAStream<'a>>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, keyframe: bool, size: usize) -> DemuxerResult<NAPacket>;
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()>;
}

impl<'a> NAPacketReader<'a> for ByteReader<'a> {
    fn read_packet(&mut self, str: Rc<NAStream<'a>>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, kf: bool, size: usize) -> DemuxerResult<NAPacket> {
        let mut buf: Vec<u8> = Vec::with_capacity(size);
        if buf.capacity() < size { return Err(DemuxerError::MemoryError); }
        buf.resize(size, 0);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        if res.unwrap() < buf.len() { return Err(DemuxerError::IOError); }
        let pkt = NAPacket::new(str, pts, dts, dur, kf, buf);
        Ok(pkt)
    }
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()> {
        let mut refbuf = pkt.get_buffer();
        let mut buf = Rc::make_mut(&mut refbuf);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        if res.unwrap() < buf.len() { return Err(DemuxerError::IOError); }
        Ok(())
    }
}

pub struct NADemuxerBuilder {
}

impl From<ByteIOError> for DemuxerError {
    fn from(_: ByteIOError) -> Self { DemuxerError::IOError }
}

impl NADemuxerBuilder {
    #[allow(unused_variables)]
    pub fn create_demuxer(name: &str, url: &str) -> DemuxerResult<Box<NADemuxer<'static>>> {
        unimplemented!()
    }
}
