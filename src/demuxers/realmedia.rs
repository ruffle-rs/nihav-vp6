use super::*;
use super::DemuxerError::*;
//use io::byteio::*;
//use frame::*;
use formats::*;
use std::io::SeekFrom;
use std::mem;

macro_rules! mktag {
    ($a:expr, $b:expr, $c:expr, $d:expr) => ({
        (($a as u32) << 24) | (($b as u32) << 16) | (($c as u32) << 8) | ($d as u32)
    });
    ($arr:expr) => ({
        (($arr[0] as u32) << 24) | (($arr[1] as u32) << 16) | (($arr[2] as u32) << 8) | ($arr[3] as u32)
    });
}

struct RMVideoStream {
    frame:      Vec<u8>,
    hdr_size:   usize,
    frame_size: usize,
    frame_pos:  usize,
}

impl RMVideoStream {
    fn new() -> Self {
        RMVideoStream {
            frame:      Vec::new(),
            hdr_size:   0,
            frame_size: 0,
            frame_pos:  0,
        }
    }
    fn flush(&mut self) {
        self.frame.truncate(0);
        self.frame_size = 0;
        self.frame_pos  = 0;
    }
    fn start_slice(&mut self, num_slices: usize, frame_size: usize, data: &[u8]) {
        self.hdr_size = num_slices * 8 + 1;
        self.frame.resize(frame_size + self.hdr_size, 0);
        self.frame[0] = (num_slices - 1) as u8;
        self.frame_pos = 0;
        self.add_slice(1, data);
    }
    fn add_slice(&mut self, slice_no: usize, data: &[u8]) {
        self.write_slice_info(slice_no);
        let dslice = &mut self.frame[self.hdr_size + self.frame_pos..][..data.len()];
        dslice.copy_from_slice(data);
        self.frame_pos += data.len();
    }
    fn write_slice_info(&mut self, slice_no: usize) {
        let off = 1 + (slice_no - 1) * 8;
        self.frame[off + 0] = 0;
        self.frame[off + 1] = 0;
        self.frame[off + 2] = 0;
        self.frame[off + 3] = 1;
        self.frame[off + 4] = (self.frame_pos >> 24) as u8;
        self.frame[off + 5] = (self.frame_pos >> 16) as u8;
        self.frame[off + 6] = (self.frame_pos >>  8) as u8;
        self.frame[off + 7] = (self.frame_pos >>  0) as u8;
    }
    fn get_frame_data(&mut self) -> Vec<u8> {
        let mut v: Vec<u8> = Vec::new();
        mem::swap(&mut v, &mut self.frame);
        self.flush();
        v
    }
}

#[allow(dead_code)]
#[derive(Clone,Copy,PartialEq)]
enum Deinterleaver {
    None,
    RA28_8,
    Generic,
    Sipro,
    VBR,
}

#[allow(dead_code)]
struct RMAudioStream {
    deint:      Deinterleaver,
    iinfo:      Option<InterleaveInfo>,
    buf:        Vec<u8>,
    sub_packet: usize,
}

const RM_ILEAVE_INT0: u32 = mktag!(b"Int0");
const RM_ILEAVE_INT4: u32 = mktag!(b"Int4");
const RM_ILEAVE_GENR: u32 = mktag!(b"genr");
const RM_ILEAVE_SIPR: u32 = mktag!(b"sipr");
const RM_ILEAVE_VBRS: u32 = mktag!(b"vbrs");

