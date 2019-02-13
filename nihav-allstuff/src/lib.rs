extern crate nihav_core;
extern crate nihav_commonfmt;
extern crate nihav_duck;
extern crate nihav_game;
extern crate nihav_indeo;
extern crate nihav_rad;
extern crate nihav_realmedia;

use nihav_core::codecs::RegisteredDecoders;
use nihav_core::demuxers::RegisteredDemuxers;

use nihav_commonfmt::codecs::generic_register_all_codecs;
use nihav_commonfmt::demuxers::generic_register_all_demuxers;

use nihav_duck::codecs::duck_register_all_codecs;

use nihav_game::codecs::game_register_all_codecs;
use nihav_game::demuxers::game_register_all_demuxers;

use nihav_indeo::codecs::indeo_register_all_codecs;

use nihav_rad::codecs::rad_register_all_codecs;
use nihav_rad::demuxers::rad_register_all_demuxers;

use nihav_realmedia::codecs::realmedia_register_all_codecs;
use nihav_realmedia::demuxers::realmedia_register_all_demuxers;

pub fn nihav_register_all_codecs(rd: &mut RegisteredDecoders) {
    generic_register_all_codecs(rd);
    duck_register_all_codecs(rd);
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
