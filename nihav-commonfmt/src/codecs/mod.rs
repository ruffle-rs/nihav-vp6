use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[cfg(feature="decoder_cinepak")]
mod cinepak;
#[cfg(feature="decoder_clearvideo")]
mod clearvideo;

#[cfg(feature="decoder_aac")]
#[allow(clippy::manual_memcpy)]
#[allow(clippy::useless_let_if_seq)]
mod aac;
#[cfg(feature="decoder_atrac3")]
#[allow(clippy::identity_op)]
#[allow(clippy::useless_let_if_seq)]
mod atrac3;
#[cfg(any(feature="decoder_pcm",feature="encoder_pcm"))]
mod pcm;
#[cfg(feature="decoder_sipro")]
#[allow(clippy::collapsible_if)]
#[allow(clippy::identity_op)]
#[allow(clippy::manual_memcpy)]
mod sipro;
#[cfg(feature="decoder_ts102366")]
mod ts102366;

const DECODERS: &[DecoderInfo] = &[
#[cfg(feature="decoder_cinepak")]
    DecoderInfo { name: "cinepak", get_decoder: cinepak::get_decoder },
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo", get_decoder: clearvideo::get_decoder },
#[cfg(feature="decoder_clearvideo")]
    DecoderInfo { name: "clearvideo_rm", get_decoder: clearvideo::get_decoder_rm },

#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "pcm", get_decoder: pcm::get_decoder },
#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "alaw", get_decoder: pcm::get_a_law_decoder },
#[cfg(feature="decoder_pcm")]
    DecoderInfo { name: "ulaw", get_decoder: pcm::get_mu_law_decoder },
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
pub fn generic_register_all_decoders(rd: &mut RegisteredDecoders) {
    for decoder in DECODERS.iter() {
        rd.add_decoder(decoder.clone());
    }
}

#[cfg(feature="encoder_cinepak")]
mod cinepakenc;

const ENCODERS: &[EncoderInfo] = &[
#[cfg(feature="encoder_cinepak")]
    EncoderInfo { name: "cinepak", get_encoder: cinepakenc::get_encoder },

#[cfg(feature="encoder_pcm")]
    EncoderInfo { name: "pcm", get_encoder: pcm::get_encoder },
];

/// Registers all available encoders provided by this crate.
pub fn generic_register_all_encoders(re: &mut RegisteredEncoders) {
    for encoder in ENCODERS.iter() {
        re.add_encoder(encoder.clone());
    }
}

