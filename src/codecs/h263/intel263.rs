use io::bitreader::*;
use io::codebook::*;
use formats;
use super::super::*;
use super::*;
use super::decoder::*;
use super::data::*;

#[allow(dead_code)]
struct Tables {
    intra_mcbpc_cb: Codebook<u8>,
    inter_mcbpc_cb: Codebook<u8>,
    cbpy_cb:        Codebook<u8>,
    rl_cb:          Codebook<H263RLSym>,
    aic_rl_cb:      Codebook<H263RLSym>,
    mv_cb:          Codebook<u8>,
}

struct Intel263Decoder {
    info:    Rc<NACodecInfo>,
    dec:     H263BaseDecoder,
    tables:  Tables,
}

struct Intel263BR<'a> {
    br:     BitReader<'a>,
    tables: &'a Tables,
    gob_no: usize,
    mb_w:   usize,
    is_pb:  bool,
}

fn check_marker<'a>(br: &mut BitReader<'a>) -> DecoderResult<()> {
    let mark = br.read(1)?;
    validate!(mark == 1);
    Ok(())
}

impl<'a> Intel263BR<'a> {
    fn new(src: &'a [u8], tables: &'a Tables) -> Self {
        Intel263BR {
            br:     BitReader::new(src, src.len(), BitReaderMode::BE),
            tables: tables,
            gob_no: 0,
            mb_w:   0,
            is_pb:  false,
        }
    }

    fn decode_block(&mut self, quant: u8, intra: bool, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        let mut br = &mut self.br;
        let mut idx = 0;
        if intra {
            let mut dc = br.read(8)?;
            if dc == 255 { dc = 128; }
            blk[0] = (dc as i16) << 3;
            idx = 1;
        }
        if !coded { return Ok(()); }

        let rl_cb = &self.tables.rl_cb; // could be aic too
        let q_add = if quant == 0 { 0i16 } else { ((quant - 1) | 1) as i16 };
        let q = (quant * 2) as i16;
        while idx < 64 {
            let code = br.read_cb(rl_cb)?;
            let run;
            let mut level;
            let last;
            if !code.is_escape() {
                run   = code.get_run();
                level = code.get_level();
                last  = code.is_last();
                if br.read_bool()? { level = -level; }
                level = (level * q) + q_add;
            } else {
                last  = br.read_bool()?;
                run   = br.read(6)? as u8;
                level = br.read_s(8)? as i16;
                if level == -128 {
                    let low = br.read(5)? as i16;
                    let top = br.read_s(6)? as i16;
                    level = (top << 5) | low;
                }
                level = (level * q) + q_add;
                if level < -2048 { level = -2048; }
                if level >  2047 { level =  2047; }
            }
            idx += run;
            validate!(idx < 64);
            let oidx = H263_ZIGZAG[idx as usize];
            blk[oidx] = level;
            idx += 1;
            if last { break; }
        }
        Ok(())
    }
}

fn decode_mv_component(br: &mut BitReader, mv_cb: &Codebook<u8>) -> DecoderResult<i16> {
    let code = br.read_cb(mv_cb)? as i16;
    if code == 0 { return Ok(0) }
    if !br.read_bool()? {
        Ok(code)
    } else {
        Ok(-code)
    }
}

fn decode_mv(br: &mut BitReader, mv_cb: &Codebook<u8>) -> DecoderResult<MV> {
    let xval = decode_mv_component(br, mv_cb)?;
    let yval = decode_mv_component(br, mv_cb)?;
    Ok(MV::new(xval, yval))
}

fn decode_b_info(br: &mut BitReader, is_pb: bool, is_intra: bool) -> DecoderResult<BBlockInfo> {
    if is_pb { // as improved pb
        let pb_mv_add = if is_intra { 1 } else { 0 };
        if br.read_bool()?{
            if br.read_bool()? {
                let pb_mv_count = 1 - (br.read(1)? as usize);
                let cbpb = br.read(6)? as u8;
                Ok(BBlockInfo::new(true, cbpb, pb_mv_count + pb_mv_add, pb_mv_count == 1))
            } else {
                Ok(BBlockInfo::new(true, 0, 1 + pb_mv_add, true))
            }
        } else {
            Ok(BBlockInfo::new(true, 0, pb_mv_add, false))
        }
    } else {
        Ok(BBlockInfo::new(false, 0, 0, false))
    }
}

