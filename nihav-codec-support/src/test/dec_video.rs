//! Routines for testing decoders.
use std::fs::File;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_core::demuxers::*;
//use nihav_core::io::byteio::*;
//use nihav_core::scale::*;
use super::wavwriter::WavWriter;
use super::md5::MD5;
use crate::imgwrite::write_pnm;
pub use super::ExpectedTestResult;

const OUTPUT_PREFIX: &str = "assets/test_out/";

/*fn open_wav_out(pfx: &str, strno: usize) -> WavWriter {
    let name = format!("assets/{}out{:02}.wav", pfx, strno);
    let mut file = File::create(name).unwrap();
    let mut fw = FileWriter::new_write(&mut file);
    let mut wr = ByteWriter::new(&mut fw);
    WavWriter::new(&mut wr)
}*/

/// Tests decoding of provided file and optionally outputs video frames as PNM (PPM for RGB video, PGM for YUV).
///
/// This function expects the following arguments:
/// * `demuxer` - container format name (used to find proper demuxer for it)
/// * `name` - input file name
/// * `limit` - optional PTS value after which decoding is stopped
/// * `decode_video`/`decode_audio` - flags for enabling video/audio decoding
/// * `video_pfx` - prefix for video frames written as pictures (if enabled then output picture names should look like `<crate_name>/assets/test_out/PFXout00_000000.ppm`
/// * `dmx_reg` and `dec_reg` - registered demuxers and decoders that should contain demuxer and decoder(s) needed to decode the provided file.
///
/// Since the function is intended for tests, it will panic instead of returning an error.
pub fn test_file_decoding(demuxer: &str, name: &str, limit: Option<u64>,
                          decode_video: bool, decode_audio: bool,
                          video_pfx: Option<&str>,
                          dmx_reg: &RegisteredDemuxers, dec_reg: &RegisteredDecoders) {
    let dmx_f = dmx_reg.find_demuxer(demuxer).unwrap();
    let mut file = File::open(name).unwrap();
    let mut fr = FileReader::new_read(&mut file);
    let mut br = ByteReader::new(&mut fr);
    let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

    let mut decs: Vec<Option<(Box<NADecoderSupport>, Box<dyn NADecoder>)>> = Vec::new();
    for i in 0..dmx.get_num_streams() {
        let s = dmx.get_stream(i).unwrap();
        let info = s.get_info();
        let decfunc = dec_reg.find_decoder(info.get_name());
        if let Some(df) = decfunc {
            if (decode_video && info.is_video()) || (decode_audio && info.is_audio()) {
                let mut dec = (df)();
                let mut dsupp = Box::new(NADecoderSupport::new());
                dec.init(&mut dsupp, info).unwrap();
                decs.push(Some((dsupp, dec)));
            } else {
                decs.push(None);
            }
        } else {
            decs.push(None);
        }
    }

    loop {
        let pktres = dmx.get_frame();
        if let Err(e) = pktres {
            if e == DemuxerError::EOF { break; }
            panic!("error");
        }
        let pkt = pktres.unwrap();
        let streamno = pkt.get_stream().get_id() as usize;
        if let Some((ref mut dsupp, ref mut dec)) = decs[streamno] {
            if let (Some(lim), Some(ppts)) = (limit, pkt.get_pts()) {
                if ppts > lim { break; }
            }
            let frm = dec.decode(dsupp, &pkt).unwrap();
            if pkt.get_stream().get_info().is_video() && video_pfx.is_some() && frm.get_frame_type() != FrameType::Skip {
                let pts = if let Some(fpts) = frm.get_pts() { fpts } else { pkt.get_pts().unwrap() };
                let pfx = OUTPUT_PREFIX.to_owned() + video_pfx.unwrap_or("") + "out";
                write_pnm(pfx.as_str(), streamno, pts, frm).unwrap();
            }
        }
    }
}

