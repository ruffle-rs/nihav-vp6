extern crate nihav_core;
extern crate nihav_codec_support;

#[cfg(feature="decoders")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::collapsible_if)]
#[allow(clippy::comparison_chain)]
#[allow(clippy::excessive_precision)]
#[allow(clippy::identity_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::single_match)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::useless_let_if_seq)]
mod codecs;
#[cfg(feature="decoders")]
pub use crate::codecs::realmedia_register_all_decoders;

#[cfg(feature="demuxers")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::useless_let_if_seq)]
mod demuxers;
#[cfg(feature="demuxers")]
pub use crate::demuxers::realmedia_register_all_demuxers;
