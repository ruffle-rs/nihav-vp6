//! Deflate format (RFC 1951) support.
//!
//! This module provides functionality for decompressing raw deflated streams via [`Inflate`] and gzip files (RFC 1952) via [`gzip_decode`].
//!
//! [`Inflate`]: ./struct.Inflate.html
//! [`gzip_decode`]: ./fn.gzip_decode.html
//!
//! # Examples
//!
//! Decompressing full input buffer into sufficiently large output buffer:
//! ```
//! # use nihav_core::compr::DecompressError;
//! use nihav_core::compr::deflate::Inflate;
//!
//! # fn decompress(input: &[u8]) -> Result<(), DecompressError> {
//! # let mut output_buffer = [0u8; 16];
//! let output_length = Inflate::uncompress(input, &mut output_buffer)?;
//! # Ok(())
//! # }
//! ```
//!
//! Decompressing input chunks into portions of output:
//! ```
//! use nihav_core::compr::DecompressError;
//! use nihav_core::compr::deflate::Inflate;
//!
//! # fn decompress(input_data: &[u8]) -> Result<(), DecompressError> {
//! let mut inflate = Inflate::new();
//! let mut dst_buf: Vec<u8> = Vec::new();
//! let mut output_chunk = [0u8; 1024];
//! for src in input_data.chunks(512) {
//!     let mut repeat = false;
//!     loop {
//!         let ret = inflate.decompress_data(src, &mut output_chunk, repeat);
//!         match ret {
//!             Ok(len) => { // we got a buffer decoded successfully to the end
//!                 dst_buf.extend_from_slice(&output_chunk[..len]);
//!                 break;
//!             },
//!             Err(DecompressError::ShortData) => { // this block of data was fully read
//!                 break;
//!             },
//!             Err(DecompressError::OutputFull) => {
//!                 // the output buffer is full, flush it and continue decoding the same block
//!                 repeat = true;
//!                 dst_buf.extend_from_slice(&output_chunk);
//!             },
//!             Err(err) => {
//!                 return Err(err);
//!             },
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::io::byteio::*;
use crate::io::bitreader::*;
use crate::io::codebook::*;
use super::*;

const NUM_LITERALS: usize = 287;
const NUM_DISTS:    usize = 32;

struct FixedLenCodeReader {}

impl CodebookDescReader<u16> for FixedLenCodeReader {
    fn bits(&mut self, idx: usize) -> u8  {
        if      idx < 144 { 8 }
        else if idx < 256 { 9 }
        else if idx < 280 { 7 }
        else              { 8 }
    }
    #[allow(clippy::identity_op)]
    fn code(&mut self, idx: usize) -> u32 {
        let base = idx as u32;
        let bits = self.bits(idx);
        if      idx < 144 { reverse_bits(base + 0x30, bits) }
        else if idx < 256 { reverse_bits(base + 0x190 - 144, bits) }
        else if idx < 280 { reverse_bits(base + 0x000 - 256, bits) }
        else              { reverse_bits(base + 0xC0 - 280, bits) }
    }
    fn sym (&mut self, idx: usize) -> u16 { idx as u16 }
    fn len(&mut self) -> usize { NUM_LITERALS + 1 }
}

#[derive(Clone,Copy,Default)]
struct BitReaderState {
    pos:            usize,
    bitbuf:         u32,
    bits:           u8,
}

struct CurrentSource<'a> {
    src:            &'a [u8],
    br:             BitReaderState,
}

impl<'a> CurrentSource<'a> {
    fn new(src: &'a [u8], br: BitReaderState) -> Self {
        let mut newsrc = Self { src, br };
        newsrc.br.pos = 0;
        newsrc.refill();
        newsrc
    }
    fn reinit(src: &'a [u8], br: BitReaderState) -> Self {
        let mut newsrc = Self { src, br };
        newsrc.refill();
        newsrc
    }
    fn refill(&mut self) {
        while (self.br.bits <= 24) && (self.br.pos < self.src.len()) {
            self.br.bitbuf |= u32::from(self.src[self.br.pos]) << self.br.bits;
            self.br.bits += 8;
            self.br.pos += 1;
        }
    }
    fn skip_cache(&mut self, nbits: u8) {
        self.br.bitbuf >>= nbits;
        self.br.bits    -= nbits;
    }
    fn read(&mut self, nbits: u8) -> BitReaderResult<u32> {
        if nbits == 0 { return Ok(0); }
        if nbits > 16 { return Err(BitReaderError::TooManyBitsRequested); }
        if self.br.bits < nbits {
            self.refill();
            if self.br.bits < nbits { return Err(BitReaderError::BitstreamEnd); }
        }
        let ret = self.br.bitbuf & ((1 << nbits) - 1);
        self.skip_cache(nbits);
        Ok(ret)
    }
    fn read_bool(&mut self) -> BitReaderResult<bool> {
        if self.br.bits == 0 {
            self.refill();
            if self.br.bits == 0 { return Err(BitReaderError::BitstreamEnd); }
        }
        let ret = (self.br.bitbuf & 1) != 0;
        self.skip_cache(1);
        Ok(ret)
    }
    fn peek(&mut self, nbits: u8) -> u32 {
        if nbits == 0 || nbits > 16 { return 0; }
        if self.br.bits < nbits {
            self.refill();
        }
        self.br.bitbuf & ((1 << nbits) - 1)
    }
    fn skip(&mut self, nbits: u32) -> BitReaderResult<()> {
        if u32::from(self.br.bits) >= nbits {
            self.skip_cache(nbits as u8);
        } else {
            unreachable!();
        }
        Ok(())
    }
    fn align(&mut self) {
        let b = self.br.bits & 7;
        if b != 0 {
            self.skip_cache(8 - (b as u8));
        }
    }
    fn left(&self) -> isize {
        ((self.src.len() as isize) - (self.br.pos as isize)) * 8 + (self.br.bits as isize)
    }
}

