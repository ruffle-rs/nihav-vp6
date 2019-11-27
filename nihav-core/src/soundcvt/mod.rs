pub use crate::formats::{NASoniton,NAChannelMap};
pub use crate::frame::{NAAudioBuffer,NAAudioInfo,NABufferType};
use crate::formats::NAChannelType;
use crate::frame::alloc_audio_buffer;
use crate::io::byteio::*;
use std::f32::consts::SQRT_2;

#[derive(Clone,Copy,Debug,PartialEq)]
pub enum SoundConvertError {
    InvalidInput,
    AllocError,
    Unsupported,
}

enum ChannelOp {
    Passthrough,
    Reorder(Vec<usize>),
    Remix(Vec<f32>),
    DupMono(Vec<bool>),
}

impl ChannelOp {
    fn is_remix(&self) -> bool {
        match *self {
            ChannelOp::Remix(_) => true,
            ChannelOp::DupMono(_) => true,
            _ => false,
        }
    }
}

fn apply_channel_op<T:Copy>(ch_op: &ChannelOp, src: &Vec<T>, dst: &mut Vec<T>) {
    match *ch_op {
        ChannelOp::Passthrough => {
            dst.copy_from_slice(src.as_slice());
        },
        ChannelOp::Reorder(ref reorder) => {
            for (out, idx) in dst.iter_mut().zip(reorder.iter()) {
                *out = src[*idx];
            }
        },
        _ => {},
    };
}

fn remix_i32(ch_op: &ChannelOp, src: &Vec<i32>, dst: &mut Vec<i32>) {
    if let ChannelOp::Remix(ref remix_mat) = ch_op {
        let sch = src.len();
        for (out, coeffs) in dst.iter_mut().zip(remix_mat.chunks(sch)) {
            let mut sum = 0.0;
            for (inval, coef) in src.iter().zip(coeffs.iter()) {
                sum += (*inval as f32) * *coef;
            }
            *out = sum as i32;
        }
    }
    if let ChannelOp::DupMono(ref dup_mat) = ch_op {
        let src = src[0];
        for (out, copy) in dst.iter_mut().zip(dup_mat.iter()) {
            *out = if *copy { src } else { 0 };
        }
    }
}

fn remix_f32(ch_op: &ChannelOp, src: &Vec<f32>, dst: &mut Vec<f32>) {
    if let ChannelOp::Remix(ref remix_mat) = ch_op {
        let sch = src.len();
        for (out, coeffs) in dst.iter_mut().zip(remix_mat.chunks(sch)) {
            let mut sum = 0.0;
            for (inval, coef) in src.iter().zip(coeffs.iter()) {
                sum += *inval * *coef;
            }
            *out = sum;
        }
    }
    if let ChannelOp::DupMono(ref dup_mat) = ch_op {
        let src = src[0];
        for (out, copy) in dst.iter_mut().zip(dup_mat.iter()) {
            *out = if *copy { src } else { 0.0 };
        }
    }
}

fn read_samples<T:Copy>(src: &NAAudioBuffer<T>, mut idx: usize, dst: &mut Vec<T>) {
    let stride = src.get_stride();
    let data = src.get_data();
    for out in dst.iter_mut() {
        *out = data[idx];
        idx += stride;
    }
}

trait FromFmt<T:Copy> {
    fn cvt_from(val: T) -> Self;
}

impl FromFmt<u8> for u8 {
    fn cvt_from(val: u8) -> u8 { val }
}
impl FromFmt<u8> for i16 {
    fn cvt_from(val: u8) -> i16 { ((val as i16) - 128) * 0x101 }
}
impl FromFmt<u8> for i32 {
    fn cvt_from(val: u8) -> i32 { ((val as i32) - 128) * 0x01010101 }
}
impl FromFmt<u8> for f32 {
    fn cvt_from(val: u8) -> f32 { ((val as f32) - 128.0) / 128.0 }
}

impl FromFmt<i16> for u8 {
    fn cvt_from(val: i16) -> u8 { ((val >> 8) + 128).min(255).max(0) as u8 }
}
impl FromFmt<i16> for i16 {
    fn cvt_from(val: i16) -> i16 { val }
}
impl FromFmt<i16> for i32 {
    fn cvt_from(val: i16) -> i32 { (val as i32) * 0x10001 }
}
impl FromFmt<i16> for f32 {
    fn cvt_from(val: i16) -> f32 { (val as f32) / 32768.0 }
}

