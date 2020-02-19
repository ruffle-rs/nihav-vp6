//! DSP routines.
#[cfg(feature="dct")]
#[allow(clippy::erasing_op)]
pub mod dct;
#[cfg(feature="fft")]
#[allow(clippy::erasing_op)]
pub mod fft;
#[cfg(feature="mdct")]
pub mod mdct;
#[cfg(feature="dsp_window")]
pub mod window;
