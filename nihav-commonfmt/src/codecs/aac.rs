use nihav_core::formats::*;
use nihav_core::frame::*;
use nihav_core::codecs::*;
use nihav_codec_support::dsp::mdct::IMDCT;
use nihav_codec_support::dsp::window::*;
use nihav_core::io::bitreader::*;
use nihav_core::io::codebook::*;
use std::fmt;
use nihav_core::io::intcode::*;
use std::mem;
use std::ptr;
use std::str::FromStr;
use std::f32::consts;

#[allow(non_camel_case_types)]
#[derive(Clone,Copy,PartialEq)]
enum M4AType {
    None,
    Main,
    LC,
    SSR,
    LTP,
    SBR,
    Scalable,
    TwinVQ,
    CELP,
    HVXC,
    TTSI,
    MainSynth,
    WavetableSynth,
    GeneralMIDI,
    Algorithmic,
    ER_AAC_LC,
    ER_AAC_LTP,
    ER_AAC_Scalable,
    ER_TwinVQ,
    ER_BSAC,
    ER_AAC_LD,
    ER_CELP,
    ER_HVXC,
    ER_HILN,
    ER_Parametric,
    SSC,
    PS,
    MPEGSurround,
    Layer1,
    Layer2,
    Layer3,
    DST,
    ALS,
    SLS,
    SLSNonCore,
    ER_AAC_ELD,
    SMRSimple,
    SMRMain,
    Reserved,
    Unknown,
}

const M4A_TYPES: &[M4AType] = &[
    M4AType::None,              M4AType::Main,      M4AType::LC,            M4AType::SSR,
    M4AType::LTP,               M4AType::SBR,       M4AType::Scalable,      M4AType::TwinVQ,
    M4AType::CELP,              M4AType::HVXC,      M4AType::Reserved,      M4AType::Reserved,
    M4AType::TTSI,              M4AType::MainSynth, M4AType::WavetableSynth, M4AType::GeneralMIDI,
    M4AType::Algorithmic,       M4AType::ER_AAC_LC, M4AType::Reserved,      M4AType::ER_AAC_LTP,
    M4AType::ER_AAC_Scalable,   M4AType::ER_TwinVQ, M4AType::ER_BSAC,       M4AType::ER_AAC_LD,
    M4AType::ER_CELP,           M4AType::ER_HVXC,   M4AType::ER_HILN,       M4AType::ER_Parametric,
    M4AType::SSC,               M4AType::PS,        M4AType::MPEGSurround,  M4AType::Reserved /*escape*/,
    M4AType::Layer1,            M4AType::Layer2,    M4AType::Layer3,        M4AType::DST,
    M4AType::ALS,               M4AType::SLS,       M4AType::SLSNonCore,    M4AType::ER_AAC_ELD,
    M4AType::SMRSimple,         M4AType::SMRMain,
];
const M4A_TYPE_NAMES: &[&str] = &[
    "None", "AAC Main", "AAC LC", "AAC SSR", "AAC LTP", "SBR", "AAC Scalable", "TwinVQ", "CELP", "HVXC",
    /*"(reserved10)", "(reserved11)", */ "TTSI",
    "Main synthetic", "Wavetable synthesis", "General MIDI", "Algorithmic Synthesis and Audio FX",
    "ER AAC LC", /*"(reserved18)",*/ "ER AAC LTP", "ER AAC Scalable", "ER TwinVQ", "ER BSAC", "ER AAC LD",
    "ER CELP", "ER HVXC", "ER HILN", "ER Parametric", "SSC", "PS", "MPEG Surround", /*"(escape)",*/
    "Layer-1", "Layer-2", "Layer-3", "DST", "ALS", "SLS", "SLS non-core", "ER AAC ELD", "SMR Simple", "SMR Main",
    "(reserved)", "(unknown)",
];

impl fmt::Display for M4AType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", M4A_TYPE_NAMES[*self as usize])
    }
}

const AAC_SAMPLE_RATES: [u32; 16] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050,
    16000, 12000, 11025,  8000,  7350, 0, 0, 0
];

const AAC_CHANNELS: [usize; 8] = [ 0, 1, 2, 3, 4, 5, 6, 8 ];

struct M4AInfo {
    otype:          M4AType,
    srate:          u32,
    channels:       usize,
    samples:        usize,
    sbr_ps_info:    Option<(u32, usize)>,
    sbr_present:    bool,
    ps_present:     bool,
}

