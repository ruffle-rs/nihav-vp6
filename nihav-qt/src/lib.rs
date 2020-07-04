extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::single_match)]
mod codecs;
pub use crate::codecs::qt_register_all_codecs;
