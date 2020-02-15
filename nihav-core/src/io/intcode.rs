//! Some universal integer codes support for bitstream reader.
use crate::io::bitreader::{BitReader, BitReaderError, BitReaderResult};

/// Unsigned integer code types.
#[derive(Debug)]
pub enum UintCodeType {
    /// Code where number is represented as run of ones with terminating zero.
    UnaryOnes,
    /// Code where number is represented as run of zeroes with terminating one.
    UnaryZeroes,
    /// Code for 0, 1 and 2 coded as `0`, `10` and `11`
    Unary012,
    /// Code for 0, 1 and 2 coded as `11`, `10` and `0`
    Unary210,
    /// General limited unary code with defined run and terminating bit.
    LimitedUnary(u32, u32),
    /// Limited run of zeroes with terminating one (unless the code has maximum length).
    ///
    /// [`Unary012`] is essentially an alias for `LimitedZeroes(2)`.
    ///
    /// [`Unary012`]: #variant.Unary012
    LimitedZeroes(u32),
    /// Limited run of one with terminating zero (unless the code has maximum length).
    LimitedOnes(u32),
    /// Golomb code.
    Golomb(u8),
    /// Rice code.
    Rice(u8),
    /// Elias Gamma code (interleaved).
    Gamma,
    /// Elias Gamma' code (sometimes incorrectly called exp-Golomb).
    GammaP,
}

/// Signed integer code types.
pub enum IntCodeType {
    /// Golomb code. Last bit represents the sign.
    Golomb(u8),
    /// Golomb code. Last bit represents the sign.
    Rice(u8),
    /// Elias Gamma code. Unsigned values are remapped as 0, 1, -1, 2, -2, ...
    Gamma,
    /// Elias Gamma' code. Unsigned values are remapped as 0, 1, -1, 2, -2, ...
    GammaP,
}

/// Universal integer code reader trait for bitstream reader.
///
/// # Examples
///
/// Read an unsigned Golomb code:
/// ````
/// use nihav_core::io::bitreader::*;
/// use nihav_core::io::intcode::{IntCodeReader,UintCodeType};
///
/// # fn foo() -> BitReaderResult<()> {
/// let mem: [u8; 4] = [ 0, 1, 2, 3];
/// let mut br = BitReader::new(&mem, BitReaderMode::BE);
/// let val = br.read_code(UintCodeType::Golomb(3))?;
/// # Ok(())
/// # }
/// ````
///
/// Read signed Elias code:
/// ````
/// use nihav_core::io::bitreader::*;
/// use nihav_core::io::intcode::{IntCodeReader,IntCodeType};
///
/// # fn foo() -> BitReaderResult<()> {
/// let mem: [u8; 4] = [ 0, 1, 2, 3];
/// let mut br = BitReader::new(&mem, BitReaderMode::BE);
/// let val = br.read_code_signed(IntCodeType::Gamma)?;
/// # Ok(())
/// # }
/// ````
pub trait IntCodeReader {
    /// Reads an unsigned integer code of requested type.
    fn read_code(&mut self, t: UintCodeType) -> BitReaderResult<u32>;
    /// Reads signed integer code of requested type.
    fn read_code_signed(&mut self, t: IntCodeType) -> BitReaderResult<i32>;
}

fn read_unary(br: &mut BitReader, terminator: u32) -> BitReaderResult<u32> {
    let mut res: u32 = 0;
    loop {
        if br.read(1)? == terminator { return Ok(res); }
        res += 1;
    }
}

fn read_unary_lim(br: &mut BitReader, len: u32, terminator: u32) -> BitReaderResult<u32> {
    let mut res: u32 = 0;
    loop {
        if br.read(1)? == terminator { return Ok(res); }
        res += 1;
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
    let cutoff = u32::from((1 << nbits) - m);
    let pfx = read_unary(br, 0)?;
    let tail = br.read(nbits - 1)?;
    if tail < cutoff {
        let res = pfx * u32::from(m) + tail;
        Ok (res)
    } else {
        let add = br.read(1)?;
        let res = pfx * u32::from(m) + (tail - cutoff) * 2 + add + cutoff;
        Ok (res)
    }
}

fn read_rice(br: &mut BitReader, k: u8) -> BitReaderResult<u32> {
    let pfx = read_unary(br, 1)?;
    let ret = (pfx << k) + br.read(k)?;
    Ok(ret)
}

fn read_gamma(br: &mut BitReader) -> BitReaderResult<u32> {
    let mut ret = 1;
    while br.read(1)? != 1 {
        ret = (ret << 1) | br.read(1)?;
    }
    Ok(ret - 1)
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
            UintCodeType::LimitedZeroes(len)      => read_unary_lim(self, len, 1),
            UintCodeType::LimitedOnes(len)        => read_unary_lim(self, len, 0),
            UintCodeType::LimitedUnary(len, term) => read_unary_lim(self, len, term),
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
    use crate::io::bitreader::*;

    #[test]
    fn int_codes() {
        const GDATA: [u8; 6] = [0b000_001_01, 0b0_0110_011, 0b1_1000_100, 0b1_1010_101, 0b10_10111_1, 0b1000_0000];
        let src = &GDATA;
        let mut br = BitReader::new(src, BitReaderMode::BE);
        for i in 0..11 {
            assert_eq!(br.read_code(UintCodeType::Golomb(5)).unwrap(), i);
        }
    }
}
