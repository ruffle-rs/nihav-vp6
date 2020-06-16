extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::collapsible_if)]
#[allow(clippy::excessive_precision)]
#[allow(clippy::identity_op)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::verbose_bit_mask)]
mod codecs;

pub use crate::codecs::duck_register_all_codecs;

#[cfg(test)]
extern crate nihav_commonfmt;