impl FromFmt<i32> for u8 {
    fn cvt_from(val: i32) -> u8 { ((val >> 24) + 128).min(255).max(0) as u8 }
}
impl FromFmt<i32> for i16 {
    fn cvt_from(val: i32) -> i16 { (val >> 16) as i16 }
}
impl FromFmt<i32> for i32 {
    fn cvt_from(val: i32) -> i32 { val }
}
impl FromFmt<i32> for f32 {
    fn cvt_from(val: i32) -> f32 { (val as f32) / 31.0f32.exp2() }
}

impl FromFmt<f32> for u8 {
    fn cvt_from(val: f32) -> u8 { ((val * 128.0) + 128.0).min(255.0).max(0.0) as u8 }
}
impl FromFmt<f32> for i16 {
    fn cvt_from(val: f32) -> i16 { (val * 32768.0).min(16383.0).max(-16384.0) as i16 }
}
impl FromFmt<f32> for i32 {
    fn cvt_from(val: f32) -> i32 { (val * 31.0f32.exp2()) as i32 }
}
impl FromFmt<f32> for f32 {
    fn cvt_from(val: f32) -> f32 { val }
}

trait IntoFmt<T:Copy> {
    fn cvt_into(self) -> T;
}

impl<T:Copy, U:Copy> IntoFmt<U> for T where U: FromFmt<T> {
    fn cvt_into(self) -> U { U::cvt_from(self) }
}


fn read_samples_i32<T:Copy>(src: &NAAudioBuffer<T>, mut idx: usize, dst: &mut Vec<i32>) where i32: FromFmt<T> {
    let stride = src.get_stride();
    let data = src.get_data();
    for out in dst.iter_mut() {
        *out = i32::cvt_from(data[idx]);
        idx += stride;
    }
}

fn read_samples_f32<T:Copy>(src: &NAAudioBuffer<T>, mut idx: usize, dst: &mut Vec<f32>) where f32: FromFmt<T> {
    let stride = src.get_stride();
    let data = src.get_data();
    for out in dst.iter_mut() {
        *out = f32::cvt_from(data[idx]);
        idx += stride;
    }
}

fn read_packed<T:Copy>(src: &NAAudioBuffer<u8>, idx: usize, dst: &mut Vec<T>, fmt: &NASoniton) where u8: IntoFmt<T>, i16: IntoFmt<T>, i32: IntoFmt<T>, f32: IntoFmt<T> {
    if (fmt.bits & 7) != 0 { unimplemented!(); }
    let bytes = (fmt.bits >> 3) as usize;
    let mut offset = idx * bytes * dst.len();
    let data = src.get_data();

    for el in dst.iter_mut() {
        let src = &data[offset..];
        *el = if !fmt.float {
println!("fmt = {} bytes, be: {}", fmt.bits, fmt.be);
                match (bytes, fmt.be) {
                    (1, _)     => src[0].cvt_into(),
                    (2, true)  => (read_u16be(src).unwrap() as i16).cvt_into(),
                    (2, false) => (read_u16le(src).unwrap() as i16).cvt_into(),
                    (3, true)  => ((read_u24be(src).unwrap() << 8) as i32).cvt_into(),
                    (3, false) => ((read_u24be(src).unwrap() << 8) as i32).cvt_into(),
                    (4, true)  => (read_u32be(src).unwrap() as i32).cvt_into(),
                    (4, false) => (read_u32be(src).unwrap() as i32).cvt_into(),
                    _ => unreachable!(),
                }
            } else {
                match (bytes, fmt.be) {
                    (4, true)  => read_f32be(src).unwrap().cvt_into(),
                    (4, false) => read_f32le(src).unwrap().cvt_into(),
                    (8, true)  => (read_f64be(src).unwrap() as f32).cvt_into(),
                    (8, false) => (read_f64le(src).unwrap() as f32).cvt_into(),
                    (_, _) => unreachable!(),
                }
            };
        offset += bytes;
    }
}

