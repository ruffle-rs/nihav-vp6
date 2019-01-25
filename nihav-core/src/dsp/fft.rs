use std::f32::{self, consts};
use std::ops::{Not, Neg, Add, AddAssign, Sub, SubAssign, Mul, MulAssign};
use std::fmt;

#[derive(Debug,Clone,Copy,PartialEq)]
pub struct FFTComplex {
    pub re: f32,
    pub im: f32,
}

impl FFTComplex {
    pub fn exp(val: f32) -> Self {
        FFTComplex { re: val.cos(), im: val.sin() }
    }
    pub fn rotate(self) -> Self {
        FFTComplex { re: -self.im, im: self.re }
    }
    pub fn scale(self, scale: f32) -> Self {
        FFTComplex { re: self.re * scale, im: self.im * scale }
    }
}

impl Neg for FFTComplex {
    type Output = FFTComplex;
    fn neg(self) -> Self::Output {
        FFTComplex { re: -self.re, im: -self.im }
    }
}

impl Not for FFTComplex {
    type Output = FFTComplex;
    fn not(self) -> Self::Output {
        FFTComplex { re: self.re, im: -self.im }
    }
}

impl Add for FFTComplex {
    type Output = FFTComplex;
    fn add(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re + other.re, im: self.im + other.im }
    }
}

impl AddAssign for FFTComplex {
    fn add_assign(&mut self, other: Self) {
        self.re += other.re;
        self.im += other.im;
    }
}

impl Sub for FFTComplex {
    type Output = FFTComplex;
    fn sub(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re - other.re, im: self.im - other.im }
    }
}

impl SubAssign for FFTComplex {
    fn sub_assign(&mut self, other: Self) {
        self.re -= other.re;
        self.im -= other.im;
    }
}

impl Mul for FFTComplex {
    type Output = FFTComplex;
    fn mul(self, other: Self) -> Self::Output {
        FFTComplex { re: self.re * other.re - self.im * other.im,
                     im: self.im * other.re + self.re * other.im }
    }
}

impl MulAssign for FFTComplex {
    fn mul_assign(&mut self, other: Self) {
        let re = self.re * other.re - self.im * other.im;
        let im = self.im * other.re + self.re * other.im;
        self.re = re;
        self.im = im;
    }
}

impl fmt::Display for FFTComplex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.re, self.im)
    }
}

pub const FFTC_ZERO: FFTComplex = FFTComplex { re: 0.0, im: 0.0 };

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum FFTMode {
    Matrix,
    CooleyTukey,
    SplitRadix,
}

pub struct FFT {
    table:   Vec<FFTComplex>,
    perms:   Vec<usize>,
    swaps:   Vec<usize>,
    bits:    u32,
    mode:    FFTMode,
}

impl FFT {
    fn do_fft_inplace_ct(&mut self, data: &mut [FFTComplex], bits: u32, forward: bool) {
        if bits == 0 { return; }
        if bits == 1 {
            let sum01 = data[0] + data[1];
            let dif01 = data[0] - data[1];
            data[0] = sum01;
            data[1] = dif01;
            return;
        }
        if bits == 2 {
            let sum01 = data[0] + data[1];
            let dif01 = data[0] - data[1];
            let sum23 = data[2] + data[3];
            let dif23 = data[2] - data[3];
            if forward {
                data[0] = sum01 + sum23;
                data[1] = dif01 - dif23.rotate();
                data[2] = sum01 - sum23;
                data[3] = dif01 + dif23.rotate();
            } else {
                data[0] = sum01 + sum23;
                data[1] = dif01 + dif23.rotate();
                data[2] = sum01 - sum23;
                data[3] = dif01 - dif23.rotate();
            }
            return;
        }

        let hsize = (1 << (bits - 1)) as usize;
        self.do_fft_inplace_ct(&mut data[0..hsize], bits - 1, forward);
        self.do_fft_inplace_ct(&mut data[hsize..],  bits - 1, forward);
        let offs = hsize;
        {
            let e = data[0];
            let o = data[hsize];
            data[0]     = e + o;
            data[hsize] = e - o;
        }
        if forward {
            for k in 1..hsize {
                let e = data[k];
                let o = data[k + hsize] * self.table[offs + k];
                data[k]         = e + o;
                data[k + hsize] = e - o;
            }
        } else {
            for k in 1..hsize {
                let e = data[k];
                let o = data[k + hsize] * !self.table[offs + k];
                data[k]         = e + o;
                data[k + hsize] = e - o;
            }
        }
    }

