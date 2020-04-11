use nihav_core::codecs::*;
use nihav_core::io::byteio::*;
use nihav_codec_support::codecs::HAMShuffler;
use std::str::FromStr;
use std::sync::Arc;

macro_rules! lz_op {
    (read; $dst:ident, $dpos:expr, $window:ident, $wpos:expr, $br:expr, $dst_size:expr) => {
        validate!($dpos < $dst_size);
        let b = $br.read_byte()?;
        $dst[$dpos] = b;
        $dpos += 1;
        $window[$wpos] = b;
        $wpos = ($wpos + 1) & 0xFFF;
    };
    (copy; $dst:ident, $dpos:expr, $window:ident, $wpos:expr, $off:expr, $dst_size:expr) => {
        let b = $window[$off];
        validate!($dpos < $dst_size);
        $dst[$dpos] = b;
        $dpos += 1;
        $window[$wpos] = b;
        $wpos = ($wpos + 1) & 0xFFF;
        $off = ($off + 1) & 0xFFF;
    };
}
fn lz_unpack(br: &mut ByteReader, dst: &mut [u8]) -> DecoderResult<()> {
    let mut window: [u8; 0x1000] = [0x20; 0x1000];

    let dst_size = br.read_u32le()? as usize;
    validate!(dst_size <= dst.len());
    let mut pos;
    let esc_len;
    if br.peek_u32le()? == 0x56781234 {
        br.read_skip(4)?;
        pos = 0x111;
        esc_len = 15;
    } else {
        pos = 0xFEE;
        esc_len = 255;
    }

    let mut opos = 0;
    while br.left() > 0 && opos < dst_size {
        let op = br.read_byte()?;
        if (op == 0xFF) && (br.left() > 8) {
            for _ in 0..8 {
                lz_op!(read; dst, opos, window, pos, br, dst_size);
            }
        } else {
            for i in 0..8 {
                if opos == dst_size { break; }
                let is_literal = ((op >> i) & 1) != 0;
                if is_literal {
                    lz_op!(read; dst, opos, window, pos, br, dst_size);
                } else {
                    let b0 = br.read_byte()? as usize;
                    let b1 = br.read_byte()? as usize;
                    let mut off = b0 | ((b1 & 0xF0) << 4);
                    let mut len = b1 & 0xF;
                    if len == esc_len {
                        len = (br.read_byte()? as usize) + esc_len;
                    }
                    for _ in 0..len+3 {
                        lz_op!(copy; dst, opos, window, pos, off, dst_size);
                    }
                }
            }
        }
    }
    Ok(())
}

fn rle_unpack(br: &mut ByteReader, len: usize, dst: &mut [u8]) -> DecoderResult<()> {
    let end = br.tell() + (len as u64);
    let mut dpos = 0;
    if (len & 1) != 0 {
        dst[dpos] = br.read_byte()?;
        dpos += 1;
    }
    while dpos < dst.len() && br.tell() < end {
        let val = br.read_byte()?;
        let len = ((val & 0x7F) as usize) * 2;
        validate!(dpos + len <= dst.len());
        if (val & 0x80) != 0 {
            let dst = &mut dst[dpos..][..len];
            br.read_buf(dst)?;
        } else {
            let val = br.read_byte()?;
            for i in 0..len {
                dst[dpos + i] = val;
            }
        }
        dpos += len;
    }
    Ok(())
}

fn decode_frame_data(br: &mut ByteReader, dst: &mut [u8], mut dpos: usize, stride: usize, w: usize, h: usize, method: u8) -> DecoderResult<bool> {
    match method {
        1 => {
            for _ in 0..h {
                let mut x = 0;
                while x < w {
                    let val                     = br.read_byte()?;
                    let len = ((val & 0x7F) as usize) + 1;
                    validate!(x + len <= w);
                    if (val & 0x80) != 0 {
                        let pix = &mut dst[dpos + x..][..len];
                                                  br.read_buf(pix)?;
                    } // otherwise skip already existing data
                    x += len;
                }
                dpos += stride;
            }
            Ok(false)
        },
        2 => {
            for _ in 0..h {
                let pix = &mut dst[dpos..][..w];
                                                  br.read_buf(pix)?;
                dpos += stride;
            }
            Ok(true)
        },
        3 => {
            for _ in 0..h {
                let mut x = 0;
                while x < w {
                    let val                     = br.read_byte()?;
                    let len = ((val & 0x7F) as usize) + 1;
                    validate!(x + len <= w);
                    if (val & 0x80) != 0 {
                        let pix = &mut dst[dpos + x..][..len];
                        if br.peek_byte()? == 0xFF {
                                                  br.read_skip(1)?;
                            rle_unpack(br, len, pix)?;
                        } else {
                                                  br.read_buf(pix)?;
                        }
                    } // otherwise data is already there
                    x += len;
                }
                dpos += stride;
            }
            Ok(false)
        },
        _ => Err(DecoderError::InvalidData),
    }
}