impl<'a, S: Copy> CodebookReader<S> for CurrentSource<'a> {
    fn read_cb(&mut self, cb: &Codebook<S>) -> CodebookResult<S> {
        let mut esc = true;
        let mut idx = 0;
        let mut lut_bits = cb.lut_bits;
        let orig_br = self.br;
        while esc {
            let lut_idx = (self.peek(lut_bits) as usize) + (idx as usize);
            if cb.table[lut_idx] == TABLE_FILL_VALUE { return Err(CodebookError::InvalidCode); }
            let bits = cb.table[lut_idx] & 0x7F;
            esc  = (cb.table[lut_idx] & 0x80) != 0;
            idx  = (cb.table[lut_idx] >> 8) as usize;
            let skip_bits = if esc { u32::from(lut_bits) } else { bits };
            if (skip_bits as isize) > self.left() {
                self.br = orig_br;
                self.refill();
                return Err(CodebookError::MemoryError);
            }
            self.skip(skip_bits as u32).unwrap();
            lut_bits = bits as u8;
        }
        Ok(cb.syms[idx])
    }
}

enum InflateState {
    Start,
    BlockStart,
    BlockMode,
    StaticBlockLen,
    StaticBlockInvLen(u32),
    StaticBlockCopy(usize),
    FixedBlock,
    FixedBlockLengthExt(usize, u8),
    FixedBlockDist(usize),
    FixedBlockDistExt(usize, usize, u8),
    FixedBlockCopy(usize, usize),
    FixedBlockLiteral(u8),
    DynBlockHlit,
    DynBlockHdist,
    DynBlockHclen,
    DynLengths(usize),
    DynCodeLengths,
    DynCodeLengthsAdd(usize),
    DynBlock,
    DynBlockLengthExt(usize, u8),
    DynBlockDist(usize),
    DynBlockDistExt(usize, usize, u8),
    DynCopy(usize, usize),
    DynBlockLiteral(u8),
    End,
}

///! The decompressor for deflated streams (RFC 1951).
pub struct Inflate {
    br:             BitReaderState,
    fix_len_cb:     Codebook<u16>,

    buf:            [u8; 65536],
    bpos:           usize,
    output_idx:     usize,
    full_pos:       usize,

    state:          InflateState,
    final_block:    bool,
    hlit:           usize,
    hdist:          usize,
    dyn_len_cb:     Option<Codebook<u32>>,
    dyn_lit_cb:     Option<Codebook<u32>>,
    dyn_dist_cb:    Option<Codebook<u32>>,
    len_lengths:    [u8; 19],
    all_lengths:    [u8; NUM_LITERALS + NUM_DISTS],
    cur_len_idx:    usize,
}

const LENGTH_ADD_BITS: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1,
    1, 1, 2, 2, 2, 2, 3, 3, 3, 3,
    4, 4, 4, 4, 5, 5, 5, 5, 0
];
const LENGTH_BASE: [u16; 29] = [
     3,   4,   5,   6,   7,   8,   9,  10,  11,  13,
    15,  17,  19,  23,  27,  31,  35,  43,  51,  59,
    67,  83,  99, 115, 131, 163, 195, 227, 258
];
const DIST_ADD_BITS: [u8; 30] = [
    0,  0,  0,  0,  1,  1,  2,  2,  3,  3,
    4,  4,  5,  5,  6,  6,  7,  7,  8,  8,
    9,  9, 10, 10, 11, 11, 12, 12, 13, 13
];
const DIST_BASE: [u16; 30] = [
       1,    2,    3,    4,    5,    7,    9,    13,    17,    25,
      33,   49,   65,   97,  129,  193,  257,   385,   513,   769,
    1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577
];
const LEN_RECODE: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
];
const REPEAT_BITS: [u8; 3] = [ 2, 3, 7 ];
const REPEAT_BASE: [u8; 3] = [ 3, 3, 11 ];

macro_rules! read_bits {
    ($self: expr, $csrc: expr, $bits: expr) => ({
        if $csrc.left() < $bits as isize {
            $self.br = $csrc.br;
            return Err(DecompressError::ShortData);
        }
        $csrc.read($bits).unwrap()
    })
}