impl<'a> BlockDecoder for Intel263BR<'a> {

#[allow(unused_variables)]
    fn decode_pichdr(&mut self) -> DecoderResult<PicInfo> {
        let mut br = &mut self.br;
        let syncw = br.read(22)?;
        validate!(syncw == 0x000020);
        let tr = br.read(8)? as u8;
        check_marker(br)?;
        let id = br.read(1)?;
        validate!(id == 0);
        br.read(1)?; // split screen indicator
        br.read(1)?; // document camera indicator
        br.read(1)?; // freeze picture release
        let mut sfmt = br.read(3)?;
        validate!((sfmt != 0b000) && (sfmt != 0b110));
        let is_intra = !br.read_bool()?;
        let umv = br.read_bool()?;
        br.read(1)?; // syntax arithmetic coding
        let apm = br.read_bool()?;
        self.is_pb = br.read_bool()?;
        let deblock;
        if sfmt == 0b111 {
            sfmt = br.read(3)?;
            validate!((sfmt != 0b000) && (sfmt != 0b111));
            br.read(2)?; // unknown flags
            deblock = br.read_bool()?;
            br.read(1)?; // unknown flag
            let pbplus = br.read_bool()?;
            br.read(5)?; // unknown flags
            let marker = br.read(5)?;
            validate!(marker == 1);
        } else {
            deblock = false;
        }
        let w; let h;
        if sfmt == 0b110 {
            let par = br.read(4)?;
            w = ((br.read(9)? + 1) * 4) as usize;
            check_marker(br)?;
            h = ((br.read(9)? + 1) * 4) as usize;
            if par == 0b1111 {
                let pixw = br.read(8)?;
                let pixh = br.read(8)?;
                validate!((pixw != 0) && (pixh != 0));
            }
        } else {
            let (w_, h_) = H263_SIZES[sfmt as usize];
            w = w_;
            h = h_;
        }
        let quant = br.read(5)?;
        let cpm = br.read_bool()?;
        validate!(!cpm);

        let pbinfo;
        if self.is_pb {
            let trb = br.read(3)?;
            let dbquant = br.read(2)?;
            pbinfo = Some(PBInfo::new(trb as u8, dbquant as u8));
        } else {
            pbinfo = None;
        }
        while br.read_bool()? { // skip PEI
            br.read(8)?;
        }
//println!("frame {}x{} intra: {} q {} pb {} apm {} umv {} @{}", w, h, is_intra, quant, self.is_pb, apm, umv, br.tell());
        self.gob_no = 0;
        self.mb_w = (w + 15) >> 4;

        let ftype = if is_intra { Type::I } else { Type::P };
        let picinfo = PicInfo::new(w, h, ftype, quant as u8, apm, umv, tr, pbinfo, deblock);
        Ok(picinfo)
    }

    #[allow(unused_variables)]
    fn decode_slice_header(&mut self, info: &PicInfo) -> DecoderResult<Slice> {
        let mut br = &mut self.br;
        let gbsc = br.read(17)?;
        validate!(gbsc == 1);
        let gn = br.read(5)?;
        let gfid = br.read(2)?;
        let gquant = br.read(5)?;
//println!("GOB gn {:X} id {} q {}", gn, gfid, gquant);
        let ret = Slice::new(0, self.gob_no, gquant as u8);
        self.gob_no += 1;
        Ok(ret)
    }

