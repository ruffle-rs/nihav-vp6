//! Bytestream reading/writing functionality.
pub use std::io::SeekFrom;
use std::fs::File;
use std::io::prelude::*;
use std::ptr;

/// A list specifying general bytestream reading and writing errors.
#[derive(Debug)]
pub enum ByteIOError {
    /// End of stream.
    EOF,
    /// Wrong seek position was provided.
    WrongRange,
    /// Tried to call read() on bytestream writer or write() on bytestream reader.
    WrongIOMode,
    /// Functionality is not implemented.
    NotImplemented,
    /// Read error.
    ReadError,
    /// Write error.
    WriteError,
    /// Seeking failed.
    SeekError,
}

/// A specialised `Result` type for bytestream operations.
pub type ByteIOResult<T> = Result<T, ByteIOError>;

/// Common trait for bytestream operations.
pub trait ByteIO {
    /// Reads data into provided buffer. Fails if it cannot fill whole buffer.
    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    /// Reads data into provided buffer. Partial read is treated as success.
    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    /// Reads data into provided buffer but does not advance read position.
    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize>;
    /// Reads single byte from the stream.
    fn read_byte(&mut self) -> ByteIOResult<u8>;
    /// Returns the next byte value in the stream without advancing read position.
    fn peek_byte(&mut self) -> ByteIOResult<u8>;
    /// Writes buffer to the stream.
    fn write_buf(&mut self, buf: &[u8]) -> ByteIOResult<()>;
    /// Returns current read or write position.
    fn tell(&mut self) -> u64;
    /// Seeks to the provided position.
    fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64>;
    /// Tells whether this is end of stream.
    fn is_eof(&self) -> bool;
    /// Reports whether stream is seekable or not.
    fn is_seekable(&mut self) -> bool;
    /// Returns stream size or -1 if it is not known.
    fn size(&mut self) -> i64;
}

/// High-level bytestream reader.
///
/// User is supposed to create some reader implementing [`ByteIO`] trait e.g. [`MemoryReader`] and use it to create `ByteReader` which can be used for reading e.g. various integer types.
///
/// # Examples
///
/// ````
/// use nihav_core::io::byteio::{MemoryReader,ByteReader};
/// # use nihav_core::io::byteio::ByteIOResult;
///
/// # fn foo() -> ByteIOResult<()> {
/// let memory: [u8; 4] = [ 0, 42, 42, 0 ];
/// let mut mr = MemoryReader::new_read(&memory);
/// let mut br = ByteReader::new(&mut mr);
/// let val = br.read_u16be()?; // read 16-bit big-endian integer, should be 42
/// let val = br.read_u16le()?; // read 16-bit little-endian integer, should be 42 as well
/// # Ok(())
/// # }
/// ````
///
/// [`ByteIO`]: ./trait.ByteIO.html
/// [`MemoryReader`]: ./struct.MemoryReader.html
#[allow(dead_code)]
pub struct ByteReader<'a> {
    io: &'a mut ByteIO,
}

/// Bytestream reader from memory.
pub struct MemoryReader<'a> {
    buf:      &'a [u8],
    pos:      usize,
}

/// Bytestream reader from file.
pub struct FileReader<'a> {
    file:     &'a File,
    eof:      bool,
}

macro_rules! read_int {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => ({
        unsafe {
            let mut buf: $inttype = 0;
            $s.read_buf(&mut *(&mut buf as *mut $inttype as *mut [u8; $size]))?;
            Ok(buf.$which())
        }
    })
}

macro_rules! peek_int {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => ({
        unsafe {
            let mut buf: $inttype = 0;
            $s.peek_buf(&mut *(&mut buf as *mut $inttype as *mut [u8; $size]))?;
            Ok(buf.$which())
        }
    })
}

macro_rules! read_int_func {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => {
/// Reads integer of certain size and endianness.
        pub fn $s(src: &[u8]) -> ByteIOResult<$inttype> {
            if src.len() < $size { return Err(ByteIOError::ReadError); }
            unsafe {
                let mut buf: $inttype = 0;
                ptr::copy_nonoverlapping(src.as_ptr(), &mut buf as *mut $inttype as *mut u8, std::mem::size_of::<$inttype>());
                Ok(buf.$which())
            }
        }
    }
}

