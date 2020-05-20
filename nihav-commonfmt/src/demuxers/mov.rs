use nihav_core::demuxers::*;
use nihav_registry::register::*;

macro_rules! mktag {
    ($a:expr, $b:expr, $c:expr, $d:expr) => ({
        (($a as u32) << 24) | (($b as u32) << 16) | (($c as u32) << 8) | ($d as u32)
    });
    ($arr:expr) => ({
        (($arr[0] as u32) << 24) | (($arr[1] as u32) << 16) | (($arr[2] as u32) << 8) | ($arr[3] as u32)
    });
}

trait Skip64 {
    fn skip64(&mut self, size: u64) -> ByteIOResult<()>;
}

impl<'a> Skip64 for ByteReader<'a> {
    fn skip64(&mut self, size: u64) -> ByteIOResult<()> {
        if (size as usize as u64) != size {
            self.seek(SeekFrom::Current(size as i64))?;
        } else {
            self.read_skip(size as usize)?;
        }
        Ok(())
    }
}

fn read_chunk_header(br: &mut ByteReader) -> DemuxerResult<(u32, u64)> {
    let size            = br.read_u32be()?;
    let ctype           = br.read_u32be()?;
    if size == 0 {
        Ok((ctype, br.left() as u64))
    } else if size == 1 {
        let size64      = br.read_u64be()?;
        validate!(size64 >= 16);
        Ok((ctype, size64 - 16))
    } else {
        validate!(size >= 8);
        Ok((ctype, (size as u64) - 8))
    }
}

fn read_palette(br: &mut ByteReader, size: u64, pal: &mut [u8; 1024]) -> DemuxerResult<u64> {
    let _seed           = br.read_u32be()?;
    let _flags          = br.read_u16be()?;
    let palsize         = (br.read_u16be()? as usize) + 1;
    validate!(palsize <= 256);
    validate!((palsize as u64) * 8 + 8 == size);
    for i in 0..palsize {
        let a           = br.read_u16be()?;
        let r           = br.read_u16be()?;
        let g           = br.read_u16be()?;
        let b           = br.read_u16be()?;
        pal[i * 4]     = (r >> 8) as u8;
        pal[i * 4 + 1] = (g >> 8) as u8;
        pal[i * 4 + 2] = (b >> 8) as u8;
        pal[i * 4 + 3] = (a >> 8) as u8;
    }
    Ok(size)
}