/// Tests audio decoder with the content in the provided file and optionally outputs decoded audio.
///
/// The syntax is very similar to [`test_file_decoding`] except that it is intended for testing audio codecs.
///
/// Since the function is intended for tests, it will panic instead of returning an error.
///
/// [`test_file_decoding`]: ./fn.test_file_decoding.html
pub fn test_decode_audio(demuxer: &str, name: &str, limit: Option<u64>, audio_pfx: Option<&str>,
                         dmx_reg: &RegisteredDemuxers, dec_reg: &RegisteredDecoders) {
    let dmx_f = dmx_reg.find_demuxer(demuxer).unwrap();
    let mut file = File::open(name).unwrap();
    let mut fr = FileReader::new_read(&mut file);
    let mut br = ByteReader::new(&mut fr);
    let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

    let mut decs: Vec<Option<(Box<NADecoderSupport>, Box<dyn NADecoder>)>> = Vec::new();
    for i in 0..dmx.get_num_streams() {
        let s = dmx.get_stream(i).unwrap();
        let info = s.get_info();
        let decfunc = dec_reg.find_decoder(info.get_name());
        if let Some(df) = decfunc {
            if info.is_audio() {
                let mut dec = (df)();
                let mut dsupp = Box::new(NADecoderSupport::new());
                dec.init(&mut dsupp, info).unwrap();
                decs.push(Some((dsupp, dec)));
            } else {
                decs.push(None);
            }
        } else {
            decs.push(None);
        }
    }

    if let Some(audio_pfx) = audio_pfx {
        let name = format!("{}/{}out.wav", OUTPUT_PREFIX, audio_pfx);
        let file = File::create(name).unwrap();
        let mut fw = FileWriter::new_write(file);
        let mut wr = ByteWriter::new(&mut fw);
        let mut wwr = WavWriter::new(&mut wr);
        let mut wrote_header = false;

        loop {
            let pktres = dmx.get_frame();
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
                panic!("error");
            }
            let pkt = pktres.unwrap();
            if limit.is_some() && pkt.get_pts().is_some() && pkt.get_pts().unwrap() > limit.unwrap() {
                break;
            }
            let streamno = pkt.get_stream().get_id() as usize;
            if let Some((ref mut dsupp, ref mut dec)) = decs[streamno] {
                let frm = dec.decode(dsupp, &pkt).unwrap();
                if frm.get_info().is_audio() {
                    if !wrote_header {
                        wwr.write_header(frm.get_info().as_ref().get_properties().get_audio_info().unwrap()).unwrap();
                        wrote_header = true;
                    }
                    wwr.write_frame(frm.get_buffer()).unwrap();
                }
            }
        }
    } else {
        loop {
            let pktres = dmx.get_frame();
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
                panic!("error");
            }
            let pkt = pktres.unwrap();
            if limit.is_some() && pkt.get_pts().is_some() && pkt.get_pts().unwrap() > limit.unwrap() {
                break;
            }
            let streamno = pkt.get_stream().get_id() as usize;
            if let Some((ref mut dsupp, ref mut dec)) = decs[streamno] {
                let _ = dec.decode(dsupp, &pkt).unwrap();
            }
        }
    }
}

fn frame_checksum(md5: &mut MD5, frm: NAFrameRef) {
    match frm.get_buffer() {
        NABufferType::Video(ref vb) => {
            md5.update_hash(vb.get_data());
        },
        NABufferType::Video16(ref vb) => {
            let mut samp = [0u8; 2];
            let data = vb.get_data();
            for el in data.iter() {
                samp[0] = (*el >> 8) as u8;
                samp[1] = (*el >> 0) as u8;
                md5.update_hash(&samp);
            }
        },
        NABufferType::Video32(ref vb) => {
            let mut samp = [0u8; 4];
            let data = vb.get_data();
            for el in data.iter() {
                samp[0] = (*el >> 24) as u8;
                samp[1] = (*el >> 16) as u8;
                samp[2] = (*el >>  8) as u8;
                samp[3] = (*el >>  0) as u8;
                md5.update_hash(&samp);
            }
        },
        NABufferType::VideoPacked(ref vb) => {
            md5.update_hash(vb.get_data());
        },
        NABufferType::AudioU8(ref ab) => {
            md5.update_hash(ab.get_data());
        },
        NABufferType::AudioI16(ref ab) => {
            let mut samp = [0u8; 2];
            let data = ab.get_data();
            for el in data.iter() {
                samp[0] = (*el >> 8) as u8;
                samp[1] = (*el >> 0) as u8;
                md5.update_hash(&samp);
            }
        },
        NABufferType::AudioI32(ref ab) => {
            let mut samp = [0u8; 4];
            let data = ab.get_data();
            for el in data.iter() {
                samp[0] = (*el >> 24) as u8;
                samp[1] = (*el >> 16) as u8;
                samp[2] = (*el >>  8) as u8;
                samp[3] = (*el >>  0) as u8;
                md5.update_hash(&samp);
            }
        },
        NABufferType::AudioF32(ref ab) => {
            let mut samp = [0u8; 4];
            let data = ab.get_data();
            for el in data.iter() {
                let bits = el.to_bits();
                samp[0] = (bits >> 24) as u8;
                samp[1] = (bits >> 16) as u8;
                samp[2] = (bits >>  8) as u8;
                samp[3] = (bits >>  0) as u8;
                md5.update_hash(&samp);
            }
        },
        NABufferType::AudioPacked(ref ab) => {
            md5.update_hash(ab.get_data());
        },
        NABufferType::Data(ref db) => {
            md5.update_hash(db.as_ref());
        },
        NABufferType::None => {},
    };
}