    fn do_fft_inplace_splitradix(&mut self, data: &mut [FFTComplex], bits: u32, forward: bool) {
        if bits == 0 { return; }
        if bits == 1 {
            let sum01 = data[0] + data[1];
            let dif01 = data[0] - data[1];
            data[0] = sum01;
            data[1] = dif01;
            return;
        }
        if bits == 2 {
            let sum01 = data[0] + data[2];
            let dif01 = data[0] - data[2];
            let sum23 = data[1] + data[3];
            let dif23 = data[1] - data[3];
            if forward {
                data[0] = sum01 + sum23;
                data[1] = dif01 - dif23.rotate();
                data[2] = sum01 - sum23;
                data[3] = dif01 + dif23.rotate();
            } else {
                data[0] = sum01 + sum23;
                data[1] = dif01 + dif23.rotate();
                data[2] = sum01 - sum23;
                data[3] = dif01 - dif23.rotate();
            }
            return;
        }
        let qsize = (1 << (bits - 2)) as usize;
        let hsize = (1 << (bits - 1)) as usize;
        let q3size = qsize + hsize;

        self.do_fft_inplace_splitradix(&mut data[0     ..hsize],  bits - 1, forward);
        self.do_fft_inplace_splitradix(&mut data[hsize ..q3size], bits - 2, forward);
        self.do_fft_inplace_splitradix(&mut data[q3size..],       bits - 2, forward);
        let off = hsize;
        if forward {
            {
                let t3 =  data[0 + hsize] + data[0 + q3size];
                let t4 = (data[0 + hsize] - data[0 + q3size]).rotate();
                let e1 = data[0];
                let e2 = data[0 + qsize];
                data[0]          = e1 + t3;
                data[0 + qsize]  = e2 - t4;
                data[0 + hsize]  = e1 - t3;
                data[0 + q3size] = e2 + t4;
            }
            for k in 1..qsize {
                let t1 = self.table[off + k * 2 + 0] * data[k + hsize];
                let t2 = self.table[off + k * 2 + 1] * data[k + q3size];
                let t3 =  t1 + t2;
                let t4 = (t1 - t2).rotate();
                let e1 = data[k];
                let e2 = data[k + qsize];
                data[k]             = e1 + t3;
                data[k + qsize]     = e2 - t4;
                data[k + hsize]     = e1 - t3;
                data[k + qsize * 3] = e2 + t4;
            }
        } else {
            {
                let t3 =  data[0 + hsize] + data[0 + q3size];
                let t4 = (data[0 + hsize] - data[0 + q3size]).rotate();
                let e1 = data[0];
                let e2 = data[0 + qsize];
                data[0]          = e1 + t3;
                data[0 + qsize]  = e2 + t4;
                data[0 + hsize]  = e1 - t3;
                data[0 + q3size] = e2 - t4;
            }
            for k in 1..qsize {
                let t1 = !self.table[off + k * 2 + 0] * data[k + hsize];
                let t2 = !self.table[off + k * 2 + 1] * data[k + q3size];
                let t3 =  t1 + t2;
                let t4 = (t1 - t2).rotate();
                let e1 = data[k];
                let e2 = data[k + qsize];
                data[k]             = e1 + t3;
                data[k + qsize]     = e2 + t4;
                data[k + hsize]     = e1 - t3;
                data[k + qsize * 3] = e2 - t4;
            }
        }
    }

