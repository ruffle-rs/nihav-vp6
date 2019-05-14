use super::*;
use super::kernel::Kernel;

const YUV_PARAMS: &[[f32; 2]] = &[
    [ 0.333,    0.333   ], // RGB
    [ 0.2126,   0.0722  ], // ITU-R BT709
    [ 0.333,    0.333   ], // unspecified
    [ 0.333,    0.333   ], // reserved
    [ 0.299,    0.114   ], // ITU-R BT601
    [ 0.299,    0.114   ], // ITU-R BT470
    [ 0.299,    0.114   ], // SMPTE 170M
    [ 0.212,    0.087   ], // SMPTE 240M
    [ 0.333,    0.333   ], // YCoCg
    [ 0.2627,   0.0593  ], // ITU-R BT2020
    [ 0.2627,   0.0593  ], // ITU-R BT2020
];

const BT_PAL_COEFFS: [f32; 2] = [ 0.493, 0.877 ];

const SMPTE_NTSC_COEFFS: &[f32; 4] = &[ -0.268, 0.7358, 0.4127, 0.4778 ];

/*const RGB2YCOCG: [[f32; 3]; 3] = [
    [  0.25,  0.5,  0.25 ],
    [ -0.25,  0.5, -0.25 ],
    [  0.5,   0.0, -0.5  ]
];
const YCOCG2RGB: [[f32; 3]; 3] = [
    [ 1.0, -1.0,  1.0 ],
    [ 1.0,  1.0,  0.0 ],
    [ 1.0, -1.0, -1.0 ]
];

const XYZ2RGB: [[f32; 3]; 3] = [
    [ 0.49,    0.31,   0.2     ],
    [ 0.17697, 0.8124, 0.01063 ],
    [ 0.0,     0.01,   0.99    ]
];
const RGB2XYZ: [[f32; 3]; 3] = [
    [  2.364613, -0.89654, -0.46807 ],
    [ -0.515167,  1.42641,  0.08876 ],
    [  0.0052,   -0.01441,  1.00920 ]
];*/

fn make_rgb2yuv(kr: f32, kb: f32, mat: &mut [[f32; 3]; 3]) {
    // Y
    mat[0][0] = kr;
    mat[0][1] = 1.0 - kr - kb;
    mat[0][2] = kb;
    // Cb
    mat[1][0] = -mat[0][0] * 0.5 / (1.0 - kb);
    mat[1][1] = -mat[0][1] * 0.5 / (1.0 - kb);
    mat[1][2] = 0.5;
    // Cr
    mat[2][0] = 0.5;
    mat[2][1] = -mat[0][1] * 0.5 / (1.0 - kr);
    mat[2][2] = -mat[0][2] * 0.5 / (1.0 - kr);
}

fn make_yuv2rgb(kr: f32, kb: f32, mat: &mut [[f32; 3]; 3]) {
    let kg = 1.0 - kr - kb;

    // R
    mat[0][0] = 1.0;
    mat[0][1] = 0.0;
    mat[0][2] = 2.0 * (1.0 - kr);
    // G
    mat[1][0] = 1.0;
    mat[1][1] = -kb * 2.0 * (1.0 - kb) / kg;
    mat[1][2] = -kr * 2.0 * (1.0 - kr) / kg;
    // B
    mat[2][0] = 1.0;
    mat[2][1] = 2.0 * (1.0 - kb);
    mat[2][2] = 0.0;
}

fn apply_pal_rgb2yuv(eu: f32, ev: f32, mat: &mut [[f32; 3]; 3]) {
    let ufac = 2.0 * (1.0 - mat[0][2]) * eu;
    let vfac = 2.0 * (1.0 - mat[0][0]) * ev;

    // U
    mat[1][0] *= ufac;
    mat[1][1] *= ufac;
    mat[1][2]  = eu * (1.0 - mat[0][2]);
    // V
    mat[2][0]  = ev * (1.0 - mat[0][0]);
    mat[2][1] *= vfac;
    mat[2][2] *= vfac;
}

fn apply_pal_yuv2rgb(eu: f32, ev: f32, mat: &mut [[f32; 3]; 3]) {
    let ufac = 1.0 / (mat[2][1] * eu);
    let vfac = 1.0 / (mat[0][2] * ev);

    // R
    mat[0][2] *= vfac;
    // G
    mat[1][1] *= ufac;
    mat[1][2] *= vfac;
    // B
    mat[2][1] *= ufac;
}

fn apply_ntsc_rgb2yiq(params: &[f32; 4], mat: &mut [[f32; 3]; 3]) {
    let ufac = 2.0 * (1.0 - mat[0][2]);
    let vfac = 2.0 * (1.0 - mat[0][0]);
    let mut tmp: [[f32; 3]; 2] = [[0.0; 3]; 2];

    for i in 0..3 {
        tmp[0][i] = mat[1][i] * ufac;
        tmp[1][i] = mat[2][i] * vfac;
    }
    for i in 0..3 {
        mat[1][i] = params[0] * tmp[0][i] + params[1] * tmp[1][i];
        mat[2][i] = params[2] * tmp[0][i] + params[3] * tmp[1][i];
    }
}

