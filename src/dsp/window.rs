use std::f32::consts;

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum WindowType {
    Square,
    Sine,
    KaiserBessel,
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
        WindowType::KaiserBessel => {
unimplemented!();
            },
    };
}
