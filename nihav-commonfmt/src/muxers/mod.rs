use nihav_core::muxers::*;

#[cfg(feature="muxer_avi")]
mod avi;

const MUXERS: &[&MuxerCreator] = &[
#[cfg(feature="muxer_avi")]
    &avi::AVIMuxerCreator {},
];

pub fn generic_register_all_muxers(rm: &mut RegisteredMuxers) {
    for muxer in MUXERS.iter() {
        rm.add_muxer(*muxer);
    }
}