read_int_func!(read_u16be, u16, 2, to_be);
read_int_func!(read_u16le, u16, 2, to_le);
read_int_func!(read_u32be, u32, 4, to_be);
read_int_func!(read_u32le, u32, 4, to_le);
read_int_func!(read_u64be, u64, 8, to_be);
read_int_func!(read_u64le, u64, 8, to_le);

/// Reads 24-bit big-endian integer.
///
/// # Example
///
/// ````
/// use nihav_core::io::byteio::read_u24be;
/// # use nihav_core::io::byteio::ByteIOResult;
///
/// # fn foo() -> ByteIOResult<()> {
/// let src: [u8; 3] = [ 1, 2, 3];
/// let value = read_u24be(&src)?; // should return 0x010203
/// # Ok(())
/// # }
/// ````
pub fn read_u24be(src: &[u8]) -> ByteIOResult<u32> {
    if src.len() < 3 { return Err(ByteIOError::ReadError); }
    Ok((u32::from(src[0]) << 16) | (u32::from(src[1]) << 8) | u32::from(src[2]))
}
/// Reads 24-bit little-endian integer.
pub fn read_u24le(src: &[u8]) -> ByteIOResult<u32> {
    if src.len() < 3 { return Err(ByteIOError::ReadError); }
    Ok((u32::from(src[2]) << 16) | (u32::from(src[1]) << 8) | u32::from(src[0]))
}
/// Reads 32-bit big-endian floating point number.
pub fn read_f32be(src: &[u8]) -> ByteIOResult<f32> { Ok(f32::from_bits(read_u32be(src)?)) }
/// Reads 32-bit little-endian floating point number.
pub fn read_f32le(src: &[u8]) -> ByteIOResult<f32> { Ok(f32::from_bits(read_u32le(src)?)) }
/// Reads 64-bit big-endian floating point number.
pub fn read_f64be(src: &[u8]) -> ByteIOResult<f64> { Ok(f64::from_bits(read_u64be(src)?)) }
/// Reads 64-bit little-endian floating point number.
pub fn read_f64le(src: &[u8]) -> ByteIOResult<f64> { Ok(f64::from_bits(read_u64le(src)?)) }

macro_rules! write_int_func {
    ($s: ident, $inttype: ty, $size: expr, $which: ident) => {
/// Writes integer of certain size and endianness into byte buffer.
        pub fn $s(dst: &mut [u8], val: $inttype) -> ByteIOResult<()> {
            if dst.len() < $size { return Err(ByteIOError::WriteError); }
            unsafe {
                let val = val.$which();
                ptr::copy_nonoverlapping(&val as *const $inttype as *const u8, dst.as_mut_ptr(), std::mem::size_of::<$inttype>());
            }
            Ok(())
        }
    }
}

write_int_func!(write_u16be, u16, 2, to_be);
write_int_func!(write_u16le, u16, 2, to_le);
write_int_func!(write_u32be, u32, 4, to_be);
write_int_func!(write_u32le, u32, 4, to_le);
write_int_func!(write_u64be, u64, 8, to_be);
write_int_func!(write_u64le, u64, 8, to_le);

