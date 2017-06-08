use formats;
use super::*;
use io::byteio::*;

struct GremlinVideoDecoder {
    info:       Rc<NACodecInfo>,
    pal:        [u8; 768],
    frame:      Vec<u8>,
    scale_v:    bool,
    scale_h:    bool,
}

struct Bits16 {
    queue: u16,
    fill:  u8,
}

struct Bits32 {
    queue: u32,
    fill:  u8,
}

const PREAMBLE_SIZE: usize = 4096;

impl Bits16 {
    fn new() -> Self { Bits16 { queue: 0, fill: 0 } }
    fn read_2bits(&mut self, br: &mut ByteReader) -> ByteIOResult<u16> {
        if self.fill == 0 {
            self.queue |= (br.read_byte()? as u16) << self.fill;
            self.fill  += 8;
        }
        let res = self.queue & 0x3;
        self.queue >>= 2;
        self.fill   -= 2;
        Ok(res)
    }
}

impl Bits32 {
    fn new() -> Self { Bits32 { queue: 0, fill: 0 } }
    fn fill(&mut self, br: &mut ByteReader) -> ByteIOResult<()> {
        self.queue = br.read_u32le()?;
        self.fill  = 32;
        Ok(())
    }
    fn read_bits(&mut self, br: &mut ByteReader, nbits: u8) -> ByteIOResult<u32> {
        let res = self.queue & ((1 << nbits) - 1);
        self.queue >>= nbits;
        self.fill   -= nbits;
        if self.fill <= 16 {
            self.queue |= (br.read_u16le()? as u32) << self.fill;
            self.fill  += 16;
        }
        Ok(res)
    }
}

impl GremlinVideoDecoder {
    fn new() -> Self {
        let dummy_info = Rc::new(DUMMY_CODEC_INFO);
        GremlinVideoDecoder {
            info: dummy_info, pal: [0; 768], frame: Vec::new(),
            scale_v: false, scale_h: false
        }
    }

    fn lz_copy(&mut self, idx: usize, offset: isize, len: usize) -> DecoderResult<()> {
        if idx + len > self.frame.len() { return Err(DecoderError::InvalidData); }
        if offset == -1 {
            let c = self.frame[idx - 1];
            for i in 0..len { self.frame[idx + i] = c; }
        } else if offset < 0 {
            let start = idx - (-offset as usize);
            for i in 0..len { self.frame[idx + i] = self.frame[start + i]; }
        } else {
            if idx + (offset as usize) + len > self.frame.len() { return Err(DecoderError::InvalidData); }
            let start = idx + (offset as usize);
            for i in 0..len { self.frame[idx + i] = self.frame[start + i]; }
        }
        Ok(())
    }

    fn rescale(&mut self, w: usize, h: usize, scale_v: bool, scale_h: bool) {
        if (self.scale_v == scale_v) && (self.scale_h == scale_h) { return; }

        if self.scale_h && self.scale_v {
            for j in 0..h {
                let y = h - j - 1;
                for i in 0..w {
                    let x = w - i - 1;
                    self.frame[PREAMBLE_SIZE + x + y * w] = self.frame[PREAMBLE_SIZE + x/2 + (y/2) * (w/2)];
                }
            }
        } else if self.scale_h {
            for j in 0..h {
                let y = h - j - 1;
                for x in 0..w {
                    self.frame[PREAMBLE_SIZE + x + y * w] = self.frame[PREAMBLE_SIZE + x + (y/2) * w];
                }
            }
        } else if self.scale_v {
            for j in 0..h {
                let y = h - j - 1;
                for i in 0..w {
                    let x = w - i - 1;
                    self.frame[PREAMBLE_SIZE + x + y * w] = self.frame[PREAMBLE_SIZE + x/2 + y * (w/2)];
                }
            }
        }

        if scale_h && scale_v {
            for y in 0..h/2 {
                for x in 0..w/2 {
                    self.frame[PREAMBLE_SIZE + x + y * (w/2)] = self.frame[PREAMBLE_SIZE + x*2 + y*2 * w];
                }
            }
        } else if scale_h {
            for y in 0..h/2 {
                for x in 0..w {
                    self.frame[PREAMBLE_SIZE + x + y * w] = self.frame[PREAMBLE_SIZE + x + y*2 * w];
                }
            }
        } else if scale_v {
            for y in 0..h {
                for x in 0..w/2 {
                    self.frame[PREAMBLE_SIZE + x + y * w] = self.frame[PREAMBLE_SIZE + x*2 + y * w];
                }
            }
        }

        self.scale_v = scale_v;
        self.scale_h = scale_h;
    }

