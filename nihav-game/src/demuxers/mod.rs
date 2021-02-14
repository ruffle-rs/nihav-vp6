use nihav_core::demuxers::*;

#[allow(unused_macros)]
macro_rules! validate {
    ($a:expr) => { if !$a { println!("check failed at {}:{}", file!(), line!()); return Err(DemuxerError::InvalidData); } };
}

#[cfg(any(feature="demuxer_bmv",feature="demuxer_bmv3"))]
mod bmv;
#[cfg(any(feature="demuxer_fst",feature="demuxer_fcmp"))]
mod fst;
#[cfg(feature="demuxer_gdv")]
mod gdv;
#[cfg(feature="demuxer_imax")]
mod imax;
#[cfg(feature="demuxer_vmd")]
mod vmd;
#[cfg(feature="demuxer_vx")]
mod vx;

const GAME_DEMUXERS: &[&dyn DemuxerCreator] = &[
#[cfg(feature="demuxer_bmv")]
    &bmv::BMVDemuxerCreator {},
#[cfg(feature="demuxer_bmv3")]
    &bmv::BMV3DemuxerCreator {},
#[cfg(feature="demuxer_fcmp")]
    &fst::FCMPDemuxerCreator {},
#[cfg(feature="demuxer_fst")]
    &fst::FSTDemuxerCreator {},
#[cfg(feature="demuxer_gdv")]
    &gdv::GDVDemuxerCreator {},
#[cfg(feature="demuxer_imax")]
    &imax::IMAXDemuxerCreator {},
#[cfg(feature="demuxer_vmd")]
    &vmd::VMDDemuxerCreator {},
#[cfg(feature="demuxer_vx")]
    &vx::VXDemuxerCreator {},
];

/// Registers all available demuxers provided by this crate.
pub fn game_register_all_demuxers(rd: &mut RegisteredDemuxers) {
    for demuxer in GAME_DEMUXERS.iter() {
        rd.add_demuxer(*demuxer);
    }
}
