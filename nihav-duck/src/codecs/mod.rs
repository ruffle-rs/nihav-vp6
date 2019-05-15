use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_truemotion1")]
mod truemotion1;
#[cfg(feature="decoder_truemotionrt")]
mod truemotionrt;
#[cfg(feature="decoder_truemotion2")]
mod truemotion2;
#[cfg(feature="decoder_truemotion2x")]
mod truemotion2x;
#[cfg(any(feature="decoder_vp3", feature="decoder_vp4"))]
mod vp3;
#[cfg(any(feature="decoder_vp5", feature="decoder_vp6"))]
mod vp56;
#[cfg(feature="decoder_vp7")]
mod vp7;

#[cfg(any(feature="decoder_dk3_adpcm", feature="decoder_dk4_adpcm"))]
mod dkadpcm;
#[cfg(feature="decoder_on2avc")]
mod on2avc;

const DUCK_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_truemotion1")]
    DecoderInfo { name: "truemotion1", get_decoder: truemotion1::get_decoder },
#[cfg(feature="decoder_truemotionrt")]
    DecoderInfo { name: "truemotionrt", get_decoder: truemotionrt::get_decoder },
#[cfg(feature="decoder_truemotion2")]
    DecoderInfo { name: "truemotion2", get_decoder: truemotion2::get_decoder },
#[cfg(feature="decoder_truemotion2x")]
    DecoderInfo { name: "truemotion2x", get_decoder: truemotion2x::get_decoder },
#[cfg(feature="decoder_vp3")]
    DecoderInfo { name: "vp3", get_decoder: vp3::get_decoder_vp3 },
#[cfg(feature="decoder_vp4")]
    DecoderInfo { name: "vp4", get_decoder: vp3::get_decoder_vp4 },
#[cfg(feature="decoder_vp5")]
    DecoderInfo { name: "vp5", get_decoder: vp56::get_decoder_vp5 },
#[cfg(feature="decoder_vp6")]
    DecoderInfo { name: "vp6", get_decoder: vp56::get_decoder_vp6 },
#[cfg(feature="decoder_vp7")]
    DecoderInfo { name: "vp7", get_decoder: vp7::get_decoder },

#[cfg(feature="decoder_dk3_adpcm")]
    DecoderInfo { name: "adpcm-dk3", get_decoder: dkadpcm::get_decoder_dk3 },
#[cfg(feature="decoder_dk4_adpcm")]
    DecoderInfo { name: "adpcm-dk4", get_decoder: dkadpcm::get_decoder_dk4 },
#[cfg(feature="decoder_on2avc")]
    DecoderInfo { name: "on2avc-500", get_decoder: on2avc::get_decoder_500 },
#[cfg(feature="decoder_on2avc")]
    DecoderInfo { name: "on2avc-501", get_decoder: on2avc::get_decoder_501 },
];

pub fn duck_register_all_codecs(rd: &mut RegisteredDecoders) {
    for decoder in DUCK_CODECS.iter() {
        rd.add_decoder(decoder.clone());
    }
}
