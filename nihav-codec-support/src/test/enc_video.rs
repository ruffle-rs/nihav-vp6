use std::fs::File;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_core::demuxers::*;
use nihav_core::muxers::*;
use nihav_core::scale::*;
use nihav_core::soundcvt::*;

pub struct DecoderTestParams {
    pub demuxer:        &'static str,
    pub in_name:        &'static str,
    pub limit:          Option<u64>,
    pub stream_type:    StreamType,
    pub dmx_reg:        RegisteredDemuxers,
    pub dec_reg:        RegisteredDecoders,
}

pub struct EncoderTestParams {
    pub muxer:          &'static str,
    pub enc_name:       &'static str,
    pub out_name:       &'static str,
    pub mux_reg:        RegisteredMuxers,
    pub enc_reg:        RegisteredEncoders,
}

pub fn test_encoding_to_file(dec_config: &DecoderTestParams, enc_config: &EncoderTestParams, mut enc_params: EncodeParameters) {
    let dmx_f = dec_config.dmx_reg.find_demuxer(dec_config.demuxer).unwrap();
    let mut file = File::open(dec_config.in_name).unwrap();
    let mut fr = FileReader::new_read(&mut file);
    let mut br = ByteReader::new(&mut fr);
    let mut dmx = create_demuxer(dmx_f, &mut br).unwrap();

    let in_stream = dmx.get_streams().find(|str| str.get_media_type() == dec_config.stream_type).unwrap();
    let in_stream_id = in_stream.id;
    let decfunc = dec_config.dec_reg.find_decoder(in_stream.get_info().get_name()).unwrap();
    let mut dec = (decfunc)();
    let mut dsupp = Box::new(NADecoderSupport::new());
    dec.init(&mut dsupp, in_stream.get_info()).unwrap();

    let mut out_sm = StreamManager::new();
    enc_params.tb_num = in_stream.tb_num;
    enc_params.tb_den = in_stream.tb_den;

    if let (NACodecTypeInfo::Video(ref mut vinfo), Some(ref_vinfo)) = (&mut enc_params.format, in_stream.get_info().get_properties().get_video_info()) {
        if vinfo.width == 0 {
            vinfo.width  = ref_vinfo.width;
            vinfo.height = ref_vinfo.height;
        }
    }
    let mut dst_chmap = NAChannelMap::new();
    if let (NACodecTypeInfo::Audio(ref mut ainfo), Some(ref_ainfo)) = (&mut enc_params.format, in_stream.get_info().get_properties().get_audio_info()) {
        if ainfo.sample_rate == 0 {
            ainfo.sample_rate = ref_ainfo.sample_rate;
        }
        if ainfo.channels == 0 {
            ainfo.channels = ref_ainfo.channels;
        }
        match ainfo.channels {
            1 => {
                dst_chmap.add_channel(NAChannelType::C);
            },
            2 => {
                dst_chmap.add_channel(NAChannelType::L);
                dst_chmap.add_channel(NAChannelType::R);
            },
            _ => panic!("cannot guess channel map"),
        }
    }

    let encfunc = enc_config.enc_reg.find_encoder(enc_config.enc_name).unwrap();
    let mut encoder = (encfunc)();
    let out_str = encoder.init(0, enc_params).unwrap();
    out_sm.add_stream(NAStream::clone(&out_str));
    
    let mux_f = enc_config.mux_reg.find_muxer(enc_config.muxer).unwrap();
    let out_name = "assets/test_out/".to_owned() + enc_config.out_name;
    let file = File::create(&out_name).unwrap();
    let mut fw = FileWriter::new_write(file);
    let mut bw = ByteWriter::new(&mut fw);
    let mut mux = create_muxer(mux_f, out_sm, &mut bw).unwrap();

    let (mut ifmt, dst_vinfo) = if let NACodecTypeInfo::Video(vinfo) = enc_params.format {
            (ScaleInfo { fmt: vinfo.format, width: vinfo.width, height: vinfo.height },
             vinfo)
        } else {
            (ScaleInfo { fmt: YUV420_FORMAT, width: 2, height: 2 },
             NAVideoInfo { width: 2, height: 2, format: YUV420_FORMAT, flipped: false })
        };
    let ofmt = ifmt;
    let mut scaler = NAScale::new(ifmt, ofmt).unwrap();
    let mut cvt_buf = alloc_video_buffer(dst_vinfo, 2).unwrap();
    loop {
        let pktres = dmx.get_frame();
        if let Err(e) = pktres {
            if e == DemuxerError::EOF { break; }
            panic!("decoding error");
        }
        let pkt = pktres.unwrap();
        if pkt.get_stream().id != in_stream_id { continue; }
        let frm = dec.decode(&mut dsupp, &pkt).unwrap();
        let buf = frm.get_buffer();
        let cfrm = if let NACodecTypeInfo::Video(_) = enc_params.format {
                let cur_ifmt = get_scale_fmt_from_pic(&buf);
                if cur_ifmt != ifmt {
                    ifmt = cur_ifmt;
                    scaler = NAScale::new(ifmt, ofmt).unwrap();
                }
                scaler.convert(&buf, &mut cvt_buf).unwrap();
                NAFrame::new(frm.get_time_information(), frm.frame_type, frm.key, frm.get_info(), cvt_buf.clone())
            } else if let NACodecTypeInfo::Audio(ref dst_ainfo) = enc_params.format {
                let cvt_buf = convert_audio_frame(&buf, dst_ainfo, &dst_chmap).unwrap();
                NAFrame::new(frm.get_time_information(), frm.frame_type, frm.key, frm.get_info(), cvt_buf)
            } else {
                panic!("unexpected format");
            };
        encoder.encode(&cfrm).unwrap();
        while let Ok(Some(pkt)) = encoder.get_packet() {
            mux.mux_frame(pkt).unwrap();
        }
        if let Some(maxts) = dec_config.limit {
            if frm.get_pts().unwrap_or(0) >= maxts {
                break;
            }
        }
    }
    encoder.flush().unwrap();
    while let Ok(Some(pkt)) = encoder.get_packet() {
        mux.mux_frame(pkt).unwrap();
    }
    mux.end().unwrap();
}
