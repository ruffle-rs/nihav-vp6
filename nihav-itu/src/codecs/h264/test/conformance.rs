use nihav_core::codecs::RegisteredDecoders;
use nihav_core::demuxers::RegisteredDemuxers;
use nihav_codec_support::test::dec_video::*;
use nihav_commonfmt::generic_register_all_demuxers;
use crate::itu_register_all_decoders;

use super::raw_demux::RawH264DemuxerCreator;

const PREFIX: &str = "assets/ITU/h264-conformance/";

fn test_files(names: &[(&str, [u32; 4])]) {
    let mut dmx_reg = RegisteredDemuxers::new();
    dmx_reg.add_demuxer(&RawH264DemuxerCreator{});
    generic_register_all_demuxers(&mut dmx_reg);
    let mut dec_reg = RegisteredDecoders::new();
    itu_register_all_decoders(&mut dec_reg);

    for (name, hash) in names.iter() {
        let test_name = format!("{}{}", PREFIX, name);
        println!("Testing {}", test_name);
        test_decoding("rawh264", "h264", &test_name, None, &dmx_reg, &dec_reg, ExpectedTestResult::MD5(*hash));
    }
}

const GENERAL_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("NL1_Sony_D.jsv", [0xD4BB8D98, 0x0C1377EE, 0x45515763, 0xAE7989FD]),
    ("SVA_NL1_B.264", [0xB5626983, 0xAC087749, 0x7FFF9A4B, 0x10D2F1D4]),
    ("NL2_Sony_H.jsv", [0x48D8380C, 0xDB7EFF52, 0x116C1AAD, 0xDBC583F5]),
    ("SVA_NL2_E.264", [0xB47E932D, 0x43628801, 0x3B8453D9, 0xA1D0F60D]),
    ("BA1_Sony_D.jsv", [0xFDE8F7A6, 0x34A7B7CE, 0xA7F1317D, 0xAAF5EB7C]),
    ("SVA_BA1_B.264", [0x627288C4, 0xE4D4E7A6, 0xA13F187C, 0x4A7A9A4D]),
    ("BA2_Sony_F.jsv", [0xB4C1B35F, 0xDC25B520, 0x5E842E64, 0x19C0E81A]),
    ("SVA_BA2_D.264", [0x18B60729, 0x98CDA04B, 0x278B1436, 0x27FC9D4A]),
    ("BA_MW_D.264", [0xC42C2D96, 0xC49254A6, 0xE980B174, 0xDB1CE2D8]),
    ("BANM_MW_D.264", [0x6572ACB5, 0xE65EA0BC, 0x4A7ECBE7, 0xE436E654]),
    ("BA1_FT_C.264", [0x355A737E, 0xE9FBDE6E, 0xAA47ACFD, 0xED7D2475]),
    ("NLMQ1_JVC_C.264", [0xB5DE2480, 0xBD391286, 0x7FE69D65, 0x7AADDD6E]),
    ("NLMQ2_JVC_C.264", [0x35635990, 0xBE9CB3E5, 0x1000CBB1, 0xC8322D5B]),
    ("BAMQ1_JVC_C.264", [0x04B40C4A, 0xF5A4B4C0, 0x94D77821, 0x79D12A88]),
    ("BAMQ2_JVC_C.264", [0xDAB08F3D, 0x5E304802, 0xC91AC830, 0x71BFB9DE]),
    ("SVA_Base_B.264", [0x4B5BB06C, 0x8C698DA3, 0xABFAD6B9, 0xA28852D2]),
    ("SVA_FM1_E.264", [0x5A20AF6C, 0xDBE9B632, 0x5D752096, 0xC587A7F1]),
    ("BASQP1_Sony_C.jsv", [0xB49014B2, 0xDC04FE5A, 0x6138C083, 0x387A9A9B]),
    /*"FM1_BT_B.h264",
    "FM2_SVA_C.264",
    "FM1_FT_E.264",*/ //special slice modes
    ("CI_MW_D.264", [0x4571A884, 0xA6C7856F, 0x4377928C, 0x830246E3]),
    ("SVA_CL1_E.264", [0x5723A151, 0x8DE9FADC, 0xA7499C5B, 0xA34DA7C4]),
    ("CI1_FT_B.264", [0x411ECE62, 0xFDD3791E, 0xE3E90B82, 0x1B79CF77]),
    ("CVFC1_Sony_C.jsv", [0x78E5AAA2, 0x48CC85CC, 0x68DD1D56, 0x535F6ED0]),
    ("AUD_MW_E.264", [0xE96FE505, 0x4DE0329A, 0x8868D060, 0x03375CDB]),
    ("MIDR_MW_D.264", [0x527E7207, 0x584DFE19, 0x3346316F, 0xCBAB1516]),
    ("NRF_MW_E.264", [0x22F2011C, 0x44661F4D, 0xABBBD4A2, 0x423AB9B8]),
    ("MPS_MW_A.264", [0x3159BB10, 0xD656899D, 0xD13D89E2, 0x44F6F5BD]),
    ("CVBS3_Sony_C.jsv", [0xFF57F1A4, 0xD03A6599, 0x8CDC4EFE, 0x19DC4ADB]),
    ("BA3_SVA_C.264", [0xe35fe99a, 0xd8ebef51, 0x017e2169, 0xe48e3ad5]),
    ("SL1_SVA_B.264", [0x738E8AAD, 0x711E58FE, 0x76C5E366, 0x432BBB90]),
    ("NL3_SVA_E.264", [0x428B0604, 0xFF02E0A0, 0x0DA08577, 0xDA0EEB76]),
    ("cvmp_mot_frm0_full_B.26l", [0xb8baed20, 0x7e57efcb, 0x22ba5538, 0x849a573f]),
    // no direct mention
    //"FM2_SVA_B.264", //special slice mode
];
#[test]
fn test_h264_general() {
    test_files(GENERAL_TEST_STREAMS);
}

