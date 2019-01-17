use nihav_core::demuxers::*;


macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DemuxerError::InvalidData); } };
}

#[cfg(feature="demuxer_avi")]
mod avi;

const DEMUXERS: &[&'static DemuxerCreator] = &[
#[cfg(feature="demuxer_avi")]
    &avi::AVIDemuxerCreator {},
];

pub fn generic_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    for demuxer in DEMUXERS.into_iter() {
        rd.add_demuxer(*demuxer);
    }
}
