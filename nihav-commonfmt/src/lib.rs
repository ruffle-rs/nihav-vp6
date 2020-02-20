extern crate nihav_core;
extern crate nihav_codec_support;
extern crate nihav_registry;

#[cfg(feature="decoders")]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::excessive_precision)]
mod codecs;

#[cfg(feature="decoders")]
pub use codecs::generic_register_all_codecs;

#[cfg(feature="demuxers")]
mod demuxers;
#[cfg(feature="demuxers")]
pub use demuxers::generic_register_all_demuxers;

#[cfg(test)]
extern crate nihav_realmedia;
