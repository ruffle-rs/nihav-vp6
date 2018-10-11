use std::str::FromStr;
use std::string::*;
use std::fmt;

#[derive(Debug,Copy,Clone,PartialEq)]
pub struct NASoniton {
    bits:       u8,
    be:         bool,
    packed:     bool,
    planar:     bool,
    float:      bool,
    signed:     bool,
}

pub const SONITON_FLAG_BE     :u32 = 0x01;
pub const SONITON_FLAG_PACKED :u32 = 0x02;
pub const SONITON_FLAG_PLANAR :u32 = 0x04;
pub const SONITON_FLAG_FLOAT  :u32 = 0x08;
pub const SONITON_FLAG_SIGNED :u32 = 0x10;

pub const SND_U8_FORMAT: NASoniton = NASoniton { bits: 8, be: false, packed: false, planar: false, float: false, signed: false };
pub const SND_S16_FORMAT: NASoniton = NASoniton { bits: 16, be: false, packed: false, planar: false, float: false, signed: true };
pub const SND_F32P_FORMAT: NASoniton = NASoniton { bits: 32, be: false, packed: false, planar: true, float: true, signed: true };

impl NASoniton {
    pub fn new(bits: u8, flags: u32) -> Self {
        let is_be = (flags & SONITON_FLAG_BE) != 0;
        let is_pk = (flags & SONITON_FLAG_PACKED) != 0;
        let is_pl = (flags & SONITON_FLAG_PLANAR) != 0;
        let is_fl = (flags & SONITON_FLAG_FLOAT) != 0;
        let is_sg = (flags & SONITON_FLAG_SIGNED) != 0;
        NASoniton { bits: bits, be: is_be, packed: is_pk, planar: is_pl, float: is_fl, signed: is_sg }
    }

    pub fn get_bits(&self)  -> u8   { self.bits }
    pub fn is_be(&self)     -> bool { self.be }
    pub fn is_packed(&self) -> bool { self.packed }
    pub fn is_planar(&self) -> bool { self.planar }
    pub fn is_float(&self)  -> bool { self.float }
    pub fn is_signed(&self) -> bool { self.signed }

    pub fn get_audio_size(&self, length: u64) -> usize {
        if self.packed {
            ((length * (self.bits as u64) + 7) >> 3) as usize
        } else {
            (length * (((self.bits + 7) >> 3) as u64)) as usize
        }
    }
}

impl fmt::Display for NASoniton {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fmt = if self.float { "float" } else if self.signed { "int" } else { "uint" };
        let end = if self.be { "BE" } else { "LE" };
        write!(f, "({} bps, {} planar: {} packed: {} {})", self.bits, end, self.packed, self.planar, fmt)
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum NAChannelType {
    C, L, R, Cs, Ls, Rs, Lss, Rss, LFE, Lc, Rc, Lh, Rh, Ch, LFE2, Lw, Rw, Ov, Lhs, Rhs, Chs, Ll, Rl, Cl, Lt, Rt, Lo, Ro
}

impl NAChannelType {
    pub fn is_center(&self) -> bool {
        match *self {
            NAChannelType::C => true,   NAChannelType::Ch => true,
            NAChannelType::Cl => true,  NAChannelType::Ov => true,
            NAChannelType::LFE => true, NAChannelType::LFE2 => true,
            NAChannelType::Cs => true,  NAChannelType::Chs => true,
            _ => false,
        }
    }
    pub fn is_left(&self) -> bool {
        match *self {
            NAChannelType::L   => true, NAChannelType::Ls => true,
            NAChannelType::Lss => true, NAChannelType::Lc => true,
            NAChannelType::Lh  => true, NAChannelType::Lw => true,
            NAChannelType::Lhs => true, NAChannelType::Ll => true,
            NAChannelType::Lt  => true, NAChannelType::Lo => true,
            _ => false,
        }
    }
    pub fn is_right(&self) -> bool {
        match *self {
            NAChannelType::R   => true, NAChannelType::Rs => true,
            NAChannelType::Rss => true, NAChannelType::Rc => true,
            NAChannelType::Rh  => true, NAChannelType::Rw => true,
            NAChannelType::Rhs => true, NAChannelType::Rl => true,
            NAChannelType::Rt  => true, NAChannelType::Ro => true,
            _ => false,
        }
    }
}

#[derive(Clone,Copy,Debug,PartialEq)]
pub struct ChannelParseError {}

impl FromStr for NAChannelType {
    type Err = ChannelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "C"     => Ok(NAChannelType::C),
            "L"     => Ok(NAChannelType::L),
            "R"     => Ok(NAChannelType::R),
            "Cs"    => Ok(NAChannelType::Cs),
            "Ls"    => Ok(NAChannelType::Ls),
            "Rs"    => Ok(NAChannelType::Rs),
            "Lss"   => Ok(NAChannelType::Lss),
            "Rss"   => Ok(NAChannelType::Rss),
            "LFE"   => Ok(NAChannelType::LFE),
            "Lc"    => Ok(NAChannelType::Lc),
            "Rc"    => Ok(NAChannelType::Rc),
            "Lh"    => Ok(NAChannelType::Lh),
            "Rh"    => Ok(NAChannelType::Rh),
            "Ch"    => Ok(NAChannelType::Ch),
            "LFE2"  => Ok(NAChannelType::LFE2),
            "Lw"    => Ok(NAChannelType::Lw),
            "Rw"    => Ok(NAChannelType::Rw),
            "Ov"    => Ok(NAChannelType::Ov),
            "Lhs"   => Ok(NAChannelType::Lhs),
            "Rhs"   => Ok(NAChannelType::Rhs),
            "Chs"   => Ok(NAChannelType::Chs),
            "Ll"    => Ok(NAChannelType::Ll),
            "Rl"    => Ok(NAChannelType::Rl),
            "Cl"    => Ok(NAChannelType::Cl),
            "Lt"    => Ok(NAChannelType::Lt),
            "Rt"    => Ok(NAChannelType::Rt),
            "Lo"    => Ok(NAChannelType::Lo),
            "Ro"    => Ok(NAChannelType::Ro),
            _   => Err(ChannelParseError{}),
        }        
    }
}

