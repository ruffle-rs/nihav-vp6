#[cfg(feature="demuxer_gdv")]
pub mod gdv;
#[cfg(feature="demuxer_avi")]
pub mod avi;

use std::fmt;
use std::rc::Rc;
use frame::*;
use std::collections::HashMap;
use io::byteio::*;

#[derive(Debug,Clone,Copy)]
#[allow(dead_code)]
pub enum StreamType {
    Video,
    Audio,
    Subtitles,
    Data,
    None,
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            StreamType::Video => write!(f, "Video"),
            StreamType::Audio => write!(f, "Audio"),
            StreamType::Subtitles => write!(f, "Subtitles"),
            StreamType::Data => write!(f, "Data"),
            StreamType::None => write!(f, "-"),
        }
    }
}


#[allow(dead_code)]
#[derive(Clone)]
pub struct NAStream {
    media_type:     StreamType,
    id:             u32,
    num:            usize,
    info:           Rc<NACodecInfo>,
}

impl NAStream {
    pub fn new(mt: StreamType, id: u32, info: NACodecInfo) -> Self {
        NAStream { media_type: mt, id: id, num: 0, info: Rc::new(info) }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_num(&self) -> usize { self.num }
    pub fn set_num(&mut self, num: usize) { self.num = num; }
    pub fn get_info(&self) -> Rc<NACodecInfo> { self.info.clone() }
}

impl fmt::Display for NAStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}#{} - {})", self.media_type, self.id, self.info.get_properties())
    }
}

#[allow(dead_code)]
pub struct NAPacket {
    stream:         Rc<NAStream>,
    pts:            Option<u64>,
    dts:            Option<u64>,
    duration:       Option<u64>,
    buffer:         Rc<Vec<u8>>,
    keyframe:       bool,
//    options:        HashMap<String, NAValue<'a>>,
}

impl NAPacket {
    pub fn new(str: Rc<NAStream>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, kf: bool, vec: Vec<u8>) -> Self {
//        let mut vec: Vec<u8> = Vec::new();
//        vec.resize(size, 0);
        NAPacket { stream: str, pts: pts, dts: dts, duration: dur, keyframe: kf, buffer: Rc::new(vec) }
    }
    pub fn get_stream(&self) -> Rc<NAStream> { self.stream.clone() }
    pub fn get_pts(&self) -> Option<u64> { self.pts }
    pub fn get_dts(&self) -> Option<u64> { self.dts }
    pub fn get_duration(&self) -> Option<u64> { self.duration }
    pub fn is_keyframe(&self) -> bool { self.keyframe }
    pub fn get_buffer(&self) -> Rc<Vec<u8>> { self.buffer.clone() }
}

impl Drop for NAPacket {
    fn drop(&mut self) {}
}

impl fmt::Display for NAPacket {
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

pub trait Demux<'a> {
    fn open(&mut self) -> DemuxerResult<()>;
    fn get_num_streams(&self) -> usize;
    fn get_stream(&self, idx: usize) -> Option<Rc<NAStream>>;
    fn get_frame(&mut self) -> DemuxerResult<NAPacket>;
    fn seek(&mut self, time: u64) -> DemuxerResult<()>;
}

pub trait NAPacketReader {
    fn read_packet(&mut self, str: Rc<NAStream>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, keyframe: bool, size: usize) -> DemuxerResult<NAPacket>;
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()>;
}

impl<'a> NAPacketReader for ByteReader<'a> {
    fn read_packet(&mut self, str: Rc<NAStream>, pts: Option<u64>, dts: Option<u64>, dur: Option<u64>, kf: bool, size: usize) -> DemuxerResult<NAPacket> {
        let mut buf: Vec<u8> = Vec::with_capacity(size);
        if buf.capacity() < size { return Err(DemuxerError::MemoryError); }
        buf.resize(size, 0);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        let pkt = NAPacket::new(str, pts, dts, dur, kf, buf);
        Ok(pkt)
    }
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()> {
        let mut refbuf = pkt.get_buffer();
        let mut buf = Rc::make_mut(&mut refbuf);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        Ok(())
    }
}

pub struct Demuxer {
    streams: Vec<Rc<NAStream>>,
}

impl Demuxer {
    pub fn new() -> Self { Demuxer { streams: Vec::new() } }
    pub fn add_stream(&mut self, stream: NAStream) -> Option<usize> {
        let stream_num = self.streams.len();
        let mut str = stream.clone();
        str.set_num(stream_num);
        self.streams.push(Rc::new(str));
        Some(stream_num)
    }
    pub fn get_stream(&self, idx: usize) -> Option<Rc<NAStream>> {
        if idx < self.streams.len() {
            Some(self.streams[idx].clone())
        } else {
            None
        }
    }
    pub fn get_stream_by_id(&self, id: u32) -> Option<Rc<NAStream>> {
        for i in 0..self.streams.len() {
            if self.streams[i].get_id() == id {
                return Some(self.streams[i].clone());
            }
        }
        None
    }
    pub fn get_num_streams(&self) -> usize { self.streams.len() }
}

impl From<ByteIOError> for DemuxerError {
    fn from(_: ByteIOError) -> Self { DemuxerError::IOError }
}

pub trait FrameFromPacket {
    fn new_from_pkt(pkt: &NAPacket, info: Rc<NACodecInfo>) -> NAFrame;
    fn fill_timestamps(&mut self, pkt: &NAPacket);
}

impl FrameFromPacket for NAFrame {
    fn new_from_pkt(pkt: &NAPacket, info: Rc<NACodecInfo>) -> NAFrame {
        NAFrame::new(pkt.pts, pkt.dts, pkt.duration, info, HashMap::new())
    }
    fn fill_timestamps(&mut self, pkt: &NAPacket) {
        self.set_pts(pkt.pts);
        self.set_dts(pkt.dts);
        self.set_duration(pkt.duration);
    }
}

pub trait DemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<Demux<'a> + 'a>;
    fn get_name(&self) -> &'static str;
}

const DEMUXERS: &[&'static DemuxerCreator] = &[
#[cfg(feature="demuxer_avi")]
    &avi::AVIDemuxerCreator {},
#[cfg(feature="demuxer_gdv")]
    &gdv::GDVDemuxerCreator {},
];

pub fn find_demuxer(name: &str) -> Option<&DemuxerCreator> {
    for i in 0..DEMUXERS.len() {
        if DEMUXERS[i].get_name() == name {
            return Some(DEMUXERS[i]);
        }
    }
    None
}
