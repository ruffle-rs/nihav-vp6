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
#[cfg(feature="decoder_truemotion3")]
mod truemotion3;
#[cfg(feature="decoder_truemotion4")]
mod truemotion4;
#[cfg(feature="decoder_truemotion5")]
mod truemotion5;
#[cfg(feature="decoder_truemotion6")]
mod truemotion6;
#[cfg(feature="decoder_truemotion7")]
mod truemotion7;

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
#[cfg(feature="decoder_truemotion3")]
    DecoderInfo { name: "truemotion3", get_decoder: truemotion3::get_decoder },
#[cfg(feature="decoder_truemotion4")]
    DecoderInfo { name: "truemotion4", get_decoder: truemotion4::get_decoder },
#[cfg(feature="decoder_truemotion5")]
    DecoderInfo { name: "truemotion5", get_decoder: truemotion5::get_decoder },
#[cfg(feature="decoder_truemotion6")]
    DecoderInfo { name: "truemotion6", get_decoder: truemotion6::get_decoder },
#[cfg(feature="decoder_truemotion7")]
    DecoderInfo { name: "truemotion7", get_decoder: truemotion7::get_decoder },

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
    for decoder in DUCK_CODECS.into_iter() {
        rd.add_decoder(decoder.clone());
    }
}
