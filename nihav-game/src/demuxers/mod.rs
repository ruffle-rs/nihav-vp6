use nihav_core::demuxers::*;

#[allow(unused_macros)]
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DemuxerError::InvalidData); } };
}

#[cfg(feature="demuxer_gdv")]
mod gdv;

const GAME_DEMUXERS: &[&'static DemuxerCreator] = &[
#[cfg(feature="demuxer_gdv")]
    &gdv::GDVDemuxerCreator {},
];

pub fn game_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    for demuxer in GAME_DEMUXERS.into_iter() {
        rd.add_demuxer(*demuxer);
    }
}