const I_PCM_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CVPCMNL1_SVA_C.264", [0x5C1FD0F6, 0x8E875200, 0x711FEBF1, 0xD683E58F]),
    ("CVPCMNL2_SVA_C.264", [0xAF1F1DBE, 0x1DD6569C, 0xB02271F0, 0x53217D88]),
];
#[test]
fn test_h264_ipcm() {
    test_files(I_PCM_TEST_STREAMS);
}

const MMCO_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("MR1_BT_A.h264", [0x6c9c7a22, 0x3c5d0f04, 0x73d7e777, 0x46d8e1c9]),
    ("MR2_TANDBERG_E.264", [0x225aff94, 0x8c079867, 0xf7f0af24, 0xc4093834]),
    ("MR3_TANDBERG_B.264", [0x49728ec3, 0x3d6247de, 0x72dd49ae, 0x22c11930]),
    ("MR4_TANDBERG_C.264", [0x98aaed22, 0x5bf63437, 0x8209bc05, 0x58ad5782]),
    ("MR5_TANDBERG_C.264", [0xa1c5e24a, 0xf96e4801, 0x2f2ac7f6, 0xb61e2779]),
    ("MR1_MW_A.264", [0x6e6ba67d, 0x1829c4e1, 0x639e9ec3, 0x68b72208]),
    ("MR2_MW_A.264", [0x08499fbb, 0x2a566e46, 0x72e5685e, 0xcacb802c]),
    /*"MR6_BT_B.h264",
    "MR7_BT_B.h264",
    "MR8_BT_B.h264",*/ // interlaced coding
    ("HCBP1_HHI_A.264", [0x69F61D9D, 0x050F777D, 0x894C3191, 0x76A33A13]),
    ("HCBP2_HHI_A.264", [0xEAE95099, 0x1F6CD60B, 0xE2435713, 0x5E4661CA]),
];
#[test]
fn test_h264_mmco() {
    test_files(MMCO_TEST_STREAMS);
}

