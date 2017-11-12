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

#[derive(Clone,Copy)]
struct RPRInfo {
    present:    bool,
    bits:       u8,
    widths:     [usize; 8],
    heights:    [usize; 8],
}

struct RealVideo20Decoder {
    info:       Rc<NACodecInfo>,
    dec:        H263BaseDecoder,
    tables:     Tables,
    w:          usize,
    h:          usize,
    minor_ver:  u8,
    rpr:        RPRInfo,
}

struct RealVideo20BR<'a> {
    br:         BitReader<'a>,
    tables:     &'a Tables,
    num_slices: usize,
    slice_no:   usize,
    slice_off:  Vec<u32>,
    w:          usize,
    h:          usize,
    mb_w:       usize,
    mb_h:       usize,
    mb_x:       usize,
    mb_y:       usize,
    mb_pos_bits: u8,
    mb_count:   usize,
    mb_end:     usize,
    minor_ver:  u8,
    rpr:        RPRInfo,
    is_intra:   bool,
}

struct RV20SliceInfo {
    ftype:  Type,
    qscale: u8,
    mb_x:   usize,
    mb_y:   usize,
    mb_pos: usize,
    w:      usize,
    h:      usize,
}

impl RV20SliceInfo {
    fn new(ftype: Type, qscale: u8, mb_x: usize, mb_y: usize, mb_pos: usize, w: usize, h: usize) -> Self {
        RV20SliceInfo { ftype: ftype, qscale: qscale, mb_x: mb_x, mb_y: mb_y, mb_pos: mb_pos, w: w, h: h }
    }
}

impl<'a> RealVideo20BR<'a> {
    fn new(src: &'a [u8], tables: &'a Tables, width: usize, height: usize, minor_ver: u8, rpr: RPRInfo) -> Self {
        let nslices = (src[0] as usize) + 1;
        let mut slice_offs = Vec::with_capacity(nslices);
        {
            let offs = &src[1..][..nslices * 8];
            let mut br = BitReader::new(offs, offs.len(), BitReaderMode::BE);
            for _ in 0..nslices {
                br.skip(32).unwrap();
                let off = br.read(32).unwrap();
                slice_offs.push(off);
            }
        }
        let soff = nslices * 8 + 1;
        let mb_w = (width  + 15) >> 4;
        let mb_h = (height + 15) >> 4;
        let max_pos = mb_w * mb_h - 1;
        let mut mbpb = 0;
        for i in 0..H263_MBB.len() {
            if max_pos <= H263_MBB[i].blocks {
                mbpb = H263_MBB[i].bits;
                break;
            }
        }
        RealVideo20BR {
            br:         BitReader::new(&src[soff..], src.len() - soff, BitReaderMode::BE),
            tables:     tables,
            num_slices: nslices,
            slice_no:   0,
            slice_off:  slice_offs,
            w:          width,
            h:          height,
            mb_w:       mb_w,
            mb_h:       mb_h,
            mb_pos_bits: mbpb,
            mb_x:       0,
            mb_y:       0,
            mb_count:   0,
            mb_end:     0,
            minor_ver:  minor_ver,
            rpr:        rpr,
            is_intra:   false,
        }
    }

