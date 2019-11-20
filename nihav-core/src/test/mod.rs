pub mod dec_video;
pub mod wavwriter;

mod md5; // for internal checksums only

pub enum ExpectedTestResult {
    Decodes,
    MD5([u32; 4]),
    MD5Frames(Vec<[u32; 4]>),
    GenerateMD5Frames,
}
