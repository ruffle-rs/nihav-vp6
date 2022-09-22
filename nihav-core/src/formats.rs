//! Audio and image sample format definitions.
//!
//! NihAV does not have a fixed list of supported formats but rather accepts format definitions both for audio and video.
//! In result exotic formats like YUV410+alpha plane that is used by Indeo 4 are supported without any additional case handing.
//! Some common format definitions are provided as constants for convenience.
use std::string::*;
use std::fmt;

/// Generic format parsing error.
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct FormatParseError {}

/// A list of RGB colour model variants.
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

/// A list of YUV colour model variants.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum YUVSubmodel {
    YCbCr,
    /// NTSC variant.
    YIQ,
    /// The YUV variant used by JPEG.
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

/// A list of known colour models.
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
    /// Returns the number of colour model components.
    ///
    /// The actual image may have more components e.g. alpha component.
    pub fn get_default_components(self) -> usize {
        match self {
            ColorModel::CMYK => 4,
            _                => 3,
        }
    }
    /// Reports whether the current colour model is RGB.
    pub fn is_rgb(self) -> bool {
        matches!(self, ColorModel::RGB(_))
    }
    /// Reports whether the current colour model is YUV.
    pub fn is_yuv(self) -> bool {
        matches!(self, ColorModel::YUV(_))
    }
    /// Returns short name for the current colour mode.
    pub fn get_short_name(self) -> &'static str {
        match self {
            ColorModel::RGB(_)   => "rgb",
            ColorModel::YUV(_)   => "yuv",
            ColorModel::CMYK     => "cmyk",
            ColorModel::HSV      => "hsv",
            ColorModel::LAB      => "lab",
            ColorModel::XYZ      => "xyz",
        }
    }
}

impl fmt::Display for ColorModel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match *self {
            ColorModel::RGB(fmt) => format!("RGB({})", fmt),
            ColorModel::YUV(fmt) => format!("YUV({})", fmt),
            ColorModel::CMYK     => "CMYK".to_string(),
            ColorModel::HSV      => "HSV".to_string(),
            ColorModel::LAB      => "LAB".to_string(),
            ColorModel::XYZ      => "XYZ".to_string(),
        };
        write!(f, "{}", name)
    }
}

/// Single colourspace component definition.
///
/// This structure defines how components of a colourspace are subsampled and where and how they are stored.
#[derive(Clone,Copy,PartialEq)]
pub struct NAPixelChromaton {
    /// Horizontal subsampling in power of two (e.g. `0` = no subsampling, `1` = only every second value is stored).
    pub h_ss:           u8,
    /// Vertial subsampling in power of two (e.g. `0` = no subsampling, `1` = only every second value is stored).
    pub v_ss:           u8,
    /// A flag to signal that component is packed.
    pub packed:         bool,
    /// Bit depth of current component.
    pub depth:          u8,
    /// Shift for packed components.
    pub shift:          u8,
    /// Component offset for byte-packed components.
    pub comp_offs:      u8,
    /// The distance to the next packed element in bytes.
    pub next_elem:      u8,
}

/// Flag for specifying that image data is stored big-endian in `NAPixelFormaton::`[`new`]`()`. Related to its [`be`] field.
///
/// [`new`]: ./struct.NAPixelFormaton.html#method.new
/// [`be`]: ./struct.NAPixelFormaton.html#structfield.new
pub const FORMATON_FLAG_BE      :u32 = 0x01;
/// Flag for specifying that image data has alpha plane in `NAPixelFormaton::`[`new`]`()`. Related to its [`alpha`] field.
///
/// [`new`]: ./struct.NAPixelFormaton.html#method.new
/// [`alpha`]: ./struct.NAPixelFormaton.html#structfield.alpha
pub const FORMATON_FLAG_ALPHA   :u32 = 0x02;
/// Flag for specifying that image data is stored in paletted form for `NAPixelFormaton::`[`new`]`()`. Related to its [`palette`] field.
///
/// [`new`]: ./struct.NAPixelFormaton.html#method.new
/// [`palette`]: ./struct.NAPixelFormaton.html#structfield.palette
pub const FORMATON_FLAG_PALETTE :u32 = 0x04;

