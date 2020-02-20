extern crate nihav_core;
extern crate nihav_codec_support;

mod codecs;
pub use codecs::game_register_all_codecs;
mod demuxers;
pub use demuxers::game_register_all_demuxers;