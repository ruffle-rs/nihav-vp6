#[cfg(any(feature="decoder_realvideo3", feature="decoder_realvideo4"))]
mod rv3040;
#[cfg(any(feature="decoder_realvideo3", feature="decoder_realvideo4"))]
mod rv34codes;
#[cfg(any(feature="decoder_realvideo3", feature="decoder_realvideo4"))]
mod rv34dsp;

#[cfg(feature="decoder_realvideo3")]
pub mod rv30;
#[cfg(feature="decoder_realvideo3")]
pub mod rv30dsp;
#[cfg(feature="decoder_realvideo4")]
pub mod rv40;
#[cfg(feature="decoder_realvideo4")]
pub mod rv40dsp;
#[cfg(feature="decoder_realvideo6")]
pub mod rv60;
#[cfg(feature="decoder_realvideo6")]
pub mod rv60codes;
#[cfg(feature="decoder_realvideo6")]
pub mod rv60dsp;

#[cfg(feature="decoder_realaudio144")]
pub mod ra144;
#[cfg(feature="decoder_realaudio288")]
pub mod ra288;
#[cfg(feature="decoder_cook")]
pub mod cook;
#[cfg(feature="decoder_ralf")]
pub mod ralf;

