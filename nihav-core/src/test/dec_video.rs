use std::fs::File;
use std::io::prelude::*;
use crate::frame::*;
use crate::codecs::*;
use crate::demuxers::*;
//use crate::io::byteio::*;
use super::wavwriter::WavWriter;

fn write_pgmyuv(pfx: &str, strno: usize, num: u64, frmref: NAFrameRef) {
    let frm = frmref.borrow();
    if let NABufferType::None = frm.get_buffer() { return; }
    let name = format!("assets/{}out{:02}_{:06}.pgm", pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    let buf = frm.get_buffer().get_vbuf().unwrap();
    let (w, h) = buf.get_dimensions(0);
    let (w2, h2) = buf.get_dimensions(1);
    let has_alpha = buf.get_info().get_format().has_alpha();
    let tot_h;
    if has_alpha {
        tot_h = h * 2 + h2;
    } else {
        tot_h = h + h2;
    }
    let hdr = format!("P5\n{} {}\n255\n", w, tot_h);
    ofile.write_all(hdr.as_bytes()).unwrap();
    let dta = buf.get_data();
    let ls = buf.get_stride(0);
    let mut idx = 0;
    let mut idx2 = w;
    let mut pad: Vec<u8> = Vec::with_capacity((w - w2 * 2) / 2);
    pad.resize((w - w2 * 2) / 2, 0xFF);
    for _ in 0..h {
        let line = &dta[idx..idx2];
        ofile.write_all(line).unwrap();
        idx  += ls;
        idx2 += ls;
    }
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

fn write_palppm(pfx: &str, strno: usize, num: u64, frmref: NAFrameRef) {
    let frm = frmref.borrow();
    let name = format!("assets/{}out{:02}_{:06}.ppm", pfx, strno, num);
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
    let mut line: Vec<u8> = Vec::with_capacity(w * 3);
    line.resize(w * 3, 0);
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

fn write_ppm(pfx: &str, strno: usize, num: u64, frmref: NAFrameRef) {
    let frm = frmref.borrow();
    let name = format!("assets/{}out{:02}_{:06}.ppm", pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    if let NABufferType::VideoPacked(ref buf) = frm.get_buffer() {
        let (w, h) = buf.get_dimensions(0);
        let hdr = format!("P6\n{} {}\n255\n", w, h);
        ofile.write_all(hdr.as_bytes()).unwrap();
        let dta = buf.get_data();
        let stride = buf.get_stride(0);
        let offs: [usize; 3] = [
                buf.get_info().get_format().get_chromaton(0).unwrap().get_offset() as usize,
                buf.get_info().get_format().get_chromaton(1).unwrap().get_offset() as usize,
                buf.get_info().get_format().get_chromaton(2).unwrap().get_offset() as usize
            ];
        let step = buf.get_info().get_format().get_elem_size() as usize;
        let mut line: Vec<u8> = Vec::with_capacity(w * 3);
        line.resize(w * 3, 0);
        for src in dta.chunks(stride) {
            for x in 0..w {
                line[x * 3 + 0] = src[x * step + offs[0]];
                line[x * 3 + 1] = src[x * step + offs[1]];
                line[x * 3 + 2] = src[x * step + offs[2]];
            }
            ofile.write_all(line.as_slice()).unwrap();
        }
    } else if let NABufferType::Video16(ref buf) = frm.get_buffer() {
        let (w, h) = buf.get_dimensions(0);
        let hdr = format!("P6\n{} {}\n255\n", w, h);
        ofile.write_all(hdr.as_bytes()).unwrap();
        let dta = buf.get_data();
        let stride = buf.get_stride(0);
        let depths: [u8; 3] = [
                buf.get_info().get_format().get_chromaton(0).unwrap().get_depth(),
                buf.get_info().get_format().get_chromaton(1).unwrap().get_depth(),
                buf.get_info().get_format().get_chromaton(2).unwrap().get_depth()
            ];
        let masks: [u16; 3] = [
                (1 << depths[0]) - 1,
                (1 << depths[1]) - 1,
                (1 << depths[2]) - 1
            ];
        let shifts: [u8; 3] = [
                buf.get_info().get_format().get_chromaton(0).unwrap().get_shift(),
                buf.get_info().get_format().get_chromaton(1).unwrap().get_shift(),
                buf.get_info().get_format().get_chromaton(2).unwrap().get_shift()
            ];
        let mut line: Vec<u8> = Vec::with_capacity(w * 3);
        line.resize(w * 3, 0);
        for src in dta.chunks(stride) {
            for x in 0..w {
                let elem = src[x];
                let r = ((elem >> shifts[0]) & masks[0]) << (8 - depths[0]);
                let g = ((elem >> shifts[1]) & masks[1]) << (8 - depths[1]);
                let b = ((elem >> shifts[2]) & masks[2]) << (8 - depths[2]);
                line[x * 3 + 0] = r as u8;
                line[x * 3 + 1] = g as u8;
                line[x * 3 + 2] = b as u8;
            }
            ofile.write_all(line.as_slice()).unwrap();
        }
    } else {
panic!(" unhandled buf format");
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

    let mut decs: Vec<Option<Box<NADecoder>>> = Vec::new();
    for i in 0..dmx.get_num_streams() {
        let s = dmx.get_stream(i).unwrap();
        let info = s.get_info();
        let decfunc = dec_reg.find_decoder(info.get_name());
        if let Some(df) = decfunc {
            if (decode_video && info.is_video()) || (decode_audio && info.is_audio()) {
                let mut dec = (df)();
                dec.init(info).unwrap();
                decs.push(Some(dec));
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
        if limit.is_some() && pkt.get_pts().is_some() {
            if pkt.get_pts().unwrap() > limit.unwrap() { break; }
        }
        let streamno = pkt.get_stream().get_id() as usize;
        if let Some(ref mut dec) = decs[streamno] {
            let frm = dec.decode(&pkt).unwrap();
            if pkt.get_stream().get_info().is_video() && video_pfx.is_some() && frm.borrow().get_frame_type() != FrameType::Skip {
                let pfx = video_pfx.unwrap();
		let pts = if let Some(fpts) = frm.borrow().get_pts() { fpts } else { pkt.get_pts().unwrap() };
                let vinfo = frm.borrow().get_buffer().get_video_info().unwrap();
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

pub fn test_decode_audio(demuxer: &str, name: &str, limit: Option<u64>, audio_pfx: &str,
                         dmx_reg: &RegisteredDemuxers, dec_reg: &RegisteredDecoders) {
    let dmx_f = dmx_reg.find_demuxer(demuxer).unwrap();
    let mut file = File::open(name).unwrap();
    let mut fr = FileReader::new_read(&mut file);
    let mut br = ByteReader::new(&mut fr);
    let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

    let mut decs: Vec<Option<Box<NADecoder>>> = Vec::new();
    for i in 0..dmx.get_num_streams() {
        let s = dmx.get_stream(i).unwrap();
        let info = s.get_info();
        let decfunc = dec_reg.find_decoder(info.get_name());
        if let Some(df) = decfunc {
            if info.is_audio() {
                let mut dec = (df)();
                dec.init(info).unwrap();
                decs.push(Some(dec));
            } else {
                decs.push(None);
            }
        } else {
            decs.push(None);
        }
    }

    let name = format!("assets/{}out.wav", audio_pfx);
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
        if limit.is_some() && pkt.get_pts().is_some() {
            if pkt.get_pts().unwrap() > limit.unwrap() { break; }
        }
        let streamno = pkt.get_stream().get_id() as usize;
        if let Some(ref mut dec) = decs[streamno] {
            let frm_ = dec.decode(&pkt).unwrap();
            let frm = frm_.borrow();
            if frm.get_info().is_audio() {
                if !wrote_header {
                    wwr.write_header(frm.get_info().as_ref().get_properties().get_audio_info().unwrap()).unwrap();
                    wrote_header = true;
                }
                wwr.write_frame(frm.get_buffer()).unwrap();
            }
        }
    }
}
