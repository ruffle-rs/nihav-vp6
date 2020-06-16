extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::collapsible_if)]
#[allow(clippy::excessive_precision)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::useless_let_if_seq)]
mod codecs;
pub use crate::codecs::game_register_all_codecs;
#[allow(clippy::collapsible_if)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::unreadable_literal)]
mod demuxers;
pub use crate::demuxers::game_register_all_demuxers;
