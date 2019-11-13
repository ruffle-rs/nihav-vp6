pub use crate::frame::*;
pub use crate::io::byteio::*;

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DemuxerError {
    EOF,
    NoSuchInput,
    InvalidData,
    IOError,
    NotImplemented,
    MemoryError,
    TryAgain,
    SeekError,
    NotPossible,
}

pub type DemuxerResult<T> = Result<T, DemuxerError>;

pub trait DemuxCore<'a> {
    fn open(&mut self, strmgr: &mut StreamManager, seek_idx: &mut SeekIndex) -> DemuxerResult<()>;
    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket>;
    fn seek(&mut self, time: u64, seek_idx: &SeekIndex) -> DemuxerResult<()>;
}

pub trait NAPacketReader {
    fn read_packet(&mut self, str: NAStreamRef, ts: NATimeInfo, keyframe: bool, size: usize) -> DemuxerResult<NAPacket>;
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()>;
}

impl<'a> NAPacketReader for ByteReader<'a> {
    fn read_packet(&mut self, str: NAStreamRef, ts: NATimeInfo, kf: bool, size: usize) -> DemuxerResult<NAPacket> {
        let mut buf: Vec<u8> = Vec::with_capacity(size);
        if buf.capacity() < size { return Err(DemuxerError::MemoryError); }
        buf.resize(size, 0);
        self.read_buf(buf.as_mut_slice())?;
        let pkt = NAPacket::new(str, ts, kf, buf);
        Ok(pkt)
    }
    fn fill_packet(&mut self, pkt: &mut NAPacket) -> DemuxerResult<()> {
        let mut refbuf = pkt.get_buffer();
        let buf = refbuf.as_mut().unwrap();
        self.read_buf(buf.as_mut_slice())?;
        Ok(())
    }
}

#[derive(Default)]
pub struct StreamManager {
    streams: Vec<NAStreamRef>,
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
        self.streams.push(str.into_ref());
        self.ignored.push(false);
        Some(stream_num)
    }
    pub fn get_stream(&self, idx: usize) -> Option<NAStreamRef> {
        if idx < self.streams.len() {
            Some(self.streams[idx].clone())
        } else {
            None
        }
    }
    pub fn get_stream_by_id(&self, id: u32) -> Option<NAStreamRef> {
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
    streams:    &'a [NAStreamRef],
    pos:        usize,
}

impl<'a> StreamIter<'a> {
    pub fn new(streams: &'a [NAStreamRef]) -> Self {
        StreamIter { streams, pos: 0 }
    }
}

impl<'a> Iterator for StreamIter<'a> {
    type Item = NAStreamRef;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.streams.len() { return None; }
        let ret = self.streams[self.pos].clone();
        self.pos += 1;
        Some(ret)
    }
}

#[derive(Clone,Copy,PartialEq)]
pub enum SeekIndexMode {
    None,
    Present,
    Automatic,
}

impl Default for SeekIndexMode {
    fn default() -> Self { SeekIndexMode::None }
}

#[derive(Clone,Copy,Default)]
pub struct SeekEntry {
    pub pts:    u64,
    pub pos:    u64,
}

#[derive(Clone)]
pub struct StreamSeekInfo {
    pub id:         u32,
    pub tb_num:     u32,
    pub tb_den:     u32,
    pub filled:     bool,
    pub entries:    Vec<SeekEntry>,
}

impl StreamSeekInfo {
    pub fn new(id: u32, tb_num: u32, tb_den: u32) -> Self {
        Self {
            id, tb_num, tb_den,
            filled:     false,
            entries:    Vec::new(),
        }
    }
    pub fn add_entry(&mut self, entry: SeekEntry) {
        self.entries.push(entry);
    }
    pub fn find_pos(&self, pts: u64) -> Option<u64> {
        if !self.entries.is_empty() {
// todo something faster like binary search
            let mut cand = 0;
            for (idx, entry) in self.entries.iter().enumerate() {
                if entry.pts <= pts {
                    cand = idx;
                } else {
                    break;
                }
            }
            Some(self.entries[cand].pos)
        } else {
            None
        }
    }
}

