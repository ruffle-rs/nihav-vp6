use nihav_core::codecs::*;
use nihav_core::io::byteio::*;

#[derive(Default)]
struct Decryptor {
    key:    [u8; 4],
}

impl Decryptor {
    fn decrypt(&mut self, buf: &mut [u8]) {
        let mut pos: u8 = 0;
        for el in buf.iter_mut() {
            *el ^= self.key[(pos & 3) as usize];
            pos = pos.wrapping_add(1);
        }
    }
    fn set_state(&mut self, mut key: u32) {
        for _ in 0..3 {
            let bit31 = (key >> 31) & 1;
            let bit21 = (key >> 21) & 1;
            let bit01 = (key >>  1) & 1;
            let nbit0 = !key & 1;
            key = (key << 1) | (bit31 ^ bit21 ^ bit01 ^ nbit0);
        }
        for i in 0..4 {
            self.key[i] = (key >> (8 * (i ^ 3))) as u8;
        }
    }
}

struct Deltas {
    tabs:       [[i16; 256]; 2],
    codebook:   [[u8; 8]; 256],
    num_elems:  [usize; 256],
    num_vq:     usize,
    vq_idx:     usize,
    vq_pos:     usize,
}

impl Deltas {
    fn reset(&mut self, br: &mut ByteReader) -> DecoderResult<()> {
        let b                               = br.read_byte()? as usize;
        self.vq_idx = b;
        self.vq_pos = 0;
        Ok(())
    }
    fn get_val(&mut self, br: &mut ByteReader) -> DecoderResult<u8> {
        if self.vq_idx > self.codebook.len() { return Err(DecoderError::ShortData); }
        let ret = self.codebook[self.vq_idx][self.vq_pos];
        self.vq_pos += 1;
        if self.vq_pos == self.num_elems[self.vq_idx] {
            if br.left() > 0 {
                self.reset(br)?;
            } else {
                self.vq_idx = self.codebook.len() + 1;
            }
        }
        Ok(ret)
    }
    fn get_int(&mut self, br: &mut ByteReader) -> DecoderResult<i16> {
        let b = self.get_val(br)?;
        if b > 0 { unimplemented!(); }
        Ok(b as i16)
    }
    fn get_dy(&mut self, br: &mut ByteReader) -> DecoderResult<i16> {
        let b = self.get_val(br)?;
        Ok(self.tabs[1][b as usize])
    }
    fn get_dc(&mut self, br: &mut ByteReader) -> DecoderResult<i16> {
        let b = self.get_val(br)?;
        Ok(self.tabs[0][b as usize])
    }
}

impl Default for Deltas {
    fn default() -> Self {
        Self {
            tabs:       [[0; 256]; 2],
            codebook:   [[0; 8]; 256],
            num_elems:  [0; 256],
            num_vq:     0,
            vq_idx:     0,
            vq_pos:     0,
        }
    }
}

#[derive(Clone,Copy, Default)]
struct BlkInfo {
    btype:      u8,
    mode:       u8,
    mv_x:       i16,
    mv_y:       i16,
}

const NUM_CPARAMS: usize = 25;
const CPARAM_NONE: u8 = 42;
const CPARAM_MISSING: u8 = 42 * 2;

macro_rules! apply_delta {
    ($buf:expr, $off:expr, $stride:expr, $hpred: expr, $delta:expr) => {
        $hpred = $hpred.wrapping_add($delta);
        $buf[$off] = $buf[$off - $stride].wrapping_add($hpred);
    };
}
macro_rules! copy_line {
    ($buf:expr, $off:expr, $stride:expr) => {
        for i in 0..8 {
            $buf[$off + i] = $buf[$off + i - $stride];
        }
    };
}

#[derive(Default)]
struct TM2XDecoder {
    info:       Rc<NACodecInfo>,
    width:      usize,
    height:     usize,
    dec_buf:    Vec<u8>,
    version:    u8,
    deltas:     Deltas,
    blk_info:   Vec<BlkInfo>,
    tile_size:  usize,
    cparams:    [[u8; 8]; NUM_CPARAMS],
    ydata:      Vec<i16>,
    udata:      Vec<i16>,
    vdata:      Vec<i16>,
    ystride:    usize,
    cstride:    usize,
}

