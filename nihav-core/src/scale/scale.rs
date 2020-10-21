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

fn fill_plane<T: Copy>(dst: &mut [T], w: usize, h: usize, stride: usize, val: T) {
    for row in dst.chunks_mut(stride).take(h) {
        for el in row.iter_mut().take(w) {
            *el = val;
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
            let dfmt = $dbuf.get_info().get_format();
            let ndcomp = dfmt.get_num_comp();
            if ndcomp > ncomp {
                if !fmt.alpha && dfmt.alpha {
                    let acomp = ndcomp - 1;
                    let dstride = $dbuf.get_stride(acomp);
                    let (dw, dh) = $dbuf.get_dimensions(acomp);
                    let doff = $dbuf.get_offset(acomp);
                    let dst = $dbuf.get_data_mut().unwrap();
                    fill_plane(&mut dst[doff..], dw, dh, dstride, 0);
                }
                if fmt.model.is_yuv() && ((!fmt.alpha && ncomp == 1) || (fmt.alpha && ncomp == 2)) && ndcomp >= 3 {
                    let uval = 1 << (dfmt.comp_info[1].unwrap().depth - 1);
                    let vval = 1 << (dfmt.comp_info[2].unwrap().depth - 1);

                    let ustride = $dbuf.get_stride(1);
                    let vstride = $dbuf.get_stride(2);
                    let (uw, uh) = $dbuf.get_dimensions(1);
                    let (vw, vh) = $dbuf.get_dimensions(2);
                    let uoff = $dbuf.get_offset(1);
                    let voff = $dbuf.get_offset(2);
                    let dst = $dbuf.get_data_mut().unwrap();
                    fill_plane(&mut dst[uoff..], uw, uh, ustride, uval);
                    fill_plane(&mut dst[voff..], vw, vh, vstride, vval);
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

