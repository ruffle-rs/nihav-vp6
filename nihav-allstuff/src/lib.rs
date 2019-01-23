extern crate nihav_core;
extern crate nihav_commonfmt;
extern crate nihav_game;
extern crate nihav_indeo;
extern crate nihav_rad;
extern crate nihav_realmedia;

use nihav_core::codecs::RegisteredDecoders;
use nihav_core::demuxers::RegisteredDemuxers;

use nihav_commonfmt::codecs::generic_register_all_codecs;
use nihav_commonfmt::demuxers::generic_register_all_demuxers;

use nihav_game::codecs::game_register_all_codecs;
use nihav_game::demuxers::game_register_all_demuxers;

use nihav_indeo::codecs::indeo_register_all_codecs;

use nihav_rad::codecs::rad_register_all_codecs;
use nihav_rad::demuxers::rad_register_all_demuxers;

use nihav_realmedia::codecs::realmedia_register_all_codecs;
use nihav_realmedia::demuxers::realmedia_register_all_demuxers;

pub fn nihav_register_all_codecs(rd: &mut RegisteredDecoders) {
    generic_register_all_codecs(rd);
    game_register_all_codecs(rd);
    indeo_register_all_codecs(rd);
    rad_register_all_codecs(rd);
    realmedia_register_all_codecs(rd);
}

pub fn nihav_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    generic_register_all_demuxers(rd);
    game_register_all_demuxers(rd);
    rad_register_all_demuxers(rd);
    realmedia_register_all_demuxers(rd);
}