impl M4AInfo {
    fn new() -> Self {
        Self {
            otype:          M4AType::None,
            srate:          0,
            channels:       0,
            samples:        0,
            sbr_ps_info:    Option::None,
            sbr_present:    false,
            ps_present:     false,
        }
    }
    fn read_object_type(br: &mut BitReader) -> DecoderResult<M4AType> {
        let otypeidx;
        if br.peek(5) == 31 {
                                                          br.skip(5)?;
            otypeidx                                    = (br.read(6)? as usize) + 32;
        } else {
            otypeidx                                    = br.read(5)? as usize;
        }
        if otypeidx >= M4A_TYPES.len() {
            Ok(M4AType::Unknown)
        } else {
            Ok(M4A_TYPES[otypeidx])
        }
    }
    fn read_sampling_frequency(br: &mut BitReader) -> DecoderResult<u32> {
        if br.peek(4) == 15 {
            let srate                                   = br.read(24)?;
            Ok(srate)
        } else {
            let srate_idx                               = br.read(4)? as usize;
            Ok(AAC_SAMPLE_RATES[srate_idx])
        }
    }
    fn read_channel_config(br: &mut BitReader) -> DecoderResult<usize> {
        let chidx                                       = br.read(4)? as usize;
        if chidx < AAC_CHANNELS.len() {
            Ok(AAC_CHANNELS[chidx])
        } else {
            Ok(chidx)
        }
    }
    fn read(&mut self, src: &[u8]) -> DecoderResult<()> {
        let mut br = BitReader::new(src, BitReaderMode::BE);
        self.otype = Self::read_object_type(&mut br)?;
        self.srate = Self::read_sampling_frequency(&mut br)?;
        validate!(self.srate > 0);
        self.channels = Self::read_channel_config(&mut br)?;

        if (self.otype == M4AType::SBR) || (self.otype == M4AType::PS) {
            let ext_srate = Self::read_sampling_frequency(&mut br)?;
            self.otype = Self::read_object_type(&mut br)?;
            let ext_chans;
            if self.otype == M4AType::ER_BSAC {
                ext_chans = Self::read_channel_config(&mut br)?;
            } else {
                ext_chans = 0;
            }
            self.sbr_ps_info = Some((ext_srate, ext_chans));
        }

        match self.otype {
            M4AType::Main | M4AType::LC | M4AType::SSR | M4AType::Scalable | M4AType::TwinVQ |
            M4AType::ER_AAC_LC | M4AType::ER_AAC_LTP | M4AType::ER_AAC_Scalable | M4AType::ER_TwinVQ |
            M4AType::ER_BSAC | M4AType::ER_AAC_LD => {
                // GASpecificConfig
                    let short_frame                     = br.read_bool()?;
                    self.samples = if short_frame { 960 } else { 1024 };
                    let depends_on_core                 = br.read_bool()?;
                    if depends_on_core {
                        let _delay                      = br.read(14)?;
                    }
                    let extension_flag                  = br.read_bool()?;
                    if self.channels == 0 {
                        unimplemented!("program config element");
                    }
                    if (self.otype == M4AType::Scalable) || (self.otype == M4AType::ER_AAC_Scalable) {
                        let _layer                      = br.read(3)?;
                    }
                    if extension_flag {
                        if self.otype == M4AType::ER_BSAC {
                            let _num_subframes          = br.read(5)? as usize;
                            let _layer_length           = br.read(11)?;
                        }
                        if (self.otype == M4AType::ER_AAC_LC) ||
                           (self.otype == M4AType::ER_AAC_LTP) ||
                           (self.otype == M4AType::ER_AAC_Scalable) ||
                           (self.otype == M4AType::ER_AAC_LD) {
                            let _section_data_resilience    = br.read_bool()?;
                            let _scalefactors_resilience    = br.read_bool()?;
                            let _spectral_data_resilience   = br.read_bool()?;
                        }
                        let extension_flag3             = br.read_bool()?;
                        if extension_flag3 {
                            unimplemented!("version3 extensions");
                        }
                    }
                },
            M4AType::CELP => { unimplemented!("CELP config"); },
            M4AType::HVXC => { unimplemented!("HVXC config"); },
            M4AType::TTSI => { unimplemented!("TTS config"); },
            M4AType::MainSynth | M4AType::WavetableSynth | M4AType::GeneralMIDI | M4AType::Algorithmic => { unimplemented!("structured audio config"); },
            M4AType::ER_CELP => { unimplemented!("ER CELP config"); },
            M4AType::ER_HVXC => { unimplemented!("ER HVXC config"); },
            M4AType::ER_HILN | M4AType::ER_Parametric => { unimplemented!("parametric config"); },
            M4AType::SSC => { unimplemented!("SSC config"); },
            M4AType::MPEGSurround => {
                                                        br.skip(1)?; // sacPayloadEmbedding
                    unimplemented!("MPEG Surround config");
                },
            M4AType::Layer1 | M4AType::Layer2 | M4AType::Layer3 => { unimplemented!("MPEG Layer 1/2/3 config"); },
            M4AType::DST => { unimplemented!("DST config"); },
            M4AType::ALS => {
                                                        br.skip(5)?; // fillBits
                    unimplemented!("ALS config");
                },
            M4AType::SLS | M4AType::SLSNonCore => { unimplemented!("SLS config"); },
            M4AType::ER_AAC_ELD => { unimplemented!("ELD config"); },
            M4AType::SMRSimple | M4AType::SMRMain => { unimplemented!("symbolic music config"); },
            _ => {},
        };
        match self.otype {
            M4AType::ER_AAC_LC | M4AType::ER_AAC_LTP | M4AType::ER_AAC_Scalable | M4AType::ER_TwinVQ |
            M4AType::ER_BSAC | M4AType::ER_AAC_LD | M4AType::ER_CELP | M4AType::ER_HVXC |
            M4AType::ER_HILN | M4AType::ER_Parametric | M4AType::ER_AAC_ELD => {
                    let ep_config                       = br.read(2)?;
                    if (ep_config == 2) || (ep_config == 3) {
                        unimplemented!("error protection config");
                    }
                    if ep_config == 3 {
                        let direct_mapping              = br.read_bool()?;
                        validate!(direct_mapping);
                    }
                },
            _ => {},
        };
        if self.sbr_ps_info.is_some() && (br.left() >= 16) {
            let sync                                    = br.read(11)?;
            if sync == 0x2B7 {
                let ext_otype = Self::read_object_type(&mut br)?;
                if ext_otype == M4AType::SBR {
                    self.sbr_present                    = br.read_bool()?;
                    if self.sbr_present {
                        let _ext_srate = Self::read_sampling_frequency(&mut br)?;
                        if br.left() >= 12 {
                            let sync                    = br.read(11)?;
                            if sync == 0x548 {
                                self.ps_present         = br.read_bool()?;
                            }
                        }
                    }
                }
                if ext_otype == M4AType::PS {
                    self.sbr_present                    = br.read_bool()?;
                    if self.sbr_present {
                        let _ext_srate = Self::read_sampling_frequency(&mut br)?;
                    }
                    let _ext_channels = br.read(4)?;
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for M4AInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MPEG 4 Audio {}, {} Hz, {} channels, {} samples per frame",
               self.otype, self.srate, self.channels, self.samples)
    }
}

const MAX_WINDOWS:  usize = 8;
const MAX_SFBS:     usize = 64;

#[derive(Clone,Copy)]
struct ICSInfo {
    window_sequence:        u8,
    prev_window_sequence:   u8,
    window_shape:           bool,
    prev_window_shape:      bool,
    scale_factor_grouping:  [bool; MAX_WINDOWS],
    group_start:            [usize; MAX_WINDOWS],
    window_groups:          usize,
    num_windows:            usize,
    max_sfb:                usize,
    predictor_data:         Option<LTPData>,
    long_win:               bool,
}

const ONLY_LONG_SEQUENCE:   u8 = 0;
const LONG_START_SEQUENCE:  u8 = 1;
const EIGHT_SHORT_SEQUENCE: u8 = 2;
const LONG_STOP_SEQUENCE:   u8 = 3;

impl ICSInfo {
    fn new() -> Self {
        Self {
            window_sequence:        0,
            prev_window_sequence:   0,
            window_shape:           false,
            prev_window_shape:      false,
            scale_factor_grouping:  [false; MAX_WINDOWS],
            group_start:            [0; MAX_WINDOWS],
            num_windows:            0,
            window_groups:          0,
            max_sfb:                0,
            predictor_data:         None,
            long_win:               true,
        }
    }
    fn decode_ics_info(&mut self, br: &mut BitReader) -> DecoderResult<()> {
        self.prev_window_sequence = self.window_sequence;
        self.prev_window_shape    = self.window_shape;
        let ics_reserved_bit                            = br.read(1)?;
        validate!(ics_reserved_bit == 0);
        self.window_sequence                            = br.read(2)? as u8;
        match self.prev_window_sequence {
            ONLY_LONG_SEQUENCE | LONG_STOP_SEQUENCE => {
                    validate!((self.window_sequence == ONLY_LONG_SEQUENCE) ||
                              (self.window_sequence == LONG_START_SEQUENCE));
                },
            LONG_START_SEQUENCE | EIGHT_SHORT_SEQUENCE => {
                    validate!((self.window_sequence == EIGHT_SHORT_SEQUENCE) ||
                              (self.window_sequence == LONG_STOP_SEQUENCE));
                },
            _ => {},
        };
        self.window_shape                               = br.read_bool()?;
        self.window_groups = 1;
        if self.window_sequence == EIGHT_SHORT_SEQUENCE {
            self.long_win = false;
            self.num_windows = 8;
            self.max_sfb                                = br.read(4)? as usize;
            for i in 0..MAX_WINDOWS-1 {
                self.scale_factor_grouping[i]           = br.read_bool()?;
                if !self.scale_factor_grouping[i] {
                    self.group_start[self.window_groups] = i + 1;
                    self.window_groups += 1;
                }
            }
        } else {
            self.long_win = true;
            self.num_windows = 1;
            self.max_sfb                                = br.read(6)? as usize;
            self.predictor_data = LTPData::read(br)?;
        }
        Ok(())
    }
    fn get_group_start(&self, g: usize) -> usize {
        if g == 0 {
            0
        } else if g >= self.window_groups {
            if self.long_win { 1 } else { 8 }
        } else {
            self.group_start[g]
        }
    }
}

#[derive(Clone,Copy)]
struct LTPData {
}

impl LTPData {
    fn read(br: &mut BitReader) -> DecoderResult<Option<Self>> {
        let predictor_data_present                      = br.read_bool()?;
        if !predictor_data_present { return Ok(None); }
unimplemented!("predictor data");
/*
        if is_main {
            let predictor_reset                         = br.read_bool()?;
            if predictor_reset {
                let predictor_reset_group_number        = br.read(5)?;
            }
            for sfb in 0..max_sfb.min(PRED_SFB_MAX) {
                prediction_used[sfb]                    = br.read_bool()?;
            }
        } else {
            let ltp_data_present                        = br.read_bool()?;
            if ltp_data_present {
                //ltp data
            }
            if common_window {
                let ltp_data_present                    = br.read_bool()?;
                if ltp_data_present {
                    //ltp data
                }
            }
        }
        Ok(Some(Self { }))
*/
    }
}

#[derive(Clone,Copy)]
#[allow(dead_code)]
struct PulseData {
    number_pulse:       usize,
    pulse_start_sfb:    usize,
    pulse_offset:       [u8; 4],
    pulse_amp:          [u8; 4],
}

impl PulseData {
    fn read(br: &mut BitReader) -> DecoderResult<Option<Self>> {
        let pulse_data_present                          = br.read_bool()?;
        if !pulse_data_present { return Ok(None); }

        let number_pulse                                = (br.read(2)? as usize) + 1;
        let pulse_start_sfb                             = br.read(6)? as usize;
        let mut pulse_offset: [u8; 4] = [0; 4];
        let mut pulse_amp: [u8; 4] = [0; 4];
        for i in 0..number_pulse {
            pulse_offset[i]                             = br.read(5)? as u8;
            pulse_amp[i]                                = br.read(4)? as u8;
        }
        Ok(Some(Self{ number_pulse, pulse_start_sfb, pulse_offset, pulse_amp }))
    }
}

const TNS_MAX_ORDER: usize = 20;
const TNS_MAX_LONG_BANDS: [usize; 12] = [ 31, 31, 34, 40, 42, 51, 46, 46, 42, 42, 42, 39 ];
const TNS_MAX_SHORT_BANDS: [usize; 12] = [ 9, 9, 10, 14, 14, 14, 14, 14, 14, 14, 14, 14 ];

#[derive(Clone,Copy)]
struct TNSCoeffs {
    length:     usize,
    order:      usize,
    direction:  bool,
    compress:   bool,
    coef:       [f32; TNS_MAX_ORDER + 1],
}

impl TNSCoeffs {
    fn new() -> Self {
        Self {
            length: 0, order: 0, direction: false, compress: false, coef: [0.0; TNS_MAX_ORDER + 1],
        }
    }
    fn read(&mut self, br: &mut BitReader, long_win: bool, coef_res: bool, max_order: usize) -> DecoderResult<()> {
        self.length                                     = br.read(if long_win { 6 } else { 4 })? as usize;
        self.order                                      = br.read(if long_win { 5 } else { 3 })? as usize;
        validate!(self.order <= max_order);
        if self.order > 0 {
            self.direction                              = br.read_bool()?;
            self.compress                               = br.read_bool()?;
            let mut coef_bits = 3;
            if coef_res      { coef_bits += 1; }
            if self.compress { coef_bits -= 1; }
            let sign_mask = 1 << (coef_bits - 1);
            let neg_mask  = !(sign_mask * 2 - 1);

            let fac_base = if coef_res { 1 << 3 } else { 1 << 2 } as f32;
            let iqfac   = (fac_base - 0.5) / (consts::PI / 2.0);
            let iqfac_m = (fac_base + 0.5) / (consts::PI / 2.0);
            let mut tmp: [f32; TNS_MAX_ORDER] = [0.0; TNS_MAX_ORDER];
            for el in tmp.iter_mut().take(self.order) {
                let val                                 = br.read(coef_bits)? as i8;
                let c = f32::from(if (val & sign_mask) != 0 { val | neg_mask } else { val });
                *el = (if c >= 0.0 { c / iqfac } else { c / iqfac_m }).sin();
            }
            // convert to LPC coefficients
            let mut b: [f32; TNS_MAX_ORDER + 1] = [0.0; TNS_MAX_ORDER + 1];
            for m in 1..=self.order {
                for i in 1..m {
                    b[i] = self.coef[i - 1] + tmp[m - 1] * self.coef[m - i - 1];
                }
                for i in 1..m {
                    self.coef[i - 1] = b[i];
                }
                self.coef[m - 1] = tmp[m - 1];
            }
        }
        Ok(())
    }
}

#[derive(Clone,Copy)]
#[allow(dead_code)]
struct TNSData {
    n_filt:     [usize; MAX_WINDOWS],
    coef_res:   [bool; MAX_WINDOWS],
    coeffs:     [[TNSCoeffs; 4]; MAX_WINDOWS],
}

impl TNSData {
    fn read(br: &mut BitReader, long_win: bool, num_windows: usize, max_order: usize) -> DecoderResult<Option<Self>> {
        let tns_data_present                            = br.read_bool()?;
        if !tns_data_present { return Ok(None); }
        let mut n_filt: [usize; MAX_WINDOWS] = [0; MAX_WINDOWS];
        let mut coef_res: [bool; MAX_WINDOWS] = [false; MAX_WINDOWS];
        let mut coeffs: [[TNSCoeffs; 4]; MAX_WINDOWS] = [[TNSCoeffs::new(); 4]; MAX_WINDOWS];
        for w in 0..num_windows {
            n_filt[w]                                   = br.read(if long_win { 2 } else { 1 })? as usize;
            if n_filt[w] != 0 {
                coef_res[w]                             = br.read_bool()?;
            }
            for filt in 0..n_filt[w] {
                coeffs[w][filt].read(br, long_win, coef_res[w], max_order)?;
            }
        }
        Ok(Some(Self { n_filt, coef_res, coeffs }))
    }
}

#[derive(Clone,Copy)]
#[allow(dead_code)]
struct GainControlData {
    max_band:       u8,
}

impl GainControlData {
    fn read(br: &mut BitReader) -> DecoderResult<Option<Self>> {
        let gain_control_data_present                   = br.read_bool()?;
        if !gain_control_data_present { return Ok(None); }
unimplemented!("gain control data");
/*        self.max_band                                   = br.read(2)? as u8;
        if window_sequence == ONLY_LONG_SEQUENCE {
            for bd in 0..max_band
...
        }
        Ok(Some(Self { }))*/
    }
}

const ZERO_HCB:         u8 = 0;
const FIRST_PAIR_HCB:   u8 = 5;
const ESC_HCB:          u8 = 11;
const RESERVED_HCB:     u8 = 12;
const NOISE_HCB:        u8 = 13;
const INTENSITY_HCB2:   u8 = 14;
const INTENSITY_HCB:    u8 = 15;

struct Codebooks {
    scale_cb:       Codebook<i8>,
    spec_cb:        [Codebook<u16>; 11],
}

fn scale_map(idx: usize) -> i8 { (idx as i8) - 60 }
fn cb_map(idx: usize) -> u16 { idx as u16 }

impl Codebooks {
    fn new() -> Self {
        let mut coderead = TableCodebookDescReader::new(AAC_SCF_CODEBOOK_CODES, AAC_SCF_CODEBOOK_BITS, scale_map);
        let scale_cb = Codebook::new(&mut coderead, CodebookMode::MSB).unwrap();
        let mut spec_cb: [Codebook<u16>; 11];
        unsafe {
            spec_cb = mem::uninitialized();
            for i in 0..AAC_SPEC_CODES.len() {
                let mut coderead = TableCodebookDescReader::new(AAC_SPEC_CODES[i], AAC_SPEC_BITS[i], cb_map);
                ptr::write(&mut spec_cb[i], Codebook::new(&mut coderead, CodebookMode::MSB).unwrap());
            }
        }
        Self { scale_cb, spec_cb }
    }
}

#[derive(Clone)]
struct ICS {
    global_gain:    u8,
    info:           ICSInfo,
    pulse_data:     Option<PulseData>,
    tns_data:       Option<TNSData>,
    gain_control:   Option<GainControlData>,
    sect_cb:        [[u8; MAX_SFBS]; MAX_WINDOWS],
    sect_len:       [[usize; MAX_SFBS]; MAX_WINDOWS],
    sfb_cb:         [[u8; MAX_SFBS]; MAX_WINDOWS],
    num_sec:        [usize; MAX_WINDOWS],
    scales:         [[u8; MAX_SFBS]; MAX_WINDOWS],
    sbinfo:         GASubbandInfo,
    coeffs:         [f32; 1024],
    delay:          [f32; 1024],
}

const INTENSITY_SCALE_MIN:  i16 = -155;
const NOISE_SCALE_MIN:      i16 = -100;
impl ICS {
    fn new(sbinfo: GASubbandInfo) -> Self {
        Self {
            global_gain:    0,
            info:           ICSInfo::new(),
            pulse_data:     None,
            tns_data:       None,
            gain_control:   None,
            sect_cb:        [[0; MAX_SFBS]; MAX_WINDOWS],
            sect_len:       [[0; MAX_SFBS]; MAX_WINDOWS],
            sfb_cb:         [[0; MAX_SFBS]; MAX_WINDOWS],
            scales:         [[0; MAX_SFBS]; MAX_WINDOWS],
            num_sec:        [0; MAX_WINDOWS],
            sbinfo,
            coeffs:         [0.0; 1024],
            delay:          [0.0; 1024],
        }
    }
    fn decode_section_data(&mut self, br: &mut BitReader, may_have_intensity: bool) -> DecoderResult<()> {
        let sect_bits = if self.info.long_win { 5 } else { 3 };
        let sect_esc_val = (1 << sect_bits) - 1;

        for g in 0..self.info.window_groups {
            let mut k = 0;
            let mut l = 0;
            while k < self.info.max_sfb {
                self.sect_cb[g][l]                      = br.read(4)? as u8;
                self.sect_len[g][l] = 0;
                validate!(self.sect_cb[g][l] != RESERVED_HCB);
                if ((self.sect_cb[g][l] == INTENSITY_HCB) || (self.sect_cb[g][l] == INTENSITY_HCB2)) && !may_have_intensity {
                    return Err(DecoderError::InvalidData);
                }
                loop {
                    let sect_len_incr                   = br.read(sect_bits)? as usize;
                    self.sect_len[g][l] += sect_len_incr;
                    if sect_len_incr < sect_esc_val { break; }
                }
                validate!(k + self.sect_len[g][l] <= self.info.max_sfb);
                for _ in 0..self.sect_len[g][l] {
                    self.sfb_cb[g][k] = self.sect_cb[g][l];
                    k += 1;
                }
                l += 1;
            }
            self.num_sec[g] = l;
        }
        Ok(())
    }
    fn is_intensity(&self, g: usize, sfb: usize) -> bool {
        (self.sfb_cb[g][sfb] == INTENSITY_HCB) || (self.sfb_cb[g][sfb] == INTENSITY_HCB2)
    }
    fn get_intensity_dir(&self, g: usize, sfb: usize) -> bool {
        self.sfb_cb[g][sfb] == INTENSITY_HCB
    }
    fn decode_scale_factor_data(&mut self, br: &mut BitReader, codebooks: &Codebooks) -> DecoderResult<()> {
        let mut noise_pcm_flag = true;
        let mut scf_normal = i16::from(self.global_gain);
        let mut scf_intensity = 0i16;
        let mut scf_noise  = 0i16;
        for g in 0..self.info.window_groups {
            for sfb in 0..self.info.max_sfb {
                if self.sfb_cb[g][sfb] != ZERO_HCB {
                    if self.is_intensity(g, sfb) {
                        let diff                        = i16::from(br.read_cb(&codebooks.scale_cb)?);
                        scf_intensity += diff;
                        validate!((scf_intensity >= INTENSITY_SCALE_MIN) && (scf_intensity < INTENSITY_SCALE_MIN + 256));
                        self.scales[g][sfb] = (scf_intensity - INTENSITY_SCALE_MIN) as u8;
                    } else if self.sfb_cb[g][sfb] == NOISE_HCB {
                        if noise_pcm_flag {
                            noise_pcm_flag = false;
                            scf_noise                   = (br.read(9)? as i16) - 256 + i16::from(self.global_gain) - 90;
                        } else {
                            scf_noise                  += i16::from(br.read_cb(&codebooks.scale_cb)?);
                        }
                        validate!((scf_noise >= NOISE_SCALE_MIN) && (scf_noise < NOISE_SCALE_MIN + 256));
                        self.scales[g][sfb] = (scf_noise - NOISE_SCALE_MIN) as u8;
                    } else {
                        scf_normal                     += i16::from(br.read_cb(&codebooks.scale_cb)?);
                        validate!((scf_normal >= 0) && (scf_normal < 255));
                        self.scales[g][sfb] = scf_normal as u8;
                    }
                }
            }
        }
        Ok(())
    }
    fn get_band_start(&self, swb: usize) -> usize {
        if self.info.long_win {
            self.sbinfo.long_bands[swb]
        } else {
            self.sbinfo.short_bands[swb]
        }
    }
    fn get_num_bands(&self) -> usize {
        if self.info.long_win {
            self.sbinfo.long_bands.len() - 1
        } else {
            self.sbinfo.short_bands.len() - 1
        }
    }
    fn decode_spectrum(&mut self, br: &mut BitReader, codebooks: &Codebooks) -> DecoderResult<()> {
        self.coeffs = [0.0; 1024];
        for g in 0..self.info.window_groups {
            let cur_w   = self.info.get_group_start(g);
            let next_w  = self.info.get_group_start(g + 1);
            for sfb in 0..self.info.max_sfb {
                let start = self.get_band_start(sfb);
                let end   = self.get_band_start(sfb + 1);
                let cb_idx = self.sfb_cb[g][sfb];
                for w in cur_w..next_w {
                    let dst = &mut self.coeffs[start + w*128..end + w*128];
                    match cb_idx {
                        ZERO_HCB => { /* zeroes */ },
                        NOISE_HCB => { /* noise */ },
                        INTENSITY_HCB | INTENSITY_HCB2 => { /* intensity */ },
                        _ => {
                                let unsigned = AAC_UNSIGNED_CODEBOOK[(cb_idx - 1) as usize];
                                let scale = get_scale(self.scales[g][sfb]);
                                let cb = &codebooks.spec_cb[(cb_idx - 1) as usize];
                                if cb_idx < FIRST_PAIR_HCB {
                                    decode_quads(br, cb, unsigned, scale, dst)?;
                                } else {
                                    decode_pairs(br, cb, unsigned, cb_idx == ESC_HCB,
                                                 AAC_CODEBOOK_MODULO[(cb_idx - FIRST_PAIR_HCB) as usize], scale, dst)?;
                                }
                            },
                    };
                }
            }
        }
        Ok(())
    }
    fn place_pulses(&mut self) {
        if let Some(ref pdata) = self.pulse_data {
            if pdata.pulse_start_sfb >= self.sbinfo.long_bands.len() - 1 { return; }
            let mut k = self.get_band_start(pdata.pulse_start_sfb);
            let mut band = pdata.pulse_start_sfb;
            for pno in 0..pdata.number_pulse {
                k += pdata.pulse_offset[pno] as usize;
                if k >= 1024 { return; }
                while self.get_band_start(band + 1) <= k { band += 1; }
                let scale = get_scale(self.scales[0][band]);
                let mut base = self.coeffs[k];
                if base != 0.0 {
                    base = requant(self.coeffs[k], scale);
                }
                if base > 0.0 {
                    base += f32::from(pdata.pulse_amp[pno]);
                } else {
                    base -= f32::from(pdata.pulse_amp[pno]);
                }
                self.coeffs[k] = iquant(base) * scale;
            }
        }
    }
    fn decode_ics(&mut self, br: &mut BitReader, codebooks: &Codebooks, m4atype: M4AType, common_window: bool, may_have_intensity: bool) -> DecoderResult<()> {
        self.global_gain                                = br.read(8)? as u8;
        if !common_window {
            self.info.decode_ics_info(br)?;
        }
        self.decode_section_data(br, may_have_intensity)?;
        self.decode_scale_factor_data(br, codebooks)?;
        self.pulse_data = PulseData::read(br)?;
        validate!(self.pulse_data.is_none() || self.info.long_win);
        let tns_max_order;
        if !self.info.long_win {
            tns_max_order = 7;
        } else if m4atype == M4AType::LC {
            tns_max_order = 12;
        } else {
            tns_max_order = TNS_MAX_ORDER;
        }
        self.tns_data = TNSData::read(br, self.info.long_win, self.info.num_windows, tns_max_order)?;
        if m4atype == M4AType::SSR {
            self.gain_control = GainControlData::read(br)?;
        } else {
            let gain_control_data_present               = br.read_bool()?;
            validate!(!gain_control_data_present);
        }
        self.decode_spectrum(br, codebooks)?;
        Ok(())
    }
    fn synth_channel(&mut self, dsp: &mut DSP, dst: &mut [f32], srate_idx: usize) {
        self.place_pulses();
        if let Some(ref tns_data) = self.tns_data {
            let tns_max_bands = (if self.info.long_win {
                    TNS_MAX_LONG_BANDS[srate_idx]
                } else {
                    TNS_MAX_SHORT_BANDS[srate_idx]
                }).min(self.info.max_sfb);
            for w in 0..self.info.num_windows {
                let mut bottom = self.get_num_bands();
                for f in 0..tns_data.n_filt[w] {
                    let top = bottom;
                    bottom = if top >= tns_data.coeffs[w][f].length { top - tns_data.coeffs[w][f].length } else { 0 };
                    let order = tns_data.coeffs[w][f].order;
                    if order == 0 { continue; }
                    let start = w * 128 + self.get_band_start(tns_max_bands.min(bottom));
                    let end   = w * 128 + self.get_band_start(tns_max_bands.min(top));
                    let lpc = &tns_data.coeffs[w][f].coef;
                    let mut state = [0.0f32; 64];
                    let mut sidx = 32;
                    if !tns_data.coeffs[w][f].direction {
                        for m in start..end {
                            for i in 0..order {
                                self.coeffs[m] -= state[(sidx + i) & 63] * lpc[i];
                            }
                            sidx = (sidx + 63) & 63;
                            state[sidx] = self.coeffs[m];
                        }
                    } else {
                        for m in (start..end).rev() {
                            for i in 0..order {
                                self.coeffs[m] -= state[(sidx + i) & 63] * lpc[i];
                            }
                            sidx = (sidx + 63) & 63;
                            state[sidx] = self.coeffs[m];
                        }
                    }
                }
            }
        }
        dsp.synth(&self.coeffs, &mut self.delay, self.info.window_sequence, self.info.window_shape, self.info.prev_window_shape, dst);
    }
}

fn get_scale(scale: u8) -> f32 {
    2.0f32.powf(0.25 * (f32::from(scale) - 100.0 - 56.0))
}
fn iquant(val: f32) -> f32 {
    if val < 0.0 {
        -((-val).powf(4.0 / 3.0))
    } else {
        val.powf(4.0 / 3.0)
    }
}
fn requant(val: f32, scale: f32) -> f32 {
    if scale == 0.0 { return 0.0; }
    let bval = val / scale;
    if bval >= 0.0 {
        val.powf(3.0 / 4.0)
    } else {
        -((-val).powf(3.0 / 4.0))
    }
}
fn decode_quads(br: &mut BitReader, cb: &Codebook<u16>, unsigned: bool, scale: f32, dst: &mut [f32]) -> DecoderResult<()> {
    for out in dst.chunks_mut(4) {
        let cw                                          = br.read_cb(cb)? as usize;
        if unsigned {
            for i in 0..4 {
                let val = AAC_QUADS[cw][i];
                if val != 0 {
                    if br.read_bool()? {
                        out[i] = iquant(-f32::from(val)) * scale;
                    } else {
                        out[i] = iquant( f32::from(val)) * scale;
                    }
                }
            }
        } else {
            for i in 0..4 {
                out[i] = iquant(f32::from(AAC_QUADS[cw][i] - 1)) * scale;
            }
        }
    }
    Ok(())
}
fn decode_pairs(br: &mut BitReader, cb: &Codebook<u16>, unsigned: bool, escape: bool, modulo: u16, scale: f32, dst: &mut [f32]) -> DecoderResult<()> {
    for out in dst.chunks_mut(2) {
        let cw                                          = br.read_cb(cb)?;
        let mut x = (cw / modulo) as i16;
        let mut y = (cw % modulo) as i16;
        if unsigned {
            if x != 0 && br.read_bool()? {
                x = -x;
            }
            if y != 0 && br.read_bool()? {
                y = -y;
            }
        } else {
            x -= (modulo >> 1) as i16;
            y -= (modulo >> 1) as i16;
        }
        if escape {
            if (x == 16) || (x == -16) {
                x += read_escape(br, x > 0)?;
            }
            if (y == 16) || (y == -16) {
                y += read_escape(br, y > 0)?;
            }
        }
        out[0] = iquant(f32::from(x)) * scale;
        out[1] = iquant(f32::from(y)) * scale;
    }
    Ok(())
}
fn read_escape(br: &mut BitReader, sign: bool) -> DecoderResult<i16> {
    let prefix                                          = br.read_code(UintCodeType::UnaryOnes)? as u8;
    validate!(prefix < 9);
    let bits                                            = br.read(prefix + 4)? as i16;
    if sign {
        Ok(bits)
    } else {
        Ok(-bits)
    }
}

#[derive(Clone)]
struct ChannelPair {
    pair:               bool,
    channel:            usize,
    common_window:      bool,
    ms_mask_present:    u8,
    ms_used:            [[bool; MAX_SFBS]; MAX_WINDOWS],
    ics:                [ICS; 2],
}

impl ChannelPair {
    fn new(pair: bool, channel: usize, sbinfo: GASubbandInfo) -> Self {
        Self {
            pair, channel,
            common_window:      false,
            ms_mask_present:    0,
            ms_used:            [[false; MAX_SFBS]; MAX_WINDOWS],
            ics:                [ICS::new(sbinfo), ICS::new(sbinfo)],
        }
    }
    fn decode_ga_sce(&mut self, br: &mut BitReader, codebooks: &Codebooks, m4atype: M4AType) -> DecoderResult<()> {
        self.ics[0].decode_ics(br, codebooks, m4atype, false, false)?;
        Ok(())
    }
    fn decode_ga_cpe(&mut self, br: &mut BitReader, codebooks: &Codebooks, m4atype: M4AType) -> DecoderResult<()> {
        let common_window                               = br.read_bool()?;
        self.common_window = common_window;
        if common_window {
            self.ics[0].info.decode_ics_info(br)?;
            self.ms_mask_present                        = br.read(2)? as u8;
            validate!(self.ms_mask_present != 3);
            if self.ms_mask_present == 1 {
                for g in 0..self.ics[0].info.window_groups {
                    for sfb in 0..self.ics[0].info.max_sfb {
                        self.ms_used[g][sfb]            = br.read_bool()?;
                    }
                }
            }
            self.ics[1].info = self.ics[0].info;
        }
        self.ics[0].decode_ics(br, codebooks, m4atype, common_window, true)?;
        self.ics[1].decode_ics(br, codebooks, m4atype, common_window, false)?;
        if common_window && self.ms_mask_present != 0 {
            let mut g = 0;
            for w in 0..self.ics[0].info.num_windows {
                if w > 0 && self.ics[0].info.scale_factor_grouping[w - 1] {
                    g += 1;
                }
                for sfb in 0..self.ics[0].info.max_sfb {
                    let start = w * 128 + self.ics[0].get_band_start(sfb);
                    let end   = w * 128 + self.ics[0].get_band_start(sfb + 1);
                    if self.ics[0].is_intensity(g, sfb) {
                        let invert = (self.ms_mask_present == 1) && self.ms_used[g][sfb];
                        let dir = self.ics[0].get_intensity_dir(g, sfb) ^ invert;
                        let scale = 0.5f32.powf(0.25 * (f32::from(self.ics[0].scales[g][sfb]) + f32::from(INTENSITY_SCALE_MIN)));
                        if !dir {
                            for i in start..end {
                                self.ics[1].coeffs[i] = scale * self.ics[0].coeffs[i];
                            }
                        } else {
                            for i in start..end {
                                self.ics[1].coeffs[i] = -scale * self.ics[0].coeffs[i];
                            }
                        }
                    } else if (self.ms_mask_present == 2) || self.ms_used[g][sfb] {
                        for i in start..end {
                            let tmp = self.ics[0].coeffs[i] - self.ics[1].coeffs[i];
                            self.ics[0].coeffs[i] += self.ics[1].coeffs[i];
                            self.ics[1].coeffs[i] = tmp;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn synth_audio(&mut self, dsp: &mut DSP, abuf: &mut NABufferType, srate_idx: usize) {
        let mut adata = abuf.get_abuf_f32().unwrap();
        let output = adata.get_data_mut().unwrap();
        let off0 = abuf.get_offset(self.channel);
        let off1 = abuf.get_offset(self.channel + 1);
        self.ics[0].synth_channel(dsp, &mut output[off0..], srate_idx);
        if self.pair {
            self.ics[1].synth_channel(dsp, &mut output[off1..], srate_idx);
        }
    }
}

struct DSP {
    kbd_long_win:   [f32; 1024],
    kbd_short_win:  [f32; 128],
    sine_long_win:  [f32; 1024],
    sine_short_win: [f32; 128],
    imdct_long:     IMDCT,
    imdct_short:    IMDCT,
    tmp:            [f32; 2048],
    ew_buf:         [f32; 1152],
}

const SHORT_WIN_POINT0: usize = 512 - 64;
const SHORT_WIN_POINT1: usize = 512 + 64;

impl DSP {
    fn new() -> Self {
        let mut kbd_long_win: [f32; 1024] = [0.0; 1024];
        let mut kbd_short_win: [f32; 128] = [0.0; 128];
        generate_window(WindowType::KaiserBessel(4.0), 1.0, 1024, true, &mut kbd_long_win);
        generate_window(WindowType::KaiserBessel(6.0), 1.0,  128, true, &mut kbd_short_win);
        let mut sine_long_win: [f32; 1024] = [0.0; 1024];
        let mut sine_short_win: [f32; 128] = [0.0; 128];
        generate_window(WindowType::Sine, 1.0, 1024, true, &mut sine_long_win);
        generate_window(WindowType::Sine, 1.0,  128, true, &mut sine_short_win);
        Self {
            kbd_long_win, kbd_short_win,
            sine_long_win, sine_short_win,
            imdct_long: IMDCT::new(1024 * 2, true),
            imdct_short: IMDCT::new(128 * 2, true),
            tmp: [0.0; 2048], ew_buf: [0.0; 1152],
        }
    }
    #[allow(clippy::cyclomatic_complexity)]
    fn synth(&mut self, coeffs: &[f32; 1024], delay: &mut [f32; 1024], seq: u8, window_shape: bool, prev_window_shape: bool, dst: &mut [f32]) {
        let long_win  = if window_shape { &self.kbd_long_win  } else { &self.sine_long_win };
        let short_win = if window_shape { &self.kbd_short_win } else { &self.sine_short_win };
        let left_long_win  = if prev_window_shape { &self.kbd_long_win  } else { &self.sine_long_win };
        let left_short_win = if prev_window_shape { &self.kbd_short_win } else { &self.sine_short_win };
        if seq != EIGHT_SHORT_SEQUENCE {
            self.imdct_long.imdct(coeffs, &mut self.tmp);
        } else {
            for (ain, aout) in coeffs.chunks(128).zip(self.tmp.chunks_mut(256)) {
                self.imdct_short.imdct(ain, aout);
            }
            self.ew_buf = [0.0; 1152];
            for (w, src) in self.tmp.chunks(256).enumerate() {
                if w > 0 {
                    for i in 0..128 {
                        self.ew_buf[w * 128 + i] += src[i] * short_win[i];
                    }
                } else { // to be left-windowed
                    for i in 0..128 {
                        self.ew_buf[i] = src[i];
                    }
                }
                for i in 0..128 {
                    self.ew_buf[w * 128 + i + 128] += src[i + 128] * short_win[127 - i];
                }
            }
        }
        if seq == ONLY_LONG_SEQUENCE { // should be the most common case
            for i in 0..1024 {
                dst[i] = delay[i] + self.tmp[i] * left_long_win[i];
                delay[i] = self.tmp[i + 1024] * long_win[1023 - i];
            }
            return;
        }
        // output new data
        match seq {
            ONLY_LONG_SEQUENCE | LONG_START_SEQUENCE => {
                    for i in 0..1024 {
                        dst[i] = self.tmp[i] * left_long_win[i] + delay[i];
                    }
                },
            EIGHT_SHORT_SEQUENCE => {
                    for i in 0..SHORT_WIN_POINT0 {
                        dst[i] = delay[i];
                    }
                    for i in SHORT_WIN_POINT0..SHORT_WIN_POINT1 {
                        let j = i - SHORT_WIN_POINT0;
                        dst[i] = delay[i] + self.ew_buf[j] * left_short_win[j];
                    }
                    for i in SHORT_WIN_POINT1..1024 {
                        let j = i - SHORT_WIN_POINT0;
                        dst[i] = self.ew_buf[j];
                    }
                },
            LONG_STOP_SEQUENCE => {
                    for i in 0..SHORT_WIN_POINT0 {
                        dst[i] = delay[i];
                    }
                    for i in SHORT_WIN_POINT0..SHORT_WIN_POINT1 {
                        dst[i] = delay[i] + self.tmp[i] * left_short_win[i - SHORT_WIN_POINT0];
                    }
                    for i in SHORT_WIN_POINT1..1024 {
                        dst[i] = self.tmp[i];
                    }
                },
            _ => unreachable!(""),
        };
        // save delay
        match seq {
            ONLY_LONG_SEQUENCE | LONG_STOP_SEQUENCE => {
                    for i in 0..1024 {
                        delay[i] = self.tmp[i + 1024] * long_win[1023 - i];
                    }
                },
            EIGHT_SHORT_SEQUENCE => {
                    for i in 0..SHORT_WIN_POINT1 { // last part is already windowed
                        delay[i] = self.ew_buf[i + 512+64];
                    }
                    for i in SHORT_WIN_POINT1..1024 {
                        delay[i] = 0.0;
                    }
                },
            LONG_START_SEQUENCE   => {
                    for i in 0..SHORT_WIN_POINT0 {
                        delay[i] = self.tmp[i + 1024];
                    }
                    for i in SHORT_WIN_POINT0..SHORT_WIN_POINT1 {
                        delay[i] = self.tmp[i + 1024] * short_win[127 - (i - SHORT_WIN_POINT0)];
                    }
                    for i in SHORT_WIN_POINT1..1024 {
                        delay[i] = 0.0;
                    }
                },
            _ => unreachable!(""),
        };
    }
}

struct AACDecoder {
    info:       NACodecInfoRef,
    chmap:      NAChannelMap,
    m4ainfo:    M4AInfo,
    pairs:      Vec<ChannelPair>,
    codebooks:  Codebooks,
    dsp:        DSP,
    sbinfo:     GASubbandInfo,
}

impl AACDecoder {
    fn new() -> Self {
        AACDecoder {
            info:       NACodecInfo::new_dummy(),
            chmap:      NAChannelMap::new(),
            m4ainfo:    M4AInfo::new(),
            pairs:      Vec::new(),
            codebooks:  Codebooks::new(),
            dsp:        DSP::new(),
            sbinfo:     AAC_SUBBAND_INFO[0],
        }
    }
    fn set_pair(&mut self, pair_no: usize, channel: usize, pair: bool) -> DecoderResult<()> {
        if self.pairs.len() <= pair_no {
            self.pairs.push(ChannelPair::new(pair, channel, self.sbinfo));
        } else {
            validate!(self.pairs[pair_no].channel == channel);
            validate!(self.pairs[pair_no].pair    == pair);
        }
        validate!(if pair { channel + 1 } else { channel } < self.m4ainfo.channels);
        Ok(())
    }
    fn decode_ga(&mut self, br: &mut BitReader, abuf: &mut NABufferType) -> DecoderResult<()> {
        let mut cur_pair = 0;
        let mut cur_ch   = 0;
        while br.left() > 3 {
            let id                                      = br.read(3)?;
            match id {
                0 => { // ID_SCE
                        let _tag                        = br.read(4)?;
                        self.set_pair(cur_pair, cur_ch, false)?;
                        self.pairs[cur_pair].decode_ga_sce(br, &self.codebooks, self.m4ainfo.otype)?;
                        cur_pair += 1;
                        cur_ch   += 1;
                    },
                1 => { // ID_CPE
                        let _tag                        = br.read(4)?;
                        self.set_pair(cur_pair, cur_ch, true)?;
                        self.pairs[cur_pair].decode_ga_cpe(br, &self.codebooks, self.m4ainfo.otype)?;
                        cur_pair += 1;
                        cur_ch   += 2;
                    },
                2 => { // ID_CCE
                        unimplemented!("coupling channel element");
                    },
                3 => { // ID_LFE
                        let _tag                        = br.read(4)?;
                        self.set_pair(cur_pair, cur_ch, false)?;
                        self.pairs[cur_pair].decode_ga_sce(br, &self.codebooks, self.m4ainfo.otype)?;
                        cur_pair += 1;
                        cur_ch   += 1;
                    },
                4 => { // ID_DSE
                        let _id                         = br.read(4)?;
                        let align                       = br.read_bool()?;
                        let mut count                   = br.read(8)? as u32;
                        if count == 255 { count        += br.read(8)? as u32; }
                        if align {                        br.align(); }
                                                          br.skip(count * 8)?; // no SBR payload or such
                    },
                5 => { // ID_PCE
                        unimplemented!("program config");
                    },
                6 => { // ID_FIL
                        let mut count                   = br.read(4)? as usize;
                        if count == 15 {
                            count                      += br.read(8)? as usize;
                            count -= 1;
                        }
                        for _ in 0..count {
                            // ext payload
                                                          br.skip(8)?;
                        }
                    },
                7 => { // ID_TERM
                        break;
                    },
                _ => { unreachable!(); },
            };
        }
        let srate_idx = GASubbandInfo::find_idx(self.m4ainfo.srate);
        for pair in 0..cur_pair {
            self.pairs[pair].synth_audio(&mut self.dsp, abuf, srate_idx);
        }
        Ok(())
    }
}

impl NADecoder for AACDecoder {
    fn init(&mut self, _supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()> {
        if let NACodecTypeInfo::Audio(_) = info.get_properties() {
            let edata = info.get_extradata().unwrap();
            validate!(edata.len() >= 2);

//print!("edata:"); for s in edata.iter() { print!(" {:02X}", *s);}println!("");
            self.m4ainfo.read(&edata)?;

            //println!("{}", self.m4ainfo);
            if (self.m4ainfo.otype != M4AType::LC) || (self.m4ainfo.channels > 2) || (self.m4ainfo.samples != 1024) {
                return Err(DecoderError::NotImplemented);
            }
            self.sbinfo = GASubbandInfo::find(self.m4ainfo.srate);

            let ainfo = NAAudioInfo::new(self.m4ainfo.srate, self.m4ainfo.channels as u8,
                                         SND_F32P_FORMAT, self.m4ainfo.samples);
            self.info = info.replace_info(NACodecTypeInfo::Audio(ainfo));

            if self.m4ainfo.channels >= DEFAULT_CHANNEL_MAP.len() {
                return Err(DecoderError::NotImplemented);
            }
            let chmap_str = DEFAULT_CHANNEL_MAP[self.m4ainfo.channels];
            if chmap_str.is_empty() { return Err(DecoderError::NotImplemented); }
            self.chmap = NAChannelMap::from_str(chmap_str).unwrap();

            Ok(())
        } else {
            Err(DecoderError::InvalidData)
        }
    }
    fn decode(&mut self, _supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef> {
        let info = pkt.get_stream().get_info();
        validate!(info.get_properties().is_audio());
        let pktbuf = pkt.get_buffer();

        let ainfo = self.info.get_properties().get_audio_info().unwrap();
        let mut abuf = alloc_audio_buffer(ainfo, self.m4ainfo.samples, self.chmap.clone())?;

        let mut br = BitReader::new(&pktbuf, BitReaderMode::BE);
        match self.m4ainfo.otype {
            M4AType::LC => {
                    self.decode_ga(&mut br, &mut abuf)?;
                },
            _ => { unimplemented!(""); }
        }

        let mut frm = NAFrame::new_from_pkt(pkt, self.info.replace_info(NACodecTypeInfo::Audio(ainfo)), abuf);
        frm.set_keyframe(true);
        Ok(frm.into_ref())
    }
    fn flush(&mut self) {
        for pair in self.pairs.iter_mut() {
            pair.ics[0].delay = [0.0; 1024];
            pair.ics[1].delay = [0.0; 1024];
        }
    }
}

impl NAOptionHandler for AACDecoder {
    fn get_supported_options(&self) -> &[NAOptionDefinition] { &[] }
    fn set_options(&mut self, _options: &[NAOption]) { }
    fn query_option_value(&self, _name: &str) -> Option<NAValue> { None }
}

pub fn get_decoder() -> Box<dyn NADecoder + Send> {
    Box::new(AACDecoder::new())
}

#[cfg(test)]
mod test {
    use nihav_core::codecs::RegisteredDecoders;
    use nihav_core::demuxers::RegisteredDemuxers;
    use nihav_codec_support::test::dec_video::test_decode_audio;
    use crate::generic_register_all_decoders;
    use nihav_realmedia::realmedia_register_all_demuxers;
    #[test]
    fn test_aac() {
        let mut dmx_reg = RegisteredDemuxers::new();
        realmedia_register_all_demuxers(&mut dmx_reg);
        let mut dec_reg = RegisteredDecoders::new();
        generic_register_all_decoders(&mut dec_reg);

//        let file = "assets/RV/rv40_weighted_mc.rmvb";
        let file = "assets/RV/rv40_weighted_mc_2.rmvb";
        test_decode_audio("realmedia", file, Some(12000), None/*Some("aac")*/, &dmx_reg, &dec_reg);
    }
}

const AAC_SCF_CODEBOOK_BITS: &[u8] = &[
    18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 18, 19, 18, 17, 17, 16, 17, 16, 16, 16, 16, 15, 15,
    14, 14, 14, 14, 14, 14, 13, 13, 12, 12, 12, 11, 12, 11, 10, 10,
    10,  9,  9,  8,  8,  8,  7,  6,  6,  5,  4,  3,  1,  4,  4,  5,
     6,  6,  7,  7,  8,  8,  9,  9, 10, 10, 10, 11, 11, 11, 11, 12,
    12, 13, 13, 13, 14, 14, 16, 15, 16, 15, 18, 19, 19, 19, 19, 19,
    19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 19, 19, 19, 19, 19, 19
];

const AAC_SCF_CODEBOOK_CODES: &[u32] = &[
    0x3FFE8, 0x3FFE6, 0x3FFE7, 0x3FFE5, 0x7FFF5, 0x7FFF1, 0x7FFED, 0x7FFF6,
    0x7FFEE, 0x7FFEF, 0x7FFF0, 0x7FFFC, 0x7FFFD, 0x7FFFF, 0x7FFFE, 0x7FFF7,
    0x7FFF8, 0x7FFFB, 0x7FFF9, 0x3FFE4, 0x7FFFA, 0x3FFE3, 0x1FFEF, 0x1FFF0,
    0x0FFF5, 0x1FFEE, 0x0FFF2, 0x0FFF3, 0x0FFF4, 0x0FFF1, 0x07FF6, 0x07FF7,
    0x03FF9, 0x03FF5, 0x03FF7, 0x03FF3, 0x03FF6, 0x03FF2, 0x01FF7, 0x01FF5,
    0x00FF9, 0x00FF7, 0x00FF6, 0x007F9, 0x00FF4, 0x007F8, 0x003F9, 0x003F7,
    0x003F5, 0x001F8, 0x001F7, 0x000FA, 0x000F8, 0x000F6, 0x00079, 0x0003A,
    0x00038, 0x0001A, 0x0000B, 0x00004, 0x00000, 0x0000A, 0x0000C, 0x0001B,
    0x00039, 0x0003B, 0x00078, 0x0007A, 0x000F7, 0x000F9, 0x001F6, 0x001F9,
    0x003F4, 0x003F6, 0x003F8, 0x007F5, 0x007F4, 0x007F6, 0x007F7, 0x00FF5,
    0x00FF8, 0x01FF4, 0x01FF6, 0x01FF8, 0x03FF8, 0x03FF4, 0x0FFF0, 0x07FF4,
    0x0FFF6, 0x07FF5, 0x3FFE2, 0x7FFD9, 0x7FFDA, 0x7FFDB, 0x7FFDC, 0x7FFDD,
    0x7FFDE, 0x7FFD8, 0x7FFD2, 0x7FFD3, 0x7FFD4, 0x7FFD5, 0x7FFD6, 0x7FFF2,
    0x7FFDF, 0x7FFE7, 0x7FFE8, 0x7FFE9, 0x7FFEA, 0x7FFEB, 0x7FFE6, 0x7FFE0,
    0x7FFE1, 0x7FFE2, 0x7FFE3, 0x7FFE4, 0x7FFE5, 0x7FFD7, 0x7FFEC, 0x7FFF4,
    0x7FFF3
];

const AAC_SPEC_CB1_BITS: &[u8] = &[
    11,  9, 11, 10,  7, 10, 11,  9, 11, 10,  7, 10,  7,  5,  7,  9,
     7, 10, 11,  9, 11,  9,  7,  9, 11,  9, 11,  9,  7,  9,  7,  5,
     7,  9,  7,  9,  7,  5,  7,  5,  1,  5,  7,  5,  7,  9,  7,  9,
     7,  5,  7,  9,  7,  9, 11,  9, 11,  9,  7,  9, 11,  9, 11, 10,
     7,  9,  7,  5,  7,  9,  7, 10, 11,  9, 11, 10,  7,  9, 11,  9,
    11
];
const AAC_SPEC_CB1_CODES: &[u16] = &[
    0x7f8, 0x1f1, 0x7fd, 0x3f5, 0x068, 0x3f0, 0x7f7, 0x1ec,
    0x7f5, 0x3f1, 0x072, 0x3f4, 0x074, 0x011, 0x076, 0x1eb,
    0x06c, 0x3f6, 0x7fc, 0x1e1, 0x7f1, 0x1f0, 0x061, 0x1f6,
    0x7f2, 0x1ea, 0x7fb, 0x1f2, 0x069, 0x1ed, 0x077, 0x017,
    0x06f, 0x1e6, 0x064, 0x1e5, 0x067, 0x015, 0x062, 0x012,
    0x000, 0x014, 0x065, 0x016, 0x06d, 0x1e9, 0x063, 0x1e4,
    0x06b, 0x013, 0x071, 0x1e3, 0x070, 0x1f3, 0x7fe, 0x1e7,
    0x7f3, 0x1ef, 0x060, 0x1ee, 0x7f0, 0x1e2, 0x7fa, 0x3f3,
    0x06a, 0x1e8, 0x075, 0x010, 0x073, 0x1f4, 0x06e, 0x3f7,
    0x7f6, 0x1e0, 0x7f9, 0x3f2, 0x066, 0x1f5, 0x7ff, 0x1f7,
    0x7f4
];
const AAC_SPEC_CB2_BITS: &[u8] = &[
    9, 7, 9, 8, 6, 8, 9, 8, 9, 8, 6, 7, 6, 5, 6, 7,
    6, 8, 9, 7, 8, 8, 6, 8, 9, 7, 9, 8, 6, 7, 6, 5,
    6, 7, 6, 8, 6, 5, 6, 5, 3, 5, 6, 5, 6, 8, 6, 7,
    6, 5, 6, 8, 6, 8, 9, 7, 9, 8, 6, 8, 8, 7, 9, 8,
    6, 7, 6, 4, 6, 8, 6, 7, 9, 7, 9, 7, 6, 8, 9, 7,
    9
];
const AAC_SPEC_CB2_CODES: &[u16] = &[
    0x1f3, 0x06f, 0x1fd, 0x0eb, 0x023, 0x0ea, 0x1f7, 0x0e8,
    0x1fa, 0x0f2, 0x02d, 0x070, 0x020, 0x006, 0x02b, 0x06e,
    0x028, 0x0e9, 0x1f9, 0x066, 0x0f8, 0x0e7, 0x01b, 0x0f1,
    0x1f4, 0x06b, 0x1f5, 0x0ec, 0x02a, 0x06c, 0x02c, 0x00a,
    0x027, 0x067, 0x01a, 0x0f5, 0x024, 0x008, 0x01f, 0x009,
    0x000, 0x007, 0x01d, 0x00b, 0x030, 0x0ef, 0x01c, 0x064,
    0x01e, 0x00c, 0x029, 0x0f3, 0x02f, 0x0f0, 0x1fc, 0x071,
    0x1f2, 0x0f4, 0x021, 0x0e6, 0x0f7, 0x068, 0x1f8, 0x0ee,
    0x022, 0x065, 0x031, 0x002, 0x026, 0x0ed, 0x025, 0x06a,
    0x1fb, 0x072, 0x1fe, 0x069, 0x02e, 0x0f6, 0x1ff, 0x06d,
    0x1f6
];
const AAC_SPEC_CB3_BITS: &[u8] = &[
     1,  4,  8,  4,  5,  8,  9,  9, 10,  4,  6,  9,  6,  6,  9,  9,
     9, 10,  9, 10, 13,  9,  9, 11, 11, 10, 12,  4,  6, 10,  6,  7,
    10, 10, 10, 12,  5,  7, 11,  6,  7, 10,  9,  9, 11,  9, 10, 13,
     8,  9, 12, 10, 11, 12,  8, 10, 15,  9, 11, 15, 13, 14, 16,  8,
    10, 14,  9, 10, 14, 12, 12, 15, 11, 12, 16, 10, 11, 15, 12, 12,
    15
];
const AAC_SPEC_CB3_CODES: &[u16] = &[
    0x0000, 0x0009, 0x00ef, 0x000b, 0x0019, 0x00f0, 0x01eb, 0x01e6,
    0x03f2, 0x000a, 0x0035, 0x01ef, 0x0034, 0x0037, 0x01e9, 0x01ed,
    0x01e7, 0x03f3, 0x01ee, 0x03ed, 0x1ffa, 0x01ec, 0x01f2, 0x07f9,
    0x07f8, 0x03f8, 0x0ff8, 0x0008, 0x0038, 0x03f6, 0x0036, 0x0075,
    0x03f1, 0x03eb, 0x03ec, 0x0ff4, 0x0018, 0x0076, 0x07f4, 0x0039,
    0x0074, 0x03ef, 0x01f3, 0x01f4, 0x07f6, 0x01e8, 0x03ea, 0x1ffc,
    0x00f2, 0x01f1, 0x0ffb, 0x03f5, 0x07f3, 0x0ffc, 0x00ee, 0x03f7,
    0x7ffe, 0x01f0, 0x07f5, 0x7ffd, 0x1ffb, 0x3ffa, 0xffff, 0x00f1,
    0x03f0, 0x3ffc, 0x01ea, 0x03ee, 0x3ffb, 0x0ff6, 0x0ffa, 0x7ffc,
    0x07f2, 0x0ff5, 0xfffe, 0x03f4, 0x07f7, 0x7ffb, 0x0ff7, 0x0ff9,
    0x7ffa
];
const AAC_SPEC_CB4_BITS: &[u8] = &[
     4,  5,  8,  5,  4,  8,  9,  8, 11,  5,  5,  8,  5,  4,  8,  8,
     7, 10,  9,  8, 11,  8,  8, 10, 11, 10, 11,  4,  5,  8,  4,  4,
     8,  8,  8, 10,  4,  4,  8,  4,  4,  7,  8,  7,  9,  8,  8, 10,
     7,  7,  9, 10,  9, 10,  8,  8, 11,  8,  7, 10, 11, 10, 12,  8,
     7, 10,  7,  7,  9, 10,  9, 11, 11, 10, 12, 10,  9, 11, 11, 10,
    11
];
const AAC_SPEC_CB4_CODES: &[u16] = &[
    0x007, 0x016, 0x0f6, 0x018, 0x008, 0x0ef, 0x1ef, 0x0f3,
    0x7f8, 0x019, 0x017, 0x0ed, 0x015, 0x001, 0x0e2, 0x0f0,
    0x070, 0x3f0, 0x1ee, 0x0f1, 0x7fa, 0x0ee, 0x0e4, 0x3f2,
    0x7f6, 0x3ef, 0x7fd, 0x005, 0x014, 0x0f2, 0x009, 0x004,
    0x0e5, 0x0f4, 0x0e8, 0x3f4, 0x006, 0x002, 0x0e7, 0x003,
    0x000, 0x06b, 0x0e3, 0x069, 0x1f3, 0x0eb, 0x0e6, 0x3f6,
    0x06e, 0x06a, 0x1f4, 0x3ec, 0x1f0, 0x3f9, 0x0f5, 0x0ec,
    0x7fb, 0x0ea, 0x06f, 0x3f7, 0x7f9, 0x3f3, 0xfff, 0x0e9,
    0x06d, 0x3f8, 0x06c, 0x068, 0x1f5, 0x3ee, 0x1f2, 0x7f4,
    0x7f7, 0x3f1, 0xffe, 0x3ed, 0x1f1, 0x7f5, 0x7fe, 0x3f5,
    0x7fc
];
const AAC_SPEC_CB5_BITS: &[u8] = &[
    13, 12, 11, 11, 10, 11, 11, 12, 13, 12, 11, 10,  9,  8,  9, 10,
    11, 12, 12, 10,  9,  8,  7,  8,  9, 10, 11, 11,  9,  8,  5,  4,
     5,  8,  9, 11, 10,  8,  7,  4,  1,  4,  7,  8, 11, 11,  9,  8,
     5,  4,  5,  8,  9, 11, 11, 10,  9,  8,  7,  8,  9, 10, 11, 12,
    11, 10,  9,  8,  9, 10, 11, 12, 13, 12, 12, 11, 10, 10, 11, 12,
    13
];
const AAC_SPEC_CB5_CODES: &[u16] = &[
    0x1fff, 0x0ff7, 0x07f4, 0x07e8, 0x03f1, 0x07ee, 0x07f9, 0x0ff8,
    0x1ffd, 0x0ffd, 0x07f1, 0x03e8, 0x01e8, 0x00f0, 0x01ec, 0x03ee,
    0x07f2, 0x0ffa, 0x0ff4, 0x03ef, 0x01f2, 0x00e8, 0x0070, 0x00ec,
    0x01f0, 0x03ea, 0x07f3, 0x07eb, 0x01eb, 0x00ea, 0x001a, 0x0008,
    0x0019, 0x00ee, 0x01ef, 0x07ed, 0x03f0, 0x00f2, 0x0073, 0x000b,
    0x0000, 0x000a, 0x0071, 0x00f3, 0x07e9, 0x07ef, 0x01ee, 0x00ef,
    0x0018, 0x0009, 0x001b, 0x00eb, 0x01e9, 0x07ec, 0x07f6, 0x03eb,
    0x01f3, 0x00ed, 0x0072, 0x00e9, 0x01f1, 0x03ed, 0x07f7, 0x0ff6,
    0x07f0, 0x03e9, 0x01ed, 0x00f1, 0x01ea, 0x03ec, 0x07f8, 0x0ff9,
    0x1ffc, 0x0ffc, 0x0ff5, 0x07ea, 0x03f3, 0x03f2, 0x07f5, 0x0ffb,
    0x1ffe
];
const AAC_SPEC_CB6_BITS: &[u8] = &[
    11, 10,  9,  9,  9,  9,  9, 10, 11, 10,  9,  8,  7,  7,  7,  8,
     9, 10,  9,  8,  6,  6,  6,  6,  6,  8,  9,  9,  7,  6,  4,  4,
     4,  6,  7,  9,  9,  7,  6,  4,  4,  4,  6,  7,  9,  9,  7,  6,
     4,  4,  4,  6,  7,  9,  9,  8,  6,  6,  6,  6,  6,  8,  9, 10,
     9,  8,  7,  7,  7,  7,  8, 10, 11, 10,  9,  9,  9,  9,  9, 10,
    11
];
const AAC_SPEC_CB6_CODES: &[u16] = &[
    0x7fe, 0x3fd, 0x1f1, 0x1eb, 0x1f4, 0x1ea, 0x1f0, 0x3fc,
    0x7fd, 0x3f6, 0x1e5, 0x0ea, 0x06c, 0x071, 0x068, 0x0f0,
    0x1e6, 0x3f7, 0x1f3, 0x0ef, 0x032, 0x027, 0x028, 0x026,
    0x031, 0x0eb, 0x1f7, 0x1e8, 0x06f, 0x02e, 0x008, 0x004,
    0x006, 0x029, 0x06b, 0x1ee, 0x1ef, 0x072, 0x02d, 0x002,
    0x000, 0x003, 0x02f, 0x073, 0x1fa, 0x1e7, 0x06e, 0x02b,
    0x007, 0x001, 0x005, 0x02c, 0x06d, 0x1ec, 0x1f9, 0x0ee,
    0x030, 0x024, 0x02a, 0x025, 0x033, 0x0ec, 0x1f2, 0x3f8,
    0x1e4, 0x0ed, 0x06a, 0x070, 0x069, 0x074, 0x0f1, 0x3fa,
    0x7ff, 0x3f9, 0x1f6, 0x1ed, 0x1f8, 0x1e9, 0x1f5, 0x3fb,
    0x7fc
];
const AAC_SPEC_CB7_BITS: &[u8] = &[
     1,  3,  6,  7,  8,  9, 10, 11,  3,  4,  6,  7,  8,  8,  9,  9,
     6,  6,  7,  8,  8,  9,  9, 10,  7,  7,  8,  8,  9,  9, 10, 10,
     8,  8,  9,  9, 10, 10, 10, 11,  9,  8,  9,  9, 10, 10, 11, 11,
    10,  9,  9, 10, 10, 11, 12, 12, 11, 10, 10, 10, 11, 11, 12, 12
];
const AAC_SPEC_CB7_CODES: &[u16] = &[
    0x000, 0x005, 0x037, 0x074, 0x0f2, 0x1eb, 0x3ed, 0x7f7,
    0x004, 0x00c, 0x035, 0x071, 0x0ec, 0x0ee, 0x1ee, 0x1f5,
    0x036, 0x034, 0x072, 0x0ea, 0x0f1, 0x1e9, 0x1f3, 0x3f5,
    0x073, 0x070, 0x0eb, 0x0f0, 0x1f1, 0x1f0, 0x3ec, 0x3fa,
    0x0f3, 0x0ed, 0x1e8, 0x1ef, 0x3ef, 0x3f1, 0x3f9, 0x7fb,
    0x1ed, 0x0ef, 0x1ea, 0x1f2, 0x3f3, 0x3f8, 0x7f9, 0x7fc,
    0x3ee, 0x1ec, 0x1f4, 0x3f4, 0x3f7, 0x7f8, 0xffd, 0xffe,
    0x7f6, 0x3f0, 0x3f2, 0x3f6, 0x7fa, 0x7fd, 0xffc, 0xfff
];
const AAC_SPEC_CB8_BITS: &[u8] = &[
     5,  4,  5,  6,  7,  8,  9, 10,  4,  3,  4,  5,  6,  7,  7,  8,
     5,  4,  4,  5,  6,  7,  7,  8,  6,  5,  5,  6,  6,  7,  8,  8,
     7,  6,  6,  6,  7,  7,  8,  9,  8,  7,  6,  7,  7,  8,  8, 10,
     9,  7,  7,  8,  8,  8,  9,  9, 10,  8,  8,  8,  9,  9,  9, 10
];
const AAC_SPEC_CB8_CODES: &[u16] = &[
    0x00e, 0x005, 0x010, 0x030, 0x06f, 0x0f1, 0x1fa, 0x3fe,
    0x003, 0x000, 0x004, 0x012, 0x02c, 0x06a, 0x075, 0x0f8,
    0x00f, 0x002, 0x006, 0x014, 0x02e, 0x069, 0x072, 0x0f5,
    0x02f, 0x011, 0x013, 0x02a, 0x032, 0x06c, 0x0ec, 0x0fa,
    0x071, 0x02b, 0x02d, 0x031, 0x06d, 0x070, 0x0f2, 0x1f9,
    0x0ef, 0x068, 0x033, 0x06b, 0x06e, 0x0ee, 0x0f9, 0x3fc,
    0x1f8, 0x074, 0x073, 0x0ed, 0x0f0, 0x0f6, 0x1f6, 0x1fd,
    0x3fd, 0x0f3, 0x0f4, 0x0f7, 0x1f7, 0x1fb, 0x1fc, 0x3ff
];
const AAC_SPEC_CB9_BITS: &[u8] = &[
     1,  3,  6,  8,  9, 10, 10, 11, 11, 12, 12, 13, 13,  3,  4,  6,
     7,  8,  8,  9, 10, 10, 10, 11, 12, 12,  6,  6,  7,  8,  8,  9,
    10, 10, 10, 11, 12, 12, 12,  8,  7,  8,  9,  9, 10, 10, 11, 11,
    11, 12, 12, 13,  9,  8,  9,  9, 10, 10, 11, 11, 11, 12, 12, 12,
    13, 10,  9,  9, 10, 11, 11, 11, 12, 11, 12, 12, 13, 13, 11,  9,
    10, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 11, 10, 10, 11, 11,
    12, 12, 13, 13, 13, 13, 13, 13, 11, 10, 10, 11, 11, 11, 12, 12,
    13, 13, 14, 13, 14, 11, 10, 11, 11, 12, 12, 12, 12, 13, 13, 14,
    14, 14, 12, 11, 11, 12, 12, 12, 13, 13, 13, 14, 14, 14, 15, 12,
    11, 12, 12, 12, 13, 13, 13, 13, 14, 14, 15, 15, 13, 12, 12, 12,
    13, 13, 13, 13, 14, 14, 14, 14, 15
];
const AAC_SPEC_CB9_CODES: &[u16] = &[
    0x0000, 0x0005, 0x0037, 0x00e7, 0x01de, 0x03ce, 0x03d9, 0x07c8,
    0x07cd, 0x0fc8, 0x0fdd, 0x1fe4, 0x1fec, 0x0004, 0x000c, 0x0035,
    0x0072, 0x00ea, 0x00ed, 0x01e2, 0x03d1, 0x03d3, 0x03e0, 0x07d8,
    0x0fcf, 0x0fd5, 0x0036, 0x0034, 0x0071, 0x00e8, 0x00ec, 0x01e1,
    0x03cf, 0x03dd, 0x03db, 0x07d0, 0x0fc7, 0x0fd4, 0x0fe4, 0x00e6,
    0x0070, 0x00e9, 0x01dd, 0x01e3, 0x03d2, 0x03dc, 0x07cc, 0x07ca,
    0x07de, 0x0fd8, 0x0fea, 0x1fdb, 0x01df, 0x00eb, 0x01dc, 0x01e6,
    0x03d5, 0x03de, 0x07cb, 0x07dd, 0x07dc, 0x0fcd, 0x0fe2, 0x0fe7,
    0x1fe1, 0x03d0, 0x01e0, 0x01e4, 0x03d6, 0x07c5, 0x07d1, 0x07db,
    0x0fd2, 0x07e0, 0x0fd9, 0x0feb, 0x1fe3, 0x1fe9, 0x07c4, 0x01e5,
    0x03d7, 0x07c6, 0x07cf, 0x07da, 0x0fcb, 0x0fda, 0x0fe3, 0x0fe9,
    0x1fe6, 0x1ff3, 0x1ff7, 0x07d3, 0x03d8, 0x03e1, 0x07d4, 0x07d9,
    0x0fd3, 0x0fde, 0x1fdd, 0x1fd9, 0x1fe2, 0x1fea, 0x1ff1, 0x1ff6,
    0x07d2, 0x03d4, 0x03da, 0x07c7, 0x07d7, 0x07e2, 0x0fce, 0x0fdb,
    0x1fd8, 0x1fee, 0x3ff0, 0x1ff4, 0x3ff2, 0x07e1, 0x03df, 0x07c9,
    0x07d6, 0x0fca, 0x0fd0, 0x0fe5, 0x0fe6, 0x1feb, 0x1fef, 0x3ff3,
    0x3ff4, 0x3ff5, 0x0fe0, 0x07ce, 0x07d5, 0x0fc6, 0x0fd1, 0x0fe1,
    0x1fe0, 0x1fe8, 0x1ff0, 0x3ff1, 0x3ff8, 0x3ff6, 0x7ffc, 0x0fe8,
    0x07df, 0x0fc9, 0x0fd7, 0x0fdc, 0x1fdc, 0x1fdf, 0x1fed, 0x1ff5,
    0x3ff9, 0x3ffb, 0x7ffd, 0x7ffe, 0x1fe7, 0x0fcc, 0x0fd6, 0x0fdf,
    0x1fde, 0x1fda, 0x1fe5, 0x1ff2, 0x3ffa, 0x3ff7, 0x3ffc, 0x3ffd,
    0x7fff
];
const AAC_SPEC_CB10_BITS: &[u8] = &[
     6,  5,  6,  6,  7,  8,  9, 10, 10, 10, 11, 11, 12,  5,  4,  4,
     5,  6,  7,  7,  8,  8,  9, 10, 10, 11,  6,  4,  5,  5,  6,  6,
     7,  8,  8,  9,  9, 10, 10,  6,  5,  5,  5,  6,  7,  7,  8,  8,
     9,  9, 10, 10,  7,  6,  6,  6,  6,  7,  7,  8,  8,  9,  9, 10,
    10,  8,  7,  6,  7,  7,  7,  8,  8,  8,  9, 10, 10, 11,  9,  7,
     7,  7,  7,  8,  8,  9,  9,  9, 10, 10, 11,  9,  8,  8,  8,  8,
     8,  9,  9,  9, 10, 10, 11, 11,  9,  8,  8,  8,  8,  8,  9,  9,
    10, 10, 10, 11, 11, 10,  9,  9,  9,  9,  9,  9, 10, 10, 10, 11,
    11, 12, 10,  9,  9,  9,  9, 10, 10, 10, 10, 11, 11, 11, 12, 11,
    10,  9, 10, 10, 10, 10, 10, 11, 11, 11, 11, 12, 11, 10, 10, 10,
    10, 10, 10, 11, 11, 12, 12, 12, 12
];
const AAC_SPEC_CB10_CODES: &[u16] = &[
    0x022, 0x008, 0x01d, 0x026, 0x05f, 0x0d3, 0x1cf, 0x3d0,
    0x3d7, 0x3ed, 0x7f0, 0x7f6, 0xffd, 0x007, 0x000, 0x001,
    0x009, 0x020, 0x054, 0x060, 0x0d5, 0x0dc, 0x1d4, 0x3cd,
    0x3de, 0x7e7, 0x01c, 0x002, 0x006, 0x00c, 0x01e, 0x028,
    0x05b, 0x0cd, 0x0d9, 0x1ce, 0x1dc, 0x3d9, 0x3f1, 0x025,
    0x00b, 0x00a, 0x00d, 0x024, 0x057, 0x061, 0x0cc, 0x0dd,
    0x1cc, 0x1de, 0x3d3, 0x3e7, 0x05d, 0x021, 0x01f, 0x023,
    0x027, 0x059, 0x064, 0x0d8, 0x0df, 0x1d2, 0x1e2, 0x3dd,
    0x3ee, 0x0d1, 0x055, 0x029, 0x056, 0x058, 0x062, 0x0ce,
    0x0e0, 0x0e2, 0x1da, 0x3d4, 0x3e3, 0x7eb, 0x1c9, 0x05e,
    0x05a, 0x05c, 0x063, 0x0ca, 0x0da, 0x1c7, 0x1ca, 0x1e0,
    0x3db, 0x3e8, 0x7ec, 0x1e3, 0x0d2, 0x0cb, 0x0d0, 0x0d7,
    0x0db, 0x1c6, 0x1d5, 0x1d8, 0x3ca, 0x3da, 0x7ea, 0x7f1,
    0x1e1, 0x0d4, 0x0cf, 0x0d6, 0x0de, 0x0e1, 0x1d0, 0x1d6,
    0x3d1, 0x3d5, 0x3f2, 0x7ee, 0x7fb, 0x3e9, 0x1cd, 0x1c8,
    0x1cb, 0x1d1, 0x1d7, 0x1df, 0x3cf, 0x3e0, 0x3ef, 0x7e6,
    0x7f8, 0xffa, 0x3eb, 0x1dd, 0x1d3, 0x1d9, 0x1db, 0x3d2,
    0x3cc, 0x3dc, 0x3ea, 0x7ed, 0x7f3, 0x7f9, 0xff9, 0x7f2,
    0x3ce, 0x1e4, 0x3cb, 0x3d8, 0x3d6, 0x3e2, 0x3e5, 0x7e8,
    0x7f4, 0x7f5, 0x7f7, 0xffb, 0x7fa, 0x3ec, 0x3df, 0x3e1,
    0x3e4, 0x3e6, 0x3f0, 0x7e9, 0x7ef, 0xff8, 0xffe, 0xffc,
    0xfff
];
const AAC_SPEC_CB11_BITS: &[u8] = &[
     4,  5,  6,  7,  8,  8,  9, 10, 10, 10, 11, 11, 12, 11, 12, 12,
    10,  5,  4,  5,  6,  7,  7,  8,  8,  9,  9,  9, 10, 10, 10, 10,
    11,  8,  6,  5,  5,  6,  7,  7,  8,  8,  8,  9,  9,  9, 10, 10,
    10, 10,  8,  7,  6,  6,  6,  7,  7,  8,  8,  8,  9,  9,  9, 10,
    10, 10, 10,  8,  8,  7,  7,  7,  7,  8,  8,  8,  8,  9,  9,  9,
    10, 10, 10, 10,  8,  8,  7,  7,  7,  7,  8,  8,  8,  9,  9,  9,
     9, 10, 10, 10, 10,  8,  9,  8,  8,  8,  8,  8,  8,  8,  9,  9,
     9, 10, 10, 10, 10, 10,  8,  9,  8,  8,  8,  8,  8,  8,  9,  9,
     9, 10, 10, 10, 10, 10, 10,  8, 10,  9,  8,  8,  9,  9,  9,  9,
     9, 10, 10, 10, 10, 10, 10, 11,  8, 10,  9,  9,  9,  9,  9,  9,
     9, 10, 10, 10, 10, 10, 10, 11, 11,  8, 11,  9,  9,  9,  9,  9,
     9, 10, 10, 10, 10, 10, 11, 10, 11, 11,  8, 11, 10,  9,  9, 10,
     9, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11,  8, 11, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11,  9, 11, 10,  9,
     9, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11,  9, 11, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11,  9, 12,
    10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11, 12, 12,  9,
     9,  8,  8,  8,  8,  8,  8,  8,  8,  8,  8,  8,  8,  8,  8,  9,
     5
];
const AAC_SPEC_CB11_CODES: &[u16] = &[
    0x000, 0x006, 0x019, 0x03d, 0x09c, 0x0c6, 0x1a7, 0x390,
    0x3c2, 0x3df, 0x7e6, 0x7f3, 0xffb, 0x7ec, 0xffa, 0xffe,
    0x38e, 0x005, 0x001, 0x008, 0x014, 0x037, 0x042, 0x092,
    0x0af, 0x191, 0x1a5, 0x1b5, 0x39e, 0x3c0, 0x3a2, 0x3cd,
    0x7d6, 0x0ae, 0x017, 0x007, 0x009, 0x018, 0x039, 0x040,
    0x08e, 0x0a3, 0x0b8, 0x199, 0x1ac, 0x1c1, 0x3b1, 0x396,
    0x3be, 0x3ca, 0x09d, 0x03c, 0x015, 0x016, 0x01a, 0x03b,
    0x044, 0x091, 0x0a5, 0x0be, 0x196, 0x1ae, 0x1b9, 0x3a1,
    0x391, 0x3a5, 0x3d5, 0x094, 0x09a, 0x036, 0x038, 0x03a,
    0x041, 0x08c, 0x09b, 0x0b0, 0x0c3, 0x19e, 0x1ab, 0x1bc,
    0x39f, 0x38f, 0x3a9, 0x3cf, 0x093, 0x0bf, 0x03e, 0x03f,
    0x043, 0x045, 0x09e, 0x0a7, 0x0b9, 0x194, 0x1a2, 0x1ba,
    0x1c3, 0x3a6, 0x3a7, 0x3bb, 0x3d4, 0x09f, 0x1a0, 0x08f,
    0x08d, 0x090, 0x098, 0x0a6, 0x0b6, 0x0c4, 0x19f, 0x1af,
    0x1bf, 0x399, 0x3bf, 0x3b4, 0x3c9, 0x3e7, 0x0a8, 0x1b6,
    0x0ab, 0x0a4, 0x0aa, 0x0b2, 0x0c2, 0x0c5, 0x198, 0x1a4,
    0x1b8, 0x38c, 0x3a4, 0x3c4, 0x3c6, 0x3dd, 0x3e8, 0x0ad,
    0x3af, 0x192, 0x0bd, 0x0bc, 0x18e, 0x197, 0x19a, 0x1a3,
    0x1b1, 0x38d, 0x398, 0x3b7, 0x3d3, 0x3d1, 0x3db, 0x7dd,
    0x0b4, 0x3de, 0x1a9, 0x19b, 0x19c, 0x1a1, 0x1aa, 0x1ad,
    0x1b3, 0x38b, 0x3b2, 0x3b8, 0x3ce, 0x3e1, 0x3e0, 0x7d2,
    0x7e5, 0x0b7, 0x7e3, 0x1bb, 0x1a8, 0x1a6, 0x1b0, 0x1b2,
    0x1b7, 0x39b, 0x39a, 0x3ba, 0x3b5, 0x3d6, 0x7d7, 0x3e4,
    0x7d8, 0x7ea, 0x0ba, 0x7e8, 0x3a0, 0x1bd, 0x1b4, 0x38a,
    0x1c4, 0x392, 0x3aa, 0x3b0, 0x3bc, 0x3d7, 0x7d4, 0x7dc,
    0x7db, 0x7d5, 0x7f0, 0x0c1, 0x7fb, 0x3c8, 0x3a3, 0x395,
    0x39d, 0x3ac, 0x3ae, 0x3c5, 0x3d8, 0x3e2, 0x3e6, 0x7e4,
    0x7e7, 0x7e0, 0x7e9, 0x7f7, 0x190, 0x7f2, 0x393, 0x1be,
    0x1c0, 0x394, 0x397, 0x3ad, 0x3c3, 0x3c1, 0x3d2, 0x7da,
    0x7d9, 0x7df, 0x7eb, 0x7f4, 0x7fa, 0x195, 0x7f8, 0x3bd,
    0x39c, 0x3ab, 0x3a8, 0x3b3, 0x3b9, 0x3d0, 0x3e3, 0x3e5,
    0x7e2, 0x7de, 0x7ed, 0x7f1, 0x7f9, 0x7fc, 0x193, 0xffd,
    0x3dc, 0x3b6, 0x3c7, 0x3cc, 0x3cb, 0x3d9, 0x3da, 0x7d3,
    0x7e1, 0x7ee, 0x7ef, 0x7f5, 0x7f6, 0xffc, 0xfff, 0x19d,
    0x1c2, 0x0b5, 0x0a1, 0x096, 0x097, 0x095, 0x099, 0x0a0,
    0x0a2, 0x0ac, 0x0a9, 0x0b1, 0x0b3, 0x0bb, 0x0c0, 0x18f,
    0x004
];

const AAC_SPEC_BITS: [&[u8]; 11] = [
    AAC_SPEC_CB1_BITS, AAC_SPEC_CB2_BITS, AAC_SPEC_CB3_BITS, AAC_SPEC_CB4_BITS,
    AAC_SPEC_CB5_BITS, AAC_SPEC_CB6_BITS, AAC_SPEC_CB7_BITS, AAC_SPEC_CB8_BITS,
    AAC_SPEC_CB9_BITS, AAC_SPEC_CB10_BITS, AAC_SPEC_CB11_BITS
];
const AAC_SPEC_CODES: [&[u16]; 11] = [
    AAC_SPEC_CB1_CODES, AAC_SPEC_CB2_CODES, AAC_SPEC_CB3_CODES, AAC_SPEC_CB4_CODES,
    AAC_SPEC_CB5_CODES, AAC_SPEC_CB6_CODES, AAC_SPEC_CB7_CODES, AAC_SPEC_CB8_CODES,
    AAC_SPEC_CB9_CODES, AAC_SPEC_CB10_CODES, AAC_SPEC_CB11_CODES
];
const AAC_UNSIGNED_CODEBOOK: [bool; 11] = [
    false, false, true, true, false, false, true, true, true, true, true
];
const AAC_CODEBOOK_MODULO: [u16; 7] = [
    9, 9, 8, 8, 13, 13, 17
];

const AAC_QUADS: [[i8; 4]; 81] = [
    [ 0, 0, 0, 0 ], [ 0, 0, 0, 1 ], [ 0, 0, 0, 2 ],
    [ 0, 0, 1, 0 ], [ 0, 0, 1, 1 ], [ 0, 0, 1, 2 ],
    [ 0, 0, 2, 0 ], [ 0, 0, 2, 1 ], [ 0, 0, 2, 2 ],
    [ 0, 1, 0, 0 ], [ 0, 1, 0, 1 ], [ 0, 1, 0, 2 ],
    [ 0, 1, 1, 0 ], [ 0, 1, 1, 1 ], [ 0, 1, 1, 2 ],
    [ 0, 1, 2, 0 ], [ 0, 1, 2, 1 ], [ 0, 1, 2, 2 ],
    [ 0, 2, 0, 0 ], [ 0, 2, 0, 1 ], [ 0, 2, 0, 2 ],
    [ 0, 2, 1, 0 ], [ 0, 2, 1, 1 ], [ 0, 2, 1, 2 ],
    [ 0, 2, 2, 0 ], [ 0, 2, 2, 1 ], [ 0, 2, 2, 2 ],
    [ 1, 0, 0, 0 ], [ 1, 0, 0, 1 ], [ 1, 0, 0, 2 ],
    [ 1, 0, 1, 0 ], [ 1, 0, 1, 1 ], [ 1, 0, 1, 2 ],
    [ 1, 0, 2, 0 ], [ 1, 0, 2, 1 ], [ 1, 0, 2, 2 ],
    [ 1, 1, 0, 0 ], [ 1, 1, 0, 1 ], [ 1, 1, 0, 2 ],
    [ 1, 1, 1, 0 ], [ 1, 1, 1, 1 ], [ 1, 1, 1, 2 ],
    [ 1, 1, 2, 0 ], [ 1, 1, 2, 1 ], [ 1, 1, 2, 2 ],
    [ 1, 2, 0, 0 ], [ 1, 2, 0, 1 ], [ 1, 2, 0, 2 ],
    [ 1, 2, 1, 0 ], [ 1, 2, 1, 1 ], [ 1, 2, 1, 2 ],
    [ 1, 2, 2, 0 ], [ 1, 2, 2, 1 ], [ 1, 2, 2, 2 ],
    [ 2, 0, 0, 0 ], [ 2, 0, 0, 1 ], [ 2, 0, 0, 2 ],
    [ 2, 0, 1, 0 ], [ 2, 0, 1, 1 ], [ 2, 0, 1, 2 ],
    [ 2, 0, 2, 0 ], [ 2, 0, 2, 1 ], [ 2, 0, 2, 2 ],
    [ 2, 1, 0, 0 ], [ 2, 1, 0, 1 ], [ 2, 1, 0, 2 ],
    [ 2, 1, 1, 0 ], [ 2, 1, 1, 1 ], [ 2, 1, 1, 2 ],
    [ 2, 1, 2, 0 ], [ 2, 1, 2, 1 ], [ 2, 1, 2, 2 ],
    [ 2, 2, 0, 0 ], [ 2, 2, 0, 1 ], [ 2, 2, 0, 2 ],
    [ 2, 2, 1, 0 ], [ 2, 2, 1, 1 ], [ 2, 2, 1, 2 ],
    [ 2, 2, 2, 0 ], [ 2, 2, 2, 1 ], [ 2, 2, 2, 2 ],
];

const DEFAULT_CHANNEL_MAP: [&str; 9] = [
    "",
    "C",
    "L,R",
    "C,L,R",
    "C,L,R,Cs",
    "C,L,R,Ls,Rs",
    "C,L,R,Ls,Rs,LFE",
    "",
    "C,L,R,Ls,Rs,Lss,Rss,LFE",
];

const SWB_OFFSET_48K_LONG: [usize; 49+1] = [
      0,   4,   8,  12,  16,  20,  24,  28,
     32,  36,  40,  48,  56,  64,  72,  80,
     88,  96, 108, 120, 132, 144, 160, 176,
    196, 216, 240, 264, 292, 320, 352, 384,
    416, 448, 480, 512, 544, 576, 608, 640,
    672, 704, 736, 768, 800, 832, 864, 896,
    928, 1024
];
const SWB_OFFSET_48K_SHORT: [usize; 14+1] = [
    0, 4, 8, 12, 16, 20, 28, 36, 44, 56, 68, 80, 96, 112, 128
];
const SWB_OFFSET_32K_LONG: [usize; 51+1] = [
      0,   4,   8,  12,  16,  20,  24,  28,
     32,  36,  40,  48,  56,  64,  72,  80,
     88,  96, 108, 120, 132, 144, 160, 176,
    196, 216, 240, 264, 292, 320, 352, 384,
    416, 448, 480, 512, 544, 576, 608, 640,
    672, 704, 736, 768, 800, 832, 864, 896,
    928, 960, 992, 1024
];
const SWB_OFFSET_8K_LONG: [usize; 40+1] = [
      0,  12,  24,  36,  48,  60,  72,  84,
     96, 108, 120, 132, 144, 156, 172, 188,
    204, 220, 236, 252, 268, 288, 308, 328,
    348, 372, 396, 420, 448, 476, 508, 544,
    580, 620, 664, 712, 764, 820, 880, 944,
    1024
];
const SWB_OFFSET_8K_SHORT: [usize; 15+1] = [
    0, 4, 8, 12, 16, 20, 24, 28, 36, 44, 52, 60, 72, 88, 108, 128
];
const SWB_OFFSET_16K_LONG: [usize; 43+1] = [
      0,   8,  16,  24,  32,  40,  48,  56,
     64,  72,  80,  88, 100, 112, 124, 136,
    148, 160, 172, 184, 196, 212, 228, 244,
    260, 280, 300, 320, 344, 368, 396, 424,
    456, 492, 532, 572, 616, 664, 716, 772,
    832, 896, 960, 1024
];
const SWB_OFFSET_16K_SHORT: [usize; 15+1] = [
    0, 4, 8, 12, 16, 20, 24, 28, 32, 40, 48, 60, 72, 88, 108, 128
];
const SWB_OFFSET_24K_LONG: [usize; 47+1] = [
      0,   4,   8,  12,  16,  20,  24,  28,
     32,  36,  40,  44,  52,  60,  68,  76,
     84,  92, 100, 108, 116, 124, 136, 148,
    160, 172, 188, 204, 220, 240, 260, 284,
    308, 336, 364, 396, 432, 468, 508, 552,
    600, 652, 704, 768, 832, 896, 960, 1024
];
const SWB_OFFSET_24K_SHORT: [usize; 15+1] = [
    0, 4, 8, 12, 16, 20, 24, 28, 36, 44, 52, 64, 76, 92, 108, 128
];
const SWB_OFFSET_64K_LONG: [usize; 47+1] = [
      0,   4,   8,  12,  16,  20,  24,  28,
     32,  36,  40,  44,  48,  52,  56,  64,
     72,  80,  88, 100, 112, 124, 140, 156,
    172, 192, 216, 240, 268, 304, 344, 384,
    424, 464, 504, 544, 584, 624, 664, 704,
    744, 784, 824, 864, 904, 944, 984, 1024
];
const SWB_OFFSET_64K_SHORT: [usize; 12+1] = [
    0, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 92, 128
];
const SWB_OFFSET_96K_LONG: [usize; 41+1] = [
      0,   4,   8,  12,  16,  20,  24,  28,
     32,  36,  40,  44,  48,  52,  56,  64,
     72,  80,  88,  96, 108, 120, 132, 144,
    156, 172, 188, 212, 240, 276, 320, 384,
    448, 512, 576, 640, 704, 768, 832, 896,
    960, 1024
];

#[derive(Clone,Copy)]
struct GASubbandInfo {
    min_srate:     u32,
    long_bands:    &'static [usize],
    short_bands:   &'static [usize],
}

impl GASubbandInfo {
    fn find(srate: u32) -> GASubbandInfo {
        for sbi in AAC_SUBBAND_INFO.iter() {
            if srate >= sbi.min_srate {
                return *sbi;
            }
        }
        unreachable!("")
    }
    fn find_idx(srate: u32) -> usize {
        for (i, sbi) in AAC_SUBBAND_INFO.iter().enumerate() {
            if srate >= sbi.min_srate {
                return i;
            }
        }
        unreachable!("")
    }
}

const AAC_SUBBAND_INFO: [GASubbandInfo; 12] = [
    GASubbandInfo { min_srate: 92017, long_bands: &SWB_OFFSET_96K_LONG, short_bands: &SWB_OFFSET_64K_SHORT }, //96K
    GASubbandInfo { min_srate: 75132, long_bands: &SWB_OFFSET_96K_LONG, short_bands: &SWB_OFFSET_64K_SHORT }, //88.2K
    GASubbandInfo { min_srate: 55426, long_bands: &SWB_OFFSET_64K_LONG, short_bands: &SWB_OFFSET_64K_SHORT }, //64K
    GASubbandInfo { min_srate: 46009, long_bands: &SWB_OFFSET_48K_LONG, short_bands: &SWB_OFFSET_48K_SHORT }, //48K
    GASubbandInfo { min_srate: 37566, long_bands: &SWB_OFFSET_48K_LONG, short_bands: &SWB_OFFSET_48K_SHORT }, //44.1K
    GASubbandInfo { min_srate: 27713, long_bands: &SWB_OFFSET_32K_LONG, short_bands: &SWB_OFFSET_48K_SHORT }, //32K
    GASubbandInfo { min_srate: 23004, long_bands: &SWB_OFFSET_24K_LONG, short_bands: &SWB_OFFSET_24K_SHORT }, //24K
    GASubbandInfo { min_srate: 18783, long_bands: &SWB_OFFSET_24K_LONG, short_bands: &SWB_OFFSET_24K_SHORT }, //22.05K
    GASubbandInfo { min_srate: 13856, long_bands: &SWB_OFFSET_16K_LONG, short_bands: &SWB_OFFSET_16K_SHORT }, //16K
    GASubbandInfo { min_srate: 11502, long_bands: &SWB_OFFSET_16K_LONG, short_bands: &SWB_OFFSET_16K_SHORT }, //12K
    GASubbandInfo { min_srate:  9391, long_bands: &SWB_OFFSET_16K_LONG, short_bands: &SWB_OFFSET_16K_SHORT }, //11.025K
    GASubbandInfo { min_srate:     0, long_bands: &SWB_OFFSET_8K_LONG,  short_bands: &SWB_OFFSET_8K_SHORT  }, //8K
];