    fn decode_block(&mut self, quant: u8, intra: bool, coded: bool, blk: &mut [i16; 64], plane_no: usize) -> DecoderResult<()> {
        let mut br = &mut self.br;
        let mut idx = 0;
        if !self.is_intra && intra {
            let mut dc = br.read(8).unwrap() as i16;
            if dc == 255 { dc = 128; }
            blk[0] = dc << 3;
            idx = 1;
        }
        if !coded { return Ok(()); }

        let rl_cb = if self.is_intra { &self.tables.aic_rl_cb } else { &self.tables.rl_cb };
        let q_add = if quant == 0 { 0i16 } else { ((quant - 1) | 1) as i16 };
        let q = (quant * 2) as i16;
        while idx < 64 {
            let code = br.read_cb(rl_cb).unwrap();
            let run;
            let mut level;
            let last;
            if !code.is_escape() {
                run   = code.get_run();
                level = code.get_level();
                last  = code.is_last();
                if br.read_bool().unwrap() { level = -level; }
                level = (level * q) + q_add;
            } else {
                last  = br.read_bool().unwrap();
                run   = br.read(6).unwrap() as u8;
                level = br.read_s(8).unwrap() as i16;
                if level == -128 {
                    let low = br.read(5).unwrap() as i16;
                    let top = br.read_s(6).unwrap() as i16;
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
    let code = br.read_cb(mv_cb).unwrap() as i16;
    if code == 0 { return Ok(0) }
    if !br.read_bool().unwrap() {
        Ok(code)
    } else {
        Ok(-code)
    }
}

fn decode_mv(br: &mut BitReader, mv_cb: &Codebook<u8>) -> DecoderResult<MV> {
    let xval = decode_mv_component(br, mv_cb).unwrap();
    let yval = decode_mv_component(br, mv_cb).unwrap();
//println!("  MV {},{} @ {}", xval, yval, br.tell());
    Ok(MV::new(xval, yval))
}

impl<'a> BlockDecoder for RealVideo20BR<'a> {

#[allow(unused_variables)]
    fn decode_pichdr(&mut self) -> DecoderResult<PicInfo> {
        self.slice_no = 0;
println!("decoding picture header size {}", if self.num_slices > 1 { self.slice_off[1] } else { ((self.br.tell() as u32) + (self.br.left() as u32))/8 });
        self.mb_x = 0;
        self.mb_y = 0;
        self.mb_count = self.mb_w * self.mb_h;
        let shdr = self.read_slice_header().unwrap();
println!("slice ends @ {}\n", self.br.tell());
        self.slice_no += 1;
        validate!((shdr.mb_x == 0) && (shdr.mb_y == 0));
        let mb_count;
        if self.slice_no < self.num_slices {
            let pos = self.br.tell();
            let shdr2 = self.read_slice_header().unwrap();
            self.br.seek(pos as u32)?;
            mb_count = shdr2.mb_pos - shdr.mb_pos;
        } else {
            mb_count = self.mb_w * self.mb_h;
        }

        self.mb_x = shdr.mb_x;
        self.mb_y = shdr.mb_y;
        self.mb_count = mb_count;
        self.mb_end = shdr.mb_pos + mb_count;
        self.is_intra = shdr.ftype == Type::I;

        let picinfo = PicInfo::new(shdr.w, shdr.h, shdr.ftype, shdr.qscale, false, MVMode::Old, 0, None, true);
        Ok(picinfo)
    }

    #[allow(unused_variables)]
    fn decode_slice_header(&mut self, info: &PicInfo) -> DecoderResult<Slice> {
//println!("read slice {} header", self.slice_no);
        let shdr = self.read_slice_header().unwrap();
        self.slice_no += 1;
        let mb_count;
        if self.slice_no < self.num_slices {
            let shdr2 = self.read_slice_header().unwrap();
            mb_count = shdr2.mb_pos - shdr.mb_pos;
        } else {
            mb_count = self.mb_w * self.mb_h - shdr.mb_pos;
        }
        let ret = Slice::new(shdr.mb_x, shdr.mb_y, shdr.qscale);
        self.mb_x = shdr.mb_x;
        self.mb_y = shdr.mb_y;
        self.mb_count = mb_count;
        self.mb_end = shdr.mb_pos + mb_count;

        Ok(ret)
    }

    fn decode_block_header(&mut self, info: &PicInfo, slice: &Slice) -> DecoderResult<BlockInfo> {
        let mut br = &mut self.br;
        let mut q = slice.get_quant();
        match info.get_mode() {
            Type::I => {
                    let mut cbpc = br.read_cb(&self.tables.intra_mcbpc_cb).unwrap();
                    while cbpc == 8 { cbpc = br.read_cb(&self.tables.intra_mcbpc_cb).unwrap(); }
if self.is_intra {
let acpred = br.read_bool()?;
println!("   acp {} @ {}", acpred, br.tell());
if acpred {
    br.skip(1)?;//pred direction
}
}
                    let cbpy = br.read_cb(&self.tables.cbpy_cb).unwrap();
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    let dquant = (cbpc & 4) != 0;
                    if dquant {
                        let idx = br.read(2).unwrap() as usize;
                        q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                    }
println!(" MB {},{} CBP {:X} @ {}", self.mb_x, self.mb_y, cbp, br.tell());
            self.mb_x += 1;
            if self.mb_x == self.mb_w {
                self.mb_x = 0;
                self.mb_y += 1;
            }
                    Ok(BlockInfo::new(Type::I, cbp, q))
                },
            Type::P => {
                    if br.read_bool().unwrap() {
self.mb_x += 1;
if self.mb_x == self.mb_w {
    self.mb_x = 0;
    self.mb_y += 1;
}
                        return Ok(BlockInfo::new(Type::Skip, 0, info.get_quant()));
                    }
                    let mut cbpc = br.read_cb(&self.tables.inter_mcbpc_cb).unwrap();
                    while cbpc == 20 { cbpc = br.read_cb(&self.tables.inter_mcbpc_cb).unwrap(); }
                    let is_intra = (cbpc & 0x04) != 0;
                    let dquant   = (cbpc & 0x08) != 0;
                    let is_4x4   = (cbpc & 0x10) != 0;
                    if is_intra {
                        let cbpy = br.read_cb(&self.tables.cbpy_cb).unwrap();
                        let cbp = (cbpy << 2) | (cbpc & 3);
                        if dquant {
                            let idx = br.read(2).unwrap() as usize;
                            q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                        }
                        let binfo = BlockInfo::new(Type::I, cbp, q);
            self.mb_x += 1;
            if self.mb_x == self.mb_w {
                self.mb_x = 0;
                self.mb_y += 1;
            }
                        return Ok(binfo);
                    }

                    let mut cbpy = br.read_cb(&self.tables.cbpy_cb).unwrap();
//                    if /* !aiv && */(cbpc & 3) != 3 {
                        cbpy ^= 0xF;
//                    }
                    let cbp = (cbpy << 2) | (cbpc & 3);
                    if dquant {
                        let idx = br.read(2).unwrap() as usize;
                        q = ((q as i16) + (H263_DQUANT_TAB[idx] as i16)) as u8;
                    }
println!(" MB {}.{} cbp = {:X}", self.mb_x, self.mb_y, cbp);
                    let mut binfo = BlockInfo::new(Type::P, cbp, q);
                    if !is_4x4 {
                        let mvec: [MV; 1] = [decode_mv(br, &self.tables.mv_cb).unwrap()];
                        binfo.set_mv(&mvec);
                    } else {
                        let mvec: [MV; 4] = [
                                decode_mv(br, &self.tables.mv_cb).unwrap(),
                                decode_mv(br, &self.tables.mv_cb).unwrap(),
                                decode_mv(br, &self.tables.mv_cb).unwrap(),
                                decode_mv(br, &self.tables.mv_cb).unwrap()
                            ];
                        binfo.set_mv(&mvec);
                    }
            self.mb_x += 1;
            if self.mb_x == self.mb_w {
                self.mb_x = 0;
                self.mb_y += 1;
            }
                    Ok(binfo)
                },
            _ => { println!("wrong info mode"); Err(DecoderError::InvalidData) },
        }
    }

    #[allow(unused_variables)]
    fn decode_block_intra(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, true, coded, blk, if no < 4 { 0 } else { no - 3 })
    }

    #[allow(unused_variables)]
    fn decode_block_inter(&mut self, info: &BlockInfo, quant: u8, no: usize, coded: bool, blk: &mut [i16; 64]) -> DecoderResult<()> {
        self.decode_block(quant, false, coded, blk, if no < 4 { 0 } else { no - 3 })
    }

    fn is_slice_end(&mut self) -> bool { self.mb_x + self.mb_y * self.mb_w >= self.mb_end }

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

impl<'a> RealVideo20BR<'a> {
    fn read_slice_header(&mut self) -> DecoderResult<RV20SliceInfo> {
        validate!(self.slice_no < self.num_slices);

        let mut br = &mut self.br;
        br.seek(self.slice_off[self.slice_no] * 8).unwrap();
//println!(" slice at off {}", br.tell());

        let frm_type    = br.read(2).unwrap();
        let ftype = match frm_type {
                0 | 1 => { Type::I },
                2     => { Type::P },
                _     => { Type::Skip },
            };

        let marker      = br.read(1).unwrap();
        validate!(marker == 0);
        let qscale      = br.read(5).unwrap() as u8;
        validate!(qscale > 0);
        if self.minor_ver >= 2 {
            br.skip(1).unwrap(); // loop filter
        }
        let seq = if self.minor_ver <= 1 {
                br.read(8).unwrap()  << 7
            } else {
                br.read(13).unwrap() << 2
            };
        let w;
        let h;
        if self.rpr.present {
            let rpr = br.read(self.rpr.bits).unwrap() as usize;
            if rpr == 0 {
                w = self.w;
                h = self.h;
            } else {
                w = self.rpr.widths[rpr];
                h = self.rpr.heights[rpr];
                validate!((w != 0) && (h != 0));
            }
        } else {
            w = self.w;
            h = self.h;
        }

        let mb_pos = br.read(self.mb_pos_bits).unwrap() as usize;
        let mb_x = mb_pos % self.mb_w;
        let mb_y = mb_pos / self.mb_w;

        br.skip(1).unwrap(); // no rounding

        if (self.minor_ver <= 1) && (frm_type == 3) {
            br.skip(5).unwrap();
        }
println!("slice q {} mb {},{}", qscale, mb_x, mb_y);

        Ok(RV20SliceInfo::new(ftype, qscale, mb_x, mb_y, mb_pos, w, h))
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

impl RealVideo20Decoder {
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

        RealVideo20Decoder{
            info:           Rc::new(DUMMY_CODEC_INFO),
            dec:            H263BaseDecoder::new(),
            tables:         tables,
            w:              0,
            h:              0,
            minor_ver:      0,
            rpr:            RPRInfo { present: false, bits: 0, widths: [0; 8], heights: [0; 8] },
        }
    }
}

impl NADecoder for RealVideo20Decoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()> {
        if let NACodecTypeInfo::Video(vinfo) = info.get_properties() {
            let w = vinfo.get_width();
            let h = vinfo.get_height();
            let fmt = formats::YUV420_FORMAT;
            let myinfo = NACodecTypeInfo::Video(NAVideoInfo::new(w, h, false, fmt));
            self.info = Rc::new(NACodecInfo::new_ref(info.get_name(), myinfo, info.get_extradata()));
            self.w = w;
            self.h = h;

            let edata = info.get_extradata().unwrap();
            let src: &[u8] = &edata;
            let ver = ((src[4] as u32) << 12) | ((src[5] as u32) << 4) | ((src[6] as u32) >> 4);
            let maj_ver = ver >> 16;
            let min_ver = (ver >> 8) & 0xFF;
            let mic_ver = ver & 0xFF;
println!("ver {:06X}", ver);
            validate!(maj_ver == 2);
            self.minor_ver = min_ver as u8;
            let rprb = src[1] & 7;
            if rprb == 0 {
                self.rpr.present = false;
            } else {
                self.rpr.present = true;
                self.rpr.bits    = rprb as u8;
                for i in 4..(src.len()/2) {
                    self.rpr.widths [i - 4] = (src[i * 2]     as usize) * 4;
                    self.rpr.heights[i - 4] = (src[i * 2 + 1] as usize) * 4;
                }
            }
            Ok(())
        } else {
println!(".unwrap().unwrap().unwrap()");
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let src = pkt.get_buffer();

println!(" decode frame size {}, {} slices", src.len(), src[0]+1);
        let mut ibr = RealVideo20BR::new(&src, &self.tables, self.w, self.h, self.minor_ver, self.rpr);

        let bufinfo = self.dec.parse_frame(&mut ibr).unwrap();

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.clone(), bufinfo);
        frm.set_keyframe(self.dec.is_intra());
        frm.set_frame_type(if self.dec.is_intra() { FrameType::I } else { FrameType::P });
        Ok(Rc::new(RefCell::new(frm)))
    }
}

struct MBB { blocks: usize, bits: u8 }
const H263_MBB: &[MBB; 7] = &[
    MBB{ blocks:    47, bits:  6 },
    MBB{ blocks:    98, bits:  7 },
    MBB{ blocks:   395, bits:  9 },
    MBB{ blocks:  1583, bits: 11 },
    MBB{ blocks:  6335, bits: 13 },
    MBB{ blocks:  9215, bits: 14 },
    MBB{ blocks: 65536, bits: 14 },
];

pub fn get_decoder() -> Box<NADecoder> {
    Box::new(RealVideo20Decoder::new())
}

#[cfg(test)]
mod test {
    use test::dec_video::test_file_decoding;
    #[test]
    fn test_rv20() {
         test_file_decoding("realmedia", "assets/RV/rv20_cook_640x352_realproducer_plus_8.51.rm", None/*Some(160)*/, true, false, Some("rv20"));
    }
}
