extern crate nihav_core;
extern crate nihav_codec_support;

mod codecs;

pub use codecs::duck_register_all_codecs;

#[cfg(test)]
extern crate nihav_commonfmt;
