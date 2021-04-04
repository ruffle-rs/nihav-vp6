extern crate nihav_core;
extern crate nihav_codec_support;

#[cfg(feature="decoders")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::collapsible_if)]
#[allow(clippy::excessive_precision)]
#[allow(clippy::identity_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::useless_let_if_seq)]
mod codecs;
#[cfg(feature="decoders")]
pub use crate::codecs::rad_register_all_decoders;

#[cfg(feature="demuxers")]
#[allow(clippy::comparison_chain)]
#[allow(clippy::cast_lossless)]
mod demuxers;
#[cfg(feature="demuxers")]
pub use crate::demuxers::rad_register_all_demuxers;