/// Writes 24-bit big-endian integer to the provided buffer.
///
/// # Example
///
/// ````
/// use nihav_core::io::byteio::write_u24be;
/// # use nihav_core::io::byteio::ByteIOResult;
///
/// # fn foo() -> ByteIOResult<()> {
/// let mut dst = [0u8; 3];
/// write_u24be(&mut dst, 0x010203)?;
/// // dst should contain [ 1, 2, 3] now
/// # Ok(())
/// # }
/// ````
pub fn write_u24be(dst: &mut [u8], val: u32) -> ByteIOResult<()> {
    if dst.len() < 3 { return Err(ByteIOError::WriteError); }
    dst[0] = (val >> 16) as u8;
    dst[1] = (val >>  8) as u8;
    dst[2] = (val >>  0) as u8;
    Ok(())
}
/// Writes 24-bit little-endian integer to the provided buffer.
pub fn write_u24le(dst: &mut [u8], val: u32) -> ByteIOResult<()> {
    if dst.len() < 3 { return Err(ByteIOError::WriteError); }
    dst[0] = (val >>  0) as u8;
    dst[1] = (val >>  8) as u8;
    dst[2] = (val >> 16) as u8;
    Ok(())
}
/// Writes 32-bit big-endian floating point number to the provided buffer.
pub fn write_f32be(dst: &mut [u8], val: f32) -> ByteIOResult<()> { write_u32be(dst, val.to_bits()) }
/// Writes 32-bit little-endian floating point number to the provided buffer.
pub fn write_f32le(dst: &mut [u8], val: f32) -> ByteIOResult<()> { write_u32le(dst, val.to_bits()) }
/// Writes 64-bit big-endian floating point number to the provided buffer.
pub fn write_f64be(dst: &mut [u8], val: f64) -> ByteIOResult<()> { write_u64be(dst, val.to_bits()) }
/// Writes 64-bit little-endian floating point number to the provided buffer.
pub fn write_f64le(dst: &mut [u8], val: f64) -> ByteIOResult<()> { write_u64le(dst, val.to_bits()) }

impl<'a> ByteReader<'a> {
    /// Constructs a new instance of bytestream reader.
    ///
    /// # Examples
    ///
    /// ````
    /// use nihav_core::io::byteio::{MemoryReader,ByteReader};
    /// # use nihav_core::io::byteio::ByteIOResult;
    ///
    /// # fn foo() -> ByteIOResult<()> {
    /// let memory: [u8; 4] = [ 0, 42, 42, 0 ];
    /// let mut mr = MemoryReader::new_read(&memory);
    /// let mut br = ByteReader::new(&mut mr);
    /// # Ok(())
    /// # }
    /// ````
    pub fn new(io: &'a mut ByteIO) -> Self { ByteReader { io } }

    /// Reads data into provided buffer. Partial read is treated as success.
    pub fn read_buf(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.read_buf(buf)
    }

    /// Reads data into provided buffer. Partial read is treated as success.
    pub fn read_buf_some(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.read_buf_some(buf)
    }

    /// Reads data into provided buffer but does not advance read position.
    pub fn peek_buf(&mut self, buf: &mut [u8])  -> ByteIOResult<usize> {
        self.io.peek_buf(buf)
    }

    /// Reads single byte from the stream.
    pub fn read_byte(&mut self) -> ByteIOResult<u8> {
        self.io.read_byte()
    }

    /// Returns the next byte value in the stream without advancing read position.
    pub fn peek_byte(&mut self) -> ByteIOResult<u8> {
        self.io.peek_byte()
    }

    /// Reads four-byte array from the stream.
    pub fn read_tag(&mut self)  -> ByteIOResult<[u8; 4]> {
        let mut buf = [0u8; 4];
        self.io.read_buf(&mut buf)?;
        Ok(buf)
    }

    /// Reads four-byte array from the stream without advancing read position.
    pub fn peek_tag(&mut self)  -> ByteIOResult<[u8; 4]> {
        let mut buf = [0u8; 4];
        self.io.peek_buf(&mut buf)?;
        Ok(buf)
    }

    /// Reads 16-bit big-endian integer from the stream.
    pub fn read_u16be(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_be)
    }

    /// Reads 16-bit big-endian integer from the stream without advancing read position.
    pub fn peek_u16be(&mut self) -> ByteIOResult<u16> {
        peek_int!(self, u16, 2, to_be)
    }

    /// Reads 24-bit big-endian integer from the stream.
    pub fn read_u24be(&mut self) -> ByteIOResult<u32> {
        let p16 = self.read_u16be()?;
        let p8 = self.read_byte()?;
        Ok((u32::from(p16) << 8) | u32::from(p8))
    }