impl TM2XDecoder {
    fn new() -> Self { Self::default() }
    fn output_frame(&mut self, buf: &mut NAVideoBuffer<u8>) {
        let fmt = buf.get_info().get_format();
        let offs = [fmt.get_chromaton(0).unwrap().get_offset() as usize,
                    fmt.get_chromaton(1).unwrap().get_offset() as usize,
                    fmt.get_chromaton(2).unwrap().get_offset() as usize];
        let stride = buf.get_stride(0);
        let mut data = buf.get_data_mut();
        let dst = data.as_mut_slice();

        let mut off = 0;
        let mut ysrc = self.ystride;
        let mut csrc = self.cstride;
        for _y in 0..self.height {
            let out = &mut dst[off..];
            for (x, pic) in out.chunks_exact_mut(3).take(self.width).enumerate() {
                let y = self.ydata[ysrc + x];
                let u = self.udata[csrc + x] - 128;
                let v = self.vdata[csrc + x] - 128;
                pic[offs[0]] = (y + v).max(0).min(255) as u8;
                pic[offs[1]] = y.max(0).min(255) as u8;
                pic[offs[2]] = (y + u).max(0).min(255) as u8;
            }
            off += stride;
            ysrc += self.ystride;
            csrc += self.cstride;
        }
    }
    fn parse_init(&mut self, version: u8) -> DecoderResult<()> {
        self.version = version;

        let mut mr = MemoryReader::new_read(&self.dec_buf);
        let mut br = ByteReader::new(&mut mr);
        if version > 4 {
            let _smth                   = br.read_u32be()?;
        }
        let height                      = br.read_u16be()? as usize;
        let width                       = br.read_u16be()? as usize;
        validate!(width == self.width && height == self.height);
        if version > 4 {
            let _smth                   = br.read_u32be()?;
        }
        let _smth                       = br.read_byte()?;
        let _nfuncs                     = br.read_byte()? as usize;
        let _smth                       = br.read_u16be()? as usize;
        let has_mv                      = br.read_byte()?;
        if has_mv != 0 {
            unimplemented!();
        }
        if version >= 4 {
            let _flags                  = br.read_u16be()?;
            let id_len                  = br.read_byte()? as usize;
                                          br.read_skip(id_len)?;
            let _smth1                  = br.read_byte()?;
            let len                     = br.read_byte()? as usize;
                                          br.read_skip(len)?;
            let _smth                   = br.read_u32be()?;
        }
        
        Ok(())
    }
    fn parse_tabs(&mut self) -> DecoderResult<()> {
        let mut mr = MemoryReader::new_read(&self.dec_buf);
        let mut br = ByteReader::new(&mut mr);

        let idx                         = br.read_byte()? as usize;
        validate!(idx < self.deltas.tabs.len());
        let len                         = br.read_byte()? as usize;
        validate!(((len * 2) as i64) == br.left());
        for i in 0..len {
            self.deltas.tabs[idx][i]    = br.read_u16be()? as i16;
        }

        Ok(())
    }
    fn parse_cb_desc(&mut self, version: u8) -> DecoderResult<()> {
        let mut mr = MemoryReader::new_read(&self.dec_buf);
        let mut br = ByteReader::new(&mut mr);

        if version == 0x0A {
            let _esc_val                = br.read_byte()?;
            let _tag                    = br.read_u16be()?;
        }
        let len                         = br.read_u16be()? as usize;
        validate!(len + 3 == (br.left() as usize));
        let num_entries                 = br.read_u16be()?;
        validate!(num_entries == 256);
        let max_elems                   = br.read_byte()?;
        validate!(max_elems > 0 && max_elems <= 8);
        let mut idx = 0;
        while br.left() > 0 {
            validate!(idx < self.deltas.codebook.len());
            let num_elems               = br.read_byte()? as usize;
            validate!(num_elems <= 8);
            self.deltas.num_elems[idx] = num_elems;
            for i in 0..num_elems {
                self.deltas.codebook[idx][i]    = br.read_byte()?;
            }
            idx += 1;
        }
        validate!(idx == 256);
        self.deltas.num_vq = idx;
        Ok(())
    }