impl RMAudioStream {
    fn new(iinfo: Option<InterleaveInfo>) -> Self {
        let deint;
        let mut buf: Vec<u8>;
        if let Some(info) = iinfo {
            deint = match info.id {
                    RM_ILEAVE_INT0 => Deinterleaver::None,
                    RM_ILEAVE_INT4 => Deinterleaver::RA28_8,
                    RM_ILEAVE_GENR => Deinterleaver::Generic,
                    RM_ILEAVE_SIPR => Deinterleaver::Sipro,
                    RM_ILEAVE_VBRS => Deinterleaver::VBR,
                    _ => {println!("unknown deint {:X}", info.id); Deinterleaver::None },
                };
            match deint {
                Deinterleaver::None     => { buf = Vec::new(); },
                Deinterleaver::RA28_8  |
                Deinterleaver::Generic |
                Deinterleaver::Sipro    => {
                        let bsize = (info.frame_size as usize) * (info.factor as usize);
                        buf = Vec::with_capacity(bsize);
                        buf.resize(bsize, 0u8);
                    },
                Deinterleaver::VBR      => { unimplemented!("deint"); },
            };
        } else {
            deint = Deinterleaver::None;
            buf = Vec::new();
        }
        RMAudioStream { deint: deint, iinfo: iinfo, buf: buf, sub_packet: 0 }
    }
    fn read_apackets(&mut self, queued_packets: &mut Vec<NAPacket>, src: &mut ByteReader, stream: Rc<NAStream>, ts: u32, keyframe: bool, payload_size: usize) -> DemuxerResult<NAPacket> {
        let (tb_num, tb_den) = stream.get_timebase();
        let ts = NATimeInfo::new(Some(ts as u64), None, None, tb_num, tb_den);

        if keyframe {
            self.sub_packet = 0;
        }
        match self.deint {
            Deinterleaver::None     => { return src.read_packet(stream, ts, keyframe, payload_size); },
            Deinterleaver::RA28_8   => {
                    let iinfo = self.iinfo.unwrap();
                    let factor   = iinfo.factor as usize;
                    let halffact = factor >> 1;
                    let fsize    = iinfo.frame_size as usize;
                    let bsize    = iinfo.block_size as usize;
                    let ppos     = self.sub_packet;
                    for sb in 0..halffact {
                        let mut dst = &mut self.buf[sb * 2 * fsize + ppos * bsize..][..bsize];
                        src.read_buf(&mut dst)?;
                    }
                    self.sub_packet += 1;
                    if self.sub_packet == factor {
                        self.sub_packet = 0;
                        return Ok(NAPacket::new(stream, ts, true, self.buf.clone()));
                    } else {
                        return Err(DemuxerError::TryAgain);
                    }
                },
            Deinterleaver::Generic  => {
                    let iinfo = self.iinfo.unwrap();
                    let factor   = iinfo.factor as usize;
                    let fsize    = iinfo.frame_size as usize;
                    let bsize    = iinfo.block_size as usize;
                    let factor2  = fsize / bsize;
                    let ppos     = self.sub_packet;

                    for sb in 0..factor2 {
                        let sb_pos = factor * sb + ((factor + 1) >> 1) * (ppos & 1) + (ppos >> 1);
                        let mut dst = &mut self.buf[bsize * sb_pos..][..bsize];
                        src.read_buf(&mut dst)?;
                    }
                },
            Deinterleaver::Sipro    => {
                    let iinfo = self.iinfo.unwrap();
                    let fsize    = iinfo.frame_size as usize;
                    let ppos     = self.sub_packet;

                    let mut dst = &mut self.buf[fsize * ppos..][..fsize];
                    src.read_buf(&mut dst)?;
                },
            _                       => { return src.read_packet(stream, ts, keyframe, payload_size); },
        };

        let iinfo = self.iinfo.unwrap();
        let factor   = iinfo.factor as usize;
        let fsize    = iinfo.frame_size as usize;

        self.sub_packet += 1;
        if self.sub_packet < factor {
            return Err(DemuxerError::TryAgain);
        }

        self.sub_packet = 0;

        if self.deint == Deinterleaver::Sipro {
// todo sipro deinterleave
        }

        let mut frames_iter = self.buf.chunks(fsize);
        let pkt0 = frames_iter.next().unwrap();

        let pkt_ts = NATimeInfo::new(None, None, None, tb_num, tb_den);
        for pkts in frames_iter {
            let pkt = NAPacket::new(stream.clone(), pkt_ts, true, pkts.to_vec());
            queued_packets.push(pkt);
        }
        queued_packets.reverse();
        Ok(NAPacket::new(stream, ts, true, pkt0.to_vec()))
    }
}

enum RMStreamType {
    Audio(RMAudioStream),
    Video(RMVideoStream),
    Logical,
    Unknown,
}

struct RealMediaDemuxer<'a> {
    src:            &'a mut ByteReader<'a>,
    data_pos:       u64,
    num_packets:    u32,
    cur_packet:     u32,

    streams:        Vec<RMStreamType>,
    str_ids:        Vec<u16>,

    queued_pkts:    Vec<NAPacket>,
    slice_buf:      Vec<u8>,
}