fn store_samples<T:Copy, U:Copy>(dst: &mut NAAudioBuffer<T>, mut idx: usize, src: &Vec<U>) where U: IntoFmt<T> {
    let stride = dst.get_stride();
    let data = dst.get_data_mut().unwrap();
    for src_el in src.iter() {
        data[idx] = (*src_el).cvt_into();
        idx += stride;
    }
}

fn store_packed<T:Copy>(dst: &mut NAAudioBuffer<u8>, idx: usize, src: &Vec<T>, fmt: &NASoniton) where u8: FromFmt<T>, i16: FromFmt<T>, i32: FromFmt<T>, f32: FromFmt<T> {
    if (fmt.bits & 7) != 0 { unimplemented!(); }
    let bytes = (fmt.bits >> 3) as usize;
    let mut offset = idx * bytes * src.len();
    let data = dst.get_data_mut().unwrap();

    for el in src.iter() {
        let dst = &mut data[offset..];
        if !fmt.float {
            match (bytes, fmt.be) {
                (1, _) => {
                    dst[0] = u8::cvt_from(*el);
                },
                (2, true)  => write_u16be(dst, i16::cvt_from(*el) as u16).unwrap(),
                (2, false) => write_u16le(dst, i16::cvt_from(*el) as u16).unwrap(),
                (3, true)  => write_u24be(dst, (i32::cvt_from(*el) >> 8) as u32).unwrap(),
                (3, false) => write_u24le(dst, (i32::cvt_from(*el) >> 8) as u32).unwrap(),
                (4, true)  => write_u32be(dst, i32::cvt_from(*el) as u32).unwrap(),
                (4, false) => write_u32le(dst, i32::cvt_from(*el) as u32).unwrap(),
                _ => unreachable!(),
            };
        } else {
            match (bytes, fmt.be) {
                (4, true)  => write_f32be(dst, f32::cvt_from(*el)).unwrap(),
                (4, false) => write_f32le(dst, f32::cvt_from(*el)).unwrap(),
                (8, true)  => write_f64be(dst, f32::cvt_from(*el) as f64).unwrap(),
                (8, false) => write_f64le(dst, f32::cvt_from(*el) as f64).unwrap(),
                (_, _) => unreachable!(),
            };
        }
        offset += bytes;
    }
}

