//! Core functionality of NihAV intended to be used by both crates implementing format support and users.
#[cfg(feature="decoders")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
pub mod codecs;

#[cfg(feature="demuxers")]
pub mod demuxers;

#[allow(clippy::too_many_arguments)]
pub mod formats;
pub mod frame;
#[allow(clippy::too_many_arguments)]
pub mod io;
pub mod refs;
pub mod register;
#[allow(clippy::unreadable_literal)]
pub mod detect;
pub mod reorder;
pub mod scale;
pub mod soundcvt;

#[cfg(feature="dsp")]
#[allow(clippy::excessive_precision)]
#[allow(clippy::identity_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::unreadable_literal)]
pub mod dsp;

pub mod data;

pub mod test;
