//! Core functionality of NihAV intended to be used by both crates implementing format support and users.
#[allow(clippy::cast_lossless)]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::unreadable_literal)]
pub mod codecs;

#[allow(clippy::needless_range_loop)]
#[allow(clippy::too_many_arguments)]
pub mod formats;
pub mod frame;
#[allow(clippy::too_many_arguments)]
pub mod io;
pub mod refs;