impl fmt::Display for NAChannelType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            NAChannelType::C    => "C".to_string(),
            NAChannelType::L    => "L".to_string(),
            NAChannelType::R    => "R".to_string(),
            NAChannelType::Cs   => "Cs".to_string(),
            NAChannelType::Ls   => "Ls".to_string(),
            NAChannelType::Rs   => "Rs".to_string(),
            NAChannelType::Lss  => "Lss".to_string(),
            NAChannelType::Rss  => "Rss".to_string(),
            NAChannelType::LFE  => "LFE".to_string(),
            NAChannelType::Lc   => "Lc".to_string(),
            NAChannelType::Rc   => "Rc".to_string(),
            NAChannelType::Lh   => "Lh".to_string(),
            NAChannelType::Rh   => "Rh".to_string(),
            NAChannelType::Ch   => "Ch".to_string(),
            NAChannelType::LFE2 => "LFE2".to_string(),
            NAChannelType::Lw   => "Lw".to_string(),
            NAChannelType::Rw   => "Rw".to_string(),
            NAChannelType::Ov   => "Ov".to_string(),
            NAChannelType::Lhs  => "Lhs".to_string(),
            NAChannelType::Rhs  => "Rhs".to_string(),
            NAChannelType::Chs  => "Chs".to_string(),
            NAChannelType::Ll   => "Ll".to_string(),
            NAChannelType::Rl   => "Rl".to_string(),
            NAChannelType::Cl   => "Cl".to_string(),
            NAChannelType::Lt   => "Lt".to_string(),
            NAChannelType::Rt   => "Rt".to_string(),
            NAChannelType::Lo   => "Lo".to_string(),
            NAChannelType::Ro   => "Ro".to_string(),
        };
        write!(f, "{}", name)
    }
}

#[derive(Clone)]
pub struct NAChannelMap {
    ids: Vec<NAChannelType>,
}

const MS_CHANNEL_MAP: [NAChannelType; 11] = [
    NAChannelType::L,
    NAChannelType::R,
    NAChannelType::C,
    NAChannelType::LFE,
    NAChannelType::Ls,
    NAChannelType::Rs,
    NAChannelType::Lss,
    NAChannelType::Rss,
    NAChannelType::Cs,
    NAChannelType::Lc,
    NAChannelType::Rc,
];

impl NAChannelMap {
    pub fn new() -> Self { NAChannelMap { ids: Vec::new() } }
    pub fn add_channel(&mut self, ch: NAChannelType) {
        self.ids.push(ch);
    }
    pub fn add_channels(&mut self, chs: &[NAChannelType]) {
        for i in 0..chs.len() {
            self.ids.push(chs[i]);
        }
    }
    pub fn num_channels(&self) -> usize {
        self.ids.len()
    }
    pub fn get_channel(&self, idx: usize) -> NAChannelType {
        self.ids[idx]
    }
    pub fn find_channel_id(&self, t: NAChannelType) -> Option<u8> {
        for i in 0..self.ids.len() {
            if self.ids[i] as i32 == t as i32 { return Some(i as u8); }
        }
        None
    }
    pub fn from_ms_mapping(chmap: u32) -> Self {
        let mut cm = NAChannelMap::new();
        for i in 0..MS_CHANNEL_MAP.len() {
            if ((chmap >> i) & 1) != 0 {
                cm.add_channel(MS_CHANNEL_MAP[i]);
            }
        }
        cm
    }
}

