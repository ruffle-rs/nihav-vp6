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
    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    fn read_byte(&mut self) -> ByteIOResult<u8>;
    fn peek_byte(&mut self) -> ByteIOResult<u8>;
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()>;
    fn tell(&mut self) -> u64;
    fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64>;
    fn is_eof(&mut self) -> bool;
    fn is_seekable(&mut self) -> bool;
    fn size(&mut self) -> i64;
}

#[allow(dead_code)]
pub struct ByteReader<'a> {
    io: &'a mut ByteIO,
}

pub struct MemoryReader<'a> {
    buf:      &'a [u8],
    size:     usize,
    pos:      usize,
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

macro_rules! peek_int {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => ({
        let mut buf = [0; $size];
        try!($s.peek_buf(&mut buf));
        unsafe {
            Ok((*(buf.as_ptr() as *const $inttype)).$which())
        }
    })
}

impl<'a> ByteReader<'a> {
    pub fn new(io: &'a mut ByteIO) -> Self { ByteReader { io: io } }

    pub fn read_buf(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.read_buf(buf)
    }

    pub fn read_buf_some(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.read_buf_some(buf)
    }

    pub fn peek_buf(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.peek_buf(buf)
    }

    pub fn read_byte(&mut self) -> ByteIOResult<u8> {
        self.io.read_byte()
    }

    pub fn peek_byte(&mut self) -> ByteIOResult<u8> {
        self.io.peek_byte()
    }

    pub fn read_u16be(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_be)
    }

    pub fn peek_u16be(&mut self) -> ByteIOResult<u16> {
        peek_int!(self, u16, 2, to_be)
    }

    pub fn read_u24be(&mut self) -> ByteIOResult<u32> {
        let p16 = self.read_u16be()?;
        let p8 = self.read_byte()?;
        Ok(((p16 as u32) << 8) | (p8 as u32))
    }

    pub fn peek_u24be(&mut self) -> ByteIOResult<u32> {
        let mut src: [u8; 3] = [0; 3];
        self.peek_buf(&mut src)?;
        Ok(((src[0] as u32) << 16) | ((src[1] as u32) << 8) | (src[2] as u32))
    }

    pub fn read_u32be(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_be)
    }

    pub fn peek_u32be(&mut self) -> ByteIOResult<u32> {
        peek_int!(self, u32, 4, to_be)
    }

    pub fn read_u64be(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_be)
    }

    pub fn peek_u64be(&mut self) -> ByteIOResult<u64> {
        peek_int!(self, u64, 8, to_be)
    }

    pub fn read_u16le(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_le)
    }

    pub fn peek_u16le(&mut self) -> ByteIOResult<u16> {
        peek_int!(self, u16, 2, to_le)
    }

    pub fn read_u24le(&mut self) -> ByteIOResult<u32> {
        let p8 = self.read_byte()?;
        let p16 = self.read_u16le()?;
        Ok(((p16 as u32) << 8) | (p8 as u32))
    }

    pub fn peek_u24le(&mut self) -> ByteIOResult<u32> {
        let mut src: [u8; 3] = [0; 3];
        self.peek_buf(&mut src)?;
        Ok((src[0] as u32) | ((src[1] as u32) << 8) | ((src[2] as u32) << 16))
    }

    pub fn read_u32le(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_le)
    }

    pub fn peek_u32le(&mut self) -> ByteIOResult<u32> {
        peek_int!(self, u32, 4, to_le)
    }

    pub fn read_u64le(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_le)
    }

    pub fn peek_u64le(&mut self) -> ByteIOResult<u64> {
        peek_int!(self, u64, 8, to_le)
    }

    pub fn read_skip(&mut self, len: usize) -> ByteIOResult<()> {
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
        Ok(())
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

    pub fn size(&mut self) -> i64 {
        self.io.size()
    }

    pub fn left(&mut self) -> i64 {
        let size = self.io.size();
        if size == -1 { return -1; }
        return size - (self.io.tell() as i64)
    }
}

impl<'a> MemoryReader<'a> {

