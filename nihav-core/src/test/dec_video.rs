use std::fs::File;
use std::io::prelude::*;
use crate::frame::*;
use crate::codecs::*;
use crate::demuxers::*;
//use crate::io::byteio::*;
use crate::scale::*;
use super::wavwriter::WavWriter;
use super::md5::MD5;
pub use super::ExpectedTestResult;

const OUTPUT_PREFIX: &str = "assets/test_out";

fn write_pgmyuv(pfx: &str, strno: usize, num: u64, frm: NAFrameRef) {
    if let NABufferType::None = frm.get_buffer() { return; }
    let name = format!("{}/{}out{:02}_{:06}.pgm", OUTPUT_PREFIX, pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    let buf = frm.get_buffer().get_vbuf().unwrap();
    let (w, h) = buf.get_dimensions(0);
    let (w2, h2) = buf.get_dimensions(1);
    let has_alpha = buf.get_info().get_format().has_alpha();
    let mut tot_h = h + h2;
    if has_alpha {
        tot_h += h;
    }
    if w2 > w/2 {
        tot_h += h2;
    }
    let hdr = format!("P5\n{} {}\n255\n", w, tot_h);
    ofile.write_all(hdr.as_bytes()).unwrap();
    let dta = buf.get_data();
    let ls = buf.get_stride(0);
    let mut idx = 0;
    let mut idx2 = w;
    for _ in 0..h {
        let line = &dta[idx..idx2];
        ofile.write_all(line).unwrap();
        idx  += ls;
        idx2 += ls;
    }
    if w2 <= w/2 {
        let pad: Vec<u8> = vec![0xFF; (w - w2 * 2) / 2];
        let mut base1 = buf.get_offset(1);
        let stride1 = buf.get_stride(1);
        let mut base2 = buf.get_offset(2);
        let stride2 = buf.get_stride(2);
        for _ in 0..h2 {
            let bend1 = base1 + w2;
            let line = &dta[base1..bend1];
            ofile.write_all(line).unwrap();
            ofile.write_all(pad.as_slice()).unwrap();

            let bend2 = base2 + w2;
            let line = &dta[base2..bend2];
            ofile.write_all(line).unwrap();
            ofile.write_all(pad.as_slice()).unwrap();

            base1 += stride1;
            base2 += stride2;
        }
    } else {
        let pad: Vec<u8> = vec![0xFF; w - w2];
        let mut base1 = buf.get_offset(1);
        let stride1 = buf.get_stride(1);
        for _ in 0..h2 {
            let bend1 = base1 + w2;
            let line = &dta[base1..bend1];
            ofile.write_all(line).unwrap();
            ofile.write_all(pad.as_slice()).unwrap();
            base1 += stride1;
        }
        let mut base2 = buf.get_offset(2);
        let stride2 = buf.get_stride(2);
        for _ in 0..h2 {
            let bend2 = base2 + w2;
            let line = &dta[base2..bend2];
            ofile.write_all(line).unwrap();
            ofile.write_all(pad.as_slice()).unwrap();
            base2 += stride2;
        }
    }
    if has_alpha {
        let ls = buf.get_stride(3);
        let mut idx = buf.get_offset(3);
        let mut idx2 = idx + w;
        for _ in 0..h {
            let line = &dta[idx..idx2];
            ofile.write_all(line).unwrap();
            idx  += ls;
            idx2 += ls;
        }
    }
}

fn write_palppm(pfx: &str, strno: usize, num: u64, frm: NAFrameRef) {
    let name = format!("{}/{}out{:02}_{:06}.ppm", OUTPUT_PREFIX, pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    let buf = frm.get_buffer().get_vbuf().unwrap();
    let (w, h) = buf.get_dimensions(0);
    let paloff = buf.get_offset(1);
    let hdr = format!("P6\n{} {}\n255\n", w, h);
    ofile.write_all(hdr.as_bytes()).unwrap();
    let dta = buf.get_data();
    let ls = buf.get_stride(0);
    let offs: [usize; 3] = [
            buf.get_info().get_format().get_chromaton(0).unwrap().get_offset() as usize,
            buf.get_info().get_format().get_chromaton(1).unwrap().get_offset() as usize,
            buf.get_info().get_format().get_chromaton(2).unwrap().get_offset() as usize
        ];
    let mut idx  = 0;
    let mut line: Vec<u8> = vec![0; w * 3];
    for _ in 0..h {
        let src = &dta[idx..(idx+w)];
        for x in 0..w {
            let pix = src[x] as usize;
            line[x * 3 + 0] = dta[paloff + pix * 3 + offs[0]];
            line[x * 3 + 1] = dta[paloff + pix * 3 + offs[1]];
            line[x * 3 + 2] = dta[paloff + pix * 3 + offs[2]];
        }
        ofile.write_all(line.as_slice()).unwrap();
        idx  += ls;
    }
}

fn write_ppm(pfx: &str, strno: usize, num: u64, frm: NAFrameRef) {
    let name = format!("{}/{}out{:02}_{:06}.ppm", OUTPUT_PREFIX, pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
        let info = frm.get_buffer().get_video_info().unwrap();
        let mut dpic = alloc_video_buffer(NAVideoInfo::new(info.get_width(), info.get_height(), false, RGB24_FORMAT), 0).unwrap();
        let ifmt = ScaleInfo { width: info.get_width(), height: info.get_height(), fmt: info.get_format() };
        let ofmt = ScaleInfo { width: info.get_width(), height: info.get_height(), fmt: RGB24_FORMAT };
        let mut scaler = NAScale::new(ifmt, ofmt).unwrap();
        scaler.convert(&frm.get_buffer(), &mut dpic).unwrap();
        let buf = dpic.get_vbuf().unwrap();
        let (w, h) = buf.get_dimensions(0);
        let hdr = format!("P6\n{} {}\n255\n", w, h);
        ofile.write_all(hdr.as_bytes()).unwrap();
        let dta = buf.get_data();
        let stride = buf.get_stride(0);
        for src in dta.chunks(stride) {
            ofile.write_all(&src[0..w*3]).unwrap();
        }
}

/*fn open_wav_out(pfx: &str, strno: usize) -> WavWriter {
    let name = format!("assets/{}out{:02}.wav", pfx, strno);
    let mut file = File::create(name).unwrap();
    let mut fw = FileWriter::new_write(&mut file);
    let mut wr = ByteWriter::new(&mut fw);
    WavWriter::new(&mut wr)
}*/

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
                let pfx = video_pfx.unwrap();
                let pts = if let Some(fpts) = frm.get_pts() { fpts } else { pkt.get_pts().unwrap() };
                let vinfo = frm.get_buffer().get_video_info().unwrap();
                if vinfo.get_format().is_paletted() {
                    write_palppm(pfx, streamno, pts, frm);
                } else if vinfo.get_format().get_model().is_yuv() {
                    write_pgmyuv(pfx, streamno, pts, frm);
                } else if vinfo.get_format().get_model().is_rgb() {
                    write_ppm(pfx, streamno, pts, frm);
                } else {
panic!(" unknown format");
                }
            }
        }
    }
}

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
println!("frame pts {:?} hash {}", pkt.get_pts(), md5);
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
