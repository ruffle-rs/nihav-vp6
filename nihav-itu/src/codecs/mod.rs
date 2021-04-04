use nihav_core::codecs::*;

macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[allow(clippy::too_many_arguments)]
#[cfg(feature="decoder_h264")]
mod h264;

const ITU_CODECS: &[DecoderInfo] = &[
#[cfg(feature="decoder_h264")]
    DecoderInfo { name: "h264", get_decoder: h264::get_decoder },
];

/// Registers all available codecs provided by this crate.
pub fn itu_register_all_decoders(rd: &mut RegisteredDecoders) {
    for decoder in ITU_CODECS.iter() {
        rd.add_decoder(*decoder);
    }
}