    pub fn new_read(buf: &'a [u8]) -> Self {
        MemoryReader { buf: buf, size: buf.len(), pos: 0 }
    }

    fn real_seek(&mut self, pos: i64) -> ByteIOResult<u64> {
        if pos < 0 || (pos as usize) > self.size {
            return Err(ByteIOError::WrongRange)
        }
        self.pos = pos as usize;
        Ok(pos as u64)
    }
}

impl<'a> ByteIO for MemoryReader<'a> {
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        if self.is_eof() { return Err(ByteIOError::EOF); }
        let res = self.buf[self.pos];
        self.pos = self.pos + 1;
        Ok(res)
    }

    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        if self.is_eof() { return Err(ByteIOError::EOF); }
        Ok(self.buf[self.pos])
    }

    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let copy_size = if self.size - self.pos < buf.len() { self.size } else { buf.len() };
        if copy_size == 0 { return Err(ByteIOError::EOF); }
        for i in 0..copy_size {
            buf[i] = self.buf[self.pos + i];
        }
        Ok(copy_size)
    }

    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let read_size = self.peek_buf(buf)?;
        if read_size < buf.len() { return Err(ByteIOError::EOF); }
        self.pos += read_size;
        Ok(read_size)
    }

    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let read_size = self.peek_buf(buf)?;
        self.pos += read_size;
        Ok(read_size)
    }

    #[allow(unused_variables)]
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()> {
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

    fn size(&mut self) -> i64 {
        self.buf.len() as i64
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

    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        let b = self.read_byte()?;
        self.seek(SeekFrom::Current(-1))?;
        Ok(b)
    }

    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let res = self.file.read(buf);
        if let Err(_) = res { return Err(ByteIOError::ReadError); }
        let sz = res.unwrap();
        if sz < buf.len() { self.eof = true; return Err(ByteIOError::EOF); }
        Ok(sz)
    }

    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let res = self.file.read(buf);
        if let Err(_) = res { return Err(ByteIOError::ReadError); }
        let sz = res.unwrap();
        if sz < buf.len() { self.eof = true; }
        Ok(sz)
    }

    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let size = self.read_buf(buf)?;
        self.seek(SeekFrom::Current(-(size as i64)))?;
        Ok(size)
    }

    #[allow(unused_variables)]
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()> {
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

    fn size(&mut self) -> i64 {
        -1
    }
}

#[allow(dead_code)]
pub struct ByteWriter<'a> {
    io: &'a mut ByteIO,
}

pub struct MemoryWriter<'a> {
    buf:      &'a mut [u8],
    size:     usize,
    pos:      usize,
}

pub struct FileWriter<'a> {
    file:     &'a File,
}

impl<'a> ByteWriter<'a> {
    pub fn new(io: &'a mut ByteIO) -> Self { ByteWriter { io: io } }

    pub fn write_buf(&mut self, buf: &[u8])  -> ByteIOResult<()> {
        self.io.write_buf(buf)
    }

    pub fn write_byte(&mut self, val: u8) -> ByteIOResult<()> {
        let buf: [u8; 1] = [val];
        self.io.write_buf(&buf)
    }

    pub fn write_u16be(&mut self, val: u16) -> ByteIOResult<()> {
        let buf: [u8; 2] = [((val >> 8) & 0xFF) as u8, (val & 0xFF) as u8];
        self.io.write_buf(&buf)
    }

    pub fn write_u16le(&mut self, val: u16) -> ByteIOResult<()> {
        let buf: [u8; 2] = [(val & 0xFF) as u8, ((val >> 8) & 0xFF) as u8];
        self.io.write_buf(&buf)
    }

    pub fn write_u24be(&mut self, val: u32) -> ByteIOResult<()> {
        let buf: [u8; 3] = [((val >> 16) & 0xFF) as u8, ((val >> 8) & 0xFF) as u8, (val & 0xFF) as u8];
        self.write_buf(&buf)
    }

    pub fn write_u24le(&mut self, val: u32) -> ByteIOResult<()> {
        let buf: [u8; 3] = [(val & 0xFF) as u8, ((val >> 8) & 0xFF) as u8, ((val >> 16) & 0xFF) as u8];
        self.write_buf(&buf)
    }

    pub fn write_u32be(&mut self, val: u32) -> ByteIOResult<()> {
        self.write_u16be(((val >> 16) & 0xFFFF) as u16)?;
        self.write_u16be((val & 0xFFFF) as u16)
    }

    pub fn write_u32le(&mut self, val: u32) -> ByteIOResult<()> {
        self.write_u16le((val & 0xFFFF) as u16)?;
        self.write_u16le(((val >> 16) & 0xFFFF) as u16)
    }

    pub fn write_u64be(&mut self, val: u64) -> ByteIOResult<()> {
        self.write_u32be(((val >> 32) & 0xFFFFFFFF) as u32)?;
        self.write_u32be((val & 0xFFFFFFFF) as u32)
    }

    pub fn write_u64le(&mut self, val: u64) -> ByteIOResult<()> {
        self.write_u32le((val & 0xFFFFFFFF) as u32)?;
        self.write_u32le(((val >> 32) & 0xFFFFFFFF) as u32)
    }

    pub fn tell(&mut self) -> u64 {
        self.io.tell()
    }

    pub fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        self.io.seek(pos)
    }

