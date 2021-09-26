
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DecoderError::InvalidData); } };
}

#[macro_use]
#[allow(clippy::erasing_op)]
#[allow(clippy::needless_range_loop)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::useless_let_if_seq)]
pub mod vpcommon;
#[allow(clippy::needless_range_loop)]
#[allow(clippy::useless_let_if_seq)]
#[allow(clippy::too_many_arguments)]
mod vp56;
mod vp6data;
#[allow(clippy::needless_range_loop)]
pub mod vp6;