fn subm_det(mat: &[[f32; 3]; 3], col: usize, row: usize) -> f32 {
    let row0 = if row == 0 { 1 } else { 0 };
    let row1 = if (row == 1) || (row0 == 1) { 2 } else { 1 };
    let col0 = if col == 0 { 1 } else { 0 };
    let col1 = if (col == 1) || (col0 == 1) { 2 } else { 1 };

    let det = mat[row0][col0] * mat[row1][col1] - mat[row0][col1] * mat[row1][col0];
    if ((col ^ row) & 1) == 0 {
        det
    } else {
        -det
    }
}

fn invert_matrix(mat: &mut [[f32; 3]; 3]) {
    let d00 = subm_det(mat, 0, 0);
    let d01 = subm_det(mat, 0, 1);
    let d02 = subm_det(mat, 0, 2);
    let d10 = subm_det(mat, 1, 0);
    let d11 = subm_det(mat, 1, 1);
    let d12 = subm_det(mat, 1, 2);
    let d20 = subm_det(mat, 2, 0);
    let d21 = subm_det(mat, 2, 1);
    let d22 = subm_det(mat, 2, 2);
    let det = 1.0 / (mat[0][0] * d00 + mat[0][1] * d10 + mat[0][2] * d20).abs();

    mat[0][0] = det * d00;
    mat[0][1] = det * d01;
    mat[0][2] = det * d02;
    mat[1][0] = det * d10;
    mat[1][1] = det * d11;
    mat[1][2] = det * d12;
    mat[2][0] = det * d20;
    mat[2][1] = det * d21;
    mat[2][2] = det * d22;
}

fn matrix_mul(mat: &[[f32; 3]; 3], a: f32, b: f32, c: f32) -> (f32, f32, f32) {
    (a * mat[0][0] + b * mat[0][1] + c * mat[0][2],
     a * mat[1][0] + b * mat[1][1] + c * mat[1][2],
     a * mat[2][0] + b * mat[2][1] + c * mat[2][2] )
}

#[derive(Default)]
struct RgbToYuv {
    matrix: [[f32; 3]; 3],
}

impl RgbToYuv {
    fn new() -> Self { Self::default() }
}

impl Kernel for RgbToYuv {
    fn init(&mut self, in_fmt: &ScaleInfo, dest_fmt: &ScaleInfo) -> ScaleResult<NABufferType> {
        let mut df = dest_fmt.fmt;
//todo coeff selection
        make_rgb2yuv(YUV_PARAMS[2][0], YUV_PARAMS[2][1], &mut self.matrix);
        if let ColorModel::YUV(yuvsm) = df.get_model() {
            match yuvsm {
            YUVSubmodel::YCbCr  => {},
            YUVSubmodel::YIQ    => { apply_ntsc_rgb2yiq(SMPTE_NTSC_COEFFS, &mut self.matrix); },
            YUVSubmodel::YUVJ   => { apply_pal_rgb2yuv(BT_PAL_COEFFS[0], BT_PAL_COEFFS[1], &mut self.matrix); },
            };
        } else {
            return Err(ScaleError::InvalidArgument);
        }
        for i in 0..MAX_CHROMATONS {
            if let Some(ref mut chr) = df.comp_info[i] {
                chr.packed = false;
                chr.comp_offs = i as u8;
                chr.h_ss = 0;
                chr.v_ss = 0;
            }
        }
println!(" [intermediate format {}]", df);
        let res = alloc_video_buffer(NAVideoInfo::new(in_fmt.width, in_fmt.height, false, df), 3);
        if res.is_err() { return Err(ScaleError::AllocError); }
        Ok(res.unwrap())
    }
    fn process(&mut self, pic_in: &NABufferType, pic_out: &mut NABufferType) {
        if let (Some(ref sbuf), Some(ref mut dbuf)) = (pic_in.get_vbuf(), pic_out.get_vbuf()) {
            let istrides = [sbuf.get_stride(0), sbuf.get_stride(1), sbuf.get_stride(2)];
            let dstrides = [dbuf.get_stride(0), dbuf.get_stride(1), dbuf.get_stride(2)];
            let (w, h) = sbuf.get_dimensions(0);

            let mut roff = sbuf.get_offset(0);
            let mut goff = sbuf.get_offset(1);
            let mut boff = sbuf.get_offset(2);
            let mut yoff = dbuf.get_offset(0);
            let mut uoff = dbuf.get_offset(1);
            let mut voff = dbuf.get_offset(2);
            let src = sbuf.get_data();
            let dst = dbuf.get_data_mut().unwrap();
            for _y in 0..h {
                for x in 0..w {
                    let r = src[roff + x] as f32;
                    let g = src[goff + x] as f32;
                    let b = src[boff + x] as f32;
                    let (y, u, v) = matrix_mul(&self.matrix, r, g, b);

                    dst[yoff + x] = (y as i16).max(0).min(255) as u8;
                    dst[uoff + x] = ((u as i16).max(-128).min(128) + 128) as u8;
                    dst[voff + x] = ((v as i16).max(-128).min(128) + 128) as u8;
                }
                roff += istrides[0];
                goff += istrides[1];
                boff += istrides[2];
                yoff += dstrides[0];
                uoff += dstrides[1];
                voff += dstrides[2];
            }
        }
    }
}