    fn decode_block_header(&mut self, info: &PicInfo, slice: &Slice) -> DecoderResult<BlockInfo> {
        let mut br = &mut self.br;
        let mut q = slice.get_quant();
        match info.get_mode() {
            Type::I => {
                    let mut cbpc = br.read_cb(&self.tables.intra_mcbpc_cb)?;
                    while cbpc == 8 { cbpc = br.read_cb(&self.tables.intra_mcbpc_cb)?; }
                    let cbpy = br.read_cb(&self.tables.cbpy_cb)?;
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    let dquant = (cbpc & 4) != 0;
                    if dquant {
                        let idx = br.read(2)? as usize;
                        q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                    }
                    Ok(BlockInfo::new(Type::I, cbp, q))
                },
            Type::P => {
                    if br.read_bool()? { return Ok(BlockInfo::new(Type::Skip, 0, info.get_quant())); }
                    let mut cbpc = br.read_cb(&self.tables.inter_mcbpc_cb)?;
                    while cbpc == 20 { cbpc = br.read_cb(&self.tables.inter_mcbpc_cb)?; }
                    let is_intra = (cbpc & 0x04) != 0;
                    let dquant   = (cbpc & 0x08) != 0;
                    let is_4x4   = (cbpc & 0x10) != 0;
                    if is_intra {
                        let mut mvec: Vec<MV> = Vec::new();
                        let bbinfo = decode_b_info(br, self.is_pb, true)?;
                        let cbpy = br.read_cb(&self.tables.cbpy_cb)?;
                        let cbp = (cbpy << 2) | (cbpc & 3);
                        if dquant {
                            let idx = br.read(2)? as usize;
                            q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                        }
                        let mut binfo = BlockInfo::new(Type::I, cbp, q);
                        binfo.set_bpart(bbinfo);
                        if self.is_pb {
                            for _ in 0..bbinfo.get_num_mv() {
                                mvec.push(decode_mv(br, &self.tables.mv_cb)?);
                            }
                            binfo.set_b_mv(mvec.as_slice());
                        }
                        return Ok(binfo);
                    }

                    let bbinfo = decode_b_info(br, self.is_pb, false)?;
                    let mut cbpy = br.read_cb(&self.tables.cbpy_cb)?;
//                    if /* !aiv && */(cbpc & 3) != 3 {
                        cbpy ^= 0xF;
//                    }
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    if dquant {
                        let idx = br.read(2)? as usize;
                        q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                    }
                    let mut binfo = BlockInfo::new(Type::P, cbp, q);
                    binfo.set_bpart(bbinfo);
                    if !is_4x4 {
                        let mvec: [MV; 1] = [decode_mv(br, &self.tables.mv_cb)?];
                        binfo.set_mv(&mvec);
                    } else {
                        let mvec: [MV; 4] = [
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?,
                                decode_mv(br, &self.tables.mv_cb)?
                            ];
                        binfo.set_mv(&mvec);
                    }
                    if self.is_pb {
                        let mut mvec: Vec<MV> = Vec::with_capacity(bbinfo.get_num_mv());
                        for _ in 0..bbinfo.get_num_mv() {
                            let mv = decode_mv(br, &self.tables.mv_cb)?;
                            mvec.push(mv);
                        }
                        binfo.set_b_mv(mvec.as_slice());
                    }
                    Ok(binfo)
                },
            _ => { Err(DecoderError::InvalidData) },
        }
    }