const WP_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CVWP5_TOSHIBA_E.264", [0xB6C61135, 0x9A6F86DE, 0xC46445A3, 0x350A75B2]),
    ("CVWP1_TOSHIBA_E.264", [0xA3F64FC4, 0xC18AA1A1, 0x622C6D25, 0x289930B2]),
    ("CVWP2_TOSHIBA_E.264", [0x42c18fb8, 0x9062f091, 0xa06c9ac1, 0x00d3bc80]),
    ("CVWP3_TOSHIBA_E.264", [0x76e164a1, 0x26ff7073, 0x655f1fe9, 0xac40a0fd]),
];
#[test]
fn test_h264_wp() {
    test_files(WP_TEST_STREAMS);
}

/*const FIELD_CODING_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "CVNLFI1_Sony_C.jsv",
    "CVNLFI2_Sony_H.jsv",
    "Sharp_MP_Field_1_B.jvt",
    "Sharp_MP_Field_2_B.jvt",
    "Sharp_MP_Field_3_B.jvt",
    "CVFI1_Sony_D.jsv",
    "CVFI2_Sony_H.jsv",
    "FI1_Sony_E.jsv",
    "CVFI1_SVA_C.264",
    "CVFI2_SVA_C.264",
    "cvmp_mot_fld0_full_B.26l",
    "CVMP_MOT_FLD_L30_B.26l",
];
#[test]
fn test_h264_field() {
    test_files(FIELD_CODING_TEST_STREAMS);
}*/

/*const FRAME_FIELD_CODING_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "Sharp_MP_PAFF_1r2.jvt",
    "CVPA1_TOSHIBA_B.264",
    "cvmp_mot_picaff0_full_B.26l",
];
#[test]
fn test_h264_frame_field() {
    test_files(FRAME_FIELD_CODING_TEST_STREAMS);
}*/

/*const MBAFF_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "CVMANL1_TOSHIBA_B.264",
    "CVMANL2_TOSHIBA_B.264",
    "CVMA1_Sony_D.jsv",
    "CVMA1_TOSHIBA_B.264",
    "CVMAQP2_Sony_G.jsv",
    "CVMAQP3_Sony_D.jsv",
    "CVMAPAQP3_Sony_E.jsv",
    "cvmp_mot_mbaff0_full_B.26l",
    "CVMP_MOT_FRM_L31_B.26l",
];
#[test]
fn test_h264_mbaff() {
    test_files(MBAFF_CODING_TEST_STREAMS);
}*/

/*const S_PICTURE_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "sp1_bt_a.h264",
    "sp2_bt_b.h264",
];
#[test]
fn test_h264_s_picture() {
    test_files(S_PICTURE_TEST_STREAMS);
}*/

const LONG_SEQUENCE_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("LS_SVA_D.264", [0xA1F4C1CC, 0x701AF32F, 0x985CDE87, 0xA0785B4D]),
];
#[test]
fn test_h264_long_sequence() {
    test_files(LONG_SEQUENCE_TEST_STREAMS);
}

const SEI_VUI_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CVSE2_Sony_B.jsv", [0xDC811E9A, 0xD11D06A0, 0x00F55FF3, 0x2179433E]),
    ("CVSE3_Sony_H.jsv", [0x30CCF52E, 0x2B0DCE8F, 0x98384A84, 0x51BD4F89]),
    ("CVSEFDFT3_Sony_E.jsv", [0x1EA2228B, 0xBDD88D50, 0x95C452C4, 0xC75A5229]),
];
#[test]
fn test_h264_sei_vui() {
    test_files(SEI_VUI_TEST_STREAMS);
}