struct VMDVideoDecoder {
    info:       NACodecInfoRef,
    pal:        [u8; 768],
    buf:        Vec<u8>,
    width:      usize,
    height:     usize,
    hams:       HAMShuffler,
}

impl VMDVideoDecoder {
    fn new() -> Self {
        Self {
            info:       NACodecInfoRef::default(),
            pal:        [0; 768],
            buf:        Vec::new(),
            width:      0,
            height:     0,
            hams:       HAMShuffler::default(),
        }
    }
    fn decode_frame(&mut self, br: &mut ByteReader, buf: &mut NAVideoBuffer<u8>) -> DecoderResult<bool> {
        let paloff = buf.get_offset(1);
        let stride = buf.get_stride(0);
        let data = buf.get_data_mut().unwrap();
        let dst = data.as_mut_slice();

        let frame_x                             = br.read_u16le()? as usize;
        let frame_y                             = br.read_u16le()? as usize;
        let frame_l                             = br.read_u16le()? as usize;
        let frame_d                             = br.read_u16le()? as usize;
                                                  br.read_skip(1)?;
        let flags                               = br.read_byte()?;
        let has_pal = (flags & 0x02) != 0;
        if (frame_x == 0xFFFF) && (frame_y == 0xFFFF) && (frame_l == 0xFFFF) && (frame_d == 0xFFFF) {
            return Ok(false);
        }
        validate!(frame_l >= frame_x && frame_d >= frame_y);
        validate!(frame_l < self.width && frame_d < self.height);

        if has_pal {
                                                  br.read_skip(2)?;
            for e in self.pal.iter_mut() {
                let val                         = br.read_byte()?;
                *e = (val << 2) | (val >> 4);
            }
        }

        let dpal = &mut dst[paloff..][..768];
        dpal.copy_from_slice(&self.pal[0..]);

        if br.left() == 0 { return Ok(false); }

        let w = frame_l + 1 - frame_x;
        let h = frame_d + 1 - frame_y;
        let dpos = frame_x + frame_y * stride;

        let method                              = br.read_byte()?;
        let is_intra;
        if (method & 0x80) != 0 {
            validate!(!self.buf.is_empty());
            lz_unpack(br, &mut self.buf)?;
            let mut mr = MemoryReader::new_read(&self.buf);
            let mut buf_br = ByteReader::new(&mut mr);
            is_intra = decode_frame_data(&mut buf_br, dst, dpos, stride, w, h, method & 0x7F)?;
        } else {
            is_intra = decode_frame_data(br, dst, dpos, stride, w, h, method & 0x7F)?;
        }
        Ok(is_intra && frame_x == 0 && frame_y == 0 && w == self.width && h == self.height)
    }
}

impl NADecoder for VMDVideoDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            self.width  = vinfo.get_width();
            self.height = vinfo.get_height();
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(self.width, self.height, false, PAL8_FORMAT));
            self.info = NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()).into_ref();
            validate!(info.get_extradata().is_some());

            if let Some(ref edata) = info.get_extradata() {
                validate!(edata.len() == 0x330);
                let unp_size = read_u32le(&edata[800..])? as usize;
                validate!(unp_size < self.width * self.height * 3 + 64); // just for sanity
                self.buf.resize(unp_size, 0);
                for i in 0..768 {
                    let el = edata[28 + i];
                    self.pal[i] = (el << 2) | (el >> 4);
                }
            }

            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        validate!(src.len() >= 10);

        let mut mr = MemoryReader::new_read(&src);
        let mut br = ByteReader::new(&mut mr);

        let mut buf;
        let bufret = self.hams.clone_ref();
        if let Some(bbuf) = bufret {
            buf = bbuf;
        } else {
            let bufinfo = alloc_video_buffer(self.info.get_properties().get_video_info().unwrap(), 4)?;
            buf = bufinfo.get_vbuf().unwrap();
            self.hams.add_frame(buf);
            buf = self.hams.get_output_frame().unwrap();
        }

        let is_intra = self.decode_frame(&mut br, &mut buf)?;

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), NABufferType::Video(buf));
        frm.set_keyframe(is_intra);
        frm.set_frame_type(if is_intra { FrameType::I } else { FrameType::P });
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        self.hams.clear();
    }
}


