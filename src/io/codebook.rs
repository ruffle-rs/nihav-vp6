use io::bitreader::BitReader;

#[derive(Debug)]
pub enum CodebookError {
    InvalidCodebook,
    MemoryError,
    InvalidCode,
}

type CodebookResult<T> = Result<T, CodebookError>;

pub struct FullCodebookDesc<S> {
    code: u32,
    bits: u8,
    sym:  S,
}

pub struct ShortCodebookDesc {
    code: u32,
    bits: u8,
}

pub trait CodebookDescReader<S> {
    fn bits(&mut self, idx: usize) -> u8;
    fn code(&mut self, idx: usize) -> u32;
    fn sym (&mut self, idx: usize) -> S;
    fn len (&mut self)             -> usize;
}

#[allow(dead_code)]
pub struct Codebook<S> {
    table: Vec<u32>,
    syms:  Vec<S>,
    lut_bits: u8,
}

pub trait CodebookReader<S> {
    fn read_cb(&mut self, cb: &Codebook<S>) -> CodebookResult<S>;
}

impl<S: Copy> Codebook<S> {
//todo allow add escapes
    pub fn new(cb: &mut CodebookDescReader<S>) -> CodebookResult<Self> {
        let mut maxbits = 0;
        let mut nnz = 0;
        for i in 0..cb.len() {
            let bits = cb.bits(i);
            if bits > 0 { nnz = nnz + 1; }
            if bits > maxbits {
                maxbits = bits;
            }
        }
        if maxbits == 0 { return Err(CodebookError::InvalidCodebook); }

        let mut table: Vec<u32> = Vec::new();
        let mut syms:  Vec<S>   = Vec::new();
        let tab_len = 1 << maxbits;
        table.reserve(tab_len);
        if table.capacity() < tab_len { return Err(CodebookError::MemoryError); }
        table.resize(tab_len, 0xFF);
        syms.reserve(nnz);
        if syms.capacity() < nnz { return Err(CodebookError::MemoryError); }

        let mut symidx: u32 = 0;
        for i in 0..cb.len() {
            let bits = cb.bits(i);
            if bits == 0 { continue; }
            let code = cb.code(i) << (maxbits - bits);
            let fill_len = 1 << (maxbits - bits);
            for j in 0..fill_len {
                let idx = (code + j) as usize;
                table[idx] = (symidx << 8) | (bits as u32);
            }
            symidx = symidx + 1;
        }

        for i in 0..cb.len() {
            if cb.bits(i) > 0 {
                syms.push(cb.sym(i));
            }
        }

        Ok(Codebook { table: table, syms: syms, lut_bits: maxbits })
    }
}

impl<'a, S: Copy> CodebookReader<S> for BitReader<'a> {
    #[allow(unused_variables)]
    fn read_cb(&mut self, cb: &Codebook<S>) -> CodebookResult<S> {
        let lut_idx = self.peek(cb.lut_bits) as usize;
        let bits = cb.table[lut_idx] & 0xFF;
        let idx  = (cb.table[lut_idx] >> 8) as usize;
        if bits == 0xFF || (bits as isize) > self.left() {
            return Err(CodebookError::InvalidCode);
        }
        if let Err(_) = self.skip(bits) {}
        let sym = cb.syms[idx];
        return Ok(sym)
    }
}

pub struct FullCodebookDescReader<S> {
    data: Vec<FullCodebookDesc<S>>,
}

impl<S> FullCodebookDescReader<S> {
    pub fn new(data: Vec<FullCodebookDesc<S>>) -> Self {
        FullCodebookDescReader { data: data }
    }
}

impl<S: Copy> CodebookDescReader<S> for FullCodebookDescReader<S> {
    fn bits(&mut self, idx: usize) -> u8  { self.data[idx].bits }
    fn code(&mut self, idx: usize) -> u32 { self.data[idx].code }
    fn sym (&mut self, idx: usize) -> S   { self.data[idx].sym  }
    fn len(&mut self) -> usize { self.data.len() }
}

pub struct ShortCodebookDescReader {
    data: Vec<ShortCodebookDesc>,
}

impl ShortCodebookDescReader {
    pub fn new(data: Vec<ShortCodebookDesc<>>) -> Self {
        ShortCodebookDescReader { data: data }
    }
}

impl CodebookDescReader<u32> for ShortCodebookDescReader {
    fn bits(&mut self, idx: usize) -> u8  { self.data[idx].bits }
    fn code(&mut self, idx: usize) -> u32 { self.data[idx].code }
    fn sym (&mut self, idx: usize) -> u32 { idx as u32 }
    fn len(&mut self) -> usize { self.data.len() }
}

#[cfg(test)]
mod test {
    use super::*;
    use io::bitreader::*;

    #[test]
    fn test_cb() {
        const BITS: [u8; 2] = [0b01011011, 0b10111100];
        let cb_desc: Vec<FullCodebookDesc<i8>> = vec!(
            FullCodebookDesc { code: 0b0,    bits: 1, sym:  16 },
            FullCodebookDesc { code: 0b10,   bits: 2, sym:  -3 },
            FullCodebookDesc { code: 0b110,  bits: 3, sym:  42 },
            FullCodebookDesc { code: 0b1110, bits: 4, sym: -42 }
        );
        let buf = &BITS;
        let mut br = BitReader::new(buf, buf.len(), BitReaderMode::BE);
        let mut cfr = FullCodebookDescReader::new(cb_desc);
        let cb = Codebook::new(&mut cfr).unwrap();
        assert_eq!(br.read_cb(&cb).unwrap(),  16);
        assert_eq!(br.read_cb(&cb).unwrap(),  -3);
        assert_eq!(br.read_cb(&cb).unwrap(),  42);
        assert_eq!(br.read_cb(&cb).unwrap(), -42);
        let ret = br.read_cb(&cb);
        if let Err(e) = ret {
            assert_eq!(e as i32, CodebookError::InvalidCode as i32);
        } else {
            assert_eq!(0, 1);
        }

        let scb_desc: Vec<ShortCodebookDesc> = vec!(
            ShortCodebookDesc { code: 0b0,    bits: 1 },
            ShortCodebookDesc { code: 0,      bits: 0 },
            ShortCodebookDesc { code: 0b10,   bits: 2 },
            ShortCodebookDesc { code: 0,      bits: 0 },
            ShortCodebookDesc { code: 0,      bits: 0 },
            ShortCodebookDesc { code: 0b110,  bits: 3 },
            ShortCodebookDesc { code: 0,      bits: 0 },
            ShortCodebookDesc { code: 0b1110, bits: 4 }
        );
        let mut br2 = BitReader::new(buf, buf.len(), BitReaderMode::BE);
        let mut cfr = ShortCodebookDescReader::new(scb_desc);
        let cb = Codebook::new(&mut cfr).unwrap();
        assert_eq!(br2.read_cb(&cb).unwrap(), 0);
        assert_eq!(br2.read_cb(&cb).unwrap(), 2);
        assert_eq!(br2.read_cb(&cb).unwrap(), 5);
        assert_eq!(br2.read_cb(&cb).unwrap(), 7);
    }
}
