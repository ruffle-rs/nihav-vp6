use std::io::SeekFrom;
use std::fs::File;
use std::io::prelude::*;

#[derive(Debug)]
pub enum ByteIOError {
    EOF,
    WrongRange,
    WrongIOMode,
    NotImplemented,
    ReadError,
    WriteError,
    SeekError,
}

type ByteIOResult<T> = Result<T, ByteIOError>;

pub trait ByteIO {
    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    fn read_byte(&mut self) -> ByteIOResult<u8>;
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<usize>;
    fn tell(&mut self) -> u64;
    fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64>;
    fn is_eof(&mut self) -> bool;
    fn is_seekable(&mut self) -> bool;
}

#[allow(dead_code)]
pub struct ByteReader<'a> {
    io: &'a mut ByteIO,
}

pub struct MemoryReader<'a> {
    buf:      &'a [u8],
    size:     usize,
    pos:      usize,
    do_write: bool,
}

pub struct FileReader<'a> {
    file:     &'a File,
    eof:      bool,
}

macro_rules! read_int {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => ({
        let mut buf = [0; $size];
        try!($s.read_buf(&mut buf));
        unsafe {
            Ok((*(buf.as_ptr() as *const $inttype)).$which())
        }
    })
}

impl<'a> ByteReader<'a> {
    pub fn new(io: &'a mut ByteIO) -> ByteReader { ByteReader { io: io } }

    pub fn read_buf(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.read_buf(buf)
    }

    pub fn read_byte(&mut self) -> ByteIOResult<u8> {
        self.io.read_byte()
    }

    pub fn read_u16be(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_be)
    }

    pub fn read_u24be(&mut self) -> ByteIOResult<u32> {
        let p16 = self.read_u16be();
        if let Err(e) = p16 { return Err(e); }
        let p8 = self.read_byte();
        if let Err(e) = p8 { return Err(e); }
        Ok(((p16.unwrap() as u32) << 8) | (p8.unwrap() as u32))
    }

    pub fn read_u32be(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_be)
    }

    pub fn read_u64be(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_be)
    }

    pub fn read_u16le(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_le)
    }

    pub fn read_u24le(&mut self) -> ByteIOResult<u32> {
        let p8 = self.read_byte();
        if let Err(e) = p8 { return Err(e); }
        let p16 = self.read_u16le();
        if let Err(e) = p16 { return Err(e); }
        Ok(((p16.unwrap() as u32) << 8) | (p8.unwrap() as u32))
    }

    pub fn read_u32le(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_le)
    }

    pub fn read_u64le(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_le)
    }

    pub fn read_skip(&mut self, len: usize) -> ByteIOResult<u64> {
        if self.io.is_seekable() {
            self.io.seek(SeekFrom::Current(len as i64))?;
        } else {
            let mut ssize = len;
            let mut buf : [u8; 16] = [0; 16];
            let mut bref = &mut buf;
            while ssize > bref.len() {
                self.io.read_buf(bref)?;
                ssize -= bref.len();
            }
            while ssize > 0 {
                self.io.read_byte()?;
                ssize = ssize - 1;
            }
        }
        Ok(self.tell())
    }

    pub fn tell(&mut self) -> u64 {
        self.io.tell()
    }

    pub fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        self.io.seek(pos)
    }

    pub fn is_eof(&mut self) -> bool {
        self.io.is_eof()
    }
}

impl<'a> MemoryReader<'a> {

    pub fn new_read(buf: &'a [u8]) -> Self {
        MemoryReader { buf: buf, size: buf.len(), pos: 0, do_write: false }
    }

    fn real_seek(&mut self, pos: i64) -> ByteIOResult<u64> {
        if pos < 0 || (pos as usize) > self.size {
            return Err(ByteIOError::WrongRange)
        }
        self.pos = pos as usize;
        Ok(pos as u64)
    }

    fn check_read_perm(&self) -> ByteIOResult<()> {
        if self.do_write {
            Err(ByteIOError::WrongIOMode)
        } else {
            Ok(())
        }
    }