const CABAC_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CANL1_TOSHIBA_G.264", [0xAFA07274, 0x6B16BD96, 0xF3152B45, 0xE2F2881E]),
    ("CANL1_Sony_E.jsv", [0x27F1D5D3, 0x89E110FC, 0x320788BF, 0x78006DB0]),
    ("CANL2_Sony_E.jsv", [0x3A28438E, 0x3E0795DE, 0xAED795FC, 0xFEFBC833]),
    ("CANL3_Sony_C.jsv", [0xFE2DC3CB, 0xA055044C, 0x739911B0, 0xE6AA66BA]),
    ("CANL1_SVA_B.264", [0xB02DEFCB, 0x741C0E98, 0x2313C574, 0x9F2008ED]),
    ("CANL2_SVA_B.264", [0xB02DEFCB, 0x741C0E98, 0x2313C574, 0x9F2008ED]),
    ("CANL3_SVA_B.264", [0x04A6DE98, 0x4EF88D1B, 0x8C1B26FC, 0x8F33A425]),
    ("CANL4_SVA_B.264", [0x19cee0ac, 0xcfbebacc, 0x57aa4cf0, 0x3e4ef26d]),
    ("CABA1_Sony_D.jsv", [0x5EB23E95, 0xD9908DBD, 0x68AAA5BF, 0x775071DE]),
    ("CABA2_Sony_E.jsv", [0xB60EE63C, 0xB7A969DA, 0x88C9120D, 0xEB6752F6]),
    ("CABA3_Sony_C.jsv", [0xC74CA8A2, 0x509C153C, 0xFE7ABF23, 0xABF8F8F0]),
    ("CABA3_TOSHIBA_E.264", [0xC559BBDC, 0x2939EBD9, 0xD09CAA95, 0x63DF81DD]),
    ("CABA1_SVA_B.264", [0x466A59AE, 0x3968AADD, 0x529FEDFB, 0x87539141]),
    ("CABA2_SVA_B.264", [0xEF495A1D, 0x8F02E1E7, 0xCA128ACC, 0xC4086CFE]),
    ("CABA3_SVA_B.264", [0x09F84428, 0xE29B6602, 0x87EF56CF, 0x6093B54F]),
    ("camp_mot_frm0_full.26l", [0x0CA9541B, 0xCEF163D0, 0x75FC5817, 0x45132421]),
];
#[test]
fn test_h264_cabac() {
    test_files(CABAC_TEST_STREAMS);
}

const CABAC_INIT_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CABACI3_Sony_B.jsv", [0xD74CBB99, 0x81FE3018, 0x0F4A15CD, 0x4C9B490D]),
];
#[test]
fn test_h264_cabac_init() {
    test_files(CABAC_INIT_TEST_STREAMS);
}

const CABAC_MB_QPTEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CAQP1_Sony_B.jsv", [0x3C60A84F, 0xA2A2F0CB, 0x6FEB91AE, 0xD97E36C5]),
    ("CACQP3_Sony_D.jsv", [0x296FAD20, 0x0369FF53, 0x042FE3A3, 0xDE6BB6C3]),
];
#[test]
fn test_h264_cabac_mb_qp() {
    test_files(CABAC_MB_QPTEST_STREAMS);
}

const CABAC_SLICE_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CABAST3_Sony_E.jsv", [0xF97789B5, 0x85A499DF, 0xAED8B05F, 0xA5024D66]),
    ("CABASTBR3_Sony_B.jsv", [0xbc738fb9, 0x946298c0, 0x3f3f894e, 0x3a10c6bc]),
];
#[test]
fn test_h264_cabac_slice() {
    test_files(CABAC_SLICE_TEST_STREAMS);
}

const CABAC_I_PCM_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CAPCMNL1_Sand_E.264", [0xEE9968EE, 0xEFE935F0, 0x45C6B70B, 0xE51691EB]),
    ("CAPCM1_Sand_E.264", [0x318680B7, 0x85FA5499, 0xB4C4B2A4, 0xD43AA656]),
    ("CAPM3_Sony_D.jsv", [0xB515EEDB, 0xF1E4C5A6, 0xD217B1C8, 0xFBEC1DB9]),
];
#[test]
fn test_h264_cabac_ipcm() {
    test_files(CABAC_I_PCM_TEST_STREAMS);
}

const CABAC_MMCO_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    /*"MR9_BT_B.h264",*/ //MBAFF
    ("HCMP1_HHI_A.264", [0x4ec3788f, 0x2bec7e4c, 0xade27eee, 0xda17b05d]),
];
#[test]
fn test_h264_cabac_mmco() {
    test_files(CABAC_MMCO_TEST_STREAMS);
}