    fn decode_frame(&mut self, src: &[u8]) -> DecoderResult<()> {
        let mut mr = MemoryReader::new_read(src);
        let mut br = ByteReader::new(&mut mr);

        self.deltas.reset(&mut br)?;
        let bw = self.width / 8;
        let bh = self.height / 8;
        let ntiles = (bw + self.tile_size - 1) / self.tile_size;
        let mut ypos = self.ystride;
        let mut cpos = self.cstride;
        for _by in 0..bh {
            for tile in 0..ntiles {
                let xpos = tile * self.tile_size;
                let len = self.tile_size.min(bw - xpos);
                for el in self.blk_info.iter_mut().take(len) {
                    let t1                      = self.deltas.get_val(&mut br)?;
                    let t2                      = self.deltas.get_val(&mut br)?;
                    if t2 > 1 { unimplemented!(); }
                    validate!((t1 as usize) < NUM_CPARAMS);
                    validate!(self.cparams[t1 as usize][0] != CPARAM_MISSING);
                    el.btype = t1;
                    el.mode  = t2;
                    if t2 > 0 {
                        el.mv_x                 = self.deltas.get_int(&mut br)?;
                        el.mv_y                 = self.deltas.get_int(&mut br)?;
                    } else {
                        el.mv_x = 0;
                        el.mv_y = 0;
                    }
                }
                for line in 0..8 {
                    let mut ypred = 0i16;
                    let mut upred = 0i16;
                    let mut vpred = 0i16;
                    for x in 0..len {
                        let bx = xpos + x;
                        let op = self.cparams[self.blk_info[x].btype as usize][line];
                        let cur_yoff = ypos + bx * 8 + line * self.ystride;
                        let cur_coff = cpos + bx * 8 + line * self.cstride;
                        match op {
                            0 => { // y4|y4
                                for i in 0..8 {
                                    let delta = self.deltas.get_dy(&mut br)?;
                                    apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                }
                                copy_line!(self.udata, cur_coff, self.cstride);
                                copy_line!(self.vdata, cur_coff, self.cstride);
                                upred = 0;
                                vpred = 0;
                            },
                            1 => { // y2|y2
                                for i in 0..8 {
                                    if (i & 1) == 0 {
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                                copy_line!(self.udata, cur_coff, self.cstride);
                                copy_line!(self.vdata, cur_coff, self.cstride);
                                upred = 0;
                                vpred = 0;
                            },
                            2 => { // y1|y1
                                for i in 0..8 {
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                                copy_line!(self.udata, cur_coff, self.cstride);
                                copy_line!(self.vdata, cur_coff, self.cstride);
                                upred = 0;
                                vpred = 0;
                            },
                            3 => { // y1|0
                                let delta = self.deltas.get_dy(&mut br)?;
                                apply_delta!(self.ydata, cur_yoff, self.ystride, ypred, delta);
                                for i in 1..8 {
                                    self.ydata[cur_yoff + i] = self.ydata[cur_yoff];
                                }
                                copy_line!(self.udata, cur_coff, self.cstride);
                                copy_line!(self.vdata, cur_coff, self.cstride);
                                upred = 0;
                                vpred = 0;
                            },
                            4 => { // c2y2c2y2|c2y2c2y2
                                for i in (0..8).step_by(2) {
                                    let delta = self.deltas.get_dc(&mut br)?;
                                    apply_delta!(self.udata, cur_coff + i + 0, self.cstride, upred, delta);
                                    self.udata[cur_coff + i + 1] = self.udata[cur_coff + i];
                                    let delta = self.deltas.get_dc(&mut br)?;
                                    apply_delta!(self.vdata, cur_coff + i + 0, self.cstride, vpred, delta);
                                    self.vdata[cur_coff + i + 1] = self.vdata[cur_coff + i];
                                    let delta = self.deltas.get_dy(&mut br)?;
                                    apply_delta!(self.ydata, cur_yoff + i + 0, self.ystride, ypred, delta);
                                    let delta = self.deltas.get_dy(&mut br)?;
                                    apply_delta!(self.ydata, cur_yoff + i + 1, self.ystride, ypred, delta);
                                }
                            },
                            5 => { // c2y1|c2y1
                                for i in 0..8 {
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            6 | 7 => unreachable!(),
                            8 => { // c2y4|c2y4
                                for i in 0..8 {
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                    }
                                    let delta = self.deltas.get_dy(&mut br)?;
                                    apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                }
                            },
                            9 => { // c2y2|c2y2
                                for i in 0..8 {
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                    }
                                    if (i & 1) == 0 {
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            10 => { // c2y1|c2y1
                                for i in 0..8 {
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            11 => unreachable!(),
                            12 => { // c2y8
                                for i in 0..8 {
                                    if i == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                    }
                                    let delta = self.deltas.get_dy(&mut br)?;
                                    apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                }
                            },
                            13 => { // c2y4
                                for i in 0..8 {
                                    if i == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                    }
                                    if (i & 1) == 0 {
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            14 => { // c2y2
                                for i in 0..8 {
                                    if i == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                    }
                                    if (i & 3) == 0 {
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            15 => { // c2y1
                                for i in 0..8 {
                                    if i == 0 {
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.udata, cur_coff + i, self.cstride, upred, delta);
                                        let delta = self.deltas.get_dc(&mut br)?;
                                        apply_delta!(self.vdata, cur_coff + i, self.cstride, vpred, delta);
                                        let delta = self.deltas.get_dy(&mut br)?;
                                        apply_delta!(self.ydata, cur_yoff + i, self.ystride, ypred, delta);
                                    } else {
                                        self.udata[cur_coff + i] = self.udata[cur_coff + i - 1];
                                        self.vdata[cur_coff + i] = self.vdata[cur_coff + i - 1];
                                        self.ydata[cur_yoff + i] = self.ydata[cur_yoff + i - 1];
                                    }
                                }
                            },
                            CPARAM_NONE => {
                                copy_line!(self.ydata, cur_yoff, self.ystride);
                                copy_line!(self.udata, cur_coff, self.cstride);
                                copy_line!(self.vdata, cur_coff, self.cstride);
                                ypred = 0;
                                upred = 0;
                                vpred = 0;
                            },
                            _ => unreachable!(),
                        }
                    }
                }
            }
            ypos += 8 * self.ystride;
            cpos += 8 * self.cstride;
        }

        Ok(())
    }
}

impl NADecoder for TM2XDecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(vinfo.get_width(), vinfo.get_height(), false, RGB24_FORMAT));
            self.width  = vinfo.get_width();
            self.height = vinfo.get_height();
            self.ystride    = self.width;
            self.cstride    = self.width;
            self.ydata.resize(self.ystride * (self.height + 1), 0x80);
            self.udata.resize(self.cstride * (self.height + 1), 0x80);
            self.vdata.resize(self.cstride * (self.height + 1), 0x80);
            self.info = Rc::new(NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()));
            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();
        validate!(src.len() > 8);
        let mut mr = MemoryReader::new_read(&src);
        let mut br = ByteReader::new(&mut mr);
        let mut dec = Decryptor::default();

        let mut initialised = false;
        let mut got_key = false;
        let mut data_size = 0;
        self.cparams = [[CPARAM_MISSING; 8]; NUM_CPARAMS];
        while br.left() >= 8 {
            let magic                           = br.read_u24be()?;
            let ctype                           = br.read_byte()?;
            let size                            = br.read_u32be()? as usize;
            validate!(magic == 0xA00001 && br.left() >= (size as i64));
            if ctype == 0x06 {
                validate!(size >= 4);
                let key                     = br.read_u32be()?;
                validate!((key as usize) == size - 4);
                dec.set_state(key);
                data_size = size - 4;
                br.read_skip(size - 4)?;
                got_key = true;
                continue;
            }
            self.dec_buf.resize(size, 0);
                                                  br.read_buf(&mut self.dec_buf)?;
            if ctype != 0x0A {
                dec.decrypt(&mut self.dec_buf);
            }
            match ctype {
                0x08 | 0x0C | 0x10 | 0x11 | 0x15 | 0x16 => {
                    if ctype < 0x15 { // old versions are not encrypted, roll back and zero key
                        dec.decrypt(&mut self.dec_buf);
                        dec.set_state(0);
                        got_key = true;
                    }
                    validate!(got_key && !initialised);
                    match ctype {
                        0x08 => self.parse_init(0)?,
                        0x0C => self.parse_init(1)?,
                        0x10 => self.parse_init(2)?,
                        0x11 => self.parse_init(3)?,
                        0x15 => self.parse_init(4)?,
                        0x16 => self.parse_init(5)?,
                        _ => unreachable!(),
                    };
                    initialised = true;
                },
                0x09 => {
                    validate!(initialised);
                    validate!(self.dec_buf.len() == 3);
                    validate!(self.dec_buf[0] == 8);
                    validate!(self.dec_buf[1] > 0);
                    validate!(self.dec_buf[2] == 1);
                    self.tile_size = self.dec_buf[1] as usize;
                    self.blk_info.resize(self.tile_size, BlkInfo::default());
                },
                0x0B => {
                    validate!(initialised);
                    validate!(self.dec_buf.len() == 4);
                    let idx = self.dec_buf[3] as usize;
                    validate!(idx < NUM_CPARAMS);
                    validate!(self.dec_buf[0] != 0);
                    validate!((self.dec_buf[0] as usize) < TM2X_CODING_PARAMS.len());
                    let tab = &TM2X_CODING_PARAMS[self.dec_buf[0] as usize];
                    let m0 = tab[0] as usize;
                    let m1 = tab[1] as usize;
                    let m2 = tab[2] as usize;
                    let m3 = tab[3] as usize;
                    let full_mode = (m2 * 4 + m0) as u8;
                    let lores_mode = m0 as u8;
                    for i in 0..8 {
                        if (i % m1) == 0 && (i % m3) == 0 {
                            self.cparams[idx][i] = full_mode;
                        } else if (i % m1) == 0 {
                            self.cparams[idx][i] = lores_mode;
                        } else {
                            self.cparams[idx][i] = CPARAM_NONE;
                        }
                    }
                },
                0x02 => {
                    validate!(initialised);
                    self.parse_tabs()?;
                },
                0x0A => {
                    validate!(initialised);
                    self.parse_cb_desc(0xA)?;
                },
                _ => { unimplemented!(); },
            };
        }
        self.decode_frame(&src[12..][..data_size])?;

        let myinfo = NAVideoInfo::new(self.width, self.height, false, RGB24_FORMAT);
        let bufret = alloc_video_buffer(myinfo, 2);
        if let Err(_) = bufret { return Err(DecoderError::InvalidData); }
        let bufinfo = bufret.unwrap();
        let mut buf = bufinfo.get_vbuf().unwrap();

        let is_intra = true;
        self.output_frame(&mut buf);

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(is_intra);
        frm.set_frame_type(if is_intra { FrameType::I } else { FrameType::P });
        Ok(Rc::new(RefCell::new(frm)))
    }
}

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(TM2XDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::duck_register_all_codecs;
    use nihav_commonfmt::demuxers::generic_register_all_demuxers;
    #[test]
    fn test_tm2x() {
        let mut dmx_reg = RegisteredDemuxers::new();
        generic_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        duck_register_all_codecs(&mut dec_reg);

        test_file_decoding("avi", "assets/Duck/TM2x.avi", Some(16), true, false, None/*Some("tm2x")*/, &dmx_reg, &dec_reg);
    }
}

const TM2X_CODING_PARAMS: [[u8; 4]; 25] = [
    [ 0, 0, 0, 0 ], [ 0, 1, 1, 1 ], [ 0, 1, 1, 2 ], [ 0, 1, 2, 4 ], [ 1, 1, 2, 4 ],
    [ 0, 2, 2, 4 ], [ 1, 2, 2, 4 ], [ 2, 2, 2, 4 ], [ 1, 4, 2, 4 ], [ 2, 4, 2, 4 ],
    [ 2, 8, 3, 8 ], [ 3, 4, 3, 8 ], [ 3, 8, 3, 8 ], [ 0, 1, 1, 4 ], [ 0, 1, 2, 2 ],
    [ 0, 2, 1, 4 ], [ 1, 1, 2, 2 ], [ 1, 4, 2, 8 ], [ 2, 2, 3, 4 ], [ 2, 4, 3, 8 ],
    [ 0, 1, 3, 8 ], [ 1, 2, 3, 8 ], [ 2, 4, 2, 4 ], [ 2, 4, 3, 8 ], [ 3, 8, 3, 8 ]
];
