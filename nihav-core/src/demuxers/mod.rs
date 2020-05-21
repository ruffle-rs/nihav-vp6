//! Demuxer definitions.
pub use crate::frame::*;
pub use crate::io::byteio::*;

/// A list specifying general demuxing errors.
#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DemuxerError {
    /// Reader got to end of stream.
    EOF,
    /// Demuxer encountered empty container.
    NoSuchInput,
    /// Demuxer encountered invalid input data.
    InvalidData,
    /// Data reading error.
    IOError,
    /// Feature is not implemented.
    NotImplemented,
    /// Allocation failed.
    MemoryError,
    /// The operation should be repeated.
    TryAgain,
    /// Seeking failed.
    SeekError,
    /// Operation cannot succeed in principle (e.g. seeking in a format not supporting seeking).
    NotPossible,
}

/// A specialised `Result` type for demuxing operations.
pub type DemuxerResult<T> = Result<T, DemuxerError>;

/// A trait for demuxing operations.
pub trait DemuxCore<'a> {
    /// Opens the input stream, reads required headers and prepares everything for packet demuxing.
    fn open(&mut self, strmgr: &mut StreamManager, seek_idx: &mut SeekIndex) -> DemuxerResult<()>;
    /// Demuxes a packet.
    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket>;
    /// Seeks to the requested time.
    fn seek(&mut self, time: u64, seek_idx: &SeekIndex) -> DemuxerResult<()>;
}

/// An auxiliary trait to make bytestream reader read packet data.
pub trait NAPacketReader {
    /// Reads input and constructs a packet containing it.
    fn read_packet(&mut self, str: NAStreamRef, ts: NATimeInfo, keyframe: bool, size: usize) -> DemuxerResult<NAPacket>;
    /// Reads input into already existing packet.
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

/// An auxiliary structure for operations with individual streams inside the container.
#[derive(Default)]
pub struct StreamManager {
    streams: Vec<NAStreamRef>,
    ignored: Vec<bool>,
    no_ign:  bool,
}

impl StreamManager {
    /// Constructs a new instance of `StreamManager`.
    pub fn new() -> Self {
        StreamManager {
            streams: Vec::new(),
            ignored: Vec::new(),
            no_ign:  true,
        }
    }
    /// Returns stream iterator.
    pub fn iter(&self) -> StreamIter { StreamIter::new(&self.streams) }

    /// Adds a new stream.
    pub fn add_stream(&mut self, stream: NAStream) -> Option<usize> {
        let stream_num = self.streams.len();
        let mut str = stream.clone();
        str.set_num(stream_num);
        self.streams.push(str.into_ref());
        self.ignored.push(false);
        Some(stream_num)
    }
    /// Returns stream with the requested index.
    pub fn get_stream(&self, idx: usize) -> Option<NAStreamRef> {
        if idx < self.streams.len() {
            Some(self.streams[idx].clone())
        } else {
            None
        }
    }
    /// Returns stream with the requested stream ID.
    pub fn get_stream_by_id(&self, id: u32) -> Option<NAStreamRef> {
        for i in 0..self.streams.len() {
            if self.streams[i].get_id() == id {
                return Some(self.streams[i].clone());
            }
        }
        None
    }
    /// Returns the number of known streams.
    pub fn get_num_streams(&self) -> usize { self.streams.len() }
    /// Reports whether the stream is marked as ignored.
    pub fn is_ignored(&self, idx: usize) -> bool {
        if self.no_ign {
            true
        } else if idx < self.ignored.len() {
            self.ignored[idx]
        } else {
            false
        }
    }
    /// Reports whether the stream with certain ID is marked as ignored.
    pub fn is_ignored_id(&self, id: u32) -> bool {
        for i in 0..self.streams.len() {
            if self.streams[i].get_id() == id {
                return self.ignored[i];
            }
        }
        false
    }
    /// Marks requested stream as ignored.
    pub fn set_ignored(&mut self, idx: usize) {
        if idx < self.ignored.len() {
            self.ignored[idx] = true;
            self.no_ign = false;
        }
    }
    /// Clears the ignored mark for the requested stream.
    pub fn set_unignored(&mut self, idx: usize) {
        if idx < self.ignored.len() {
            self.ignored[idx] = false;
        }
    }
}

/// Stream iterator.
pub struct StreamIter<'a> {
    streams:    &'a [NAStreamRef],
    pos:        usize,
}

