use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_bmv")]
pub mod bmv;
#[cfg(feature="decoder_gdvvid")]
pub mod gremlinvideo;

const GAME_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_gdvvid")]
    DecoderInfo { name: "gdv-audio", get_decoder: gremlinvideo::get_decoder_audio },
#[cfg(feature="decoder_gdvvid")]
    DecoderInfo { name: "gdv-video", get_decoder: gremlinvideo::get_decoder_video },
#[cfg(feature="decoder_bmv")]
    DecoderInfo { name: "bmv-audio", get_decoder: bmv::get_decoder_audio },
#[cfg(feature="decoder_bmv")]
    DecoderInfo { name: "bmv-video", get_decoder: bmv::get_decoder_video },
];

pub fn game_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in GAME_CODECS.into_iter() {
        rd.add_decoder(decoder.clone());
    }
}