macro_rules! read_cb {
    ($self: expr, $csrc: expr, $cb: expr) => ({
        let ret = $csrc.read_cb($cb);
        if let Err(CodebookError::MemoryError) = ret {
            $self.br = $csrc.br;
            return Err(DecompressError::ShortData);
        }
        match ret {
            Ok(val) => val,
            Err(_)  => {
                $self.state = InflateState::End;
                return Err(DecompressError::InvalidData);
            },
        }
    })
}

impl Inflate {
    ///! Creates a new instance of `Inflate` struct.
    pub fn new() -> Self {
        let mut cr = FixedLenCodeReader {};
        let fix_len_cb = Codebook::new(&mut cr, CodebookMode::LSB).unwrap();
        Self {
            br:             BitReaderState::default(),
            fix_len_cb,

            buf:            [0; 65536],
            bpos:           0,
            output_idx:     0,
            full_pos:       0,

            state:          InflateState::Start,
            final_block:    false,
            dyn_len_cb:     None,
            dyn_lit_cb:     None,
            dyn_dist_cb:    None,
            hlit:           0,
            hdist:          0,
            len_lengths:    [0; 19],
            all_lengths:    [0; NUM_LITERALS + NUM_DISTS],
            cur_len_idx:    0,
        }
    }
    fn put_literal(&mut self, val: u8) {
        self.buf[self.bpos] = val;
        self.bpos = (self.bpos + 1) & (self.buf.len() - 1);
        self.full_pos += 1;
    }
    fn lz_copy(&mut self, offset: usize, len: usize, dst: &mut [u8]) -> DecompressResult<()> {
        let mask = self.buf.len() - 1;
        if offset > self.full_pos {
            return Err(DecompressError::InvalidData);
        }
        let cstart = (self.bpos.wrapping_sub(offset)) & mask;
        for i in 0..len {
            self.buf[(self.bpos + i) & mask] = self.buf[(cstart + i) & mask];
            dst[i] = self.buf[(cstart + i) & mask];
        }
        self.bpos = (self.bpos + len) & mask;
        self.full_pos += len;
        Ok(())
    }
    ///! Reports whether decoder has finished decoding the input.
    pub fn is_finished(&self) -> bool {
        match self.state {
            InflateState::End => true,
            _ => false,
        }
    }
    ///! Reports the current amount of bytes output into the destination buffer after the last run.
    pub fn get_current_output_size(&self) -> usize { self.output_idx }
    ///! Reports the total amount of bytes decoded so far.
    pub fn get_total_output_size(&self) -> usize { self.bpos }
    ///! Tries to decompress input data and write it to the output buffer.
    ///!
    ///! Since the decompressor can work with arbitrary input and output chunks its return value may have several meanings:
    ///! * `Ok(len)` means the stream has been fully decoded and then number of bytes output into the destination buffer is returned.
    ///! * [`DecompressError::ShortData`] means the input stream has been fully read but more data is needed.
    ///! * [`DecompressError::OutputFull`] means the output buffer is full and should be flushed. Then decoding should continue on the same input block with `continue_block` parameter set to `true`.
    ///!
    ///! [`DecompressError::ShortData`]: ../enum.DecompressError.html#variant.ShortData
    ///! [`DecompressError::OutputFull`]: ../enum.DecompressError.html#variant.OutputFull
    pub fn decompress_data(&mut self, src: &[u8], dst: &mut [u8], continue_block: bool) -> DecompressResult<usize> {
        if src.is_empty() || dst.is_empty() {
            return Err(DecompressError::InvalidArgument);
        }
        let mut csrc = if !continue_block {
                CurrentSource::new(src, self.br)
            } else {
                self.output_idx = 0;
                CurrentSource::reinit(src, self.br)
            };
        'main: loop {
            match self.state {
                InflateState::Start | InflateState::BlockStart => {
                    if csrc.left() == 0 {
                        self.br = csrc.br;
                        return Err(DecompressError::ShortData);
                    }
                    self.final_block = csrc.read_bool().unwrap();
                    self.state = InflateState::BlockMode;
                },
                InflateState::BlockMode => {
                    let bmode = read_bits!(self, csrc, 2);
                    match bmode {
                        0 => {
                            csrc.align();
                            self.state = InflateState::StaticBlockLen;
                        },
                        1 => { self.state = InflateState::FixedBlock; },
                        2 => { self.state = InflateState::DynBlockHlit; },
                        _ => {
                            self.state = InflateState::End;
                            return Err(DecompressError::InvalidHeader);
                        },
                    };
                },
                InflateState::StaticBlockLen => {
                    let len = read_bits!(self, csrc, 16);
                    self.state = InflateState::StaticBlockInvLen(len);
                },
                InflateState::StaticBlockInvLen(len) => {
                    let inv_len = read_bits!(self, csrc, 16);
                    if len != !inv_len {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidHeader);
                    }
                    self.state = InflateState::StaticBlockCopy(len as usize);
                },
                InflateState::StaticBlockCopy(len) => {
                    for i in 0..len {
                        if csrc.left() < 8 {
                            self.br = csrc.br;
                            self.state = InflateState::StaticBlockCopy(len - i);
                            return Err(DecompressError::ShortData);
                        }
                        let val = csrc.read(8).unwrap() as u8;
                        self.put_literal(val);
                    }
                    self.state = InflateState::BlockStart;
                }
                InflateState::FixedBlock => {
                    let val = read_cb!(self, csrc, &self.fix_len_cb);
                    if val < 256 {
                        if self.output_idx >= dst.len() {
                            self.br = csrc.br;
                            self.state = InflateState::FixedBlockLiteral(val as u8);
                            return Err(DecompressError::OutputFull);
                        }
                        self.put_literal(val as u8);
                        dst[self.output_idx] = val as u8;
                        self.output_idx += 1;
                    } else if val == 256 {
                        if self.final_block {
                            self.state = InflateState::End;
                            return Ok(self.output_idx);
                        } else {
                            self.state = InflateState::BlockStart;
                        }
                    } else {
                        let len_idx = (val - 257) as usize;
                        if len_idx >= LENGTH_BASE.len() {
                            self.state = InflateState::End;
                            return Err(DecompressError::InvalidData);
                        }
                        let len_bits = LENGTH_ADD_BITS[len_idx];
                        let add_base = LENGTH_BASE[len_idx] as usize;
                        if len_bits > 0 {
                            self.state = InflateState::FixedBlockLengthExt(add_base, len_bits);
                        } else {
                            self.state = InflateState::FixedBlockDist(add_base);
                        }
                    }
                },
                InflateState::FixedBlockLiteral(sym) => {
                    if self.output_idx >= dst.len() {
                        self.br = csrc.br;
                        return Err(DecompressError::OutputFull);
                    }
                    self.put_literal(sym);
                    dst[self.output_idx] = sym;
                    self.output_idx += 1;
                    self.state = InflateState::FixedBlock;
                },
                InflateState::FixedBlockLengthExt(base, bits) => {
                    let add = read_bits!(self, csrc, bits) as usize;
                    self.state = InflateState::FixedBlockDist(base + add);
                },
                InflateState::FixedBlockDist(length) => {
                    let dist_idx = reverse_bits(read_bits!(self, csrc, 5), 5) as usize;
                    if dist_idx >= DIST_BASE.len() {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidData);
                    }
                    let dist_bits = DIST_ADD_BITS[dist_idx];
                    let dist_base = DIST_BASE[dist_idx] as usize;
                    if dist_bits == 0 {
                        self.state = InflateState::FixedBlockCopy(length, dist_base);
                    } else {
                        self.state = InflateState::FixedBlockDistExt(length, dist_base, dist_bits);
                    }
                },
                InflateState::FixedBlockDistExt(length, base, bits) => {
                    let add = read_bits!(self, csrc, bits) as usize;
                    self.state = InflateState::FixedBlockCopy(length, base + add);
                },
                InflateState::FixedBlockCopy(length, dist) => {
                    if self.output_idx + length > dst.len() {
                        let copy_size = dst.len() - self.output_idx;
                        let ret = self.lz_copy(dist, copy_size, &mut dst[self.output_idx..]);
                        if ret.is_err() {
                            self.state = InflateState::End;
                            return Err(DecompressError::InvalidData);
                        }
                        self.output_idx += copy_size;
                        self.br = csrc.br;
                        self.state = InflateState::FixedBlockCopy(length - copy_size, dist);
                        return Err(DecompressError::OutputFull);
                    }
                    let ret = self.lz_copy(dist, length, &mut dst[self.output_idx..]);
                    if ret.is_err() {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidData);
                    }
                    self.output_idx += length;
                    self.state = InflateState::FixedBlock;
                }
                InflateState::DynBlockHlit => {
                    self.hlit = (read_bits!(self, csrc, 5) as usize) + 257;
                    if self.hlit >= 287 {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidHeader);
                    }
                    self.state = InflateState::DynBlockHdist;
                }
                InflateState::DynBlockHdist => {
                    self.hdist = (read_bits!(self, csrc, 5) as usize) + 1;
                    self.state = InflateState::DynBlockHclen;
                },
                InflateState::DynBlockHclen => {
                    let hclen = (read_bits!(self, csrc, 4) as usize) + 4;
                    self.cur_len_idx = 0;
                    self.len_lengths = [0; 19];
                    self.all_lengths = [0; NUM_LITERALS + NUM_DISTS];
                    self.state = InflateState::DynLengths(hclen);
                },
                InflateState::DynLengths(len) => {
                    for i in 0..len {
                        if csrc.left() < 3 {
                            self.br = csrc.br;
                            self.state = InflateState::DynLengths(len - i);
                            return Err(DecompressError::ShortData);
                        }
                        self.len_lengths[LEN_RECODE[self.cur_len_idx]] = csrc.read(3).unwrap() as u8;
                        self.cur_len_idx += 1;
                    }
                    let mut len_codes = [ShortCodebookDesc { code: 0, bits: 0 }; 19];
                    lengths_to_codes(&self.len_lengths, &mut len_codes)?;
                    let mut cr = ShortCodebookDescReader::new(len_codes.to_vec());
                    let ret = Codebook::new(&mut cr, CodebookMode::LSB);
                    if ret.is_err() {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidHeader);
                    }
                    self.dyn_len_cb = Some(ret.unwrap());
                    self.cur_len_idx = 0;
                    self.state = InflateState::DynCodeLengths;
                },
                InflateState::DynCodeLengths => {
                    if let Some(ref len_cb) = self.dyn_len_cb {
                        while self.cur_len_idx < self.hlit + self.hdist {
                            let ret = csrc.read_cb(len_cb);
                            let val = match ret {
                                    Ok(val) => val,
                                    Err(CodebookError::MemoryError) => {
                                        self.br = csrc.br;
                                        return Err(DecompressError::ShortData);
                                    },
                                    Err(_) => {
                                        self.state = InflateState::End;
                                        return Err(DecompressError::InvalidHeader);
                                    },
                                };
                            if val < 16 {
                                self.all_lengths[self.cur_len_idx] = val as u8;
                                self.cur_len_idx += 1;
                            } else {
                                let idx = (val as usize) - 16;
                                if idx > 2 {
                                    self.state = InflateState::End;
                                    return Err(DecompressError::InvalidHeader);
                                }
                                self.state = InflateState::DynCodeLengthsAdd(idx);
                                continue 'main;
                            }
                        }
                        let (lit_lengths, dist_lengths) = self.all_lengths.split_at(self.hlit);

                        let mut lit_codes = [ShortCodebookDesc { code: 0, bits: 0 }; NUM_LITERALS];
                        lengths_to_codes(&lit_lengths, &mut lit_codes)?;
                        let mut cr = ShortCodebookDescReader::new(lit_codes.to_vec());
                        let ret = Codebook::new(&mut cr, CodebookMode::LSB);
                        if ret.is_err() { return Err(DecompressError::InvalidHeader); }
                        self.dyn_lit_cb = Some(ret.unwrap());

                        let mut dist_codes = [ShortCodebookDesc { code: 0, bits: 0 }; NUM_DISTS];
                        lengths_to_codes(&dist_lengths[..self.hdist], &mut dist_codes)?;
                        let mut cr = ShortCodebookDescReader::new(dist_codes.to_vec());
                        let ret = Codebook::new(&mut cr, CodebookMode::LSB);
                        if ret.is_err() { return Err(DecompressError::InvalidHeader); }
                        self.dyn_dist_cb = Some(ret.unwrap());

                        self.state = InflateState::DynBlock;
                    } else {
                        unreachable!();
                    }
                },
                InflateState::DynCodeLengthsAdd(mode) => {
                    let base = REPEAT_BASE[mode] as usize;
                    let bits = REPEAT_BITS[mode];
                    let len = base + read_bits!(self, csrc, bits) as usize;
                    if self.cur_len_idx + len > self.hlit + self.hdist {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidHeader);
                    }
                    let rpt = if mode == 0 {
                            if self.cur_len_idx == 0 {
                                self.state = InflateState::End;
                                return Err(DecompressError::InvalidHeader);
                            }
                            self.all_lengths[self.cur_len_idx - 1]
                        } else {
                            0
                        };
                    for _ in 0..len {
                        self.all_lengths[self.cur_len_idx] = rpt;
                        self.cur_len_idx += 1;
                    }
                    self.state = InflateState::DynCodeLengths;
                },
                InflateState::DynBlock => {
                    if let Some(ref lit_cb) = self.dyn_lit_cb {
                        let val = read_cb!(self, csrc, lit_cb);
                        if val < 256 {
                            if self.output_idx >= dst.len() {
                                self.br = csrc.br;
                                self.state = InflateState::DynBlockLiteral(val as u8);
                                return Err(DecompressError::OutputFull);
                            }
                            self.put_literal(val as u8);
                            dst[self.output_idx] = val as u8;
                            self.output_idx += 1;
                        } else if val == 256 {
                            if self.final_block {
                                self.state = InflateState::End;
                                return Ok(self.output_idx);
                            } else {
                                self.state = InflateState::BlockStart;
                            }
                        } else {
                            let len_idx = (val - 257) as usize;
                            if len_idx >= LENGTH_BASE.len() {
                                self.state = InflateState::End;
                                return Err(DecompressError::InvalidData);
                            }
                            let len_bits = LENGTH_ADD_BITS[len_idx];
                            let add_base = LENGTH_BASE[len_idx] as usize;
                            if len_bits > 0 {
                                self.state = InflateState::DynBlockLengthExt(add_base, len_bits);
                            } else {
                                self.state = InflateState::DynBlockDist(add_base);
                            }
                        }
                    } else {
                        unreachable!();
                    }
                },
                InflateState::DynBlockLiteral(sym) => {
                    if self.output_idx >= dst.len() {
                        self.br = csrc.br;
                        return Err(DecompressError::OutputFull);
                    }
                    self.put_literal(sym);
                    dst[self.output_idx] = sym;
                    self.output_idx += 1;
                    self.state = InflateState::DynBlock;
                },
                InflateState::DynBlockLengthExt(base, bits) => {
                    let add = read_bits!(self, csrc, bits) as usize;
                    self.state = InflateState::DynBlockDist(base + add);
                },
                InflateState::DynBlockDist(length) => {
                    if let Some(ref dist_cb) = self.dyn_dist_cb {
                        let dist_idx = read_cb!(self, csrc, dist_cb) as usize;
                        if dist_idx >= DIST_BASE.len() {
                            self.state = InflateState::End;
                            return Err(DecompressError::InvalidData);
                        }
                        let dist_bits = DIST_ADD_BITS[dist_idx];
                        let dist_base = DIST_BASE[dist_idx] as usize;
                        if dist_bits == 0 {
                            self.state = InflateState::DynCopy(length, dist_base);
                        } else {
                            self.state = InflateState::DynBlockDistExt(length, dist_base, dist_bits);
                        }
                    } else {
                        unreachable!();
                    }
                },
                InflateState::DynBlockDistExt(length, base, bits) => {
                    let add = read_bits!(self, csrc, bits) as usize;
                    self.state = InflateState::DynCopy(length, base + add);
                },
                InflateState::DynCopy(length, dist) => {
                    if self.output_idx + length > dst.len() {
                        let copy_size = dst.len() - self.output_idx;
                        let ret = self.lz_copy(dist, copy_size, &mut dst[self.output_idx..]);
                        if ret.is_err() {
                            self.state = InflateState::End;
                            return Err(DecompressError::InvalidData);
                        }
                        self.output_idx += copy_size;
                        self.br = csrc.br;
                        self.state = InflateState::DynCopy(length - copy_size, dist);
                        return Err(DecompressError::OutputFull);
                    }
                    let ret = self.lz_copy(dist, length, &mut dst[self.output_idx..]);
                    if ret.is_err() {
                        self.state = InflateState::End;
                        return Err(DecompressError::InvalidData);
                    }
                    self.output_idx += length;
                    self.state = InflateState::DynBlock;
                }
                InflateState::End => {
                    return Ok(0);
                },
            }
        }
    }
    ///! Decompresses input data into output returning the uncompressed data length.
    pub fn uncompress(src: &[u8], dst: &mut [u8]) -> DecompressResult<usize> {
        let mut inflate = Self::new();
        let off = if src.len() > 2 && src[0] == 0x78 && src[1] == 0x9C { 2 } else { 0 };
        inflate.decompress_data(&src[off..], dst, false)
    }
}