    fn output_frame(&mut self, bufinfo: &mut NABufferType, w: usize, h: usize) {
        let bufo = bufinfo.get_vbuf();
        let mut buf = bufo.unwrap();
        let paloff = buf.get_offset(1);
        let stride = buf.get_stride(0);
        let mut data = buf.get_data_mut();
        let mut dst = data.as_mut_slice();
        let mut sidx = PREAMBLE_SIZE;
        let mut didx = 0;

        for i in 0..768 { dst[paloff + i] = self.pal[i]; }
        if !self.scale_v && !self.scale_h {
            for _ in 0..h {
                for x in 0..w { dst[didx + x] = self.frame[sidx + x]; }
                sidx += w;
                didx += stride;
            }
        } else {
            for y in 0..h {
                if !self.scale_v {
                    for x in 0..w { dst[didx + x] = self.frame[sidx + x]; }
                } else {
                    for x in 0..w { dst[didx + x] = self.frame[sidx + x/2]; }
                }
                if !self.scale_h || ((y & 1) == 1) {
                    sidx += if !self.scale_v { w } else { w/2 };
                }
                didx += stride;
            }
        }
    }

    fn decode_method2(&mut self, br: &mut ByteReader) -> DecoderResult<()> {
        let mut bits = Bits16::new();

        let mut size = self.info.get_properties().get_video_info().unwrap().get_width() *
                        self.info.get_properties().get_video_info().unwrap().get_height();
        let mut idx = PREAMBLE_SIZE;
        for c in 0..256 {
            for i in 0..16 { self.frame[c * 16 + i] = c as u8; }
        }
        while size > 0 {
            let tag = bits.read_2bits(br)?;
            if tag == 0 {
                self.frame[idx] = br.read_byte()?;
                size -= 1;
                idx  += 1;
            } else if tag == 1 {
                let b = br.read_byte()?;
                let len = ((b & 0xF) as usize) + 3;
                let top = (b >> 4) as isize;
                let off = (top << 8) + (br.read_byte()? as isize) - 4096;
                validate!(len <= size);
                size -= len;
                self.lz_copy(idx, off, len)?; 
                idx += len;
            } else if tag == 2 {
                let len = (br.read_byte()? as usize) + 2;
                validate!(len <= size);
                size -= len;
                idx += len;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn decode_method5(&mut self, br: &mut ByteReader, skip: usize) -> DecoderResult<()> {
        let mut bits = Bits16::new();

        let mut size = self.info.get_properties().get_video_info().unwrap().get_width() *
                        self.info.get_properties().get_video_info().unwrap().get_height();
        let mut idx = PREAMBLE_SIZE;
        validate!(size >= skip);
        size -= skip;
        idx  += skip;
        while size > 0 {
            let tag = bits.read_2bits(br)?;
            if tag == 0 {
                self.frame[idx] = br.read_byte()?;
                size -= 1;
                idx  += 1;
            } else if tag == 1 {
                let b = br.read_byte()?;
                let len = ((b & 0xF) as usize) + 3;
                let top = (b >> 4) as isize;
                let off = (top << 8) + (br.read_byte()? as isize) - 4096;
                validate!(len <= size);
                size -= len;
                self.lz_copy(idx, off, len)?; 
                idx += len;
            } else if tag == 2 {
                let b = br.read_byte()?;
                if b == 0 { break; }
                let len: usize = if b != 0xFF { b as usize } else { br.read_u16le()? as usize };
                validate!(len <= size);
                size -= len;
                idx += len;
            } else {
                let b = br.read_byte()?;
                let len = ((b & 0x3) as usize) + 2;
                let off = -((b >> 2) as isize) - 1;
                validate!(len <= size);
                size -= len;
                self.lz_copy(idx, off, len)?; 
                idx += len;
            }
        }
        Ok(())
    }

    fn decode_method68(&mut self, br: &mut ByteReader,
                       skip: usize, use8: bool) -> DecoderResult<()> {
        let mut bits = Bits32::new();

        let mut size = self.info.get_properties().get_video_info().unwrap().get_width() *
                        self.info.get_properties().get_video_info().unwrap().get_height();
        let mut idx = PREAMBLE_SIZE;
        validate!(size >= skip);
        size -= skip;
        idx  += skip;
        bits.fill(br)?;
        while size > 0 {
            let tag = bits.read_bits(br, 2)?;
            if tag == 0 { //draw
                let b = bits.read_bits(br, 1)?;
                if b == 0 {
                    self.frame[idx] = br.read_byte()?;
                    size -= 1;
                    idx  += 1;
                } else {
                    let mut len: usize = 2;
                    let mut lbits = 0;
                    loop {
                        lbits += 1;
                        let val = bits.read_bits(br, lbits)?;
                        len += val as usize;
                        if val != ((1 << lbits) - 1) { break; }
                        validate!(lbits < 16);
                    }
                    validate!(len <= size);
                    for i in 0..len { self.frame[idx + i] = br.read_byte()?; }
                    size -= len;
                    idx  += len;
                }
            } else if tag == 1 { //skip
                let b = bits.read_bits(br, 1)?;
                let len: usize;
                if b == 0 {
                    len = (bits.read_bits(br, 4)? as usize) + 2;
                } else {
                    let bb = br.read_byte()?;
                    if (bb & 0x80) == 0 {
                        len = (bb as usize) + 18;
                    } else {
                        let top = ((bb & 0x7F) as usize) << 8;
                        len = top + (br.read_byte()? as usize) + 146;
                    }
                }
                validate!(len <= size);
                size -= len;
                idx  += len;
            } else if tag == 2 {
                let subtag = bits.read_bits(br, 2)? as usize;
                if subtag != 3 {
                    let top = (bits.read_bits(br, 4)? as usize) << 8;
                    let offs = top + (br.read_byte()? as usize);
                    if (subtag != 0) || (offs <= 0xF80) {
                        let len = (subtag as usize) + 3;
                        self.lz_copy(idx, (offs as isize) - 4096, len)?;
                        idx += len;
                    } else {
                        if offs == 0xFFF { return Ok(()); }
                        let real_off = ((offs >> 4) & 0x7) + 1;
                        let len = ((offs & 0xF) + 2) * 2;
                        validate!(len <= size);
                        size -= len;
                        let c1 = self.frame[idx - real_off];
                        let c2 = self.frame[idx - real_off + 1];
                        for i in 0..len/2 {
                            self.frame[idx + i*2 + 0] = c1;
                            self.frame[idx + i*2 + 1] = c2;
                        }
                        idx += len;
                    }
                } else {
                    let b = br.read_byte()?;
                    let off = ((b & 0x7F) as usize) + 1;
                    let len = if (b & 0x80) == 0 { 2 } else { 3 };
                    validate!(len <= size);
                    size -= len;
                    self.lz_copy(idx, -(off as isize), len)?; 
                    idx += len;
                }
            } else {
                let len: usize;
                let off: isize;
                if use8 {
                    let b = br.read_byte()?;
                    if (b & 0xC0) == 0xC0 {
                        len = ((b & 0x3F) as usize) + 8;
                        let q = bits.read_bits(br, 4)? as isize;
                        off = (q << 8) + (br.read_byte()? as isize) + 1;
                    } else {
                        let ofs1: isize;
                        if (b & 0x80) == 0 {
                            len = ((b >> 4) as usize) + 6;
                            ofs1 = (b & 0xF) as isize;
                        } else {
                            len = ((b & 0x3F) as usize) + 14;
                            ofs1 = bits.read_bits(br, 4)? as isize;
                        }
                        off = (ofs1 << 8) + (br.read_byte()? as isize) - 4096;
                    }
                } else {
                    let b = br.read_byte()?;
                    if (b >> 4) == 0xF {
                        len = (br.read_byte()? as usize) + 21;
                    } else {
                        len = ((b >> 4) as usize) + 6;
                    }
                    let ofs1 = (b & 0xF) as isize;
                    off = (ofs1 << 8) + (br.read_byte()? as isize) - 4096;
                }
                validate!(len <= size);
                size -= len;
                self.lz_copy(idx, off, len)?; 
                idx += len;
            }
        }
        Ok(())
    }
}

impl NADecoder for GremlinVideoDecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            if !vinfo.get_format().is_paletted() { return Err(DecoderError::NotImplemented); }
            let fmt = formats::PAL8_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = Rc::new(NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()));

            self.frame.resize(PREAMBLE_SIZE + w * h, 0);
            for i in 0..2 {
                for j in 0..256 {
                    for k in 0..8 {
                        self.frame[i * 2048 + j * 8 + k] = j as u8;
                    }
                }
            }
            let edata = info.get_extradata().unwrap();
            validate!(edata.len() == 768);
            for c in 0..256 {
                for i in 0..3 {
                    let cc = edata[c * 3 + i];
                    self.pal[c * 3 + (2 - i)] = (cc << 2) | (cc >> 4);
                }
            }
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        let mut mr = MemoryReader::new_read(&src);
        let mut br = ByteReader::new(&mut mr);
        let flags = br.read_u32le()?;
        let w = self.info.get_properties().get_video_info().unwrap().get_width();
        let h = self.info.get_properties().get_video_info().unwrap().get_height();

        let cmethod = flags & 0xF;
        let is_intra = (flags & 0x40) != 0;
        let scale_v = (flags & 0x10) != 0;
        let scale_h = (flags & 0x20) != 0;

        self.rescale(w, h, scale_v, scale_h);

        if (cmethod == 0) || (cmethod == 1) {
            for c in 0..256 {
                for i in 0..3 {
                    let b = br.read_byte()?;
                    self.pal[c * 3 + (2 - i)] = (b << 2) | (b >> 4);
                }
            }
            if cmethod == 1 {
                for i in PREAMBLE_SIZE..self.frame.len() { self.frame[i] = 0x00; }
            }
            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), NABufferType::None);
            frm.set_keyframe(false);
            frm.set_frame_type(FrameType::Skip);
            return Ok(Rc::new(RefCell::new(frm)))
        } else if cmethod == 3 {
            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), NABufferType::None);
            frm.set_keyframe(false);
            frm.set_frame_type(FrameType::Skip);
            return Ok(Rc::new(RefCell::new(frm)))
        } else if cmethod == 2 {
            self.decode_method2(&mut br)?;
        } else if cmethod == 5 {
            self.decode_method5(&mut br, (flags >> 8) as usize)?;
        } else if cmethod == 6 {
            self.decode_method68(&mut br, (flags >> 8) as usize, false)?;
        } else if cmethod == 8 {
            self.decode_method68(&mut br, (flags >> 8) as usize, true)?;
        } else {
            return Err(DecoderError::NotImplemented);
        }

        let bufret = alloc_video_buffer(self.info.get_properties().get_video_info().unwrap(), 0);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let mut bufinfo = bufret.unwrap();

        self.output_frame(&mut bufinfo, w, h);

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(is_intra);
        frm.set_frame_type(if is_intra { FrameType::I } else { FrameType::P });
        Ok(Rc::new(RefCell::new(frm)))
    }
}

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(GremlinVideoDecoder::new())
}

