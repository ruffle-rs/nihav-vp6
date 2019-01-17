extern crate nihav_core;
extern crate nihav_game;
extern crate nihav_indeo;
extern crate nihav_realmedia;

use nihav_core::codecs::{RegisteredDecoders, core_register_all_codecs};
use nihav_core::demuxers::{RegisteredDemuxers, core_register_all_demuxers};

use nihav_game::codecs::game_register_all_codecs;
use nihav_game::demuxers::game_register_all_demuxers;

use nihav_indeo::codecs::indeo_register_all_codecs;

use nihav_realmedia::codecs::realmedia_register_all_codecs;
use nihav_realmedia::demuxers::realmedia_register_all_demuxers;

pub fn nihav_register_all_codecs(rd: &mut RegisteredDecoders) {
    core_register_all_codecs(rd);
    game_register_all_codecs(rd);
    indeo_register_all_codecs(rd);
    realmedia_register_all_codecs(rd);
}

pub fn nihav_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    core_register_all_demuxers(rd);
    game_register_all_demuxers(rd);
    realmedia_register_all_demuxers(rd);
}