impl<'a> StreamIter<'a> {
    /// Constructs a new instance of `StreamIter`.
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

/// Seeking modes.
#[derive(Clone,Copy,PartialEq)]
pub enum SeekIndexMode {
    /// No seeking index present.
    None,
    /// Seeking index is present.
    Present,
    /// Seeking index should be constructed by the demuxer if possible.
    Automatic,
}

impl Default for SeekIndexMode {
    fn default() -> Self { SeekIndexMode::None }
}

/// A structure holding seeking information.
#[derive(Clone,Copy,Default)]
pub struct SeekEntry {
    /// Time in milliseconds.
    pub time:   u64,
    /// PTS
    pub pts:    u64,
    /// Position in file.
    pub pos:    u64,
}

/// Seeking information for individual streams.
#[derive(Clone)]
pub struct StreamSeekInfo {
    /// Stream ID.
    pub id:         u32,
    /// Index is present.
    pub filled:     bool,
    /// Packet seeking information.
    pub entries:    Vec<SeekEntry>,
}

impl StreamSeekInfo {
    /// Constructs a new `StreamSeekInfo` instance.
    pub fn new(id: u32) -> Self {
        Self {
            id,
            filled:     false,
            entries:    Vec::new(),
        }
    }
    /// Adds new seeking point.
    pub fn add_entry(&mut self, entry: SeekEntry) {
        self.entries.push(entry);
    }
    /// Searches for an appropriate seek position before requested time.
    pub fn find_pos(&self, time: u64) -> Option<SeekEntry> {
        if !self.entries.is_empty() {
// todo something faster like binary search
            let mut cand = None;
            for entry in self.entries.iter() {
                if entry.time <= time {
                    cand = Some(*entry);
                } else {
                    break;
                }
            }
            cand
        } else {
            None
        }
    }
}

/// Structure for holding seeking point search results.
#[derive(Clone,Copy,Default)]
pub struct SeekIndexResult {
    /// Packet PTS.
    pub pts:        u64,
    /// Position in file.
    pub pos:        u64,
    /// Stream ID.
    pub str_id:     u32,
}

/// Seek information for the whole container.
#[derive(Default)]
pub struct SeekIndex {
    /// Seek information for individual streams.
    pub seek_info:  Vec<StreamSeekInfo>,
    /// Seeking index mode.
    pub mode:       SeekIndexMode,
    /// Ignore index flag.
    pub skip_index: bool,
}

impl SeekIndex {
    /// Constructs a new `SeekIndex` instance.
    pub fn new() -> Self { Self::default() }
    pub fn add_stream(&mut self, id: u32) -> usize {
        let ret = self.stream_id_to_index(id);
        if ret.is_none() {
            self.seek_info.push(StreamSeekInfo::new(id));
            self.seek_info.len() - 1
        } else {
            ret.unwrap()
        }
    }
    /// Adds a new stream to the index.
    pub fn stream_id_to_index(&self, id: u32) -> Option<usize> {
        for (idx, str) in self.seek_info.iter().enumerate() {
            if str.id == id {
                return Some(idx);
            }
        }
        None
    }
    /// Returns stream reference for provided stream ID.
    pub fn get_stream_index(&mut self, id: u32) -> Option<&mut StreamSeekInfo> {
        for str in self.seek_info.iter_mut() {
            if str.id == id {
                return Some(str);
            }
        }
        None
    }
    /// Adds seeking information to the index.
    pub fn add_entry(&mut self, id: u32, entry: SeekEntry) {
        let mut idx = self.stream_id_to_index(id);
        if idx.is_none() {
            idx = Some(self.add_stream(id));
        }
        self.seek_info[idx.unwrap()].add_entry(entry);
        self.seek_info[idx.unwrap()].filled = true;
    }
    /// Searches for a seek position before requested time.
    pub fn find_pos(&self, time: u64) -> Option<SeekIndexResult> {
        let mut cand = None;
        for str in self.seek_info.iter() {
            if !str.filled { continue; }
            let res = str.find_pos(time);
            if res.is_none() { continue; }
            let res = res.unwrap();
            if cand.is_none() {
                cand = Some(SeekIndexResult { pts: res.pts, pos: res.pos, str_id: str.id });
            } else if let Some(entry) = cand {
                if res.pos < entry.pos {
                    cand = Some(SeekIndexResult { pts: res.pts, pos: res.pos, str_id: str.id });
                }
            }
        }
        cand
    }
}

/// Demuxer structure with auxiliary data.
pub struct Demuxer<'a> {
    dmx:        Box<dyn DemuxCore<'a> + 'a>,
    streams:    StreamManager,
    seek_idx:   SeekIndex,
}