    /// Reads 24-bit big-endian integer from the stream without advancing read position.
    pub fn peek_u24be(&mut self) -> ByteIOResult<u32> {
        let mut src: [u8; 3] = [0; 3];
        self.peek_buf(&mut src)?;
        Ok((u32::from(src[0]) << 16) | (u32::from(src[1]) << 8) | u32::from(src[2]))
    }

    /// Reads 32-bit big-endian integer from the stream.
    pub fn read_u32be(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_be)
    }

    /// Reads 32-bit big-endian integer from the stream without advancing read position.
    pub fn peek_u32be(&mut self) -> ByteIOResult<u32> {
        peek_int!(self, u32, 4, to_be)
    }

    /// Reads 64-bit big-endian integer from the stream.
    pub fn read_u64be(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_be)
    }

    /// Reads 64-bit big-endian integer from the stream without advancing read position.
    pub fn peek_u64be(&mut self) -> ByteIOResult<u64> {
        peek_int!(self, u64, 8, to_be)
    }

    /// Reads 32-bit big-endian floating point number from the stream.
    pub fn read_f32be(&mut self) -> ByteIOResult<f32> {
        Ok(f32::from_bits(self.read_u32be()?))
    }

    /// Reads 32-bit big-endian floating point number from the stream without advancing read position.
    pub fn peek_f32be(&mut self) -> ByteIOResult<f32> {
        Ok(f32::from_bits(self.peek_u32be()?))
    }

    /// Reads 64-bit big-endian floating point number from the stream.
    pub fn read_f64be(&mut self) -> ByteIOResult<f64> {
        Ok(f64::from_bits(self.read_u64be()?))
    }

    /// Reads 64-bit big-endian floating point number from the stream without advancing read position.
    pub fn peek_f64be(&mut self) -> ByteIOResult<f64> {
        Ok(f64::from_bits(self.peek_u64be()?))
    }

    /// Reads 16-bit little-endian integer from the stream.
    pub fn read_u16le(&mut self) -> ByteIOResult<u16> {
        read_int!(self, u16, 2, to_le)
    }

    /// Reads 16-bit little-endian integer from the stream without advancing read position.
    pub fn peek_u16le(&mut self) -> ByteIOResult<u16> {
        peek_int!(self, u16, 2, to_le)
    }

    /// Reads 24-bit little-endian integer from the stream.
    pub fn read_u24le(&mut self) -> ByteIOResult<u32> {
        let p8 = self.read_byte()?;
        let p16 = self.read_u16le()?;
        Ok((u32::from(p16) << 8) | u32::from(p8))
    }

    /// Reads 24-bit little-endian integer from the stream without advancing read position.
    pub fn peek_u24le(&mut self) -> ByteIOResult<u32> {
        let mut src: [u8; 3] = [0; 3];
        self.peek_buf(&mut src)?;
        Ok(u32::from(src[0]) | (u32::from(src[1]) << 8) | (u32::from(src[2]) << 16))
    }

    /// Reads 32-bit little-endian integer from the stream.
    pub fn read_u32le(&mut self) -> ByteIOResult<u32> {
        read_int!(self, u32, 4, to_le)
    }

    /// Reads 32-bit little-endian integer from the stream without advancing read position.
    pub fn peek_u32le(&mut self) -> ByteIOResult<u32> {
        peek_int!(self, u32, 4, to_le)
    }

    /// Reads 64-bit little-endian integer from the stream.
    pub fn read_u64le(&mut self) -> ByteIOResult<u64> {
        read_int!(self, u64, 8, to_le)
    }

    /// Reads 64-bit little-endian integer from the stream without advancing read position.
    pub fn peek_u64le(&mut self) -> ByteIOResult<u64> {
        peek_int!(self, u64, 8, to_le)
    }

    /// Reads 32-bit little-endian floating point number from the stream.
    pub fn read_f32le(&mut self) -> ByteIOResult<f32> {
        Ok(f32::from_bits(self.read_u32le()?))
    }

    /// Reads 32-bit little-endian floating point number from the stream without advancing read position.
    pub fn peek_f32le(&mut self) -> ByteIOResult<f32> {
        Ok(f32::from_bits(self.peek_u32le()?))
    }

    /// Reads 64-bit little-endian floating point number from the stream.
    pub fn read_f64le(&mut self) -> ByteIOResult<f64> {
        Ok(f64::from_bits(self.read_u64le()?))
    }

    /// Reads 64-bit little-endian floating point number from the stream without advancing read position.
    pub fn peek_f64le(&mut self) -> ByteIOResult<f64> {
        Ok(f64::from_bits(self.peek_u64le()?))
    }

    /// Skips requested number of bytes.
    pub fn read_skip(&mut self, len: usize) -> ByteIOResult<()> {
        if self.io.is_seekable() {
            self.io.seek(SeekFrom::Current(len as i64))?;
        } else {
            let mut ssize = len;
            let mut buf : [u8; 16] = [0; 16];
            let bref = &mut buf;
            while ssize > bref.len() {
                self.io.read_buf(bref)?;
                ssize -= bref.len();
            }
            while ssize > 0 {
                self.io.read_byte()?;
                ssize -= 1;
            }
        }
        Ok(())
    }

    /// Returns current read position.
    pub fn tell(&mut self) -> u64 {
        self.io.tell()
    }

    /// Seeks to the provided position.
    pub fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        self.io.seek(pos)
    }

    /// Tells whether this is end of stream.
    pub fn is_eof(&self) -> bool {
        self.io.is_eof()
    }

    /// Returns stream size or -1 if it is not known.
    pub fn size(&mut self) -> i64 {
        self.io.size()
    }

    /// Reports number of bytes left in the stream.
    pub fn left(&mut self) -> i64 {
        let size = self.io.size();
        if size == -1 { return -1; }
        size - (self.io.tell() as i64)
    }
}