fn find_codec_name(registry: &[(&[u8;4], &'static str)], fcc: u32) -> &'static str {
    for &(fourcc, name) in registry {
        if mktag!(fourcc) == fcc { return name; }
    }
    "unknown"
}

fn read_14or30(src: &mut ByteReader) -> DemuxerResult<(bool, u32)> {
    let tmp = src.read_u16be()?;
    let flag = (tmp & 0x8000) != 0;
    if (tmp & 0x4000) == 0x4000 {
        Ok((flag, ((tmp & 0x3FFF) as u32)))
    } else {
        let val = ((tmp as u32) << 16) | (src.read_u16be()? as u32);
        Ok((flag, val & 0x3FFFFFFF))
    }
}

fn read_video_buf(src: &mut ByteReader, stream: Rc<NAStream>, ts: u32, keyframe: bool, frame_size: usize) -> DemuxerResult<NAPacket> {
    let size = (frame_size as usize) + 9;
    let mut vec: Vec<u8> = Vec::with_capacity(size);
    vec.resize(size, 0);
    //v[0] = 0; // 1 slice
    vec[4] = 1;
    src.read_buf(&mut vec[9..])?;

    let (tb_num, tb_den) = stream.get_timebase();
    let ts = NATimeInfo::new(Some(ts as u64), None, None, tb_num, tb_den);
    Ok(NAPacket::new(stream, ts, keyframe, vec))
}

fn read_multiple_frame(src: &mut ByteReader, stream: Rc<NAStream>, keyframe: bool, skip_mtype: bool) -> DemuxerResult<NAPacket> {
    if !skip_mtype {
        let mtype       = src.read_byte()?;
        validate!(mtype == 0xC0);
    }
    let (_, frame_size) = read_14or30(src)?;
    let (_, timestamp)  = read_14or30(src)?;
    let _seq_no         = src.read_byte()?;
//println!("  multiple frame size {} ts {} seq {}", frame_size, timestamp, seq_no);

    read_video_buf(src, stream, timestamp, keyframe, frame_size as usize)
}

impl<'a> DemuxCore<'a> for RealMediaDemuxer<'a> {
    #[allow(unused_variables)]
    fn open(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        self.read_header(strmgr)?;
        Ok(())
    }

#[allow(unused_variables)]
    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if !self.queued_pkts.is_empty() {
            let pkt = self.queued_pkts.pop().unwrap();
            return Ok(pkt);
        }
        loop {
            if self.cur_packet >= self.num_packets { return Err(DemuxerError::EOF); }

            let pkt_start = self.src.tell();
            let ver             = self.src.read_u16le()?;
            validate!(ver <= 1);
            let len             = self.src.read_u16be()? as usize;
            let str_no          = self.src.read_u16be()?;
            let ts              = self.src.read_u32be()?;
            let _pkt_grp;
            if ver == 0 {
                _pkt_grp         = self.src.read_byte()?;
            } else {
                //asm_rule        = self.src.read_u16le()?;
                self.src.read_skip(2)?;
                _pkt_grp = 0;
            }
            let flags           = self.src.read_byte()?;
            let hdr_size = self.src.tell() - pkt_start;
//println!("packet @{:X} size {} for {} ts {} grp {} flags {:X}", pkt_start, len, str_no, ts, _pkt_grp, flags);
            self.cur_packet += 1;

            let payload_size = len - (hdr_size as usize);

            let sr = self.str_ids.iter().position(|x| *x == str_no);
            if sr.is_none() {
//println!("stream {} not found", str_no);
                self.src.read_skip(payload_size)?;
                return Err(DemuxerError::InvalidData);
            }
            let str_id = sr.unwrap();
            let streamres = strmgr.get_stream_by_id(str_no as u32);
            if streamres.is_none() {
                self.src.read_skip(payload_size)?;
                continue;
            }
            let stream = streamres.unwrap();
//println!("  stream {}", str_id);
            if strmgr.is_ignored_id(str_no as u32) {
                self.src.read_skip(payload_size)?;
                continue;
            }
            //todo skip unwanted packet
            let keyframe = (flags & KEYFRAME_FLAG) != 0;

            let result = match self.streams[str_id] {
                RMStreamType::Video(ref mut vstr) => {

                        let pos = self.src.tell();
                        let b0          = self.src.read_byte()?;
                        match b0 >> 6 {
                            0 => { // partial frame
                                    let b1  = self.src.read_byte()?;
                                    let hdr1 = ((b0 as u16) << 8) | (b1 as u16);
                                    let num_pkts = ((hdr1 >> 7) & 0x7F) as usize;
                                    let packet_num = hdr1 & 0x7F;
                                    let (_, frame_size) = read_14or30(self.src)?;
                                    let (_, off)        = read_14or30(self.src)?;
                                    let seq_no = self.src.read_byte()?;
//println!(" mode 0 pkt {}/{} off {}/{} seq {}", packet_num, num_pkts, off, frame_size, seq_no);
                                    let hdr_skip = (self.src.tell() - pos) as usize;

                                    let slice_size = (payload_size - hdr_skip) as usize;
                                    self.slice_buf.resize(slice_size, 0);
                                    self.src.read_buf(self.slice_buf.as_mut_slice())?;
                                    if packet_num == 1 {
                                        vstr.start_slice(num_pkts, frame_size as usize, self.slice_buf.as_slice());
                                    } else {
                                        vstr.add_slice(packet_num as usize, self.slice_buf.as_slice()); 
                                    }
                                    if (packet_num as usize) < num_pkts {
                                        continue;
                                    }
                                    //todo: check if full frame is received
                                    let (tb_num, tb_den) = stream.get_timebase();
                                    let ts = NATimeInfo::new(Some(ts as u64), None, None, tb_num, tb_den);
                                    let pkt = NAPacket::new(stream, ts, keyframe, vstr.get_frame_data());
                                    Ok(pkt)
                                },
                            1 => { // whole frame
                                    let seq_no = self.src.read_byte()?;
//println!(" mode 1 seq {}", seq_no);
                                    read_video_buf(self.src, stream, ts, keyframe, payload_size - 1)
                                },
                            2 => { // last partial frame
                                    let b1  = self.src.read_byte()?;
                                    let hdr1 = ((b0 as u16) << 8) | (b1 as u16);
                                    let num_pkts = ((hdr1 >> 7) & 0x7F) as usize;
                                    let packet_num = hdr1 & 0x7F;
                                    let (_, frame_size) = read_14or30(self.src)?;
                                    let (_, tail_size)  = read_14or30(self.src)?;
                                    let seq_no = self.src.read_byte()?;
//println!(" mode 2 pkt {}/{} tail {}/{} seq {}", packet_num, num_pkts, tail_size, frame_size, seq_no);
                                    self.slice_buf.resize(tail_size as usize, 0);
                                    self.src.read_buf(self.slice_buf.as_mut_slice())?;
                                    if packet_num == 1 && frame_size == tail_size {
                                        vstr.start_slice(num_pkts, frame_size as usize, self.slice_buf.as_slice());
                                    } else {
                                        vstr.add_slice(packet_num as usize, self.slice_buf.as_slice()); 
                                    }

                                    while self.src.tell() < pos + (payload_size as u64) {
                                        let res = read_multiple_frame(self.src, stream.clone(), false, false);
                                        if res.is_err() { break; }
                                        self.queued_pkts.push(res.unwrap());
                                    }
                                    self.queued_pkts.reverse();
                                    let (tb_num, tb_den) = stream.get_timebase();
                                    let ts = NATimeInfo::new(Some(ts as u64), None, None, tb_num, tb_den);
                                    let pkt = NAPacket::new(stream, ts, keyframe, vstr.get_frame_data());
                                    Ok(pkt)
                            },
                        _ => { // multiple frames
//println!(" mode 3");
                                    let res = read_multiple_frame(self.src, stream.clone(), keyframe, true);
                                    if res.is_err() { return res; }
                                    while self.src.tell() < pos + (payload_size as u64) {
                                        let res = read_multiple_frame(self.src, stream.clone(), false, false);
                                        if res.is_err() { break; }
                                        self.queued_pkts.push(res.unwrap());
                                    }
                                    self.queued_pkts.reverse();
                                    res
                                },
                        }
                    },
                RMStreamType::Audio(ref mut astr) => {
                        let ret = astr.read_apackets(&mut self.queued_pkts, &mut self.src, stream, ts, keyframe, payload_size);
                        if let Err(DemuxerError::TryAgain) = ret {
                            continue;
                        }
                        ret
                    },
                _ => {
//                        self.src.read_skip(payload_size)?;
                        Err(DemuxerError::InvalidData)
                    },
            };
            return result;
        }
    }

    #[allow(unused_variables)]
    fn seek(&mut self, time: u64) -> DemuxerResult<()> {
        Err(NotImplemented)
    }
}

fn read_chunk(src: &mut ByteReader) -> DemuxerResult<(u32, u32, u16)> {
    let id      = src.read_u32be()?;
if id == 0 { return Ok((0, 0, 0)); }
    let size    = src.read_u32be()?;
if size == 0 {
    let ver     = src.read_u16be()?;
    validate!(ver <= 1);
    return Ok((id, 0x0FFFFFFF, ver));
}
    validate!(size >= 10);
    let ver     = src.read_u16be()?;
    validate!(ver <= 1);
    Ok((id, size, ver))
}

#[derive(Clone,Copy,Debug)]
struct InterleaveInfo {
    id:         u32,
    factor:     u16,
    block_size: u16,
    frame_size: u16,
}

#[derive(Clone,Copy,Debug)]
struct RealAudioInfo {
    fcc:                u32,
    sample_rate:        u32,
    sample_size:        u16,
    channels:           u16,
    channel_mask:       u32,
    granularity:        u32,
    bytes_per_minute:   u32,
    total_bytes:        u32,
    edata_size:         u32,
    ileave_info:        Option<InterleaveInfo>
}

fn skip_ra_metadata(src: &mut ByteReader) -> DemuxerResult<()> {
    let title_len           = src.read_byte()? as usize;
    src.read_skip(title_len)?;
    let author_len          = src.read_byte()? as usize;
    src.read_skip(author_len)?;
    let copywrong_len       = src.read_byte()? as usize;
    src.read_skip(copywrong_len)?;
    let comment_len         = src.read_byte()? as usize;
    src.read_skip(comment_len)?;
    Ok(())
}

#[allow(unused_variables)]
fn parse_aformat3(src: &mut ByteReader) -> DemuxerResult<RealAudioInfo> {
    let start = src.tell();
    let header_len          = src.read_u16be()?;
    validate!(header_len >= 24);
    let flavor              = src.read_u16be()?;
    let granularity         = src.read_u32be()?;
    let bytes_per_minute    = src.read_u32be()?;
    let total_bytes         = src.read_u32be()?;

    skip_ra_metadata(src)?;

    let _can_copy           = src.read_byte()?;
    let fcc_len             = src.read_byte()?;
    validate!(fcc_len == 4);
    let fcc                 = src.read_u32be()?;

    let end = src.tell();
    validate!(end - start <= (header_len as u64) + 2);

    Ok(RealAudioInfo {
        fcc: fcc, sample_rate: 8000, sample_size: 16, channels: 1, channel_mask: 0,
        granularity: granularity, bytes_per_minute: bytes_per_minute,
        total_bytes: total_bytes, edata_size: 0,
        ileave_info: None,
    })
}

#[allow(unused_variables)]
fn parse_aformat4(src: &mut ByteReader) -> DemuxerResult<RealAudioInfo> {
    let start = src.tell();
    src.read_skip(2)?; // zeroes
    let id                  = src.read_u32be()?;
    validate!(id == mktag!(b".ra4"));
    let data_size           = src.read_u32be()?;
    let _ver4               = src.read_u16be()?; // should be 4
    let header_size         = src.read_u32be()?;
    let _flavor             = src.read_u16be()?;
    let granularity         = src.read_u32be()?;
    let total_bytes         = src.read_u32be()?;
    let bytes_per_minute    = src.read_u32be()?;
    let _bytes_per_minute2  = src.read_u32be()?;
    let ileave_factor       = src.read_u16be()?;
    let ileave_block_size   = src.read_u16be()?;
    let _user_data          = src.read_u16be()?;
    let sample_rate         = src.read_u32be()?;
    let sample_size         = src.read_u32be()?;
    let channels            = src.read_u16be()?;
    let interleaver_id_len  = src.read_byte()?;
    validate!(interleaver_id_len == 4);
    let interleaver_id      = src.read_u32be()?;
    let fcc_len             = src.read_byte()?;
    validate!(fcc_len == 4);
    let fcc                 = src.read_u32be()?;
    let is_interleaved      = src.read_byte()?;
    let _can_copy           = src.read_byte()?;
    let _stream_type        = src.read_byte()?;

    skip_ra_metadata(src)?;

    let end = src.tell();
    validate!(end - start <= (header_size as u64) + 10);

    let ileave_info = if is_interleaved != 0 {
            Some(InterleaveInfo {
                    id: interleaver_id, factor: ileave_factor, block_size: granularity as u16,
                    frame_size: ileave_block_size,
                })
        } else {
            None
        };

    Ok(RealAudioInfo {
        fcc: fcc, sample_rate: sample_rate, sample_size: sample_size as u16, channels: channels, channel_mask: 0,
        granularity: granularity, bytes_per_minute: bytes_per_minute,
        total_bytes: total_bytes & 0xFFFFFF, edata_size: 0,
        ileave_info: ileave_info,
    })
}

#[allow(unused_variables)]
fn parse_aformat5(src: &mut ByteReader) -> DemuxerResult<RealAudioInfo> {
    let start = src.tell();
    src.read_skip(2)?; // zeroes
    let id                  = src.read_u32be()?;
    validate!(id == mktag!(b".ra5"));
    let data_size           = src.read_u32be()?;
    let _ver5               = src.read_u16be()?; // should be 5
    let header_size         = src.read_u32be()?;
    let _flavor             = src.read_u16be()?;
    let granularity         = src.read_u32be()?;
    let total_bytes         = src.read_u32be()?;
    let bytes_per_minute    = src.read_u32be()?;
    let _bytes_per_minute2  = src.read_u32be()?;
    let ileave_factor       = src.read_u16be()?;
    let frame_size          = src.read_u16be()?;
    let ileave_block_size   = src.read_u16be()?;
    let user_data           = src.read_u32be()?;
    let _sample_rate1       = src.read_u16be()?;
    let sample_rate         = src.read_u32be()?;
    let sample_size         = src.read_u32be()?;
    let channels            = src.read_u16be()?;
    let interleaver_id      = src.read_u32be()?;
    let fcc                 = src.read_u32be()?;
    let is_interleaved      = src.read_byte()?;
    let _can_copy           = src.read_byte()?;
    let _stream_type        = src.read_byte()?;
    let has_ileave_pattern  = src.read_byte()?;
    if has_ileave_pattern != 0 {
unimplemented!("ra5 interleave pattern");
    }
    let edata_size          = src.read_u32be()?;

    let end = src.tell();
    validate!(end - start <= (header_size as u64) + 10);
    src.read_skip((end as usize) - (header_size as usize))?;

    let ileave_info = if is_interleaved != 0 {
            Some(InterleaveInfo {
                    id: interleaver_id, factor: ileave_factor, block_size: ileave_block_size, frame_size: frame_size,
                })
        } else {
            None
        };

    Ok(RealAudioInfo {
        fcc: fcc, sample_rate: sample_rate, sample_size: sample_size as u16, channels: channels, channel_mask: 0,
        granularity: granularity, bytes_per_minute: bytes_per_minute,
        total_bytes: total_bytes & 0xFFFFFF, edata_size: edata_size,
        ileave_info: ileave_info,
    })
}

const RMVB_HDR_SIZE:  u32 = 18;
const RMVB_PROP_SIZE: u32 = 50;
const KEYFRAME_FLAG: u8 = 0x02;

impl<'a> RealMediaDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        RealMediaDemuxer {
            src:            io,
            data_pos:       0,
            num_packets:    0,
            cur_packet:     0,
            streams:        Vec::new(),
            str_ids:        Vec::new(),
            queued_pkts:    Vec::new(),
            slice_buf:      Vec::new(),
        }
    }