const CABAC_WP_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("CAWP1_TOSHIBA_E.264", [0x305937E4, 0x30B50003, 0xDEC317BD, 0x3A0CDB9C]),
    ("CAWP5_TOSHIBA_E.264", [0xB6C61135, 0x9A6F86DE, 0xC46445A3, 0x350A75B2]),
];
#[test]
fn test_h264_cabac_wp() {
    test_files(CABAC_WP_TEST_STREAMS);
}

/*const CABAC_FIELD_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "CABREF3_Sand_D.264",
    "CAFI1_SVA_C.264",
    "camp_mot_fld0_full.26l",
];
#[test]
fn test_h264_cabac_field_() {
    test_files(CABAC_FIELD_TEST_STREAMS);
}*/

/*const CABAC_FIELD_FRAME_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "Sharp_MP_PAFF_2.jvt",
    "CAPA1_TOSHIBA_B.264",
    "camp_mot_picaff0_full.26l",
];
#[test]
fn test_h264_cabac_field_frame() {
    test_files(CABAC_FIELD_FRAMETEST_STREAMS);
}*/

/*const CABAC_MBAFF_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "CAMANL1_TOSHIBA_B.264",
    "CAMANL2_TOSHIBA_B.264",
    "CANLMA2_Sony_C.jsv",
    "CANLMA3_Sony_C.jsv",
    "CAMA1_Sony_C.jsv",
    "CAMA1_TOSHIBA_B.264",
    "CAMANL3_Sand_E.264",
    "CAMA3_Sand_E.264",
    "CAMASL3_Sony_B.jsv",
    "CAMACI3_Sony_C.jsv",
    "camp_mot_mbaff0_full.26l",
    "CAMP_MOT_MBAFF_L30.26l",
    "CAMP_MOT_MBAFF_L31.26l",
    "CAPAMA3_Sand_F.264",
    "cama1_vtc_c.avc",
    "cama2_vtc_b.avc",
    "cama3_vtc_b.avc",
];
#[test]
fn test_h264_cabac_mbaff() {
    test_files(CABAC_MBAFF_TEST_STREAMS);
}*/

/*const CABAC_CAVLC_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "CVCANLMA2_Sony_C.jsv",
];
#[test]
fn test_h264_cabac_cavlc() {
    test_files(CABAC_CAVLC_TEST_STREAMS);
}*/ // contains MBAFF

const CABAC_PRED_BW_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("src19td.IBP.264", [0xEE7F2F8E, 0x722B297A, 0x532DFA94, 0xDEE55779]),
];
#[test]
fn test_h264_cabac_pred_bw() {
    test_files(CABAC_PRED_BW_TEST_STREAMS);
}