impl<'a> MemoryReader<'a> {
    /// Constructs a new instance of `MemoryReader`.
    pub fn new_read(buf: &'a [u8]) -> Self {
        MemoryReader { buf, pos: 0 }
    }

    fn real_seek(&mut self, pos: i64) -> ByteIOResult<u64> {
        if pos < 0 || (pos as usize) > self.buf.len() {
            return Err(ByteIOError::WrongRange);
        }
        self.pos = pos as usize;
        Ok(pos as u64)
    }
}

impl<'a> ByteIO for MemoryReader<'a> {
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        if self.is_eof() { return Err(ByteIOError::EOF); }
        let res = self.buf[self.pos];
        self.pos += 1;
        Ok(res)
    }

    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        if self.is_eof() { return Err(ByteIOError::EOF); }
        Ok(self.buf[self.pos])
    }

    fn peek_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let copy_size = if self.buf.len() - self.pos < buf.len() { self.buf.len() - self.pos } else { buf.len() };
        if copy_size == 0 { return Err(ByteIOError::EOF); }
        let dst = &mut buf[0..copy_size];
        dst.copy_from_slice(&self.buf[self.pos..][..copy_size]);
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
        let cur_pos  = self.pos       as i64;
        let cur_size = self.buf.len() as i64;
        match pos {
            SeekFrom::Start(x)   => self.real_seek(x as i64),
            SeekFrom::Current(x) => self.real_seek(cur_pos + x),
            SeekFrom::End(x)     => self.real_seek(cur_size + x),
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn is_seekable(&mut self) -> bool {
        true
    }

    fn size(&mut self) -> i64 {
        self.buf.len() as i64
    }
}

impl<'a> FileReader<'a> {

    /// Constructs a new instance of `FileReader`.
    pub fn new_read(file: &'a mut File) -> Self {
        FileReader { file, eof : false }
    }
}

impl<'a> ByteIO for FileReader<'a> {
    fn read_byte(&mut self) -> ByteIOResult<u8> {
        let mut byte : [u8; 1] = [0];
        let ret = self.file.read(&mut byte);
        if ret.is_err() { return Err(ByteIOError::ReadError); }
        let sz = ret.unwrap();
        if sz == 0 { self.eof = true; return Err(ByteIOError::EOF); }
        Ok (byte[0])
    }

