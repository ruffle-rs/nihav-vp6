extern crate nihav_core;
extern crate nihav_codec_support;

mod codecs;
pub use crate::codecs::game_register_all_codecs;
mod demuxers;
pub use crate::demuxers::game_register_all_demuxers;