impl<'a> Demuxer<'a> {
    /// Constructs a new `Demuxer` instance.
    fn new(dmx: Box<dyn DemuxCore<'a> + 'a>, str: StreamManager, seek_idx: SeekIndex) -> Self {
        Demuxer {
            dmx,
            streams:    str,
            seek_idx,
        }
    }
    /// Returns a stream reference by its number.
    pub fn get_stream(&self, idx: usize) -> Option<NAStreamRef> {
        self.streams.get_stream(idx)
    }
    /// Returns a stream reference by its ID.
    pub fn get_stream_by_id(&self, id: u32) -> Option<NAStreamRef> {
        self.streams.get_stream_by_id(id)
    }
    /// Reports the total number of streams.
    pub fn get_num_streams(&self) -> usize {
        self.streams.get_num_streams()
    }
    /// Returns a reference to the internal stream manager.
    pub fn get_stream_manager(&self) -> &StreamManager {
        &self.streams
    }
    /// Returns an iterator over streams.
    pub fn get_streams(&self) -> StreamIter {
        self.streams.iter()
    }
    /// Returns 'ignored' marker for requested stream.
    pub fn is_ignored_stream(&self, idx: usize) -> bool {
        self.streams.is_ignored(idx)
    }
    /// Sets 'ignored' marker for requested stream.
    pub fn set_ignored_stream(&mut self, idx: usize) {
        self.streams.set_ignored(idx)
    }
    /// Clears 'ignored' marker for requested stream.
    pub fn set_unignored_stream(&mut self, idx: usize) {
        self.streams.set_unignored(idx)
    }

    /// Demuxes a new packet from the container.
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
    /// Seeks to the requested time (in milliseconds) if possible.
    pub fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        if self.seek_idx.skip_index {
            return Err(DemuxerError::NotPossible);
        }
        self.dmx.seek(time, &self.seek_idx)
    }
    /// Returns internal seek index.
    pub fn get_seek_index(&self) -> &SeekIndex {
        &self.seek_idx
    }
}

impl From<ByteIOError> for DemuxerError {
    fn from(_: ByteIOError) -> Self { DemuxerError::IOError }
}

/// The trait for creating demuxers.
pub trait DemuxerCreator {
    /// Creates new demuxer instance that will use `ByteReader` source as an input.
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a>;
    /// Returns the name of current demuxer creator (equal to the container name it can demux).
    fn get_name(&self) -> &'static str;
}

/// Creates demuxer for a provided bytestream.
pub fn create_demuxer<'a>(dmxcr: &DemuxerCreator, br: &'a mut ByteReader<'a>) -> DemuxerResult<Demuxer<'a>> {
    let mut dmx = dmxcr.new_demuxer(br);
    let mut str = StreamManager::new();
    let mut seek_idx = SeekIndex::new();
    dmx.open(&mut str, &mut seek_idx)?;
    Ok(Demuxer::new(dmx, str, seek_idx))
}

/// List of registered demuxers.
#[derive(Default)]
pub struct RegisteredDemuxers {
    dmxs:   Vec<&'static DemuxerCreator>,
}

impl RegisteredDemuxers {
    /// Constructs a new `RegisteredDemuxers` instance.
    pub fn new() -> Self {
        Self { dmxs: Vec::new() }
    }
    /// Registers a new demuxer.
    pub fn add_demuxer(&mut self, dmx: &'static DemuxerCreator) {
        self.dmxs.push(dmx);
    }
    /// Searches for a demuxer that supports requested container format.
    pub fn find_demuxer(&self, name: &str) -> Option<&DemuxerCreator> {
        for &dmx in self.dmxs.iter() {
            if dmx.get_name() == name {
                return Some(dmx);
            }
        }
        None
    }
}