struct RootChunkHandler {
    ctype:  u32,
    parse:  fn(dmx: &mut MOVDemuxer, strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64>,
}

struct TrackChunkHandler {
    ctype:  u32,
    parse:  fn(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64>,
}

const IGNORED_CHUNKS: &[u32] = &[
    mktag!(b"free"), mktag!(b"skip"), mktag!(b"udta"), mktag!(b"wide")
];

const ROOT_CHUNK_HANDLERS: &[RootChunkHandler] = &[
    RootChunkHandler { ctype: mktag!(b"ftyp"), parse: read_ftyp },
    RootChunkHandler { ctype: mktag!(b"mdat"), parse: read_mdat },
    RootChunkHandler { ctype: mktag!(b"moov"), parse: read_moov },
];

macro_rules! read_chunk_list {
    (root; $name: expr, $fname: ident, $handlers: ident) => {
        fn $fname(&mut self, strmgr: &mut StreamManager, size: u64) -> DemuxerResult<()> {
            self.depth += 1;
            validate!(self.depth < 32);
            let list_end = self.src.tell() + size;
            while self.src.tell() < list_end {
                let ret = read_chunk_header(&mut self.src);
                if ret.is_err() { break; }
                let (ctype, size) = ret.unwrap();
                if self.src.tell() + size > list_end {
                    break;
                }
                if IGNORED_CHUNKS.contains(&ctype) {
                    self.src.skip64(size)?;
                    continue;
                }
                let handler = $handlers.iter().find(|x| x.ctype == ctype);
                let read_size;
                if let Some(ref handler) = handler {
                    read_size = (handler.parse)(self, strmgr, size)?;
                } else {
                    println!("skipping unknown chunk {:08X} size {}", ctype, size);
                    read_size = 0;
                }
                validate!(read_size <= size);
                self.src.skip64(size - read_size)?;
            }
            self.depth -= 1;
            validate!(self.src.tell() == list_end);
            Ok(())
        }
    };
    (track; $name: expr, $fname: ident, $handlers: ident) => {
        fn $fname(&mut self, br: &mut ByteReader, size: u64) -> DemuxerResult<()> {
            self.depth += 1;
            validate!(self.depth < 32);
            let list_end = br.tell() + size;
            while br.tell() < list_end {
                let ret = read_chunk_header(br);
                if ret.is_err() { break; }
                let (ctype, size) = ret.unwrap();
                if br.tell() + size > list_end {
                    break;
                }
                if IGNORED_CHUNKS.contains(&ctype) {
                    br.skip64(size)?;
                    continue;
                }
                let handler = $handlers.iter().find(|x| x.ctype == ctype);
                let read_size;
                if let Some(ref handler) = handler {
                    read_size = (handler.parse)(self, br, size)?;
                } else {
                    read_size = 0;
                }
                validate!(read_size <= size);
                br.skip64(size - read_size)?;
            }
            self.depth -= 1;
            validate!(br.tell() == list_end);
            Ok(())
        }
    }
}

fn skip_chunk(_track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    br.skip64(size)?;
    Ok(size)
}

fn read_ftyp(dmx: &mut MOVDemuxer, _strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    dmx.src.skip64(size)?;
    Ok(size)
}

fn read_mdat(dmx: &mut MOVDemuxer, _strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    dmx.mdat_pos  = dmx.src.tell();
    dmx.mdat_size = size;
    dmx.src.skip64(size)?;
    Ok(size)
}

fn read_moov(dmx: &mut MOVDemuxer, strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    dmx.read_moov(strmgr, size)?;
    Ok(size)
}

const MOOV_CHUNK_HANDLERS: &[RootChunkHandler] = &[
    RootChunkHandler { ctype: mktag!(b"mvhd"), parse: read_mvhd },
    RootChunkHandler { ctype: mktag!(b"ctab"), parse: read_ctab },
    RootChunkHandler { ctype: mktag!(b"trak"), parse: read_trak },
    RootChunkHandler { ctype: mktag!(b"meta"), parse: read_meta },
];

fn read_mvhd(dmx: &mut MOVDemuxer, _strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    const KNOWN_MVHD_SIZE: u64 = 100;
    let br = &mut dmx.src;
    validate!(size >= KNOWN_MVHD_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let _ctime              = br.read_u32be()?;
    let _mtime              = br.read_u32be()?;
    let tscale              = br.read_u32be()?;
    let duration            = br.read_u32be()?;
    let _pref_rate          = br.read_u32be()?;
    let _pref_volume        = br.read_u16be()?;
                              br.read_skip(10)?;
                              br.read_skip(36)?; // matrix
    let _preview_time       = br.read_u32be()?;
    let _preview_duration   = br.read_u32be()?;
    let _poster_time        = br.read_u32be()?;
    let _sel_time           = br.read_u32be()?;
    let _sel_duration       = br.read_u32be()?;
    let _cur_time           = br.read_u32be()?;
    let _next_track_id      = br.read_u32be()?;
    dmx.duration = duration;
    dmx.tb_den = tscale;

    Ok(KNOWN_MVHD_SIZE)
}

fn read_ctab(dmx: &mut MOVDemuxer, _strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    let mut pal = [0; 1024];
    let size = read_palette(&mut dmx.src, size, &mut pal)?;
    dmx.pal = Some(Arc::new(pal));
    Ok(size)
}

fn read_meta(dmx: &mut MOVDemuxer, _strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    dmx.src.skip64(size)?;
    Ok(size)
}

fn read_trak(dmx: &mut MOVDemuxer, strmgr: &mut StreamManager, size: u64) -> DemuxerResult<u64> {
    let mut track = Track::new(dmx.cur_track as u32, dmx.tb_den);
    track.read_trak(&mut dmx.src, size)?;
    validate!(track.tkhd_found && track.stsd_found);
    validate!(strmgr.get_stream_by_id(track.track_id).is_none());
    dmx.cur_track += 1;
    let mut str = None;
    std::mem::swap(&mut track.stream, &mut str);
    if let Some(stream) = str {
        let str_id = strmgr.add_stream(stream).unwrap();
        track.track_str_id = str_id;
    }
    dmx.tracks.push(track);
    Ok(size)
}

const TRAK_CHUNK_HANDLERS: &[TrackChunkHandler] = &[
    TrackChunkHandler { ctype: mktag!(b"clip"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"matt"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"edts"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"tref"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"load"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"imap"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"tkhd"), parse: read_tkhd },
    TrackChunkHandler { ctype: mktag!(b"mdia"), parse: read_mdia },
];

fn read_tkhd(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    const KNOWN_TKHD_SIZE: u64 = 84;
    validate!(size >= KNOWN_TKHD_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let _ctime              = br.read_u32be()?;
    let _mtime              = br.read_u32be()?;
    let track_id            = br.read_u32be()?;
                              br.read_skip(4)?;
    let _duration           = br.read_u32be()?;
                              br.read_skip(8)?;
    let _layer              = br.read_u16be()?;
    let _alt_group          = br.read_u16be()?;
    let _volume             = br.read_u16be()?;
                              br.read_skip(2)?;
                              br.read_skip(36)?; // matrix
    let width               = br.read_u32be()? as usize;
    let height              = br.read_u32be()? as usize;
    track.width  = width  >> 16;
    track.height = height >> 16;
    track.track_id = track_id;

    track.tkhd_found = true;
    Ok(KNOWN_TKHD_SIZE)
}

fn read_mdia(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    track.stream_type = StreamType::None;
    track.read_mdia(br, size)?;
    Ok(size)
}

const MDIA_CHUNK_HANDLERS: &[TrackChunkHandler] = &[
    TrackChunkHandler { ctype: mktag!(b"mdhd"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"hdlr"), parse: read_hdlr },
    TrackChunkHandler { ctype: mktag!(b"minf"), parse: read_minf },
];

fn read_hdlr(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    const KNOWN_HDLR_SIZE: u64 = 24;
    validate!(size >= KNOWN_HDLR_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let flags               = br.read_u24be()?;
    validate!(flags == 0);
    let comp_type           = br.read_u32be()?;
    let comp_subtype        = br.read_u32be()?;
    let _comp_manufacturer  = br.read_u32be()?;
    let _comp_flags         = br.read_u32be()?;
    let _comp_flags_mask    = br.read_u32be()?;

    if comp_type == mktag!(b"mhlr") {
        if comp_subtype == mktag!(b"vide") {
            track.stream_type = StreamType::Video;
        } else if comp_subtype == mktag!(b"soun") {
            track.stream_type = StreamType::Audio;
        } else {
            track.stream_type = StreamType::Data;
        }
    } else if comp_type == mktag!(b"dhlr") {
        track.stream_type = StreamType::Data;
    } else {
        println!("Unknown stream type");
        track.stream_type = StreamType::Data;
    }
    
    Ok(KNOWN_HDLR_SIZE)
}

fn read_minf(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    track.read_minf(br, size)?;
    Ok(size)
}

const MINF_CHUNK_HANDLERS: &[TrackChunkHandler] = &[
    TrackChunkHandler { ctype: mktag!(b"hdlr"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"dinf"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"vmhd"), parse: read_vmhd },
    TrackChunkHandler { ctype: mktag!(b"smhd"), parse: read_smhd },
    TrackChunkHandler { ctype: mktag!(b"gmhd"), parse: read_gmhd },
    TrackChunkHandler { ctype: mktag!(b"gmin"), parse: read_gmin },
    TrackChunkHandler { ctype: mktag!(b"stbl"), parse: read_stbl },
];

fn read_vmhd(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    const KNOWN_VMHD_SIZE: u64 = 12;
    validate!(track.stream_type == StreamType::Video);
    validate!(size >= KNOWN_VMHD_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
                              br.read_skip(2)?; // graphics mode
                              br.read_skip(6)?; // opcolor
    Ok(KNOWN_VMHD_SIZE)
}

fn read_smhd(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    const KNOWN_SMHD_SIZE: u64 = 8;
    validate!(track.stream_type == StreamType::Audio);
    validate!(size >= KNOWN_SMHD_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
                              br.read_skip(2)?; // balance
                              br.read_skip(2)?;
    Ok(KNOWN_SMHD_SIZE)
}

fn read_gmhd(track: &mut Track, _br: &mut ByteReader, _size: u64) -> DemuxerResult<u64> {
    validate!(track.stream_type == StreamType::Data);
    Ok(0)
}

fn read_gmin(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    validate!(track.stream_type == StreamType::Data);
    const KNOWN_GMIN_SIZE: u64 = 16;
    validate!(size >= KNOWN_GMIN_SIZE);
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
                              br.read_skip(2)?; // graphics mode
                              br.read_skip(6)?; // opcolor
                              br.read_skip(2)?; // balance
                              br.read_skip(2)?;
    Ok(KNOWN_GMIN_SIZE)
}

fn read_stbl(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    track.read_stbl(br, size)?;
    Ok(size)
}

const STBL_CHUNK_HANDLERS: &[TrackChunkHandler] = &[
    TrackChunkHandler { ctype: mktag!(b"stsd"), parse: read_stsd },
    TrackChunkHandler { ctype: mktag!(b"stts"), parse: skip_chunk },
    TrackChunkHandler { ctype: mktag!(b"stss"), parse: read_stss },
    TrackChunkHandler { ctype: mktag!(b"stsc"), parse: read_stsc },
    TrackChunkHandler { ctype: mktag!(b"stsz"), parse: read_stsz },
    TrackChunkHandler { ctype: mktag!(b"stco"), parse: read_stco },
    TrackChunkHandler { ctype: mktag!(b"stsh"), parse: skip_chunk },
];

fn read_stsd(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    const KNOWN_STSD_SIZE: u64 = 24;
    validate!(size >= KNOWN_STSD_SIZE);
    let start_pos = br.tell();
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let entries             = br.read_u32be()?;
    validate!(entries > 0);
    let esize               = br.read_u32be()? as u64;
    validate!(esize + 8 <= size);
    let mut fcc = [0u8; 4];
                              br.read_buf(&mut fcc)?;
                              br.read_skip(6)?;
    let _data_ref           = br.read_u16be()?;

    track.fcc = fcc;

    let codec_info;
    match track.stream_type {
        StreamType::Video => {
            let _ver            = br.read_u16be()?;
            let _revision       = br.read_u16le()?;
            let _vendor         = br.read_u32be()?;
            let _temp_quality   = br.read_u32be()?;
            let _spat_quality   = br.read_u32be()?;
            let width           = br.read_u16be()? as usize;
            let height          = br.read_u16be()? as usize;
            let _hor_res        = br.read_u32be()?;
            let _vert_res       = br.read_u32be()?;
            let data_size       = br.read_u32be()?;
            validate!(data_size == 0);
            let _frame_count    = br.read_u16be()? as usize;
            let _cname_len      = br.read_byte()? as usize;
                                  br.read_skip(31)?; // actual compressor name
            let depth           = br.read_u16be()?;
            let ctable_id       = br.read_u16be()?;
            validate!((depth <= 8) || (ctable_id == 0xFFFF));
            if ctable_id == 0 {
                let max_pal_size = start_pos + size - br.tell();
                let mut pal = [0; 1024];
                read_palette(br, max_pal_size, &mut pal)?;
                track.pal = Some(Arc::new(pal));
            }
// todo other atoms, put as extradata
            let cname = if let Some(name) = find_codec_from_mov_video_fourcc(&fcc) {
                    name
                } else if let Some(name) = find_codec_from_avi_fourcc(&fcc) {
                    name
                } else {
                    "unknown"
                };
            let format = if depth > 8 { RGB24_FORMAT } else { PAL8_FORMAT };
            let vhdr = NAVideoInfo::new(width, height, false, format);
            let edata;
            if br.tell() - start_pos + 4 < size {
//todo skip various common atoms
                let edata_size  = br.read_u32be()? as usize;
                let mut buf = vec![0; edata_size];
                                  br.read_buf(buf.as_mut_slice())?;
                edata = Some(buf);
            } else {
                edata = None;
            }
            codec_info = NACodecInfo::new(cname, NACodecTypeInfo::Video(vhdr), edata);
        },
        StreamType::Audio => {
            let _ver            = br.read_u16be()?;
            let _revision       = br.read_u16le()?;
            let _vendor         = br.read_u32be()?;
            let nchannels       = br.read_u16be()?;
            validate!(nchannels <= 64);
            let sample_size     = br.read_u16be()?;
            validate!(sample_size <= 128);
            let _compr_id       = br.read_u16be()?;
            let packet_size     = br.read_u16be()? as usize;
            validate!(packet_size == 0);
            let sample_rate     = br.read_u32be()?;
            validate!(sample_rate > 0);
            let cname = if let Some(name) = find_codec_from_mov_audio_fourcc(&fcc) {
                    name
                } else if let (true, Some(name)) = ((fcc[0] == b'm' && fcc[1] == b's'),  find_codec_from_wav_twocc(u16::from(fcc[2]) * 256 + u16::from(fcc[3]))) {
                    name
                } else {
                    "unknown"
                };
//todo adjust format for various PCM kinds
            let soniton = NASoniton::new(sample_size as u8, SONITON_FLAG_SIGNED | SONITON_FLAG_BE);
            let block_align = 1;
            let ahdr = NAAudioInfo::new(sample_rate >> 16, nchannels as u8, soniton, block_align);
            let edata = None;
            codec_info = NACodecInfo::new(cname, NACodecTypeInfo::Audio(ahdr), edata);
            track.channels  = nchannels as usize;
            track.bits      = sample_size as usize;
        },
        StreamType::None => {
            return Err(DemuxerError::InvalidData);
        },
        _ => {
//todo put it all into extradata
            let edata = None;
            codec_info = NACodecInfo::new("unknown", NACodecTypeInfo::None, edata);
        },
    };
    let read_size = br.tell() - start_pos;
    validate!(read_size <= size);
    track.stream = Some(NAStream::new(track.stream_type, track.track_no, codec_info, 1, track.tb_den));
    track.stsd_found = true;
    Ok(read_size)
}

fn read_stss(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let entries             = br.read_u32be()? as usize;
    validate!(entries < ((std::u32::MAX >> 2) - 8) as usize);
    validate!((entries * 4 + 8) as u64 == size);
    track.keyframes = Vec::with_capacity(entries);
    let mut last_sample_no = 0;
    for _ in 0..entries {
        let sample_no       = br.read_u32be()?;
        validate!(sample_no > last_sample_no);
        track.keyframes.push(sample_no);
        last_sample_no = sample_no;
    }
    Ok(size)
}

fn read_stsc(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let entries             = br.read_u32be()? as usize;
    validate!(entries < ((std::u32::MAX / 12) - 8) as usize);
    validate!((entries * 12 + 8) as u64 == size);
    track.sample_map = Vec::with_capacity(entries);
    let mut last_sample_no = 0;
    for _i in 0..entries {
        let sample_no       = br.read_u32be()?;
        validate!(sample_no > last_sample_no);
        let nsamples        = br.read_u32be()?;
        let _sample_desc    = br.read_u32be()?;
        track.sample_map.push((sample_no, nsamples));
        last_sample_no = sample_no;
    }
    Ok(size)
}

fn read_stsz(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let sample_size         = br.read_u32be()?;
    if sample_size != 0 {
        track.sample_size = sample_size;
        Ok(8)
    } else {
        let entries             = br.read_u32be()? as usize;
        validate!((entries * 4 + 12) as u64 == size);
        track.chunk_sizes = Vec::with_capacity(entries);
        for _ in 0..entries {
            let sample_size     = br.read_u32be()?;
            track.chunk_sizes.push(sample_size);
        }
        Ok(size)
    }
}

fn read_stco(track: &mut Track, br: &mut ByteReader, size: u64) -> DemuxerResult<u64> {
    let version             = br.read_byte()?;
    validate!(version == 0);
    let _flags              = br.read_u24be()?;
    let entries             = br.read_u32be()? as usize;
    validate!((entries * 4 + 8) as u64 == size);
    track.chunk_offsets = Vec::with_capacity(entries);
    for _i in 0..entries {
        let sample_offset   = br.read_u32be()?;
        track.chunk_offsets.push(u64::from(sample_offset));
    }
    Ok(size)
}

struct MOVDemuxer<'a> {
    src:            &'a mut ByteReader<'a>,
    depth:          usize,
    mdat_pos:       u64,
    mdat_size:      u64,
    tracks:         Vec<Track>,
    cur_track:      usize,
    tb_den:         u32,
    duration:       u32,
    pal:            Option<Arc<[u8; 1024]>>,
}

struct Track {
    track_id:       u32,
    track_str_id:   usize,
    track_no:       u32,
    tb_den:         u32,
    depth:          u8,
    tkhd_found:     bool,
    stsd_found:     bool,
    stream_type:    StreamType,
    width:          usize,
    height:         usize,
    channels:       usize,
    bits:           usize,
    fcc:            [u8; 4],
    keyframes:      Vec<u32>,
    chunk_sizes:    Vec<u32>,
    chunk_offsets:  Vec<u64>,
    sample_map:     Vec<(u32, u32)>,
    sample_size:    u32,
    stream:         Option<NAStream>,
    cur_chunk:      usize,
    cur_sample:     usize,
    samples_left:   usize,
    last_offset:    u64,
    pal:            Option<Arc<[u8; 1024]>>,
}

impl Track {
    fn new(track_no: u32, tb_den: u32) -> Self {
        Self {
            tkhd_found:     false,
            stsd_found:     false,
            track_id:       0,
            track_str_id:   0,
            track_no,
            tb_den,
            stream_type:    StreamType::None,
            width:          0,
            height:         0,
            channels:       0,
            bits:           0,
            fcc:            [0; 4],
            keyframes:      Vec::new(),
            chunk_sizes:    Vec::new(),
            chunk_offsets:  Vec::new(),
            sample_map:     Vec::new(),
            sample_size:    0,
            stream:         None,
            depth:          0,
            cur_chunk:      0,
            cur_sample:     0,
            samples_left:   0,
            last_offset:    0,
            pal:            None,
        }
    }
    read_chunk_list!(track; "trak", read_trak, TRAK_CHUNK_HANDLERS);
    read_chunk_list!(track; "mdia", read_mdia, MDIA_CHUNK_HANDLERS);
    read_chunk_list!(track; "minf", read_minf, MINF_CHUNK_HANDLERS);
    read_chunk_list!(track; "stbl", read_stbl, STBL_CHUNK_HANDLERS);
    fn fill_seek_index(&self, seek_index: &mut SeekIndex) {
        if self.keyframes.len() > 0 {
            seek_index.mode = SeekIndexMode::Present;
        }
        for kf_time in self.keyframes.iter() {
            let pts = u64::from(*kf_time - 1);
            let time = NATimeInfo::ts_to_time(pts, 1000, 1, self.tb_den);
            let idx = (*kf_time - 1) as usize;
            if idx < self.chunk_offsets.len() {
                let pos = self.chunk_offsets[idx];
                seek_index.add_entry(self.track_no as u32, SeekEntry { time, pts, pos });
            }
        }
    }
    fn calculate_chunk_size(&self, nsamp: usize) -> usize {
        if nsamp == 0 {
            self.sample_size as usize
        } else {
            match &self.fcc {
                b"NONE" | b"raw " | b"twos" | b"sowt" => {
                    (nsamp * self.bits * self.channels + 7) >> 3
                },
                b"ima4" => {
                    let nblocks = (nsamp + 63) >> 6;
                    nblocks * 34 * self.channels
                },
                b"MAC3" => {
                    (nsamp + 5) / 6 * 2 * self.channels
                },
                b"MAC6" => {
                    (nsamp + 5) / 6 * self.channels
                },
                b"in24" => nsamp * 3 * self.channels,
                b"in32" | b"fl32" => nsamp * 4 * self.channels,
                b"fl64" => nsamp * 8 * self.channels,
                b"ulaw" | b"alaw" => nsamp,
                b"ms\x00\x02" => { //MS ADPCM
                    ((nsamp - 1) / 2 + 7) * self.channels
                },
                b"ms\x00\x21" => { //IMA ADPCM
                    (nsamp / 2 + 4) * self.channels
                },
                _ => self.sample_size as usize,
            }
        }
    }
    fn get_next_chunk(&mut self) -> Option<(NATimeInfo, u64, usize)> {
        let pts = NATimeInfo::new(Some(self.cur_sample as u64), None, None, 1, self.tb_den);
//todo dts decoding
        if self.chunk_offsets.len() == self.chunk_sizes.len() { // simple one-to-one mapping
            if self.cur_sample >= self.chunk_sizes.len() {
                return None;
            }
            let offset = self.chunk_offsets[self.cur_sample];
            let size   = self.chunk_sizes[self.cur_sample] as usize;
            self.cur_sample += 1;
            Some((pts, offset, size))
        } else {
            if self.samples_left == 0 {
                if self.cur_chunk >= self.chunk_offsets.len() {
                    return None;
                }
                for (idx, samples) in self.sample_map.iter() {
                    if *idx as usize <= self.cur_chunk + 1 {
                        self.samples_left = *samples as usize;
                    } else {
                        break;
                    }
                }
                self.last_offset = self.chunk_offsets[self.cur_chunk];
                self.cur_chunk += 1;
            }
            let offset = self.last_offset;
            let size = self.get_size(self.cur_sample);
            self.last_offset += size as u64;
            if self.stream_type == StreamType::Video {
                self.samples_left -= 1;
            } else {
                self.samples_left = 0;
            }
            self.cur_sample += 1;
            Some((pts, offset, size))
        }
    }
    fn get_size(&self, sample_no: usize) -> usize {
        if self.chunk_sizes.len() > 0 {
            self.chunk_sizes[sample_no] as usize
        } else if self.sample_map.len() > 0 {
            let mut nsamp = 0;
            for (idx, samples) in self.sample_map.iter() {
                if *idx as usize <= self.cur_chunk {
                    nsamp = *samples;
                } else {
                    break;
                }
            }
            self.calculate_chunk_size(nsamp as usize)
        } else {
            self.sample_size as usize
        }
    }
    fn seek(&mut self, pts: u64) {
        self.cur_sample = pts as usize;
        self.samples_left = 0;
        if self.stream_type == StreamType::Audio {
            self.cur_chunk = self.cur_sample;
        } else if self.chunk_offsets.len() != self.chunk_sizes.len() && self.sample_map.len() > 0{
            let mut csamp = 0;
            self.cur_chunk = 0;
            let mut cmap = self.sample_map.iter();
            let mut cur_samps = 0;
            let (mut next_idx, mut next_samples) = cmap.next().unwrap();
            loop {
                if self.cur_chunk == next_idx as usize {
                    self.samples_left = cur_samps;
                    cur_samps = next_samples as usize;
                    if let Some((new_idx, new_samples)) = cmap.next() {
                        next_idx = *new_idx;
                        next_samples = *new_samples;
                    }
                }
                csamp += cur_samps;
                if csamp >= self.cur_sample {
                    self.last_offset = self.chunk_offsets[self.cur_chunk];
                    break;
                }
                self.cur_chunk += 1;
            }
            csamp -= cur_samps;
            for sample_no in csamp..self.cur_chunk {
                self.last_offset += self.get_size(sample_no) as u64;
            }
            self.samples_left = self.cur_sample - csamp - cur_samps;
        }
    }
}

impl<'a> DemuxCore<'a> for MOVDemuxer<'a> {
    fn open(&mut self, strmgr: &mut StreamManager, seek_index: &mut SeekIndex) -> DemuxerResult<()> {
        self.read_root(strmgr)?;
        validate!(self.mdat_pos > 0);
        validate!(self.tracks.len() > 0);
        for track in self.tracks.iter() {
            track.fill_seek_index(seek_index);
        }
        self.src.seek(SeekFrom::Start(self.mdat_pos))?;
        self.cur_track = 0;
        Ok(())
    }

    fn get_frame(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<NAPacket> {
        if self.tracks.len() == 0 {
            return Err(DemuxerError::EOF);
        }
        for _ in 0..self.tracks.len() {
            if self.cur_track >= self.tracks.len() {
                self.cur_track = 0;
            }
            let track = &mut self.tracks[self.cur_track];
            self.cur_track += 1;
            let first = track.cur_sample == 0;
            if let Some((pts, offset, size)) = track.get_next_chunk() {
                let str = strmgr.get_stream(track.track_str_id);
                if str.is_none() { return Err(DemuxerError::InvalidData); }
                let stream = str.unwrap();
                self.src.seek(SeekFrom::Start(offset))?;
                let mut pkt = self.src.read_packet(stream, pts, false, size)?;
                if let Some(ref pal) = track.pal {
                    let side_data = NASideData::Palette(first, pal.clone());
                    pkt.add_side_data(side_data);
                }
                return Ok(pkt);
            }
        }
        return Err(DemuxerError::EOF);
    }

    fn seek(&mut self, time: u64, seek_index: &SeekIndex) -> DemuxerResult<()> {
        let ret = seek_index.find_pos(time);
        if ret.is_none() {
            return Err(DemuxerError::SeekError);
        }
        let seek_info = ret.unwrap();
        for track in self.tracks.iter_mut() {
            track.seek(seek_info.pts);
        }
        Ok(())
    }
}

impl<'a> MOVDemuxer<'a> {
    fn new(io: &'a mut ByteReader<'a>) -> Self {
        MOVDemuxer {
            src:            io,
            depth:          0,
            mdat_pos:       0,
            mdat_size:      0,
            tracks:         Vec::with_capacity(2),
            cur_track:      0,
            tb_den:         0,
            duration:       0,
            pal:            None,
        }
    }
    fn read_root(&mut self, strmgr: &mut StreamManager) -> DemuxerResult<()> {
        self.depth = 0;
        while self.src.left() != 0 {
            let ret = read_chunk_header(&mut self.src);
            if ret.is_err() { break; }
            let (ctype, size) = ret.unwrap();
            if IGNORED_CHUNKS.contains(&ctype) {
                self.src.skip64(size)?;
                continue;
            }
            let handler = ROOT_CHUNK_HANDLERS.iter().find(|x| x.ctype == ctype);
            let read_size;
            if let Some(ref handler) = handler {
                read_size = (handler.parse)(self, strmgr, size)?;
            } else {
                println!("skipping unknown chunk {:08X} size {}", ctype, size);
                read_size = 0;
            }
            validate!(read_size <= size);
            self.src.skip64(size - read_size)?;
        }
//todo check if all needed chunks are found
        Ok(())
    }
    read_chunk_list!(root; "moov", read_moov, MOOV_CHUNK_HANDLERS);
}

pub struct MOVDemuxerCreator { }

impl DemuxerCreator for MOVDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<dyn DemuxCore<'a> + 'a> {
        Box::new(MOVDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "mov" }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_mov_demux() {
        let mut file = File::open("assets/Indeo/cubes.mov").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = MOVDemuxer::new(&mut br);
        let mut sm = StreamManager::new();
        let mut si = SeekIndex::new();
        dmx.open(&mut sm, &mut si).unwrap();

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
