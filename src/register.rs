use std::fmt;

#[derive(Debug,Clone,Copy,PartialEq)]
#[allow(dead_code)]
pub enum CodecType {
    Video,
    Audio,
    Subtitles,
    Data,
    None,
}

impl fmt::Display for CodecType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CodecType::Video => write!(f, "Video"),
            CodecType::Audio => write!(f, "Audio"),
            CodecType::Subtitles => write!(f, "Subtitles"),
            CodecType::Data => write!(f, "Data"),
            CodecType::None => write!(f, "-"),
        }
    }
}

const CODEC_CAP_INTRAONLY:u32   = 0x000001;
const CODEC_CAP_LOSSLESS:u32    = 0x000002;
const CODEC_CAP_REORDER:u32     = 0x000004;
const CODEC_CAP_HYBRID:u32      = 0x000008;
const CODEC_CAP_SCALABLE:u32    = 0x000010;

#[derive(Clone)]
pub struct CodecDescription {
    name:  &'static str,
    fname: &'static str,
    ctype: CodecType,
    caps:  u32,
}

impl CodecDescription {
    pub fn get_name(&self) -> &'static str { self.name }
    pub fn get_full_name(&self) -> &'static str { self.fname }
    pub fn get_codec_type(&self) -> CodecType { self.ctype }
    pub fn is_intraonly(&self) -> bool { (self.caps & CODEC_CAP_INTRAONLY) != 0 }
    pub fn is_lossless(&self)  -> bool { (self.caps & CODEC_CAP_LOSSLESS)  != 0 }
    pub fn has_reorder(&self)  -> bool { (self.caps & CODEC_CAP_REORDER)   != 0 }
    pub fn is_hybrid(&self)    -> bool { (self.caps & CODEC_CAP_HYBRID)    != 0 }
    pub fn is_scalable(&self)  -> bool { (self.caps & CODEC_CAP_SCALABLE)  != 0 }
}

impl fmt::Display for CodecDescription {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut out = format!("{}", self.fname);
        if self.caps != 0 {
            let mut capfmt = "".to_string();
            if (self.caps & CODEC_CAP_INTRAONLY) != 0 {
                capfmt = format!("{} Intra-only", capfmt);
            }
            if (self.caps & CODEC_CAP_LOSSLESS) != 0 {
                capfmt = format!("{} Lossless", capfmt);
            }
            if (self.caps & CODEC_CAP_REORDER) != 0 {
                capfmt = format!("{} Frame reorder", capfmt);
            }
            if (self.caps & CODEC_CAP_HYBRID) != 0 {
                capfmt = format!("{} Can be lossy and lossless", capfmt);
            }
            if (self.caps & CODEC_CAP_SCALABLE) != 0 {
                capfmt = format!("{} Scalable", capfmt);
            }
            out = format!("{} ({})", out, capfmt);
        }
        write!(f, "{}", out)
    }
}

macro_rules! desc {
    (video; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: 0 }
    });
    (video; $n:expr, $fn:expr, $c:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: $c }
    });
    (video-ll; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: CODEC_CAP_LOSSLESS | CODEC_CAP_INTRAONLY }
    });
    (video-llp; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: CODEC_CAP_LOSSLESS }
    });
    (video-im; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: CODEC_CAP_INTRAONLY }
    });
    (video-modern; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Video,
                          caps: CODEC_CAP_REORDER | CODEC_CAP_HYBRID }
    });
    (audio; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Audio,
                          caps: 0 }
    });
    (audio-ll; $n:expr, $fn:expr) => ({
        CodecDescription{ name: $n, fname: $fn, ctype: CodecType::Audio,
                          caps: CODEC_CAP_LOSSLESS | CODEC_CAP_INTRAONLY }
    });
}

pub fn get_codec_description(name: &str) -> Option<&'static CodecDescription> {
    for reg in CODEC_REGISTER {
        if reg.name == name {
            return Some(reg);
        }
    }
    None
}

static CODEC_REGISTER: &'static [CodecDescription] = &[
    desc!(video-im; "indeo1", "Intel Raw IF09"),
    desc!(video-im; "indeo2", "Intel Indeo 2"),
    desc!(video;    "indeo3", "Intel Indeo 3"),
    desc!(video;    "indeo4", "Intel Indeo 4", CODEC_CAP_REORDER | CODEC_CAP_SCALABLE),
    desc!(video;    "indeo5", "Intel Indeo 5", CODEC_CAP_REORDER | CODEC_CAP_SCALABLE),
    desc!(video;    "intel263", "Intel I263", CODEC_CAP_REORDER),
    desc!(audio;    "iac",    "Intel Indeo audio"),
    desc!(audio;    "imc",    "Intel Music Coder"),

    desc!(video;    "realvideo1", "Real Video 1"),
    desc!(video;    "realvideo2", "Real Video 2"),
    desc!(video;    "realvideo3", "Real Video 3", CODEC_CAP_REORDER),
    desc!(video;    "realvideo4", "Real Video 4", CODEC_CAP_REORDER),
    desc!(video;    "clearvideo", "ClearVideo"),
    desc!(audio;    "ra14.4",     "RealAudio 14.4"),
    desc!(audio;    "ra28.8",     "RealAudio 28.8"),
    desc!(audio;    "cook",       "RealAudio Cooker"),
    desc!(audio;    "ralf",       "RealAudio Lossless"),
    desc!(audio;    "aac",        "AAC"),
    desc!(audio;    "ac3",        "AC-3"),
    desc!(audio;    "atrac3",     "Sony Atrac3"),
    desc!(audio;    "sipro",      "Sipro Labs ADPCM"),
];

static AVI_VIDEO_CODEC_REGISTER: &'static [(&[u8;4], &str)] = &[
    (b"IF09", "indeo1"),
    (b"RT21", "indeo2"),
    (b"IV31", "indeo3"),
    (b"IV32", "indeo3"),
    (b"IV41", "indeo4"),
    (b"IV50", "indeo5"),
    (b"I263", "intel263"),

    (b"CLV1", "clearvideo"),
];

static WAV_CODEC_REGISTER: &'static [(u16, &str)] = &[
    (0x0000, "pcm"),
    (0x0001, "pcm"),
    (0x0003, "pcm"),
    (0x0401, "imc"),
    (0x0402, "iac"),
];

pub fn find_codec_from_avi_fourcc(fcc: &[u8;4]) -> Option<&'static str> {
    for i in 0..AVI_VIDEO_CODEC_REGISTER.len() {
        let (fourcc, name) = AVI_VIDEO_CODEC_REGISTER[i];
        if fourcc == fcc { return Some(name); }
    }
    None
}

pub fn find_codec_from_wav_twocc(tcc: u16) -> Option<&'static str> {
    for i in 0..WAV_CODEC_REGISTER.len() {
        let (twocc, name) = WAV_CODEC_REGISTER[i];
        if twocc == tcc { return Some(name); }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_register() {
        let c1 = find_codec_from_avi_fourcc(b"IV41").unwrap();
        let c2 = find_codec_from_wav_twocc(0x401).unwrap();
        println!("found {} and {}", c1, c2);
        let cd1 = get_codec_description(c1).unwrap();
        let cd2 = get_codec_description(c2).unwrap();
        println!("got {} and {}", cd1, cd2);
    }
}
