use super::*;
use super::kernel::Kernel;

struct NNResampler {}

impl NNResampler {
    fn new() -> Self { Self{} }
}

fn scale_line<T:Copy>(src: &[T], dst: &mut [T], src_w: usize, dst_w: usize) {
    if src_w == dst_w {
        (&mut dst[..dst_w]).copy_from_slice(&src[..dst_w]);
    } else if src_w < dst_w {
        if dst_w % src_w == 0 {
            let step = dst_w / src_w;
            for (out, srcv) in dst.chunks_exact_mut(step).take(src_w).zip(src.iter()) {
                for el in out.iter_mut() {
                    *el = *srcv;
                }
            }
        } else {
            let mut pos = 0;
            for out in dst.iter_mut().take(dst_w) {
                *out = src[pos / dst_w];
                pos += src_w;
            }
        }
    } else {
        if dst_w % src_w == 0 {
            let step = src_w / dst_w;
            for (out, srcv) in dst.iter_mut().take(dst_w).zip(src.iter().step_by(step)) {
                *out = *srcv;
            }
        } else {
            let mut pos = 0;
            for out in dst.iter_mut().take(dst_w) {
                *out = src[pos / dst_w];
                pos += src_w;
            }
        }
    }
}

macro_rules! scale_loop {
    ($sbuf:expr, $dbuf:expr) => {
            let fmt = $sbuf.get_info().get_format();
            let ncomp = fmt.get_num_comp();
            for comp in 0..ncomp {
                let istride = $sbuf.get_stride(comp);
                let dstride = $dbuf.get_stride(comp);
                let (sw, sh) = $sbuf.get_dimensions(comp);
                let (dw, dh) = $dbuf.get_dimensions(comp);
                let ioff = $sbuf.get_offset(comp);
                let mut doff = $dbuf.get_offset(comp);
                let src = $sbuf.get_data();
                let dst = $dbuf.get_data_mut().unwrap();
                for y in 0..dh {
                    let sy = y * sh / dh;
                    let soff = ioff + sy * istride;
                    scale_line(&src[soff..], &mut dst[doff..], sw, dw);
                    doff += dstride;
                }
            }
    };
}

impl Kernel for NNResampler {
    fn init(&mut self, in_fmt: &ScaleInfo, dest_fmt: &ScaleInfo) -> ScaleResult<NABufferType> {
        let res = alloc_video_buffer(NAVideoInfo::new(dest_fmt.width, dest_fmt.height, false, in_fmt.fmt), 3);
        if res.is_err() { return Err(ScaleError::AllocError); }
        Ok(res.unwrap())
    }
    fn process(&mut self, pic_in: &NABufferType, pic_out: &mut NABufferType) {
        if let (Some(ref sbuf), Some(ref mut dbuf)) = (pic_in.get_vbuf(), pic_out.get_vbuf()) {
            scale_loop!(sbuf, dbuf);
        } else if let (Some(ref sbuf), Some(ref mut dbuf)) = (pic_in.get_vbuf16(), pic_out.get_vbuf16()) {
            scale_loop!(sbuf, dbuf);
        } else if let (Some(ref sbuf), Some(ref mut dbuf)) = (pic_in.get_vbuf32(), pic_out.get_vbuf32()) {
            scale_loop!(sbuf, dbuf);
        } else {
            unreachable!();
        }
    }
}

pub fn create_scale() -> Box<dyn Kernel> {
    Box::new(NNResampler::new())
}