#[derive(Clone,Copy,Default)]
pub struct SeekIndexResult {
    pub pts:        u64,
    pub pos:        u64,
    pub str_id:     u32,
}

#[derive(Default)]
pub struct SeekIndex {
    pub seek_info:  Vec<StreamSeekInfo>,
    pub mode:       SeekIndexMode,
}

impl SeekIndex {
    pub fn new() -> Self { Self::default() }
    pub fn add_stream(&mut self, id: u32, tb_num: u32, tb_den: u32) {
        if self.stream_id_to_index(id).is_none() {
            self.seek_info.push(StreamSeekInfo::new(id, tb_num, tb_den));
        }
    }
    pub fn stream_id_to_index(&self, id: u32) -> Option<usize> {
        for (idx, str) in self.seek_info.iter().enumerate() {
            if str.id == id {
                return Some(idx);
            }
        }
        None
    }
    pub fn find_pos(&self, time: u64) -> Option<SeekIndexResult> {
        let mut cand = None;
        for str in self.seek_info.iter() {
            if !str.filled { continue; }
            let pts = NATimeInfo::time_to_ts(time, 1000, str.tb_num, str.tb_den);
            let pos = str.find_pos(pts);
            if pos.is_none() { continue; }
            let pos = pos.unwrap();
            if cand.is_none() {
                cand = Some(SeekIndexResult { pts, pos, str_id: str.id });
            } else if let Some(entry) = cand {
                if pos < entry.pos {
                    cand = Some(SeekIndexResult { pts, pos, str_id: str.id });
                }
            }
        }
        cand
    }
}

pub struct Demuxer<'a> {
    dmx:        Box<dyn DemuxCore<'a> + 'a>,
    streams:    StreamManager,
    seek_idx:   SeekIndex,
}

impl<'a> Demuxer<'a> {
    fn new(dmx: Box<dyn DemuxCore<'a> + 'a>, str: StreamManager, seek_idx: SeekIndex) -> Self {
        Demuxer {
            dmx,
            streams:    str,
            seek_idx,
        }
    }
    pub fn get_stream(&self, idx: usize) -> Option<NAStreamRef> {
        self.streams.get_stream(idx)
    }
    pub fn get_stream_by_id(&self, id: u32) -> Option<NAStreamRef> {
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
        self.dmx.seek(time, &self.seek_idx)
    }
    pub fn get_seek_index(&self) -> &SeekIndex {
        &self.seek_idx
    }
}

impl From<ByteIOError> for DemuxerError {
    fn from(_: ByteIOError) -> Self { DemuxerError::IOError }
}

///The structure used to create demuxers.
pub trait DemuxerCreator {
    /// Create new demuxer instance that will use `ByteReader` source as an input.
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a>;
    /// Get the name of current demuxer creator.
    fn get_name(&self) -> &'static str;
}

pub fn create_demuxer<'a>(dmxcr: &DemuxerCreator, br: &'a mut ByteReader<'a>) -> DemuxerResult<Demuxer<'a>> {
    let mut dmx = dmxcr.new_demuxer(br);
    let mut str = StreamManager::new();
    let mut seek_idx = SeekIndex::new();
    dmx.open(&mut str, &mut seek_idx)?;
    Ok(Demuxer::new(dmx, str, seek_idx))
}

#[derive(Default)]
pub struct RegisteredDemuxers {
    dmxs:   Vec<&'static DemuxerCreator>,
}

impl RegisteredDemuxers {
    pub fn new() -> Self {
        Self { dmxs: Vec::new() }
    }
    pub fn add_demuxer(&mut self, dmx: &'static DemuxerCreator) {
        self.dmxs.push(dmx);
    }
    pub fn find_demuxer(&self, name: &str) -> Option<&DemuxerCreator> {
        for &dmx in self.dmxs.iter() {
            if dmx.get_name() == name {
                return Some(dmx);
            }
        }
        None
    }
}