    pub fn do_fft(&mut self, src: &[FFTComplex], dst: &mut [FFTComplex], forward: bool) {
        match self.mode {
            FFTMode::Matrix => {
                    let base = if forward { -consts::PI * 2.0 / (src.len() as f32) }
                               else       {  consts::PI * 2.0 / (src.len() as f32) };
                    for k in 0..src.len() {
                        let mut sum = FFTC_ZERO;
                        for n in 0..src.len() {
                            let w = FFTComplex::exp(base * ((n * k) as f32));
                            sum += src[n] * w;
                        }
                        dst[k] = sum;
                    }
                },
            FFTMode::CooleyTukey => {
                    let bits = self.bits;
                    for k in 0..src.len() { dst[k] = src[self.perms[k]]; }
                    self.do_fft_inplace_ct(dst, bits, forward);
                },
            FFTMode::SplitRadix => {
                    let bits = self.bits;
                    for k in 0..src.len() { dst[k] = src[self.perms[k]]; }
                    self.do_fft_inplace_splitradix(dst, bits, forward);
                },
        };
    }

    pub fn do_fft_inplace(&mut self, data: &mut [FFTComplex], forward: bool) {
        for idx in 0..self.swaps.len() {
            let nidx = self.swaps[idx];
            if idx != nidx {
                let t      = data[nidx];
                data[nidx] = data[idx];
                data[idx]  = t;
            }
        }
        match self.mode {
            FFTMode::Matrix => {
                    let size = (1 << self.bits) as usize;
                    let base = if forward { -consts::PI * 2.0 / (size as f32) }
                               else       {  consts::PI * 2.0 / (size as f32) };
                    let mut res: Vec<FFTComplex> = Vec::with_capacity(size);
                    for k in 0..size {
                        let mut sum = FFTC_ZERO;
                        for n in 0..size {
                            let w = FFTComplex::exp(base * ((n * k) as f32));
                            sum += data[n] * w;
                        }
                        res.push(sum);
                    }
                    for k in 0..size {
                        data[k] = res[k];
                    }
                },
            FFTMode::CooleyTukey => {
                    let bits = self.bits;
                    self.do_fft_inplace_ct(data, bits, forward);
                },
            FFTMode::SplitRadix => {
                    let bits = self.bits;
                    self.do_fft_inplace_splitradix(data, bits, forward);
                },
        };
    }
}

pub struct FFTBuilder {
}

fn reverse_bits(inval: u32) -> u32 {
    const REV_TAB: [u8; 16] = [
        0b0000, 0b1000, 0b0100, 0b1100, 0b0010, 0b1010, 0b0110, 0b1110,
        0b0001, 0b1001, 0b0101, 0b1101, 0b0011, 0b1011, 0b0111, 0b1111,
    ];

    let mut ret = 0;
    let mut val = inval;
    for _ in 0..8 {
        ret = (ret << 4) | (REV_TAB[(val & 0xF) as usize] as u32);
        val = val >> 4;
    }
    ret
}

fn swp_idx(idx: usize, bits: u32) -> usize {
    let s = reverse_bits(idx as u32) as usize;
    s >> (32 - bits)
}

fn gen_sr_perms(swaps: &mut [usize], size: usize) {
    if size <= 4 { return; }
    let mut evec:  Vec<usize> = Vec::with_capacity(size / 2);
    let mut ovec1: Vec<usize> = Vec::with_capacity(size / 4);
    let mut ovec2: Vec<usize> = Vec::with_capacity(size / 4);
    for k in 0..size/4 {
        evec.push (swaps[k * 4 + 0]);
        ovec1.push(swaps[k * 4 + 1]);
        evec.push (swaps[k * 4 + 2]);
        ovec2.push(swaps[k * 4 + 3]);
    }
    for k in 0..size/2 { swaps[k]            = evec[k]; }
    for k in 0..size/4 { swaps[k +   size/2] = ovec1[k]; }
    for k in 0..size/4 { swaps[k + 3*size/4] = ovec2[k]; }
    gen_sr_perms(&mut swaps[0..size/2],        size/2);
    gen_sr_perms(&mut swaps[size/2..3*size/4], size/4);
    gen_sr_perms(&mut swaps[3*size/4..],       size/4);
}

