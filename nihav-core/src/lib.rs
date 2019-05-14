#[cfg(feature="decoders")]
#[allow(clippy::unreadable_literal)]
pub mod codecs;

#[cfg(feature="demuxers")]
pub mod demuxers;

pub mod formats;
pub mod frame;
pub mod io;
pub mod refs;
pub mod register;
#[allow(clippy::unreadable_literal)]
pub mod detect;
pub mod scale;

#[cfg(feature="dsp")]
#[allow(clippy::unreadable_literal)]
pub mod dsp;

pub mod test;
