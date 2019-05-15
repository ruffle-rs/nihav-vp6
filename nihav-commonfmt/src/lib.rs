extern crate nihav_core;

#[cfg(feature="decoders")]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::excessive_precision)]
pub mod codecs;

#[cfg(feature="demuxers")]
pub mod demuxers;

#[cfg(test)]
extern crate nihav_realmedia;