/// The current limit on number of components in image colourspace model (including alpha component).
pub const MAX_CHROMATONS: usize = 5;

/// Image colourspace representation.
///
/// This structure includes both definitions for each component and some common definitions.
/// For example the format can be paletted and then components describe the palette storage format while actual data is 8-bit palette indices.
#[derive(Clone,Copy,PartialEq)]
pub struct NAPixelFormaton {
    /// Image colour model.
    pub model:      ColorModel,
    /// Actual number of components present.
    pub components: u8,
    /// Format definition for each component.
    pub comp_info:  [Option<NAPixelChromaton>; MAX_CHROMATONS],
    /// Single pixel size for packed formats.
    pub elem_size:  u8,
    /// A flag signalling that data is stored as big-endian.
    pub be:         bool,
    /// A flag signalling that image has alpha component.
    pub alpha:      bool,
    /// A flag signalling that data is paletted.
    ///
    /// This means that image data is stored as 8-bit indices (in the first image component) for the palette stored as second component of the image and actual palette format is described in this structure.
    pub palette:    bool,
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

/// Predefined format for planar 8-bit YUV with 4:2:0 subsampling.
pub const YUV420_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 1, 1, 1),
                                            chromaton!(yuv8; 1, 1, 2),
                                            None, None],
                                        elem_size: 0, be: false, alpha: false, palette: false };

/// Predefined format for planar 8-bit YUV with 4:1:0 subsampling.
pub const YUV410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            None, None],
                                        elem_size: 0, be: false, alpha: false, palette: false };
/// Predefined format for planar 8-bit YUV with 4:1:0 subsampling and alpha component.
pub const YUVA410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::YUV(YUVSubmodel::YUVJ), components: 4,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            chromaton!(0, 0, false, 8, 0, 3, 1),
                                            None],
                                        elem_size: 0, be: false, alpha: true, palette: false };

/// Predefined format with RGB24 palette.
pub const PAL8_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(pal8; 0),
                                            chromaton!(pal8; 1),
                                            chromaton!(pal8; 2),
                                            None, None],
                                        elem_size: 3, be: false, alpha: false, palette: true };

/// Predefined format for RGB565 packed video.
pub const RGB565_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(packrgb; 5, 11, 0, 2),
                                            chromaton!(packrgb; 6,  5, 0, 2),
                                            chromaton!(packrgb; 5,  0, 0, 2),
                                            None, None],
                                        elem_size: 2, be: false, alpha: false, palette: false };

/// Predefined format for RGB24.
pub const RGB24_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColorModel::RGB(RGBSubmodel::RGB), components: 3,
                                        comp_info: [
                                            chromaton!(packrgb; 8, 0, 0, 3),
                                            chromaton!(packrgb; 8, 0, 1, 3),
                                            chromaton!(packrgb; 8, 0, 2, 3),
                                            None, None],
                                        elem_size: 3, be: false, alpha: false, palette: false };

impl NAPixelChromaton {
    /// Constructs a new `NAPixelChromaton` instance.
    pub fn new(h_ss: u8, v_ss: u8, packed: bool, depth: u8, shift: u8, comp_offs: u8, next_elem: u8) -> Self {
        Self { h_ss, v_ss, packed, depth, shift, comp_offs, next_elem }
    }
    /// Returns subsampling for the current component.
    pub fn get_subsampling(self) -> (u8, u8) { (self.h_ss, self.v_ss) }
    /// Reports whether current component is packed.
    pub fn is_packed(self) -> bool { self.packed }
    /// Returns bit depth of current component.
    pub fn get_depth(self) -> u8   { self.depth }
    /// Returns bit shift for packed component.
    pub fn get_shift(self) -> u8   { self.shift }
    /// Returns byte offset for packed component.
    pub fn get_offset(self) -> u8  { self.comp_offs }
    /// Returns byte offset to the next element of current packed component.
    pub fn get_step(self)  -> u8   { self.next_elem }

