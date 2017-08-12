use io::bitreader::{BitReader, BitReaderError, BitReaderResult};

#[derive(Debug)]
pub enum UintCodeType {
    UnaryOnes,
    UnaryZeroes,
    Unary012,
    Unary210,
    LimitedUnary(u32, u32),
    Golomb(u8),
    Rice(u8),
    Gamma,
    GammaP,
}

pub enum IntCodeType {
    Golomb(u8),
    Rice(u8),
    Gamma,
    GammaP,
}

pub trait IntCodeReader {
    fn read_code(&mut self, t: UintCodeType) -> BitReaderResult<u32>;
    fn read_code_signed(&mut self, t: IntCodeType) -> BitReaderResult<i32>;
}

fn read_unary(br: &mut BitReader, terminator: u32) -> BitReaderResult<u32> {
    let mut res: u32 = 0;
    loop {
        if br.read(1)? == terminator { return Ok(res); }
        res = res + 1;
    }
}

fn read_unary_lim(br: &mut BitReader, terminator: u32, len: u32) -> BitReaderResult<u32> {
    let mut res: u32 = 0;
    loop {
        if br.read(1)? == terminator { return Ok(res); }
        res = res + 1;
        if res == len { return Ok(res); }
    }
}

fn read_unary210(br: &mut BitReader) -> BitReaderResult<u32> {
    let val = read_unary_lim(br, 2, 0)?;
    Ok(2 - val)
}

fn read_golomb(br: &mut BitReader, m: u8) -> BitReaderResult<u32> {
    if m == 0 { return Err(BitReaderError::InvalidValue); }
    let nbits = (8 - m.leading_zeros()) as u8;
    if (m & (m - 1)) == 0 { return read_rice(br, nbits); }
    let cutoff = ((1 << nbits) - m) as u32;
    let pfx = read_unary(br, 0)?;
    let tail = br.read(nbits - 1)?;
    if tail < cutoff {
        let res = pfx * (m as u32) + tail;
        Ok (res)
    } else {
        let add = br.read(1)?;
        let res = pfx * (m as u32) + (tail - cutoff) * 2 + add + cutoff;
        Ok (res)
    }
}

fn read_rice(br: &mut BitReader, k: u8) -> BitReaderResult<u32> {
    let pfx = read_unary(br, 1)?;
    let ret = (pfx << k) + br.read(k)?;
    Ok(ret)
}

fn read_gamma(br: &mut BitReader) -> BitReaderResult<u32> {
    let mut ret = 0;
    while br.read(1)? != 1 {
        ret = (ret << 1) | br.read(1)?;
    }
    Ok(ret)
}

fn read_gammap(br: &mut BitReader) -> BitReaderResult<u32> {
    let pfx = read_unary(br, 1)?;
    if pfx > 32 { return Err(BitReaderError::InvalidValue); }
    let ret = (1 << pfx) + br.read(pfx as u8)?;
    Ok(ret)
}

fn uval_to_sval0mp(uval: u32) -> i32 {
    if (uval & 1) != 0 { -((uval >> 1) as i32) }
    else               { (uval >> 1) as i32 }
}

fn uval_to_sval0pm(uval: u32) -> i32 {
    if (uval & 1) != 0 { ((uval + 1) >> 1) as i32 }
    else               { -((uval >> 1) as i32) }
}

impl<'a> IntCodeReader for BitReader<'a> {
    #[inline(always)]
    fn read_code(&mut self, t: UintCodeType) -> BitReaderResult<u32> {
        match t {
            UintCodeType::UnaryOnes               => read_unary(self, 0),
            UintCodeType::UnaryZeroes             => read_unary(self, 1),
            UintCodeType::LimitedUnary(len, term) => read_unary_lim(self, term, len),
            UintCodeType::Unary012                => read_unary_lim(self, 2, 0),
            UintCodeType::Unary210                => read_unary210(self),
            UintCodeType::Golomb(m)               => read_golomb(self, m),
            UintCodeType::Rice(k)                 => read_rice(self, k),
            UintCodeType::Gamma                   => read_gamma(self),
            UintCodeType::GammaP                  => read_gammap(self),
        }
    }
    #[allow(unused_variables)]
    fn read_code_signed(&mut self, t: IntCodeType) -> BitReaderResult<i32> {
        let uval =
            match t {
                IntCodeType::Golomb(m)               => read_golomb(self, m)?,
                IntCodeType::Rice(k)                 => read_rice(self, k)?,
                IntCodeType::Gamma                   => read_gamma(self)?,
                IntCodeType::GammaP                  => read_gammap(self)?,
            };
        match t {
            IntCodeType::Golomb(m)               => Ok(uval_to_sval0mp(uval)),
            IntCodeType::Rice(k)                 => Ok(uval_to_sval0mp(uval)),
            IntCodeType::Gamma                   => Ok(uval_to_sval0pm(uval)),
            IntCodeType::GammaP                  => Ok(uval_to_sval0pm(uval)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use io::bitreader::*;

    #[test]
    fn int_codes() {
        const GDATA: [u8; 6] = [0b000_001_01, 0b0_0110_011, 0b1_1000_100, 0b1_1010_101, 0b10_10111_1, 0b1000_0000];
        let src = &GDATA;
        let mut br = BitReader::new(src, src.len(), BitReaderMode::BE);
        for i in 0..11 {
            assert_eq!(br.read_code(UintCodeType::Golomb(5)).unwrap(), i);
        }
    }
}
