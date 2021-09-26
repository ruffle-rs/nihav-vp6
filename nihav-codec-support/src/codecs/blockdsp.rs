//! Various pixel block manipulation functions.
use nihav_core::frame::*;

/// Copies block from the picture with pixels beyond the picture borders being replaced with replicated edge pixels.
pub fn edge_emu(src: &NAVideoBuffer<u8>, xpos: isize, ypos: isize, bw: usize, bh: usize, dst: &mut [u8], dstride: usize, comp: usize, align: u8) {
    let stride = src.get_stride(comp);
    let offs   = src.get_offset(comp);
    let (w_, h_) = src.get_dimensions(comp);
    let (hss, vss) = src.get_info().get_format().get_chromaton(comp).unwrap().get_subsampling();
    let data = src.get_data();
    let framebuf: &[u8] = data.as_slice();

    let (w, h) = if align == 0 {
            (w_, h_)
        } else {
            let wa = if align > hss { (1 << (align - hss)) - 1 } else { 0 };
            let ha = if align > vss { (1 << (align - vss)) - 1 } else { 0 };
            ((w_ + wa) & !wa, (h_ + ha) & !ha)
        };

    for y in 0..bh {
        let srcy;
        if (y as isize) + ypos < 0 { srcy = 0; }
        else if (y as isize) + ypos >= (h as isize) { srcy = h - 1; }
        else { srcy = ((y as isize) + ypos) as usize; }

        for x in 0..bw {
            let srcx;
            if (x as isize) + xpos < 0 { srcx = 0; }
            else if (x as isize) + xpos >= (w as isize) { srcx = w - 1; }
            else { srcx = ((x as isize) + xpos) as usize; }
            dst[x + y * dstride] = framebuf[offs + srcx + srcy * stride];
        }
    }
}

/// A generic type for motion interpolation function used by [`copy_blocks`]
///
/// The function expects following parameters:
/// * destination buffer
/// * destination buffer stride
/// * source buffer
/// * source buffer stride
/// * block width
/// * block height
///
/// [`copy_blocks`]: ./fn.copy_blocks.html
pub type BlkInterpFunc = fn(&mut [u8], usize, &[u8], usize, usize, usize);

/// Performs motion compensation on arbitrary block on some plane.
///
/// See [`copy_blocks`] for the arguments explanation.
///
/// [`copy_blocks`]: ./fn.copy_blocks.html
pub fn copy_block(dst: &mut NASimpleVideoFrame<u8>, src: NAVideoBufferRef<u8>, comp: usize,
                  dx: usize, dy: usize, mv_x: i16, mv_y: i16, bw: usize, bh: usize,
                  preborder: usize, postborder: usize,
                  mode: usize, interp: &[BlkInterpFunc])
{
    let pre  = if mode != 0 { preborder  as isize } else { 0 };
    let post = if mode != 0 { postborder as isize } else { 0 };
    let (w, h) = src.get_dimensions(comp);
    let sx = (dx as isize) + (mv_x as isize);
    let sy = (dy as isize) + (mv_y as isize);

    if (sx - pre < 0) || (sx + (bw as isize) + post > (w as isize)) ||
       (sy - pre < 0) || (sy + (bh as isize) + post > (h as isize)) {
        let ebuf_stride: usize = 32;
        let mut ebuf: Vec<u8> = vec![0; ebuf_stride * (bh + ((pre + post) as usize))];

        let dstride = dst.stride[comp];
        let doff    = dst.offset[comp];
        let edge = (pre + post) as usize;
        edge_emu(&src, sx - pre, sy - pre, bw + edge, bh + edge,
                 ebuf.as_mut_slice(), ebuf_stride, comp, 0);
        (interp[mode])(&mut dst.data[doff + dx + dy * dstride..], dstride,
                       ebuf.as_slice(), ebuf_stride, bw, bh);
    } else {
        let sstride = src.get_stride(comp);
        let soff    = src.get_offset(comp);
        let sdta    = src.get_data();
        let sbuf: &[u8] = sdta.as_slice();
        let dstride = dst.stride[comp];
        let doff    = dst.offset[comp];
        let saddr = soff + ((sx - pre) as usize) + ((sy - pre) as usize) * sstride;
        (interp[mode])(&mut dst.data[doff + dx + dy * dstride..], dstride,
                       &sbuf[saddr..], sstride, bw, bh);
    }
}
