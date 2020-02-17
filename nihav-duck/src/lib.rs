extern crate nihav_core;

mod codecs;

pub use codecs::duck_register_all_codecs;

#[cfg(test)]
extern crate nihav_commonfmt;