#[cfg(test)]
mod test {
    use codecs::*;
    use demuxers::*;
    use io::byteio::*;
    use std::fs::File;

    #[test]
    fn test_gdv() {
        let gdv_dmx = find_demuxer("gdv").unwrap();
        let mut file = File::open("assets/intro1.gdv").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let mut dmx = gdv_dmx.new_demuxer(&mut br);
        dmx.open().unwrap();

        let mut decs: Vec<Option<Box<NADecoder>>> = Vec::new();
        for i in 0..dmx.get_num_streams() {
            let s = dmx.get_stream(i).unwrap();
            let info = s.get_info();
            let decfunc = find_decoder(info.get_name());
            if !info.is_video() {
                decs.push(None);
            } else if let Some(df) = decfunc {
                let mut dec = (df)();
                dec.init(info).unwrap();
                decs.push(Some(dec));
            } else {
panic!("decoder {} not found", info.get_name());
            }
        }

        loop {
            let pktres = dmx.get_frame();
            if let Err(e) = pktres {
                if e == DemuxerError::EOF { break; }
            }
            let pkt = pktres.unwrap();
            let streamno = pkt.get_stream().get_id() as usize;
            if let Some(ref mut dec) = decs[streamno] {
//                let frm = 
dec.decode(&pkt).unwrap();
//                if pkt.get_stream().get_info().is_video() {
//                    if frm.borrow().get_frame_type() != FrameType::Skip {
//                        write_palppm("gdv", streamno, pkt.get_pts().unwrap(), frm);
//                    }
//                }
            }
            if pkt.get_pts().unwrap() > 8 { break; }
        }
//panic!("end");
    }
}
