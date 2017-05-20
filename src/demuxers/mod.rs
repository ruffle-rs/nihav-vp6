#[cfg(feature="demuxer_gdv")]
pub mod gdv;
#[cfg(feature="demuxer_avi")]
pub mod avi;

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

struct Demuxer {
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
    #[allow(dead_code)]
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

///The structure used to create demuxers.
pub trait DemuxerCreator {
    /// Create new demuxer instance that will use `ByteReader` source as an input.
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<Demux<'a> + 'a>;
    /// Get the name of current demuxer creator.
    fn get_name(&self) -> &'static str;
}

const DEMUXERS: &[&'static DemuxerCreator] = &[
#[cfg(feature="demuxer_avi")]
    &avi::AVIDemuxerCreator {},
#[cfg(feature="demuxer_gdv")]
    &gdv::GDVDemuxerCreator {},
];

pub fn find_demuxer(name: &str) -> Option<&DemuxerCreator> {
    for &dmx in DEMUXERS {
        if dmx.get_name() == name {
            return Some(dmx);
        }
    }
    None
}
