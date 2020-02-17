//! Umbrella crate to register decoders and demuxers from all known NihAV crates.
extern crate nihav_core;
extern crate nihav_commonfmt;
extern crate nihav_duck;
extern crate nihav_game;
extern crate nihav_indeo;
extern crate nihav_rad;
extern crate nihav_realmedia;

use nihav_core::codecs::RegisteredDecoders;
use nihav_core::demuxers::RegisteredDemuxers;

use nihav_commonfmt::generic_register_all_codecs;
use nihav_commonfmt::generic_register_all_demuxers;

use nihav_duck::duck_register_all_codecs;

use nihav_game::game_register_all_codecs;
use nihav_game::game_register_all_demuxers;

use nihav_indeo::indeo_register_all_codecs;

use nihav_rad::rad_register_all_codecs;
use nihav_rad::rad_register_all_demuxers;

use nihav_realmedia::realmedia_register_all_codecs;
use nihav_realmedia::realmedia_register_all_demuxers;

/// Registers all known decoders.
pub fn nihav_register_all_codecs(rd: &mut RegisteredDecoders) {
    generic_register_all_codecs(rd);
    duck_register_all_codecs(rd);
    game_register_all_codecs(rd);
    indeo_register_all_codecs(rd);
    rad_register_all_codecs(rd);
    realmedia_register_all_codecs(rd);
}

/// Registers all known demuxers.
pub fn nihav_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    generic_register_all_demuxers(rd);
    game_register_all_demuxers(rd);
    rad_register_all_demuxers(rd);
    realmedia_register_all_demuxers(rd);
}

#[cfg(test)]
mod test {
    use super::*;
    use nihav_core::register::get_codec_description;

    #[test]
    fn test_descriptions() {
        let mut rd = RegisteredDecoders::new();
        nihav_register_all_codecs(&mut rd);
        let mut has_missing = false;
        for dec in rd.iter() {
            print!("decoder {} - ", dec.name);
            let ret = get_codec_description(dec.name);
            if let Some(desc) = ret {
                println!("{}", desc);
            } else {
                println!("missing!");
                has_missing = true;
            }
        }
        assert!(!has_missing);
    }
}