    fn peek_byte(&mut self) -> ByteIOResult<u8> {
        let b = self.read_byte()?;
        self.seek(SeekFrom::Current(-1))?;
        Ok(b)
    }

    fn read_buf(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let ret = self.file.read(buf);
        if ret.is_err() { return Err(ByteIOError::ReadError); }
        let sz = ret.unwrap();
        if sz < buf.len() { self.eof = true; return Err(ByteIOError::EOF); }
        Ok(sz)
    }

    fn read_buf_some(&mut self, buf: &mut [u8]) -> ByteIOResult<usize> {
        let ret = self.file.read(buf);
        if ret.is_err() { return Err(ByteIOError::ReadError); }
        let sz = ret.unwrap();
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

    fn is_eof(&self) -> bool {
        self.eof
    }

    fn is_seekable(&mut self) -> bool {
        true
    }

    fn size(&mut self) -> i64 {
        -1
    }
}

/// High-level bytestream writer.
///
/// User is supposed to create some writer implementing [`ByteIO`] trait e.g. [`MemoryWriter`] and use it to create `ByteWriter` which can be used for writing e.g. various integer types.
///
/// # Examples
///
/// ````
/// use nihav_core::io::byteio::{MemoryWriter,ByteWriter};
/// # use nihav_core::io::byteio::ByteIOResult;
///
/// # fn foo() -> ByteIOResult<()> {
/// let mut memory = [0u8; 4];
/// let mut mw = MemoryWriter::new_write(&mut memory);
/// let mut bw = ByteWriter::new(&mut mw);
/// let val = bw.write_u16be(42)?; // memory should be [ 0, 42, 0, 0 ]
/// let val = bw.write_u16le(42)?; // memory should be [ 0, 42, 42, 0 ]
/// # Ok(())
/// # }
/// ````
///
/// [`ByteIO`]: ./trait.ByteIO.html
/// [`MemoryWriter`]: ./struct.MemoryWriter.html
#[allow(dead_code)]
pub struct ByteWriter<'a> {
    io: &'a mut ByteIO,
}

/// Bytestream writer to memory.
pub struct MemoryWriter<'a> {
    buf:      &'a mut [u8],
    pos:      usize,
}

/// Bytestream writer to file.
pub struct FileWriter {
    file:     File,
}

