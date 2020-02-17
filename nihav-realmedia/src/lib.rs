extern crate nihav_core;

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
pub use codecs::realmedia_register_all_codecs;

#[cfg(feature="demuxers")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::useless_let_if_seq)]
mod demuxers;
#[cfg(feature="demuxers")]
pub use demuxers::realmedia_register_all_demuxers;