impl fmt::Display for NAChannelMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut map = String::new();
        for el in self.ids.iter() {
            if map.len() > 0 { map.push(','); }
            map.push_str(&*el.to_string());
        }
        write!(f, "{}", map)
    }
}

impl FromStr for NAChannelMap {
    type Err = ChannelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chm = NAChannelMap::new();
        for tok in s.split(',') {
            chm.add_channel(NAChannelType::from_str(tok)?);
        }
        Ok(chm)
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum RGBSubmodel {
    RGB,
    SRGB,
}

impl fmt::Display for RGBSubmodel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            RGBSubmodel::RGB  => "RGB".to_string(),
            RGBSubmodel::SRGB => "sRGB".to_string(),
        };
        write!(f, "{}", name)
    }
}

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum YUVSubmodel {
    YCbCr,
    YIQ,
    YUVJ,
}

impl fmt::Display for YUVSubmodel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            YUVSubmodel::YCbCr => "YCbCr".to_string(),
            YUVSubmodel::YIQ   => "YIQ".to_string(),
            YUVSubmodel::YUVJ  => "YUVJ".to_string(),
        };
        write!(f, "{}", name)
    }
}

#[derive(Debug, Clone,Copy,PartialEq)]
pub enum ColorModel {
    RGB(RGBSubmodel),
    YUV(YUVSubmodel),
    CMYK,
    HSV,
    LAB,
    XYZ,
}

impl ColorModel {
    pub fn get_default_components(&self) -> usize {
        match *self {
            ColorModel::CMYK => 4,
            _                => 3,
        }
    }
}

impl fmt::Display for ColorModel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            ColorModel::RGB(fmt) => format!("RGB({})", fmt).to_string(),
            ColorModel::YUV(fmt) => format!("YUV({})", fmt).to_string(),
            ColorModel::CMYK     => "CMYK".to_string(),
            ColorModel::HSV      => "HSV".to_string(),
            ColorModel::LAB      => "LAB".to_string(),
            ColorModel::XYZ      => "XYZ".to_string(),
        };
        write!(f, "{}", name)
    }
}

#[derive(Clone,Copy,PartialEq)]
pub struct NAPixelChromaton {
    h_ss:           u8,
    v_ss:           u8,
    packed:         bool,
    depth:          u8,
    shift:          u8,
    comp_offs:      u8,
    next_elem:      u8,
}

pub const FORMATON_FLAG_BE      :u32 = 0x01;
pub const FORMATON_FLAG_ALPHA   :u32 = 0x02;
pub const FORMATON_FLAG_PALETTE :u32 = 0x04;


#[derive(Clone,Copy,PartialEq)]
pub struct NAPixelFormaton {
    model:      ColorModel,
    components: u8,
    comp_info:  [Option<NAPixelChromaton>; 5],
    elem_size:  u8,
    be:         bool,
    alpha:      bool,
    palette:    bool,
}

macro_rules! chromaton {
    ($hs: expr, $vs: expr, $pck: expr, $d: expr, $sh: expr, $co: expr, $ne: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, packed: $pck, depth: $d, shift: $sh, comp_offs: $co, next_elem: $ne })
    });
    (yuv8; $hs: expr, $vs: expr, $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, packed: false, depth: 8, shift: 0, comp_offs: $co, next_elem: 1 })
    });
    (packrgb; $d: expr, $s: expr, $co: expr, $ne: expr) => ({
        Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: $d, shift: $s, comp_offs: $co, next_elem: $ne })
    });
    (pal8; $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: 8, shift: 0, comp_offs: $co, next_elem: 3 })
    });
}

pub const YUV420_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 1, 1, 1),
                                            chromaton!(yuv8; 1, 1, 2),
                                            None, None],
                                        elem_size: 0, be: false, alpha: false, palette: false };

pub const YUV410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            None, None],
                                        elem_size: 0, be: false, alpha: false, palette: false };
pub const YUVA410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 4,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            chromaton!(0, 0, false, 8, 0, 3, 1),
                                            None],
                                        elem_size: 0, be: false, alpha: true, palette: false };

pub const PAL8_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(pal8; 0),
                                            chromaton!(pal8; 1),
                                            chromaton!(pal8; 2),
                                            None, None],
                                        elem_size: 3, be: false, alpha: false, palette: true };

pub const RGB565_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(packrgb; 5, 11, 0, 2),
                                            chromaton!(packrgb; 6,  5, 0, 2),
                                            chromaton!(packrgb; 5,  0, 0, 2),
                                            None, None],
                                        elem_size: 2, be: false, alpha: false, palette: false };

