use nihav_core::formats::*;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_core::dsp::mdct::IMDCT;
use nihav_core::io::bitreader::*;
use nihav_core::io::byteio::{ByteReader, MemoryReader};
use nihav_core::io::codebook::*;
use nihav_core::io::intcode::*;
use std::f32::consts;
use std::mem::swap;

#[derive(Debug,Clone,Copy,PartialEq)]
enum Mode {
    Mono,
    Stereo,
    JointStereo,
}

impl Mode {
    fn get_channels(self) -> usize {
        match self {
            Mode::Mono  => 1,
            _           => 2,
        }
    }
}

struct CookBookReader {
    bits:  &'static [u8],
    codes: &'static [u16],
}
impl CodebookDescReader<u16> for CookBookReader {
    fn bits(&mut self, idx: usize) -> u8  { self.bits[idx] }
    fn code(&mut self, idx: usize) -> u32 { self.codes[idx] as u32 }
    fn sym (&mut self, idx: usize) -> u16 { idx as u16 }
    fn len(&mut self) -> usize { self.bits.len() }
}

struct Codebooks {
    cpl_cb:     [Codebook<u16>; 5],
    quant_cb:   Vec<Codebook<u16>>,
    vq_cb:      [Codebook<u16>; 7],
}

impl Codebooks {
    fn new() -> Self {
        let mut cpl0 = CookBookReader { codes: COOK_CPL_2BITS_CODES, bits: COOK_CPL_2BITS_BITS };
        let mut cpl1 = CookBookReader { codes: COOK_CPL_3BITS_CODES, bits: COOK_CPL_3BITS_BITS };
        let mut cpl2 = CookBookReader { codes: COOK_CPL_4BITS_CODES, bits: COOK_CPL_4BITS_BITS };
        let mut cpl3 = CookBookReader { codes: COOK_CPL_5BITS_CODES, bits: COOK_CPL_5BITS_BITS };
        let mut cpl4 = CookBookReader { codes: COOK_CPL_6BITS_CODES, bits: COOK_CPL_6BITS_BITS };
        let cpl_cb = [Codebook::new(&mut cpl0, CodebookMode::MSB).unwrap(),
                      Codebook::new(&mut cpl1, CodebookMode::MSB).unwrap(),
                      Codebook::new(&mut cpl2, CodebookMode::MSB).unwrap(),
                      Codebook::new(&mut cpl3, CodebookMode::MSB).unwrap(),
                      Codebook::new(&mut cpl4, CodebookMode::MSB).unwrap()];
        let mut quant_cb: Vec<Codebook<u16>> = Vec::with_capacity(COOK_QUANT_CODES.len());
        for i in 0..COOK_QUANT_CODES.len() {
            let mut quant = CookBookReader { codes: COOK_QUANT_CODES[i], bits: COOK_QUANT_BITS[i] };
            quant_cb.push(Codebook::new(&mut quant, CodebookMode::MSB).unwrap());
        }
        let mut vq0 = CookBookReader { codes: COOK_VQ0_CODES, bits: COOK_VQ0_BITS };
        let mut vq1 = CookBookReader { codes: COOK_VQ1_CODES, bits: COOK_VQ1_BITS };
        let mut vq2 = CookBookReader { codes: COOK_VQ2_CODES, bits: COOK_VQ2_BITS };
        let mut vq3 = CookBookReader { codes: COOK_VQ3_CODES, bits: COOK_VQ3_BITS };
        let mut vq4 = CookBookReader { codes: COOK_VQ4_CODES, bits: COOK_VQ4_BITS };
        let mut vq5 = CookBookReader { codes: COOK_VQ5_CODES, bits: COOK_VQ5_BITS };
        let mut vq6 = CookBookReader { codes: COOK_VQ6_CODES, bits: COOK_VQ6_BITS };
        let vq_cb = [Codebook::new(&mut vq0, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq1, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq2, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq3, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq4, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq5, CodebookMode::MSB).unwrap(),
                     Codebook::new(&mut vq6, CodebookMode::MSB).unwrap()];
        Codebooks {
            cpl_cb,
            quant_cb,
            vq_cb,
        }
    }
}

struct CookDSP {
    imdct:      IMDCT,
    window:     [f32; 1024],
    out:        [f32; 2048],
    size:       usize,
    pow_tab:    [f32; 128],
    hpow_tab:   [f32; 128],
    gain_tab:   [f32; 23],
}

impl CookDSP {
    fn new(samples: usize) -> Self {
        let fsamples = samples as f32;
        let mut window: [f32; 1024] = [0.0; 1024];
        let factor = consts::PI / (2.0 * fsamples);
        let scale = (2.0 / fsamples).sqrt() / 32768.0;
        for k in 0..samples {
            window[k] = (factor * ((k as f32) + 0.5)).sin() * scale;
        }
        let mut pow_tab: [f32; 128] = [0.0; 128];
        let mut hpow_tab: [f32; 128] = [0.0; 128];
        for i in 0..128 {
            pow_tab[i]  = 2.0f32.powf((i as f32) - 64.0);
            hpow_tab[i] = 2.0f32.powf(((i as f32) - 64.0) * 0.5);
        }
        let mut gain_tab: [f32; 23] = [0.0; 23];
        for i in 0..23 {
            gain_tab[i] = pow_tab[i + 53].powf(8.0 / fsamples);
        }
        let size = samples;
        CookDSP { imdct: IMDCT::new(samples*2, false), window, out: [0.0; 2048], size, pow_tab, hpow_tab, gain_tab }
    }
}

trait ClipCat {
    fn clip_cat(&self) -> usize;
}

impl ClipCat for i32 {
    fn clip_cat(&self) -> usize { ((*self).max(0) as usize).min(NUM_CATEGORIES - 1) }
}

const BAND_SIZE: usize = 20;
const MAX_SAMPLES: usize = MAX_SUBBANDS * BAND_SIZE;
const MAX_PAIRS: usize = 5;
const MAX_SUBBANDS: usize = 52;
const NUM_CATEGORIES: usize = 8;

#[derive(Clone,Copy)]
struct CookChannelPair {
    start_ch:       usize,
    mode:           Mode,
    samples:        usize,
    subbands:       usize,
    js_start:       usize,
    js_bits:        u8,
    vector_bits:    u8,

    decouple:       [u8; BAND_SIZE],
    category:       [u8; MAX_SUBBANDS * 2],

    block:          [[f32; MAX_SAMPLES * 2]; 2],
    delay:          [[f32; MAX_SAMPLES]; 2],
    gains:          [[i32; 9]; 2],
    prev_gains:     [[i32; 9]; 2],
    qindex:         [i8; MAX_SUBBANDS * 2],
}