pub fn get_decoder_video() -> Box<dyn NADecoder + Send> {
    Box::new(VMDVideoDecoder::new())
}

#[derive(Clone,Copy,PartialEq)]
enum VMDAudioMode {
    U8,
    DPCM,
    StereoDPCM,
}

struct VMDAudioDecoder {
    ainfo:      NAAudioInfo,
    info:       Arc<NACodecInfo>,
    chmap:      NAChannelMap,
    blk_align:  usize,
    blk_size:   usize,
    mode:       VMDAudioMode,
    pred:       [i32; 2],
    last_byte:  Option<u8>,
    is_odd:     bool,
    ch:         usize,
}

const SOL_AUD_STEPS16: [i16; 128] = [
     0x00,   0x08,   0x10,   0x20,   0x30,   0x40,   0x50,   0x60,
     0x70,   0x80,   0x90,   0xA0,   0xB0,   0xC0,   0xD0,   0xE0,
     0xF0,  0x100,  0x110,  0x120,  0x130,  0x140,  0x150,  0x160,
    0x170,  0x180,  0x190,  0x1A0,  0x1B0,  0x1C0,  0x1D0,  0x1E0,
    0x1F0,  0x200,  0x208,  0x210,  0x218,  0x220,  0x228,  0x230,
    0x238,  0x240,  0x248,  0x250,  0x258,  0x260,  0x268,  0x270,
    0x278,  0x280,  0x288,  0x290,  0x298,  0x2A0,  0x2A8,  0x2B0,
    0x2B8,  0x2C0,  0x2C8,  0x2D0,  0x2D8,  0x2E0,  0x2E8,  0x2F0,
    0x2F8,  0x300,  0x308,  0x310,  0x318,  0x320,  0x328,  0x330,
    0x338,  0x340,  0x348,  0x350,  0x358,  0x360,  0x368,  0x370,
    0x378,  0x380,  0x388,  0x390,  0x398,  0x3A0,  0x3A8,  0x3B0,
    0x3B8,  0x3C0,  0x3C8,  0x3D0,  0x3D8,  0x3E0,  0x3E8,  0x3F0,
    0x3F8,  0x400,  0x440,  0x480,  0x4C0,  0x500,  0x540,  0x580,
    0x5C0,  0x600,  0x640,  0x680,  0x6C0,  0x700,  0x740,  0x780,
    0x7C0,  0x800,  0x900,  0xA00,  0xB00,  0xC00,  0xD00,  0xE00,
    0xF00, 0x1000, 0x1400, 0x1800, 0x1C00, 0x2000, 0x3000, 0x4000
];

impl VMDAudioDecoder {
    fn new() -> Self {
        Self {
            ainfo:  NAAudioInfo::new(0, 1, SND_S16P_FORMAT, 0),
            info:   NACodecInfo::new_dummy(),
            chmap:  NAChannelMap::new(),
            blk_align: 0,
            blk_size: 0,
            mode:   VMDAudioMode::U8,
            pred:   [0; 2],
            last_byte: None,
            is_odd: false,
            ch:     0,
        }
    }
    fn decode_16bit(&self, dst: &mut [i16], off1: usize, br: &mut ByteReader, nblocks: usize, mut mask: u32) -> DecoderResult<()> {
        let channels = self.chmap.num_channels();
        let mut off = [0, off1];
        for _ in 0..nblocks {
            if (mask & 1) != 0 {
                for ch in 0..channels {
                    for i in 0..self.blk_align {
                        dst[off[ch] + i] = 0;
                    }
                    off[ch] += self.blk_align;
                }
            } else {
                let mut pred: [i32; 2] = [0; 2];
                for ch in 0..channels {
                    pred[ch]                        = i32::from(br.read_u16le()?);
                    dst[off[ch]] = pred[ch] as i16;
                    off[ch] += 1;
                }
                let mut ch = 0;
                let flip_ch = if channels == 2 { 1 } else { 0 };
                for _ in channels..self.blk_align*channels {
                    pred[ch] = Self::pred16(pred[ch], br.read_byte()?);
                    //pred[ch] = pred[ch].max(-32768).min(32767);
                    dst[off[ch]] = pred[ch] as i16;
                    off[ch] += 1;
                    ch ^= flip_ch;
                }
            }
            mask >>= 1;
        }
        validate!(br.left() == 0);
        Ok(())
    }
    fn pred16(pred: i32, val: u8) -> i32 {
        if (val & 0x80) != 0 {
            pred - i32::from(SOL_AUD_STEPS16[(val & 0x7F) as usize])
        } else {
            pred + i32::from(SOL_AUD_STEPS16[(val & 0x7F) as usize])
        }
    }
    fn cvt_u8(val: u8) -> u8 {
        val ^ 0x80
    }
}

