use io::codebook::CodebookDescReader;

#[allow(dead_code)]
pub const H263_SCALES: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 6, 7, 8, 9, 9, 10, 10, 11, 11, 12, 12, 12, 13, 13, 13, 14, 14, 14, 14, 14, 15, 15, 15, 15, 15 ];

pub const H263_ZIGZAG: &[usize] = &[
    0,   1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63
];

pub const H263_SIZES: &[(usize, usize)] = &[
    (0, 0), (128, 96), (176, 144), (352, 288), (704, 576), (1408, 1152)
];

pub const H263_INTRA_MCBPC: &[(u8, u8)] = &[
    (1, 1), (1, 3), (2, 3), (3, 3), (1, 4), (1, 6), (2, 6), (3, 6), (1, 9)
];

pub const H263_INTER_MCBPC: &[(u8, u8)] = &[
    (1, 1), (3, 4), (2, 4), (5, 6), (3, 5), (4, 8), (3, 8), (3, 7),
    (3, 3), (7, 7), (6, 7), (5, 9), (4, 6), (4, 9), (3, 9), (2, 9),
    (2, 3), (5, 7), (4, 7), (5, 8), (1, 9), (0, 0), (0, 0), (0, 0),
    (2, 11), (12, 13), (14, 13), (15, 13)
];

pub const H263_CBPY: &[(u8, u8)] = &[
    ( 3, 4), ( 5, 5), ( 4, 5), ( 9, 4), ( 3, 5), ( 7, 4), ( 2, 6), (11, 4),
    ( 2, 5), ( 3, 6), ( 5, 4), (10, 4), ( 4, 4), ( 8, 4), ( 6, 4), ( 3, 2)
];

pub const H263_MV: &[(u8, u8)] = &[
    ( 1,  1), ( 1,  2), ( 1,  3), ( 1,  4), ( 3,  6), ( 5,  7), ( 4,  7), ( 3,  7),
    (11,  9), (10,  9), ( 9,  9), (17, 10), (16, 10), (15, 10), (14, 10), (13, 10),
    (12, 10), (11, 10), (10, 10), ( 9, 10), ( 8, 10), ( 7, 10), ( 6, 10), ( 5, 10),
    ( 4, 10), ( 7, 11), ( 6, 11), ( 5, 11), ( 4, 11), ( 3, 11), ( 2, 11), ( 3, 12),
    ( 2, 12)
];

pub const H263_DQUANT_TAB: &[i8] = &[-1, -2, 1, 2];