pub fn convert_audio_frame(src: &NABufferType, dst_info: &NAAudioInfo, dst_chmap: &NAChannelMap) -> 
Result<NABufferType, SoundConvertError> {
    let nsamples = src.get_audio_length();
    if nsamples == 0 {
        return Err(SoundConvertError::InvalidInput);
    }
    let src_chmap = src.get_chmap().unwrap();
    let src_info  = src.get_audio_info().unwrap();
    if (src_chmap.num_channels() == 0) || (dst_chmap.num_channels() == 0) {
        return Err(SoundConvertError::InvalidInput);
    }

    let needs_remix = src_chmap.num_channels() != dst_chmap.num_channels();
    let no_channel_needs = !needs_remix && channel_maps_equal(src_chmap, dst_chmap);
    let needs_reorder = !needs_remix && !no_channel_needs && channel_maps_reordered(src_chmap, dst_chmap);

    let channel_op = if no_channel_needs {
            ChannelOp::Passthrough
        } else if needs_reorder {
            let reorder_mat = calculate_reorder_matrix(src_chmap, dst_chmap);
            ChannelOp::Reorder(reorder_mat)
        } else if src_chmap.num_channels() > 1 {
            let remix_mat = calculate_remix_matrix(src_chmap, dst_chmap);
            ChannelOp::Remix(remix_mat)
        } else {
            let mut dup_mat: Vec<bool> = Vec::with_capacity(dst_chmap.num_channels());
            for i in 0..dst_chmap.num_channels() {
                let ch =  dst_chmap.get_channel(i);
                if ch.is_left() || ch.is_right() || ch == NAChannelType::C {
                    dup_mat.push(true);
                } else {
                    dup_mat.push(false);
                }
            }
            ChannelOp::DupMono(dup_mat)
        };

    let src_fmt = src_info.get_format();
    let dst_fmt = dst_info.get_format();
    let no_conversion = src_fmt == dst_fmt;

    if no_conversion && no_channel_needs {
        return Ok(src.clone());
    }

    let ret = alloc_audio_buffer(dst_info.clone(), nsamples, dst_chmap.clone());
    if ret.is_err() {
        return Err(SoundConvertError::AllocError);
    }
    let mut dst_buf = ret.unwrap();

    if no_conversion {
        match (src, &mut dst_buf) {
            (NABufferType::AudioU8(sb), NABufferType::AudioU8(ref mut db)) => {
                let mut svec = vec![0; src_chmap.num_channels()];
                let mut tvec1 = vec![0; src_chmap.num_channels()];
                let mut tvec2 = vec![0; dst_chmap.num_channels()];
                let mut dvec = vec![0; dst_chmap.num_channels()];
                for i in 0..nsamples {
                    read_samples(sb, i, &mut svec);
                    if !channel_op.is_remix() {
                        apply_channel_op(&channel_op, &svec, &mut dvec);
                    } else {
                        for (oel, iel) in tvec1.iter_mut().zip(svec.iter()) {
                            *oel = (*iel as i32) - 128;
                        }
                        remix_i32(&channel_op, &tvec1, &mut tvec2);
                        for (oel, iel) in dvec.iter_mut().zip(tvec2.iter()) {
                            *oel = (*iel + 128).min(255).max(0) as u8;
                        }
                    }
                    store_samples(db, i, &dvec);
                }
            },
            (NABufferType::AudioI16(sb), NABufferType::AudioI16(ref mut db)) => {
                let mut svec = vec![0; src_chmap.num_channels()];
                let mut tvec1 = vec![0; src_chmap.num_channels()];
                let mut tvec2 = vec![0; dst_chmap.num_channels()];
                let mut dvec = vec![0; dst_chmap.num_channels()];
                for i in 0..nsamples {
                    read_samples(sb, i, &mut svec);
                    if !channel_op.is_remix() {
                        apply_channel_op(&channel_op, &svec, &mut dvec);
                    } else {
                        for (oel, iel) in tvec1.iter_mut().zip(svec.iter()) {
                            *oel = *iel as i32;
                        }
                        remix_i32(&channel_op, &tvec1, &mut tvec2);
                        for (oel, iel) in dvec.iter_mut().zip(tvec2.iter()) {
                            *oel = (*iel).min(16383).max(-16384) as i16;
                        }
                    }
                    store_samples(db, i, &dvec);
                }
            },
            (NABufferType::AudioI32(sb), NABufferType::AudioI32(ref mut db)) => {
                let mut svec = vec![0; src_chmap.num_channels()];
                let mut dvec = vec![0; dst_chmap.num_channels()];
                for i in 0..nsamples {
                    read_samples(sb, i, &mut svec);
                    if !channel_op.is_remix() {
                        apply_channel_op(&channel_op, &svec, &mut dvec);
                    } else {
                        remix_i32(&channel_op, &svec, &mut dvec);
                    }
                    store_samples(db, i, &dvec);
                }
            },
            (NABufferType::AudioF32(sb), NABufferType::AudioF32(ref mut db)) => {
                let mut svec = vec![0.0; src_chmap.num_channels()];
                let mut dvec = vec![0.0; dst_chmap.num_channels()];
                for i in 0..nsamples {
                    read_samples(sb, i, &mut svec);
                    if !channel_op.is_remix() {
                        apply_channel_op(&channel_op, &svec, &mut dvec);
                    } else {
                        remix_f32(&channel_op, &svec, &mut dvec);
                    }
                    store_samples(db, i, &dvec);
                }
            },
            _ => unimplemented!(),
        };
    } else {
        let into_float = dst_fmt.float;
        if !into_float {
            let mut svec = vec![0i32; src_chmap.num_channels()];
            let mut dvec = vec![0i32; dst_chmap.num_channels()];
            for i in 0..nsamples {
                match src {
                    NABufferType::AudioU8 (ref sb) => read_samples_i32(sb, i, &mut svec),
                    NABufferType::AudioI16(ref sb) => read_samples_i32(sb, i, &mut svec),
                    NABufferType::AudioI32(ref sb) => read_samples_i32(sb, i, &mut svec),
                    NABufferType::AudioF32(ref sb) => read_samples_i32(sb, i, &mut svec),
                    NABufferType::AudioPacked(ref sb) => read_packed(sb, i, &mut svec, &src_fmt),
                    _ => unreachable!(),
                };
                if !channel_op.is_remix() {
                    apply_channel_op(&channel_op, &svec, &mut dvec);
                } else {
                    remix_i32(&channel_op, &svec, &mut dvec);
                }
                match dst_buf {
                    NABufferType::AudioU8 (ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioI16(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioI32(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioF32(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioPacked(ref mut buf) => store_packed(buf, i, &dvec, &dst_fmt),
                    _ => unreachable!(),
                };
            }
        } else {
            let mut svec = vec![0.0f32; src_chmap.num_channels()];
            let mut dvec = vec![0.0f32; dst_chmap.num_channels()];
            for i in 0..nsamples {
                match src {
                    NABufferType::AudioU8 (ref sb) => read_samples_f32(sb, i, &mut svec),
                    NABufferType::AudioI16(ref sb) => read_samples_f32(sb, i, &mut svec),
                    NABufferType::AudioI32(ref sb) => read_samples_f32(sb, i, &mut svec),
                    NABufferType::AudioF32(ref sb) => read_samples_f32(sb, i, &mut svec),
                    NABufferType::AudioPacked(ref sb) => read_packed(sb, i, &mut svec, &src_fmt),
                    _ => unreachable!(),
                };
                if !channel_op.is_remix() {
                    apply_channel_op(&channel_op, &svec, &mut dvec);
                } else {
                    remix_f32(&channel_op, &svec, &mut dvec);
                }
                match dst_buf {
                    NABufferType::AudioU8 (ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioI16(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioI32(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioF32(ref mut db) => store_samples(db, i, &dvec),
                    NABufferType::AudioPacked(ref mut buf) => store_packed(buf, i, &dvec, &dst_fmt),
                    _ => unreachable!(),
                };
            }
        }
    }
    
    Ok(dst_buf)
}

pub fn channel_maps_equal(a: &NAChannelMap, b: &NAChannelMap) -> bool {
    if a.num_channels() != b.num_channels() { return false; }
    for i in 0..a.num_channels() {
        if a.get_channel(i) != b.get_channel(i) {
            return false;
        }
    }
    true
}

pub fn channel_maps_reordered(a: &NAChannelMap, b: &NAChannelMap) -> bool {
    if a.num_channels() != b.num_channels() { return false; }
    let mut count_a = [0u8; 32];
    let mut count_b = [0u8; 32];
    for i in 0..a.num_channels() {
        count_a[a.get_channel(i) as usize] += 1;
        count_b[b.get_channel(i) as usize] += 1;
    }
    for (c0, c1) in count_a.iter().zip(count_b.iter()) {
        if *c0 != *c1 {
            return false;
        }
    }
    true
}

pub fn calculate_reorder_matrix(src: &NAChannelMap, dst: &NAChannelMap) -> Vec<usize> {
    if src.num_channels() != dst.num_channels() { return Vec::new(); }
    let num_channels = src.num_channels();
    let mut reorder: Vec<usize> = Vec::with_capacity(num_channels);
    for i in 0..num_channels {
        let dst_ch = dst.get_channel(i);
        for j in 0..num_channels {
            if src.get_channel(j) == dst_ch {
                reorder.push(j);
                break;
            }
        }
    }
    if reorder.len() != num_channels { reorder.clear(); }
    reorder
}

fn is_stereo(chmap: &NAChannelMap) -> bool {
    (chmap.num_channels() == 2) &&
    (chmap.get_channel(0) == NAChannelType::L) && 
    (chmap.get_channel(1) == NAChannelType::R)
}

pub fn calculate_remix_matrix(src: &NAChannelMap, dst: &NAChannelMap) -> Vec<f32> {
    if is_stereo(src) && dst.num_channels() == 1 &&
        (dst.get_channel(0) == NAChannelType::L || dst.get_channel(0) == NAChannelType::C) {
        return vec![0.5, 0.5];
    }
    if src.num_channels() >= 5 && is_stereo(dst) {
        let src_nch = src.num_channels();
        let mut mat = vec![0.0f32; src_nch * 2];
        let (l_mat, r_mat) = mat.split_at_mut(src_nch);
        for ch in 0..src_nch {
            match src.get_channel(ch) {
                NAChannelType::L    => l_mat[ch] = 1.0,
                NAChannelType::R    => r_mat[ch] = 1.0,
                NAChannelType::C    => { l_mat[ch] = SQRT_2 / 2.0; r_mat[ch] = SQRT_2 / 2.0; },
                NAChannelType::Ls   => l_mat[ch] = SQRT_2 / 2.0,
                NAChannelType::Rs   => r_mat[ch] = SQRT_2 / 2.0,
                _ => {},
            };
        }
        return mat;
    }
unimplemented!();
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;
    use crate::formats::*;

    #[test]
    fn test_matrices() {
        let chcfg51 = NAChannelMap::from_str("L,R,C,LFE,Ls,Rs").unwrap();
        let chcfg52 = NAChannelMap::from_str("C,L,R,Ls,Rs,LFE").unwrap();
        let stereo  = NAChannelMap::from_str("L,R").unwrap();
        let reorder = calculate_reorder_matrix(&chcfg51, &chcfg52);
        assert_eq!(reorder.as_slice(), [ 2, 0, 1, 4, 5, 3]);
        let remix   = calculate_remix_matrix(&chcfg51, &stereo);
        assert_eq!(remix.as_slice(), [ 1.0, 0.0, SQRT_2 / 2.0, 0.0, SQRT_2 / 2.0, 0.0,
                                       0.0, 1.0, SQRT_2 / 2.0, 0.0, 0.0, SQRT_2 / 2.0 ]);
    }
    #[test]
    fn test_conversion() {
        const CHANNEL_VALUES: [u8; 6] = [ 140, 90, 130, 128, 150, 70 ];
        let chcfg51 = NAChannelMap::from_str("L,R,C,LFE,Ls,Rs").unwrap();
        let stereo  = NAChannelMap::from_str("L,R").unwrap();
        let src_ainfo = NAAudioInfo {
                            sample_rate:    44100,
                            channels:       chcfg51.num_channels() as u8,
                            format:         SND_U8_FORMAT,
                            block_len:      512,
                        };
        let mut dst_ainfo = NAAudioInfo {
                            sample_rate:    44100,
                            channels:       stereo.num_channels() as u8,
                            format:         SND_S16P_FORMAT,
                            block_len:      512,
                        };
        let mut src_frm = alloc_audio_buffer(src_ainfo, 42, chcfg51.clone()).unwrap();
        if let NABufferType::AudioPacked(ref mut abuf) = src_frm {
            let data = abuf.get_data_mut().unwrap();
            let mut idx = 0;
            for _ in 0..42 {
                for ch in 0..chcfg51.num_channels() {
                    data[idx] = CHANNEL_VALUES[ch];
                    idx += 1;
                }
            }
        } else {
            panic!("wrong buffer type");
        }

        let out_frm = convert_audio_frame(&src_frm, &dst_ainfo, &stereo).unwrap();
        if let NABufferType::AudioI16(ref abuf) = out_frm {
            let off0 = abuf.get_offset(0);
            let off1 = abuf.get_offset(1);
            let data = abuf.get_data();
            let l = data[off0];
            let r = data[off1];
            assert_eq!(l, 7445);
            assert_eq!(r, -19943);
        } else {
            panic!("wrong buffer type");
        }

        dst_ainfo.format = SND_F32P_FORMAT;
        let out_frm = convert_audio_frame(&src_frm, &dst_ainfo, &stereo).unwrap();
        if let NABufferType::AudioF32(ref abuf) = out_frm {
            let off0 = abuf.get_offset(0);
            let off1 = abuf.get_offset(1);
            let data = abuf.get_data();
            let l = data[off0];
            let r = data[off1];
            assert_eq!(l,  0.22633252);
            assert_eq!(r, -0.6062342);
        } else {
            panic!("wrong buffer type");
        }
    }
}