impl Default for Inflate {
    fn default() -> Self {
        Self::new()
    }
}

fn lengths_to_codes(lens: &[u8], codes: &mut [ShortCodebookDesc]) -> DecompressResult<()> {
    let mut bits = [0u32; 32];
    let mut pfx  = [0u32; 33];
    for len in lens.iter() {
        let len = *len as usize;
        if len >= bits.len() {
            return Err(DecompressError::InvalidHeader);
        }
        bits[len] += 1;
    }
    bits[0] = 0;
    let mut code = 0;
    for i in 0..bits.len() {
        code = (code + bits[i]) << 1;
        pfx[i + 1] = code;
    }

    for (len, codes) in lens.iter().zip(codes.iter_mut()) {
        let len = *len as usize;
        if len != 0 {
            let bits = len as u8;
            *codes = ShortCodebookDesc { code: reverse_bits(pfx[len], bits), bits };
            pfx[len] += 1;
        } else {
            *codes = ShortCodebookDesc { code: 0, bits: 0 };
        }
    }

    Ok(())
}

struct GzipCRC32 {
    tab: [u32; 256],
    crc: u32,
}

impl GzipCRC32 {
    #[allow(clippy::unreadable_literal)]
    fn new() -> Self {
        let mut tab = [0u32; 256];
        for i in 0..256 {
            let mut c = i as u32;
            for _ in 0..8 {
                if (c & 1) != 0 {
                    c = 0xEDB88320 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            tab[i] = c;
        }
        Self { tab, crc: 0 }
    }
    fn update_crc(&mut self, src: &[u8]) {
        let mut c = !self.crc;
        for el in src.iter() {
            c = self.tab[((c ^ u32::from(*el)) & 0xFF) as usize] ^ (c >> 8);
        }
        self.crc = !c;
    }
}

///! Decodes input data in gzip file format (RFC 1952) returning a vector containing decoded data.
pub fn gzip_decode(br: &mut ByteReader, skip_crc: bool) -> DecompressResult<Vec<u8>> {
    const FLAG_HCRC:    u8 = 0x02;
    const FLAG_EXTRA:   u8 = 0x04;
    const FLAG_NAME:    u8 = 0x08;
    const FLAG_COMMENT: u8 = 0x10;

    let id1 = br.read_byte()?;
    let id2 = br.read_byte()?;
    let cm  = br.read_byte()?;
    let flg = br.read_byte()?;
    let _mtime = br.read_u32le()?;
    let _xfl   = br.read_byte()?;
    let _os    = br.read_byte()?;
    if id1 != 0x1F || id2 != 0x8B || cm != 8 {
        return Err(DecompressError::InvalidHeader);
    }

    if (flg & FLAG_EXTRA) != 0 {
        let xlen = br.read_u16le()? as usize;
        br.read_skip(xlen)?;
    }
    if (flg & FLAG_NAME) != 0 {
        loop {
            let b = br.read_byte()?;
            if b == 0 {
                break;
            }
        }
    }
    if (flg & FLAG_COMMENT) != 0 {
        loop {
            let b = br.read_byte()?;
            if b == 0 {
                break;
            }
        }
    }
    let _hcrc =  if (flg & FLAG_HCRC) != 0 {
            br.read_u16le()?
        } else {
            0
        };
    if (flg & 0xE0) != 0 {
        return Err(DecompressError::Unsupported);
    }

    let mut output: Vec<u8> = Vec::new();
    let mut tail = [0u8; 8];
    let mut inblk = [0u8; 1024];
    let mut oblk = [0u8; 4096];
    let mut inflate = Inflate::new();
    let mut checker = GzipCRC32::new();

    loop {
        let ret = br.read_buf_some(&mut inblk);
        if let Err(ByteIOError::EOF) = ret {
            break;
        }
        let inlen = match ret {
                Ok(val) => val,
                Err(_)  => return Err(DecompressError::IOError),
            };
        let mut repeat = false;
        loop {
            let ret = inflate.decompress_data(&inblk[..inlen], &mut oblk, repeat);
            match ret {
                Ok(outlen) => {
                    checker.update_crc(&oblk[..outlen]);
                    output.extend_from_slice(&oblk[..outlen]);
                    break;
                },
                Err(DecompressError::ShortData) => {
                    break;
                },
                Err(DecompressError::OutputFull) => {
                    repeat = true;
                    checker.update_crc(&oblk);
                    output.extend_from_slice(&oblk);
                },
                Err(err) => {
                    return Err(err);
                },
            }
        }
        // Save last 8 bytes for CRC and size.
        if inlen >= 8 {
            tail.copy_from_slice(&inblk[inlen - 8..][..8]);
        } else {
            let shift_len = 8 - inlen;
            for i in 0..shift_len {
                tail[i] = tail[i + inlen];
            }
            for i in shift_len..8 {
                tail[i] = inblk[i - shift_len];
            }
        }
    }
    if !skip_crc {
        if !inflate.is_finished() { println!("???"); }
        let crc  = read_u32le(&tail[0..4])?;
        let size = read_u32le(&tail[4..8])?;
        if size != (output.len() as u32) {
            return Err(DecompressError::CRCError);
        }
        if crc != checker.crc {
            return Err(DecompressError::CRCError);
        }
    }

    Ok(output)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_inflate1() {
        const TEST_DATA: &[u8] = &[
                0xF3, 0x48, 0xCD, 0xC9, 0xC9, 0xD7, 0x51, 0x28,
                0xCF, 0x2F, 0xCA, 0x49, 0x51, 0x04, 0x00 ];
        const TEST_REF: &[u8] = b"Hello, world!";
        let mut dst_buf = [0u8; 13];
        let len = Inflate::uncompress(TEST_DATA, &mut dst_buf).unwrap();
        assert_eq!(len, 13);
        for i in 0..len {
            assert_eq!(dst_buf[i], TEST_REF[i]);
        }
    }
    #[test]
    fn test_inflate2() {
        const TEST_DATA3: &[u8] = &[ 0x4B, 0x4C, 0x44, 0x80, 0x24, 0x54, 0x80, 0x2C, 0x06, 0x00 ];
        const TEST_REF3: &[u8] = b"aaaaaaaaaaaabbbbbbbbbbbbbbbaaaaabbbbbbb";
        let mut dst_buf = [0u8; 39];

        let mut inflate = Inflate::new();
        let mut output_chunk = [0u8; 7];
        let mut output_pos = 0;
        for input in TEST_DATA3.chunks(3) {
            let mut repeat = false;
            loop {
                let ret = inflate.decompress_data(input, &mut output_chunk, repeat);
                match ret {
                    Ok(len) => {
                        for i in 0..len {
                            dst_buf[output_pos + i] = output_chunk[i];
                        }
                        output_pos += len;
                        break;
                    },
                    Err(DecompressError::ShortData) => {
                        break;
                    },
                    Err(DecompressError::OutputFull) => {
                        repeat = true;
                        for i in 0..output_chunk.len() {
                            dst_buf[output_pos + i] = output_chunk[i];
                        }
                        output_pos += output_chunk.len();
                    },
                    _ => {
                        panic!("decompress error {:?}", ret.err().unwrap());
                    },
                }
            }
        }

        assert_eq!(output_pos, dst_buf.len());
        for i in 0..output_pos {
            assert_eq!(dst_buf[i], TEST_REF3[i]);
        }
    }
    #[test]
    fn test_inflate3() {
        const TEST_DATA: &[u8] = &[
    0x1F, 0x8B, 0x08, 0x08, 0xF6, 0x7B, 0x90, 0x5E, 0x02, 0x03, 0x31, 0x2E, 0x74, 0x78, 0x74, 0x00,
    0xE5, 0x95, 0x4B, 0x4E, 0xC3, 0x30, 0x10, 0x40, 0xF7, 0x39, 0xC5, 0x1C, 0x00, 0x16, 0x70, 0x83,
    0x0A, 0xB5, 0x3B, 0xE8, 0x82, 0x5E, 0x60, 0x1A, 0x4F, 0xE2, 0x11, 0xFE, 0x44, 0x1E, 0xA7, 0x69,
    0x6E, 0xCF, 0x38, 0xDD, 0xB0, 0x40, 0xA2, 0x46, 0x2D, 0x20, 0x2A, 0xE5, 0xAB, 0xCC, 0xE7, 0xBD,
    0x49, 0xAC, 0x6C, 0x03, 0x64, 0x4B, 0xD0, 0x71, 0x92, 0x0C, 0x06, 0x67, 0x88, 0x1D, 0x3C, 0xD9,
    0xC4, 0x92, 0x3D, 0x4A, 0xF3, 0x3C, 0x43, 0x4E, 0x23, 0x81, 0x8B, 0x07, 0x82, 0x1E, 0xF5, 0x90,
    0x23, 0x78, 0x6A, 0x56, 0x30, 0x60, 0xCA, 0x89, 0x4D, 0x4F, 0xC0, 0x01, 0x10, 0x06, 0xC2, 0xA4,
    0xA1, 0x44, 0xCD, 0xF6, 0x54, 0x50, 0xA8, 0x8D, 0xC1, 0x9C, 0x5F, 0x71, 0x37, 0x45, 0xC8, 0x63,
    0xCA, 0x8E, 0xC0, 0xE8, 0x23, 0x69, 0x56, 0x9A, 0x8D, 0x5F, 0xB6, 0xC9, 0x96, 0x53, 0x4D, 0x17,
    0xAB, 0xB9, 0xB0, 0x49, 0x14, 0x5A, 0x0B, 0x96, 0x82, 0x7C, 0xB7, 0x6F, 0x17, 0x35, 0xC7, 0x9E,
    0xDF, 0x78, 0xA3, 0xF1, 0xD0, 0xA2, 0x73, 0x1C, 0x7A, 0xD8, 0x2B, 0xB3, 0x5C, 0x90, 0x85, 0xBB,
    0x2A, 0x14, 0x2E, 0xF7, 0xD1, 0x19, 0x48, 0x0A, 0x23, 0x57, 0x45, 0x13, 0x3E, 0xD6, 0xA0, 0xBD,
    0xF2, 0x11, 0x7A, 0x22, 0x21, 0xAD, 0xE5, 0x70, 0x56, 0xA0, 0x9F, 0xA5, 0xA5, 0x03, 0x85, 0x2A,
    0xDE, 0x92, 0x00, 0x32, 0x61, 0x10, 0xAD, 0x27, 0x13, 0x7B, 0x5F, 0x98, 0x7F, 0x59, 0x83, 0xB8,
    0xB7, 0x35, 0x16, 0xEB, 0x12, 0x0F, 0x1E, 0xD9, 0x14, 0x0B, 0xCF, 0xEE, 0x6D, 0x91, 0xF8, 0x93,
    0x6E, 0x81, 0x3F, 0x7F, 0x41, 0xA4, 0x22, 0x1F, 0xB7, 0xE6, 0x85, 0x83, 0x9A, 0xA2, 0x61, 0x12,
    0x0D, 0x0F, 0x6D, 0x01, 0xBD, 0xB0, 0xE8, 0x1D, 0xEC, 0xD1, 0xA0, 0xBF, 0x1F, 0x4E, 0xFB, 0x55,
    0xBD, 0x73, 0xDD, 0x87, 0xB9, 0x53, 0x23, 0x17, 0xD3, 0xE2, 0xE9, 0x08, 0x87, 0x42, 0xFF, 0xCF,
    0x26, 0x42, 0xAE, 0x76, 0xB5, 0xAE, 0x97, 0x0C, 0x18, 0x78, 0xA0, 0x24, 0xE5, 0x54, 0x0C, 0x6E,
    0x60, 0x52, 0x79, 0x22, 0x57, 0xF5, 0x87, 0x78, 0x78, 0x04, 0x93, 0x46, 0xEF, 0xCB, 0x98, 0x96,
    0x8B, 0x65, 0x00, 0xB7, 0x36, 0xBD, 0x77, 0xA8, 0xBD, 0x5A, 0xAA, 0x1A, 0x09, 0x00, 0x00
        ];

        let mut mr = MemoryReader::new_read(TEST_DATA);
        let mut br = ByteReader::new(&mut mr);
        let _dst_buf = gzip_decode(&mut br, false).unwrap();

//        println!("{}", String::from_utf8_lossy(_dst_buf.as_slice()));
    }
}
