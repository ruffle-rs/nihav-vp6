extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::unreadable_literal)]
#[allow(clippy::verbose_bit_mask)]
mod codecs;
#[allow(clippy::unreadable_literal)]
mod demuxers;
pub use crate::codecs::llaudio_register_all_decoders;
pub use crate::demuxers::llaudio_register_all_demuxers;