    fn check_write_perm(&self) -> ByteIOResult<()> {
        if !self.do_write {
            Err(ByteIOError::WrongIOMode)
        } else {
            Ok(())
        }
    }
}

impl<'a> ByteIO for MemoryReader<'a> {
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        self.check_read_perm()?;
        if self.is_eof() { return Err(ByteIOError::EOF); }
        let res = self.buf[self.pos];
        self.pos = self.pos + 1;
        Ok(res)
    }

    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        self.check_read_perm()?;
        let copy_size = if self.size - self.pos < buf.len() { self.size } else { buf.len() };
        if copy_size == 0 { return Err(ByteIOError::EOF); }
        for i in 0..copy_size {
            buf[i] = self.buf[self.pos + i];
        }
        self.pos += copy_size;
        Ok(copy_size)
    }

    #[allow(unused_variables)]
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<usize> {
        self.check_write_perm()?;
        Err(ByteIOError::NotImplemented)
    }

    fn tell(&mut self) -> u64 {
        self.pos as u64
    }

    fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        let cur_pos  = self.pos  as i64;
        let cur_size = self.size as i64;
        match pos {
            SeekFrom::Start(x)   => self.real_seek(x as i64),
            SeekFrom::Current(x) => self.real_seek(cur_pos + x),
            SeekFrom::End(x)     => self.real_seek(cur_size + x),
        }
    }

    fn is_eof(&mut self) -> bool {
        self.pos >= self.size
    }

    fn is_seekable(&mut self) -> bool {
        true
    }
}

impl<'a> FileReader<'a> {

    pub fn new_read(file: &'a mut File) -> Self {
        FileReader { file: file, eof : false }
    }
}

impl<'a> ByteIO for FileReader<'a> {
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        let mut byte : [u8; 1] = [0];
        let err = self.file.read(&mut byte);
        if let Err(_) = err { return Err(ByteIOError::ReadError); }
        let sz = err.unwrap();
        if sz == 0 { self.eof = true; return Err(ByteIOError::EOF); }
        Ok (byte[0])
    }

    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let res = self.file.read(buf);
        if let Err(_) = res { return Err(ByteIOError::ReadError); }
        let sz = res.unwrap();
        if sz < buf.len() { self.eof = true; }
        Ok(sz)
    }

    #[allow(unused_variables)]
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    fn tell(&mut self) -> u64 {
        self.file.seek(SeekFrom::Current(0)).unwrap()
    }

    fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        let res = self.file.seek(pos);
        match res {
            Ok(r) => Ok(r),
            Err(_) => Err(ByteIOError::SeekError),
        }
    }

    fn is_eof(&mut self) -> bool {
        self.eof
    }

    fn is_seekable(&mut self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_read() {
        //const DATA : &'static [u8] = include_bytes!("../../assets/file");
        let buf: [u8; 64] = [1; 64];
        let mut mr = MemoryReader::new_read(&buf);
        let mut reader = ByteReader::new(&mut mr);
        assert_eq!(reader.read_byte().unwrap(),  0x01u8);
        assert_eq!(reader.read_u16le().unwrap(), 0x0101u16);
        assert_eq!(reader.read_u24le().unwrap(), 0x010101u32);
        assert_eq!(reader.read_u32le().unwrap(), 0x01010101u32);
        assert_eq!(reader.read_u64le().unwrap(), 0x0101010101010101u64);
        let mut file = File::open("assets/MaoMacha.asx").unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br2 = ByteReader::new(&mut fr);
        assert_eq!(br2.read_byte().unwrap(), 0x30);
        assert_eq!(br2.read_u24be().unwrap(), 0x26B275);
        assert_eq!(br2.read_u24le().unwrap(), 0xCF668E);
        assert_eq!(br2.read_u32be().unwrap(), 0x11A6D900);
        assert_eq!(br2.read_u32le().unwrap(), 0xCE6200AA);
    }
    #[test]
    fn test_write() {
    }
}
