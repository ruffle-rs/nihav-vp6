use frame::*;
use std::rc::Rc;
use std::cell::RefCell;
use io::byteio::ByteIOError;
use io::bitreader::BitReaderError;
use io::codebook::CodebookError;

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum DecoderError {
    TryAgain,
    InvalidData,
    ShortData,
    MissingReference,
    NotImplemented,
    Bug,
}

type DecoderResult<T> = Result<T, DecoderError>;

impl From<ByteIOError> for DecoderError {
    fn from(_: ByteIOError) -> Self { DecoderError::ShortData }
}

impl From<BitReaderError> for DecoderError {
    fn from(e: BitReaderError) -> Self {
        match e {
            BitReaderError::BitstreamEnd => DecoderError::ShortData,
            _ => DecoderError::InvalidData,
        }
    }
}

impl From<CodebookError> for DecoderError {
    fn from(_: CodebookError) -> Self { DecoderError::InvalidData }
}

#[allow(dead_code)]
struct HAMShuffler {
    lastframe: Option<NAVideoBuffer<u8>>,
}

impl HAMShuffler {
    #[allow(dead_code)]
    fn new() -> Self { HAMShuffler { lastframe: None } }
    #[allow(dead_code)]
    fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    fn add_frame(&mut self, buf: NAVideoBuffer<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    fn clone_ref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref mut frm) = self.lastframe {
            let newfrm = frm.copy_buffer();
            *frm = newfrm.clone();
            Some(newfrm)
        } else {
            None
        }
    }
    #[allow(dead_code)]
    fn get_output_frame(&mut self) -> Option<NAVideoBuffer<u8>> {
        match self.lastframe {
            Some(ref frm) => Some(frm.clone()),
            None => None,
        }
    }
}

#[allow(dead_code)]
struct IPShuffler {
    lastframe: Option<NAVideoBuffer<u8>>,
}

impl IPShuffler {
    #[allow(dead_code)]
    fn new() -> Self { IPShuffler { lastframe: None } }
    #[allow(dead_code)]
    fn clear(&mut self) { self.lastframe = None; }
    #[allow(dead_code)]
    fn add_frame(&mut self, buf: NAVideoBuffer<u8>) {
        self.lastframe = Some(buf);
    }
    #[allow(dead_code)]
    fn get_ref(&mut self) -> Option<NAVideoBuffer<u8>> {
        if let Some(ref frm) = self.lastframe {
            Some(frm.clone())
        } else {
            None
        }
    }
}

pub trait NADecoder {
    fn init(&mut self, info: Rc<NACodecInfo>) -> DecoderResult<()>;
    fn decode(&mut self, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
}

#[derive(Clone,Copy)]
pub struct DecoderInfo {
    name: &'static str,
    get_decoder: fn () -> Box<NADecoder>,
}

macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_indeo2")]
mod indeo2;
#[cfg(feature="decoder_indeo3")]
mod indeo3;
#[cfg(feature="decoder_pcm")]
mod pcm;

const DECODERS: &[DecoderInfo] = &[
#[cfg(feature="decoder_indeo2")]
    DecoderInfo { name: "indeo2", get_decoder: indeo2::get_decoder },
#[cfg(feature="decoder_indeo3")]
    DecoderInfo { name: "indeo3", get_decoder: indeo3::get_decoder },

#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "pcm", get_decoder: pcm::get_decoder },
];

pub fn find_decoder(name: &str) -> Option<fn () -> Box<NADecoder>> {
    for &dec in DECODERS {
        if dec.name == name {
            return Some(dec.get_decoder);
        }
    }
    None
}

#[cfg(test)]
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::io::prelude::*;

#[cfg(test)]
#[allow(dead_code)]
fn write_pgmyuv(pfx: &str, strno: usize, num: u64, frmref: NAFrameRef) {
    let frm = frmref.borrow();
    let name = format!("assets/{}out{:02}_{:04}.pgm", pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    let buf = frm.get_buffer().get_vbuf().unwrap();
    let (w, h) = buf.get_dimensions(0);
    let (w2, h2) = buf.get_dimensions(1);
    let tot_h = h + h2;
    let hdr = format!("P5\n{} {}\n255\n", w, tot_h);
    ofile.write_all(hdr.as_bytes()).unwrap();
    let dta = buf.get_data();
    let ls = buf.get_stride(0);
    let mut idx = 0;
    let mut idx2 = ls;
    let mut pad: Vec<u8> = Vec::with_capacity((w - w2 * 2) / 2);
    pad.resize((w - w2 * 2) / 2, 0xFF);
    for _ in 0..h {
        let line = &dta[idx..idx2];
        ofile.write_all(line).unwrap();
        idx  += ls;
        idx2 += ls;
    }
    let mut base1 = buf.get_offset(1);
    let stride1 = buf.get_stride(1);
    let mut base2 = buf.get_offset(2);
    let stride2 = buf.get_stride(2);
    for _ in 0..h2 {
        let bend1 = base1 + w2;
        let line = &dta[base1..bend1];
        ofile.write_all(line).unwrap();
        ofile.write_all(pad.as_slice()).unwrap();

        let bend2 = base2 + w2;
        let line = &dta[base2..bend2];
        ofile.write_all(line).unwrap();
        ofile.write_all(pad.as_slice()).unwrap();

        base1 += stride1;
        base2 += stride2;
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn write_palppm(pfx: &str, strno: usize, num: u64, frmref: NAFrameRef) {
    let frm = frmref.borrow();
    let name = format!("assets/{}out{:02}_{:04}.ppm", pfx, strno, num);
    let mut ofile = File::create(name).unwrap();
    let buf = frm.get_buffer().get_vbuf().unwrap();
    let (w, h) = buf.get_dimensions(0);
    let paloff = buf.get_offset(1);
    let hdr = format!("P6\n{} {}\n255\n", w, h);
    ofile.write_all(hdr.as_bytes()).unwrap();
    let dta = buf.get_data();
    let ls = buf.get_stride(0);
    let mut idx  = 0;
    let mut line: Vec<u8> = Vec::with_capacity(w * 3);
    line.resize(w * 3, 0);
    for _ in 0..h {
        let src = &dta[idx..(idx+w)];
        for x in 0..w {
            let pix = src[x] as usize;
            line[x * 3 + 0] = dta[paloff + pix * 3 + 2];
            line[x * 3 + 1] = dta[paloff + pix * 3 + 1];
            line[x * 3 + 2] = dta[paloff + pix * 3 + 0];
        }
        ofile.write_all(line.as_slice()).unwrap();
        idx  += ls;
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn write_sound(pfx: &str, strno: usize, frmref: NAFrameRef, first: bool) {
    let frm = frmref.borrow();
    let name = format!("assets/{}out{:02}.raw", pfx, strno);
    let mut file = if first { File::create(name).unwrap() } else { OpenOptions::new().write(true).append(true).open(name).unwrap() };
    let btype = frm.get_buffer();
    let _ = match btype {
        NABufferType::AudioU8(ref ab)      => file.write_all(ab.get_data().as_ref()),
        NABufferType::AudioPacked(ref ab)   => file.write_all(ab.get_data().as_ref()),
        _ => Ok(()),
    };
}
