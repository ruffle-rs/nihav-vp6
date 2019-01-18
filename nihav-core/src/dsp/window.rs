use std::f32::consts;

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum WindowType {
    Square,
    Sine,
    KaiserBessel(f32),
}

pub fn generate_window(mode: WindowType, scale: f32, size: usize, half: bool, dst: &mut [f32]) {
    match mode {
        WindowType::Square => {
                for n in 0..size { dst[n] = scale; }
            },
        WindowType::Sine => {
                let param;
                if half {
                    param = consts::PI / ((2 * size) as f32);
                } else {
                    param = consts::PI / (size as f32);
                }
                for n in 0..size {
                    dst[n] = (((n as f32) + 0.5) * param).sin() * scale;
                }
            },
        WindowType::KaiserBessel(alpha) => {
                let dlen = if half { size as f32 } else { (size as f32) * 0.5 };
                let alpha2 = ((alpha * consts::PI / dlen) * (alpha * consts::PI / dlen)) as f64;

                let mut kb: Vec<f64> = Vec::with_capacity(size);
                let mut sum = 0.0;
                for n in 0..size {
                    let b = bessel_i0(((n * (size - n)) as f64) * alpha2);
                    sum += b;
                    kb.push(sum);
                }
                sum += 1.0;
                for n in 0..size {
                    dst[n] = (kb[n] / sum).sqrt() as f32;
                }
            },
    };
}

fn bessel_i0(inval: f64) -> f64 {
    let mut val: f64 = 1.0;
    for n in (1..64).rev() {
        val *= inval / ((n * n) as f64);
        val += 1.0;
    }
    val
}