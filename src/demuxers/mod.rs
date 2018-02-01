use std::rc::Rc;
use frame::*;
use io::byteio::*;

#[derive(Debug,Clone,Copy,PartialEq)]
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

pub trait DemuxCore<'a> {
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()>;
    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket>;
    fn seek(&mut self, time: u64) -> DemuxerResult<()>;
}

pub trait NAPacketReader {
    fn read_packet(&mut self, str: Rc<NAStream>, ts: NATimeInfo, keyframe: bool, size: usize) -> DemuxerResult<NAPacket>;
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()>;
}

impl<'a> NAPacketReader for ByteReader<'a> {
    fn read_packet(&mut self, str: Rc<NAStream>, ts: NATimeInfo, kf: bool, size: usize) -> DemuxerResult<NAPacket> {
        let mut buf: Vec<u8> = Vec::with_capacity(size);
        if buf.capacity() < size { return Err(DemuxerError::MemoryError); }
        buf.resize(size, 0);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        let pkt = NAPacket::new(str, ts, kf, buf);
        Ok(pkt)
    }
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()> {
        let mut refbuf = pkt.get_buffer();
        let buf = Rc::make_mut(&mut refbuf);
        let res = self.read_buf(buf.as_mut_slice());
        if let Err(_) = res { return Err(DemuxerError::IOError); }
        Ok(())
    }
}

pub struct StreamManager {
    streams: Vec<Rc<NAStream>>,
    ignored: Vec<bool>,
    no_ign:  bool,
}

impl StreamManager {
    pub fn new() -> Self {
        StreamManager {
            streams: Vec::new(),
            ignored: Vec::new(),
            no_ign:  true,
        }
    }
    pub fn iter(&self) -> StreamIter { StreamIter::new(&self.streams) }

    pub fn add_stream(&mut self, stream: NAStream) -> Option<usize> {
        let stream_num = self.streams.len();
        let mut str = stream.clone();
        str.set_num(stream_num);
        self.streams.push(Rc::new(str));
        self.ignored.push(false);
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
    pub fn is_ignored(&self, idx: usize) -> bool {
        if self.no_ign {
            true
        } else if idx < self.ignored.len() {
            self.ignored[idx]
        } else {
            false
        }
    }
    pub fn is_ignored_id(&self, id: u32) -> bool {
        for i in 0..self.streams.len() {
            if self.streams[i].get_id() == id {
                return self.ignored[i];
            }
        }
        false
    }
    pub fn set_ignored(&mut self, idx: usize) {
        if idx < self.ignored.len() {
            self.ignored[idx] = true;
            self.no_ign = false;
        }
    }
    pub fn set_unignored(&mut self, idx: usize) {
        if idx < self.ignored.len() {
            self.ignored[idx] = false;
        }
    }
}

pub struct StreamIter<'a> {
    streams:    &'a Vec<Rc<NAStream>>,
    pos:        usize,
}

impl<'a> StreamIter<'a> {
    pub fn new(streams: &'a Vec<Rc<NAStream>>) -> Self {
        StreamIter { streams: streams, pos: 0 }
    }
}

impl<'a> Iterator for StreamIter<'a> {
    type Item = Rc<NAStream>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.streams.len() { return None; }
        let ret = self.streams[self.pos].clone();
        self.pos += 1;
        Some(ret)
    }
}

pub struct Demuxer<'a> {
    dmx:        Box<DemuxCore<'a> + 'a>,
    streams:    StreamManager,
}

impl<'a> Demuxer<'a> {
    fn new(dmx: Box<DemuxCore<'a> + 'a>, str: StreamManager) -> Self {
        Demuxer {
            dmx:        dmx,
            streams:    str,
        }
    }
    pub fn get_stream(&self, idx: usize) -> Option<Rc<NAStream>> {
        self.streams.get_stream(idx)
    }
    pub fn get_stream_by_id(&self, id: u32) -> Option<Rc<NAStream>> {
        self.streams.get_stream_by_id(id)
    }
    pub fn get_num_streams(&self) -> usize {
        self.streams.get_num_streams()
    }
    pub fn get_streams(&self) -> StreamIter {
        self.streams.iter()
    }
    pub fn is_ignored_stream(&self, idx: usize) -> bool {
        self.streams.is_ignored(idx)
    }
    pub fn set_ignored_stream(&mut self, idx: usize) {
        self.streams.set_ignored(idx)
    }
    pub fn set_unignored_stream(&mut self, idx: usize) {
        self.streams.set_unignored(idx)
    }

    pub fn get_frame(&mut self) -> DemuxerResult<NAPacket> {
        loop {
            let res = self.dmx.get_frame(&mut self.streams);
            if self.streams.no_ign || res.is_err() { return res; }
            let res = res.unwrap();
            let idx = res.get_stream().get_num();
            if !self.is_ignored_stream(idx) {
                return Ok(res);
            }
        }
    }
    pub fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        self.dmx.seek(time)
    }
}

impl From<ByteIOError> for DemuxerError {
    fn from(_: ByteIOError) -> Self { DemuxerError::IOError }
}

///The structure used to create demuxers.
pub trait DemuxerCreator {
    /// Create new demuxer instance that will use `ByteReader` source as an input.
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<DemuxCore<'a> + 'a>;
    /// Get the name of current demuxer creator.
    fn get_name(&self) -> &'static str;
}

macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DemuxerError::InvalidData); } };
}

#[cfg(feature="demuxer_gdv")]
mod gdv;
#[cfg(feature="demuxer_avi")]
mod avi;
#[cfg(feature="demuxer_real")]
mod realmedia;


const DEMUXERS: &[&'static DemuxerCreator] = &[
#[cfg(feature="demuxer_avi")]
    &avi::AVIDemuxerCreator {},
#[cfg(feature="demuxer_gdv")]
    &gdv::GDVDemuxerCreator {},
#[cfg(feature="demuxer_real")]
    &realmedia::RealMediaDemuxerCreator {},
//#[cfg(feature="demuxer_real")]
//    &realmedia::RealAudioDemuxerCreator {},
//#[cfg(feature="demuxer_real")]
//    &realmedia::RealIVRDemuxerCreator {},
];

pub fn find_demuxer(name: &str) -> Option<&DemuxerCreator> {
    for &dmx in DEMUXERS {
        if dmx.get_name() == name {
            return Some(dmx);
        }
    }
    None
}

pub fn create_demuxer<'a>(dmxcr: &DemuxerCreator, br: &'a mut ByteReader<'a>) -> DemuxerResult<Demuxer<'a>> {
    let mut dmx = dmxcr.new_demuxer(br);
    let mut str = StreamManager::new();
    dmx.open(&mut str)?;    
    Ok(Demuxer::new(dmx, str))
}
