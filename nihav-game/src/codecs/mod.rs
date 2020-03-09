use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_bmv")]
pub mod bmv;
#[cfg(feature="decoder_bmv3")]
pub mod bmv3;
#[cfg(feature="decoder_gdvvid")]
pub mod gremlinvideo;
#[cfg(feature="decoder_midivid")]
pub mod midivid;
#[cfg(feature="decoder_midivid3")]
pub mod midivid3;
#[cfg(feature="decoder_vmd")]
pub mod vmd;

const GAME_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_gdvvid")]
    DecoderInfo { name: "gdv-audio", get_decoder: gremlinvideo::get_decoder_audio },
#[cfg(feature="decoder_gdvvid")]
    DecoderInfo { name: "gdv-video", get_decoder: gremlinvideo::get_decoder_video },
#[cfg(feature="decoder_bmv")]
    DecoderInfo { name: "bmv-audio", get_decoder: bmv::get_decoder_audio },
#[cfg(feature="decoder_bmv")]
    DecoderInfo { name: "bmv-video", get_decoder: bmv::get_decoder_video },
#[cfg(feature="decoder_bmv3")]
    DecoderInfo { name: "bmv3-audio", get_decoder: bmv3::get_decoder_audio },
#[cfg(feature="decoder_bmv3")]
    DecoderInfo { name: "bmv3-video", get_decoder: bmv3::get_decoder_video },
#[cfg(feature="decoder_vmd")]
    DecoderInfo { name: "vmd-audio", get_decoder: vmd::get_decoder_audio },
#[cfg(feature="decoder_vmd")]
    DecoderInfo { name: "vmd-video", get_decoder: vmd::get_decoder_video },
#[cfg(feature="decoder_midivid")]
    DecoderInfo { name: "midivid", get_decoder: midivid::get_decoder_video },
#[cfg(feature="decoder_midivid3")]
    DecoderInfo { name: "midivid3", get_decoder: midivid3::get_decoder_video },
];

/// Registers all available codecs provided by this crate.
pub fn game_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in GAME_CODECS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
