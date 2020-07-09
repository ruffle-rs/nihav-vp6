extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::collapsible_if)]
#[allow(clippy::identity_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::useless_let_if_seq)]
#[allow(clippy::verbose_bit_mask)]
mod codecs;

pub use crate::codecs::indeo_register_all_decoders;

#[cfg(test)]
extern crate nihav_commonfmt;