fn gen_swaps_for_perm(swaps: &mut Vec<usize>, perms: &Vec<usize>) {
    let mut idx_arr: Vec<usize> = Vec::with_capacity(perms.len());
    for i in 0..perms.len() { idx_arr.push(i); }
    let mut run_size = 0;
    let mut run_pos  = 0;
    for idx in 0..perms.len() {
        if perms[idx] == idx_arr[idx] {
            if run_size == 0 { run_pos = idx; }
            run_size += 1;
        } else {
            for i in 0..run_size {
                swaps.push(run_pos + i);
            }
            run_size = 0;
            let mut spos = idx + 1;
            while idx_arr[spos] != perms[idx] { spos += 1; }
            idx_arr[spos] = idx_arr[idx];
            idx_arr[idx]  = perms[idx];
            swaps.push(spos);
        }
    }
}

impl FFTBuilder {
    pub fn new_fft(mode: FFTMode, size: usize) -> FFT {
        let mut swaps: Vec<usize>;
        let mut perms: Vec<usize>;
        let mut table: Vec<FFTComplex>;
        let bits = 31 - (size as u32).leading_zeros();
        match mode {
            FFTMode::Matrix => {
                    swaps = Vec::new();
                    perms = Vec::new();
                    table = Vec::new();
                },
            FFTMode::CooleyTukey => {
                    perms = Vec::with_capacity(size);
                    for i in 0..size {
                        perms.push(swp_idx(i, bits));
                    }
                    swaps = Vec::with_capacity(size);
                    table = Vec::with_capacity(size);
                    for _ in 0..4 { table.push(FFTC_ZERO); }
                    for b in 3..(bits+1) {
                        let hsize = (1 << (b - 1)) as usize;
                        let base = -consts::PI / (hsize as f32);
                        for k in 0..hsize {
                            table.push(FFTComplex::exp(base * (k as f32)));
                        }
                    }
                },
            FFTMode::SplitRadix => {
                    perms = Vec::with_capacity(size);
                    for i in 0..size {
                        perms.push(i);
                    }
                    gen_sr_perms(perms.as_mut_slice(), 1 << bits);
                    swaps = Vec::with_capacity(size);
                    table = Vec::with_capacity(size);
                    for _ in 0..4 { table.push(FFTC_ZERO); }
                    for b in 3..(bits+1) {
                        let qsize = (1 << (b - 2)) as usize;
                        let base = -consts::PI / ((qsize * 2) as f32);
                        for k in 0..qsize {
                            table.push(FFTComplex::exp(base * ((k * 1) as f32)));
                            table.push(FFTComplex::exp(base * ((k * 3) as f32)));
                        }
                    }
                },
        };
        gen_swaps_for_perm(&mut swaps, &perms);
        FFT { mode: mode, swaps: swaps, perms: perms, bits: bits, table: table }
    }
}

pub struct RDFT {
    table:  Vec<FFTComplex>,
    fft:    FFT,
    fwd:    bool,
    size:   usize,
}

fn crossadd(a: &FFTComplex, b: &FFTComplex) -> FFTComplex {
    FFTComplex { re: a.re + b.re, im: a.im - b.im }
}

impl RDFT {
    pub fn do_rdft(&mut self, src: &[FFTComplex], dst: &mut [FFTComplex]) {
        dst.copy_from_slice(src);
        self.do_rdft_inplace(dst);
    }
    pub fn do_rdft_inplace(&mut self, buf: &mut [FFTComplex]) {
        if !self.fwd {
            for n in 0..self.size/2 {
                let in0 = buf[n + 1];
                let in1 = buf[self.size - n - 1];

                let t0 = crossadd(&in0, &in1);
                let t1 = FFTComplex { re: in1.im + in0.im, im: in1.re - in0.re };
                let tab = self.table[n];
                let t2 = FFTComplex { re: t1.im * tab.im + t1.re * tab.re, im: t1.im * tab.re - t1.re * tab.im };

                buf[n + 1] = FFTComplex { re: t0.im - t2.im, im: t0.re - t2.re }; // (t0 - t2).conj().rotate()
                buf[self.size - n - 1] = (t0 + t2).rotate();
            }
            let a = buf[0].re;
            let b = buf[0].im;
            buf[0].re = a - b;
            buf[0].im = a + b;
        }
        self.fft.do_fft_inplace(buf, true);
        if self.fwd {
            for n in 0..self.size/2 {
                let in0 = buf[n + 1];
                let in1 = buf[self.size - n - 1];

                let t0 = crossadd(&in0, &in1).scale(0.5);
                let t1 = FFTComplex { re: in0.im + in1.im, im: in0.re - in1.re };
                let t2 = t1 * self.table[n];

                buf[n + 1] = crossadd(&t0, &t2);
                buf[self.size - n - 1] = FFTComplex { re: t0.re - t2.re, im: -(t0.im + t2.im) }; 
            }
            let a = buf[0].re;
            let b = buf[0].im;
            buf[0].re = a + b;
            buf[0].im = a - b;
        } else {
            for n in 0..self.size {
                buf[n] = FFTComplex{ re: buf[n].im, im: buf[n].re };
            }
        }
    }
}