    /// Calculates the width for current component from general image width.
    pub fn get_width(self, width: usize) -> usize {
        (width  + ((1 << self.h_ss) - 1)) >> self.h_ss
    }
    /// Calculates the height for current component from general image height.
    pub fn get_height(self, height: usize) -> usize {
        (height + ((1 << self.v_ss) - 1)) >> self.v_ss
    }
    /// Calculates the minimal stride for current component from general image width.
    pub fn get_linesize(self, width: usize) -> usize {
        let d = self.depth as usize;
        if self.packed {
            (self.get_width(width) * d + d - 1) >> 3
        } else {
            self.get_width(width)
        }
    }
    /// Calculates the required image size in pixels for current component from general image width.
    pub fn get_data_size(self, width: usize, height: usize) -> usize {
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
    /// Constructs a new instance of `NAPixelFormaton`.
    pub fn new(model: ColorModel,
               comp1: Option<NAPixelChromaton>,
               comp2: Option<NAPixelChromaton>,
               comp3: Option<NAPixelChromaton>,
               comp4: Option<NAPixelChromaton>,
               comp5: Option<NAPixelChromaton>,
               flags: u32, elem_size: u8) -> Self {
        let mut chromatons: [Option<NAPixelChromaton>; MAX_CHROMATONS] = [None; MAX_CHROMATONS];
        let mut ncomp = 0;
        let be      = (flags & FORMATON_FLAG_BE)      != 0;
        let alpha   = (flags & FORMATON_FLAG_ALPHA)   != 0;
        let palette = (flags & FORMATON_FLAG_PALETTE) != 0;
        if let Some(c) = comp1 { chromatons[0] = Some(c); ncomp += 1; }
        if let Some(c) = comp2 { chromatons[1] = Some(c); ncomp += 1; }
        if let Some(c) = comp3 { chromatons[2] = Some(c); ncomp += 1; }
        if let Some(c) = comp4 { chromatons[3] = Some(c); ncomp += 1; }
        if let Some(c) = comp5 { chromatons[4] = Some(c); ncomp += 1; }
        NAPixelFormaton { model,
                          components: ncomp,
                          comp_info: chromatons,
                          elem_size,
                          be, alpha, palette }
    }

    /// Returns current colour model.
    pub fn get_model(&self) -> ColorModel { self.model }
    /// Returns the number of components.
    pub fn get_num_comp(&self) -> usize { self.components as usize }
    /// Returns selected component information.
    pub fn get_chromaton(&self, idx: usize) -> Option<NAPixelChromaton> {
        if idx < self.comp_info.len() { return self.comp_info[idx]; }
        None
    }
    /// Reports whether the packing format is big-endian.
    pub fn is_be(self) -> bool { self.be }
    /// Reports whether colourspace has alpha component.
    pub fn has_alpha(self) -> bool { self.alpha }
    /// Reports whether this is paletted format.
    pub fn is_paletted(self) -> bool { self.palette }
    /// Returns single packed pixel size.
    pub fn get_elem_size(self) -> u8 { self.elem_size }
    /// Reports whether the format is not packed.
    pub fn is_unpacked(&self) -> bool {
        if self.palette { return false; }
        for chr in self.comp_info.iter() {
            if let Some(ref chromaton) = chr {
                if chromaton.is_packed() { return false; }
            }
        }
        true
    }
    /// Returns the maximum component bit depth.
    pub fn get_max_depth(&self) -> u8 {
        let mut mdepth = 0;
        for chr in self.comp_info.iter() {
            if let Some(ref chromaton) = chr {
                mdepth = mdepth.max(chromaton.depth);
            }
        }
        mdepth
    }
    /// Returns the total amount of bits needed for components.
    pub fn get_total_depth(&self) -> u8 {
        let mut depth = 0;
        for chr in self.comp_info.iter() {
            if let Some(ref chromaton) = chr {
                depth += chromaton.depth;
            }
        }
        depth
    }
    /// Returns the maximum component subsampling.
    pub fn get_max_subsampling(&self) -> u8 {
        let mut ssamp = 0;
        for chr in self.comp_info.iter() {
            if let Some(ref chromaton) = chr {
                let (ss_v, ss_h) = chromaton.get_subsampling();
                ssamp = ssamp.max(ss_v).max(ss_h);
            }
        }
        ssamp
    }
    #[allow(clippy::cognitive_complexity)]
    /// Returns a short string description of the format if possible.
    pub fn to_short_string(&self) -> Option<String> {
        match self.model {
            ColorModel::RGB(_) => {
                if self.is_paletted() {
                    if *self == PAL8_FORMAT {
                        return Some("pal8".to_string());
                    } else {
                        return None;
                    }
                }
                let mut name = [b'z'; 4];
                let planar = self.is_unpacked();

                let mut start_off = 0;
                let mut start_shift = 0;
                let mut use_shift = true;
                for comp in self.comp_info.iter() {
                    if let Some(comp) = comp {
                        start_off = start_off.min(comp.comp_offs);
                        start_shift = start_shift.min(comp.shift);
                        if comp.comp_offs != 0 { use_shift = false; }
                    }
                }
                for component in 0..(self.components as usize) {
                    for (comp, cname) in self.comp_info.iter().zip(b"rgba".iter()) {
                        if let Some(comp) = comp {
                            if use_shift {
                                if comp.shift == start_shift {
                                    name[component] = *cname;
                                    start_shift += comp.depth;
                                }
                            } else if comp.comp_offs == start_off {
                                name[component] = *cname;
                                if planar {
                                    start_off += 1;
                                } else {
                                    start_off += (comp.depth + 7) / 8;
                                }
                            }
                        }
                    }
                }

                for (comp, cname) in self.comp_info.iter().zip(b"rgba".iter()) {
                    if let Some(comp) = comp {
                        name[comp.comp_offs as usize] = *cname;
                    } else {
                        break;
                    }
                }
                let mut name = String::from_utf8(name[..self.components as usize].to_vec()).unwrap();
                let depth = self.get_total_depth();
                if depth == 15 || depth == 16 {
                    for c in self.comp_info.iter() {
                        if let Some(comp) = c {
                            name.push((b'0' + comp.depth) as char);
                        } else {
                            break;
                        }
                    }
                    name += if self.be { "be" } else { "le" };
                    return Some(name);
                }
                if depth == 24 || depth != 8 * self.components {
                    name += depth.to_string().as_str();
                }
                if planar {
                    name.push('p');
                }
                if self.get_max_depth() > 8 {
                    name += if self.be { "be" } else { "le" };
                }
                Some(name)
            },
            ColorModel::YUV(_) => {
                let max_depth = self.get_max_depth();
                if self.get_total_depth() != max_depth * self.components {
                    return None;
                }
                if self.components < 3 {
                    if self.components == 1 && max_depth == 8 {
                        return Some("y8".to_string());
                    }
                    if self.components == 2 && self.alpha && max_depth == 8 {
                        return Some("y8a".to_string());
                    }
                    return None;
                }
                let cu = self.comp_info[1].unwrap();
                let cv = self.comp_info[2].unwrap();
                if cu.h_ss != cv.h_ss || cu.v_ss != cv.v_ss || cu.h_ss > 2 || cu.v_ss > 2 {
                    return None;
                }
                let mut name = "yuv".to_string();
                if self.alpha {
                    name.push('a');
                }
                name.push('4');
                let sch = b"421"[cu.h_ss as usize];
                let tch = if cu.v_ss > 1 { b'0' } else { sch };
                name.push(sch as char);
                name.push(tch as char);
                if self.is_unpacked() {
                    name.push('p');
                }
                if max_depth != 8 {
                    name += max_depth.to_string().as_str();
                }
                Some(name)
            },
            _ => None,
        }
    }
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
        println!("formaton yuv- {}", YUV420_FORMAT);
        println!("formaton pal- {}", PAL8_FORMAT);
        println!("formaton rgb565- {}", RGB565_FORMAT);

        assert_eq!(RGB565_FORMAT.to_short_string().unwrap(), "bgr565le");
        assert_eq!(PAL8_FORMAT.to_short_string().unwrap(), "pal8");
        assert_eq!(YUV420_FORMAT.to_short_string().unwrap(), "yuv422p");
        assert_eq!(YUVA410_FORMAT.to_short_string().unwrap(), "yuva410p");
    }
}