impl CookChannelPair {
    fn new() -> Self {
        CookChannelPair {
            start_ch:       0,
            mode:           Mode::Mono,
            samples:        0,
            subbands:       0,
            js_start:       0,
            js_bits:        0,
            vector_bits:    0,

            decouple:       [0; BAND_SIZE],
            category:       [0; MAX_SUBBANDS * 2],

            block:          [[0.0; MAX_SAMPLES * 2]; 2],
            delay:          [[0.0; MAX_SAMPLES]; 2],
            gains:          [[0; 9]; 2],
            prev_gains:     [[0; 9]; 2],
            qindex:         [0; MAX_SUBBANDS * 2],
        }
    }
    fn read_hdr_v1(&mut self, br: &mut ByteReader) -> DecoderResult<()> {
        let ver                                         = br.read_u32be()?;
        let micro_ver = ver & 0xFF;
        self.samples                                    = br.read_u16be()? as usize;
        validate!(self.samples > 0 && ((self.samples & (self.samples - 1)) == 0));
        self.subbands                                   = br.read_u16be()? as usize;
        validate!(self.subbands <= MAX_SUBBANDS);
        match micro_ver {
            1 => {
                    self.mode       = Mode::Mono;
                    self.js_start   = 0;
                    self.js_bits    = 0;
                },
            2 => {
                    self.mode       = Mode::Stereo;
                    self.js_start   = 0;
                    self.js_bits    = 0;
                },
            3 => {
                    self.mode       = Mode::JointStereo;
                    let _delay                          = br.read_u32be()?;
                    self.js_start                       = br.read_u16be()? as usize;
                    self.js_bits                        = br.read_u16be()? as u8;
                    validate!(self.js_start < MAX_SUBBANDS);
                    validate!((self.js_bits >= 2) && (self.js_bits <= 6));
                },
            _ => { return Err(DecoderError::InvalidData);}
        }
        Ok(())
    }
    fn read_hdr_v2(&mut self, br: &mut ByteReader) -> DecoderResult<u32> {
        let ver                                         = br.read_u32be()?;
        validate!((ver >> 24) == 2);
        self.samples                                    = br.read_u16be()? as usize;
        self.subbands                                   = br.read_u16be()? as usize;
        validate!(self.subbands <= MAX_SUBBANDS);
        let _delay                                      = br.read_u32be()?;
        self.js_start                                   = br.read_u16be()? as usize;
        validate!(self.js_start < MAX_SUBBANDS);
        let js_bits                                     = br.read_u16be()?;
        let chmap                                       = br.read_u32be()?;
        if chmap.count_ones() == 1 {
            self.js_bits    = 0;
            self.mode       = Mode::Mono;
        } else {
            validate!((js_bits >= 2) && (js_bits <= 6));
            self.js_bits    = js_bits as u8;
            self.mode       = Mode::JointStereo;
        }
        Ok(chmap)
    }
    fn bitalloc(&mut self, num_vectors: usize, bits: usize) {
        let avail_bits = (if bits > self.samples { self.samples + ((bits - self.samples) * 5) / 8 } else { bits }) as i32;
        let total_subbands = self.subbands + self.js_start;

        let mut bias: i32 = -32;
        for i in 0..6 {
            let mut sum = 0;
            for j in 0..total_subbands {
                let idx = ((32 >> i) + bias - (self.qindex[j] as i32)) / 2;
                sum += COOK_EXP_BITS[idx.clip_cat()];
            }
            if sum >= (avail_bits - 32) {
                bias += 32 >> i;
            }
        }

        let mut exp_index1: [usize; MAX_SUBBANDS * 2] = [0; MAX_SUBBANDS * 2];
        let mut exp_index2: [usize; MAX_SUBBANDS * 2] = [0; MAX_SUBBANDS * 2];
        let mut sum = 0;
        for i in 0..total_subbands {
            let idx = ((bias - (self.qindex[i] as i32)) / 2).clip_cat();
            sum += COOK_EXP_BITS[idx];
            exp_index1[i] = idx;
            exp_index2[i] = idx;
        }

        let mut tbias1 = sum;
        let mut tbias2 = sum;
        let mut tcat: [usize; 128*2] = [0; 128*2];
        let mut tcat_idx1 = 128;
        let mut tcat_idx2 = 128;
        for _ in 1..(1 << self.vector_bits) {
            if tbias1 + tbias2 > avail_bits * 2 {
                let mut max = -999999;
                let mut idx = total_subbands + 1;
                for j in 0..total_subbands {
                    if exp_index1[j] >= (NUM_CATEGORIES - 1) { continue; }
                    let t = -2 * (exp_index1[j] as i32) - (self.qindex[j] as i32) + bias;
                    if t >= max {
                        max = t;
                        idx = j;
                    }
                }
                if idx >= total_subbands { break; }
                tcat[tcat_idx1] = idx;
                tcat_idx1 += 1;
                tbias1 -= COOK_EXP_BITS[exp_index1[idx]] - COOK_EXP_BITS[exp_index1[idx] + 1];
                exp_index1[idx] += 1;
            } else {
                let mut min = 999999;
                let mut idx = total_subbands + 1;
                for j in 0..total_subbands {
                    if exp_index2[j] == 0 { continue; }
                    let t = -2 * (exp_index2[j] as i32) - (self.qindex[j] as i32) + bias;
                    if t < min {
                        min = t;
                        idx = j;
                    }
                }
                if idx >= total_subbands { break; }
                tcat_idx2 -= 1;
                tcat[tcat_idx2] = idx;
                tbias2 -= COOK_EXP_BITS[exp_index2[idx]] - COOK_EXP_BITS[exp_index2[idx] - 1];
                exp_index2[idx] -= 1;
            }
        }
        for i in 0..total_subbands {
            self.category[i] = exp_index2[i] as u8;
        }

        for _ in 0..num_vectors {
            let idx = tcat[tcat_idx2];
            tcat_idx2 += 1;
            self.category[idx] = (self.category[idx] + 1).min((NUM_CATEGORIES - 1) as u8) as u8;
        }
    }
    fn decode_channel_data(&mut self, dsp: &mut CookDSP, rnd: &mut RND, codebooks: &Codebooks, src: &[u8], buf: &mut [u8], channel: usize) -> DecoderResult<()> {
        // decrypt
        for (i, b) in src.iter().enumerate() {
            buf[i] = b ^ COOK_XOR_KEY[i & 3];
        }
        let mut br = BitReader::new(buf, src.len(), BitReaderMode::BE);

        let num_gains                                   = br.read_code(UintCodeType::UnaryOnes)? as usize;
        validate!(num_gains <= 8);

        swap(&mut self.gains[channel], &mut self.prev_gains[channel]);
        self.block[channel] = [0.0; MAX_SAMPLES * 2];

        // gains
        let mut ipos = 0;
        for _ in 0..num_gains {
            let idx                                     = br.read(3)? as usize;
            let val;
            if br.read_bool()? {
                val                                     = (br.read(4)? as i32) - 7;
            } else {
                val = -1;
            }
            validate!(idx >= ipos);
            while ipos <= idx {
                self.prev_gains[channel][ipos] = val;
                ipos += 1;
            }
        }
        while ipos <= 8 {
            self.prev_gains[channel][ipos] = 0;
            ipos += 1;
        }

        // coupling information
        if self.mode == Mode::JointStereo {
            let cstart = COOK_CPL_BAND[self.js_start] as usize;
            let cend   = COOK_CPL_BAND[self.subbands - 1] as usize;
            if br.read_bool()? {
                let cb = &codebooks.cpl_cb[(self.js_bits - 2) as usize];
                for i in cstart..=cend {
                    self.decouple[i]                    = br.read_cb(cb)? as u8;
                }
            } else {
                for i in cstart..=cend {
                    self.decouple[i]                    = br.read(self.js_bits)? as u8;
                }
            }
        }

        // envelope
        let tot_subbands = self.subbands + self.js_start;
        self.qindex[0]                                  = (br.read(6)? as i8) - 6;
        for i in 1..tot_subbands {
            let mut pos = i;
            if pos >= self.js_start * 2 {
                pos -= self.js_start;
            } else {
                pos >>= 1;
            }
            let ipos = ((pos as i8) - 1).max(0).min(12);
            let cb = &codebooks.quant_cb[ipos as usize];
            self.qindex[i]                              = (br.read_cb(cb)? as i8) + self.qindex[i - 1] - 12;
            validate!((self.qindex[i] >= -63) && (self.qindex[i] <= 63));
        }
        let num_vectors                                 = br.read(self.vector_bits)? as usize;
        self.bitalloc(num_vectors, br.left() as usize);

        // coefficients
        self.block[channel] = [0.0; MAX_SAMPLES * 2];
        let mut off = 0;
        for sb in 0..tot_subbands {
            let mut coef_index: [u8; BAND_SIZE] = [0; BAND_SIZE];
            let mut coef_sign:  [bool; BAND_SIZE] = [false; BAND_SIZE];
            let cat = self.category[sb] as usize;
            if (cat < NUM_CATEGORIES - 1) && br.left() > 0 {
                unpack_band(&mut br, codebooks, &mut coef_index, &mut coef_sign, cat)?;
            }
            for i in 0..BAND_SIZE {
                let val;
                if coef_index[i] == 0 {
                    let v = COOK_DITHER_TAB[cat];
                    val = if !rnd.get_sign() { v } else { -v };
                } else {
                    let v = COOK_QUANT_CENTROID[cat][coef_index[i] as usize];
                    val = if !coef_sign[i] { v } else { -v };
                }
                self.block[channel][off + i] = val * dsp.hpow_tab[(self.qindex[sb] + 64) as usize];
            }
            off += BAND_SIZE;
        }

        Ok(())
    }
    fn decode(&mut self, dsp: &mut CookDSP, rnd: &mut RND, codebooks: &Codebooks, src: &[u8], buf: &mut [u8], abuf: &mut NABufferType) -> DecoderResult<()> {
        if self.mode == Mode::Stereo {
            let mut schunk = src.chunks(src.len() / 2);
            self.decode_channel_data(dsp, rnd, codebooks, schunk.next().unwrap(), buf, 0)?;
            self.decode_channel_data(dsp, rnd, codebooks, schunk.next().unwrap(), buf, 1)?;
        } else {
            self.decode_channel_data(dsp, rnd, codebooks, src, buf, 0)?;
        }
        // uncouple joint stereo channels
        if self.mode == Mode::JointStereo {
            for i in 0..self.js_start {
                for j in 0..BAND_SIZE {
                    self.block[1][i * BAND_SIZE + j] = self.block[0][(i * 2 + 1) * BAND_SIZE + j];
                    self.block[0][i * BAND_SIZE + j] = self.block[0][(i * 2)     * BAND_SIZE + j];
                }
            }
            let scale_idx = (self.js_bits as usize) - 2;
            let scale_off = (1 << self.js_bits) as usize;
            for i in self.js_start..self.subbands {
                let idx = self.decouple[COOK_CPL_BAND[i] as usize] as usize;
                let doff = i * BAND_SIZE;
                let soff = (i + self.js_start) * BAND_SIZE;
                let m1 = COOK_CPL_SCALES[scale_idx][            1 + idx];
                let m2 = COOK_CPL_SCALES[scale_idx][scale_off - 1 - idx];
                for j in 0..BAND_SIZE {
                    self.block[0][doff + j] = self.block[0][soff + j] * m1;
                    self.block[1][doff + j] = self.block[0][soff + j] * m2;
                }
            }
            for i in (self.subbands * BAND_SIZE)..MAX_SAMPLES {
                self.block[0][i] = 0.0;
                self.block[1][i] = 0.0;
            }
            self.gains[1] = self.gains[0];
            self.prev_gains[1] = self.prev_gains[0];
        }
        for ch in 0..self.mode.get_channels() {
            let off = abuf.get_offset(ch + self.start_ch);
            let mut adata = abuf.get_abuf_f32().unwrap();
            let output = adata.get_data_mut().unwrap();
            let dst = &mut output[off..];

            dsp.imdct.imdct(&self.block[ch], &mut dsp.out);

            let prev_gain = dsp.pow_tab[(self.prev_gains[ch][0] + 64) as usize];
            let mut cur_gain = 0.0;
            let mut cur_gain2 = 0.0;
            let mut gain_idx = 0;
            let eighthmask = (self.samples >> 3) - 1;
            for (i, out) in dst.iter_mut().take(self.samples).enumerate() {
                *out = dsp.out[i + self.samples] * prev_gain * dsp.window[i]
                       - self.delay[ch][i] * dsp.window[self.samples - i - 1];
                if (i & eighthmask) == 0 {
                    if (self.gains[ch][gain_idx] == 0) && (self.gains[ch][gain_idx + 1] == 0) {
                        cur_gain  = 1.0;
                        cur_gain2 = 1.0;
                    } else {
                        cur_gain  = dsp.pow_tab[(self.gains[ch][gain_idx] + 64) as usize];
                        cur_gain2 = dsp.gain_tab[(self.gains[ch][gain_idx + 1] - self.gains[ch][gain_idx] + 11) as usize];
                    }
                    gain_idx += 1;
                }
                *out *= cur_gain;
                cur_gain *= cur_gain2;
            }
            for i in 0..self.samples { self.delay[ch][i] = dsp.out[i]; }
        }
        Ok(())
    }
}

