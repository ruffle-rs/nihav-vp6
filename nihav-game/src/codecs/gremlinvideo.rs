use nihav_core::frame::*;
use nihav_core::formats;
use nihav_core::formats::{NAChannelType, NAChannelMap};
use nihav_core::codecs::*;
use nihav_core::io::byteio::*;

struct GremlinVideoDecoder {
    info:       NACodecInfoRef,
    pal:        [u8; 768],
    frame:      Vec<u8>,
    scale_v:    bool,
    scale_h:    bool,
}

struct Bits8 {
    queue: u8,
    fill:  u8,
}

struct Bits32 {
    queue: u32,
    fill:  u8,
}

const PREAMBLE_SIZE: usize = 4096;

impl Bits8 {
    fn new() -> Self { Bits8 { queue: 0, fill: 0 } }
    fn read_2bits(&mut self, br: &mut ByteReader) -> ByteIOResult<u8> {
        if self.fill == 0 {
            self.queue  = br.read_byte()?;
            self.fill  += 8;
        }
        let res = self.queue >> 6;
        self.queue <<= 2;
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
        GremlinVideoDecoder {
            info: NACodecInfoRef::default(), pal: [0; 768], frame: Vec::new(),
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
        let data = buf.get_data_mut().unwrap();
        let dst = data.as_mut_slice();
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
        let mut bits = Bits8::new();

        let mut size = self.info.get_properties().get_video_info().unwrap().get_width() *
                        self.info.get_properties().get_video_info().unwrap().get_height();
        let mut idx = PREAMBLE_SIZE;
        if self.frame[8] != 0 {
            for c in 0..256 {
                for i in 0..16 { self.frame[c * 16 + i] = c as u8; }
            }
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
                let bot = (b >> 4) as isize;
                let off = ((br.read_byte()? as isize) << 4) + bot - 4096;
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
        let mut bits = Bits8::new();

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
                let bot = (b >> 4) as isize;
                let off = ((br.read_byte()? as isize) << 4) + bot - 4096;
                validate!(len <= size);
                size -= len;
                self.lz_copy(idx, off, len)?;
                idx += len;
            } else if tag == 2 {
                let b = br.read_byte()?;
                if b == 0 { break; }
                let len: usize = (if b != 0xFF { b as usize } else { br.read_u16le()? as usize }) + 1;
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
    fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            if !vinfo.get_format().is_paletted() { return Err(DecoderError::NotImplemented); }
            let fmt = formats::PAL8_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();

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

pub fn get_decoder_video() -> Box<NADecoder> {
    Box::new(GremlinVideoDecoder::new())
}

struct GremlinAudioDecoder {
    ainfo:      NAAudioInfo,
    chmap:      NAChannelMap,
    delta_tab: [i16; 256],
    state0:     i16,
    state1:     i16,
}

impl GremlinAudioDecoder {
    fn new() -> Self {
        let mut delta_tab: [i16; 256] = [0; 256];
        let mut delta = 0;
        let mut code = 64;
        let mut step = 45;
        for i in 0..127 {
            delta += code >> 5;
            code  += step;
            step  += 2;
            delta_tab[i * 2 + 1] =  delta;
            delta_tab[i * 2 + 2] = -delta;
        }
        delta_tab[255] = 32767;//delta + (code >> 5);
        GremlinAudioDecoder {
            ainfo:  NAAudioInfo::new(0, 1, formats::SND_S16_FORMAT, 0),
            chmap:  NAChannelMap::new(),
            delta_tab,
            state0: 0,
            state1: 0,
        }
    }
}

const CHMAP_MONO: [NAChannelType; 1] = [NAChannelType::C];
const CHMAP_STEREO: [NAChannelType; 2] = [NAChannelType::L, NAChannelType::R];

fn get_default_chmap(nch: u8) -> NAChannelMap {
    let mut chmap = NAChannelMap::new();
    match nch {
        1 => chmap.add_channels(&CHMAP_MONO),
        2 => chmap.add_channels(&CHMAP_STEREO),
        _ => (),
    }
    chmap
}

impl NADecoder for GremlinAudioDecoder {
    fn init(&mut self, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            self.ainfo = NAAudioInfo::new(ainfo.get_sample_rate(), ainfo.get_channels(), formats::SND_S16P_FORMAT, ainfo.get_block_len());
            self.chmap = get_default_chmap(ainfo.get_channels());
            if self.chmap.num_channels() == 0 { return Err(DecoderError::InvalidData); }
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        if let NACodecTypeInfo::Audio(_) = info.get_properties() {
            let pktbuf = pkt.get_buffer();
            let samples = pktbuf.len() / self.chmap.num_channels();
            let abuf = alloc_audio_buffer(self.ainfo, samples, self.chmap.clone())?;
            let mut adata = abuf.get_abuf_i16().unwrap();
            let off1 = adata.get_offset(1);
            let buf = adata.get_data_mut().unwrap();
            if self.chmap.num_channels() == 2 {
                for (i, e) in pktbuf.chunks(2).enumerate() {
                    self.state0 = self.state0.wrapping_add(self.delta_tab[e[0] as usize]);
                    buf[i] = self.state0;
                    self.state1 = self.state1.wrapping_add(self.delta_tab[e[1] as usize]);
                    buf[off1 + i] = self.state1;
                }
            } else {
                for (i, e) in pktbuf.iter().enumerate() {
                    self.state0 += self.delta_tab[*e as usize];
                    buf[i] = self.state0;
                }
            }
            let mut frm = NAFrame::new_from_pkt(pkt, info, abuf);
            frm.set_duration(Some(samples as u64));
            frm.set_keyframe(false);
            Ok(Rc::new(RefCell::new(frm)))
        } else {
            Err(DecoderError::InvalidData)
        }
    }
}

pub fn get_decoder_audio() -> Box<NADecoder> {
    Box::new(GremlinAudioDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::test_file_decoding;
    use crate::codecs::game_register_all_codecs;
    use crate::demuxers::game_register_all_demuxers;
    #[test]
    fn test_gdv() {
        let mut dmx_reg = RegisteredDemuxers::new();
        game_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        game_register_all_codecs(&mut dec_reg);

        test_file_decoding("gdv", "assets/Game/intro1.gdv", Some(10), true, false, None, &dmx_reg, &dec_reg);
    }
}