/// Tests decoder for requested codec in provided file.
///
/// This functions tries to decode a stream corresponding to `dec_name` codec in input file and validate the results against expected ones.
///
/// Since the function is intended for tests, it will panic instead of returning an error.
///
/// # Examples
///
/// Test RealVideo 4 decoder in test stream:
/// ```no_run
/// use nihav_codec_support::test::ExpectedTestResult;
/// use nihav_codec_support::test::dec_video::test_decoding;
/// use nihav_core::codecs::RegisteredDecoders;
/// use nihav_core::demuxers::RegisteredDemuxers;
///
/// let mut dmx_reg = RegisteredDemuxers::new();
/// let mut dec_reg = RegisteredDecoders::new();
/// // ... register RealMedia demuxers and RealVideo decoders ...
/// test_decoding("realmedia", "rv40", "assets/test_file.rmvb", None, &dmx_reg, &dec_reg, ExpectedTestResult::MD5([0x00010203, 0x04050607, 0x08090a0b, 0x0c0d0e0f]));
/// ```
pub fn test_decoding(demuxer: &str, dec_name: &str, filename: &str, limit: Option<u64>,
                     dmx_reg: &RegisteredDemuxers, dec_reg: &RegisteredDecoders,
                     test: ExpectedTestResult) {
    let dmx_f = dmx_reg.find_demuxer(demuxer).unwrap();
    let mut file = File::open(filename).unwrap();
    let mut fr = FileReader::new_read(&mut file);
    let mut br = ByteReader::new(&mut fr);
    let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

    let mut decs: Vec<Option<(Box<NADecoderSupport>, Box<dyn NADecoder>)>> = Vec::new();
    let mut found = false;
    for i in 0..dmx.get_num_streams() {
        let s = dmx.get_stream(i).unwrap();
        let info = s.get_info();
println!("stream {} codec {} / {}", i, info.get_name(), dec_name);
        if !found && (info.get_name() == dec_name) {
            let decfunc = dec_reg.find_decoder(info.get_name());
            if let Some(df) = decfunc {
                let mut dec = (df)();
                let mut dsupp = Box::new(NADecoderSupport::new());
                dec.init(&mut dsupp, info).unwrap();
                decs.push(Some((dsupp, dec)));
                found = true;
            } else {
                decs.push(None);
            }
        } else {
            decs.push(None);
        }
    }

    let mut md5 = MD5::new();
    let mut frameiter = if let ExpectedTestResult::MD5Frames(ref vec) = test {
            Some(vec.iter())
        } else {
            None
        };
    loop {
        let pktres = dmx.get_frame();
        if let Err(e) = pktres {
            if e == DemuxerError::EOF { break; }
            panic!("error");
        }
        let pkt = pktres.unwrap();
        let streamno = pkt.get_stream().get_id() as usize;
        if let Some((ref mut dsupp, ref mut dec)) = decs[streamno] {
            if limit.is_some() && pkt.get_pts().is_some() && pkt.get_pts().unwrap() > limit.unwrap() {
                break;
            }
            let frm = dec.decode(dsupp, &pkt).unwrap();
            match &test {
                ExpectedTestResult::Decodes => {},
                ExpectedTestResult::MD5(_) => { frame_checksum(&mut md5, frm); },
                ExpectedTestResult::MD5Frames(_) => {
                    md5 = MD5::new();
                    frame_checksum(&mut md5, frm);
                    md5.finish();
                    if let Some(ref mut iter) = frameiter {
                        let ret = iter.next();
                        if ret.is_none() { break; }
                        let ref_hash = ret.unwrap();
                        let mut hash = [0u32; 4];
                        md5.get_hash(&mut hash);
println!("frame pts {:?} hash {}", pkt.get_pts(), md5);
                        assert_eq!(&hash, ref_hash);
                    }
                },
                ExpectedTestResult::GenerateMD5Frames => {
                    md5 = MD5::new();
                    frame_checksum(&mut md5, frm);
                    md5.finish();
                    let mut hash = [0u32; 4];
                    md5.get_hash(&mut hash);
println!("frame pts {:?} hash [0x{:08x}, 0x{:08x}, 0x{:08x}, 0x{:08x}],", pkt.get_pts(), hash[0], hash[1], hash[2], hash[3]);
                },
            };
        }
    }
    if let ExpectedTestResult::MD5(ref ref_hash) = test {
        md5.finish();
        let mut hash = [0u32; 4];
        md5.get_hash(&mut hash);
println!("full hash {}", md5);
        assert_eq!(&hash, ref_hash);
    }
    if let ExpectedTestResult::GenerateMD5Frames = test {
        panic!("generated hashes");
    }
}