pub struct RDFTBuilder {
}

impl RDFTBuilder {
    pub fn new_rdft(mode: FFTMode, size: usize, forward: bool) -> RDFT {
        let mut table: Vec<FFTComplex> = Vec::with_capacity(size / 4);
        let (base, scale) = if forward { (consts::PI / (size as f32), 0.5) } else { (-consts::PI / (size as f32), 1.0) };
        for i in 0..size/2 {
            table.push(FFTComplex::exp(base * ((i + 1) as f32)).scale(scale));
        }
        let fft = FFTBuilder::new_fft(mode, size);
        RDFT { table, fft, size, fwd: forward }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fft() {
        let mut fin:   [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut fout1: [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut fout2: [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut fout3: [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut fft1 = FFTBuilder::new_fft(FFTMode::Matrix,      fin.len());
        let mut fft2 = FFTBuilder::new_fft(FFTMode::CooleyTukey, fin.len());
        let mut fft3 = FFTBuilder::new_fft(FFTMode::SplitRadix,  fin.len());
        let mut seed: u32 = 42;
        for i in 0..fin.len() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = (seed >> 16) as i16;
            fin[i].re = (val as f32) / 256.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = (seed >> 16) as i16;
            fin[i].im = (val as f32) / 256.0;
        }
        fft1.do_fft(&fin, &mut fout1, true);
        fft2.do_fft(&fin, &mut fout2, true);
        fft3.do_fft(&fin, &mut fout3, true);

        for i in 0..fin.len() {
            assert!((fout1[i].re - fout2[i].re).abs() < 1.0);
            assert!((fout1[i].im - fout2[i].im).abs() < 1.0);
            assert!((fout1[i].re - fout3[i].re).abs() < 1.0);
            assert!((fout1[i].im - fout3[i].im).abs() < 1.0);
        }
        fft1.do_fft_inplace(&mut fout1, false);
        fft2.do_fft_inplace(&mut fout2, false);
        fft3.do_fft_inplace(&mut fout3, false);

        let sc = 1.0 / (fin.len() as f32);
        for i in 0..fin.len() {
            assert!((fin[i].re - fout1[i].re * sc).abs() < 1.0);
            assert!((fin[i].im - fout1[i].im * sc).abs() < 1.0);
            assert!((fout1[i].re - fout2[i].re).abs() < 1.0);
            assert!((fout1[i].im - fout2[i].im).abs() < 1.0);
            assert!((fout1[i].re - fout3[i].re).abs() < 1.0);
            assert!((fout1[i].im - fout3[i].im).abs() < 1.0);
        }
    }

    #[test]
    fn test_rdft() {
        let mut fin:   [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut fout1: [FFTComplex; 128] = [FFTC_ZERO; 128];
        let mut rdft = RDFTBuilder::new_rdft(FFTMode::SplitRadix,  fin.len(), true);
        let mut seed: u32 = 42;
        for i in 0..fin.len() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = (seed >> 16) as i16;
            fin[i].re = (val as f32) / 256.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = (seed >> 16) as i16;
            fin[i].im = (val as f32) / 256.0;
        }
        rdft.do_rdft(&fin, &mut fout1);
        let mut irdft = RDFTBuilder::new_rdft(FFTMode::SplitRadix,  fin.len(), false);
        irdft.do_rdft_inplace(&mut fout1);

        for i in 0..fin.len() {
            let tst = fout1[i].scale(0.5/(fout1.len() as f32));
            assert!((tst.re - fin[i].re).abs() < 1.0);
            assert!((tst.im - fin[i].im).abs() < 1.0);
        }
    }
}