    #[allow(unused_variables)]
    fn decode_block_intra(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, true, coded, blk)
    }

    #[allow(unused_variables)]
    fn decode_block_inter(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, false, coded, blk)
    }

    fn is_slice_end(&mut self) -> bool { self.br.peek(16) == 0 }

    fn filter_row(&mut self, buf: &mut NAVideoBuffer<u8>, mb_y: usize, mb_w: usize, cbpi: &CBPInfo) {
        let stride  = buf.get_stride(0);
        let mut off = buf.get_offset(0) + mb_y * 16 * stride;
        for mb_x in 0..mb_w {
            let coff = off;
            let coded0 = cbpi.is_coded(mb_x, 0);
            let coded1 = cbpi.is_coded(mb_x, 1);
            let q = cbpi.get_q(mb_w + mb_x);
            if mb_y != 0 {
                if coded0 && cbpi.is_coded_top(mb_x, 0) { deblock_hor(buf, 0, q, coff); }
                if coded1 && cbpi.is_coded_top(mb_x, 1) { deblock_hor(buf, 0, q, coff + 8); }
            }
            let coff = off + 8 * stride;
            if cbpi.is_coded(mb_x, 2) && coded0 { deblock_hor(buf, 0, q, coff); }
            if cbpi.is_coded(mb_x, 3) && coded1 { deblock_hor(buf, 0, q, coff + 8); }
            off += 16;
        }
        let mut leftt = false;
        let mut leftc = false;
        let mut off = buf.get_offset(0) + mb_y * 16 * stride;
        for mb_x in 0..mb_w {
            let ctop0 = cbpi.is_coded_top(mb_x, 0);
            let ctop1 = cbpi.is_coded_top(mb_x, 0);
            let ccur0 = cbpi.is_coded(mb_x, 0);
            let ccur1 = cbpi.is_coded(mb_x, 1);
            let q = cbpi.get_q(mb_w + mb_x);
            if mb_y != 0 {
                let coff = off - 8 * stride;
                let qtop = cbpi.get_q(mb_x);
                if leftt && ctop0 { deblock_ver(buf, 0, qtop, coff); }
                if ctop0 && ctop1 { deblock_ver(buf, 0, qtop, coff + 8); }
            }
            if leftc && ccur0 { deblock_ver(buf, 0, q, off); }
            if ccur0 && ccur1 { deblock_ver(buf, 0, q, off + 8); }
            leftt = ctop1;
            leftc = ccur1;
            off += 16;
        }
        let strideu  = buf.get_stride(1);
        let stridev  = buf.get_stride(2);
        let offu = buf.get_offset(1) + mb_y * 8 * strideu;
        let offv = buf.get_offset(2) + mb_y * 8 * stridev;
        if mb_y != 0 {
            for mb_x in 0..mb_w {
                let ctu = cbpi.is_coded_top(mb_x, 4);
                let ccu = cbpi.is_coded(mb_x, 4);
                let ctv = cbpi.is_coded_top(mb_x, 5);
                let ccv = cbpi.is_coded(mb_x, 5);
                let q = cbpi.get_q(mb_w + mb_x);
                if ctu && ccu { deblock_hor(buf, 1, q, offu + mb_x * 8); }
                if ctv && ccv { deblock_hor(buf, 2, q, offv + mb_x * 8); }
            }
            let mut leftu = false;
            let mut leftv = false;
            let offu = buf.get_offset(1) + (mb_y - 1) * 8 * strideu;
            let offv = buf.get_offset(2) + (mb_y - 1) * 8 * stridev;
            for mb_x in 0..mb_w {
                let ctu = cbpi.is_coded_top(mb_x, 4);
                let ctv = cbpi.is_coded_top(mb_x, 5);
                let qt = cbpi.get_q(mb_x);
                if leftu && ctu { deblock_ver(buf, 1, qt, offu + mb_x * 8); }
                if leftv && ctv { deblock_ver(buf, 2, qt, offv + mb_x * 8); }
                leftu = ctu;
                leftv = ctv;
            }
        }
    }
}

fn deblock_hor(buf: &mut NAVideoBuffer<u8>, comp: usize, q: u8, off: usize) {
    let stride = buf.get_stride(comp);
    let mut dptr = buf.get_data_mut();
    let mut buf = dptr.as_mut_slice();
    for x in 0..8 {
        let a = buf[off - 2 * stride + x] as i16;
        let b = buf[off - 1 * stride + x] as i16;
        let c = buf[off + 0 * stride + x] as i16;
        let d = buf[off + 1 * stride + x] as i16;
        let diff = ((a - d) * 3 + (c - b) * 8) >> 4;
        if (diff != 0) && (diff >= -32) && (diff < 32) {
            let d0 = diff.abs() * 2 - (q as i16);
            let d1 = if d0 < 0 { 0 } else { d0 };
            let d2 = diff.abs() - d1;
            let d3 = if d2 < 0 { 0 } else { d2 };

            let delta = if diff < 0 { -d3 } else { d3 };

            let b1 = b + delta;
            if      b1 < 0   { buf[off - 1 * stride + x] = 0; }
            else if b1 > 255 { buf[off - 1 * stride + x] = 0xFF; }
            else             { buf[off - 1 * stride + x] = b1 as u8; }
            let c1 = c - delta;
            if      c1 < 0   { buf[off + x] = 0; }
            else if c1 > 255 { buf[off + x] = 0xFF; }
            else             { buf[off + x] = c1 as u8; }
        }
    }
}

