use nihav_core::demuxers::*;


#[allow(unused_macros)]
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DemuxerError::InvalidData); } };
}

#[cfg(feature="demuxer_avi")]
#[allow(clippy::cast_lossless)]
mod avi;
#[cfg(feature="demuxer_mov")]
mod mov;

const DEMUXERS: &[&DemuxerCreator] = &[
#[cfg(feature="demuxer_avi")]
    &avi::AVIDemuxerCreator {},
#[cfg(feature="demuxer_mov")]
    &mov::MOVDemuxerCreator {},
];

/// Registers all available demuxers provided by this crate.
pub fn generic_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    for demuxer in DEMUXERS.iter() {
        rd.add_demuxer(*demuxer);
    }
}