#[allow(unused_variables)]
    fn read_header(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let (id, size, ver) = read_chunk(self.src)?;
        validate!(id == mktag!(b".RMF"));
        validate!(size >= RMVB_HDR_SIZE);
        let fver    = self.src.read_u32be()?;
        validate!(fver <= 1);
        let num_hdr = self.src.read_u32be()? as usize;
        validate!(num_hdr >= 1);
        if size > RMVB_HDR_SIZE {
            self.src.read_skip((size - RMVB_HDR_SIZE) as usize)?;
        }

        let (id, size, ver) = read_chunk(self.src)?;
        validate!(size >= RMVB_PROP_SIZE);
        validate!(ver == 0);
        let maxbr       = self.src.read_u32be()?;
        let avgbr       = self.src.read_u32be()?;
        let maxps       = self.src.read_u32be()?;
        let avgps       = self.src.read_u32be()?;
        let num_pkt     = self.src.read_u32be()? as usize;
        let duration    = self.src.read_u32be()?;
        let preroll     = self.src.read_u32be()?;
        let idx_off     = self.src.read_u32be()?;
        let data_off    = self.src.read_u32be()?;
        let num_streams = self.src.read_u16be()? as usize;
        let flags       = self.src.read_u16be()?;
        if size > RMVB_PROP_SIZE {
            self.src.read_skip((size - RMVB_PROP_SIZE) as usize)?;
        }

        for _ in 0..num_hdr {
            if self.src.is_eof() {
                //warn maybe?
                break;
            }
            let res = self.parse_chunk(strmgr);
            match res {
                Ok(last) => { if last { break; } },
                Err(DemuxerError::IOError) => { break; },
                Err(etype) => { return Err(etype); },
            };
        }
//println!("now @ {:X} / {}", self.src.tell(), self.data_pos);
        validate!(self.data_pos > 0);
        self.src.seek(SeekFrom::Start(self.data_pos))?;
        let num_packets     = self.src.read_u32be()?;
        let next_data_hdr   = self.src.read_u32be()?;
        self.num_packets = if num_packets > 0 { num_packets } else { 0xFFFFFF };
        self.cur_packet  = 0;
        Ok(())
    }
    fn parse_chunk(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<bool> {
        let (id, size, ver) = read_chunk(self.src)?;
        let end_pos = self.src.tell() - 10 + (size as u64);

        validate!(ver == 0);
             if id == mktag!(b"CONT") { self.parse_content_desc()?; }
        else if id == mktag!(b"MDPR") { self.parse_mdpr(strmgr)?; }
        else if id == mktag!(b"DATA") { if self.data_pos == 0 { self.data_pos = self.src.tell(); } }
        else if id == mktag!(b"INDX") { /* do nothing for now */ }
        else if id == 0               { return Ok(true); }
        else                          { println!("unknown chunk type {:08X}", id); }

        let cpos = self.src.tell();
        if cpos < end_pos {
            self.src.read_skip((end_pos - cpos) as usize)?;
        }
        Ok(false)
    }
#[allow(unused_variables)]
    fn parse_content_desc(&mut self) -> DemuxerResult<()> {
        let title_len       = self.src.read_u16be()? as usize;
        self.src.read_skip(title_len)?;
        let author_len      = self.src.read_u16be()? as usize;
        self.src.read_skip(author_len)?;
        let copywrong_len   = self.src.read_u16be()? as usize;
        self.src.read_skip(copywrong_len)?;
        let comment_len     = self.src.read_u16be()? as usize;
        self.src.read_skip(comment_len)?;
        Ok(())
    }
#[allow(unused_variables)]
    fn parse_mdpr(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        let stream_no       = self.src.read_u16be()?;
//todo check stream_no for duplicates
        let maxbr           = self.src.read_u32be()?;
        let avgbr           = self.src.read_u32be()?;
        let maxps           = self.src.read_u32be()?;
        let avgps           = self.src.read_u32be()?;
        let start           = self.src.read_u32be()?;
        let preroll         = self.src.read_u32be()?;
        let duration        = self.src.read_u32be()?;
        let sname_size      = self.src.read_byte()? as usize;
        let sname           = read_string_size(self.src, sname_size)?;
//println!("str #{} sname = {} pkts {}/{} start {} preroll {}", stream_no, sname, maxps, avgps, start, preroll);
        let mime_size       = self.src.read_byte()? as usize;
        let mime            = read_string_size(self.src, mime_size)?;
//println!("mime = {}", mime);
        let edata_size      = self.src.read_u32be()? as usize;
        let edata: Option<Vec<u8>> = if edata_size == 0 { None } else {
            let mut edvec: Vec<u8> = Vec::with_capacity(edata_size);
            edvec.resize(edata_size, 0);
            self.src.read_buf(&mut edvec)?;
            Some(edvec)
        };
        self.str_ids.push(stream_no);
        if edata_size > 8 {
            if let Some(edata_) = edata {
                let mut mr = MemoryReader::new_read(edata_.as_slice());
                let mut src = ByteReader::new(&mut mr);

                let tag  = src.read_u32be()?;
                let tag2 = src.peek_u32be()?;
//println!("tag1 {:X} tag2 {:X}", tag, tag2);
                if tag == mktag!('.', 'r', 'a', 0xFD) {
                    let ver         = src.read_u16be()?;
                    let ainfo = match ver {
                        3 => {
                                parse_aformat3(&mut src)?
                            },
                        4 => {
                                parse_aformat4(&mut src)?
                            },
                        5 => {
                                parse_aformat5(&mut src)?
                            },
                        _ => {
                                println!("unknown version {}", ver);
                                return Err(DemuxerError::InvalidData);
                            },
                    };
println!(" got ainfo {:?}", ainfo);
                    let cname = find_codec_name(RM_AUDIO_CODEC_REGISTER, ainfo.fcc);
                    let srate = ainfo.sample_rate;
                    let soniton = NASoniton::new(ainfo.sample_size as u8, SONITON_FLAG_SIGNED);
                    let ahdr = NAAudioInfo::new(srate, ainfo.channels as u8, soniton, 1);
                    let extradata = if ainfo.edata_size == 0 {
                            None
                        } else {
                            let eslice = &edata_[(src.tell() as usize)..];
                            Some(eslice.to_vec())
                        };
                    let nainfo = NACodecInfo::new(cname, NACodecTypeInfo::Audio(ahdr), extradata);
                    let res = strmgr.add_stream(NAStream::new(StreamType::Audio, stream_no as u32, nainfo, 1, srate));
                    if res.is_none() { return Err(MemoryError); }

                    let astr = RMAudioStream::new(ainfo.ileave_info);
                    self.streams.push(RMStreamType::Audio(astr));
                } else if ((tag2 == mktag!('V', 'I', 'D', 'O')) || (tag2 == mktag!('I', 'M', 'A', 'G'))) && ((tag as usize) <= edata_size) {
                    src.read_skip(4)?;
                    let fcc         = src.read_u32be()?;
                    let width       = src.read_u16be()? as usize;
                    let height      = src.read_u16be()? as usize;
                    let bpp         = src.read_u16be()?;
                    let pad_w       = src.read_u16be()?;
                    let pad_h       = src.read_u16be()?;
                    let fps;
                    if tag2 == mktag!('V', 'I', 'D', 'O') {
                        fps         = src.read_u32be()?;
                    } else {
                        fps = 0x10000;
                    }
                    let extradata: Option<Vec<u8>>;
                    if src.left() > 0 {
                        let eslice = &edata_[(src.tell() as usize)..];
                        extradata = Some(eslice.to_vec());
                    } else {
                        extradata = None;
                    }
                    let cname = find_codec_name(RM_VIDEO_CODEC_REGISTER, fcc);

                    let vhdr = NAVideoInfo::new(width, height, false, RGB24_FORMAT);
                    let vinfo = NACodecInfo::new(cname, NACodecTypeInfo::Video(vhdr), extradata);
                    let res = strmgr.add_stream(NAStream::new(StreamType::Video, stream_no as u32, vinfo, 0x10000, fps));
                    if res.is_none() { return Err(DemuxerError::MemoryError); }

                    let vstr = RMVideoStream::new();
                    self.streams.push(RMStreamType::Video(vstr));
                } else if tag == mktag!(b"LSD:") {
                    let extradata = Some(edata_.to_vec());

                    src.read_skip(4)?; //version
                    let channels    = src.read_u16be()?;
                    let samp_size   = src.read_u16be()?;
                    let sample_rate = src.read_u32be()?;

                    println!("LSD sr {}, {} ch", sample_rate, channels);
                    let soniton = NASoniton::new(samp_size as u8, SONITON_FLAG_SIGNED);
                    let ahdr = NAAudioInfo::new(sample_rate, channels as u8, soniton, 1);
                    let nainfo = NACodecInfo::new("ralf", NACodecTypeInfo::Audio(ahdr), extradata);
                    let res = strmgr.add_stream(NAStream::new(StreamType::Audio, stream_no as u32, nainfo, 1, sample_rate));
                    if res.is_none() { return Err(MemoryError); }
                    let astr = RMAudioStream::new(None);
                    self.streams.push(RMStreamType::Audio(astr));
                } else {
                    self.streams.push(RMStreamType::Logical);
                }
            }
        } else {
            self.streams.push(RMStreamType::Unknown);
        }

        Ok(())
    }
/*#[allow(unused_variables)]
    fn read_pkt_header(&mut self) -> DemuxerResult<()> {
        let ver             = self.src.read_u16be()?;
        validate!(ver <= 1);
        let str_no          = self.src.read_u16be()?;
        let timestamp       = self.src.read_u32be()?;
        if ver == 0 {
            let pkt_group   = self.src.read_byte()?;
            let pkt_flags   = self.src.read_byte()?;
        } else {
            let asm_rule    = self.src.read_u16be()?;
            let asm_flags   = self.src.read_byte()?;
        }
        Ok(())
    }*/
}