pub fn create_rgb2yuv() -> Box<dyn Kernel> {
    Box::new(RgbToYuv::new())
}

#[derive(Default)]
struct YuvToRgb {
    matrix: [[f32; 3]; 3],
}

impl YuvToRgb {
    fn new() -> Self { Self::default() }
}

impl Kernel for YuvToRgb {
    fn init(&mut self, in_fmt: &ScaleInfo, dest_fmt: &ScaleInfo) -> ScaleResult<NABufferType> {
        let mut df = dest_fmt.fmt;
//todo coeff selection
        make_yuv2rgb(YUV_PARAMS[2][0], YUV_PARAMS[2][1], &mut self.matrix);
        if let ColorModel::YUV(yuvsm) = in_fmt.fmt.get_model() {
            match yuvsm {
                YUVSubmodel::YCbCr  => {},
                YUVSubmodel::YIQ    => {
                    make_rgb2yuv(YUV_PARAMS[2][0], YUV_PARAMS[2][1], &mut self.matrix);
                    apply_ntsc_rgb2yiq(SMPTE_NTSC_COEFFS, &mut self.matrix);
                    invert_matrix(&mut self.matrix);
                },
                YUVSubmodel::YUVJ   => {
                    apply_pal_yuv2rgb(BT_PAL_COEFFS[0], BT_PAL_COEFFS[1], &mut self.matrix);
                },
            };
        } else {
            return Err(ScaleError::InvalidArgument);
        }
        for i in 0..MAX_CHROMATONS {
            if let Some(ref mut chr) = df.comp_info[i] {
                chr.packed = false;
                chr.comp_offs = i as u8;
            }
        }
println!(" [intermediate format {}]", df);
        let res = alloc_video_buffer(NAVideoInfo::new(in_fmt.width, in_fmt.height, false, df), 3);
        if res.is_err() { return Err(ScaleError::AllocError); }
        Ok(res.unwrap())
    }
    fn process(&mut self, pic_in: &NABufferType, pic_out: &mut NABufferType) {
        if let (Some(ref sbuf), Some(ref mut dbuf)) = (pic_in.get_vbuf(), pic_out.get_vbuf()) {
            let istrides = [sbuf.get_stride(0), sbuf.get_stride(1), sbuf.get_stride(2)];
            let dstrides = [dbuf.get_stride(0), dbuf.get_stride(1), dbuf.get_stride(2)];
            let (w, h) = sbuf.get_dimensions(0);
            let (sv0, sh0) = sbuf.get_info().get_format().get_chromaton(1).unwrap().get_subsampling();
            let (sv1, sh1) = sbuf.get_info().get_format().get_chromaton(2).unwrap().get_subsampling();

            let uhmask = (1 << sh0) - 1;
            let vhmask = (1 << sh1) - 1;
            let mut roff = dbuf.get_offset(0);
            let mut goff = dbuf.get_offset(1);
            let mut boff = dbuf.get_offset(2);
            let mut yoff = sbuf.get_offset(0);
            let mut uoff = sbuf.get_offset(1);
            let mut voff = sbuf.get_offset(2);
            let src = sbuf.get_data();
            let dst = dbuf.get_data_mut().unwrap();
            for y in 0..h {
                for x in 0..w {
                    let y = src[yoff + x] as f32;
                    let u = ((src[uoff + (x >> sv0)] as i16) - 128) as f32;
                    let v = ((src[voff + (x >> sv1)] as i16) - 128) as f32;

                    let (r, g, b) = matrix_mul(&self.matrix, y, u, v);
                    dst[roff + x] = (r as i16).max(0).min(255) as u8;
                    dst[goff + x] = (g as i16).max(0).min(255) as u8;
                    dst[boff + x] = (b as i16).max(0).min(255) as u8;
                }
                roff += dstrides[0];
                goff += dstrides[1];
                boff += dstrides[2];
                yoff += istrides[0];
                if (y & uhmask) == uhmask {
                    uoff += istrides[1];
                }
                if (y & vhmask) == vhmask {
                    voff += istrides[2];
                }
            }
        }
    }
}

pub fn create_yuv2rgb() -> Box<dyn Kernel> {
    Box::new(YuvToRgb::new())
}