pub struct H263ShortCodeReader { tab: &'static [(u8, u8)] }

impl H263ShortCodeReader {
    pub fn new(tab: &'static [(u8, u8)]) -> Self { H263ShortCodeReader { tab: tab } }
}

impl CodebookDescReader<u8> for H263ShortCodeReader {
    fn bits(&mut self, idx: usize) -> u8  { let (_, bits) = self.tab[idx]; bits }
    fn code(&mut self, idx: usize) -> u32 { let (code, _) = self.tab[idx]; code as u32 }
    fn sym (&mut self, idx: usize) -> u8 { idx as u8 }
    fn len(&mut self) -> usize { self.tab.len() }
}

#[derive(Clone,Copy)]
pub struct H263RLSym { run: u8, level: i8 }
impl H263RLSym {
    pub fn get_run(&self)   -> u8 { self.run }
    pub fn is_last(&self)   -> bool { self.level < 0 }
    pub fn is_escape(&self) -> bool { (self.run == 0) && (self.level == 0) }
    pub fn get_level(&self) -> i16 { if self.level < 0 { -self.level as i16 } else { self.level as i16 } }
}

pub struct H263RLCodeDesc { code: u8, bits: u8, sym: H263RLSym }

macro_rules! rlcodes{
    ($(($c:expr, $b:expr, $r:expr, $l:expr)),*) => {
        &[$(H263RLCodeDesc{ code: $c, bits: $b, sym: H263RLSym{ run: $r, level: $l }}),*]
    }
}

pub const H263_RL_CODES: &[H263RLCodeDesc] = rlcodes!(
    (0x02,  2,  0,  1), (0x0F,  4,  0,  2), (0x15,  6,  0,  3), (0x17,  7,  0,  4),
    (0x1F,  8,  0,  5), (0x25,  9,  0,  6), (0x24,  9,  0,  7), (0x21, 10,  0,  8),
    (0x20, 10,  0,  9), (0x07, 11,  0, 10), (0x06, 11,  0, 11), (0x20, 11,  0, 12),
    (0x06,  3,  1,  1), (0x14,  6,  1,  2), (0x1E,  8,  1,  3), (0x0F, 10,  1,  4),
    (0x21, 11,  1,  5), (0x50, 12,  1,  6), (0x0E,  4,  2,  1), (0x1D,  8,  2,  2),
    (0x0E, 10,  2,  3), (0x51, 12,  2,  4), (0x0D,  5,  3,  1), (0x23,  9,  3,  2),
    (0x0D, 10,  3,  3), (0x0C,  5,  4,  1), (0x22,  9,  4,  2), (0x52, 12,  4,  3),
    (0x0B,  5,  5,  1), (0x0C, 10,  5,  2), (0x53, 12,  5,  3), (0x13,  6,  6,  1),
    (0x0B, 10,  6,  2), (0x54, 12,  6,  3), (0x12,  6,  7,  1), (0x0A, 10,  7,  2),
    (0x11,  6,  8,  1), (0x09, 10,  8,  2), (0x10,  6,  9,  1), (0x08, 10,  9,  2),
    (0x16,  7, 10,  1), (0x55, 12, 10,  2), (0x15,  7, 11,  1), (0x14,  7, 12,  1),
    (0x1C,  8, 13,  1), (0x1B,  8, 14,  1), (0x21,  9, 15,  1), (0x20,  9, 16,  1),
    (0x1F,  9, 17,  1), (0x1E,  9, 18,  1), (0x1D,  9, 19,  1), (0x1C,  9, 20,  1),
    (0x1B,  9, 21,  1), (0x1A,  9, 22,  1), (0x22, 11, 23,  1), (0x23, 11, 24,  1),
    (0x56, 12, 25,  1), (0x57, 12, 26,  1), (0x07,  4,  0, -1), (0x19,  9,  0, -2),
    (0x05, 11,  0, -3), (0x0F,  6,  1, -1), (0x04, 11,  1, -2), (0x0E,  6,  2, -1),
    (0x0D,  6,  3, -1), (0x0C,  6,  4, -1), (0x13,  7,  5, -1), (0x12,  7,  6, -1),
    (0x11,  7,  7, -1), (0x10,  7,  8, -1), (0x1A,  8,  9, -1), (0x19,  8, 10, -1),
    (0x18,  8, 11, -1), (0x17,  8, 12, -1), (0x16,  8, 13, -1), (0x15,  8, 14, -1),
    (0x14,  8, 15, -1), (0x13,  8, 16, -1), (0x18,  9, 17, -1), (0x17,  9, 18, -1),
    (0x16,  9, 19, -1), (0x15,  9, 20, -1), (0x14,  9, 21, -1), (0x13,  9, 22, -1),
    (0x12,  9, 23, -1), (0x11,  9, 24, -1), (0x07, 10, 25, -1), (0x06, 10, 26, -1),
    (0x05, 10, 27, -1), (0x04, 10, 28, -1), (0x24, 11, 29, -1), (0x25, 11, 30, -1),
    (0x26, 11, 31, -1), (0x27, 11, 32, -1), (0x58, 12, 33, -1), (0x59, 12, 34, -1),
    (0x5A, 12, 35, -1), (0x5B, 12, 36, -1), (0x5C, 12, 37, -1), (0x5D, 12, 38, -1),
    (0x5E, 12, 39, -1), (0x5F, 12, 40, -1), (0x03,  7,  0,  0)
);

pub const H263_RL_CODES_AIC: &[H263RLCodeDesc] = rlcodes!(
    (0x02,  2,  0,  1), (0x06,  3,  0,  2), (0x0E,  4,  0,  3), (0x0C,  5,  0,  4),
    (0x0D,  5,  0,  5), (0x10,  6,  0,  6), (0x11,  6,  0,  7), (0x12,  6,  0,  8),
    (0x16,  7,  0,  9), (0x1B,  8,  0, 10), (0x20,  9,  0, 11), (0x21,  9,  0, 12),
    (0x1A,  9,  0, 13), (0x1B,  9,  0, 14), (0x1C,  9,  0, 15), (0x1D,  9,  0, 16),
    (0x1E,  9,  0, 17), (0x1F,  9,  0, 18), (0x23, 11,  0, 19), (0x22, 11,  0, 20),
    (0x57, 12,  0, 21), (0x56, 12,  0, 22), (0x55, 12,  0, 23), (0x54, 12,  0, 24),
    (0x53, 12,  0, 25), (0x0F,  4,  1,  1), (0x14,  6,  1,  2), (0x14,  7,  1,  3),
    (0x1E,  8,  1,  4), (0x0F, 10,  1,  5), (0x21, 11,  1,  6), (0x50, 12,  1,  7),
    (0x0B,  5,  2,  1), (0x15,  7,  2,  2), (0x0E, 10,  2,  3), (0x09, 10,  2,  4),
    (0x15,  6,  3,  1), (0x1D,  8,  3,  2), (0x0D, 10,  3,  3), (0x51, 12,  3,  4),
    (0x13,  6,  4,  1), (0x23,  9,  4,  2), (0x07, 11,  4,  3), (0x17,  7,  5,  1),
    (0x22,  9,  5,  2), (0x52, 12,  5,  3), (0x1C,  8,  6,  1), (0x0C, 10,  6,  2),
    (0x1F,  8,  7,  1), (0x0B, 10,  7,  2), (0x25,  9,  8,  1), (0x0A, 10,  8,  2),
    (0x24,  9,  9,  1), (0x06, 11,  9,  2), (0x21, 10, 10,  1), (0x20, 10, 11,  1),
    (0x08, 10, 12,  1), (0x20, 11, 13,  1), (0x07,  4,  0, -1), (0x0C,  6,  0, -2),
    (0x10,  7,  0, -3), (0x13,  8,  0, -4), (0x11,  9,  0, -5), (0x12,  9,  0, -6),
    (0x04, 10,  0, -7), (0x27, 11,  0, -8), (0x26, 11,  0, -9), (0x5F, 12,  0,-10),
    (0x0F,  6,  1, -1), (0x13,  9,  1, -2), (0x05, 10,  1, -3), (0x25, 11,  1, -4),
    (0x0E,  6,  2, -1), (0x14,  9,  2, -2), (0x24, 11,  2, -3), (0x0D,  6,  3, -1),
    (0x06, 10,  3, -2), (0x5E, 12,  3, -3), (0x11,  7,  4, -1), (0x07, 10,  4, -2),
    (0x13,  7,  5, -1), (0x5D, 12,  5, -2), (0x12,  7,  6, -1), (0x5C, 12,  6, -2),
    (0x14,  8,  7, -1), (0x5B, 12,  7, -2), (0x15,  8,  8, -1), (0x1A,  8,  9, -1),
    (0x19,  8, 10, -1), (0x18,  8, 11, -1), (0x17,  8, 12, -1), (0x16,  8, 13, -1),
    (0x19,  9, 14, -1), (0x15,  9, 15, -1), (0x16,  9, 16, -1), (0x18,  9, 17, -1),
    (0x17,  9, 18, -1), (0x04, 11, 19, -1), (0x05, 11, 20, -1), (0x58, 12, 21, -1),
    (0x59, 12, 22, -1), (0x5A, 12, 23, -1), (0x03,  7,  0,  0)
);

pub struct H263RLCodeReader { tab: &'static [H263RLCodeDesc] }

impl H263RLCodeReader {
    pub fn new(tab: &'static [H263RLCodeDesc]) -> Self { H263RLCodeReader { tab: tab } }
}

impl CodebookDescReader<H263RLSym> for H263RLCodeReader {
    fn bits(&mut self, idx: usize) -> u8  { self.tab[idx].bits }
    fn code(&mut self, idx: usize) -> u32 { self.tab[idx].code as u32 }
    fn sym (&mut self, idx: usize) -> H263RLSym { self.tab[idx].sym }
    fn len(&mut self) -> usize { self.tab.len() }
}