    pub fn size_left(&mut self) -> i64 {
        let sz = self.io.size();
        if sz == -1 { return -1; }
        sz - (self.tell() as i64)
    }
}

impl<'a> MemoryWriter<'a> {

    pub fn new_write(buf: &'a mut [u8]) -> Self {
        let len = buf.len();
        MemoryWriter { buf: buf, size: len, pos: 0 }
    }

    fn real_seek(&mut self, pos: i64) -> ByteIOResult<u64> {
        if pos < 0 || (pos as usize) > self.size {
            return Err(ByteIOError::WrongRange)
        }
        self.pos = pos as usize;
        Ok(pos as u64)
    }
}

impl<'a> ByteIO for MemoryWriter<'a> {
    #[allow(unused_variables)]
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()> {
        if self.pos + buf.len() > self.size { return Err(ByteIOError::WriteError); }
        for i in 0..buf.len() {
            self.buf[self.pos + i] = buf[i];
        }
        self.pos += buf.len();
        Ok(())
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

    fn size(&mut self) -> i64 {
        self.buf.len() as i64
    }
}

impl<'a> FileWriter<'a> {
    pub fn new_write(file: &'a mut File) -> Self {
        FileWriter { file: file }
    }
}

impl<'a> ByteIO for FileWriter<'a> {
    #[allow(unused_variables)]
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    #[allow(unused_variables)]
    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        Err(ByteIOError::NotImplemented)
    }

    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()> {
        match self.file.write_all(buf) {
            Ok(()) => Ok(()),
            Err(_) => Err(ByteIOError::WriteError),
        }
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
        false
    }

    fn is_seekable(&mut self) -> bool {
        true
    }

    fn size(&mut self) -> i64 {
        -1
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
        let mut buf: [u8; 64] = [0; 64];
        {
            let mut mw = MemoryWriter::new_write(&mut buf);
            let mut bw = ByteWriter::new(&mut mw);
            bw.write_byte(0x00).unwrap();
            bw.write_u16be(0x0102).unwrap();
            bw.write_u24be(0x030405).unwrap();
            bw.write_u32be(0x06070809).unwrap();
            bw.write_u64be(0x0A0B0C0D0E0F1011).unwrap();
            bw.write_byte(0x00).unwrap();
            bw.write_u16le(0x0201).unwrap();
            bw.write_u24le(0x050403).unwrap();
            bw.write_u32le(0x09080706).unwrap();
            bw.write_u64le(0x11100F0E0D0C0B0A).unwrap();
            assert_eq!(bw.size_left(), 28);
        }
        for i in 0..0x12 {
            assert_eq!(buf[(i + 0x00) as usize], i);
            assert_eq!(buf[(i + 0x12) as usize], i);
        }
    }
}
