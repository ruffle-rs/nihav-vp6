extern crate nihav_core;

#[cfg(feature="decoders")]
pub mod codecs;

#[cfg(feature="demuxers")]
pub mod demuxers;

#[cfg(test)]
extern crate nihav_realmedia;
