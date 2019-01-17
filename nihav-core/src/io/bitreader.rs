#[derive(Debug)]
pub enum BitReaderMode {
    BE,
    LE,
    LE16MSB,
    LE32MSB,
}

#[derive(Debug)]
pub enum BitReaderError {
    BitstreamEnd,
    TooManyBitsRequested,
    InvalidValue,
}

use self::BitReaderError::*;

pub type BitReaderResult<T> = Result<T, BitReaderError>;

#[derive(Debug)]
pub struct BitReader<'a> {
    cache: u64,
    bits:  u8,
    pos:   usize,
    end:   usize,
    src:   &'a [u8],
    mode:  BitReaderMode,
}

impl<'a> BitReader<'a> {

    pub fn new(src: &'a [u8], size: usize, mode: BitReaderMode) -> Self {
        if src.len() < size { panic!("size is less than needed"); }
        BitReader{ cache: 0, pos: 0, bits: 0, end: size, src: src, mode: mode }
    }

    pub fn tell(&self) -> usize {
        self.pos * 8 - (self.bits as usize)
    }

    pub fn left(&self) -> isize {
        ((self.end as isize) - (self.pos as isize)) * 8 + (self.bits as isize)
    }

    fn fill32be(&mut self, src: &[u8]) {
        let nw = (((src[0] as u32) << 24) |
                  ((src[1] as u32) << 16) |
                  ((src[2] as u32) <<  8) |
                  ((src[3] as u32) <<  0)) as u64;
        self.cache |= nw << (32 - self.bits);
    }

    fn fill32le16(&mut self, src: &[u8]) {
        let nw = (((src[1] as u32) << 24) |
                  ((src[0] as u32) << 16) |
                  ((src[3] as u32) <<  8) |
                  ((src[2] as u32) <<  0)) as u64;
        self.cache |= nw << (32 - self.bits);
    }

    fn fill32le32(&mut self, src: &[u8], lsb: bool) {
        let nw = (((src[3] as u32) << 24) |
                  ((src[2] as u32) << 16) |
                  ((src[1] as u32) <<  8) |
                  ((src[0] as u32) <<  0)) as u64;
        if lsb {
            self.cache |= nw << self.bits;
        } else {
            self.cache |= nw << (32 - self.bits);
        }
    }