fn read_string(src: &mut ByteReader) -> DemuxerResult<String> {
    let mut vec: Vec<u8> = Vec::new();
    loop {
        let c = src.read_byte()?;
        if c == 0 { break; }
        vec.push(c);
    }
    let str = String::from_utf8(vec);
    if str.is_ok() {
        Ok(str.unwrap())
    } else {
        Ok(String::new())
    }
}

fn read_string_size(src: &mut ByteReader, size: usize) -> DemuxerResult<String> {
    let mut vec: Vec<u8> = Vec::new();
    for _ in 0..size {
        let c = src.read_byte()?;
        vec.push(c);
    }
    let str = String::from_utf8(vec);
    if str.is_ok() {
        Ok(str.unwrap())
    } else {
        Ok(String::new())
    }
}

#[allow(dead_code)]
#[allow(unused_variables)]
fn parse_rm_stream(io: &mut ByteReader) -> DemuxerResult<NAStream> {
    let mimetype    = read_string(io)?;
    let strname     = read_string(io)?;
    let strnum      = io.read_u32le()?;
    let maxbr       = io.read_u32le()?;
    let avgbr       = io.read_u32le()?;
    let maxsize     = io.read_u32le()?;
    let avgsize     = io.read_u32le()?;
    let duration    = io.read_u32le()?;
    let preroll     = io.read_u32le()?;
    let start       = io.read_u32le()?;
    let edatalen    = io.read_u32le()? as usize;
    let mut edata: Vec<u8> = Vec::with_capacity(edatalen);
    edata.resize(edatalen, 0);
    io.read_buf(&mut edata)?;
    let numprops    = io.read_u32le()? as usize;
    //read properties
    unimplemented!();
}