const FREXT_420_8_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    ("FRext/FRExt1_Panasonic.avc", [0xBD8EA0B1, 0x9668C25E, 0xFBB50D85, 0xABAFFE7C]),
    ("FRext/FRExt3_Panasonic.avc", [0x39772F7C, 0xC227DCE3, 0x80732096, 0xEB970937]),
    ("FRext/HCAFR1_HHI.264", [0x4C1C4214, 0x7190D5B8, 0x6650E6B9, 0xD86BCB03]),
    //("FRext/HCAFF1_HHI.264", [0;4]), //PAFF
    //("FRext/HCAMFF1_HHI.264", [0;4]), //MBAFF
    //("FRext/FRExt2_Panasonic.avc", [0;4]), //PAFF
    //("FRext/FRExt4_Panasonic.avc", [0;4]), //MBAFF
    ("FRext/HPCANL_BRCM_C.264", [0x24BB8150, 0xC03A9FBC, 0x304A427C, 0x5C11B5D7]),
    ("FRext/HPCA_BRCM_C.264", [0x46AF80A6, 0x8CAA5AD0, 0x42F65E88, 0x0EEE65E4]),
    /*("FRext/HPCAFLNL_BRCM_C.264", [0;4]), //PAFF
    ("FRext/HPCAFL_BRCM_C.264", [0;4]),*/
    ("FRext/HCAFR2_HHI.264", [0x79CC14EA, 0xBD39DDFF, 0x82D49538, 0xF3D9AE1A]),
    ("FRext/HCAFR3_HHI.264", [0x280AF93D, 0x551539E1, 0xA3F1979D, 0xC1CF64DF]),
    ("FRext/HCAFR4_HHI.264", [0x6E80B189, 0xAAE83055, 0x6F51F4EE, 0xC3BEE5C8]),
    ("FRext/HPCADQ_BRCM_B.264", [0xCAB10745, 0xB7CB657A, 0xB51600CE, 0x7C7E7A19]),
    ("FRext/HPCALQ_BRCM_B.264", [0xCAB10745, 0xB7CB657A, 0xB51600CE, 0x7C7E7A19]),
    //("FRext/HPCAMAPALQ_BRCM_B.264", [0;4]), //MBAFF
    ("FRext/HPCV_BRCM_A.264", [0x9B2D963E, 0x953DE431, 0x8A4385F8, 0x41D7C42C]),
    ("FRext/HPCVNL_BRCM_A.264", [0x45E2D980, 0xFAB71BA7, 0xC2DFD63B, 0x80AC89E7]),
    /*("FRext/HPCVFL_BRCM_A.264", [0;4]), //PAFF
    ("FRext/HPCVFLNL_BRCM_A.264", [0;4]),*/
    //("FRext/HPCVMOLQ_BRCM_B.264", [0;4]), //grayscale
    //("FRext/HPCAMOLQ_BRCM_B.264", [0;4]), //grayscale
    ("FRext/HPCAQ2LQ_BRCM_B.264", [0x04101005, 0x61E5ED27, 0xBBD135FF, 0x7E35F162]),
    ("FRext/Freh1_B.264", [0xC9FB3A23, 0x59564945, 0x659E23DB, 0x2D61DE13]),
    ("FRext/Freh2_B.264", [0x3E1853A5, 0x7B36CA1A, 0xDEDA7FB6, 0xFF60A2E7]),
    ("FRext/freh3.264", [0x482BA0B8, 0x388252D8, 0x0B7095C9, 0x07D32939]),
    //("FRext/freh4.264", [0;4]), //PAFF
    //("FRext/freh5.264", [0;4]), //MBAFF
    //("FRext/freh6.264", [0;4]), //PAFF
    //("FRext/Freh7_B.264", [0;4]), //PAFF
    ("FRext/freh8.264", [0xFC3BC8E0, 0xF6728372, 0x448C0E26, 0xE7472E6F]),
    ("FRext/freh9.264", [0xA118CCC1, 0xBDFFDFF0, 0xAD0FD32F, 0x9A3821A3]),
    //("FRext/freh10.264", [0;4]), //PAFF
    //("FRext/freh11.264", [0;4]), //PAFF
    ("FRext/Freh12_B.264", [0xE474287F, 0xCB9CCD28, 0xFD24CD02, 0x02E97603]),
    /*("FRext/FREXT01_JVC_D.264", [0;4]), //MBAFF
    ("FRext/FREXT02_JVC_C.264", [0;4]),*/
    ("FRext/FRExt_MMCO4_Sony_B.264", [0x3B226B30, 0x42AC899B, 0x9FE1EB2C, 0x4B6ED90C]),

    ("FRext/test8b43.264", [0x81A43E33, 0x6811D40D, 0x2DEAAC38, 0xBCC4F535]),
];
#[test]
fn test_h264_frext_420_8() {
    test_files(FREXT_420_8_TEST_STREAMS);
}

/*const FREXT_420_10I_TEST_STREAMS: &[(&str, [u32; 4])] = &[
    "FRext/PPH10I1_Panasonic_A.264",
    "FRext/PPH10I2_Panasonic_A.264",
    "FRext/PPH10I3_Panasonic_A.264",
    "FRext/PPH10I4_Panasonic_A.264",
    "FRext/PPH10I5_Panasonic_A.264",
    "FRext/PPH10I6_Panasonic_A.264",
    "FRext/PPH10I7_Panasonic_A.264",
];
#[test]
fn test_h264_frext_420_10i() {
    test_files(FREXT_420_10I_TEST_STREAMS);
}*/
