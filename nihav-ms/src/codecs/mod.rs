use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_msvideo1")]
pub mod msvideo1;

#[cfg(feature="decoder_ima_adpcm_ms")]
pub mod imaadpcm;

#[cfg(feature="decoder_ms_adpcm")]
pub mod msadpcm;

const MS_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_msvideo1")]
    DecoderInfo { name: "msvideo1", get_decoder: msvideo1::get_decoder },
#[cfg(feature="decoder_ima_adpcm_ms")]
    DecoderInfo { name: "ima-adpcm-ms", get_decoder: imaadpcm::get_decoder },
#[cfg(feature="decoder_ms_adpcm")]
    DecoderInfo { name: "ms-adpcm", get_decoder: msadpcm::get_decoder },
];

/// Registers all available codecs provided by this crate.
pub fn ms_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in MS_CODECS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