#[allow(dead_code)]
#[allow(unused_variables)]
fn read_ra_vbr_stream(io: &mut ByteReader) -> DemuxerResult<NAPacket> {
    let hdrsizesize = io.read_u16le()?;
    let num_entries = (hdrsizesize / 16) as usize;
    let mut sizes: Vec<usize> = Vec::with_capacity(num_entries);
    for _ in 0..num_entries {
        let sz      = io.read_u16le()? as usize;
        sizes.push(sz);
    }
    for i in 0..num_entries {
//read packet of sizes[i]
    }
    unimplemented!();
}

//todo interleavers

//todo opaque data


static RM_VIDEO_CODEC_REGISTER: &'static [(&[u8;4], &str)] = &[
    (b"RV10", "realvideo1"),
    (b"RV20", "realvideo2"),
    (b"RVTR", "realvideo2"),
    (b"RV30", "realvideo3"),
    (b"RV40", "realvideo4"),
    (b"CLV1", "clearvideo_rm"),
];

#[allow(dead_code)]
static RM_AUDIO_CODEC_REGISTER: &'static [(&[u8;4], &str)] = &[
    (b"lpcJ", "ra14.4"),
    (b"28_8", "ra28.8"),
    (b"cook", "cook"),
    (b"dnet", "ac3"),
    (b"sipr", "sipro"),
    (b"atrc", "atrac3"),
    (b"LSD:", "ralf"),
    (b"raac", "aac"),
    (b"racp", "aac"),
];