const COOK_VQ_GROUP_SIZE: [usize; 7] = [  2,  2,  2, 4, 4, 5, 5 ];
const COOK_NUM_VQ_GROUPS: [usize; 7] = [ 10, 10, 10, 5, 5, 4, 4 ];
const COOK_VQ_INV_RADIX: [u32; 7] = [ 74899, 104858, 149797, 209716, 262144, 349526, 524288 ];
const COOK_VQ_MULT: [u32; 7] = [ 13, 9, 6, 4, 3, 2, 1 ];
fn unpack_band(br: &mut BitReader, codebooks: &Codebooks, coef_index: &mut [u8; BAND_SIZE], coef_sign: &mut [bool; BAND_SIZE], cat: usize) -> DecoderResult<()> {
    let cb = &codebooks.vq_cb[cat];
    let group_size = COOK_VQ_GROUP_SIZE[cat];
    let mult = COOK_VQ_MULT[cat] + 1;
    for i in 0..COOK_NUM_VQ_GROUPS[cat] {
        let ret                                         = br.read_cb(cb);
        let mut val;
        if let Ok(v) = ret {
            val = v as u32;
        } else {
            let left = br.left() as u32;
            br.skip(left)?;
            break;
        }
        let mut nnz = 0;
        for j in (0..group_size).rev() {
            let t = (val * COOK_VQ_INV_RADIX[cat]) >> 20;
            coef_index[i * group_size + j] = (val - t * mult) as u8;
            if coef_index[i * group_size + j] != 0 {
                nnz += 1;
            }
            val = t;
        }
        if (br.left() as usize) < nnz {
            let left = br.left() as u32;
            br.skip(left)?;
            break;
        }
        for j in 0..group_size {
            if coef_index[i * group_size + j] != 0 {
                coef_sign[i * group_size + j]           = br.read_bool()?;
            } else {
                coef_sign[i * group_size + j] = false;
            }
        }
    }
    Ok(())
}

struct RND {
    state:  u32,
}

impl RND {
    fn new() -> Self {
        Self { state: 0xC0DECC00 }
    }
    fn get_sign(&mut self) -> bool {
        self.state = (self.state & 0xFFFF).wrapping_mul(36969).wrapping_add(self.state >> 16);
        (self.state & 0x10000) != 0
    }
}

struct CookDecoder {
    info:       NACodecInfoRef,
    chmap:      NAChannelMap,
    src:        [u8; 65536],
    num_pairs:  usize,
    pairs:      [CookChannelPair; MAX_PAIRS],
    channels:   usize,
    samples:    usize,
    codebooks:  Codebooks,
    rnd:        RND,
    dsp:        CookDSP,
}

impl CookDecoder {
    fn new() -> Self {
        CookDecoder {
            info:       NACodecInfo::new_dummy(),
            chmap:      NAChannelMap::new(),
            src:        [0; 65536],
            num_pairs:  0,
            channels:   0,
            samples:    0,
            pairs:      [CookChannelPair::new(); MAX_PAIRS],
            codebooks:  Codebooks::new(),
            rnd:        RND::new(),
            dsp:        CookDSP::new(1024),
        }
    }
}

impl NADecoder for CookDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(ainfo) = info.get_properties() {
            let edata = info.get_extradata().unwrap();
            validate!(edata.len() >= 4);

            let mut mr = MemoryReader::new_read(&edata);
            let mut br = ByteReader::new(&mut mr);
            let ver                                     = br.peek_u32be()?;

            let maj_ver = ver >> 24;
            let mut chmap: u32 = 0;
            match maj_ver {
                1 => {
                        self.num_pairs  = 1;
                        self.pairs[0].read_hdr_v1(&mut br)?;
                        self.channels = self.pairs[0].mode.get_channels();
                        if ainfo.get_channels() == 1 { // forced mono
                            self.pairs[0].mode = Mode::Mono;
                            self.channels       = 1;
                            chmap = 0x4;
                        } else {
                            chmap = 0x3;
                        }
                    },
                2 => {
                        self.num_pairs  = (edata.len() - (br.tell() as usize)) / 20;
                        validate!(self.num_pairs <= MAX_PAIRS);
                        let mut start_ch = 0;
                        for i in 0..self.num_pairs {
                            let pair_chmap = self.pairs[i].read_hdr_v2(&mut br)?;
                            self.pairs[i].start_ch = start_ch;
                            validate!((chmap & pair_chmap) == 0);
                            start_ch += self.pairs[i].mode.get_channels();
                        }
                        self.channels = start_ch;
                    },
                _ => { return Err(DecoderError::InvalidData); }
            };

            self.samples = self.pairs[0].samples / self.pairs[0].mode.get_channels();
            validate!((self.samples >= 16) && (self.samples <= 1024));
            if self.samples != self.dsp.size {
                self.dsp = CookDSP::new(self.samples);
            }
            self.chmap   = NAChannelMap::from_ms_mapping(chmap);

            for i in 1..self.num_pairs {
                validate!((self.pairs[i].samples / self.pairs[i].mode.get_channels()) == self.samples);
            }

            let vector_bits = match self.samples {
                    16 | 32 | 64 | 128 | 256 => 5,
                    512                      => 6,
                    1024                     => 7,
                    _                        => unreachable!(),
                };
            for pair in self.pairs.iter_mut() {
                match pair.mode {
                    Mode::Mono          => {
                            pair.vector_bits = 5;
                        },
                    Mode::Stereo        => {
                            pair.vector_bits = 5;
                            pair.samples >>= 1;
                        },
                    Mode::JointStereo   => {
                            pair.vector_bits = vector_bits;
                            pair.samples >>= 1;
                        },
                };
            }

            let ainfo = NAAudioInfo::new(ainfo.get_sample_rate(), self.channels as u8,
                                         SND_F32P_FORMAT, self.samples);
            self.info = info.replace_info(NACodecTypeInfo::Audio(ainfo));

            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        validate!(info.get_properties().is_audio());
        let pktbuf = pkt.get_buffer();
        validate!(pktbuf.len() > self.num_pairs * 2);

        let mut seg_size: [usize; MAX_PAIRS] = [0; MAX_PAIRS];
        let mut seg_start: [usize; MAX_PAIRS+1] = [0; MAX_PAIRS+1];

        let ainfo = self.info.get_properties().get_audio_info().unwrap();

        seg_size[0] = pktbuf.len() - (self.num_pairs - 1);
        for i in 1..self.num_pairs {
            seg_size[i] = (pktbuf[pktbuf.len() - self.num_pairs + i] as usize) * 2;
            validate!(seg_size[i] != 0);
            let ret = seg_size[0].checked_sub(seg_size[i]);
            if let Some(val) = ret {
                seg_size[0] = val;
            } else {
                return Err(DecoderError::InvalidData);
            }
        }
        validate!(seg_size[0] != 0);
        seg_start[0] = 0;
        for i in 0..self.num_pairs {
            seg_start[i + 1] = seg_start[i] + seg_size[i];
        }

        let mut abuf = alloc_audio_buffer(ainfo, self.samples, self.chmap.clone())?;

        for pair in 0..self.num_pairs {
            self.pairs[pair].decode(&mut self.dsp, &mut self.rnd, &self.codebooks, &pktbuf[seg_start[pair]..seg_start[pair + 1]], &mut self.src, &mut abuf)?;
        }

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.replace_info(NACodecTypeInfo::Audio(ainfo)), abuf);
        frm.set_keyframe(true);
        Ok(frm.into_ref())
    }
}

