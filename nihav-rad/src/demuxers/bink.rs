use nihav_core::demuxers::*;
use nihav_core::io::byteio::ByteReader;

pub struct BinkDemuxerCreator { }

impl DemuxerCreator for BinkDemuxerCreator {
    fn new_demuxer<'a>(&self, br: &'a mut ByteReader<'a>) -> Box<DemuxCore<'a> + 'a> {
        unimplemented!("");//Box::new(GremlinVideoDemuxer::new(br))
    }
    fn get_name(&self) -> &'static str { "bink" }
}

