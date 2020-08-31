use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_ape")]
pub mod ape;
#[cfg(feature="decoder_ape")]
mod apepred;
#[cfg(feature="decoder_ape")]
mod apereader;

#[cfg(feature="decoder_flac")]
pub mod flac;

#[cfg(feature="decoder_tta")]
pub mod tta;

#[cfg(feature="decoder_wavpack")]
pub mod wavpack;

const LL_AUDIO_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_ape")]
    DecoderInfo { name: "ape", get_decoder: ape::get_decoder },
#[cfg(feature="decoder_flac")]
    DecoderInfo { name: "flac", get_decoder: flac::get_decoder },
#[cfg(feature="decoder_tta")]
    DecoderInfo { name: "tta", get_decoder: tta::get_decoder },
#[cfg(feature="decoder_wavpack")]
    DecoderInfo { name: "wavpack", get_decoder: wavpack::get_decoder },
];

/// Registers all available codecs provided by this crate.
pub fn llaudio_register_all_decoders(rd: &mut RegisteredDecoders) {
    for decoder in LL_AUDIO_CODECS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