pub fn get_decoder() -> Box<dyn NADecoder> {
    Box::new(CookDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_core::test::dec_video::*;
    use crate::codecs::realmedia_register_all_codecs;
    use crate::demuxers::realmedia_register_all_demuxers;
    #[test]
    fn test_cook() {
        let mut dmx_reg = RegisteredDemuxers::new();
        realmedia_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        realmedia_register_all_codecs(&mut dec_reg);

//        let file = "assets/RV/rv30_weighted_mc.rm";
        let file = "assets/RV/multichannel.rma";
        test_decode_audio("realmedia", file, Some(2000), "cook", &dmx_reg, &dec_reg);
    }
}

const COOK_XOR_KEY: [u8; 4] = [ 0x37, 0xC5, 0x11, 0xF2 ];

const COOK_CPL_2BITS_BITS: &[u8; 3] = &[ 2, 1, 2 ];
const COOK_CPL_2BITS_CODES: &[u16; 3] = &[ 0x02, 0x00, 0x03 ];
const COOK_CPL_3BITS_BITS: &[u8; 7] = &[ 6, 5, 2, 1, 3, 4, 6 ];
const COOK_CPL_3BITS_CODES: &[u16; 7] = &[ 0x3e, 0x1e, 0x02, 0x00, 0x06, 0x0e, 0x3f ];
const COOK_CPL_4BITS_BITS: &[u8; 15] = &[ 8, 8, 7, 6, 5, 4, 3, 1, 3, 4, 5, 6, 7, 8, 8 ];
const COOK_CPL_4BITS_CODES: &[u16; 15] = &[
    0xfc, 0xfd, 0x7c, 0x3c, 0x1c, 0x0c, 0x04, 0x00,
    0x05, 0x0d, 0x1d, 0x3d, 0x7d, 0xfe, 0xff
];
const COOK_CPL_5BITS_BITS: &[u8; 31] = &[
    10, 10, 10, 10,  9,  9,  8,  8,  7,  7,  6,  6,  5,  5,  3,  1,
     3,  5,  5,  6,  6,  7,  7,  8,  8,  9,  9, 10, 10, 10, 10
];
const COOK_CPL_5BITS_CODES: &[u16; 31] = &[
    0x03F8, 0x03F9, 0x03FA, 0x03FB, 0x01F8, 0x01F9, 0x00F8, 0x00F9,
    0x0078, 0x0079, 0x0038, 0x0039, 0x0018, 0x0019, 0x0004, 0x0000,
    0x0005, 0x001A, 0x001B, 0x003A, 0x003B, 0x007A, 0x007B, 0x00FA,
    0x00FB, 0x01FA, 0x01FB, 0x03FC, 0x03FD, 0x03FE, 0x03FF
];
const COOK_CPL_6BITS_BITS: &[u8; 63] = &[
    16, 15, 14, 13, 12, 11, 11, 11, 11, 10, 10, 10, 10,  9,  9,  9,
     9,  9,  8,  8,  8,  8,  7,  7,  7,  7,  6,  6,  5,  5,  3,  1,
     4,  5,  5,  6,  6,  7,  7,  7,  7,  8,  8,  8,  8,  9,  9,  9,
     9, 10, 10, 10, 10, 10, 11, 11, 11, 11, 12, 13, 14, 14, 16
];
const COOK_CPL_6BITS_CODES: &[u16; 63] = &[
    0xFFFE, 0x7FFE, 0x3FFC, 0x1FFC, 0x0FFC, 0x07F6, 0x07F7, 0x07F8,
    0x07F9, 0x03F2, 0x03F3, 0x03F4, 0x03F5, 0x01F0, 0x01F1, 0x01F2,
    0x01F3, 0x01F4, 0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x0070, 0x0071,
    0x0072, 0x0073, 0x0034, 0x0035, 0x0016, 0x0017, 0x0004, 0x0000,
    0x000A, 0x0018, 0x0019, 0x0036, 0x0037, 0x0074, 0x0075, 0x0076,
    0x0077, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x01F5, 0x01F6, 0x01F7,
    0x01F8, 0x03F6, 0x03F7, 0x03F8, 0x03F9, 0x03FA, 0x07FA, 0x07FB,
    0x07FC, 0x07FD, 0x0FFD, 0x1FFD, 0x3FFD, 0x3FFE, 0xFFFF
];

const COOK_QUANT_BITS: [&[u8; 24]; 13] = [
    &[  4,  6,  5,  5,  4, 4, 4, 4, 4, 4, 3, 3, 3, 4, 5, 7,  8,  9, 11, 11, 12, 12, 12, 12 ],
    &[ 10,  8,  6,  5,  5, 4, 3, 3, 3, 3, 3, 3, 4, 5, 7, 9, 11, 12, 13, 15, 15, 15, 16, 16 ],
    &[ 12, 10,  8,  6,  5, 4, 4, 4, 4, 4, 4, 3, 3, 3, 4, 4,  5,  5,  7,  9, 11, 13, 14, 14 ],
    &[ 13, 10,  9,  9,  7, 7, 5, 5, 4, 3, 3, 3, 3, 3, 4, 4,  4,  5,  7,  9, 11, 13, 13, 13 ],
    &[ 12, 13, 10,  8,  6, 6, 5, 5, 4, 4, 3, 3, 3, 3, 3, 4,  5,  5,  6,  7,  9, 11, 14, 14 ],
    &[ 12, 11,  9,  8,  8, 7, 5, 4, 4, 3, 3, 3, 3, 3, 4, 4,  5,  5,  7,  8, 10, 13, 14, 14 ],
    &[ 15, 16, 15, 12, 10, 8, 6, 5, 4, 3, 3, 3, 2, 3, 4, 5,  5,  7,  9, 11, 13, 16, 16, 16 ],
    &[ 14, 14, 11, 10,  9, 7, 7, 5, 5, 4, 3, 3, 2, 3, 3, 4,  5,  7,  9,  9, 12, 14, 15, 15 ],
    &[  9,  9,  9,  8,  7, 6, 5, 4, 3, 3, 3, 3, 3, 3, 4, 5,  6,  7,  8, 10, 11, 12, 13, 13 ],
    &[ 14, 12, 10,  8,  6, 6, 5, 4, 3, 3, 3, 3, 3, 3, 4, 5,  6,  8,  8,  9, 11, 14, 14, 14 ],
    &[ 13, 10,  9,  8,  6, 6, 5, 4, 4, 4, 3, 3, 2, 3, 4, 5,  6,  8,  9,  9, 11, 12, 14, 14 ],
    &[ 16, 13, 12, 11,  9, 6, 5, 5, 4, 4, 4, 3, 2, 3, 3, 4,  5,  7,  8, 10, 14, 16, 16, 16 ],
    &[ 13, 14, 14, 14, 10, 8, 7, 7, 5, 4, 3, 3, 2, 3, 3, 4,  5,  5,  7,  9, 11, 14, 14, 14 ],
];
const COOK_QUANT_CODES: [&[u16; 24]; 13] = [
  &[ 0x0006, 0x003e, 0x001c, 0x001d, 0x0007, 0x0008, 0x0009, 0x000a, 0x000b, 0x000c, 0x0000, 0x0001,
     0x0002, 0x000d, 0x001e, 0x007e, 0x00fe, 0x01fe, 0x07fc, 0x07fd, 0x0ffc, 0x0ffd, 0x0ffe, 0x0fff ],
  &[ 0x03fe, 0x00fe, 0x003e, 0x001c, 0x001d, 0x000c, 0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005,
     0x000d, 0x001e, 0x007e, 0x01fe, 0x07fe, 0x0ffe, 0x1ffe, 0x7ffc, 0x7ffd, 0x7ffe, 0xfffe, 0xffff ],
  &[ 0x0ffe, 0x03fe, 0x00fe, 0x003e, 0x001c, 0x0006, 0x0007, 0x0008, 0x0009, 0x000a, 0x000b, 0x0000,
     0x0001, 0x0002, 0x000c, 0x000d, 0x001d, 0x001e, 0x007e, 0x01fe, 0x07fe, 0x1ffe, 0x3ffe, 0x3fff ],
  &[ 0x1ffc, 0x03fe, 0x01fc, 0x01fd, 0x007c, 0x007d, 0x001c, 0x001d, 0x000a, 0x0000, 0x0001, 0x0002,
     0x0003, 0x0004, 0x000b, 0x000c, 0x000d, 0x001e, 0x007e, 0x01fe, 0x07fe, 0x1ffd, 0x1ffe, 0x1fff ],
  &[ 0x0ffe, 0x1ffe, 0x03fe, 0x00fe, 0x003c, 0x003d, 0x001a, 0x001b, 0x000a, 0x000b, 0x0000, 0x0001,
     0x0002, 0x0003, 0x0004, 0x000c, 0x001c, 0x001d, 0x003e, 0x007e, 0x01fe, 0x07fe, 0x3ffe, 0x3fff ],
  &[ 0x0ffe, 0x07fe, 0x01fe, 0x00fc, 0x00fd, 0x007c, 0x001c, 0x000a, 0x000b, 0x0000, 0x0001, 0x0002,
     0x0003, 0x0004, 0x000c, 0x000d, 0x001d, 0x001e, 0x007d, 0x00fe, 0x03fe, 0x1ffe, 0x3ffe, 0x3fff ],
  &[ 0x7ffc, 0xfffc, 0x7ffd, 0x0ffe, 0x03fe, 0x00fe, 0x003e, 0x001c, 0x000c, 0x0002, 0x0003, 0x0004,
     0x0000, 0x0005, 0x000d, 0x001d, 0x001e, 0x007e, 0x01fe, 0x07fe, 0x1ffe, 0xfffd, 0xfffe, 0xffff ],
  &[ 0x3ffc, 0x3ffd, 0x07fe, 0x03fe, 0x01fc, 0x007c, 0x007d, 0x001c, 0x001d, 0x000c, 0x0002, 0x0003,
     0x0000, 0x0004, 0x0005, 0x000d, 0x001e, 0x007e, 0x01fd, 0x01fe, 0x0ffe, 0x3ffe, 0x7ffe, 0x7fff ],
  &[ 0x01fc, 0x01fd, 0x01fe, 0x00fc, 0x007c, 0x003c, 0x001c, 0x000c, 0x0000, 0x0001, 0x0002, 0x0003,
     0x0004, 0x0005, 0x000d, 0x001d, 0x003d, 0x007d, 0x00fd, 0x03fe, 0x07fe, 0x0ffe, 0x1ffe, 0x1fff ],
  &[ 0x3ffc, 0x0ffe, 0x03fe, 0x00fc, 0x003c, 0x003d, 0x001c, 0x000c, 0x0000, 0x0001, 0x0002, 0x0003,
     0x0004, 0x0005, 0x000d, 0x001d, 0x003e, 0x00fd, 0x00fe, 0x01fe, 0x07fe, 0x3ffd, 0x3ffe, 0x3fff ],
  &[ 0x1ffe, 0x03fe, 0x01fc, 0x00fc, 0x003c, 0x003d, 0x001c, 0x000a, 0x000b, 0x000c, 0x0002, 0x0003,
     0x0000, 0x0004, 0x000d, 0x001d, 0x003e, 0x00fd, 0x01fd, 0x01fe, 0x07fe, 0x0ffe, 0x3ffe, 0x3fff ],
  &[ 0xfffc, 0x1ffe, 0x0ffe, 0x07fe, 0x01fe, 0x003e, 0x001c, 0x001d, 0x000a, 0x000b, 0x000c, 0x0002,
     0x0000, 0x0003, 0x0004, 0x000d, 0x001e, 0x007e, 0x00fe, 0x03fe, 0x3ffe, 0xfffd, 0xfffe, 0xffff ],
  &[ 0x1ffc, 0x3ffa, 0x3ffb, 0x3ffc, 0x03fe, 0x00fe, 0x007c, 0x007d, 0x001c, 0x000c, 0x0002, 0x0003,
     0x0000, 0x0004, 0x0005, 0x000d, 0x001d, 0x001e, 0x007e, 0x01fe, 0x07fe, 0x3ffd, 0x3ffe, 0x3fff ],
];

const COOK_VQ0_BITS: &[u8; 191] = &[
    1, 4, 6, 6, 7, 7, 8, 8, 8, 9, 9, 10,
    11, 11, 4, 5, 6, 7, 7, 8, 8, 9, 9, 9,
    9, 10, 11, 11, 5, 6, 7, 8, 8, 9, 9, 9,
    9, 10, 10, 10, 11, 12, 6, 7, 8, 9, 9, 9,
    9, 10, 10, 10, 10, 11, 12, 13, 7, 7, 8, 9,
    9, 9, 10, 10, 10, 10, 11, 11, 12, 13, 8, 8,
    9, 9, 9, 10, 10, 10, 10, 11, 11, 12, 13, 14,
    8, 8, 9, 9, 10, 10, 11, 11, 11, 12, 12, 13,
    13, 15, 8, 8, 9, 9, 10, 10, 11, 11, 11, 12,
    12, 13, 14, 15, 9, 9, 9, 10, 10, 10, 11, 11,
    12, 13, 12, 14, 15, 16, 9, 9, 10, 10, 10, 10,
    11, 12, 12, 14, 14, 16, 16, 0, 9, 9, 10, 10,
    11, 11, 12, 13, 13, 14, 14, 15, 0, 0, 10, 10,
    10, 11, 11, 12, 12, 13, 15, 15, 16, 0, 0, 0,
    11, 11, 11, 12, 13, 13, 13, 15, 16, 16, 0, 0,
    0, 0, 11, 11, 12, 13, 13, 14, 15, 16, 16
];
const COOK_VQ0_CODES: &[u16; 191] = &[
    0x0000, 0x0008, 0x002c, 0x002d, 0x0062, 0x0063, 0x00d4, 0x00d5,
    0x00d6, 0x01c6, 0x01c7, 0x03ca, 0x07d6, 0x07d7, 0x0009, 0x0014,
    0x002e, 0x0064, 0x0065, 0x00d7, 0x00d8, 0x01c8, 0x01c9, 0x01ca,
    0x01cb, 0x03cb, 0x07d8, 0x07d9, 0x0015, 0x002f, 0x0066, 0x00d9,
    0x00da, 0x01cc, 0x01cd, 0x01ce, 0x01cf, 0x03cc, 0x03cd, 0x03ce,
    0x07da, 0x0fe4, 0x0030, 0x0067, 0x00db, 0x01d0, 0x01d1, 0x01d2,
    0x01d3, 0x03cf, 0x03d0, 0x03d1, 0x03d2, 0x07db, 0x0fe5, 0x1fea,
    0x0068, 0x0069, 0x00dc, 0x01d4, 0x01d5, 0x01d6, 0x03d3, 0x03d4,
    0x03d5, 0x03d6, 0x07dc, 0x07dd, 0x0fe6, 0x1feb, 0x00dd, 0x00de,
    0x01d7, 0x01d8, 0x01d9, 0x03d7, 0x03d8, 0x03d9, 0x03da, 0x07de,
    0x07df, 0x0fe7, 0x1fec, 0x3ff2, 0x00df, 0x00e0, 0x01da, 0x01db,
    0x03db, 0x03dc, 0x07e0, 0x07e1, 0x07e2, 0x0fe8, 0x0fe9, 0x1fed,
    0x1fee, 0x7ff4, 0x00e1, 0x00e2, 0x01dc, 0x01dd, 0x03dd, 0x03de,
    0x07e3, 0x07e4, 0x07e5, 0x0fea, 0x0feb, 0x1fef, 0x3ff3, 0x7ff5,
    0x01de, 0x01df, 0x01e0, 0x03df, 0x03e0, 0x03e1, 0x07e6, 0x07e7,
    0x0fec, 0x1ff0, 0x0fed, 0x3ff4, 0x7ff6, 0xfff8, 0x01e1, 0x01e2,
    0x03e2, 0x03e3, 0x03e4, 0x03e5, 0x07e8, 0x0fee, 0x0fef, 0x3ff5,
    0x3ff6, 0xfff9, 0xfffa, 0xfffa, 0x01e3, 0x01e4, 0x03e6, 0x03e7,
    0x07e9, 0x07ea, 0x0ff0, 0x1ff1, 0x1ff2, 0x3ff7, 0x3ff8, 0x7ff7,
    0x7ff7, 0xfffa, 0x03e8, 0x03e9, 0x03ea, 0x07eb, 0x07ec, 0x0ff1,
    0x0ff2, 0x1ff3, 0x7ff8, 0x7ff9, 0xfffb, 0x3ff8, 0x7ff7, 0x7ff7,
    0x07ed, 0x07ee, 0x07ef, 0x0ff3, 0x1ff4, 0x1ff5, 0x1ff6, 0x7ffa,
    0xfffc, 0xfffd, 0xfffb, 0xfffb, 0x3ff8, 0x7ff7, 0x07f0, 0x07f1,
    0x0ff4, 0x1ff7, 0x1ff8, 0x3ff9, 0x7ffb, 0xfffe, 0xffff
];
const COOK_VQ1_BITS: &[u8; 97] = &[
    1, 4, 5, 6, 7, 8, 8, 9, 10, 10, 4, 5,
    6, 7, 7, 8, 8, 9, 9, 11, 5, 5, 6, 7,
    8, 8, 9, 9, 10, 11, 6, 6, 7, 8, 8, 9,
    9, 10, 11, 12, 7, 7, 8, 8, 9, 9, 10, 11,
    11, 13, 8, 8, 8, 9, 9, 10, 10, 11, 12, 14,
    8, 8, 8, 9, 10, 11, 11, 12, 13, 15, 9, 9,
    9, 10, 11, 12, 12, 14, 14, 0, 9, 9, 9, 10,
    11, 12, 14, 16, 0, 0, 10, 10, 11, 12, 13, 14, 16
];
const COOK_VQ1_CODES: &[u16; 97] = &[
    0x0000, 0x0008, 0x0014, 0x0030, 0x006a, 0x00e2, 0x00e3, 0x01e4,
    0x03ec, 0x03ed, 0x0009, 0x0015, 0x0031, 0x006b, 0x006c, 0x00e4,
    0x00e5, 0x01e5, 0x01e6, 0x07f0, 0x0016, 0x0017, 0x0032, 0x006d,
    0x00e6, 0x00e7, 0x01e7, 0x01e8, 0x03ee, 0x07f1, 0x0033, 0x0034,
    0x006e, 0x00e8, 0x00e9, 0x01e9, 0x01ea, 0x03ef, 0x07f2, 0x0ff6,
    0x006f, 0x0070, 0x00ea, 0x00eb, 0x01eb, 0x01ec, 0x03f0, 0x07f3,
    0x07f4, 0x1ffa, 0x00ec, 0x00ed, 0x00ee, 0x01ed, 0x01ee, 0x03f1,
    0x03f2, 0x07f5, 0x0ff7, 0x3ffa, 0x00ef, 0x00f0, 0x00f1, 0x01ef,
    0x03f3, 0x07f6, 0x07f7, 0x0ff8, 0x1ffb, 0x7ffe, 0x01f0, 0x01f1,
    0x01f2, 0x03f4, 0x07f8, 0x0ff9, 0x0ffa, 0x3ffb, 0x3ffc, 0x0000,
    0x01f3, 0x01f4, 0x01f5, 0x03f5, 0x07f9, 0x0ffb, 0x3ffd, 0xfffe,
    0x0000, 0x0000, 0x03f6, 0x03f7, 0x07fa, 0x0ffc, 0x1ffc, 0x3ffe,
    0xffff
];
const COOK_VQ2_BITS: &[u8; 48] = &[
    1, 4, 5, 7, 8, 9, 10, 3, 4, 5, 7, 8,
    9, 10, 5, 5, 6, 7, 8, 10, 10, 7, 6, 7,
    8, 9, 10, 12, 8, 8, 8, 9, 10, 12, 14, 8,
    9, 9, 10, 11, 15, 16, 9, 10, 11, 12, 13, 16
];
const COOK_VQ2_CODES: &[u16; 48] = &[
    0x0000, 0x000a, 0x0018, 0x0074, 0x00f2, 0x01f4, 0x03f6, 0x0004, 0x000b, 0x0019, 0x0075, 0x00f3,
    0x01f5, 0x03f7, 0x001a, 0x001b, 0x0038, 0x0076, 0x00f4, 0x03f8, 0x03f9, 0x0077, 0x0039, 0x0078,
    0x00f5, 0x01f6, 0x03fa, 0x0ffc, 0x00f6, 0x00f7, 0x00f8, 0x01f7, 0x03fb, 0x0ffd, 0x3ffe, 0x00f9,
    0x01f8, 0x01f9, 0x03fc, 0x07fc, 0x7ffe, 0xfffe, 0x01fa, 0x03fd, 0x07fd, 0x0ffe, 0x1ffe, 0xffff
];
const COOK_VQ3_BITS: &[u8; 607] = &[
    2, 4, 6, 8, 10, 5, 5, 6, 8, 10, 7, 8,
    8, 10, 12, 9, 9, 10, 12, 15, 10, 11, 13, 16,
    16, 5, 6, 8, 10, 11, 5, 6, 8, 10, 12, 7,
    7, 8, 10, 13, 9, 9, 10, 12, 15, 12, 11, 13,
    16, 16, 7, 9, 10, 12, 15, 7, 8, 10, 12, 13,
    9, 9, 11, 13, 16, 11, 11, 12, 14, 16, 12, 12,
    14, 16, 0, 9, 11, 12, 16, 16, 9, 10, 13, 15,
    16, 10, 11, 12, 16, 16, 13, 13, 16, 16, 16, 16,
    16, 15, 16, 0, 11, 13, 16, 16, 15, 11, 13, 15,
    16, 16, 13, 13, 16, 16, 0, 14, 16, 16, 16, 0,
    16, 16, 0, 0, 0, 4, 6, 8, 10, 13, 6, 6,
    8, 10, 13, 9, 8, 10, 12, 16, 10, 10, 11, 15,
    16, 13, 12, 14, 16, 16, 5, 6, 8, 11, 13, 6,
    6, 8, 10, 13, 8, 8, 9, 11, 14, 10, 10, 12,
    12, 16, 13, 12, 13, 15, 16, 7, 8, 9, 12, 16,
    7, 8, 10, 12, 14, 9, 9, 10, 13, 16, 11, 10,
    12, 15, 16, 13, 13, 16, 16, 0, 9, 11, 13, 16,
    16, 9, 10, 12, 15, 16, 10, 11, 13, 16, 16, 13,
    12, 16, 16, 16, 16, 16, 16, 16, 0, 11, 13, 16,
    16, 16, 11, 13, 16, 16, 16, 12, 13, 15, 16, 0,
    16, 16, 16, 16, 0, 16, 16, 0, 0, 0, 6, 8,
    11, 13, 16, 8, 8, 10, 12, 16, 11, 10, 11, 13,
    16, 12, 13, 13, 15, 16, 16, 16, 14, 16, 0, 6,
    8, 10, 13, 16, 8, 8, 10, 12, 16, 10, 10, 11,
    13, 16, 13, 12, 13, 16, 16, 14, 14, 14, 16, 0,
    8, 9, 11, 13, 16, 8, 9, 11, 16, 14, 10, 10,
    12, 15, 16, 12, 12, 13, 16, 16, 15, 16, 16, 16,
    0, 10, 12, 15, 16, 16, 10, 12, 12, 14, 16, 12,
    12, 13, 16, 16, 14, 15, 16, 16, 0, 16, 16, 16,
    0, 0, 12, 15, 15, 16, 0, 13, 13, 16, 16, 0,
    14, 16, 16, 16, 0, 16, 16, 16, 0, 0, 0, 0,
    0, 0, 0, 8, 10, 13, 15, 16, 10, 11, 13, 16,
    16, 13, 13, 14, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 0, 8, 10, 11, 15, 16, 9, 10, 12,
    16, 16, 12, 12, 15, 16, 16, 16, 14, 16, 16, 16,
    16, 16, 16, 16, 0, 9, 11, 14, 16, 16, 10, 11,
    13, 16, 16, 14, 13, 14, 16, 16, 16, 15, 15, 16,
    0, 16, 16, 16, 0, 0, 11, 13, 16, 16, 16, 11,
    13, 15, 16, 16, 13, 16, 16, 16, 0, 16, 16, 16,
    16, 0, 16, 16, 0, 0, 0, 15, 16, 16, 16, 0,
    14, 16, 16, 16, 0, 16, 16, 16, 0, 0, 16, 16,
    0, 0, 0, 0, 0, 0, 0, 0, 9, 13, 16, 16,
    16, 11, 13, 16, 16, 16, 14, 15, 16, 16, 0, 15,
    16, 16, 16, 0, 16, 16, 0, 0, 0, 9, 13, 15,
    15, 16, 12, 13, 14, 16, 16, 16, 15, 16, 16, 0,
    16, 16, 16, 16, 0, 16, 16, 0, 0, 0, 11, 13,
    15, 16, 0, 12, 14, 16, 16, 0, 16, 16, 16, 16,
    0, 16, 16, 16, 0, 0, 0, 0, 0, 0, 0, 16,
    16, 16, 16, 0, 16, 16, 16, 16, 0, 16, 16, 16,
    0, 0, 16, 16, 0, 0, 0, 0, 0, 0, 0, 0,
    16, 16, 0, 0, 0, 16, 16
];
const COOK_VQ3_CODES: &[u16; 607] = &[
    0x0000, 0x0004, 0x0022, 0x00c6, 0x03b0, 0x000c, 0x000d, 0x0023, 0x00c7, 0x03b1, 0x005c, 0x00c8,
    0x00c9, 0x03b2, 0x0fa4, 0x01c2, 0x01c3, 0x03b3, 0x0fa5, 0x7f72, 0x03b4, 0x07b2, 0x1f9a, 0xff24,
    0xff25, 0x000e, 0x0024, 0x00ca, 0x03b5, 0x07b3, 0x000f, 0x0025, 0x00cb, 0x03b6, 0x0fa6, 0x005d,
    0x005e, 0x00cc, 0x03b7, 0x1f9b, 0x01c4, 0x01c5, 0x03b8, 0x0fa7, 0x7f73, 0x0fa8, 0x07b4, 0x1f9c,
    0xff26, 0xff27, 0x005f, 0x01c6, 0x03b9, 0x0fa9, 0x7f74, 0x0060, 0x00cd, 0x03ba, 0x0faa, 0x1f9d,
    0x01c7, 0x01c8, 0x07b5, 0x1f9e, 0xff28, 0x07b6, 0x07b7, 0x0fab, 0x3fa2, 0xff29, 0x0fac, 0x0fad,
    0x3fa3, 0xff2a, 0x3fa2, 0x01c9, 0x07b8, 0x0fae, 0xff2b, 0xff2c, 0x01ca, 0x03bb, 0x1f9f, 0x7f75,
    0xff2d, 0x03bc, 0x07b9, 0x0faf, 0xff2e, 0xff2f, 0x1fa0, 0x1fa1, 0xff30, 0xff31, 0xff32, 0xff33,
    0xff34, 0x7f76, 0xff35, 0xff31, 0x07ba, 0x1fa2, 0xff36, 0xff37, 0x7f77, 0x07bb, 0x1fa3, 0x7f78,
    0xff38, 0xff39, 0x1fa4, 0x1fa5, 0xff3a, 0xff3b, 0xff2e, 0x3fa4, 0xff3c, 0xff3d, 0xff3e, 0xff31,
    0xff3f, 0xff40, 0xff30, 0xff31, 0xff31, 0x0005, 0x0026, 0x00ce, 0x03bd, 0x1fa6, 0x0027, 0x0028,
    0x00cf, 0x03be, 0x1fa7, 0x01cb, 0x00d0, 0x03bf, 0x0fb0, 0xff41, 0x03c0, 0x03c1, 0x07bc, 0x7f79,
    0xff42, 0x1fa8, 0x0fb1, 0x3fa5, 0xff43, 0xff44, 0x0010, 0x0029, 0x00d1, 0x07bd, 0x1fa9, 0x002a,
    0x002b, 0x00d2, 0x03c2, 0x1faa, 0x00d3, 0x00d4, 0x01cc, 0x07be, 0x3fa6, 0x03c3, 0x03c4, 0x0fb2,
    0x0fb3, 0xff45, 0x1fab, 0x0fb4, 0x1fac, 0x7f7a, 0xff46, 0x0061, 0x00d5, 0x01cd, 0x0fb5, 0xff47,
    0x0062, 0x00d6, 0x03c5, 0x0fb6, 0x3fa7, 0x01ce, 0x01cf, 0x03c6, 0x1fad, 0xff48, 0x07bf, 0x03c7,
    0x0fb7, 0x7f7b, 0xff49, 0x1fae, 0x1faf, 0xff4a, 0xff4b, 0x7f7b, 0x01d0, 0x07c0, 0x1fb0, 0xff4c,
    0xff4d, 0x01d1, 0x03c8, 0x0fb8, 0x7f7c, 0xff4e, 0x03c9, 0x07c1, 0x1fb1, 0xff4f, 0xff50, 0x1fb2,
    0x0fb9, 0xff51, 0xff52, 0xff53, 0xff54, 0xff55, 0xff56, 0xff57, 0xff52, 0x07c2, 0x1fb3, 0xff58,
    0xff59, 0xff5a, 0x07c3, 0x1fb4, 0xff5b, 0xff5c, 0xff5d, 0x0fba, 0x1fb5, 0x7f7d, 0xff5e, 0xff4f,
    0xff5f, 0xff60, 0xff61, 0xff62, 0xff52, 0xff63, 0xff64, 0xff51, 0xff52, 0xff52, 0x002c, 0x00d7,
    0x07c4, 0x1fb6, 0xff65, 0x00d8, 0x00d9, 0x03ca, 0x0fbb, 0xff66, 0x07c5, 0x03cb, 0x07c6, 0x1fb7,
    0xff67, 0x0fbc, 0x1fb8, 0x1fb9, 0x7f7e, 0xff68, 0xff69, 0xff6a, 0x3fa8, 0xff6b, 0x7f7e, 0x002d,
    0x00da, 0x03cc, 0x1fba, 0xff6c, 0x00db, 0x00dc, 0x03cd, 0x0fbd, 0xff6d, 0x03ce, 0x03cf, 0x07c7,
    0x1fbb, 0xff6e, 0x1fbc, 0x0fbe, 0x1fbd, 0xff6f, 0xff70, 0x3fa9, 0x3faa, 0x3fab, 0xff71, 0xff6f,
    0x00dd, 0x01d2, 0x07c8, 0x1fbe, 0xff72, 0x00de, 0x01d3, 0x07c9, 0xff73, 0x3fac, 0x03d0, 0x03d1,
    0x0fbf, 0x7f7f, 0xff74, 0x0fc0, 0x0fc1, 0x1fbf, 0xff75, 0xff76, 0x7f80, 0xff77, 0xff78, 0xff79,
    0xff75, 0x03d2, 0x0fc2, 0x7f81, 0xff7a, 0xff7b, 0x03d3, 0x0fc3, 0x0fc4, 0x3fad, 0xff7c, 0x0fc5,
    0x0fc6, 0x1fc0, 0xff7d, 0xff7e, 0x3fae, 0x7f82, 0xff7f, 0xff80, 0xff80, 0xff81, 0xff82, 0xff83,
    0xff80, 0xff80, 0x0fc7, 0x7f83, 0x7f84, 0xff84, 0xff7a, 0x1fc1, 0x1fc2, 0xff85, 0xff86, 0x3fad,
    0x3faf, 0xff87, 0xff88, 0xff89, 0xff7d, 0xff8a, 0xff8b, 0xff8c, 0xff80, 0xff80, 0x3fae, 0x7f82,
    0xff7f, 0xff80, 0xff80, 0x00df, 0x03d4, 0x1fc3, 0x7f85, 0xff8d, 0x03d5, 0x07ca, 0x1fc4, 0xff8e,
    0xff8f, 0x1fc5, 0x1fc6, 0x3fb0, 0xff90, 0xff91, 0xff92, 0xff93, 0xff94, 0xff95, 0xff96, 0xff97,
    0xff98, 0xff99, 0xff9a, 0xff95, 0x00e0, 0x03d6, 0x07cb, 0x7f86, 0xff9b, 0x01d4, 0x03d7, 0x0fc8,
    0xff9c, 0xff9d, 0x0fc9, 0x0fca, 0x7f87, 0xff9e, 0xff9f, 0xffa0, 0x3fb1, 0xffa1, 0xffa2, 0xffa3,
    0xffa4, 0xffa5, 0xffa6, 0xffa7, 0xffa2, 0x01d5, 0x07cc, 0x3fb2, 0xffa8, 0xffa9, 0x03d8, 0x07cd,
    0x1fc7, 0xffaa, 0xffab, 0x3fb3, 0x1fc8, 0x3fb4, 0xffac, 0xffad, 0xffae, 0x7f88, 0x7f89, 0xffaf,
    0xffaf, 0xffb0, 0xffb1, 0xffb2, 0xffaf, 0xffaf, 0x07ce, 0x1fc9, 0xffb3, 0xffb4, 0xffb5, 0x07cf,
    0x1fca, 0x7f8a, 0xffb6, 0xffb7, 0x1fcb, 0xffb8, 0xffb9, 0xffba, 0xffba, 0xffbb, 0xffbc, 0xffbd,
    0xffbe, 0xffbe, 0xffbf, 0xffc0, 0xffbd, 0xffbe, 0xffbe, 0x7f8b, 0xffc1, 0xffc2, 0xffc3, 0xffb4,
    0x3fb5, 0xffc4, 0xffc5, 0xffc6, 0xffb6, 0xffc7, 0xffc8, 0xffc9, 0xffba, 0xffba, 0xffca, 0xffcb,
    0xffbd, 0xffbe, 0xffbe, 0xffbb, 0xffbc, 0xffbd, 0xffbe, 0xffbe, 0x01d6, 0x1fcc, 0xffcc, 0xffcd,
    0xffce, 0x07d0, 0x1fcd, 0xffcf, 0xffd0, 0xffd1, 0x3fb6, 0x7f8c, 0xffd2, 0xffd3, 0xff90, 0x7f8d,
    0xffd4, 0xffd5, 0xffd6, 0xff95, 0xffd7, 0xffd8, 0xff94, 0xff95, 0xff95, 0x01d7, 0x1fce, 0x7f8e,
    0x7f8f, 0xffd9, 0x0fcb, 0x1fcf, 0x3fb7, 0xffda, 0xffdb, 0xffdc, 0x7f90, 0xffdd, 0xffde, 0xff9e,
    0xffdf, 0xffe0, 0xffe1, 0xffe2, 0xffa2, 0xffe3, 0xffe4, 0xffa1, 0xffa2, 0xffa2, 0x07d1, 0x1fd0,
    0x7f91, 0xffe5, 0xffa8, 0x0fcc, 0x3fb8, 0xffe6, 0xffe7, 0xffaa, 0xffe8, 0xffe9, 0xffea, 0xffeb,
    0xffac, 0xffec, 0xffed, 0xffee, 0xffaf, 0xffaf, 0xffae, 0x7f88, 0x7f89, 0xffaf, 0xffaf, 0xffef,
    0xfff0, 0xfff1, 0xfff2, 0xffb4, 0xfff3, 0xfff4, 0xfff5, 0xfff6, 0xffb6, 0xfff7, 0xfff8, 0xfff9,
    0xffba, 0xffba, 0xfffa, 0xfffb, 0xffbd, 0xffbe, 0xffbe, 0xffbb, 0xffbc, 0xffbd, 0xffbe, 0xffbe,
    0xfffc, 0xfffd, 0xffb3, 0xffb4, 0xffb4, 0xfffe, 0xffff
];
const COOK_VQ4_BITS: &[u8; 246] = &[
    2, 4, 7, 10, 4, 5, 7, 10, 7, 8, 10, 14,
    11, 11, 15, 15, 4, 5, 9, 12, 5, 5, 8, 12,
    8, 7, 10, 15, 11, 11, 15, 15, 7, 9, 12, 15,
    8, 8, 12, 15, 10, 10, 13, 15, 14, 14, 15, 0,
    11, 13, 15, 15, 11, 13, 15, 15, 14, 15, 15, 0,
    15, 15, 0, 0, 4, 5, 9, 13, 5, 6, 9, 13,
    9, 9, 11, 15, 14, 13, 15, 15, 4, 6, 9, 12,
    5, 6, 9, 13, 9, 8, 11, 15, 13, 12, 15, 15,
    7, 9, 12, 15, 7, 8, 11, 15, 10, 10, 14, 15,
    14, 15, 15, 0, 10, 12, 15, 15, 11, 13, 15, 15,
    15, 15, 15, 0, 15, 15, 0, 0, 6, 9, 13, 14,
    8, 9, 12, 15, 12, 12, 15, 15, 15, 15, 15, 0,
    7, 9, 13, 15, 8, 9, 12, 15, 11, 12, 15, 15,
    15, 15, 15, 0, 9, 11, 15, 15, 9, 11, 15, 15,
    14, 14, 15, 0, 15, 15, 0, 0, 14, 15, 15, 0,
    14, 15, 15, 0, 15, 15, 0, 0, 0, 0, 0, 0,
    9, 12, 15, 15, 12, 13, 15, 15, 15, 15, 15, 0,
    15, 15, 0, 0, 10, 12, 15, 15, 12, 14, 15, 15,
    15, 15, 15, 0, 15, 15, 0, 0, 14, 15, 15, 0,
    15, 15, 15, 0, 15, 15, 0, 0, 0, 0, 0, 0,
    15, 15, 0, 0, 15, 15
];
const COOK_VQ4_CODES: &[u16; 246] = &[
    0x0000, 0x0004, 0x006c, 0x03e6, 0x0005, 0x0012, 0x006d, 0x03e7, 0x006e, 0x00e8, 0x03e8, 0x3fc4,
    0x07e0, 0x07e1, 0x7fa4, 0x7fa5, 0x0006, 0x0013, 0x01e2, 0x0fda, 0x0014, 0x0015, 0x00e9, 0x0fdb,
    0x00ea, 0x006f, 0x03e9, 0x7fa6, 0x07e2, 0x07e3, 0x7fa7, 0x7fa8, 0x0070, 0x01e3, 0x0fdc, 0x7fa9,
    0x00eb, 0x00ec, 0x0fdd, 0x7faa, 0x03ea, 0x03eb, 0x1fd6, 0x7fab, 0x3fc5, 0x3fc6, 0x7fac, 0x1fd6,
    0x07e4, 0x1fd7, 0x7fad, 0x7fae, 0x07e5, 0x1fd8, 0x7faf, 0x7fb0, 0x3fc7, 0x7fb1, 0x7fb2, 0x1fd6,
    0x7fb3, 0x7fb4, 0x1fd6, 0x1fd6, 0x0007, 0x0016, 0x01e4, 0x1fd9, 0x0017, 0x0032, 0x01e5, 0x1fda,
    0x01e6, 0x01e7, 0x07e6, 0x7fb5, 0x3fc8, 0x1fdb, 0x7fb6, 0x7fb7, 0x0008, 0x0033, 0x01e8, 0x0fde,
    0x0018, 0x0034, 0x01e9, 0x1fdc, 0x01ea, 0x00ed, 0x07e7, 0x7fb8, 0x1fdd, 0x0fdf, 0x7fb9, 0x7fba,
    0x0071, 0x01eb, 0x0fe0, 0x7fbb, 0x0072, 0x00ee, 0x07e8, 0x7fbc, 0x03ec, 0x03ed, 0x3fc9, 0x7fbd,
    0x3fca, 0x7fbe, 0x7fbf, 0x3fc9, 0x03ee, 0x0fe1, 0x7fc0, 0x7fc1, 0x07e9, 0x1fde, 0x7fc2, 0x7fc3,
    0x7fc4, 0x7fc5, 0x7fc6, 0x3fc9, 0x7fc7, 0x7fc8, 0x3fc9, 0x3fc9, 0x0035, 0x01ec, 0x1fdf, 0x3fcb,
    0x00ef, 0x01ed, 0x0fe2, 0x7fc9, 0x0fe3, 0x0fe4, 0x7fca, 0x7fcb, 0x7fcc, 0x7fcd, 0x7fce, 0x7fca,
    0x0073, 0x01ee, 0x1fe0, 0x7fcf, 0x00f0, 0x01ef, 0x0fe5, 0x7fd0, 0x07ea, 0x0fe6, 0x7fd1, 0x7fd2,
    0x7fd3, 0x7fd4, 0x7fd5, 0x7fd1, 0x01f0, 0x07eb, 0x7fd6, 0x7fd7, 0x01f1, 0x07ec, 0x7fd8, 0x7fd9,
    0x3fcc, 0x3fcd, 0x7fda, 0x7fda, 0x7fdb, 0x7fdc, 0x7fda, 0x7fda, 0x3fce, 0x7fdd, 0x7fde, 0x7fd6,
    0x3fcf, 0x7fdf, 0x7fe0, 0x7fd8, 0x7fe1, 0x7fe2, 0x7fda, 0x7fda, 0x3fcc, 0x3fcd, 0x7fda, 0x7fda,
    0x01f2, 0x0fe7, 0x7fe3, 0x7fe4, 0x0fe8, 0x1fe1, 0x7fe5, 0x7fe6, 0x7fe7, 0x7fe8, 0x7fe9, 0x7fca,
    0x7fea, 0x7feb, 0x7fca, 0x7fca, 0x03ef, 0x0fe9, 0x7fec, 0x7fed, 0x0fea, 0x3fd0, 0x7fee, 0x7fef,
    0x7ff0, 0x7ff1, 0x7ff2, 0x7fd1, 0x7ff3, 0x7ff4, 0x7fd1, 0x7fd1, 0x3fd1, 0x7ff5, 0x7ff6, 0x7fd6,
    0x7ff7, 0x7ff8, 0x7ff9, 0x7fd8, 0x7ffa, 0x7ffb, 0x7fda, 0x7fda, 0x3fcc, 0x3fcd, 0x7fda, 0x7fda,
    0x7ffc, 0x7ffd, 0x7fd6, 0x7fd6, 0x7ffe, 0x7fff
];
const COOK_VQ5_BITS: &[u8; 230] = &[
    2, 4, 8, 4, 5, 9, 9, 10, 14, 4, 6, 11,
    5, 6, 12, 10, 11, 15, 9, 11, 15, 10, 13, 15,
    14, 15, 0, 4, 6, 12, 6, 7, 12, 12, 12, 15,
    5, 7, 13, 6, 7, 13, 12, 13, 15, 10, 12, 15,
    11, 13, 15, 15, 15, 0, 8, 13, 15, 11, 12, 15,
    15, 15, 0, 10, 13, 15, 12, 15, 15, 15, 15, 0,
    15, 15, 0, 15, 15, 0, 0, 0, 0, 4, 5, 11,
    5, 7, 12, 11, 12, 15, 6, 7, 13, 7, 8, 14,
    12, 14, 15, 11, 13, 15, 12, 13, 15, 15, 15, 0,
    5, 6, 13, 7, 8, 15, 12, 14, 15, 6, 8, 14,
    7, 8, 15, 14, 15, 15, 12, 12, 15, 12, 13, 15,
    15, 15, 0, 9, 13, 15, 12, 13, 15, 15, 15, 0,
    11, 13, 15, 13, 13, 15, 15, 15, 0, 14, 15, 0,
    15, 15, 0, 0, 0, 0, 8, 10, 15, 11, 12, 15,
    15, 15, 0, 10, 12, 15, 12, 13, 15, 15, 15, 0,
    14, 15, 0, 15, 15, 0, 0, 0, 0, 8, 12, 15,
    12, 13, 15, 15, 15, 0, 11, 13, 15, 13, 15, 15,
    15, 15, 0, 15, 15, 0, 15, 15, 0, 0, 0, 0,
    14, 15, 0, 15, 15, 0, 0, 0, 0, 15, 15, 0,
    15, 15
];
const COOK_VQ5_CODES: &[u16; 230] = &[
    0x0000, 0x0004, 0x00f0, 0x0005, 0x0012, 0x01f0, 0x01f1, 0x03e8, 0x3fce, 0x0006, 0x0030, 0x07de,
    0x0013, 0x0031, 0x0fd2, 0x03e9, 0x07df, 0x7fb0, 0x01f2, 0x07e0, 0x7fb1, 0x03ea, 0x1fd2, 0x7fb2,
    0x3fcf, 0x7fb3, 0x0031, 0x0007, 0x0032, 0x0fd3, 0x0033, 0x0070, 0x0fd4, 0x0fd5, 0x0fd6, 0x7fb4,
    0x0014, 0x0071, 0x1fd3, 0x0034, 0x0072, 0x1fd4, 0x0fd7, 0x1fd5, 0x7fb5, 0x03eb, 0x0fd8, 0x7fb6,
    0x07e1, 0x1fd6, 0x7fb7, 0x7fb8, 0x7fb9, 0x0072, 0x00f1, 0x1fd7, 0x7fba, 0x07e2, 0x0fd9, 0x7fbb,
    0x7fbc, 0x7fbd, 0x0070, 0x03ec, 0x1fd8, 0x7fbe, 0x0fda, 0x7fbf, 0x7fc0, 0x7fc1, 0x7fc2, 0x0072,
    0x7fc3, 0x7fc4, 0x0071, 0x7fc5, 0x7fc6, 0x0072, 0x0034, 0x0072, 0x0072, 0x0008, 0x0015, 0x07e3,
    0x0016, 0x0073, 0x0fdb, 0x07e4, 0x0fdc, 0x7fc7, 0x0035, 0x0074, 0x1fd9, 0x0075, 0x00f2, 0x3fd0,
    0x0fdd, 0x3fd1, 0x7fc8, 0x07e5, 0x1fda, 0x7fc9, 0x0fde, 0x1fdb, 0x7fca, 0x7fcb, 0x7fcc, 0x00f2,
    0x0017, 0x0036, 0x1fdc, 0x0076, 0x00f3, 0x7fcd, 0x0fdf, 0x3fd2, 0x7fce, 0x0037, 0x00f4, 0x3fd3,
    0x0077, 0x00f5, 0x7fcf, 0x3fd4, 0x7fd0, 0x7fd1, 0x0fe0, 0x0fe1, 0x7fd2, 0x0fe2, 0x1fdd, 0x7fd3,
    0x7fd4, 0x7fd5, 0x00f5, 0x01f3, 0x1fde, 0x7fd6, 0x0fe3, 0x1fdf, 0x7fd7, 0x7fd8, 0x7fd9, 0x00f3,
    0x07e6, 0x1fe0, 0x7fda, 0x1fe1, 0x1fe2, 0x7fdb, 0x7fdc, 0x7fdd, 0x00f5, 0x3fd5, 0x7fde, 0x00f4,
    0x7fdf, 0x7fe0, 0x00f5, 0x0077, 0x00f5, 0x00f5, 0x00f6, 0x03ed, 0x7fe1, 0x07e7, 0x0fe4, 0x7fe2,
    0x7fe3, 0x7fe4, 0x0073, 0x03ee, 0x0fe5, 0x7fe5, 0x0fe6, 0x1fe3, 0x7fe6, 0x7fe7, 0x7fe8, 0x00f2,
    0x3fd6, 0x7fe9, 0x0074, 0x7fea, 0x7feb, 0x00f2, 0x0075, 0x00f2, 0x00f2, 0x00f7, 0x0fe7, 0x7fec,
    0x0fe8, 0x1fe4, 0x7fed, 0x7fee, 0x7fef, 0x00f3, 0x07e8, 0x1fe5, 0x7ff0, 0x1fe6, 0x7ff1, 0x7ff2,
    0x7ff3, 0x7ff4, 0x00f5, 0x7ff5, 0x7ff6, 0x00f4, 0x7ff7, 0x7ff8, 0x00f5, 0x0077, 0x00f5, 0x00f5,
    0x3fd7, 0x7ff9, 0x0036, 0x7ffa, 0x7ffb, 0x00f3, 0x0076, 0x00f3, 0x00f3, 0x7ffc, 0x7ffd, 0x0000,
    0x7ffe, 0x7fff
];
const COOK_VQ6_BITS: &[u8; 32] = &[
     1,  4,  4,  6,  4,  6,  6,  8,  4,  6,  6,  8,
     6,  9,  8, 10,  4,  6,  7,  8,  6,  9,  8, 11,
     6,  9,  8, 10,  8, 10,  9, 11
];
const COOK_VQ6_CODES: &[u16; 32] = &[
    0x0000, 0x0008, 0x0009, 0x0034, 0x000a, 0x0035, 0x0036, 0x00f6,
    0x000b, 0x0037, 0x0038, 0x00f7, 0x0039, 0x01fa, 0x00f8, 0x03fc,
    0x000c, 0x003a, 0x007a, 0x00f9, 0x003b, 0x01fb, 0x00fa, 0x07fe,
    0x003c, 0x01fc, 0x00fb, 0x03fd, 0x00fc, 0x03fe, 0x01fd, 0x07ff
];

const COOK_CPL_SCALE2: &[f32; 5] = &[
    1.0, 0.953020632266998, 0.70710676908493, 0.302905440330505, 0.0
];
const COOK_CPL_SCALE3: &[f32; 9] = &[
    1.0, 0.981279790401459, 0.936997592449188, 0.875934481620789, 0.70710676908493,
    0.482430040836334, 0.349335819482803, 0.192587479948997, 0.0
];
const COOK_CPL_SCALE4: &[f32; 17] = &[
    1.0, 0.991486728191376, 0.973249018192291, 0.953020632266998, 0.930133521556854,
    0.903453230857849, 0.870746195316315, 0.826180458068848, 0.70710676908493,
    0.563405573368073, 0.491732746362686, 0.428686618804932, 0.367221474647522,
    0.302905440330505, 0.229752898216248, 0.130207896232605, 0.0
];
const COOK_CPL_SCALE5: &[f32; 33] = &[
    1.0, 0.995926380157471, 0.987517595291138, 0.978726446628571, 0.969505727291107,
    0.95979779958725, 0.949531257152557, 0.938616216182709, 0.926936149597168,
    0.914336204528809, 0.900602877140045, 0.885426938533783, 0.868331849575043,
    0.84851086139679, 0.824381768703461, 0.791833400726318, 0.70710676908493,
    0.610737144947052, 0.566034197807312, 0.529177963733673, 0.495983630418777,
    0.464778542518616, 0.434642940759659, 0.404955863952637, 0.375219136476517,
    0.344963222742081, 0.313672333955765, 0.280692428350449, 0.245068684220314,
    0.205169528722763, 0.157508864998817, 0.0901700109243393, 0.0
];
const COOK_CPL_SCALE6: &[f32; 65] = &[
    1.0, 0.998005926609039, 0.993956744670868, 0.989822506904602, 0.985598564147949,
    0.981279790401459, 0.976860702037811, 0.972335040569305, 0.967696130275726,
    0.962936460971832, 0.958047747612000, 0.953020632266998, 0.947844684123993,
    0.942508161067963, 0.936997592449188, 0.931297719478607, 0.925390899181366,
    0.919256627559662, 0.912870943546295, 0.906205296516418, 0.899225592613220,
    0.891890347003937, 0.884148240089417, 0.875934481620789, 0.867165684700012,
    0.857730865478516, 0.847477376461029, 0.836184680461884, 0.823513329029083,
    0.808890223503113, 0.791194140911102, 0.767520070075989, 0.707106769084930,
    0.641024887561798, 0.611565053462982, 0.587959706783295, 0.567296981811523,
    0.548448026180267, 0.530831515789032, 0.514098942279816, 0.498019754886627,
    0.482430040836334, 0.467206478118896, 0.452251672744751, 0.437485188245773,
    0.422837972640991, 0.408248275518417, 0.393658757209778, 0.379014074802399,
    0.364258885383606, 0.349335819482803, 0.334183186292648, 0.318732559680939,
    0.302905440330505, 0.286608695983887, 0.269728302955627, 0.252119421958923,
    0.233590632677078, 0.213876649737358, 0.192587479948997, 0.169101938605309,
    0.142307326197624, 0.109772264957428, 0.0631198287010193, 0.0
];
const COOK_CPL_SCALES: [&[f32]; 5] = [
    COOK_CPL_SCALE2, COOK_CPL_SCALE3, COOK_CPL_SCALE4, COOK_CPL_SCALE5, COOK_CPL_SCALE6
];

const COOK_CPL_BAND: [u8; MAX_SUBBANDS - 1] = [
     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 11, 12, 12, 13,
    13, 14, 14, 14, 15, 15, 15, 15, 16, 16, 16, 16, 16, 17, 17, 17,
    17, 17, 17, 18, 18, 18, 18, 18, 18, 18, 19, 19, 19, 19, 19, 19,
    19, 19, 19
];

#[allow(clippy::approx_constant)]
const COOK_DITHER_TAB: [f32; 9] = [ 0.0, 0.0, 0.0, 0.0, 0.0, 0.176777, 0.25, 0.707107, 1.0 ];

const COOK_QUANT_CENTROID: [[f32; 14]; 7] = [
  [ 0.000, 0.392, 0.761, 1.120, 1.477, 1.832, 2.183, 2.541, 2.893, 3.245, 3.598, 3.942, 4.288, 4.724 ],
  [ 0.000, 0.544, 1.060, 1.563, 2.068, 2.571, 3.072, 3.562, 4.070, 4.620, 0.000, 0.000, 0.000, 0.000 ],
  [ 0.000, 0.746, 1.464, 2.180, 2.882, 3.584, 4.316, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000 ],
  [ 0.000, 1.006, 2.000, 2.993, 3.985, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000 ],
  [ 0.000, 1.321, 2.703, 3.983, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000 ],
  [ 0.000, 1.657, 3.491, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000 ],
  [ 0.000, 1.964, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000, 0.000 ]
];

const COOK_EXP_BITS: [i32; NUM_CATEGORIES] = [ 52, 47, 43, 37, 29, 22, 16, 0 ];
