use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_clearvideo")]
mod clearvideo;

#[cfg(feature="decoder_aac")]
mod aac;
#[cfg(feature="decoder_atrac3")]
mod atrac3;
#[cfg(feature="decoder_pcm")]
mod pcm;
#[cfg(feature="decoder_sipro")]
mod sipro;
#[cfg(feature="decoder_ts102366")]
mod ts102366;

const DECODERS: &[DecoderInfo] = &[
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo", get_decoder: clearvideo::get_decoder },
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo_rm", get_decoder: clearvideo::get_decoder_rm },

#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "pcm", get_decoder: pcm::get_decoder },
#[cfg(feature="decoder_sipro")]
    DecoderInfo { name: "sipro", get_decoder: sipro::get_decoder },
#[cfg(feature="decoder_ts102366")]
    DecoderInfo { name: "ac3", get_decoder: ts102366::get_decoder },
#[cfg(feature="decoder_atrac3")]
    DecoderInfo { name: "atrac3", get_decoder: atrac3::get_decoder },
#[cfg(feature="decoder_aac")]
    DecoderInfo { name: "aac", get_decoder: aac::get_decoder },
];

/// Registers all available codecs provided by this crate.
pub fn generic_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in DECODERS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