impl NADecoder for VMDAudioDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            let fmt;
            let channels = ainfo.get_channels() as usize;
            let edata = info.get_extradata();
            let flags = if let Some(ref buf) = edata {
                    validate!(buf.len() >= 2);
                    (buf[0] as u16) | ((buf[1] as u16) << 8)
                } else {
                    0
                };
            validate!((channels == 1) ^ ((flags & 0x8200) != 0));
            if ainfo.get_format().get_bits() == 8 {
                self.blk_size = ainfo.get_block_len();
                self.blk_align = ainfo.get_block_len() / channels;
                if (flags & 0x8000) == 0 {
                    fmt = SND_U8_FORMAT;
                    self.mode = VMDAudioMode::U8;
                } else {
                    fmt = SND_S16_FORMAT;
                    self.mode = VMDAudioMode::StereoDPCM;
                    self.is_odd = (channels == 2) && ((self.blk_size & 1) != 0);
                }
            } else {
                fmt = SND_S16P_FORMAT;
                self.blk_size = (ainfo.get_block_len() + 1) * channels;
                self.blk_align = ainfo.get_block_len();
                self.mode = VMDAudioMode::DPCM;
            };
            self.ainfo = NAAudioInfo::new(ainfo.get_sample_rate(), ainfo.get_channels(), fmt, ainfo.get_block_len());
            self.info = info.replace_info(NACodecTypeInfo::Audio(self.ainfo.clone()));
            self.chmap = NAChannelMap::from_str(if channels == 1 { "C" } else { "L,R" }).unwrap();
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        if let NACodecTypeInfo::Audio(_) = info.get_properties() {
            let pktbuf = pkt.get_buffer();
            validate!(pktbuf.len() >= 6);
            let mut mr = MemoryReader::new_read(&pktbuf);
            let mut br = ByteReader::new(&mut mr);
            let blk_type                        = br.read_byte()?;
                                                  br.read_skip(9)?;
            let mask;
            let nblocks;
            if blk_type == 2 { // initial
                mask                            = br.read_u32le()?;
                nblocks = (mask.count_ones() as usize) + (pktbuf.len() - 14) / self.blk_size;
            } else if blk_type == 3 { // silence
                mask    = 1;
                nblocks = 1;
            } else {
                mask    = 0;
                nblocks = 1;
            }
            let mut samples = nblocks * self.blk_align;
            if self.mode == VMDAudioMode::StereoDPCM && self.is_odd {
                samples += (nblocks + if self.last_byte.is_some() { 1 } else { 0 }) / 2;
            }
            let abuf = alloc_audio_buffer(self.ainfo, samples, self.chmap.clone())?;
            match self.mode {
                VMDAudioMode::DPCM => {
                    let mut adata = abuf.get_abuf_i16().unwrap();
                    let off1 = adata.get_offset(1);
                    let mut dst = adata.get_data_mut().unwrap();
                    self.decode_16bit(&mut dst, off1, &mut br, nblocks, mask)?;
                },
                VMDAudioMode::U8 => {
                    let mut adata = abuf.get_abuf_u8().unwrap();
                    let dst = adata.get_data_mut().unwrap();
                    let mut doff = 0;
                    let mut mask = mask;
                    let channels = self.chmap.num_channels();
                    for _ in 0..nblocks {
                        if (mask & 1) != 0 {
                            for i in 0..self.blk_align * channels {
                                dst[doff + i] = 128;
                            }
                        } else if channels == 1 {
                            for i in 0..self.blk_size {
                                dst[doff + i]       = br.read_byte()?;
                            }
                        } else {
                            for i in 0..self.blk_size {
                                let val             = Self::cvt_u8(br.read_byte()?);
                                dst[doff + i] = val;
                            }
                        }
                        doff += self.blk_align * channels;
                        mask >>= 1;
                    }
                },
                VMDAudioMode::StereoDPCM => {
                    let mut adata = abuf.get_abuf_i16().unwrap();
                    let dst = adata.get_data_mut().unwrap();
                    let mut doff = 0;
                    let mut mask = mask;
                    let mut ch = self.ch;
                    for _ in 0..nblocks {
                        let put_sample = self.last_byte.is_some();
                        if let (true, Some(val)) = (self.is_odd, self.last_byte) {
                            self.pred[ch] = Self::pred16(self.pred[ch], val);
                            dst[doff] = self.pred[ch] as i16;
                            doff += 1;
                            ch ^= 1;
                            self.last_byte = None;
                        }
                        if (mask & 1) != 0 {
                            for i in 0..self.blk_align {
                                dst[doff + i * 2 + 0] = self.pred[ch]     as i16;
                                dst[doff + i * 2 + 1] = self.pred[ch ^ 1] as i16;
                            }
                            if self.is_odd {
                                if put_sample {
                                    dst[doff + self.blk_align * 2] = self.pred[ch] as i16;
                                    doff += 1;
                                    ch ^= 1;
                                } else {
                                    self.last_byte = Some(0);
                                }
                            }
                        } else {
                            for i in 0..self.blk_align {
                                self.pred[ch] = Self::pred16(self.pred[ch], br.read_byte()?);
                                dst[doff + i * 2] = self.pred[ch] as i16;
                                self.pred[ch ^ 1] = Self::pred16(self.pred[ch ^ 1], br.read_byte()?);
                                dst[doff + i * 2 + 1] = self.pred[ch ^ 1] as i16;
                            }
                            if self.is_odd {
                                let val                     = br.read_byte()?;
                                if put_sample {
                                    self.pred[ch] = Self::pred16(self.pred[ch], val);
                                    dst[doff + self.blk_align * 2] = self.pred[ch] as i16;
                                    doff += 1;
                                    ch ^= 1;
                                } else {
                                    self.last_byte = Some(val);
                                }
                            }
                        }
                        doff += self.blk_align * 2;
                        mask >>= 1;
                    }
                    self.ch = ch;
                },
            };

            let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), abuf);
            frm.set_duration(Some(samples as u64));
            frm.set_keyframe(true);
            Ok(frm.into_ref())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn flush(&mut self) {
    }
}