fn deblock_ver(buf: &mut NAVideoBuffer<u8>, comp: usize, q: u8, off: usize) {
    let stride = buf.get_stride(comp);
    let mut dptr = buf.get_data_mut();
    let mut buf = dptr.as_mut_slice();
    for y in 0..8 {
        let a = buf[off - 2 + y * stride] as i16;
        let b = buf[off - 1 + y * stride] as i16;
        let c = buf[off + 0 + y * stride] as i16;
        let d = buf[off + 1 + y * stride] as i16;
        let diff = ((a - d) * 3 + (c - b) * 8) >> 4;
        if (diff != 0) && (diff >= -32) && (diff < 32) {
            let d0 = diff.abs() * 2 - (q as i16);
            let d1 = if d0 < 0 { 0 } else { d0 };
            let d2 = diff.abs() - d1;
            let d3 = if d2 < 0 { 0 } else { d2 };

            let delta = if diff < 0 { -d3 } else { d3 };

            let b1 = b + delta;
            if      b1 < 0   { buf[off - 1 + y * stride] = 0; }
            else if b1 > 255 { buf[off - 1 + y * stride] = 0xFF; }
            else             { buf[off - 1 + y * stride] = b1 as u8; }
            let c1 = c - delta;
            if      c1 < 0   { buf[off + y * stride] = 0; }
            else if c1 > 255 { buf[off + y * stride] = 0xFF; }
            else             { buf[off + y * stride] = c1 as u8; }
        }
    }
}

impl Intel263Decoder {
    fn new() -> Self {
        let mut coderead = H263ShortCodeReader::new(H263_INTRA_MCBPC);
        let intra_mcbpc_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_INTER_MCBPC);
        let inter_mcbpc_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_CBPY);
        let cbpy_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263RLCodeReader::new(H263_RL_CODES);
        let rl_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263RLCodeReader::new(H263_RL_CODES_AIC);
        let aic_rl_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut coderead = H263ShortCodeReader::new(H263_MV);
        let mv_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let tables = Tables {
            intra_mcbpc_cb: intra_mcbpc_cb,
            inter_mcbpc_cb: inter_mcbpc_cb,
            cbpy_cb:        cbpy_cb,
            rl_cb:          rl_cb,
            aic_rl_cb:      aic_rl_cb,
            mv_cb:          mv_cb,
        };

        Intel263Decoder{
            info:           Rc::new(DUMMY_CODEC_INFO),
            dec:            H263BaseDecoder::new(),
            tables:         tables,
        }
    }
}

impl NADecoder for Intel263Decoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let fmt = formats::YUV420_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = Rc::new(NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()));
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

        if src.len() == 8 {
            let bret = self.dec.get_bframe();
            let buftype;
            let is_skip;
            if let Ok(btype) = bret {
                buftype = btype;
                is_skip = false;
            } else {
                buftype = NABufferType::None;
                is_skip = true;
            }
            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), buftype);
            frm.set_keyframe(false);
            frm.set_frame_type(if is_skip { FrameType::Skip } else { FrameType::B });
            return Ok(Rc::new(RefCell::new(frm)));
        }
        let mut ibr = Intel263BR::new(&src, &self.tables);

        let bufinfo = self.dec.parse_frame(&mut ibr)?;

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(self.dec.is_intra());
        frm.set_frame_type(if self.dec.is_intra() { FrameType::I } else { FrameType::P });
        Ok(Rc::new(RefCell::new(frm)))
    }
}


pub fn get_decoder() -> Box<NADecoder> {
    Box::new(Intel263Decoder::new())
}

#[cfg(test)]
mod test {
    use codecs::*;
    use demuxers::*;
    use io::byteio::*;

    #[test]
    fn test_intel263() {
        let avi_dmx = find_demuxer("avi").unwrap();
        let mut file = File::open("assets/neal73_saber.avi").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = avi_dmx.new_demuxer(&mut br);
        dmx.open().unwrap();

        let mut decs: Vec<Option<Box<NADecoder>>> = Vec::new();
        for i in 0..dmx.get_num_streams() {
            let s = dmx.get_stream(i).unwrap();
            let info = s.get_info();
            let decfunc = find_decoder(info.get_name());
            if let Some(df) = decfunc {
                let mut dec = (df)();
                dec.init(info).unwrap();
                decs.push(Some(dec));
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
            //if pkt.get_pts().unwrap() > 146 { break; }
            let streamno = pkt.get_stream().get_id() as usize;
            if let Some(ref mut dec) = decs[streamno] {
                let frm = dec.decode(&pkt).unwrap();
                if pkt.get_stream().get_info().is_video() {
                    write_pgmyuv("ih263_", streamno, pkt.get_pts().unwrap(), frm);
                }
            }
        }
//panic!("THE END");
    }
}
