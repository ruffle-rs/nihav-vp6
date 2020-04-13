//! Core functionality of NihAV intended to be used by both crates implementing format support and users.
#[cfg(feature="decoders")]
#[allow(clippy::cast_lossless)]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
pub mod codecs;

#[cfg(feature="compr")]
pub mod compr;

#[cfg(feature="demuxers")]
pub mod demuxers;

#[allow(clippy::too_many_arguments)]
pub mod formats;
pub mod frame;
#[allow(clippy::too_many_arguments)]
pub mod io;
pub mod refs;
pub mod reorder;
pub mod scale;
pub mod soundcvt;
