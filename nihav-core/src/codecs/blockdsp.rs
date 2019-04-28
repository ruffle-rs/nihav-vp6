use crate::frame::*;

pub fn put_blocks(buf: &mut NAVideoBuffer<u8>, xpos: usize, ypos: usize, blk: &[[i16;64]; 6]) {
    let stridey = buf.get_stride(0);
    let strideu = buf.get_stride(1);
    let stridev = buf.get_stride(2);
    let mut idxy = buf.get_offset(0) + xpos * 16 + ypos * 16 * stridey;
    let mut idxu = buf.get_offset(1) + xpos *  8 + ypos *  8 * strideu;
    let mut idxv = buf.get_offset(2) + xpos *  8 + ypos *  8 * stridev;

    let data = buf.get_data_mut().unwrap();
    let framebuf: &mut [u8] = data.as_mut_slice();

    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[0][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[1][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k + 8] = v as u8;
        }
        idxy += stridey;
    }
    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[2][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[3][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k + 8] = v as u8;
        }
        idxy += stridey;
    }

    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[4][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxu + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[5][k + j * 8];
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxv + k] = v as u8;
        }
        idxu += strideu;
        idxv += stridev;
    }
}

pub fn add_blocks(buf: &mut NAVideoBuffer<u8>, xpos: usize, ypos: usize, blk: &[[i16;64]; 6]) {
    let stridey = buf.get_stride(0);
    let strideu = buf.get_stride(1);
    let stridev = buf.get_stride(2);
    let mut idxy = buf.get_offset(0) + xpos * 16 + ypos * 16 * stridey;
    let mut idxu = buf.get_offset(1) + xpos *  8 + ypos *  8 * strideu;
    let mut idxv = buf.get_offset(2) + xpos *  8 + ypos *  8 * stridev;

    let data = buf.get_data_mut().unwrap();
    let framebuf: &mut [u8] = data.as_mut_slice();

    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[0][k + j * 8] + (framebuf[idxy + k] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[1][k + j * 8] + (framebuf[idxy + k + 8] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k + 8] = v as u8;
        }
        idxy += stridey;
    }
    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[2][k + j * 8] + (framebuf[idxy + k] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[3][k + j * 8] + (framebuf[idxy + k + 8] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxy + k + 8] = v as u8;
        }
        idxy += stridey;
    }

    for j in 0..8 {
        for k in 0..8 {
            let mut v = blk[4][k + j * 8] + (framebuf[idxu + k] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxu + k] = v as u8;
        }
        for k in 0..8 {
            let mut v = blk[5][k + j * 8] + (framebuf[idxv + k] as i16);
            if v < 0 { v = 0; } if v > 255 { v = 255; }
            framebuf[idxv + k] = v as u8;
        }
        idxu += strideu;
        idxv += stridev;
    }
}

pub fn edge_emu(src: &NAVideoBuffer<u8>, xpos: isize, ypos: isize, bw: usize, bh: usize, dst: &mut [u8], dstride: usize, comp: usize) {
    let stride = src.get_stride(comp);
    let offs   = src.get_offset(comp);
    let (w, h) = src.get_dimensions(comp);
    let data = src.get_data();
    let framebuf: &[u8] = data.as_slice();

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

pub fn copy_blocks(dst: &mut NAVideoBuffer<u8>, src: &NAVideoBuffer<u8>,
                   dx: usize, dy: usize, sx: isize, sy: isize, bw: usize, bh: usize,
                   preborder: usize, postborder: usize,
                   mode: usize, interp: &[fn(&mut [u8], usize, &[u8], usize, usize, usize)])
{
    let pre  = if mode != 0 { preborder  as isize } else { 0 };
    let post = if mode != 0 { postborder as isize } else { 0 };
    let (w, h) = src.get_dimensions(0);

    if (sx - pre < 0) || ((sx >> 1) - pre < 0) || (sx + (bw as isize) + post > (w as isize)) ||
       (sy - pre < 0) || ((sy >> 1) - pre < 0) || (sy + (bh as isize) + post > (h as isize)) {
        let ebuf_stride: usize = 32;
        let mut ebuf: Vec<u8> = Vec::with_capacity(ebuf_stride * (bh + ((pre + post) as usize)));
        ebuf.resize((((pre + post) as usize) + bh) * ebuf_stride, 0);

        for comp in 0..3 {
            let dstride = dst.get_stride(comp);
            let doff    = dst.get_offset(comp);
            let ddta    = dst.get_data_mut().unwrap();
            let dbuf: &mut [u8] = ddta.as_mut_slice();
            let x   = if comp > 0 { dx/2 } else { dx };
            let y   = if comp > 0 { dy/2 } else { dy };
            let sx_ = (if comp > 0 { sx >> 1 } else { sx }) - pre;
            let sy_ = (if comp > 0 { sy >> 1 } else { sy }) - pre;
            let bw_ = (if comp > 0 { bw/2 } else { bw }) + ((pre + post) as usize);
            let bh_ = (if comp > 0 { bh/2 } else { bh }) + ((pre + post) as usize);
            edge_emu(src, sx_ - pre, sy_ - pre, bw_, bh_,
                     ebuf.as_mut_slice(), ebuf_stride, comp);
            let bw_ = if comp > 0 { bw/2 } else { bw };
            let bh_ = if comp > 0 { bh/2 } else { bh };
            (interp[mode])(&mut dbuf[doff + x + y * dstride..], dstride, ebuf.as_slice(), ebuf_stride, bw_, bh_);
        }
    } else {
        for comp in 0..3 {
            let sstride = src.get_stride(comp);
            let soff    = src.get_offset(comp);
            let sdta    = src.get_data();
            let sbuf: &[u8] = sdta.as_slice();
            let dstride = dst.get_stride(comp);
            let doff    = dst.get_offset(comp);
            let ddta    = dst.get_data_mut().unwrap();
            let dbuf: &mut [u8] = ddta.as_mut_slice();
            let x   = if comp > 0 { dx/2 } else { dx };
            let y   = if comp > 0 { dy/2 } else { dy };
            let sx_ = ((if comp > 0 { sx >> 1 } else { sx }) - pre) as usize;
            let sy_ = ((if comp > 0 { sy >> 1 } else { sy }) - pre) as usize;
            let bw_ = if comp > 0 { bw/2 } else { bw };
            let bh_ = if comp > 0 { bh/2 } else { bh };
            (interp[mode])(&mut dbuf[doff + x + y * dstride..], dstride, &sbuf[(soff + sx_ + sy_ * sstride)..], sstride, bw_, bh_);
        }
    }
}
