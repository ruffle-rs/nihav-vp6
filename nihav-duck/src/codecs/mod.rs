use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(any(feature="decoder_vp3", feature="decoder_vp4", feature="decoder_vp5", feature="decoder_vp6", feature="decoder_vp7"))]
#[macro_use]
#[allow(clippy::erasing_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::useless_let_if_seq)]
mod vpcommon;
#[cfg(any(feature="decoder_vp5", feature="decoder_vp6"))]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::useless_let_if_seq)]
#[allow(clippy::too_many_arguments)]
mod vp56;
#[cfg(feature="decoder_vp6")]
mod vp6data;
#[cfg(feature="decoder_vp6")]
#[allow(clippy::needless_range_loop)]
mod vp6;