pub const RGB24_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(packrgb; 8, 0, 2, 3),
                                            chromaton!(packrgb; 8, 0, 1, 3),
                                            chromaton!(packrgb; 8, 0, 0, 3),
                                            None, None],
                                        elem_size: 3, be: false, alpha: false, palette: false };

impl NAPixelChromaton {
    pub fn get_subsampling(&self) -> (u8, u8) { (self.h_ss, self.v_ss) }
    pub fn is_packed(&self) -> bool { self.packed }
    pub fn get_depth(&self) -> u8   { self.depth }
    pub fn get_shift(&self) -> u8   { self.shift }
    pub fn get_offset(&self) -> u8  { self.comp_offs }
    pub fn get_step(&self)  -> u8   { self.next_elem }

    pub fn get_width(&self, width: usize) -> usize {
        (width  + ((1 << self.h_ss) - 1)) >> self.h_ss
    }
    pub fn get_height(&self, height: usize) -> usize {
        (height + ((1 << self.v_ss) - 1)) >> self.v_ss
    }
    pub fn get_linesize(&self, width: usize) -> usize {
        let d = self.depth as usize;
        if self.packed {
            (self.get_width(width) * d + d - 1) >> 3
        } else {
            self.get_width(width)
        }
    }
    pub fn get_data_size(&self, width: usize, height: usize) -> usize {
        let nh = (height + ((1 << self.v_ss) - 1)) >> self.v_ss;
        self.get_linesize(width) * nh
    }
}

impl fmt::Display for NAPixelChromaton {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let pfmt = if self.packed {
            let mask = ((1 << self.depth) - 1) << self.shift;
            format!("packed(+{},{:X}, step {})", self.comp_offs, mask, self.next_elem)
        } else {
            format!("planar({},{})", self.comp_offs, self.next_elem)
        };
        write!(f, "({}x{}, {})", self.h_ss, self.v_ss, pfmt)
    }
}

impl NAPixelFormaton {
    pub fn new(model: ColorModel,
               comp1: Option<NAPixelChromaton>,
               comp2: Option<NAPixelChromaton>,
               comp3: Option<NAPixelChromaton>,
               comp4: Option<NAPixelChromaton>,
               comp5: Option<NAPixelChromaton>,
               flags: u32, elem_size: u8) -> Self {
        let mut chromatons: [Option<NAPixelChromaton>; 5] = [None; 5];
        let mut ncomp = 0;
        let be      = (flags & FORMATON_FLAG_BE)      != 0;
        let alpha   = (flags & FORMATON_FLAG_ALPHA)   != 0;
        let palette = (flags & FORMATON_FLAG_PALETTE) != 0;
        if let Some(c) = comp1 { chromatons[0] = Some(c); ncomp += 1; }
        if let Some(c) = comp2 { chromatons[1] = Some(c); ncomp += 1; }
        if let Some(c) = comp3 { chromatons[2] = Some(c); ncomp += 1; }
        if let Some(c) = comp4 { chromatons[3] = Some(c); ncomp += 1; }
        if let Some(c) = comp5 { chromatons[4] = Some(c); ncomp += 1; }
        NAPixelFormaton { model: model,
                          components: ncomp,
                          comp_info: chromatons,
                          elem_size: elem_size,
                         be: be, alpha: alpha, palette: palette }
    }

    pub fn get_model(&self) -> ColorModel { self.model }
    pub fn get_num_comp(&self) -> usize { self.components as usize }
    pub fn get_chromaton(&self, idx: usize) -> Option<NAPixelChromaton> {
        if idx < self.comp_info.len() { return self.comp_info[idx]; }
        None
    }
    pub fn is_be(&self) -> bool { self.be }
    pub fn has_alpha(&self) -> bool { self.alpha }
    pub fn is_paletted(&self) -> bool { self.palette }
    pub fn get_elem_size(&self) -> u8 { self.elem_size }
}

impl fmt::Display for NAPixelFormaton {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let end = if self.be { "BE" } else { "LE" };
        let palstr = if self.palette { "palette " } else { "" };
        let astr = if self.alpha { "alpha " } else { "" };
        let mut str = format!("Formaton for {} ({}{}elem {} size {}): ", self.model, palstr, astr,end, self.elem_size);
        for i in 0..self.comp_info.len() {
            if let Some(chr) = self.comp_info[i] {
                str = format!("{} {}", str, chr);
            }
        }
        write!(f, "[{}]", str)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fmt() {
        println!("{}", SND_S16_FORMAT);
        println!("{}", SND_U8_FORMAT);
        println!("{}", SND_F32P_FORMAT);
        println!("formaton yuv- {}", YUV420_FORMAT);
        println!("formaton pal- {}", PAL8_FORMAT);
        println!("formaton rgb565- {}", RGB565_FORMAT);
    }
}