impl<'a> ByteWriter<'a> {
    /// Constructs a new instance of `ByteWriter`.
    pub fn new(io: &'a mut ByteIO) -> Self { ByteWriter { io } }

    /// Writes byte array to the output.
    pub fn write_buf(&mut self, buf: &[u8])  -> ByteIOResult<()> {
        self.io.write_buf(buf)
    }

    /// Writes single byte to the output.
    pub fn write_byte(&mut self, val: u8) -> ByteIOResult<()> {
        let buf: [u8; 1] = [val];
        self.io.write_buf(&buf)
    }

    /// Writes 16-bit big-endian integer to the output.
    pub fn write_u16be(&mut self, val: u16) -> ByteIOResult<()> {
        let buf: [u8; 2] = [((val >> 8) & 0xFF) as u8, (val & 0xFF) as u8];
        self.io.write_buf(&buf)
    }

    /// Writes 16-bit little-endian integer to the output.
    pub fn write_u16le(&mut self, val: u16) -> ByteIOResult<()> {
        let buf: [u8; 2] = [(val & 0xFF) as u8, ((val >> 8) & 0xFF) as u8];
        self.io.write_buf(&buf)
    }

    /// Writes 24-bit big-endian integer to the output.
    pub fn write_u24be(&mut self, val: u32) -> ByteIOResult<()> {
        let buf: [u8; 3] = [((val >> 16) & 0xFF) as u8, ((val >> 8) & 0xFF) as u8, (val & 0xFF) as u8];
        self.write_buf(&buf)
    }

    /// Writes 24-bit little-endian integer to the output.
    pub fn write_u24le(&mut self, val: u32) -> ByteIOResult<()> {
        let buf: [u8; 3] = [(val & 0xFF) as u8, ((val >> 8) & 0xFF) as u8, ((val >> 16) & 0xFF) as u8];
        self.write_buf(&buf)
    }

    /// Writes 32-bit big-endian integer to the output.
    pub fn write_u32be(&mut self, val: u32) -> ByteIOResult<()> {
        self.write_u16be(((val >> 16) & 0xFFFF) as u16)?;
        self.write_u16be((val & 0xFFFF) as u16)
    }

    /// Writes 32-bit little-endian integer to the output.
    pub fn write_u32le(&mut self, val: u32) -> ByteIOResult<()> {
        self.write_u16le((val & 0xFFFF) as u16)?;
        self.write_u16le(((val >> 16) & 0xFFFF) as u16)
    }

    /// Writes 64-bit big-endian integer to the output.
    pub fn write_u64be(&mut self, val: u64) -> ByteIOResult<()> {
        self.write_u32be((val >> 32) as u32)?;
        self.write_u32be(val as u32)
    }

    /// Writes 64-bit little-endian integer to the output.
    pub fn write_u64le(&mut self, val: u64) -> ByteIOResult<()> {
        self.write_u32le(val as u32)?;
        self.write_u32le((val >> 32) as u32)
    }

    /// Writes 32-bit big-endian floating point number to the output.
    pub fn write_f32be(&mut self, val: f32) -> ByteIOResult<()> {
        self.write_u32be(val.to_bits())
    }

    /// Writes 32-bit little-endian floating point number to the output.
    pub fn write_f32le(&mut self, val: f32) -> ByteIOResult<()> {
        self.write_u32le(val.to_bits())
    }

    /// Writes 64-bit big-endian floating point number to the output.
    pub fn write_f64be(&mut self, val: f64) -> ByteIOResult<()> {
        self.write_u64be(val.to_bits())
    }

    /// Writes 64-bit little-endian floating point number to the output.
    pub fn write_f64le(&mut self, val: f64) -> ByteIOResult<()> {
        self.write_u64le(val.to_bits())
    }

    /// Reports the current write position.
    pub fn tell(&mut self) -> u64 {
        self.io.tell()
    }

    /// Seeks to the requested position.
    pub fn seek(&mut self, pos: SeekFrom) -> ByteIOResult<u64> {
        self.io.seek(pos)
    }

    /// Reports the amount of bytes the writer can still write (-1 if unknown).
    pub fn size_left(&mut self) -> i64 {
        let sz = self.io.size();
        if sz == -1 { return -1; }
        sz - (self.tell() as i64)
    }
}

impl<'a> MemoryWriter<'a> {

    /// Constructs a new instance of `MemoryWriter`.
    pub fn new_write(buf: &'a mut [u8]) -> Self {
        MemoryWriter { buf, pos: 0 }
    }

    fn real_seek(&mut self, pos: i64) -> ByteIOResult<u64> {
        if pos < 0 || (pos as usize) > self.buf.len() {
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
        if self.pos + buf.len() > self.buf.len() { return Err(ByteIOError::WriteError); }
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
        let cur_pos  = self.pos       as i64;
        let cur_size = self.buf.len() as i64;
        match pos {
            SeekFrom::Start(x)   => self.real_seek(x as i64),
            SeekFrom::Current(x) => self.real_seek(cur_pos + x),
            SeekFrom::End(x)     => self.real_seek(cur_size + x),
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn is_seekable(&mut self) -> bool {
        true
    }

    fn size(&mut self) -> i64 {
        self.buf.len() as i64
    }
}

impl FileWriter {
    /// Constructs a new instance of `FileWriter`.
    pub fn new_write(file: File) -> Self {
        FileWriter { file }
    }
}

impl ByteIO for FileWriter {
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

    fn is_eof(&self) -> bool {
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
        let mut file = File::open("assets/Misc/MaoMacha.asx").unwrap();
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
