use nihav_core::codecs::*;

#[allow(unused_macros)]
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(any(feature="decoder_vivo1", feature="decoder_vivo2"))]
mod vivo;
#[cfg(feature="decoder_g723_1")]
mod g723_1;
#[cfg(feature="decoder_siren")]
mod siren;

const VIVO_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_vivo1")]
    DecoderInfo { name: "vivo1", get_decoder: vivo::get_decoder },
#[cfg(feature="decoder_vivo2")]
    DecoderInfo { name: "vivo2", get_decoder: vivo::get_decoder },
#[cfg(feature="decoder_g723_1")]
    DecoderInfo { name: "g723.1", get_decoder: g723_1::get_decoder },
#[cfg(feature="decoder_siren")]
    DecoderInfo { name: "siren", get_decoder: siren::get_decoder },
];

/// Registers all available codecs provided by this crate.
pub fn vivo_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in VIVO_CODECS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
