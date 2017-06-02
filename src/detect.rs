use std::io::SeekFrom;
use io::byteio::ByteReader;

#[derive(Debug,Clone,Copy,PartialEq)]
pub enum DetectionScore {
    No,
    ExtensionMatches,
    MagicMatches,
}

impl DetectionScore {
    pub fn less(&self, other: DetectionScore) -> bool {
        (*self as i32) < (other as i32)
    }
}

#[allow(dead_code)]
enum CondArg {
    Byte(u8),
    U16BE(u16),
    U16LE(u16),
    U24BE(u32),
    U24LE(u32),
    U32BE(u32),
    U32LE(u32),
    U64BE(u64),
    U64LE(u64),
}

impl CondArg {
    fn val(&self) -> u64 {
        match *self {
            CondArg::Byte(b) => { b as u64 }
            CondArg::U16BE(v) => { v as u64 }
            CondArg::U16LE(v) => { v as u64 }
            CondArg::U24BE(v) => { v as u64 }
            CondArg::U24LE(v) => { v as u64 }
            CondArg::U32BE(v) => { v as u64 }
            CondArg::U32LE(v) => { v as u64 }
            CondArg::U64BE(v) => { v }
            CondArg::U64LE(v) => { v }
        }
    }
    fn read_val(&self, src: &mut ByteReader) -> Option<u64> {
        match *self {
            CondArg::Byte(_) => {
                let res = src.peek_byte();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U16BE(_) => {
                let res = src.peek_u16be();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U16LE(_) => {
                let res = src.peek_u16le();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U24BE(_) => {
                let res = src.peek_u24be();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U24LE(_) => {
                let res = src.peek_u24le();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U32BE(_) => {
                let res = src.peek_u32be();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U32LE(_) => {
                let res = src.peek_u32le();
                if let Err(_) = res { return None; }
                Some(res.unwrap() as u64)
            }
            CondArg::U64BE(_) => {
                let res = src.peek_u64be();
                if let Err(_) = res { return None; }
                Some(res.unwrap())
            }
            CondArg::U64LE(_) => {
                let res = src.peek_u64le();
                if let Err(_) = res { return None; }
                Some(res.unwrap())
            }
        }
    }
    fn eq(&self, src: &mut ByteReader) -> bool {
        let val = self.read_val(src);
        if let None = val { false }
        else { val.unwrap() == self.val() }
    }
    fn ge(&self, src: &mut ByteReader) -> bool {
        let val = self.read_val(src);
        if let None = val { false }
        else { val.unwrap() >= self.val() }
    }
    fn gt(&self, src: &mut ByteReader) -> bool {
        let val = self.read_val(src);
        if let None = val { false }
        else { val.unwrap() > self.val() }
    }
    fn le(&self, src: &mut ByteReader) -> bool {
        let val = self.read_val(src);
        if let None = val { false }
        else { val.unwrap() <= self.val() }
    }
    fn lt(&self, src: &mut ByteReader) -> bool {
        let val = self.read_val(src);
        if let None = val { false }
        else { val.unwrap() < self.val() }
    }
}

#[allow(dead_code)]
enum Cond<'a> {
    And(&'a Cond<'a>, &'a Cond<'a>),
    Or(&'a Cond<'a>, &'a Cond<'a>),
    Equals(CondArg),
    EqualsString(&'static [u8]),
    InRange(CondArg, CondArg),
    LessThan(CondArg),
    GreaterThan(CondArg),
}

impl<'a> Cond<'a> {
    fn eval(&self, src: &mut ByteReader) -> bool {
        match *self {
            Cond::And(ref a, ref b) => { a.eval(src) && b.eval(src) },
            Cond::Or (ref a, ref b) => { a.eval(src) || b.eval(src) },
            Cond::Equals(ref arg)       => { arg.eq(src) },
            Cond::InRange(ref a, ref b) => { a.le(src) && b.ge(src) },
            Cond::LessThan(ref arg)     => { arg.lt(src) },
            Cond::GreaterThan(ref arg)  => { arg.gt(src) },
            Cond::EqualsString(str) => {
                let mut val: Vec<u8> = Vec::with_capacity(str.len());
                val.resize(str.len(), 0);
                let res = src.peek_buf(val.as_mut_slice());
                if let Err(_) = res { return false; }
                val == str
            }
        }
    }
}

struct CheckItem<'a> {
    offs: u32,
    cond: &'a Cond<'a>,
}

#[allow(dead_code)]
struct DetectConditions<'a> {
    demux_name: &'static str,
    extensions: &'static str,
    conditions: &'a [CheckItem<'a>],
}

const DETECTORS: &[DetectConditions] = &[
    DetectConditions {
        demux_name: "avi",
        extensions: ".avi",
        conditions: &[CheckItem{offs:0, cond: &Cond::Or(
                                                &Cond::EqualsString(b"RIFX"),
                                                &Cond::EqualsString(b"RIFF")) },
                      CheckItem{offs:8, cond: &Cond::EqualsString(b"AVI LIST") }
                     ],
    },
    DetectConditions {
        demux_name: "gdv",
        extensions: ".gdv",
        conditions: &[CheckItem{offs:0, cond: &Cond::Equals(CondArg::U32LE(0x29111994))}],
    },
];

pub fn detect_format(name: &str, src: &mut ByteReader) -> Option<(&'static str, DetectionScore)> {
    let mut result = None;
    let lname = name.to_lowercase();
    for detector in DETECTORS {
        let mut score = DetectionScore::No;
        if name.len() > 0 {
            for ext in detector.extensions.split(',') {
                if lname.ends_with(ext) {
                    score = DetectionScore::ExtensionMatches;
                    break;
                }
            }
        }
        let mut passed = detector.conditions.len() > 0;
        for ck in detector.conditions {
            let ret = src.seek(SeekFrom::Start(ck.offs as u64));
            if let Err(_) = ret {
                passed = false;
                break;
            }
            if !ck.cond.eval(src) {
                passed = false;
                break;
            }
        }
        if passed {
            score = DetectionScore::MagicMatches;
        }
        if score == DetectionScore::MagicMatches {
            return Some((detector.demux_name, score));
        }
        if let None = result {
            result = Some((detector.demux_name, score));
        } else {
            let (_, oldsc) = result.unwrap();
            if oldsc.less(score) {
                result = Some((detector.demux_name, score));
            }
        }
    }
    result
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use io::byteio::*;

    #[test]
    fn test_avi_detect() {
        let name = "assets/laser05.avi";
        let mut file = File::open(name).unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let (name, score) = detect_format(name, &mut br).unwrap();
        assert_eq!(name, "avi");
        assert_eq!(score, DetectionScore::MagicMatches);
    }

    #[test]
    fn test_gdv_detect() {
        let name = "assets/intro1.gdv";
        let mut file = File::open(name).unwrap();
        let mut fr = FileReader::new_read(&mut file);
        let mut br = ByteReader::new(&mut fr);
        let (name, score) = detect_format(name, &mut br).unwrap();
        assert_eq!(name, "gdv");
        assert_eq!(score, DetectionScore::MagicMatches);
    }
}