pub fn get_decoder_audio() -> Box<dyn NADecoder + Send> {
    Box::new(VMDAudioDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::*;
    use crate::game_register_all_codecs;
    use crate::game_register_all_demuxers;
    #[test]
    fn test_vmd_video() {
        let mut dmx_reg = RegisteredDemuxers::new();
        game_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        game_register_all_codecs(&mut dec_reg);

        test_decoding("vmd", "vmd-video", "assets/Game/2832.VMD", Some(10), &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5Frames(vec![
                            [0xd29e0214, 0xf38ad154, 0xccbd381f, 0x3de1109c],
                            [0x904074eb, 0x202b1d6f, 0xe3f68538, 0xf0db641c],
                            [0x9c8b1b6c, 0xe205b8dc, 0xbfb07406, 0x993ace41],
                            [0x71ce4220, 0x8747fd05, 0x854dd86d, 0x2664cde5],
                            [0x3bc65fa4, 0xebb95292, 0xe0a0fea6, 0x0acfdea1],
                            [0x33982045, 0x8d11b69b, 0xac254a75, 0x63896a21],
                            [0xa667db33, 0x90e122d3, 0x2243da15, 0xcc4bffd2],
                            [0x518621c1, 0xb91412bc, 0x12312869, 0x141ef647],
                            [0x3069977e, 0x68fd3fa0, 0x2bfdb00d, 0x1e694684],
                            [0x246c12aa, 0x15137fb0, 0xa4b0fc3e, 0x626a2676],
                            [0x72cce7e3, 0x98506d04, 0xd4d8bbaf, 0x3cc5e32d]]));
    }
    #[test]
    fn test_vmd_audio_u8() {
        let mut dmx_reg = RegisteredDemuxers::new();
        game_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        game_register_all_codecs(&mut dec_reg);

        test_decoding("vmd", "vmd-audio", "assets/Game/1491.VMD", None, &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5([0x75037601, 0xbc7b3976, 0x6e1c948b, 0xf05a3d6c]));
    }
    #[test]
    fn test_vmd_audio_s16_old() {
        let mut dmx_reg = RegisteredDemuxers::new();
        game_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        game_register_all_codecs(&mut dec_reg);

        test_decoding("vmd", "vmd-audio", "assets/Game/2832.VMD", None, &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5([0x32dcdf0e, 0xee058684, 0x43ed5bf1, 0x2ff18b5a]));
    }
    #[test]
    fn test_vmd_audio_s16_new() {
        let mut dmx_reg = RegisteredDemuxers::new();
        game_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        game_register_all_codecs(&mut dec_reg);

        test_decoding("vmd", "vmd-audio", "assets/Game/1000.VMD", None, &dmx_reg, &dec_reg,
                      ExpectedTestResult::MD5([0xc36215d3, 0x96530a80, 0x89f1fa8e, 0x49da302b]));
    }
}