    #[inline(always)]
    fn refill(&mut self) -> BitReaderResult<()> {
        if self.pos >= self.end { return Err(BitstreamEnd) }
        while self.bits <= 32 {
            if self.pos + 4 <= self.end {
                let buf = &self.src[self.pos..];
                match self.mode {
                    BitReaderMode::BE      => self.fill32be  (buf),
                    BitReaderMode::LE16MSB => self.fill32le16(buf),
                    BitReaderMode::LE      => self.fill32le32(buf, true),
                    BitReaderMode::LE32MSB => self.fill32le32(buf, false),
                }
                self.pos  +=  4;
                self.bits += 32;
            } else {
                let mut buf: [u8; 4] = [0, 0, 0, 0];
                let mut newbits: u8 = 0;
                for i in 0..3 {
                    if self.pos < self.end {
                        buf[i] = self.src[self.pos];
                        self.pos = self.pos + 1;
                        newbits += 8;
                    }
                }
                if newbits == 0 { break; }
                match self.mode {
                    BitReaderMode::BE      => self.fill32be  (&buf),
                    BitReaderMode::LE16MSB => self.fill32le16(&buf),
                    BitReaderMode::LE      => self.fill32le32(&buf, true),
                    BitReaderMode::LE32MSB => self.fill32le32(&buf, false),
                }
                self.bits += newbits;
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn read_cache(&mut self, nbits: u8) -> u32 {
        let res = match self.mode {
            BitReaderMode::LE => ((1u64 << nbits) - 1) & self.cache,
            _                 => (self.cache as u64) >> (64 - nbits),
        };
        res as u32
    }

    fn read_cache_s(&mut self, nbits: u8) -> i32 {
        let res = match self.mode {
            BitReaderMode::LE => ((self.cache as i64) << (64 - nbits)) >> (64 - nbits),
            _                 => (self.cache as i64) >> (64 - nbits),
        };
        res as i32
    }

    #[inline(always)]
    fn skip_cache(&mut self, nbits: u8) {
        match self.mode {
            BitReaderMode::LE => self.cache >>= nbits,
            _                 => self.cache <<= nbits,
        };
        self.bits -= nbits;
    }

    #[inline(always)]
    fn reset_cache(&mut self) {
        self.bits = 0;
        self.cache = 0;
    }

    #[inline(always)]
    pub fn read(&mut self, nbits: u8) -> BitReaderResult<u32> {
        if nbits == 0 { return Ok(0) }
        if nbits > 32 { return Err(TooManyBitsRequested) }
        if self.bits < nbits {
            if let Err(err) = self.refill() { return Err(err) }
            if self.bits < nbits { return Err(BitstreamEnd) }
        }
        let res = self.read_cache(nbits);
        self.skip_cache(nbits);
        Ok(res)
    }

    pub fn read_s(&mut self, nbits: u8) -> BitReaderResult<i32> {
        if nbits == 0 || nbits > 32 { return Err(TooManyBitsRequested) }
        if self.bits < nbits {
            if let Err(err) = self.refill() { return Err(err) }
            if self.bits < nbits { return Err(BitstreamEnd) }
        }
        let res = self.read_cache_s(nbits);
        self.skip_cache(nbits);
        Ok(res)
    }

    #[inline(always)]
    pub fn read_bool(&mut self) -> BitReaderResult<bool> {
        if self.bits < 1 {
            if let Err(err) = self.refill() { return Err(err) }
            if self.bits < 1 { return Err(BitstreamEnd) }
        }
        let res = self.read_cache(1);
        self.skip_cache(1);
        Ok(res == 1)
    }

    #[inline(always)]
    pub fn peek(&mut self, nbits: u8) -> u32 {
        if nbits > 32 { return 0 }
        if self.bits < nbits { let _ = self.refill(); }
        self.read_cache(nbits)
    }

    #[inline(always)]
    pub fn skip(&mut self, nbits: u32) -> BitReaderResult<()> {
        if self.bits as u32 >= nbits {
            self.skip_cache(nbits as u8);
            return Ok(());
        }
        let mut skip_bits = nbits - (self.bits as u32);
        self.reset_cache();
        self.pos += ((skip_bits / 32) * 4) as usize;
        skip_bits = skip_bits & 0x1F;
        self.refill()?;
        if skip_bits > 0 {
            self.skip_cache(skip_bits as u8);
        }
        Ok(())
    }

    pub fn seek(&mut self, nbits: u32) -> BitReaderResult<()> {
        if ((nbits + 7) >> 3) as usize > self.end { return Err(TooManyBitsRequested); }
        self.reset_cache();
        self.pos = ((nbits / 32) * 4) as usize;
        self.skip(nbits & 0x1F)
    }

    pub fn align(&mut self) {
        let pos = self.bits & 7;
        if pos != 0 {
            self.skip_cache(pos);
        }
    }
}

pub fn reverse_bits(inval: u32, len: u8) -> u32 {
    if len == 0 { return 0; }
    const REV_TAB: [u8; 16] = [
        0b0000, 0b1000, 0b0100, 0b1100, 0b0010, 0b1010, 0b0110, 0b1110,
        0b0001, 0b1001, 0b0101, 0b1101, 0b0011, 0b1011, 0b0111, 0b1111,
    ];

    let mut ret = 0;
    let mut val = inval;
    for _ in 0..8 {
        ret = (ret << 4) | (REV_TAB[(val & 0xF) as usize] as u32);
        val = val >> 4;
    }
    ret >> (32 - len)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn br_works() {
        const DATA: [u8; 18] = [0b00011011; 18];
        let src = &DATA;
        let mut br = BitReader::new(src, src.len(), BitReaderMode::LE16MSB);

        for _ in 0..8 {
            assert_eq!(br.read(16).unwrap(), 0x1B1B);
        }
        const DATA2: [u8; 1] = [ 0b00011011 ];
        let src = &DATA2;
        let mut br = BitReader::new(src, src.len(), BitReaderMode::LE);
        assert_eq!(br.read_s(5).unwrap(), -5);
    }
}