pub struct RealMediaDemuxerCreator { }

impl DemuxerCreator for RealMediaDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<DemuxCore<'a> + 'a> {
        Box::new(RealMediaDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "realmedia" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_rm_demux() {
        let mut file =
            File::open("assets/RV/rv10_dnet_640x352_realvideo_encoder_4.0.rm").unwrap();
//            File::open("assets/RV/rv20_cook_640x352_realproducer_plus_8.51.rm").unwrap();
//            File::open("assets/RV/rv20_svt_atrc_640x352_realproducer_plus_8.51.rm").unwrap();
//            File::open("assets/RV/rv30_atrc_384x208_realproducer_plus_8.51.rm").unwrap();
//            File::open("assets/RV/rv30_chroma_drift.rm").unwrap();
//            File::open("assets/RV/rv30_weighted_mc.rm").unwrap();
//            File::open("assets/RV/rv40_weighted_mc.rmvb").unwrap();
//            File::open("assets/RV/rv40_weighted_mc_2.rmvb").unwrap();
//            File::open("assets/RV/clv1_sipr_384x208_realvideo_encoder_4.0.rm").unwrap();
//            File::open("assets/RV/luckynight.rmvb").unwrap();
//            File::open("assets/RV/rv40_ralf.rmvb").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = RealMediaDemuxer::new(&mut br);
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
//panic!("the end");
    }
}
