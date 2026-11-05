// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! HAL300 (26A434) firmware layouts. Unknown fields retain offset names.

use kernel::prelude::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Object {
    HardwareData = 0,
    PrivateData = 1,
    RegionC = 2,
    ZeroPage = 3,
    ComputeGlobal = 4,
    Completion0 = 5,
    Completion1 = 6,
    Completion2 = 7,
    Completion3 = 8,
    SubmissionArena = 9,
    PoolDescriptors = 10,
    ComputePool0 = 11,
    ComputePool1 = 12,
    ComputePool2 = 13,
    RegionA = 14,
    PrimaryRoot = 15,
    SecondaryRoot = 16,
    PbDescriptors = 17,
    StampA = 18,
    StampB = 19,
}

pub(crate) struct ObjectSpec {
    pub(crate) kind: Object,
    pub(crate) va: u64,
    pub(crate) size: usize,
    pub(crate) aliases: [u64; 2],
}

/// Owner calibration, version 1. All integers and IEEE bit patterns are little endian.
#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
pub(crate) struct Calibration {
    pub(crate) version: u32,
    pub(crate) chip_id: u32,
    pub(crate) core_mask: u32,
    pub(crate) sample_period_ms: u32,
    pub(crate) hardwaredata_unk_0ff4: u32,
    pub(crate) maximum_pstate: u32,
    pub(crate) hardwaredata_unk_19e8: u32,
    pub(crate) hardwaredata_unk_19ec: u32,
    pub(crate) hardwaredata_unk_19f0: u32,
    pub(crate) hardwaredata_unk_19f4: u32,
    pub(crate) hardwaredata_unk_19f8: u32,
    pub(crate) hardwaredata_unk_19fc: u32,
    pub(crate) hardwaredata_unk_1a00: u32,
    pub(crate) hardwaredata_unk_1a04: u32,
    pub(crate) hardwaredata_unk_1a08: u32,
    pub(crate) hardwaredata_unk_1a0c: u32,
    pub(crate) hardwaredata_unk_1a10: u32,
    pub(crate) hardwaredata_unk_1a14: u32,
    pub(crate) hardwaredata_unk_1a18: u32,
    pub(crate) hardwaredata_unk_1a28: u64,
    pub(crate) hardwaredata_unk_1a30: u64,
    pub(crate) hardwaredata_unk_1a38: u64,
    pub(crate) hardwaredata_unk_1a40: u64,
    pub(crate) hardwaredata_unk_1a48: u64,
    pub(crate) hardwaredata_unk_1a50: u64,
    pub(crate) hardwaredata_unk_53c0: u32,
    pub(crate) hardwaredata_unk_53c8: u32,
    pub(crate) hardwaredata_unk_5400: u32,
    pub(crate) hardwaredata_unk_5408: u32,
    pub(crate) hardwaredata_unk_5d6c: u64,
    pub(crate) hardwaredata_unk_5d7c: u32,
    pub(crate) hardwaredata_unk_5d80: u64,
    pub(crate) hardwaredata_unk_5d88: u32,
    pub(crate) hardwaredata_unk_5d94: u32,
    pub(crate) hardwaredata_unk_5dc8: u32,
    pub(crate) hardwaredata_unk_5e00: u32,
    pub(crate) hardwaredata_unk_5e0c: u32,
    pub(crate) hardwaredata_unk_5e14: u32,
    pub(crate) hardwaredata_unk_5e1c: u32,
    pub(crate) hardwaredata_unk_5e38: u64,
    pub(crate) hardwaredata_unk_5e4c: u32,
    pub(crate) hardwaredata_unk_5e60: u64,
    pub(crate) hardwaredata_unk_5ec0: u64,
    pub(crate) hardwaredata_unk_5ec8: u64,
    pub(crate) hardwaredata_unk_5f04: u32,
    pub(crate) hardwaredata_unk_5f08: u64,
    pub(crate) hardwaredata_unk_5f10: u32,
    pub(crate) hardwaredata_unk_5f50: u64,
    pub(crate) effort_normalization: u32,
    pub(crate) effort_floor_min: u32,
    pub(crate) effort_floor_max: u32,
    pub(crate) effort_max_frequency_mhz: u32,
    pub(crate) hardwaredata_unk_7648: u32,
    pub(crate) hardwaredata_unk_7654: u32,
    pub(crate) hardwaredata_unk_76e8: u64,
    pub(crate) hardwaredata_unk_76f0: u32,
    pub(crate) hardwaredata_unk_7730: u32,
    pub(crate) hardwaredata_unk_7b92: u32,
    pub(crate) hardwaredata_unk_7bd6: u32,
    pub(crate) hardwaredata_unk_7ca8: u32,
    pub(crate) hardwaredata_unk_7e18: u32,
    pub(crate) hardwaredata_unk_7e20: u32,
    pub(crate) hardwaredata_unk_7e24: u32,
    pub(crate) hardwaredata_unk_7e28: u32,
    pub(crate) hardwaredata_unk_7e40: u32,
    pub(crate) hardwaredata_unk_7e44: u64,
    pub(crate) hardwaredata_unk_7f14: u32,
    pub(crate) hardwaredata_unk_7f20: u32,
    pub(crate) hardwaredata_unk_7f28: u32,
    pub(crate) hardwaredata_unk_7f30: u32,
    pub(crate) hardwaredata_unk_7f4c: u64,
    pub(crate) hardwaredata_unk_7f54: u32,
    pub(crate) hardwaredata_unk_7f5c: u32,
    pub(crate) hardwaredata_unk_7f60: u32,
    pub(crate) hardwaredata_unk_7f6c: u32,
    pub(crate) hardwaredata_unk_7f74: u64,
    pub(crate) hardwaredata_unk_7f94: u32,
    pub(crate) hardwaredata_unk_80c0: u32,
    pub(crate) hardwaredata_unk_80c4: u32,
    pub(crate) hardwaredata_unk_80dc: u64,
    pub(crate) hardwaredata_unk_80e4: u32,
    pub(crate) hardwaredata_unk_80ec: u32,
    pub(crate) hardwaredata_unk_80fc: u32,
    pub(crate) hardwaredata_unk_8100: u32,
    pub(crate) hardwaredata_unk_8104: u64,
    pub(crate) hardwaredata_unk_810c: u64,
    pub(crate) hardwaredata_unk_8140: u32,
    pub(crate) hardwaredata_unk_8144: u32,
    pub(crate) hardwaredata_unk_819c: u32,
    pub(crate) hardwaredata_unk_8334: u32,
    pub(crate) hardwaredata_unk_8338: u32,
    pub(crate) hardwaredata_unk_839c: u32,
    pub(crate) hardwaredata_unk_83a0: u32,
    pub(crate) hardwaredata_unk_83a4: u32,
    pub(crate) hardwaredata_unk_83b0: u32,
    pub(crate) hardwaredata_unk_83e4: u32,
    pub(crate) hardwaredata_unk_8430: u32,
    pub(crate) hardwaredata_unk_bfec: u32,
    pub(crate) hardwaredata_unk_c02c: u32,
    pub(crate) hardwaredata_unk_c06c: u32,
    pub(crate) privatedata_unk_ea90: u32,
    pub(crate) privatedata_unk_ea94: u32,
    pub(crate) privatedata_unk_ebac: u32,
    pub(crate) privatedata_unk_ebb4: u32,
    pub(crate) privatedata_unk_f0a8: u32,
    pub(crate) privatedata_unk_f0b8: u32,
    pub(crate) regionc_unk_0088: u32,
}
// SAFETY: The packed record contains only integers.
unsafe impl crate::util::AnyBitPattern for Calibration {}

#[repr(C, packed)]
pub(crate) struct HardwareData {
    pub(crate) unk_0000: u64,
    pub(crate) unk_0008: u64,
    pub(crate) unk_0010: u64,
    pub(crate) unk_0018: u64,
    pub(crate) unk_0020: u64,
    pub(crate) unk_0028: u64,
    pub(crate) pad_0030: [u8; 0x630],
    pub(crate) unk_0660: u32,
    pub(crate) pad_0664: [u8; 0x24],
    pub(crate) unk_0688: u32,
    pub(crate) pad_068c: [u8; 0x24],
    pub(crate) unk_06b0: u32,
    pub(crate) pad_06b4: [u8; 0x24],
    pub(crate) unk_06d8: u32,
    pub(crate) pad_06dc: [u8; 0x24],
    pub(crate) unk_0700: u32,
    pub(crate) pad_0704: [u8; 0x24],
    pub(crate) unk_0728: u32,
    pub(crate) pad_072c: [u8; 0x24],
    pub(crate) unk_0750: u32,
    pub(crate) pad_0754: [u8; 0x24],
    pub(crate) unk_0778: u32,
    pub(crate) pad_077c: [u8; 0x24],
    pub(crate) unk_07a0: u32,
    pub(crate) pad_07a4: [u8; 0x4],
    pub(crate) unk_07a8: u64,
    pub(crate) ptr_07b0: u64,
    pub(crate) unk_07b8: u64,
    pub(crate) pad_07c0: [u8; 0x8],
    pub(crate) unk_07c8: u32,
    pub(crate) pad_07cc: [u8; 0x4],
    pub(crate) unk_07d0: u64,
    pub(crate) ptr_07d8: u64,
    pub(crate) unk_07e0: u64,
    pub(crate) pad_07e8: [u8; 0x8],
    pub(crate) unk_07f0: u32,
    pub(crate) pad_07f4: [u8; 0x24],
    pub(crate) unk_0818: u32,
    pub(crate) pad_081c: [u8; 0x24],
    pub(crate) unk_0840: u32,
    pub(crate) pad_0844: [u8; 0x24],
    pub(crate) unk_0868: u32,
    pub(crate) pad_086c: [u8; 0x24],
    pub(crate) unk_0890: u32,
    pub(crate) pad_0894: [u8; 0x24],
    pub(crate) unk_08b8: u32,
    pub(crate) pad_08bc: [u8; 0x24],
    pub(crate) unk_08e0: u32,
    pub(crate) pad_08e4: [u8; 0x4],
    pub(crate) register_08e8: u64,
    pub(crate) ptr_08f0: u64,
    pub(crate) unk_08f8: u64,
    pub(crate) pad_0900: [u8; 0x8],
    pub(crate) unk_0908: u32,
    pub(crate) pad_090c: [u8; 0x24],
    pub(crate) unk_0930: u32,
    pub(crate) pad_0934: [u8; 0x24],
    pub(crate) unk_0958: u32,
    pub(crate) pad_095c: [u8; 0x24],
    pub(crate) unk_0980: u32,
    pub(crate) pad_0984: [u8; 0x24],
    pub(crate) unk_09a8: u32,
    pub(crate) pad_09ac: [u8; 0x4],
    pub(crate) register_09b0: u64,
    pub(crate) ptr_09b8: u64,
    pub(crate) unk_09c0: u64,
    pub(crate) pad_09c8: [u8; 0x8],
    pub(crate) unk_09d0: u32,
    pub(crate) pad_09d4: [u8; 0x24],
    pub(crate) unk_09f8: u32,
    pub(crate) pad_09fc: [u8; 0x4],
    pub(crate) register_0a00: u64,
    pub(crate) ptr_0a08: u64,
    pub(crate) unk_0a10: u64,
    pub(crate) pad_0a18: [u8; 0x8],
    pub(crate) unk_0a20: u32,
    pub(crate) pad_0a24: [u8; 0x24],
    pub(crate) unk_0a48: u32,
    pub(crate) pad_0a4c: [u8; 0x24],
    pub(crate) unk_0a70: u32,
    pub(crate) pad_0a74: [u8; 0x24],
    pub(crate) unk_0a98: u32,
    pub(crate) pad_0a9c: [u8; 0x24],
    pub(crate) unk_0ac0: u32,
    pub(crate) pad_0ac4: [u8; 0x24],
    pub(crate) unk_0ae8: u32,
    pub(crate) pad_0aec: [u8; 0x24],
    pub(crate) unk_0b10: u32,
    pub(crate) pad_0b14: [u8; 0x4],
    pub(crate) register_0b18: u64,
    pub(crate) ptr_0b20: u64,
    pub(crate) unk_0b28: u64,
    pub(crate) unk_0b30: u64,
    pub(crate) unk_0b38: u32,
    pub(crate) pad_0b3c: [u8; 0x4],
    pub(crate) register_0b40: u64,
    pub(crate) ptr_0b48: u64,
    pub(crate) unk_0b50: u64,
    pub(crate) unk_0b58: u64,
    pub(crate) unk_0b60: u32,
    pub(crate) pad_0b64: [u8; 0x4],
    pub(crate) register_0b68: u64,
    pub(crate) ptr_0b70: u64,
    pub(crate) unk_0b78: u64,
    pub(crate) unk_0b80: u64,
    pub(crate) unk_0b88: u32,
    pub(crate) pad_0b8c: [u8; 0x4],
    pub(crate) register_0b90: u64,
    pub(crate) ptr_0b98: u64,
    pub(crate) unk_0ba0: u64,
    pub(crate) unk_0ba8: u64,
    pub(crate) unk_0bb0: u32,
    pub(crate) pad_0bb4: [u8; 0x24],
    pub(crate) unk_0bd8: u32,
    pub(crate) pad_0bdc: [u8; 0x4],
    pub(crate) register_0be0: u64,
    pub(crate) ptr_0be8: u64,
    pub(crate) unk_0bf0: u64,
    pub(crate) unk_0bf8: u64,
    pub(crate) unk_0c00: u32,
    pub(crate) pad_0c04: [u8; 0x4],
    pub(crate) register_0c08: u64,
    pub(crate) ptr_0c10: u64,
    pub(crate) unk_0c18: u64,
    pub(crate) unk_0c20: u64,
    pub(crate) unk_0c28: u32,
    pub(crate) pad_0c2c: [u8; 0x24],
    pub(crate) unk_0c50: u32,
    pub(crate) pad_0c54: [u8; 0x24],
    pub(crate) unk_0c78: u32,
    pub(crate) pad_0c7c: [u8; 0x4],
    pub(crate) register_0c80: u64,
    pub(crate) ptr_0c88: u64,
    pub(crate) unk_0c90: u64,
    pub(crate) unk_0c98: u64,
    pub(crate) unk_0ca0: u32,
    pub(crate) pad_0ca4: [u8; 0x24],
    pub(crate) unk_0cc8: u32,
    pub(crate) pad_0ccc: [u8; 0x24],
    pub(crate) unk_0cf0: u32,
    pub(crate) pad_0cf4: [u8; 0x24],
    pub(crate) unk_0d18: u32,
    pub(crate) pad_0d1c: [u8; 0x4],
    pub(crate) register_0d20: u64,
    pub(crate) ptr_0d28: u64,
    pub(crate) unk_0d30: u64,
    pub(crate) pad_0d38: [u8; 0x8],
    pub(crate) unk_0d40: u32,
    pub(crate) pad_0d44: [u8; 0x24],
    pub(crate) unk_0d68: u32,
    pub(crate) pad_0d6c: [u8; 0x4],
    pub(crate) register_0d70: u64,
    pub(crate) ptr_0d78: u64,
    pub(crate) unk_0d80: u64,
    pub(crate) pad_0d88: [u8; 0x8],
    pub(crate) unk_0d90: u32,
    pub(crate) pad_0d94: [u8; 0x4],
    pub(crate) register_0d98: u64,
    pub(crate) ptr_0da0: u64,
    pub(crate) unk_0da8: u64,
    pub(crate) pad_0db0: [u8; 0x8],
    pub(crate) unk_0db8: u32,
    pub(crate) pad_0dbc: [u8; 0x24],
    pub(crate) unk_0de0: u32,
    pub(crate) pad_0de4: [u8; 0x24],
    pub(crate) unk_0e08: u32,
    pub(crate) pad_0e0c: [u8; 0x4],
    pub(crate) register_0e10: u64,
    pub(crate) ptr_0e18: u64,
    pub(crate) unk_0e20: u64,
    pub(crate) unk_0e28: u64,
    pub(crate) unk_0e30: u32,
    pub(crate) pad_0e34: [u8; 0x24],
    pub(crate) unk_0e58: u32,
    pub(crate) pad_0e5c: [u8; 0x24],
    pub(crate) unk_0e80: u32,
    pub(crate) pad_0e84: [u8; 0x24],
    pub(crate) unk_0ea8: u32,
    pub(crate) pad_0eac: [u8; 0x24],
    pub(crate) unk_0ed0: u32,
    pub(crate) pad_0ed4: [u8; 0x24],
    pub(crate) unk_0ef8: u32,
    pub(crate) pad_0efc: [u8; 0x4],
    pub(crate) register_0f00: u64,
    pub(crate) ptr_0f08: u64,
    pub(crate) unk_0f10: u64,
    pub(crate) unk_0f18: u64,
    pub(crate) unk_0f20: u32,
    pub(crate) pad_0f24: [u8; 0x24],
    pub(crate) unk_0f48: u32,
    pub(crate) pad_0f4c: [u8; 0x24],
    pub(crate) unk_0f70: u32,
    pub(crate) pad_0f74: [u8; 0x24],
    pub(crate) unk_0f98: u32,
    pub(crate) pad_0f9c: [u8; 0x14],
    pub(crate) unk_0fb0: u64,
    pub(crate) pad_0fb8: [u8; 0x8],
    pub(crate) unk_0fc0: u32,
    pub(crate) pad_0fc4: [u8; 0x10],
    pub(crate) unk_0fd4: u32,
    pub(crate) unk_0fd8: u64,
    pub(crate) pad_0fe0: [u8; 0x4],
    pub(crate) unk_0fe4: u32,
    pub(crate) pad_0fe8: [u8; 0x4],
    pub(crate) unk_0fec: u32,
    pub(crate) unk_0ff0: u32,
    pub(crate) hardwaredata_unk_0ff4: u32,
    pub(crate) pad_0ff8: [u8; 0x4],
    pub(crate) unk_0ffc: u32,
    pub(crate) unk_1000: u64,
    pub(crate) pad_1008: [u8; 0x18],
    pub(crate) unk_1020: u32,
    pub(crate) pad_1024: [u8; 0x1c],
    pub(crate) unk_1040: u64,
    pub(crate) pad_1048: [u8; 0x4],
    pub(crate) unk_104c: u32,
    pub(crate) unk_1050: u64,
    pub(crate) unk_1058: u32,
    pub(crate) pad_105c: [u8; 0x48],
    pub(crate) unk_10a4: u32,
    pub(crate) unk_10a8: u64,
    pub(crate) pad_10b0: [u8; 0x24],
    pub(crate) unk_10d4: u32,
    pub(crate) unk_10d8: u64,
    pub(crate) maximum_pstate: u32,
    pub(crate) pstate_mhz_0: u32,
    pub(crate) pstate_mhz_1: u32,
    pub(crate) pstate_mhz_2: u32,
    pub(crate) pstate_mhz_3: u32,
    pub(crate) pstate_mhz_4: u32,
    pub(crate) pstate_mhz_5: u32,
    pub(crate) pstate_mhz_6: u32,
    pub(crate) pstate_mhz_7: u32,
    pub(crate) pstate_mhz_8: u32,
    pub(crate) pstate_mhz_9: u32,
    pub(crate) pstate_mhz_10: u32,
    pub(crate) pstate_mhz_11: u32,
    pub(crate) pstate_mhz_12: u32,
    pub(crate) pstate_mhz_13: u32,
    pub(crate) pstate_mhz_14: u32,
    pub(crate) pstate_mhz_15: u32,
    pub(crate) pstate_core_mv_0: u32,
    pub(crate) pad_1128: [u8; 0x3c],
    pub(crate) pstate_core_mv_1: u32,
    pub(crate) pad_1168: [u8; 0x3c],
    pub(crate) pstate_core_mv_2: u32,
    pub(crate) pad_11a8: [u8; 0x3c],
    pub(crate) pstate_core_mv_3: u32,
    pub(crate) pad_11e8: [u8; 0x3c],
    pub(crate) pstate_core_mv_4: u32,
    pub(crate) pad_1228: [u8; 0x3c],
    pub(crate) pstate_core_mv_5: u32,
    pub(crate) pad_1268: [u8; 0x3c],
    pub(crate) pstate_core_mv_6: u32,
    pub(crate) pad_12a8: [u8; 0x3c],
    pub(crate) pstate_core_mv_7: u32,
    pub(crate) pad_12e8: [u8; 0x3c],
    pub(crate) pstate_core_mv_8: u32,
    pub(crate) pad_1328: [u8; 0x3c],
    pub(crate) pstate_core_mv_9: u32,
    pub(crate) pad_1368: [u8; 0x3c],
    pub(crate) pstate_core_mv_10: u32,
    pub(crate) pad_13a8: [u8; 0x3c],
    pub(crate) pstate_core_mv_11: u32,
    pub(crate) pad_13e8: [u8; 0x3c],
    pub(crate) pstate_core_mv_12: u32,
    pub(crate) pad_1428: [u8; 0x3c],
    pub(crate) pstate_core_mv_13: u32,
    pub(crate) pad_1468: [u8; 0x3c],
    pub(crate) pstate_core_mv_14: u32,
    pub(crate) pad_14a8: [u8; 0x3c],
    pub(crate) pstate_core_mv_15: u32,
    pub(crate) pad_14e8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_0: u32,
    pub(crate) pad_1528: [u8; 0x3c],
    pub(crate) pstate_sram_mv_1: u32,
    pub(crate) pad_1568: [u8; 0x3c],
    pub(crate) pstate_sram_mv_2: u32,
    pub(crate) pad_15a8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_3: u32,
    pub(crate) pad_15e8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_4: u32,
    pub(crate) pad_1628: [u8; 0x3c],
    pub(crate) pstate_sram_mv_5: u32,
    pub(crate) pad_1668: [u8; 0x3c],
    pub(crate) pstate_sram_mv_6: u32,
    pub(crate) pad_16a8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_7: u32,
    pub(crate) pad_16e8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_8: u32,
    pub(crate) pad_1728: [u8; 0x3c],
    pub(crate) pstate_sram_mv_9: u32,
    pub(crate) pad_1768: [u8; 0x3c],
    pub(crate) pstate_sram_mv_10: u32,
    pub(crate) pad_17a8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_11: u32,
    pub(crate) pad_17e8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_12: u32,
    pub(crate) pad_1828: [u8; 0x3c],
    pub(crate) pstate_sram_mv_13: u32,
    pub(crate) pad_1868: [u8; 0x3c],
    pub(crate) pstate_sram_mv_14: u32,
    pub(crate) pad_18a8: [u8; 0x3c],
    pub(crate) pstate_sram_mv_15: u32,
    pub(crate) pad_18e8: [u8; 0x7c],
    pub(crate) unk_1964: u32,
    pub(crate) unk_1968: u64,
    pub(crate) unk_1970: u64,
    pub(crate) unk_1978: u64,
    pub(crate) unk_1980: u64,
    pub(crate) unk_1988: u64,
    pub(crate) unk_1990: u64,
    pub(crate) unk_1998: u32,
    pub(crate) pad_199c: [u8; 0x4c],
    pub(crate) hardwaredata_unk_19e8: u32,
    pub(crate) hardwaredata_unk_19ec: u32,
    pub(crate) hardwaredata_unk_19f0: u32,
    pub(crate) hardwaredata_unk_19f4: u32,
    pub(crate) hardwaredata_unk_19f8: u32,
    pub(crate) hardwaredata_unk_19fc: u32,
    pub(crate) hardwaredata_unk_1a00: u32,
    pub(crate) hardwaredata_unk_1a04: u32,
    pub(crate) hardwaredata_unk_1a08: u32,
    pub(crate) hardwaredata_unk_1a0c: u32,
    pub(crate) hardwaredata_unk_1a10: u32,
    pub(crate) hardwaredata_unk_1a14: u32,
    pub(crate) hardwaredata_unk_1a18: u32,
    pub(crate) pad_1a1c: [u8; 0xc],
    pub(crate) hardwaredata_unk_1a28: u64,
    pub(crate) hardwaredata_unk_1a30: u64,
    pub(crate) hardwaredata_unk_1a38: u64,
    pub(crate) hardwaredata_unk_1a40: u64,
    pub(crate) hardwaredata_unk_1a48: u64,
    pub(crate) hardwaredata_unk_1a50: u64,
    pub(crate) unk_1a58: u32,
    pub(crate) pad_1a5c: [u8; 0xc04],
    pub(crate) unk_2660: u64,
    pub(crate) unk_2668: u64,
    pub(crate) unk_2670: u32,
    pub(crate) pad_2674: [u8; 0x10],
    pub(crate) unk_2684: u32,
    pub(crate) pad_2688: [u8; 0x4],
    pub(crate) unk_268c: u32,
    pub(crate) pad_2690: [u8; 0x20],
    pub(crate) unk_26b0: u64,
    pub(crate) unk_26b8: u32,
    pub(crate) pad_26bc: [u8; 0x4],
    pub(crate) unk_26c0: u64,
    pub(crate) unk_26c8: u32,
    pub(crate) pad_26cc: [u8; 0x4],
    pub(crate) unk_26d0: u64,
    pub(crate) unk_26d8: u64,
    pub(crate) unk_26e0: u64,
    pub(crate) unk_26e8: u64,
    pub(crate) unk_26f0: u64,
    pub(crate) unk_26f8: u64,
    pub(crate) pad_2700: [u8; 0xc],
    pub(crate) unk_270c: u32,
    pub(crate) unk_2710: u8,
    pub(crate) unk_2711: u8,
    pub(crate) unk_2712: u8,
    pub(crate) unk_2713: u8,
    pub(crate) scheduling_pause_us: u32,
    pub(crate) pad_2718: [u8; 0x8],
    pub(crate) unk_2720: u32,
    pub(crate) pad_2724: [u8; 0x4],
    pub(crate) unk_2728: u64,
    pub(crate) unk_2730: u32,
    pub(crate) ptr_2734: u64,
    pub(crate) ptr_273c: u64,
    pub(crate) unk_2744: u32,
    pub(crate) unk_2748: u64,
    pub(crate) pad_2750: [u8; 0x4],
    pub(crate) ptr_2754: u64,
    pub(crate) ptr_275c: u64,
    pub(crate) unk_2764: u32,
    pub(crate) unk_2768: u64,
    pub(crate) pad_2770: [u8; 0x4],
    pub(crate) ptr_2774: u64,
    pub(crate) ptr_277c: u64,
    pub(crate) unk_2784: u32,
    pub(crate) unk_2788: u64,
    pub(crate) pad_2790: [u8; 0x4],
    pub(crate) ptr_2794: u64,
    pub(crate) ptr_279c: u64,
    pub(crate) unk_27a4: u32,
    pub(crate) unk_27a8: u64,
    pub(crate) pad_27b0: [u8; 0x1c],
    pub(crate) ptr_27cc: u64,
    pub(crate) pad_27d4: [u8; 0x8],
    pub(crate) unk_27dc: u32,
    pub(crate) unk_27e0: u32,
    pub(crate) pad_27e4: [u8; 0x24],
    pub(crate) unk_2808: u64,
    pub(crate) unk_2810: u64,
    pub(crate) pad_2818: [u8; 0x16c0],
    pub(crate) unk_3ed8: u32,
    pub(crate) pad_3edc: [u8; 0x14],
    pub(crate) unk_3ef0: u32,
    pub(crate) pad_3ef4: [u8; 0x1490],
    pub(crate) sample_period_5384: u32,
    pub(crate) sample_period_5388: u32,
    pub(crate) unk_538c: u32,
    pub(crate) unk_5390: u64,
    pub(crate) unk_5398: u32,
    pub(crate) pad_539c: [u8; 0x10],
    pub(crate) unk_53ac: u32,
    pub(crate) unk_53b0: u32,
    pub(crate) pad_53b4: [u8; 0xc],
    pub(crate) hardwaredata_unk_53c0: u32,
    pub(crate) unk_53c4: u32,
    pub(crate) hardwaredata_unk_53c8: u32,
    pub(crate) pad_53cc: [u8; 0x4],
    pub(crate) unk_53d0: u32,
    pub(crate) pad_53d4: [u8; 0x8],
    pub(crate) unk_53dc: u32,
    pub(crate) unk_53e0: u64,
    pub(crate) unk_53e8: u64,
    pub(crate) unk_53f0: u64,
    pub(crate) pad_53f8: [u8; 0x8],
    pub(crate) hardwaredata_unk_5400: u32,
    pub(crate) unk_5404: u32,
    pub(crate) hardwaredata_unk_5408: u32,
    pub(crate) pad_540c: [u8; 0x4],
    pub(crate) unk_5410: u32,
    pub(crate) pad_5414: [u8; 0x8],
    pub(crate) unk_541c: u32,
    pub(crate) unk_5420: u64,
    pub(crate) unk_5428: u32,
    pub(crate) pad_542c: [u8; 0x24],
    pub(crate) unk_5450: u64,
    pub(crate) unk_5458: u64,
    pub(crate) unk_5460: u64,
    pub(crate) unk_5468: u64,
    pub(crate) unk_5470: u64,
    pub(crate) unk_5478: u64,
    pub(crate) unk_5480: u64,
    pub(crate) pad_5488: [u8; 0x8c0],
    pub(crate) unk_5d48: u32,
    pub(crate) pad_5d4c: [u8; 0x8],
    pub(crate) unk_5d54: u32,
    pub(crate) pad_5d58: [u8; 0x4],
    pub(crate) unk_5d5c: u32,
    pub(crate) pad_5d60: [u8; 0x4],
    pub(crate) unk_5d64: u32,
    pub(crate) pad_5d68: [u8; 0x4],
    pub(crate) hardwaredata_unk_5d6c: u64,
    pub(crate) unk_5d74: u32,
    pub(crate) pad_5d78: [u8; 0x4],
    pub(crate) hardwaredata_unk_5d7c: u32,
    pub(crate) hardwaredata_unk_5d80: u64,
    pub(crate) hardwaredata_unk_5d88: u32,
    pub(crate) pad_5d8c: [u8; 0x8],
    pub(crate) hardwaredata_unk_5d94: u32,
    pub(crate) pad_5d98: [u8; 0x30],
    pub(crate) hardwaredata_unk_5dc8: u32,
    pub(crate) pad_5dcc: [u8; 0x34],
    pub(crate) hardwaredata_unk_5e00: u32,
    pub(crate) pad_5e04: [u8; 0x8],
    pub(crate) hardwaredata_unk_5e0c: u32,
    pub(crate) pad_5e10: [u8; 0x4],
    pub(crate) hardwaredata_unk_5e14: u32,
    pub(crate) pad_5e18: [u8; 0x4],
    pub(crate) hardwaredata_unk_5e1c: u32,
    pub(crate) pad_5e20: [u8; 0x4],
    pub(crate) unk_5e24: u32,
    pub(crate) unk_5e28: u64,
    pub(crate) pad_5e30: [u8; 0x8],
    pub(crate) hardwaredata_unk_5e38: u64,
    pub(crate) pad_5e40: [u8; 0xc],
    pub(crate) hardwaredata_unk_5e4c: u32,
    pub(crate) pad_5e50: [u8; 0x8],
    pub(crate) unk_5e58: u32,
    pub(crate) pad_5e5c: [u8; 0x4],
    pub(crate) hardwaredata_unk_5e60: u64,
    pub(crate) pad_5e68: [u8; 0x50],
    pub(crate) unk_5eb8: u32,
    pub(crate) pad_5ebc: [u8; 0x4],
    pub(crate) hardwaredata_unk_5ec0: u64,
    pub(crate) hardwaredata_unk_5ec8: u64,
    pub(crate) unk_5ed0: u64,
    pub(crate) pad_5ed8: [u8; 0x4],
    pub(crate) unk_5edc: u32,
    pub(crate) unk_5ee0: u64,
    pub(crate) unk_5ee8: u64,
    pub(crate) unk_5ef0: u64,
    pub(crate) unk_5ef8: u64,
    pub(crate) unk_5f00: u32,
    pub(crate) hardwaredata_unk_5f04: u32,
    pub(crate) hardwaredata_unk_5f08: u64,
    pub(crate) hardwaredata_unk_5f10: u32,
    pub(crate) pad_5f14: [u8; 0x8],
    pub(crate) unk_5f1c: u32,
    pub(crate) pad_5f20: [u8; 0x4],
    pub(crate) unk_5f24: u32,
    pub(crate) pad_5f28: [u8; 0x28],
    pub(crate) hardwaredata_unk_5f50: u64,
    pub(crate) pad_5f58: [u8; 0x18],
    pub(crate) unk_5f70: u64,
    pub(crate) unk_5f78: u64,
    pub(crate) unk_5f80: u64,
    pub(crate) unk_5f88: u64,
    pub(crate) unk_5f90: u64,
    pub(crate) unk_5f98: u64,
    pub(crate) pad_5fa0: [u8; 0x54],
    pub(crate) unk_5ff4: u8,
    pub(crate) pad_5ff5: [u8; 0x1032],
    pub(crate) unk_7027: u8,
    pub(crate) unk_7028: u64,
    pub(crate) unk_7030: u64,
    pub(crate) unk_7038: u64,
    pub(crate) unk_7040: u64,
    pub(crate) unk_7048: u64,
    pub(crate) unk_7050: u64,
    pub(crate) unk_7058: u64,
    pub(crate) unk_7060: u64,
    pub(crate) unk_7068: u64,
    pub(crate) unk_7070: u64,
    pub(crate) unk_7078: u64,
    pub(crate) unk_7080: u64,
    pub(crate) unk_7088: u64,
    pub(crate) unk_7090: u64,
    pub(crate) unk_7098: u64,
    pub(crate) unk_70a0: u64,
    pub(crate) unk_70a8: u64,
    pub(crate) unk_70b0: u64,
    pub(crate) unk_70b8: u64,
    pub(crate) unk_70c0: u64,
    pub(crate) unk_70c8: u64,
    pub(crate) unk_70d0: u64,
    pub(crate) unk_70d8: u64,
    pub(crate) unk_70e0: u64,
    pub(crate) unk_70e8: u64,
    pub(crate) unk_70f0: u64,
    pub(crate) unk_70f8: u64,
    pub(crate) unk_7100: u64,
    pub(crate) unk_7108: u64,
    pub(crate) unk_7110: u64,
    pub(crate) unk_7118: u64,
    pub(crate) unk_7120: u64,
    pub(crate) unk_7128: u64,
    pub(crate) unk_7130: u64,
    pub(crate) unk_7138: u64,
    pub(crate) unk_7140: u64,
    pub(crate) unk_7148: u64,
    pub(crate) unk_7150: u64,
    pub(crate) unk_7158: u64,
    pub(crate) unk_7160: u64,
    pub(crate) unk_7168: u64,
    pub(crate) unk_7170: u64,
    pub(crate) unk_7178: u64,
    pub(crate) unk_7180: u64,
    pub(crate) unk_7188: u64,
    pub(crate) unk_7190: u64,
    pub(crate) unk_7198: u64,
    pub(crate) unk_71a0: u64,
    pub(crate) unk_71a8: u64,
    pub(crate) unk_71b0: u64,
    pub(crate) unk_71b8: u64,
    pub(crate) unk_71c0: u64,
    pub(crate) unk_71c8: u64,
    pub(crate) unk_71d0: u64,
    pub(crate) unk_71d8: u64,
    pub(crate) unk_71e0: u64,
    pub(crate) unk_71e8: u64,
    pub(crate) unk_71f0: u64,
    pub(crate) unk_71f8: u64,
    pub(crate) unk_7200: u64,
    pub(crate) unk_7208: u64,
    pub(crate) unk_7210: u64,
    pub(crate) unk_7218: u64,
    pub(crate) unk_7220: u64,
    pub(crate) unk_7228: u64,
    pub(crate) unk_7230: u64,
    pub(crate) unk_7238: u64,
    pub(crate) unk_7240: u64,
    pub(crate) unk_7248: u64,
    pub(crate) unk_7250: u64,
    pub(crate) unk_7258: u64,
    pub(crate) unk_7260: u64,
    pub(crate) unk_7268: u64,
    pub(crate) unk_7270: u64,
    pub(crate) unk_7278: u64,
    pub(crate) unk_7280: u64,
    pub(crate) unk_7288: u64,
    pub(crate) unk_7290: u64,
    pub(crate) unk_7298: u64,
    pub(crate) unk_72a0: u64,
    pub(crate) unk_72a8: u64,
    pub(crate) unk_72b0: u64,
    pub(crate) unk_72b8: u64,
    pub(crate) unk_72c0: u64,
    pub(crate) unk_72c8: u64,
    pub(crate) unk_72d0: u64,
    pub(crate) unk_72d8: u64,
    pub(crate) unk_72e0: u64,
    pub(crate) unk_72e8: u64,
    pub(crate) unk_72f0: u64,
    pub(crate) unk_72f8: u64,
    pub(crate) unk_7300: u64,
    pub(crate) unk_7308: u64,
    pub(crate) unk_7310: u64,
    pub(crate) unk_7318: u64,
    pub(crate) unk_7320: u64,
    pub(crate) unk_7328: u64,
    pub(crate) unk_7330: u64,
    pub(crate) unk_7338: u64,
    pub(crate) unk_7340: u64,
    pub(crate) unk_7348: u64,
    pub(crate) unk_7350: u64,
    pub(crate) unk_7358: u64,
    pub(crate) unk_7360: u64,
    pub(crate) unk_7368: u64,
    pub(crate) unk_7370: u64,
    pub(crate) unk_7378: u64,
    pub(crate) unk_7380: u64,
    pub(crate) unk_7388: u64,
    pub(crate) unk_7390: u64,
    pub(crate) unk_7398: u64,
    pub(crate) unk_73a0: u64,
    pub(crate) unk_73a8: u64,
    pub(crate) unk_73b0: u64,
    pub(crate) unk_73b8: u64,
    pub(crate) unk_73c0: u64,
    pub(crate) unk_73c8: u64,
    pub(crate) unk_73d0: u64,
    pub(crate) unk_73d8: u64,
    pub(crate) unk_73e0: u64,
    pub(crate) unk_73e8: u64,
    pub(crate) unk_73f0: u64,
    pub(crate) unk_73f8: u64,
    pub(crate) unk_7400: u64,
    pub(crate) unk_7408: u64,
    pub(crate) unk_7410: u64,
    pub(crate) unk_7418: u64,
    pub(crate) unk_7420: u32,
    pub(crate) unk_7424: u16,
    pub(crate) unk_7426: u8,
    pub(crate) pad_7427: [u8; 0x201],
    pub(crate) effort_normalization: u32,
    pub(crate) effort_floor_min: u32,
    pub(crate) effort_floor_max: u32,
    pub(crate) effort_max_frequency_mhz: u32,
    pub(crate) pad_7638: [u8; 0x10],
    pub(crate) hardwaredata_unk_7648: u32,
    pub(crate) pad_764c: [u8; 0x4],
    pub(crate) unk_7650: u32,
    pub(crate) hardwaredata_unk_7654: u32,
    pub(crate) pad_7658: [u8; 0x4],
    pub(crate) unk_765c: u32,
    pub(crate) unk_7660: u32,
    pub(crate) pad_7664: [u8; 0x3c],
    pub(crate) unk_76a0: u64,
    pub(crate) unk_76a8: u32,
    pub(crate) pad_76ac: [u8; 0x4],
    pub(crate) unk_76b0: u32,
    pub(crate) pad_76b4: [u8; 0x10],
    pub(crate) unk_76c4: u32,
    pub(crate) pad_76c8: [u8; 0x4],
    pub(crate) unk_76cc: u32,
    pub(crate) pad_76d0: [u8; 0x4],
    pub(crate) unk_76d4: u32,
    pub(crate) unk_76d8: u64,
    pub(crate) pad_76e0: [u8; 0x8],
    pub(crate) hardwaredata_unk_76e8: u64,
    pub(crate) hardwaredata_unk_76f0: u32,
    pub(crate) pad_76f4: [u8; 0x8],
    pub(crate) unk_76fc: u32,
    pub(crate) pad_7700: [u8; 0x30],
    pub(crate) hardwaredata_unk_7730: u32,
    pub(crate) pad_7734: [u8; 0x214],
    pub(crate) unk_7948: u64,
    pub(crate) pad_7950: [u8; 0x212],
    pub(crate) unk_7b62: u16,
    pub(crate) unk_7b64: u16,
    pub(crate) pad_7b66: [u8; 0x4],
    pub(crate) unk_7b6a: u16,
    pub(crate) unk_7b6c: u16,
    pub(crate) pad_7b6e: [u8; 0x4],
    pub(crate) unk_7b72: u16,
    pub(crate) unk_7b74: u16,
    pub(crate) pad_7b76: [u8; 0x4],
    pub(crate) unk_7b7a: u16,
    pub(crate) unk_7b7c: u32,
    pub(crate) unk_7b80: u32,
    pub(crate) unk_7b84: u16,
    pub(crate) pad_7b86: [u8; 0x4],
    pub(crate) unk_7b8a: u16,
    pub(crate) unk_7b8c: u32,
    pub(crate) unk_7b90: u16,
    pub(crate) hardwaredata_unk_7b92: u32,
    pub(crate) unk_7b96: u16,
    pub(crate) unk_7b98: u16,
    pub(crate) pad_7b9a: [u8; 0x4],
    pub(crate) unk_7b9e: u16,
    pub(crate) unk_7ba0: u32,
    pub(crate) unk_7ba4: u16,
    pub(crate) pad_7ba6: [u8; 0x30],
    pub(crate) hardwaredata_unk_7bd6: u32,
    pub(crate) pad_7bda: [u8; 0x18],
    pub(crate) unk_7bf2: u16,
    pub(crate) unk_7bf4: u16,
    pub(crate) pad_7bf6: [u8; 0xb2],
    pub(crate) hardwaredata_unk_7ca8: u32,
    pub(crate) pad_7cac: [u8; 0x10],
    pub(crate) unk_7cbc: u32,
    pub(crate) pad_7cc0: [u8; 0x48],
    pub(crate) unk_7d08: u8,
    pub(crate) pad_7d09: [u8; 0x1],
    pub(crate) unk_7d0a: u8,
    pub(crate) pad_7d0b: [u8; 0x3],
    pub(crate) unk_7d0e: u16,
    pub(crate) unk_7d10: u16,
    pub(crate) pad_7d12: [u8; 0x14],
    pub(crate) unk_7d26: u16,
    pub(crate) unk_7d28: u16,
    pub(crate) unk_7d2a: u8,
    pub(crate) pad_7d2b: [u8; 0x3],
    pub(crate) unk_7d2e: u16,
    pub(crate) unk_7d30: u16,
    pub(crate) pad_7d32: [u8; 0x14],
    pub(crate) unk_7d46: u16,
    pub(crate) unk_7d48: u16,
    pub(crate) unk_7d4a: u8,
    pub(crate) pad_7d4b: [u8; 0x3],
    pub(crate) unk_7d4e: u16,
    pub(crate) unk_7d50: u16,
    pub(crate) pad_7d52: [u8; 0x14],
    pub(crate) unk_7d66: u16,
    pub(crate) unk_7d68: u16,
    pub(crate) pad_7d6a: [u8; 0xa8],
    pub(crate) unk_7e12: u16,
    pub(crate) unk_7e14: u16,
    pub(crate) pad_7e16: [u8; 0x2],
    pub(crate) hardwaredata_unk_7e18: u32,
    pub(crate) pad_7e1c: [u8; 0x4],
    pub(crate) hardwaredata_unk_7e20: u32,
    pub(crate) hardwaredata_unk_7e24: u32,
    pub(crate) hardwaredata_unk_7e28: u32,
    pub(crate) pad_7e2c: [u8; 0x8],
    pub(crate) unk_7e34: u32,
    pub(crate) unk_7e38: u64,
    pub(crate) hardwaredata_unk_7e40: u32,
    pub(crate) hardwaredata_unk_7e44: u64,
    pub(crate) pad_7e4c: [u8; 0xc8],
    pub(crate) hardwaredata_unk_7f14: u32,
    pub(crate) pad_7f18: [u8; 0x8],
    pub(crate) hardwaredata_unk_7f20: u32,
    pub(crate) pad_7f24: [u8; 0x4],
    pub(crate) hardwaredata_unk_7f28: u32,
    pub(crate) pad_7f2c: [u8; 0x4],
    pub(crate) hardwaredata_unk_7f30: u32,
    pub(crate) pad_7f34: [u8; 0x4],
    pub(crate) unk_7f38: u64,
    pub(crate) unk_7f40: u32,
    pub(crate) pad_7f44: [u8; 0x8],
    pub(crate) hardwaredata_unk_7f4c: u64,
    pub(crate) hardwaredata_unk_7f54: u32,
    pub(crate) pad_7f58: [u8; 0x4],
    pub(crate) hardwaredata_unk_7f5c: u32,
    pub(crate) hardwaredata_unk_7f60: u32,
    pub(crate) pad_7f64: [u8; 0x8],
    pub(crate) hardwaredata_unk_7f6c: u32,
    pub(crate) pad_7f70: [u8; 0x4],
    pub(crate) hardwaredata_unk_7f74: u64,
    pub(crate) pad_7f7c: [u8; 0x18],
    pub(crate) hardwaredata_unk_7f94: u32,
    pub(crate) pad_7f98: [u8; 0x3c],
    pub(crate) unk_7fd4: u32,
    pub(crate) pad_7fd8: [u8; 0xcc],
    pub(crate) unk_80a4: u32,
    pub(crate) unk_80a8: u32,
    pub(crate) pad_80ac: [u8; 0x4],
    pub(crate) unk_80b0: u64,
    pub(crate) unk_80b8: u64,
    pub(crate) hardwaredata_unk_80c0: u32,
    pub(crate) hardwaredata_unk_80c4: u32,
    pub(crate) unk_80c8: u64,
    pub(crate) unk_80d0: u64,
    pub(crate) pad_80d8: [u8; 0x4],
    pub(crate) hardwaredata_unk_80dc: u64,
    pub(crate) hardwaredata_unk_80e4: u32,
    pub(crate) pad_80e8: [u8; 0x4],
    pub(crate) hardwaredata_unk_80ec: u32,
    pub(crate) unk_80f0: u32,
    pub(crate) pad_80f4: [u8; 0x4],
    pub(crate) unk_80f8: u32,
    pub(crate) hardwaredata_unk_80fc: u32,
    pub(crate) hardwaredata_unk_8100: u32,
    pub(crate) hardwaredata_unk_8104: u64,
    pub(crate) hardwaredata_unk_810c: u64,
    pub(crate) pad_8114: [u8; 0x4],
    pub(crate) unk_8118: u32,
    pub(crate) pad_811c: [u8; 0x24],
    pub(crate) hardwaredata_unk_8140: u32,
    pub(crate) hardwaredata_unk_8144: u32,
    pub(crate) unk_8148: u64,
    pub(crate) unk_8150: u64,
    pub(crate) unk_8158: u32,
    pub(crate) pad_815c: [u8; 0x2c],
    pub(crate) unk_8188: u64,
    pub(crate) pad_8190: [u8; 0x8],
    pub(crate) unk_8198: u32,
    pub(crate) hardwaredata_unk_819c: u32,
    pub(crate) pad_81a0: [u8; 0xc8],
    pub(crate) unk_8268: u64,
    pub(crate) pad_8270: [u8; 0x18],
    pub(crate) unk_8288: u64,
    pub(crate) unk_8290: u64,
    pub(crate) pad_8298: [u8; 0x9c],
    pub(crate) hardwaredata_unk_8334: u32,
    pub(crate) hardwaredata_unk_8338: u32,
    pub(crate) unk_833c: u32,
    pub(crate) unk_8340: u32,
    pub(crate) pad_8344: [u8; 0x4],
    pub(crate) unk_8348: u32,
    pub(crate) pad_834c: [u8; 0x18],
    pub(crate) unk_8364: u32,
    pub(crate) pad_8368: [u8; 0x8],
    pub(crate) unk_8370: u32,
    pub(crate) pad_8374: [u8; 0x4],
    pub(crate) unk_8378: u32,
    pub(crate) pad_837c: [u8; 0x4],
    pub(crate) unk_8380: u32,
    pub(crate) pad_8384: [u8; 0x4],
    pub(crate) unk_8388: u64,
    pub(crate) unk_8390: u32,
    pub(crate) pad_8394: [u8; 0x8],
    pub(crate) hardwaredata_unk_839c: u32,
    pub(crate) hardwaredata_unk_83a0: u32,
    pub(crate) hardwaredata_unk_83a4: u32,
    pub(crate) pad_83a8: [u8; 0x8],
    pub(crate) hardwaredata_unk_83b0: u32,
    pub(crate) pad_83b4: [u8; 0x30],
    pub(crate) hardwaredata_unk_83e4: u32,
    pub(crate) pad_83e8: [u8; 0x48],
    pub(crate) hardwaredata_unk_8430: u32,
    pub(crate) pad_8434: [u8; 0x14],
    pub(crate) unk_8448: u64,
    pub(crate) pad_8450: [u8; 0xc60],
    pub(crate) ptr_90b0: u64,
    pub(crate) pad_90b8: [u8; 0x200c],
    pub(crate) unk_b0c4: u32,
    pub(crate) unk_b0c8: u32,
    pub(crate) pad_b0cc: [u8; 0x78c],
    pub(crate) ptr_b858: u64,
    pub(crate) unk_b860: u32,
    pub(crate) pad_b864: [u8; 0x7c],
    pub(crate) unk_b8e0: u64,
    pub(crate) pad_b8e8: [u8; 0x8],
    pub(crate) unk_b8f0: u32,
    pub(crate) pad_b8f4: [u8; 0x6a0],
    pub(crate) unk_bf94: u32,
    pub(crate) unk_bf98: u32,
    pub(crate) pad_bf9c: [u8; 0x44],
    pub(crate) unk_bfe0: u32,
    pub(crate) pad_bfe4: [u8; 0x8],
    pub(crate) hardwaredata_unk_bfec: u32,
    pub(crate) pad_bff0: [u8; 0x3c],
    pub(crate) hardwaredata_unk_c02c: u32,
    pub(crate) pad_c030: [u8; 0x3c],
    pub(crate) hardwaredata_unk_c06c: u32,
    pub(crate) pad_c070: [u8; 0xa8],
    pub(crate) unk_c118: u64,
    pub(crate) pad_c120: [u8; 0x12d20],
    pub(crate) ptr_1ee40: u64,
    pub(crate) ptr_1ee48: u64,
    pub(crate) ptr_1ee50: u64,
    pub(crate) pad_1ee58: [u8; 0x8],
    pub(crate) ptr_1ee60: u64,
    pub(crate) ptr_1ee68: u64,
    pub(crate) ptr_1ee70: u64,
    pub(crate) ptr_1ee78: u64,
    pub(crate) ptr_1ee80: u64,
    pub(crate) ptr_1ee88: u64,
    pub(crate) ptr_1ee90: u64,
    pub(crate) ptr_1ee98: u64,
    pub(crate) ptr_1eea0: u64,
    pub(crate) ptr_1eea8: u64,
    pub(crate) ptr_1eeb0: u64,
    pub(crate) ptr_1eeb8: u64,
    pub(crate) ptr_1eec0: u64,
    pub(crate) ptr_1eec8: u64,
    pub(crate) ptr_1eed0: u64,
    pub(crate) ptr_1eed8: u64,
    pub(crate) ptr_1eee0: u64,
    pub(crate) ptr_1eee8: u64,
    pub(crate) ptr_1eef0: u64,
    pub(crate) ptr_1eef8: u64,
    pub(crate) ptr_1ef00: u64,
    pub(crate) ptr_1ef08: u64,
    pub(crate) ptr_1ef10: u64,
    pub(crate) ptr_1ef18: u64,
    pub(crate) ptr_1ef20: u64,
    pub(crate) ptr_1ef28: u64,
    pub(crate) ptr_1ef30: u64,
    pub(crate) ptr_1ef38: u64,
    pub(crate) ptr_1ef40: u64,
    pub(crate) ptr_1ef48: u64,
    pub(crate) ptr_1ef50: u64,
    pub(crate) ptr_1ef58: u64,
    pub(crate) ptr_1ef60: u64,
    pub(crate) ptr_1ef68: u64,
    pub(crate) ptr_1ef70: u64,
    pub(crate) ptr_1ef78: u64,
    pub(crate) ptr_1ef80: u64,
    pub(crate) ptr_1ef88: u64,
    pub(crate) ptr_1ef90: u64,
    pub(crate) ptr_1ef98: u64,
    pub(crate) ptr_1efa0: u64,
    pub(crate) ptr_1efa8: u64,
    pub(crate) ptr_1efb0: u64,
    pub(crate) ptr_1efb8: u64,
    pub(crate) ptr_1efc0: u64,
    pub(crate) ptr_1efc8: u64,
    pub(crate) ptr_1efd0: u64,
    pub(crate) ptr_1efd8: u64,
    pub(crate) ptr_1efe0: u64,
    pub(crate) ptr_1efe8: u64,
    pub(crate) ptr_1eff0: u64,
    pub(crate) ptr_1eff8: u64,
    pub(crate) ptr_1f000: u64,
    pub(crate) ptr_1f008: u64,
    pub(crate) ptr_1f010: u64,
    pub(crate) ptr_1f018: u64,
    pub(crate) ptr_1f020: u64,
    pub(crate) ptr_1f028: u64,
    pub(crate) ptr_1f030: u64,
    pub(crate) ptr_1f038: u64,
    pub(crate) ptr_1f040: u64,
    pub(crate) pad_1f048: [u8; 0x4c],
    pub(crate) ptr_1f094: u64,
    pub(crate) ptr_1f09c: u64,
    pub(crate) ptr_1f0a4: u64,
    pub(crate) ptr_1f0ac: u64,
    pub(crate) ptr_1f0b4: u64,
    pub(crate) pad_1f0bc: [u8; 0x6c],
    pub(crate) ptr_1f128: u64,
    pub(crate) ptr_1f130: u64,
    pub(crate) pad_1f138: [u8; 0xe8],
    pub(crate) unk_1f220: u8,
    pub(crate) pad_1f221: [u8; 0xdf],
    pub(crate) unk_1f300: u64,
    pub(crate) pad_1f308: [u8; 0x3ff8],
    pub(crate) ptr_23300: u64,
    pub(crate) ptr_23308: u64,
    pub(crate) ptr_23310: u64,
    pub(crate) pad_23318: [u8; 0x188],
    pub(crate) ptr_234a0: u64,
    pub(crate) ptr_234a8: u64,
    pub(crate) ptr_234b0: u64,
    pub(crate) ptr_234b8: u64,
    pub(crate) ptr_234c0: u64,
    pub(crate) ptr_234c8: u64,
    pub(crate) ptr_234d0: u64,
    pub(crate) ptr_234d8: u64,
    pub(crate) ptr_234e0: u64,
    pub(crate) ptr_234e8: u64,
    pub(crate) ptr_234f0: u64,
    pub(crate) ptr_234f8: u64,
    pub(crate) ptr_23500: u64,
    pub(crate) pad_23508: [u8; 0xe0],
    pub(crate) ptr_235e8: u64,
    pub(crate) ptr_235f0: u64,
    pub(crate) pad_235f8: [u8; 0x189],
    pub(crate) ptr_23781: u64,
    pub(crate) pad_23789: [u8; 0x37],
    pub(crate) unk_237c0: u64,
    pub(crate) pad_237c8: [u8; 0x4838],
}
static_assert!(core::mem::size_of::<HardwareData>() == 0x28000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for HardwareData {}

pub(crate) struct HardwareDataConstants {
    pub(crate) unk_0000: u64,
    pub(crate) unk_0008: u64,
    pub(crate) unk_0010: u64,
    pub(crate) unk_0018: u64,
    pub(crate) unk_0020: u64,
    pub(crate) unk_0028: u64,
    pub(crate) unk_0660: u32,
    pub(crate) unk_0688: u32,
    pub(crate) unk_06b0: u32,
    pub(crate) unk_06d8: u32,
    pub(crate) unk_0700: u32,
    pub(crate) unk_0728: u32,
    pub(crate) unk_0750: u32,
    pub(crate) unk_0778: u32,
    pub(crate) unk_07a0: u32,
    pub(crate) unk_07a8: u64,
    pub(crate) unk_07b8: u64,
    pub(crate) unk_07c8: u32,
    pub(crate) unk_07d0: u64,
    pub(crate) unk_07e0: u64,
    pub(crate) unk_07f0: u32,
    pub(crate) unk_0818: u32,
    pub(crate) unk_0840: u32,
    pub(crate) unk_0868: u32,
    pub(crate) unk_0890: u32,
    pub(crate) unk_08b8: u32,
    pub(crate) unk_08e0: u32,
    pub(crate) unk_08f8: u64,
    pub(crate) unk_0908: u32,
    pub(crate) unk_0930: u32,
    pub(crate) unk_0958: u32,
    pub(crate) unk_0980: u32,
    pub(crate) unk_09a8: u32,
    pub(crate) unk_09c0: u64,
    pub(crate) unk_09d0: u32,
    pub(crate) unk_09f8: u32,
    pub(crate) unk_0a10: u64,
    pub(crate) unk_0a20: u32,
    pub(crate) unk_0a48: u32,
    pub(crate) unk_0a70: u32,
    pub(crate) unk_0a98: u32,
    pub(crate) unk_0ac0: u32,
    pub(crate) unk_0ae8: u32,
    pub(crate) unk_0b10: u32,
    pub(crate) unk_0b28: u64,
    pub(crate) unk_0b30: u64,
    pub(crate) unk_0b38: u32,
    pub(crate) unk_0b50: u64,
    pub(crate) unk_0b58: u64,
    pub(crate) unk_0b60: u32,
    pub(crate) unk_0b78: u64,
    pub(crate) unk_0b80: u64,
    pub(crate) unk_0b88: u32,
    pub(crate) unk_0ba0: u64,
    pub(crate) unk_0ba8: u64,
    pub(crate) unk_0bb0: u32,
    pub(crate) unk_0bd8: u32,
    pub(crate) unk_0bf0: u64,
    pub(crate) unk_0bf8: u64,
    pub(crate) unk_0c00: u32,
    pub(crate) unk_0c18: u64,
    pub(crate) unk_0c20: u64,
    pub(crate) unk_0c28: u32,
    pub(crate) unk_0c50: u32,
    pub(crate) unk_0c78: u32,
    pub(crate) unk_0c90: u64,
    pub(crate) unk_0c98: u64,
    pub(crate) unk_0ca0: u32,
    pub(crate) unk_0cc8: u32,
    pub(crate) unk_0cf0: u32,
    pub(crate) unk_0d18: u32,
    pub(crate) unk_0d30: u64,
    pub(crate) unk_0d40: u32,
    pub(crate) unk_0d68: u32,
    pub(crate) unk_0d80: u64,
    pub(crate) unk_0d90: u32,
    pub(crate) unk_0da8: u64,
    pub(crate) unk_0db8: u32,
    pub(crate) unk_0de0: u32,
    pub(crate) unk_0e08: u32,
    pub(crate) unk_0e20: u64,
    pub(crate) unk_0e28: u64,
    pub(crate) unk_0e30: u32,
    pub(crate) unk_0e58: u32,
    pub(crate) unk_0e80: u32,
    pub(crate) unk_0ea8: u32,
    pub(crate) unk_0ed0: u32,
    pub(crate) unk_0ef8: u32,
    pub(crate) unk_0f10: u64,
    pub(crate) unk_0f18: u64,
    pub(crate) unk_0f20: u32,
    pub(crate) unk_0f48: u32,
    pub(crate) unk_0f70: u32,
    pub(crate) unk_0f98: u32,
    pub(crate) unk_0fb0: u64,
    pub(crate) unk_0fc0: u32,
    pub(crate) unk_0fd4: u32,
    pub(crate) unk_0fd8: u64,
    pub(crate) unk_0fe4: u32,
    pub(crate) unk_0fec: u32,
    pub(crate) unk_0ff0: u32,
    pub(crate) unk_0ffc: u32,
    pub(crate) unk_1000: u64,
    pub(crate) unk_1020: u32,
    pub(crate) unk_1040: u64,
    pub(crate) unk_104c: u32,
    pub(crate) unk_1050: u64,
    pub(crate) unk_1058: u32,
    pub(crate) unk_10a4: u32,
    pub(crate) unk_10a8: u64,
    pub(crate) unk_10d4: u32,
    pub(crate) unk_10d8: u64,
    pub(crate) unk_1964: u32,
    pub(crate) unk_1968: u64,
    pub(crate) unk_1970: u64,
    pub(crate) unk_1978: u64,
    pub(crate) unk_1980: u64,
    pub(crate) unk_1988: u64,
    pub(crate) unk_1990: u64,
    pub(crate) unk_1998: u32,
    pub(crate) unk_1a58: u32,
    pub(crate) unk_2660: u64,
    pub(crate) unk_2668: u64,
    pub(crate) unk_2670: u32,
    pub(crate) unk_2684: u32,
    pub(crate) unk_268c: u32,
    pub(crate) unk_26b0: u64,
    pub(crate) unk_26b8: u32,
    pub(crate) unk_26c0: u64,
    pub(crate) unk_26c8: u32,
    pub(crate) unk_26d0: u64,
    pub(crate) unk_26d8: u64,
    pub(crate) unk_26e0: u64,
    pub(crate) unk_26e8: u64,
    pub(crate) unk_26f0: u64,
    pub(crate) unk_26f8: u64,
    pub(crate) unk_270c: u32,
    pub(crate) unk_2710: u8,
    pub(crate) unk_2711: u8,
    pub(crate) unk_2712: u8,
    pub(crate) unk_2713: u8,
    pub(crate) scheduling_pause_us: u32,
    pub(crate) unk_2720: u32,
    pub(crate) unk_2728: u64,
    pub(crate) unk_2730: u32,
    pub(crate) unk_2744: u32,
    pub(crate) unk_2748: u64,
    pub(crate) unk_2764: u32,
    pub(crate) unk_2768: u64,
    pub(crate) unk_2784: u32,
    pub(crate) unk_2788: u64,
    pub(crate) unk_27a4: u32,
    pub(crate) unk_27a8: u64,
    pub(crate) unk_27dc: u32,
    pub(crate) unk_27e0: u32,
    pub(crate) unk_2808: u64,
    pub(crate) unk_2810: u64,
    pub(crate) unk_3ed8: u32,
    pub(crate) unk_3ef0: u32,
    pub(crate) unk_538c: u32,
    pub(crate) unk_5390: u64,
    pub(crate) unk_5398: u32,
    pub(crate) unk_53ac: u32,
    pub(crate) unk_53b0: u32,
    pub(crate) unk_53c4: u32,
    pub(crate) unk_53d0: u32,
    pub(crate) unk_53dc: u32,
    pub(crate) unk_53e0: u64,
    pub(crate) unk_53e8: u64,
    pub(crate) unk_53f0: u64,
    pub(crate) unk_5404: u32,
    pub(crate) unk_5410: u32,
    pub(crate) unk_541c: u32,
    pub(crate) unk_5420: u64,
    pub(crate) unk_5428: u32,
    pub(crate) unk_5450: u64,
    pub(crate) unk_5458: u64,
    pub(crate) unk_5460: u64,
    pub(crate) unk_5468: u64,
    pub(crate) unk_5470: u64,
    pub(crate) unk_5478: u64,
    pub(crate) unk_5480: u64,
    pub(crate) unk_5d48: u32,
    pub(crate) unk_5d54: u32,
    pub(crate) unk_5d5c: u32,
    pub(crate) unk_5d64: u32,
    pub(crate) unk_5d74: u32,
    pub(crate) unk_5e24: u32,
    pub(crate) unk_5e28: u64,
    pub(crate) unk_5e58: u32,
    pub(crate) unk_5eb8: u32,
    pub(crate) unk_5ed0: u64,
    pub(crate) unk_5edc: u32,
    pub(crate) unk_5ee0: u64,
    pub(crate) unk_5ee8: u64,
    pub(crate) unk_5ef0: u64,
    pub(crate) unk_5ef8: u64,
    pub(crate) unk_5f00: u32,
    pub(crate) unk_5f1c: u32,
    pub(crate) unk_5f24: u32,
    pub(crate) unk_5f70: u64,
    pub(crate) unk_5f78: u64,
    pub(crate) unk_5f80: u64,
    pub(crate) unk_5f88: u64,
    pub(crate) unk_5f90: u64,
    pub(crate) unk_5f98: u64,
    pub(crate) unk_5ff4: u8,
    pub(crate) unk_7027: u8,
    pub(crate) unk_7028: u64,
    pub(crate) unk_7030: u64,
    pub(crate) unk_7038: u64,
    pub(crate) unk_7040: u64,
    pub(crate) unk_7048: u64,
    pub(crate) unk_7050: u64,
    pub(crate) unk_7058: u64,
    pub(crate) unk_7060: u64,
    pub(crate) unk_7068: u64,
    pub(crate) unk_7070: u64,
    pub(crate) unk_7078: u64,
    pub(crate) unk_7080: u64,
    pub(crate) unk_7088: u64,
    pub(crate) unk_7090: u64,
    pub(crate) unk_7098: u64,
    pub(crate) unk_70a0: u64,
    pub(crate) unk_70a8: u64,
    pub(crate) unk_70b0: u64,
    pub(crate) unk_70b8: u64,
    pub(crate) unk_70c0: u64,
    pub(crate) unk_70c8: u64,
    pub(crate) unk_70d0: u64,
    pub(crate) unk_70d8: u64,
    pub(crate) unk_70e0: u64,
    pub(crate) unk_70e8: u64,
    pub(crate) unk_70f0: u64,
    pub(crate) unk_70f8: u64,
    pub(crate) unk_7100: u64,
    pub(crate) unk_7108: u64,
    pub(crate) unk_7110: u64,
    pub(crate) unk_7118: u64,
    pub(crate) unk_7120: u64,
    pub(crate) unk_7128: u64,
    pub(crate) unk_7130: u64,
    pub(crate) unk_7138: u64,
    pub(crate) unk_7140: u64,
    pub(crate) unk_7148: u64,
    pub(crate) unk_7150: u64,
    pub(crate) unk_7158: u64,
    pub(crate) unk_7160: u64,
    pub(crate) unk_7168: u64,
    pub(crate) unk_7170: u64,
    pub(crate) unk_7178: u64,
    pub(crate) unk_7180: u64,
    pub(crate) unk_7188: u64,
    pub(crate) unk_7190: u64,
    pub(crate) unk_7198: u64,
    pub(crate) unk_71a0: u64,
    pub(crate) unk_71a8: u64,
    pub(crate) unk_71b0: u64,
    pub(crate) unk_71b8: u64,
    pub(crate) unk_71c0: u64,
    pub(crate) unk_71c8: u64,
    pub(crate) unk_71d0: u64,
    pub(crate) unk_71d8: u64,
    pub(crate) unk_71e0: u64,
    pub(crate) unk_71e8: u64,
    pub(crate) unk_71f0: u64,
    pub(crate) unk_71f8: u64,
    pub(crate) unk_7200: u64,
    pub(crate) unk_7208: u64,
    pub(crate) unk_7210: u64,
    pub(crate) unk_7218: u64,
    pub(crate) unk_7220: u64,
    pub(crate) unk_7228: u64,
    pub(crate) unk_7230: u64,
    pub(crate) unk_7238: u64,
    pub(crate) unk_7240: u64,
    pub(crate) unk_7248: u64,
    pub(crate) unk_7250: u64,
    pub(crate) unk_7258: u64,
    pub(crate) unk_7260: u64,
    pub(crate) unk_7268: u64,
    pub(crate) unk_7270: u64,
    pub(crate) unk_7278: u64,
    pub(crate) unk_7280: u64,
    pub(crate) unk_7288: u64,
    pub(crate) unk_7290: u64,
    pub(crate) unk_7298: u64,
    pub(crate) unk_72a0: u64,
    pub(crate) unk_72a8: u64,
    pub(crate) unk_72b0: u64,
    pub(crate) unk_72b8: u64,
    pub(crate) unk_72c0: u64,
    pub(crate) unk_72c8: u64,
    pub(crate) unk_72d0: u64,
    pub(crate) unk_72d8: u64,
    pub(crate) unk_72e0: u64,
    pub(crate) unk_72e8: u64,
    pub(crate) unk_72f0: u64,
    pub(crate) unk_72f8: u64,
    pub(crate) unk_7300: u64,
    pub(crate) unk_7308: u64,
    pub(crate) unk_7310: u64,
    pub(crate) unk_7318: u64,
    pub(crate) unk_7320: u64,
    pub(crate) unk_7328: u64,
    pub(crate) unk_7330: u64,
    pub(crate) unk_7338: u64,
    pub(crate) unk_7340: u64,
    pub(crate) unk_7348: u64,
    pub(crate) unk_7350: u64,
    pub(crate) unk_7358: u64,
    pub(crate) unk_7360: u64,
    pub(crate) unk_7368: u64,
    pub(crate) unk_7370: u64,
    pub(crate) unk_7378: u64,
    pub(crate) unk_7380: u64,
    pub(crate) unk_7388: u64,
    pub(crate) unk_7390: u64,
    pub(crate) unk_7398: u64,
    pub(crate) unk_73a0: u64,
    pub(crate) unk_73a8: u64,
    pub(crate) unk_73b0: u64,
    pub(crate) unk_73b8: u64,
    pub(crate) unk_73c0: u64,
    pub(crate) unk_73c8: u64,
    pub(crate) unk_73d0: u64,
    pub(crate) unk_73d8: u64,
    pub(crate) unk_73e0: u64,
    pub(crate) unk_73e8: u64,
    pub(crate) unk_73f0: u64,
    pub(crate) unk_73f8: u64,
    pub(crate) unk_7400: u64,
    pub(crate) unk_7408: u64,
    pub(crate) unk_7410: u64,
    pub(crate) unk_7418: u64,
    pub(crate) unk_7420: u32,
    pub(crate) unk_7424: u16,
    pub(crate) unk_7426: u8,
    pub(crate) unk_7650: u32,
    pub(crate) unk_765c: u32,
    pub(crate) unk_7660: u32,
    pub(crate) unk_76a0: u64,
    pub(crate) unk_76a8: u32,
    pub(crate) unk_76b0: u32,
    pub(crate) unk_76c4: u32,
    pub(crate) unk_76cc: u32,
    pub(crate) unk_76d4: u32,
    pub(crate) unk_76d8: u64,
    pub(crate) unk_76fc: u32,
    pub(crate) unk_7948: u64,
    pub(crate) unk_7b62: u16,
    pub(crate) unk_7b64: u16,
    pub(crate) unk_7b6a: u16,
    pub(crate) unk_7b6c: u16,
    pub(crate) unk_7b72: u16,
    pub(crate) unk_7b74: u16,
    pub(crate) unk_7b7a: u16,
    pub(crate) unk_7b7c: u32,
    pub(crate) unk_7b80: u32,
    pub(crate) unk_7b84: u16,
    pub(crate) unk_7b8a: u16,
    pub(crate) unk_7b8c: u32,
    pub(crate) unk_7b90: u16,
    pub(crate) unk_7b96: u16,
    pub(crate) unk_7b98: u16,
    pub(crate) unk_7b9e: u16,
    pub(crate) unk_7ba0: u32,
    pub(crate) unk_7ba4: u16,
    pub(crate) unk_7bf2: u16,
    pub(crate) unk_7bf4: u16,
    pub(crate) unk_7cbc: u32,
    pub(crate) unk_7d08: u8,
    pub(crate) unk_7d0a: u8,
    pub(crate) unk_7d0e: u16,
    pub(crate) unk_7d10: u16,
    pub(crate) unk_7d26: u16,
    pub(crate) unk_7d28: u16,
    pub(crate) unk_7d2a: u8,
    pub(crate) unk_7d2e: u16,
    pub(crate) unk_7d30: u16,
    pub(crate) unk_7d46: u16,
    pub(crate) unk_7d48: u16,
    pub(crate) unk_7d4a: u8,
    pub(crate) unk_7d4e: u16,
    pub(crate) unk_7d50: u16,
    pub(crate) unk_7d66: u16,
    pub(crate) unk_7d68: u16,
    pub(crate) unk_7e12: u16,
    pub(crate) unk_7e14: u16,
    pub(crate) unk_7e34: u32,
    pub(crate) unk_7e38: u64,
    pub(crate) unk_7f38: u64,
    pub(crate) unk_7f40: u32,
    pub(crate) unk_7fd4: u32,
    pub(crate) unk_80a4: u32,
    pub(crate) unk_80a8: u32,
    pub(crate) unk_80b0: u64,
    pub(crate) unk_80b8: u64,
    pub(crate) unk_80c8: u64,
    pub(crate) unk_80d0: u64,
    pub(crate) unk_80f0: u32,
    pub(crate) unk_80f8: u32,
    pub(crate) unk_8118: u32,
    pub(crate) unk_8148: u64,
    pub(crate) unk_8150: u64,
    pub(crate) unk_8158: u32,
    pub(crate) unk_8188: u64,
    pub(crate) unk_8198: u32,
    pub(crate) unk_8268: u64,
    pub(crate) unk_8288: u64,
    pub(crate) unk_8290: u64,
    pub(crate) unk_833c: u32,
    pub(crate) unk_8340: u32,
    pub(crate) unk_8348: u32,
    pub(crate) unk_8364: u32,
    pub(crate) unk_8370: u32,
    pub(crate) unk_8378: u32,
    pub(crate) unk_8380: u32,
    pub(crate) unk_8388: u64,
    pub(crate) unk_8390: u32,
    pub(crate) unk_8448: u64,
    pub(crate) unk_b0c4: u32,
    pub(crate) unk_b0c8: u32,
    pub(crate) unk_b860: u32,
    pub(crate) unk_b8e0: u64,
    pub(crate) unk_b8f0: u32,
    pub(crate) unk_bf94: u32,
    pub(crate) unk_bf98: u32,
    pub(crate) unk_bfe0: u32,
    pub(crate) unk_c118: u64,
    pub(crate) unk_1f220: u8,
    pub(crate) unk_1f300: u64,
    pub(crate) unk_237c0: u64,
}

impl HardwareData {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.unk_0000 = crate::hw::t8142::HARDWAREDATA.unk_0000;
        self.unk_0008 = crate::hw::t8142::HARDWAREDATA.unk_0008;
        self.unk_0010 = crate::hw::t8142::HARDWAREDATA.unk_0010;
        self.unk_0018 = crate::hw::t8142::HARDWAREDATA.unk_0018;
        self.unk_0020 = crate::hw::t8142::HARDWAREDATA.unk_0020;
        self.unk_0028 = crate::hw::t8142::HARDWAREDATA.unk_0028;
        self.unk_0660 = crate::hw::t8142::HARDWAREDATA.unk_0660;
        self.unk_0688 = crate::hw::t8142::HARDWAREDATA.unk_0688;
        self.unk_06b0 = crate::hw::t8142::HARDWAREDATA.unk_06b0;
        self.unk_06d8 = crate::hw::t8142::HARDWAREDATA.unk_06d8;
        self.unk_0700 = crate::hw::t8142::HARDWAREDATA.unk_0700;
        self.unk_0728 = crate::hw::t8142::HARDWAREDATA.unk_0728;
        self.unk_0750 = crate::hw::t8142::HARDWAREDATA.unk_0750;
        self.unk_0778 = crate::hw::t8142::HARDWAREDATA.unk_0778;
        self.unk_07a0 = crate::hw::t8142::HARDWAREDATA.unk_07a0;
        self.unk_07a8 = crate::hw::t8142::HARDWAREDATA.unk_07a8;
        self.ptr_07b0 = 0xfffffc2200000000u64 + 9u64 * 0x100000 + 0x0;
        self.unk_07b8 = crate::hw::t8142::HARDWAREDATA.unk_07b8;
        self.unk_07c8 = crate::hw::t8142::HARDWAREDATA.unk_07c8;
        self.unk_07d0 = crate::hw::t8142::HARDWAREDATA.unk_07d0;
        self.ptr_07d8 = 0xfffffc2200000000u64 + 10u64 * 0x100000 + 0x0;
        self.unk_07e0 = crate::hw::t8142::HARDWAREDATA.unk_07e0;
        self.unk_07f0 = crate::hw::t8142::HARDWAREDATA.unk_07f0;
        self.unk_0818 = crate::hw::t8142::HARDWAREDATA.unk_0818;
        self.unk_0840 = crate::hw::t8142::HARDWAREDATA.unk_0840;
        self.unk_0868 = crate::hw::t8142::HARDWAREDATA.unk_0868;
        self.unk_0890 = crate::hw::t8142::HARDWAREDATA.unk_0890;
        self.unk_08b8 = crate::hw::t8142::HARDWAREDATA.unk_08b8;
        self.unk_08e0 = crate::hw::t8142::HARDWAREDATA.unk_08e0;
        self.register_08e8 = inputs.sgx_base + 0x0;
        self.ptr_08f0 = 0xfffffc2200000000u64 + 17u64 * 0x100000 + 0x0;
        self.unk_08f8 = crate::hw::t8142::HARDWAREDATA.unk_08f8;
        self.unk_0908 = crate::hw::t8142::HARDWAREDATA.unk_0908;
        self.unk_0930 = crate::hw::t8142::HARDWAREDATA.unk_0930;
        self.unk_0958 = crate::hw::t8142::HARDWAREDATA.unk_0958;
        self.unk_0980 = crate::hw::t8142::HARDWAREDATA.unk_0980;
        self.unk_09a8 = crate::hw::t8142::HARDWAREDATA.unk_09a8;
        self.register_09b0 = inputs.sgx_base + 0x1000000;
        self.ptr_09b8 = 0xfffffc2200000000u64 + 22u64 * 0x100000 + 0x0;
        self.unk_09c0 = crate::hw::t8142::HARDWAREDATA.unk_09c0;
        self.unk_09d0 = crate::hw::t8142::HARDWAREDATA.unk_09d0;
        self.unk_09f8 = crate::hw::t8142::HARDWAREDATA.unk_09f8;
        self.register_0a00 = inputs.sgx_base + 0xec0000;
        self.ptr_0a08 = 0xfffffc2200000000u64 + 24u64 * 0x100000 + 0x0;
        self.unk_0a10 = crate::hw::t8142::HARDWAREDATA.unk_0a10;
        self.unk_0a20 = crate::hw::t8142::HARDWAREDATA.unk_0a20;
        self.unk_0a48 = crate::hw::t8142::HARDWAREDATA.unk_0a48;
        self.unk_0a70 = crate::hw::t8142::HARDWAREDATA.unk_0a70;
        self.unk_0a98 = crate::hw::t8142::HARDWAREDATA.unk_0a98;
        self.unk_0ac0 = crate::hw::t8142::HARDWAREDATA.unk_0ac0;
        self.unk_0ae8 = crate::hw::t8142::HARDWAREDATA.unk_0ae8;
        self.unk_0b10 = crate::hw::t8142::HARDWAREDATA.unk_0b10;
        self.register_0b18 = inputs.sgx_base + 0xd04000;
        self.ptr_0b20 = 0xfffffc2200000000u64 + 31u64 * 0x100000 + 0x0;
        self.unk_0b28 = crate::hw::t8142::HARDWAREDATA.unk_0b28;
        self.unk_0b30 = crate::hw::t8142::HARDWAREDATA.unk_0b30;
        self.unk_0b38 = crate::hw::t8142::HARDWAREDATA.unk_0b38;
        self.register_0b40 = inputs.sgx_base + 0xd0d000;
        self.ptr_0b48 = 0xfffffc2200000000u64 + 32u64 * 0x100000 + 0x1000;
        self.unk_0b50 = crate::hw::t8142::HARDWAREDATA.unk_0b50;
        self.unk_0b58 = crate::hw::t8142::HARDWAREDATA.unk_0b58;
        self.unk_0b60 = crate::hw::t8142::HARDWAREDATA.unk_0b60;
        self.register_0b68 = inputs.sgx_base + 0xd50000;
        self.ptr_0b70 = 0xfffffc2200000000u64 + 33u64 * 0x100000 + 0x0;
        self.unk_0b78 = crate::hw::t8142::HARDWAREDATA.unk_0b78;
        self.unk_0b80 = crate::hw::t8142::HARDWAREDATA.unk_0b80;
        self.unk_0b88 = crate::hw::t8142::HARDWAREDATA.unk_0b88;
        self.register_0b90 = inputs.sgx_base + 0xd10000;
        self.ptr_0b98 = 0xfffffc2200000000u64 + 34u64 * 0x100000 + 0x0;
        self.unk_0ba0 = crate::hw::t8142::HARDWAREDATA.unk_0ba0;
        self.unk_0ba8 = crate::hw::t8142::HARDWAREDATA.unk_0ba8;
        self.unk_0bb0 = crate::hw::t8142::HARDWAREDATA.unk_0bb0;
        self.unk_0bd8 = crate::hw::t8142::HARDWAREDATA.unk_0bd8;
        self.register_0be0 = inputs.sgx_base + 0xd40000;
        self.ptr_0be8 = 0xfffffc2200000000u64 + 36u64 * 0x100000 + 0x0;
        self.unk_0bf0 = crate::hw::t8142::HARDWAREDATA.unk_0bf0;
        self.unk_0bf8 = crate::hw::t8142::HARDWAREDATA.unk_0bf8;
        self.unk_0c00 = crate::hw::t8142::HARDWAREDATA.unk_0c00;
        self.register_0c08 = inputs.sgx_base + 0xd60000;
        self.ptr_0c10 = 0xfffffc2200000000u64 + 37u64 * 0x100000 + 0x0;
        self.unk_0c18 = crate::hw::t8142::HARDWAREDATA.unk_0c18;
        self.unk_0c20 = crate::hw::t8142::HARDWAREDATA.unk_0c20;
        self.unk_0c28 = crate::hw::t8142::HARDWAREDATA.unk_0c28;
        self.unk_0c50 = crate::hw::t8142::HARDWAREDATA.unk_0c50;
        self.unk_0c78 = crate::hw::t8142::HARDWAREDATA.unk_0c78;
        self.register_0c80 = inputs.sgx_base + 0xe00000;
        self.ptr_0c88 = 0xfffffc2200000000u64 + 40u64 * 0x100000 + 0x0;
        self.unk_0c90 = crate::hw::t8142::HARDWAREDATA.unk_0c90;
        self.unk_0c98 = crate::hw::t8142::HARDWAREDATA.unk_0c98;
        self.unk_0ca0 = crate::hw::t8142::HARDWAREDATA.unk_0ca0;
        self.unk_0cc8 = crate::hw::t8142::HARDWAREDATA.unk_0cc8;
        self.unk_0cf0 = crate::hw::t8142::HARDWAREDATA.unk_0cf0;
        self.unk_0d18 = crate::hw::t8142::HARDWAREDATA.unk_0d18;
        self.register_0d20 = inputs.sgx_base + 0xe08000;
        self.ptr_0d28 = 0xfffffc2200000000u64 + 44u64 * 0x100000 + 0x0;
        self.unk_0d30 = crate::hw::t8142::HARDWAREDATA.unk_0d30;
        self.unk_0d40 = crate::hw::t8142::HARDWAREDATA.unk_0d40;
        self.unk_0d68 = crate::hw::t8142::HARDWAREDATA.unk_0d68;
        self.register_0d70 = inputs.sgx_base + 0xe33800;
        self.ptr_0d78 = 0xfffffc2200000000u64 + 46u64 * 0x100000 + 0x3800;
        self.unk_0d80 = crate::hw::t8142::HARDWAREDATA.unk_0d80;
        self.unk_0d90 = crate::hw::t8142::HARDWAREDATA.unk_0d90;
        self.register_0d98 = inputs.sgx_base + 0xe35000;
        self.ptr_0da0 = 0xfffffc2200000000u64 + 47u64 * 0x100000 + 0x1000;
        self.unk_0da8 = crate::hw::t8142::HARDWAREDATA.unk_0da8;
        self.unk_0db8 = crate::hw::t8142::HARDWAREDATA.unk_0db8;
        self.unk_0de0 = crate::hw::t8142::HARDWAREDATA.unk_0de0;
        self.unk_0e08 = crate::hw::t8142::HARDWAREDATA.unk_0e08;
        self.register_0e10 = inputs.sgx_base + 0xd24000;
        self.ptr_0e18 = 0xfffffc2200000000u64 + 50u64 * 0x100000 + 0x0;
        self.unk_0e20 = crate::hw::t8142::HARDWAREDATA.unk_0e20;
        self.unk_0e28 = crate::hw::t8142::HARDWAREDATA.unk_0e28;
        self.unk_0e30 = crate::hw::t8142::HARDWAREDATA.unk_0e30;
        self.unk_0e58 = crate::hw::t8142::HARDWAREDATA.unk_0e58;
        self.unk_0e80 = crate::hw::t8142::HARDWAREDATA.unk_0e80;
        self.unk_0ea8 = crate::hw::t8142::HARDWAREDATA.unk_0ea8;
        self.unk_0ed0 = crate::hw::t8142::HARDWAREDATA.unk_0ed0;
        self.unk_0ef8 = crate::hw::t8142::HARDWAREDATA.unk_0ef8;
        self.register_0f00 = inputs.sgx_base + 0x14400;
        self.ptr_0f08 = 0xfffffc2200000000u64 + 56u64 * 0x100000 + 0x400;
        self.unk_0f10 = crate::hw::t8142::HARDWAREDATA.unk_0f10;
        self.unk_0f18 = crate::hw::t8142::HARDWAREDATA.unk_0f18;
        self.unk_0f20 = crate::hw::t8142::HARDWAREDATA.unk_0f20;
        self.unk_0f48 = crate::hw::t8142::HARDWAREDATA.unk_0f48;
        self.unk_0f70 = crate::hw::t8142::HARDWAREDATA.unk_0f70;
        self.unk_0f98 = crate::hw::t8142::HARDWAREDATA.unk_0f98;
        self.unk_0fb0 = crate::hw::t8142::HARDWAREDATA.unk_0fb0;
        self.unk_0fc0 = crate::hw::t8142::HARDWAREDATA.unk_0fc0;
        self.unk_0fd4 = crate::hw::t8142::HARDWAREDATA.unk_0fd4;
        self.unk_0fd8 = crate::hw::t8142::HARDWAREDATA.unk_0fd8;
        self.unk_0fe4 = crate::hw::t8142::HARDWAREDATA.unk_0fe4;
        self.unk_0fec = crate::hw::t8142::HARDWAREDATA.unk_0fec;
        self.unk_0ff0 = crate::hw::t8142::HARDWAREDATA.unk_0ff0;
        self.hardwaredata_unk_0ff4 = inputs.calibration.hardwaredata_unk_0ff4;
        self.unk_0ffc = crate::hw::t8142::HARDWAREDATA.unk_0ffc;
        self.unk_1000 = crate::hw::t8142::HARDWAREDATA.unk_1000;
        self.unk_1020 = crate::hw::t8142::HARDWAREDATA.unk_1020;
        self.unk_1040 = crate::hw::t8142::HARDWAREDATA.unk_1040;
        self.unk_104c = crate::hw::t8142::HARDWAREDATA.unk_104c;
        self.unk_1050 = crate::hw::t8142::HARDWAREDATA.unk_1050;
        self.unk_1058 = crate::hw::t8142::HARDWAREDATA.unk_1058;
        self.unk_10a4 = crate::hw::t8142::HARDWAREDATA.unk_10a4;
        self.unk_10a8 = crate::hw::t8142::HARDWAREDATA.unk_10a8;
        self.unk_10d4 = crate::hw::t8142::HARDWAREDATA.unk_10d4;
        self.unk_10d8 = crate::hw::t8142::HARDWAREDATA.unk_10d8;
        self.maximum_pstate = inputs.calibration.maximum_pstate;
        self.pstate_mhz_0 = inputs.state(0).mhz;
        self.pstate_mhz_1 = inputs.state(1).mhz;
        self.pstate_mhz_2 = inputs.state(2).mhz;
        self.pstate_mhz_3 = inputs.state(3).mhz;
        self.pstate_mhz_4 = inputs.state(4).mhz;
        self.pstate_mhz_5 = inputs.state(5).mhz;
        self.pstate_mhz_6 = inputs.state(6).mhz;
        self.pstate_mhz_7 = inputs.state(7).mhz;
        self.pstate_mhz_8 = inputs.state(8).mhz;
        self.pstate_mhz_9 = inputs.state(9).mhz;
        self.pstate_mhz_10 = inputs.state(10).mhz;
        self.pstate_mhz_11 = inputs.state(11).mhz;
        self.pstate_mhz_12 = inputs.state(12).mhz;
        self.pstate_mhz_13 = inputs.state(13).mhz;
        self.pstate_mhz_14 = inputs.state(14).mhz;
        self.pstate_mhz_15 = inputs.state(15).mhz;
        self.pstate_core_mv_0 = inputs.state(0).core_mv;
        self.pstate_core_mv_1 = inputs.state(1).core_mv;
        self.pstate_core_mv_2 = inputs.state(2).core_mv;
        self.pstate_core_mv_3 = inputs.state(3).core_mv;
        self.pstate_core_mv_4 = inputs.state(4).core_mv;
        self.pstate_core_mv_5 = inputs.state(5).core_mv;
        self.pstate_core_mv_6 = inputs.state(6).core_mv;
        self.pstate_core_mv_7 = inputs.state(7).core_mv;
        self.pstate_core_mv_8 = inputs.state(8).core_mv;
        self.pstate_core_mv_9 = inputs.state(9).core_mv;
        self.pstate_core_mv_10 = inputs.state(10).core_mv;
        self.pstate_core_mv_11 = inputs.state(11).core_mv;
        self.pstate_core_mv_12 = inputs.state(12).core_mv;
        self.pstate_core_mv_13 = inputs.state(13).core_mv;
        self.pstate_core_mv_14 = inputs.state(14).core_mv;
        self.pstate_core_mv_15 = inputs.state(15).core_mv;
        self.pstate_sram_mv_0 = inputs.state(0).sram_mv;
        self.pstate_sram_mv_1 = inputs.state(1).sram_mv;
        self.pstate_sram_mv_2 = inputs.state(2).sram_mv;
        self.pstate_sram_mv_3 = inputs.state(3).sram_mv;
        self.pstate_sram_mv_4 = inputs.state(4).sram_mv;
        self.pstate_sram_mv_5 = inputs.state(5).sram_mv;
        self.pstate_sram_mv_6 = inputs.state(6).sram_mv;
        self.pstate_sram_mv_7 = inputs.state(7).sram_mv;
        self.pstate_sram_mv_8 = inputs.state(8).sram_mv;
        self.pstate_sram_mv_9 = inputs.state(9).sram_mv;
        self.pstate_sram_mv_10 = inputs.state(10).sram_mv;
        self.pstate_sram_mv_11 = inputs.state(11).sram_mv;
        self.pstate_sram_mv_12 = inputs.state(12).sram_mv;
        self.pstate_sram_mv_13 = inputs.state(13).sram_mv;
        self.pstate_sram_mv_14 = inputs.state(14).sram_mv;
        self.pstate_sram_mv_15 = inputs.state(15).sram_mv;
        self.unk_1964 = crate::hw::t8142::HARDWAREDATA.unk_1964;
        self.unk_1968 = crate::hw::t8142::HARDWAREDATA.unk_1968;
        self.unk_1970 = crate::hw::t8142::HARDWAREDATA.unk_1970;
        self.unk_1978 = crate::hw::t8142::HARDWAREDATA.unk_1978;
        self.unk_1980 = crate::hw::t8142::HARDWAREDATA.unk_1980;
        self.unk_1988 = crate::hw::t8142::HARDWAREDATA.unk_1988;
        self.unk_1990 = crate::hw::t8142::HARDWAREDATA.unk_1990;
        self.unk_1998 = crate::hw::t8142::HARDWAREDATA.unk_1998;
        self.hardwaredata_unk_19e8 = inputs.calibration.hardwaredata_unk_19e8;
        self.hardwaredata_unk_19ec = inputs.calibration.hardwaredata_unk_19ec;
        self.hardwaredata_unk_19f0 = inputs.calibration.hardwaredata_unk_19f0;
        self.hardwaredata_unk_19f4 = inputs.calibration.hardwaredata_unk_19f4;
        self.hardwaredata_unk_19f8 = inputs.calibration.hardwaredata_unk_19f8;
        self.hardwaredata_unk_19fc = inputs.calibration.hardwaredata_unk_19fc;
        self.hardwaredata_unk_1a00 = inputs.calibration.hardwaredata_unk_1a00;
        self.hardwaredata_unk_1a04 = inputs.calibration.hardwaredata_unk_1a04;
        self.hardwaredata_unk_1a08 = inputs.calibration.hardwaredata_unk_1a08;
        self.hardwaredata_unk_1a0c = inputs.calibration.hardwaredata_unk_1a0c;
        self.hardwaredata_unk_1a10 = inputs.calibration.hardwaredata_unk_1a10;
        self.hardwaredata_unk_1a14 = inputs.calibration.hardwaredata_unk_1a14;
        self.hardwaredata_unk_1a18 = inputs.calibration.hardwaredata_unk_1a18;
        self.hardwaredata_unk_1a28 = inputs.calibration.hardwaredata_unk_1a28;
        self.hardwaredata_unk_1a30 = inputs.calibration.hardwaredata_unk_1a30;
        self.hardwaredata_unk_1a38 = inputs.calibration.hardwaredata_unk_1a38;
        self.hardwaredata_unk_1a40 = inputs.calibration.hardwaredata_unk_1a40;
        self.hardwaredata_unk_1a48 = inputs.calibration.hardwaredata_unk_1a48;
        self.hardwaredata_unk_1a50 = inputs.calibration.hardwaredata_unk_1a50;
        self.unk_1a58 = crate::hw::t8142::HARDWAREDATA.unk_1a58;
        self.unk_2660 = crate::hw::t8142::HARDWAREDATA.unk_2660;
        self.unk_2668 = crate::hw::t8142::HARDWAREDATA.unk_2668;
        self.unk_2670 = crate::hw::t8142::HARDWAREDATA.unk_2670;
        self.unk_2684 = crate::hw::t8142::HARDWAREDATA.unk_2684;
        self.unk_268c = crate::hw::t8142::HARDWAREDATA.unk_268c;
        self.unk_26b0 = crate::hw::t8142::HARDWAREDATA.unk_26b0;
        self.unk_26b8 = crate::hw::t8142::HARDWAREDATA.unk_26b8;
        self.unk_26c0 = crate::hw::t8142::HARDWAREDATA.unk_26c0;
        self.unk_26c8 = crate::hw::t8142::HARDWAREDATA.unk_26c8;
        self.unk_26d0 = crate::hw::t8142::HARDWAREDATA.unk_26d0;
        self.unk_26d8 = crate::hw::t8142::HARDWAREDATA.unk_26d8;
        self.unk_26e0 = crate::hw::t8142::HARDWAREDATA.unk_26e0;
        self.unk_26e8 = crate::hw::t8142::HARDWAREDATA.unk_26e8;
        self.unk_26f0 = crate::hw::t8142::HARDWAREDATA.unk_26f0;
        self.unk_26f8 = crate::hw::t8142::HARDWAREDATA.unk_26f8;
        self.unk_270c = crate::hw::t8142::HARDWAREDATA.unk_270c;
        self.unk_2710 = crate::hw::t8142::HARDWAREDATA.unk_2710;
        self.unk_2711 = crate::hw::t8142::HARDWAREDATA.unk_2711;
        self.unk_2712 = crate::hw::t8142::HARDWAREDATA.unk_2712;
        self.unk_2713 = crate::hw::t8142::HARDWAREDATA.unk_2713;
        self.scheduling_pause_us = crate::hw::t8142::HARDWAREDATA.scheduling_pause_us;
        self.unk_2720 = crate::hw::t8142::HARDWAREDATA.unk_2720;
        self.unk_2728 = crate::hw::t8142::HARDWAREDATA.unk_2728;
        self.unk_2730 = crate::hw::t8142::HARDWAREDATA.unk_2730;
        self.ptr_2734 = crate::hw::t8142::OBJECTS[5].aliases[0] + 0x0;
        self.ptr_273c = crate::hw::t8142::OBJECTS[5].va + 0x0;
        self.unk_2744 = crate::hw::t8142::HARDWAREDATA.unk_2744;
        self.unk_2748 = crate::hw::t8142::HARDWAREDATA.unk_2748;
        self.ptr_2754 = crate::hw::t8142::OBJECTS[6].aliases[0] + 0x0;
        self.ptr_275c = crate::hw::t8142::OBJECTS[6].va + 0x0;
        self.unk_2764 = crate::hw::t8142::HARDWAREDATA.unk_2764;
        self.unk_2768 = crate::hw::t8142::HARDWAREDATA.unk_2768;
        self.ptr_2774 = crate::hw::t8142::OBJECTS[7].aliases[0] + 0x0;
        self.ptr_277c = crate::hw::t8142::OBJECTS[7].va + 0x0;
        self.unk_2784 = crate::hw::t8142::HARDWAREDATA.unk_2784;
        self.unk_2788 = crate::hw::t8142::HARDWAREDATA.unk_2788;
        self.ptr_2794 = crate::hw::t8142::OBJECTS[8].aliases[0] + 0x0;
        self.ptr_279c = crate::hw::t8142::OBJECTS[8].va + 0x0;
        self.unk_27a4 = crate::hw::t8142::HARDWAREDATA.unk_27a4;
        self.unk_27a8 = crate::hw::t8142::HARDWAREDATA.unk_27a8;
        self.ptr_27cc = crate::hw::t8142::OBJECTS[4].va + 0x0;
        self.unk_27dc = crate::hw::t8142::HARDWAREDATA.unk_27dc;
        self.unk_27e0 = crate::hw::t8142::HARDWAREDATA.unk_27e0;
        self.unk_2808 = crate::hw::t8142::HARDWAREDATA.unk_2808;
        self.unk_2810 = crate::hw::t8142::HARDWAREDATA.unk_2810;
        self.unk_3ed8 = crate::hw::t8142::HARDWAREDATA.unk_3ed8;
        self.unk_3ef0 = crate::hw::t8142::HARDWAREDATA.unk_3ef0;
        self.sample_period_5384 = inputs.sample_period_ms * 24000;
        self.sample_period_5388 = inputs.sample_period_ms * 24000;
        self.unk_538c = crate::hw::t8142::HARDWAREDATA.unk_538c;
        self.unk_5390 = crate::hw::t8142::HARDWAREDATA.unk_5390;
        self.unk_5398 = crate::hw::t8142::HARDWAREDATA.unk_5398;
        self.unk_53ac = crate::hw::t8142::HARDWAREDATA.unk_53ac;
        self.unk_53b0 = crate::hw::t8142::HARDWAREDATA.unk_53b0;
        self.hardwaredata_unk_53c0 = inputs.calibration.hardwaredata_unk_53c0;
        self.unk_53c4 = crate::hw::t8142::HARDWAREDATA.unk_53c4;
        self.hardwaredata_unk_53c8 = inputs.calibration.hardwaredata_unk_53c8;
        self.unk_53d0 = crate::hw::t8142::HARDWAREDATA.unk_53d0;
        self.unk_53dc = crate::hw::t8142::HARDWAREDATA.unk_53dc;
        self.unk_53e0 = crate::hw::t8142::HARDWAREDATA.unk_53e0;
        self.unk_53e8 = crate::hw::t8142::HARDWAREDATA.unk_53e8;
        self.unk_53f0 = crate::hw::t8142::HARDWAREDATA.unk_53f0;
        self.hardwaredata_unk_5400 = inputs.calibration.hardwaredata_unk_5400;
        self.unk_5404 = crate::hw::t8142::HARDWAREDATA.unk_5404;
        self.hardwaredata_unk_5408 = inputs.calibration.hardwaredata_unk_5408;
        self.unk_5410 = crate::hw::t8142::HARDWAREDATA.unk_5410;
        self.unk_541c = crate::hw::t8142::HARDWAREDATA.unk_541c;
        self.unk_5420 = crate::hw::t8142::HARDWAREDATA.unk_5420;
        self.unk_5428 = crate::hw::t8142::HARDWAREDATA.unk_5428;
        self.unk_5450 = crate::hw::t8142::HARDWAREDATA.unk_5450;
        self.unk_5458 = crate::hw::t8142::HARDWAREDATA.unk_5458;
        self.unk_5460 = crate::hw::t8142::HARDWAREDATA.unk_5460;
        self.unk_5468 = crate::hw::t8142::HARDWAREDATA.unk_5468;
        self.unk_5470 = crate::hw::t8142::HARDWAREDATA.unk_5470;
        self.unk_5478 = crate::hw::t8142::HARDWAREDATA.unk_5478;
        self.unk_5480 = crate::hw::t8142::HARDWAREDATA.unk_5480;
        self.unk_5d48 = crate::hw::t8142::HARDWAREDATA.unk_5d48;
        self.unk_5d54 = crate::hw::t8142::HARDWAREDATA.unk_5d54;
        self.unk_5d5c = crate::hw::t8142::HARDWAREDATA.unk_5d5c;
        self.unk_5d64 = crate::hw::t8142::HARDWAREDATA.unk_5d64;
        self.hardwaredata_unk_5d6c = inputs.calibration.hardwaredata_unk_5d6c;
        self.unk_5d74 = crate::hw::t8142::HARDWAREDATA.unk_5d74;
        self.hardwaredata_unk_5d7c = inputs.calibration.hardwaredata_unk_5d7c;
        self.hardwaredata_unk_5d80 = inputs.calibration.hardwaredata_unk_5d80;
        self.hardwaredata_unk_5d88 = inputs.calibration.hardwaredata_unk_5d88;
        self.hardwaredata_unk_5d94 = inputs.calibration.hardwaredata_unk_5d94;
        self.hardwaredata_unk_5dc8 = inputs.calibration.hardwaredata_unk_5dc8;
        self.hardwaredata_unk_5e00 = inputs.calibration.hardwaredata_unk_5e00;
        self.hardwaredata_unk_5e0c = inputs.calibration.hardwaredata_unk_5e0c;
        self.hardwaredata_unk_5e14 = inputs.calibration.hardwaredata_unk_5e14;
        self.hardwaredata_unk_5e1c = inputs.calibration.hardwaredata_unk_5e1c;
        self.unk_5e24 = crate::hw::t8142::HARDWAREDATA.unk_5e24;
        self.unk_5e28 = crate::hw::t8142::HARDWAREDATA.unk_5e28;
        self.hardwaredata_unk_5e38 = inputs.calibration.hardwaredata_unk_5e38;
        self.hardwaredata_unk_5e4c = inputs.calibration.hardwaredata_unk_5e4c;
        self.unk_5e58 = crate::hw::t8142::HARDWAREDATA.unk_5e58;
        self.hardwaredata_unk_5e60 = inputs.calibration.hardwaredata_unk_5e60;
        self.unk_5eb8 = crate::hw::t8142::HARDWAREDATA.unk_5eb8;
        self.hardwaredata_unk_5ec0 = inputs.calibration.hardwaredata_unk_5ec0;
        self.hardwaredata_unk_5ec8 = inputs.calibration.hardwaredata_unk_5ec8;
        self.unk_5ed0 = crate::hw::t8142::HARDWAREDATA.unk_5ed0;
        self.unk_5edc = crate::hw::t8142::HARDWAREDATA.unk_5edc;
        self.unk_5ee0 = crate::hw::t8142::HARDWAREDATA.unk_5ee0;
        self.unk_5ee8 = crate::hw::t8142::HARDWAREDATA.unk_5ee8;
        self.unk_5ef0 = crate::hw::t8142::HARDWAREDATA.unk_5ef0;
        self.unk_5ef8 = crate::hw::t8142::HARDWAREDATA.unk_5ef8;
        self.unk_5f00 = crate::hw::t8142::HARDWAREDATA.unk_5f00;
        self.hardwaredata_unk_5f04 = inputs.calibration.hardwaredata_unk_5f04;
        self.hardwaredata_unk_5f08 = inputs.calibration.hardwaredata_unk_5f08;
        self.hardwaredata_unk_5f10 = inputs.calibration.hardwaredata_unk_5f10;
        self.unk_5f1c = crate::hw::t8142::HARDWAREDATA.unk_5f1c;
        self.unk_5f24 = crate::hw::t8142::HARDWAREDATA.unk_5f24;
        self.hardwaredata_unk_5f50 = inputs.calibration.hardwaredata_unk_5f50;
        self.unk_5f70 = crate::hw::t8142::HARDWAREDATA.unk_5f70;
        self.unk_5f78 = crate::hw::t8142::HARDWAREDATA.unk_5f78;
        self.unk_5f80 = crate::hw::t8142::HARDWAREDATA.unk_5f80;
        self.unk_5f88 = crate::hw::t8142::HARDWAREDATA.unk_5f88;
        self.unk_5f90 = crate::hw::t8142::HARDWAREDATA.unk_5f90;
        self.unk_5f98 = crate::hw::t8142::HARDWAREDATA.unk_5f98;
        self.unk_5ff4 = crate::hw::t8142::HARDWAREDATA.unk_5ff4;
        self.unk_7027 = crate::hw::t8142::HARDWAREDATA.unk_7027;
        self.unk_7028 = crate::hw::t8142::HARDWAREDATA.unk_7028;
        self.unk_7030 = crate::hw::t8142::HARDWAREDATA.unk_7030;
        self.unk_7038 = crate::hw::t8142::HARDWAREDATA.unk_7038;
        self.unk_7040 = crate::hw::t8142::HARDWAREDATA.unk_7040;
        self.unk_7048 = crate::hw::t8142::HARDWAREDATA.unk_7048;
        self.unk_7050 = crate::hw::t8142::HARDWAREDATA.unk_7050;
        self.unk_7058 = crate::hw::t8142::HARDWAREDATA.unk_7058;
        self.unk_7060 = crate::hw::t8142::HARDWAREDATA.unk_7060;
        self.unk_7068 = crate::hw::t8142::HARDWAREDATA.unk_7068;
        self.unk_7070 = crate::hw::t8142::HARDWAREDATA.unk_7070;
        self.unk_7078 = crate::hw::t8142::HARDWAREDATA.unk_7078;
        self.unk_7080 = crate::hw::t8142::HARDWAREDATA.unk_7080;
        self.unk_7088 = crate::hw::t8142::HARDWAREDATA.unk_7088;
        self.unk_7090 = crate::hw::t8142::HARDWAREDATA.unk_7090;
        self.unk_7098 = crate::hw::t8142::HARDWAREDATA.unk_7098;
        self.unk_70a0 = crate::hw::t8142::HARDWAREDATA.unk_70a0;
        self.unk_70a8 = crate::hw::t8142::HARDWAREDATA.unk_70a8;
        self.unk_70b0 = crate::hw::t8142::HARDWAREDATA.unk_70b0;
        self.unk_70b8 = crate::hw::t8142::HARDWAREDATA.unk_70b8;
        self.unk_70c0 = crate::hw::t8142::HARDWAREDATA.unk_70c0;
        self.unk_70c8 = crate::hw::t8142::HARDWAREDATA.unk_70c8;
        self.unk_70d0 = crate::hw::t8142::HARDWAREDATA.unk_70d0;
        self.unk_70d8 = crate::hw::t8142::HARDWAREDATA.unk_70d8;
        self.unk_70e0 = crate::hw::t8142::HARDWAREDATA.unk_70e0;
        self.unk_70e8 = crate::hw::t8142::HARDWAREDATA.unk_70e8;
        self.unk_70f0 = crate::hw::t8142::HARDWAREDATA.unk_70f0;
        self.unk_70f8 = crate::hw::t8142::HARDWAREDATA.unk_70f8;
        self.unk_7100 = crate::hw::t8142::HARDWAREDATA.unk_7100;
        self.unk_7108 = crate::hw::t8142::HARDWAREDATA.unk_7108;
        self.unk_7110 = crate::hw::t8142::HARDWAREDATA.unk_7110;
        self.unk_7118 = crate::hw::t8142::HARDWAREDATA.unk_7118;
        self.unk_7120 = crate::hw::t8142::HARDWAREDATA.unk_7120;
        self.unk_7128 = crate::hw::t8142::HARDWAREDATA.unk_7128;
        self.unk_7130 = crate::hw::t8142::HARDWAREDATA.unk_7130;
        self.unk_7138 = crate::hw::t8142::HARDWAREDATA.unk_7138;
        self.unk_7140 = crate::hw::t8142::HARDWAREDATA.unk_7140;
        self.unk_7148 = crate::hw::t8142::HARDWAREDATA.unk_7148;
        self.unk_7150 = crate::hw::t8142::HARDWAREDATA.unk_7150;
        self.unk_7158 = crate::hw::t8142::HARDWAREDATA.unk_7158;
        self.unk_7160 = crate::hw::t8142::HARDWAREDATA.unk_7160;
        self.unk_7168 = crate::hw::t8142::HARDWAREDATA.unk_7168;
        self.unk_7170 = crate::hw::t8142::HARDWAREDATA.unk_7170;
        self.unk_7178 = crate::hw::t8142::HARDWAREDATA.unk_7178;
        self.unk_7180 = crate::hw::t8142::HARDWAREDATA.unk_7180;
        self.unk_7188 = crate::hw::t8142::HARDWAREDATA.unk_7188;
        self.unk_7190 = crate::hw::t8142::HARDWAREDATA.unk_7190;
        self.unk_7198 = crate::hw::t8142::HARDWAREDATA.unk_7198;
        self.unk_71a0 = crate::hw::t8142::HARDWAREDATA.unk_71a0;
        self.unk_71a8 = crate::hw::t8142::HARDWAREDATA.unk_71a8;
        self.unk_71b0 = crate::hw::t8142::HARDWAREDATA.unk_71b0;
        self.unk_71b8 = crate::hw::t8142::HARDWAREDATA.unk_71b8;
        self.unk_71c0 = crate::hw::t8142::HARDWAREDATA.unk_71c0;
        self.unk_71c8 = crate::hw::t8142::HARDWAREDATA.unk_71c8;
        self.unk_71d0 = crate::hw::t8142::HARDWAREDATA.unk_71d0;
        self.unk_71d8 = crate::hw::t8142::HARDWAREDATA.unk_71d8;
        self.unk_71e0 = crate::hw::t8142::HARDWAREDATA.unk_71e0;
        self.unk_71e8 = crate::hw::t8142::HARDWAREDATA.unk_71e8;
        self.unk_71f0 = crate::hw::t8142::HARDWAREDATA.unk_71f0;
        self.unk_71f8 = crate::hw::t8142::HARDWAREDATA.unk_71f8;
        self.unk_7200 = crate::hw::t8142::HARDWAREDATA.unk_7200;
        self.unk_7208 = crate::hw::t8142::HARDWAREDATA.unk_7208;
        self.unk_7210 = crate::hw::t8142::HARDWAREDATA.unk_7210;
        self.unk_7218 = crate::hw::t8142::HARDWAREDATA.unk_7218;
        self.unk_7220 = crate::hw::t8142::HARDWAREDATA.unk_7220;
        self.unk_7228 = crate::hw::t8142::HARDWAREDATA.unk_7228;
        self.unk_7230 = crate::hw::t8142::HARDWAREDATA.unk_7230;
        self.unk_7238 = crate::hw::t8142::HARDWAREDATA.unk_7238;
        self.unk_7240 = crate::hw::t8142::HARDWAREDATA.unk_7240;
        self.unk_7248 = crate::hw::t8142::HARDWAREDATA.unk_7248;
        self.unk_7250 = crate::hw::t8142::HARDWAREDATA.unk_7250;
        self.unk_7258 = crate::hw::t8142::HARDWAREDATA.unk_7258;
        self.unk_7260 = crate::hw::t8142::HARDWAREDATA.unk_7260;
        self.unk_7268 = crate::hw::t8142::HARDWAREDATA.unk_7268;
        self.unk_7270 = crate::hw::t8142::HARDWAREDATA.unk_7270;
        self.unk_7278 = crate::hw::t8142::HARDWAREDATA.unk_7278;
        self.unk_7280 = crate::hw::t8142::HARDWAREDATA.unk_7280;
        self.unk_7288 = crate::hw::t8142::HARDWAREDATA.unk_7288;
        self.unk_7290 = crate::hw::t8142::HARDWAREDATA.unk_7290;
        self.unk_7298 = crate::hw::t8142::HARDWAREDATA.unk_7298;
        self.unk_72a0 = crate::hw::t8142::HARDWAREDATA.unk_72a0;
        self.unk_72a8 = crate::hw::t8142::HARDWAREDATA.unk_72a8;
        self.unk_72b0 = crate::hw::t8142::HARDWAREDATA.unk_72b0;
        self.unk_72b8 = crate::hw::t8142::HARDWAREDATA.unk_72b8;
        self.unk_72c0 = crate::hw::t8142::HARDWAREDATA.unk_72c0;
        self.unk_72c8 = crate::hw::t8142::HARDWAREDATA.unk_72c8;
        self.unk_72d0 = crate::hw::t8142::HARDWAREDATA.unk_72d0;
        self.unk_72d8 = crate::hw::t8142::HARDWAREDATA.unk_72d8;
        self.unk_72e0 = crate::hw::t8142::HARDWAREDATA.unk_72e0;
        self.unk_72e8 = crate::hw::t8142::HARDWAREDATA.unk_72e8;
        self.unk_72f0 = crate::hw::t8142::HARDWAREDATA.unk_72f0;
        self.unk_72f8 = crate::hw::t8142::HARDWAREDATA.unk_72f8;
        self.unk_7300 = crate::hw::t8142::HARDWAREDATA.unk_7300;
        self.unk_7308 = crate::hw::t8142::HARDWAREDATA.unk_7308;
        self.unk_7310 = crate::hw::t8142::HARDWAREDATA.unk_7310;
        self.unk_7318 = crate::hw::t8142::HARDWAREDATA.unk_7318;
        self.unk_7320 = crate::hw::t8142::HARDWAREDATA.unk_7320;
        self.unk_7328 = crate::hw::t8142::HARDWAREDATA.unk_7328;
        self.unk_7330 = crate::hw::t8142::HARDWAREDATA.unk_7330;
        self.unk_7338 = crate::hw::t8142::HARDWAREDATA.unk_7338;
        self.unk_7340 = crate::hw::t8142::HARDWAREDATA.unk_7340;
        self.unk_7348 = crate::hw::t8142::HARDWAREDATA.unk_7348;
        self.unk_7350 = crate::hw::t8142::HARDWAREDATA.unk_7350;
        self.unk_7358 = crate::hw::t8142::HARDWAREDATA.unk_7358;
        self.unk_7360 = crate::hw::t8142::HARDWAREDATA.unk_7360;
        self.unk_7368 = crate::hw::t8142::HARDWAREDATA.unk_7368;
        self.unk_7370 = crate::hw::t8142::HARDWAREDATA.unk_7370;
        self.unk_7378 = crate::hw::t8142::HARDWAREDATA.unk_7378;
        self.unk_7380 = crate::hw::t8142::HARDWAREDATA.unk_7380;
        self.unk_7388 = crate::hw::t8142::HARDWAREDATA.unk_7388;
        self.unk_7390 = crate::hw::t8142::HARDWAREDATA.unk_7390;
        self.unk_7398 = crate::hw::t8142::HARDWAREDATA.unk_7398;
        self.unk_73a0 = crate::hw::t8142::HARDWAREDATA.unk_73a0;
        self.unk_73a8 = crate::hw::t8142::HARDWAREDATA.unk_73a8;
        self.unk_73b0 = crate::hw::t8142::HARDWAREDATA.unk_73b0;
        self.unk_73b8 = crate::hw::t8142::HARDWAREDATA.unk_73b8;
        self.unk_73c0 = crate::hw::t8142::HARDWAREDATA.unk_73c0;
        self.unk_73c8 = crate::hw::t8142::HARDWAREDATA.unk_73c8;
        self.unk_73d0 = crate::hw::t8142::HARDWAREDATA.unk_73d0;
        self.unk_73d8 = crate::hw::t8142::HARDWAREDATA.unk_73d8;
        self.unk_73e0 = crate::hw::t8142::HARDWAREDATA.unk_73e0;
        self.unk_73e8 = crate::hw::t8142::HARDWAREDATA.unk_73e8;
        self.unk_73f0 = crate::hw::t8142::HARDWAREDATA.unk_73f0;
        self.unk_73f8 = crate::hw::t8142::HARDWAREDATA.unk_73f8;
        self.unk_7400 = crate::hw::t8142::HARDWAREDATA.unk_7400;
        self.unk_7408 = crate::hw::t8142::HARDWAREDATA.unk_7408;
        self.unk_7410 = crate::hw::t8142::HARDWAREDATA.unk_7410;
        self.unk_7418 = crate::hw::t8142::HARDWAREDATA.unk_7418;
        self.unk_7420 = crate::hw::t8142::HARDWAREDATA.unk_7420;
        self.unk_7424 = crate::hw::t8142::HARDWAREDATA.unk_7424;
        self.unk_7426 = crate::hw::t8142::HARDWAREDATA.unk_7426;
        self.effort_normalization = inputs.calibration.effort_normalization;
        self.effort_floor_min = inputs.calibration.effort_floor_min;
        self.effort_floor_max = inputs.calibration.effort_floor_max;
        self.effort_max_frequency_mhz = inputs.calibration.effort_max_frequency_mhz;
        self.hardwaredata_unk_7648 = inputs.calibration.hardwaredata_unk_7648;
        self.unk_7650 = crate::hw::t8142::HARDWAREDATA.unk_7650;
        self.hardwaredata_unk_7654 = inputs.calibration.hardwaredata_unk_7654;
        self.unk_765c = crate::hw::t8142::HARDWAREDATA.unk_765c;
        self.unk_7660 = crate::hw::t8142::HARDWAREDATA.unk_7660;
        self.unk_76a0 = crate::hw::t8142::HARDWAREDATA.unk_76a0;
        self.unk_76a8 = crate::hw::t8142::HARDWAREDATA.unk_76a8;
        self.unk_76b0 = crate::hw::t8142::HARDWAREDATA.unk_76b0;
        self.unk_76c4 = crate::hw::t8142::HARDWAREDATA.unk_76c4;
        self.unk_76cc = crate::hw::t8142::HARDWAREDATA.unk_76cc;
        self.unk_76d4 = crate::hw::t8142::HARDWAREDATA.unk_76d4;
        self.unk_76d8 = crate::hw::t8142::HARDWAREDATA.unk_76d8;
        self.hardwaredata_unk_76e8 = inputs.calibration.hardwaredata_unk_76e8;
        self.hardwaredata_unk_76f0 = inputs.calibration.hardwaredata_unk_76f0;
        self.unk_76fc = crate::hw::t8142::HARDWAREDATA.unk_76fc;
        self.hardwaredata_unk_7730 = inputs.calibration.hardwaredata_unk_7730;
        self.unk_7948 = crate::hw::t8142::HARDWAREDATA.unk_7948;
        self.unk_7b62 = crate::hw::t8142::HARDWAREDATA.unk_7b62;
        self.unk_7b64 = crate::hw::t8142::HARDWAREDATA.unk_7b64;
        self.unk_7b6a = crate::hw::t8142::HARDWAREDATA.unk_7b6a;
        self.unk_7b6c = crate::hw::t8142::HARDWAREDATA.unk_7b6c;
        self.unk_7b72 = crate::hw::t8142::HARDWAREDATA.unk_7b72;
        self.unk_7b74 = crate::hw::t8142::HARDWAREDATA.unk_7b74;
        self.unk_7b7a = crate::hw::t8142::HARDWAREDATA.unk_7b7a;
        self.unk_7b7c = crate::hw::t8142::HARDWAREDATA.unk_7b7c;
        self.unk_7b80 = crate::hw::t8142::HARDWAREDATA.unk_7b80;
        self.unk_7b84 = crate::hw::t8142::HARDWAREDATA.unk_7b84;
        self.unk_7b8a = crate::hw::t8142::HARDWAREDATA.unk_7b8a;
        self.unk_7b8c = crate::hw::t8142::HARDWAREDATA.unk_7b8c;
        self.unk_7b90 = crate::hw::t8142::HARDWAREDATA.unk_7b90;
        self.hardwaredata_unk_7b92 = inputs.calibration.hardwaredata_unk_7b92;
        self.unk_7b96 = crate::hw::t8142::HARDWAREDATA.unk_7b96;
        self.unk_7b98 = crate::hw::t8142::HARDWAREDATA.unk_7b98;
        self.unk_7b9e = crate::hw::t8142::HARDWAREDATA.unk_7b9e;
        self.unk_7ba0 = crate::hw::t8142::HARDWAREDATA.unk_7ba0;
        self.unk_7ba4 = crate::hw::t8142::HARDWAREDATA.unk_7ba4;
        self.hardwaredata_unk_7bd6 = inputs.calibration.hardwaredata_unk_7bd6;
        self.unk_7bf2 = crate::hw::t8142::HARDWAREDATA.unk_7bf2;
        self.unk_7bf4 = crate::hw::t8142::HARDWAREDATA.unk_7bf4;
        self.hardwaredata_unk_7ca8 = inputs.calibration.hardwaredata_unk_7ca8;
        self.unk_7cbc = crate::hw::t8142::HARDWAREDATA.unk_7cbc;
        self.unk_7d08 = crate::hw::t8142::HARDWAREDATA.unk_7d08;
        self.unk_7d0a = crate::hw::t8142::HARDWAREDATA.unk_7d0a;
        self.unk_7d0e = crate::hw::t8142::HARDWAREDATA.unk_7d0e;
        self.unk_7d10 = crate::hw::t8142::HARDWAREDATA.unk_7d10;
        self.unk_7d26 = crate::hw::t8142::HARDWAREDATA.unk_7d26;
        self.unk_7d28 = crate::hw::t8142::HARDWAREDATA.unk_7d28;
        self.unk_7d2a = crate::hw::t8142::HARDWAREDATA.unk_7d2a;
        self.unk_7d2e = crate::hw::t8142::HARDWAREDATA.unk_7d2e;
        self.unk_7d30 = crate::hw::t8142::HARDWAREDATA.unk_7d30;
        self.unk_7d46 = crate::hw::t8142::HARDWAREDATA.unk_7d46;
        self.unk_7d48 = crate::hw::t8142::HARDWAREDATA.unk_7d48;
        self.unk_7d4a = crate::hw::t8142::HARDWAREDATA.unk_7d4a;
        self.unk_7d4e = crate::hw::t8142::HARDWAREDATA.unk_7d4e;
        self.unk_7d50 = crate::hw::t8142::HARDWAREDATA.unk_7d50;
        self.unk_7d66 = crate::hw::t8142::HARDWAREDATA.unk_7d66;
        self.unk_7d68 = crate::hw::t8142::HARDWAREDATA.unk_7d68;
        self.unk_7e12 = crate::hw::t8142::HARDWAREDATA.unk_7e12;
        self.unk_7e14 = crate::hw::t8142::HARDWAREDATA.unk_7e14;
        self.hardwaredata_unk_7e18 = inputs.calibration.hardwaredata_unk_7e18;
        self.hardwaredata_unk_7e20 = inputs.calibration.hardwaredata_unk_7e20;
        self.hardwaredata_unk_7e24 = inputs.calibration.hardwaredata_unk_7e24;
        self.hardwaredata_unk_7e28 = inputs.calibration.hardwaredata_unk_7e28;
        self.unk_7e34 = crate::hw::t8142::HARDWAREDATA.unk_7e34;
        self.unk_7e38 = crate::hw::t8142::HARDWAREDATA.unk_7e38;
        self.hardwaredata_unk_7e40 = inputs.calibration.hardwaredata_unk_7e40;
        self.hardwaredata_unk_7e44 = inputs.calibration.hardwaredata_unk_7e44;
        self.hardwaredata_unk_7f14 = inputs.calibration.hardwaredata_unk_7f14;
        self.hardwaredata_unk_7f20 = inputs.calibration.hardwaredata_unk_7f20;
        self.hardwaredata_unk_7f28 = inputs.calibration.hardwaredata_unk_7f28;
        self.hardwaredata_unk_7f30 = inputs.calibration.hardwaredata_unk_7f30;
        self.unk_7f38 = crate::hw::t8142::HARDWAREDATA.unk_7f38;
        self.unk_7f40 = crate::hw::t8142::HARDWAREDATA.unk_7f40;
        self.hardwaredata_unk_7f4c = inputs.calibration.hardwaredata_unk_7f4c;
        self.hardwaredata_unk_7f54 = inputs.calibration.hardwaredata_unk_7f54;
        self.hardwaredata_unk_7f5c = inputs.calibration.hardwaredata_unk_7f5c;
        self.hardwaredata_unk_7f60 = inputs.calibration.hardwaredata_unk_7f60;
        self.hardwaredata_unk_7f6c = inputs.calibration.hardwaredata_unk_7f6c;
        self.hardwaredata_unk_7f74 = inputs.calibration.hardwaredata_unk_7f74;
        self.hardwaredata_unk_7f94 = inputs.calibration.hardwaredata_unk_7f94;
        self.unk_7fd4 = crate::hw::t8142::HARDWAREDATA.unk_7fd4;
        self.unk_80a4 = crate::hw::t8142::HARDWAREDATA.unk_80a4;
        self.unk_80a8 = crate::hw::t8142::HARDWAREDATA.unk_80a8;
        self.unk_80b0 = crate::hw::t8142::HARDWAREDATA.unk_80b0;
        self.unk_80b8 = crate::hw::t8142::HARDWAREDATA.unk_80b8;
        self.hardwaredata_unk_80c0 = inputs.calibration.hardwaredata_unk_80c0;
        self.hardwaredata_unk_80c4 = inputs.calibration.hardwaredata_unk_80c4;
        self.unk_80c8 = crate::hw::t8142::HARDWAREDATA.unk_80c8;
        self.unk_80d0 = crate::hw::t8142::HARDWAREDATA.unk_80d0;
        self.hardwaredata_unk_80dc = inputs.calibration.hardwaredata_unk_80dc;
        self.hardwaredata_unk_80e4 = inputs.calibration.hardwaredata_unk_80e4;
        self.hardwaredata_unk_80ec = inputs.calibration.hardwaredata_unk_80ec;
        self.unk_80f0 = crate::hw::t8142::HARDWAREDATA.unk_80f0;
        self.unk_80f8 = crate::hw::t8142::HARDWAREDATA.unk_80f8;
        self.hardwaredata_unk_80fc = inputs.calibration.hardwaredata_unk_80fc;
        self.hardwaredata_unk_8100 = inputs.calibration.hardwaredata_unk_8100;
        self.hardwaredata_unk_8104 = inputs.calibration.hardwaredata_unk_8104;
        self.hardwaredata_unk_810c = inputs.calibration.hardwaredata_unk_810c;
        self.unk_8118 = crate::hw::t8142::HARDWAREDATA.unk_8118;
        self.hardwaredata_unk_8140 = inputs.calibration.hardwaredata_unk_8140;
        self.hardwaredata_unk_8144 = inputs.calibration.hardwaredata_unk_8144;
        self.unk_8148 = crate::hw::t8142::HARDWAREDATA.unk_8148;
        self.unk_8150 = crate::hw::t8142::HARDWAREDATA.unk_8150;
        self.unk_8158 = crate::hw::t8142::HARDWAREDATA.unk_8158;
        self.unk_8188 = crate::hw::t8142::HARDWAREDATA.unk_8188;
        self.unk_8198 = crate::hw::t8142::HARDWAREDATA.unk_8198;
        self.hardwaredata_unk_819c = inputs.calibration.hardwaredata_unk_819c;
        self.unk_8268 = crate::hw::t8142::HARDWAREDATA.unk_8268;
        self.unk_8288 = crate::hw::t8142::HARDWAREDATA.unk_8288;
        self.unk_8290 = crate::hw::t8142::HARDWAREDATA.unk_8290;
        self.hardwaredata_unk_8334 = inputs.calibration.hardwaredata_unk_8334;
        self.hardwaredata_unk_8338 = inputs.calibration.hardwaredata_unk_8338;
        self.unk_833c = crate::hw::t8142::HARDWAREDATA.unk_833c;
        self.unk_8340 = crate::hw::t8142::HARDWAREDATA.unk_8340;
        self.unk_8348 = crate::hw::t8142::HARDWAREDATA.unk_8348;
        self.unk_8364 = crate::hw::t8142::HARDWAREDATA.unk_8364;
        self.unk_8370 = crate::hw::t8142::HARDWAREDATA.unk_8370;
        self.unk_8378 = crate::hw::t8142::HARDWAREDATA.unk_8378;
        self.unk_8380 = crate::hw::t8142::HARDWAREDATA.unk_8380;
        self.unk_8388 = crate::hw::t8142::HARDWAREDATA.unk_8388;
        self.unk_8390 = crate::hw::t8142::HARDWAREDATA.unk_8390;
        self.hardwaredata_unk_839c = inputs.calibration.hardwaredata_unk_839c;
        self.hardwaredata_unk_83a0 = inputs.calibration.hardwaredata_unk_83a0;
        self.hardwaredata_unk_83a4 = inputs.calibration.hardwaredata_unk_83a4;
        self.hardwaredata_unk_83b0 = inputs.calibration.hardwaredata_unk_83b0;
        self.hardwaredata_unk_83e4 = inputs.calibration.hardwaredata_unk_83e4;
        self.hardwaredata_unk_8430 = inputs.calibration.hardwaredata_unk_8430;
        self.unk_8448 = crate::hw::t8142::HARDWAREDATA.unk_8448;
        self.ptr_90b0 = crate::hw::t8142::OBJECTS[1].va + 0x1777c0;
        self.unk_b0c4 = crate::hw::t8142::HARDWAREDATA.unk_b0c4;
        self.unk_b0c8 = crate::hw::t8142::HARDWAREDATA.unk_b0c8;
        self.ptr_b858 = crate::hw::t8142::OBJECTS[3].va + 0x0;
        self.unk_b860 = crate::hw::t8142::HARDWAREDATA.unk_b860;
        self.unk_b8e0 = crate::hw::t8142::HARDWAREDATA.unk_b8e0;
        self.unk_b8f0 = crate::hw::t8142::HARDWAREDATA.unk_b8f0;
        self.unk_bf94 = crate::hw::t8142::HARDWAREDATA.unk_bf94;
        self.unk_bf98 = crate::hw::t8142::HARDWAREDATA.unk_bf98;
        self.unk_bfe0 = crate::hw::t8142::HARDWAREDATA.unk_bfe0;
        self.hardwaredata_unk_bfec = inputs.calibration.hardwaredata_unk_bfec;
        self.hardwaredata_unk_c02c = inputs.calibration.hardwaredata_unk_c02c;
        self.hardwaredata_unk_c06c = inputs.calibration.hardwaredata_unk_c06c;
        self.unk_c118 = crate::hw::t8142::HARDWAREDATA.unk_c118;
        self.ptr_1ee40 = crate::hw::t8142::OBJECTS[0].va + 0x0;
        self.ptr_1ee48 = crate::hw::t8142::OBJECTS[0].va + 0xcd80;
        self.ptr_1ee50 = crate::hw::t8142::OBJECTS[0].va + 0xcd80;
        self.ptr_1ee60 = crate::hw::t8142::OBJECTS[1].va + 0xc0;
        self.ptr_1ee68 = crate::hw::t8142::OBJECTS[1].va + 0xd0;
        self.ptr_1ee70 = crate::hw::t8142::OBJECTS[1].va + 0xe0;
        self.ptr_1ee78 = crate::hw::t8142::OBJECTS[0].va + 0x11640;
        self.ptr_1ee80 = crate::hw::t8142::OBJECTS[1].va + 0x1c0;
        self.ptr_1ee88 = crate::hw::t8142::OBJECTS[1].va + 0x1d0;
        self.ptr_1ee90 = crate::hw::t8142::OBJECTS[1].va + 0x1e0;
        self.ptr_1ee98 = crate::hw::t8142::OBJECTS[0].va + 0x17640;
        self.ptr_1eea0 = crate::hw::t8142::OBJECTS[1].va + 0x2c0;
        self.ptr_1eea8 = crate::hw::t8142::OBJECTS[1].va + 0x2d0;
        self.ptr_1eeb0 = crate::hw::t8142::OBJECTS[1].va + 0x2e0;
        self.ptr_1eeb8 = crate::hw::t8142::OBJECTS[0].va + 0x1d640;
        self.ptr_1eec0 = crate::hw::t8142::OBJECTS[1].va + 0x80;
        self.ptr_1eec8 = crate::hw::t8142::OBJECTS[1].va + 0x90;
        self.ptr_1eed0 = crate::hw::t8142::OBJECTS[1].va + 0xa0;
        self.ptr_1eed8 = crate::hw::t8142::OBJECTS[0].va + 0xfe40;
        self.ptr_1eee0 = crate::hw::t8142::OBJECTS[1].va + 0x180;
        self.ptr_1eee8 = crate::hw::t8142::OBJECTS[1].va + 0x190;
        self.ptr_1eef0 = crate::hw::t8142::OBJECTS[1].va + 0x1a0;
        self.ptr_1eef8 = crate::hw::t8142::OBJECTS[0].va + 0x15e40;
        self.ptr_1ef00 = crate::hw::t8142::OBJECTS[1].va + 0x280;
        self.ptr_1ef08 = crate::hw::t8142::OBJECTS[1].va + 0x290;
        self.ptr_1ef10 = crate::hw::t8142::OBJECTS[1].va + 0x2a0;
        self.ptr_1ef18 = crate::hw::t8142::OBJECTS[0].va + 0x1be40;
        self.ptr_1ef20 = crate::hw::t8142::OBJECTS[1].va + 0x40;
        self.ptr_1ef28 = crate::hw::t8142::OBJECTS[1].va + 0x50;
        self.ptr_1ef30 = crate::hw::t8142::OBJECTS[1].va + 0x60;
        self.ptr_1ef38 = crate::hw::t8142::OBJECTS[0].va + 0xe640;
        self.ptr_1ef40 = crate::hw::t8142::OBJECTS[1].va + 0x140;
        self.ptr_1ef48 = crate::hw::t8142::OBJECTS[1].va + 0x150;
        self.ptr_1ef50 = crate::hw::t8142::OBJECTS[1].va + 0x160;
        self.ptr_1ef58 = crate::hw::t8142::OBJECTS[0].va + 0x14640;
        self.ptr_1ef60 = crate::hw::t8142::OBJECTS[1].va + 0x240;
        self.ptr_1ef68 = crate::hw::t8142::OBJECTS[1].va + 0x250;
        self.ptr_1ef70 = crate::hw::t8142::OBJECTS[1].va + 0x260;
        self.ptr_1ef78 = crate::hw::t8142::OBJECTS[0].va + 0x1a640;
        self.ptr_1ef80 = crate::hw::t8142::OBJECTS[1].va + 0x0;
        self.ptr_1ef88 = crate::hw::t8142::OBJECTS[1].va + 0x10;
        self.ptr_1ef90 = crate::hw::t8142::OBJECTS[1].va + 0x20;
        self.ptr_1ef98 = crate::hw::t8142::OBJECTS[0].va + 0xce40;
        self.ptr_1efa0 = crate::hw::t8142::OBJECTS[1].va + 0x100;
        self.ptr_1efa8 = crate::hw::t8142::OBJECTS[1].va + 0x110;
        self.ptr_1efb0 = crate::hw::t8142::OBJECTS[1].va + 0x120;
        self.ptr_1efb8 = crate::hw::t8142::OBJECTS[0].va + 0x12e40;
        self.ptr_1efc0 = crate::hw::t8142::OBJECTS[1].va + 0x200;
        self.ptr_1efc8 = crate::hw::t8142::OBJECTS[1].va + 0x210;
        self.ptr_1efd0 = crate::hw::t8142::OBJECTS[1].va + 0x220;
        self.ptr_1efd8 = crate::hw::t8142::OBJECTS[0].va + 0x18e40;
        self.ptr_1efe0 = crate::hw::t8142::OBJECTS[1].va + 0x300;
        self.ptr_1efe8 = crate::hw::t8142::OBJECTS[1].va + 0x310;
        self.ptr_1eff0 = crate::hw::t8142::OBJECTS[1].va + 0x320;
        self.ptr_1eff8 = crate::hw::t8142::OBJECTS[0].va + 0x1f300;
        self.ptr_1f000 = crate::hw::t8142::OBJECTS[1].va + 0xf1c0;
        self.ptr_1f008 = crate::hw::t8142::OBJECTS[1].va + 0xf440;
        self.ptr_1f010 = crate::hw::t8142::OBJECTS[1].va + 0xf200;
        self.ptr_1f018 = crate::hw::t8142::OBJECTS[1].va + 0x13c40;
        self.ptr_1f020 = crate::hw::t8142::OBJECTS[1].va + 0xf3c0;
        self.ptr_1f028 = crate::hw::t8142::OBJECTS[1].va + 0xb5c40;
        self.ptr_1f030 = crate::hw::t8142::OBJECTS[1].va + 0xf400;
        self.ptr_1f038 = crate::hw::t8142::OBJECTS[1].va + 0xbec40;
        self.ptr_1f040 = crate::hw::t8142::OBJECTS[1].va + 0x3c440;
        self.ptr_1f094 = crate::hw::t8142::OBJECTS[0].va + 0x2880;
        self.ptr_1f09c = crate::hw::t8142::OBJECTS[0].va + 0x34c0;
        self.ptr_1f0a4 = crate::hw::t8142::OBJECTS[0].va + 0x4540;
        self.ptr_1f0ac = crate::hw::t8142::OBJECTS[0].va + 0xc500;
        self.ptr_1f0b4 = crate::hw::t8142::OBJECTS[0].va + 0xc580;
        self.ptr_1f128 = crate::hw::t8142::OBJECTS[10].aliases[0] + 0x0;
        self.ptr_1f130 = crate::hw::t8142::OBJECTS[10].va + 0x0;
        self.unk_1f220 = crate::hw::t8142::HARDWAREDATA.unk_1f220;
        self.unk_1f300 = crate::hw::t8142::HARDWAREDATA.unk_1f300;
        self.ptr_23300 = crate::hw::t8142::OBJECTS[0].va + 0x0;
        self.ptr_23308 = crate::hw::t8142::OBJECTS[0].va + 0xcd80;
        self.ptr_23310 = crate::hw::t8142::OBJECTS[0].va + 0xcd80;
        self.ptr_234a0 = crate::hw::t8142::OBJECTS[1].va + 0x177700;
        self.ptr_234a8 = crate::hw::t8142::OBJECTS[1].va + 0x177710;
        self.ptr_234b0 = crate::hw::t8142::OBJECTS[1].va + 0x177720;
        self.ptr_234b8 = crate::hw::t8142::OBJECTS[0].va + 0x237c0;
        self.ptr_234c0 = crate::hw::t8142::OBJECTS[1].va + 0xc3480;
        self.ptr_234c8 = crate::hw::t8142::OBJECTS[1].va + 0xc3700;
        self.ptr_234d0 = crate::hw::t8142::OBJECTS[1].va + 0xc34c0;
        self.ptr_234d8 = crate::hw::t8142::OBJECTS[1].va + 0xc7f00;
        self.ptr_234e0 = crate::hw::t8142::OBJECTS[1].va + 0xc3680;
        self.ptr_234e8 = crate::hw::t8142::OBJECTS[1].va + 0x169f00;
        self.ptr_234f0 = crate::hw::t8142::OBJECTS[1].va + 0xc36c0;
        self.ptr_234f8 = crate::hw::t8142::OBJECTS[1].va + 0x172f00;
        self.ptr_23500 = crate::hw::t8142::OBJECTS[1].va + 0xf0700;
        self.ptr_235e8 = crate::hw::t8142::OBJECTS[10].aliases[0] + 0x0;
        self.ptr_235f0 = crate::hw::t8142::OBJECTS[10].va + 0x0;
        self.ptr_23781 = crate::hw::t8142::OBJECTS[0].va + 0x5380;
        self.unk_237c0 = crate::hw::t8142::HARDWAREDATA.unk_237c0;
    }
}

#[repr(C, packed)]
pub(crate) struct PrivateData {
    pub(crate) pad_0000: [u8; 0x320],
    pub(crate) unk_0320: u32,
    pub(crate) pad_0324: [u8; 0xe730],
    pub(crate) unk_ea54: u32,
    pub(crate) pad_ea58: [u8; 0x8],
    pub(crate) ptr_ea60: u64,
    pub(crate) pad_ea68: [u8; 0x28],
    pub(crate) unk_ea90: u32,
    pub(crate) unk_ea94: u32,
    pub(crate) unk_ea98: u64,
    pub(crate) unk_eaa0: u32,
    pub(crate) pad_eaa4: [u8; 0xa4],
    pub(crate) unk_eb48: u64,
    pub(crate) pad_eb50: [u8; 0x8],
    pub(crate) unk_eb58: u64,
    pub(crate) pad_eb60: [u8; 0x10],
    pub(crate) unk_eb70: u64,
    pub(crate) pad_eb78: [u8; 0x10],
    pub(crate) unk_eb88: u64,
    pub(crate) unk_eb90: u32,
    pub(crate) pad_eb94: [u8; 0x14],
    pub(crate) unk_eba8: u32,
    pub(crate) unk_ebac: u32,
    pub(crate) unk_ebb0: u32,
    pub(crate) unk_ebb4: u32,
    pub(crate) pad_ebb8: [u8; 0x20],
    pub(crate) unk_ebd8: u32,
    pub(crate) pad_ebdc: [u8; 0x64],
    pub(crate) unk_ec40: u32,
    pub(crate) pad_ec44: [u8; 0x4],
    pub(crate) unk_ec48: u64,
    pub(crate) unk_ec50: u64,
    pub(crate) unk_ec58: u64,
    pub(crate) pad_ec60: [u8; 0x3bc],
    pub(crate) unk_f01c: u32,
    pub(crate) unk_f020: u64,
    pub(crate) unk_f028: u64,
    pub(crate) unk_f030: u32,
    pub(crate) pad_f034: [u8; 0x30],
    pub(crate) unk_f064: u32,
    pub(crate) unk_f068: u32,
    pub(crate) pad_f06c: [u8; 0x8],
    pub(crate) unk_f074: u32,
    pub(crate) unk_f078: u64,
    pub(crate) unk_f080: u64,
    pub(crate) unk_f088: u64,
    pub(crate) unk_f090: u32,
    pub(crate) pad_f094: [u8; 0x4],
    pub(crate) unk_f098: u64,
    pub(crate) unk_f0a0: u64,
    pub(crate) unk_f0a8: u32,
    pub(crate) pad_f0ac: [u8; 0xc],
    pub(crate) unk_f0b8: u32,
    pub(crate) pad_f0bc: [u8; 0x88],
    pub(crate) unk_f144: u32,
    pub(crate) ptr_f148: u64,
    pub(crate) pad_f150: [u8; 0x34],
    pub(crate) unk_f184: u32,
    pub(crate) pad_f188: [u8; 0xb42bc],
    pub(crate) unk_c3444: u32,
    pub(crate) pad_c3448: [u8; 0xb42d8],
    pub(crate) unk_177720: u32,
    pub(crate) pad_177724: [u8; 0x28],
    pub(crate) unk_17774c: u32,
    pub(crate) unk_177750: u64,
    pub(crate) unk_177758: u32,
    pub(crate) pad_17775c: [u8; 0x10],
    pub(crate) unk_17776c: u32,
    pub(crate) unk_177770: u32,
    pub(crate) pad_177774: [u8; 0x8],
    pub(crate) unk_17777c: u32,
    pub(crate) unk_177780: u32,
    pub(crate) pad_177784: [u8; 0x8],
    pub(crate) unk_17778c: u32,
    pub(crate) unk_177790: u32,
    pub(crate) pad_177794: [u8; 0x86c],
}
static_assert!(core::mem::size_of::<PrivateData>() == 0x178000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for PrivateData {}

pub(crate) struct PrivateDataConstants {
    pub(crate) unk_0320: u32,
    pub(crate) unk_ea54: u32,
    pub(crate) unk_ea98: u64,
    pub(crate) unk_eaa0: u32,
    pub(crate) unk_eb48: u64,
    pub(crate) unk_eb58: u64,
    pub(crate) unk_eb70: u64,
    pub(crate) unk_eb88: u64,
    pub(crate) unk_eb90: u32,
    pub(crate) unk_eba8: u32,
    pub(crate) unk_ebb0: u32,
    pub(crate) unk_ebd8: u32,
    pub(crate) unk_ec40: u32,
    pub(crate) unk_ec48: u64,
    pub(crate) unk_ec50: u64,
    pub(crate) unk_ec58: u64,
    pub(crate) unk_f01c: u32,
    pub(crate) unk_f020: u64,
    pub(crate) unk_f028: u64,
    pub(crate) unk_f030: u32,
    pub(crate) unk_f064: u32,
    pub(crate) unk_f068: u32,
    pub(crate) unk_f074: u32,
    pub(crate) unk_f078: u64,
    pub(crate) unk_f080: u64,
    pub(crate) unk_f088: u64,
    pub(crate) unk_f090: u32,
    pub(crate) unk_f098: u64,
    pub(crate) unk_f0a0: u64,
    pub(crate) unk_f144: u32,
    pub(crate) unk_f184: u32,
    pub(crate) unk_c3444: u32,
    pub(crate) unk_177720: u32,
    pub(crate) unk_17774c: u32,
    pub(crate) unk_177750: u64,
    pub(crate) unk_177758: u32,
    pub(crate) unk_17776c: u32,
    pub(crate) unk_177770: u32,
    pub(crate) unk_17777c: u32,
    pub(crate) unk_177780: u32,
    pub(crate) unk_17778c: u32,
    pub(crate) unk_177790: u32,
}

impl PrivateData {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.unk_0320 = crate::hw::t8142::PRIVATEDATA.unk_0320;
        self.unk_ea54 = crate::hw::t8142::PRIVATEDATA.unk_ea54;
        self.ptr_ea60 = crate::hw::t8142::OBJECTS[1].va + 0xf144;
        self.unk_ea90 = inputs.calibration.privatedata_unk_ea90;
        self.unk_ea94 = inputs.calibration.privatedata_unk_ea94;
        self.unk_ea98 = crate::hw::t8142::PRIVATEDATA.unk_ea98;
        self.unk_eaa0 = crate::hw::t8142::PRIVATEDATA.unk_eaa0;
        self.unk_eb48 = crate::hw::t8142::PRIVATEDATA.unk_eb48;
        self.unk_eb58 = crate::hw::t8142::PRIVATEDATA.unk_eb58;
        self.unk_eb70 = crate::hw::t8142::PRIVATEDATA.unk_eb70;
        self.unk_eb88 = crate::hw::t8142::PRIVATEDATA.unk_eb88;
        self.unk_eb90 = crate::hw::t8142::PRIVATEDATA.unk_eb90;
        self.unk_eba8 = crate::hw::t8142::PRIVATEDATA.unk_eba8;
        self.unk_ebac = inputs.calibration.privatedata_unk_ebac;
        self.unk_ebb0 = crate::hw::t8142::PRIVATEDATA.unk_ebb0;
        self.unk_ebb4 = inputs.calibration.privatedata_unk_ebb4;
        self.unk_ebd8 = crate::hw::t8142::PRIVATEDATA.unk_ebd8;
        self.unk_ec40 = crate::hw::t8142::PRIVATEDATA.unk_ec40;
        self.unk_ec48 = crate::hw::t8142::PRIVATEDATA.unk_ec48;
        self.unk_ec50 = crate::hw::t8142::PRIVATEDATA.unk_ec50;
        self.unk_ec58 = crate::hw::t8142::PRIVATEDATA.unk_ec58;
        self.unk_f01c = crate::hw::t8142::PRIVATEDATA.unk_f01c;
        self.unk_f020 = crate::hw::t8142::PRIVATEDATA.unk_f020;
        self.unk_f028 = crate::hw::t8142::PRIVATEDATA.unk_f028;
        self.unk_f030 = crate::hw::t8142::PRIVATEDATA.unk_f030;
        self.unk_f064 = crate::hw::t8142::PRIVATEDATA.unk_f064;
        self.unk_f068 = crate::hw::t8142::PRIVATEDATA.unk_f068;
        self.unk_f074 = crate::hw::t8142::PRIVATEDATA.unk_f074;
        self.unk_f078 = crate::hw::t8142::PRIVATEDATA.unk_f078;
        self.unk_f080 = crate::hw::t8142::PRIVATEDATA.unk_f080;
        self.unk_f088 = crate::hw::t8142::PRIVATEDATA.unk_f088;
        self.unk_f090 = crate::hw::t8142::PRIVATEDATA.unk_f090;
        self.unk_f098 = crate::hw::t8142::PRIVATEDATA.unk_f098;
        self.unk_f0a0 = crate::hw::t8142::PRIVATEDATA.unk_f0a0;
        self.unk_f0a8 = inputs.calibration.privatedata_unk_f0a8;
        self.unk_f0b8 = inputs.calibration.privatedata_unk_f0b8;
        self.unk_f144 = crate::hw::t8142::PRIVATEDATA.unk_f144;
        self.ptr_f148 = crate::hw::t8142::OBJECTS[1].va + 0xea5c;
        self.unk_f184 = crate::hw::t8142::PRIVATEDATA.unk_f184;
        self.unk_c3444 = crate::hw::t8142::PRIVATEDATA.unk_c3444;
        self.unk_177720 = crate::hw::t8142::PRIVATEDATA.unk_177720;
        self.unk_17774c = crate::hw::t8142::PRIVATEDATA.unk_17774c;
        self.unk_177750 = crate::hw::t8142::PRIVATEDATA.unk_177750;
        self.unk_177758 = crate::hw::t8142::PRIVATEDATA.unk_177758;
        self.unk_17776c = crate::hw::t8142::PRIVATEDATA.unk_17776c;
        self.unk_177770 = crate::hw::t8142::PRIVATEDATA.unk_177770;
        self.unk_17777c = crate::hw::t8142::PRIVATEDATA.unk_17777c;
        self.unk_177780 = crate::hw::t8142::PRIVATEDATA.unk_177780;
        self.unk_17778c = crate::hw::t8142::PRIVATEDATA.unk_17778c;
        self.unk_177790 = crate::hw::t8142::PRIVATEDATA.unk_177790;
    }
}

#[repr(C, packed)]
pub(crate) struct RegionC {
    pub(crate) pad_0000: [u8; 0x24],
    pub(crate) unk_0024: u32,
    pub(crate) pad_0028: [u8; 0x18],
    pub(crate) unk_0040: u32,
    pub(crate) pad_0044: [u8; 0x14],
    pub(crate) unk_0058: u32,
    pub(crate) unk_005c: u16,
    pub(crate) pad_005e: [u8; 0x4],
    pub(crate) unk_0062: u16,
    pub(crate) unk_0064: u32,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u16,
    pub(crate) pad_0072: [u8; 0x16],
    pub(crate) unk_0088: u32,
    pub(crate) unk_008c: u32,
    pub(crate) unk_0090: u32,
    pub(crate) pad_0094: [u8; 0x8],
    pub(crate) unk_009c: u32,
    pub(crate) pad_00a0: [u8; 0x728],
    pub(crate) unk_07c8: u64,
    pub(crate) unk_07d0: u64,
    pub(crate) unk_07d8: u64,
    pub(crate) unk_07e0: u64,
    pub(crate) unk_07e8: u64,
    pub(crate) unk_07f0: u64,
    pub(crate) pad_07f8: [u8; 0x1b4],
    pub(crate) unk_09ac: u32,
    pub(crate) unk_09b0: u32,
    pub(crate) pad_09b4: [u8; 0x8],
    pub(crate) unk_09bc: u32,
    pub(crate) unk_09c0: u64,
    pub(crate) pad_09c8: [u8; 0x4],
    pub(crate) unk_09cc: u32,
    pub(crate) unk_09d0: u64,
    pub(crate) pad_09d8: [u8; 0x4],
    pub(crate) unk_09dc: u32,
    pub(crate) pad_09e0: [u8; 0x441],
    pub(crate) unk_0e21: u8,
    pub(crate) unk_0e22: u16,
    pub(crate) unk_0e24: u8,
    pub(crate) pad_0e25: [u8; 0x38],
    pub(crate) unk_0e5d: u8,
    pub(crate) unk_0e5e: u16,
    pub(crate) unk_0e60: u32,
    pub(crate) unk_0e64: u8,
    pub(crate) pad_0e65: [u8; 0x319b],
}
static_assert!(core::mem::size_of::<RegionC>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for RegionC {}

pub(crate) struct RegionCConstants {
    pub(crate) unk_0024: u32,
    pub(crate) unk_0040: u32,
    pub(crate) unk_0058: u32,
    pub(crate) unk_005c: u16,
    pub(crate) unk_0062: u16,
    pub(crate) unk_0064: u32,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u16,
    pub(crate) unk_008c: u32,
    pub(crate) unk_0090: u32,
    pub(crate) unk_009c: u32,
    pub(crate) unk_07c8: u64,
    pub(crate) unk_07d0: u64,
    pub(crate) unk_07d8: u64,
    pub(crate) unk_07e0: u64,
    pub(crate) unk_07e8: u64,
    pub(crate) unk_07f0: u64,
    pub(crate) unk_09ac: u32,
    pub(crate) unk_09b0: u32,
    pub(crate) unk_09bc: u32,
    pub(crate) unk_09c0: u64,
    pub(crate) unk_09cc: u32,
    pub(crate) unk_09d0: u64,
    pub(crate) unk_09dc: u32,
    pub(crate) unk_0e21: u8,
    pub(crate) unk_0e22: u16,
    pub(crate) unk_0e24: u8,
    pub(crate) unk_0e5d: u8,
    pub(crate) unk_0e5e: u16,
    pub(crate) unk_0e60: u32,
    pub(crate) unk_0e64: u8,
}

impl RegionC {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.unk_0024 = crate::hw::t8142::REGIONC.unk_0024;
        self.unk_0040 = crate::hw::t8142::REGIONC.unk_0040;
        self.unk_0058 = crate::hw::t8142::REGIONC.unk_0058;
        self.unk_005c = crate::hw::t8142::REGIONC.unk_005c;
        self.unk_0062 = crate::hw::t8142::REGIONC.unk_0062;
        self.unk_0064 = crate::hw::t8142::REGIONC.unk_0064;
        self.unk_0068 = crate::hw::t8142::REGIONC.unk_0068;
        self.unk_0070 = crate::hw::t8142::REGIONC.unk_0070;
        self.unk_0088 = inputs.calibration.regionc_unk_0088;
        self.unk_008c = crate::hw::t8142::REGIONC.unk_008c;
        self.unk_0090 = crate::hw::t8142::REGIONC.unk_0090;
        self.unk_009c = crate::hw::t8142::REGIONC.unk_009c;
        self.unk_07c8 = crate::hw::t8142::REGIONC.unk_07c8;
        self.unk_07d0 = crate::hw::t8142::REGIONC.unk_07d0;
        self.unk_07d8 = crate::hw::t8142::REGIONC.unk_07d8;
        self.unk_07e0 = crate::hw::t8142::REGIONC.unk_07e0;
        self.unk_07e8 = crate::hw::t8142::REGIONC.unk_07e8;
        self.unk_07f0 = crate::hw::t8142::REGIONC.unk_07f0;
        self.unk_09ac = crate::hw::t8142::REGIONC.unk_09ac;
        self.unk_09b0 = crate::hw::t8142::REGIONC.unk_09b0;
        self.unk_09bc = crate::hw::t8142::REGIONC.unk_09bc;
        self.unk_09c0 = crate::hw::t8142::REGIONC.unk_09c0;
        self.unk_09cc = crate::hw::t8142::REGIONC.unk_09cc;
        self.unk_09d0 = crate::hw::t8142::REGIONC.unk_09d0;
        self.unk_09dc = crate::hw::t8142::REGIONC.unk_09dc;
        self.unk_0e21 = crate::hw::t8142::REGIONC.unk_0e21;
        self.unk_0e22 = crate::hw::t8142::REGIONC.unk_0e22;
        self.unk_0e24 = crate::hw::t8142::REGIONC.unk_0e24;
        self.unk_0e5d = crate::hw::t8142::REGIONC.unk_0e5d;
        self.unk_0e5e = crate::hw::t8142::REGIONC.unk_0e5e;
        self.unk_0e60 = crate::hw::t8142::REGIONC.unk_0e60;
        self.unk_0e64 = crate::hw::t8142::REGIONC.unk_0e64;
    }
}

#[repr(C, packed)]
pub(crate) struct ZeroPage {
    pub(crate) pad_0000: [u8; 0x4000],
}
static_assert!(core::mem::size_of::<ZeroPage>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for ZeroPage {}

impl ZeroPage {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
    }
}

#[repr(C, packed)]
pub(crate) struct ComputeGlobal {
    pub(crate) pad_0000: [u8; 0x4000],
}
static_assert!(core::mem::size_of::<ComputeGlobal>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for ComputeGlobal {}

impl ComputeGlobal {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
    }
}

#[repr(C, packed)]
pub(crate) struct PoolDescriptors {
    pub(crate) pad_0000: [u8; 0x60],
    pub(crate) bootstrap_pool: super::PagePoolDescriptor,
    pub(crate) pad_0080: [u8; 0x3f80],
}
static_assert!(core::mem::size_of::<PoolDescriptors>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for PoolDescriptors {}

impl PoolDescriptors {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.bootstrap_pool =
            super::PagePoolDescriptor::g17g(0x7010000000, 0, 5376, 0, 5376, 0x40000);
    }
}

#[repr(C, packed)]
pub(crate) struct RegionA {
    pub(crate) pad_0000: [u8; 0x4000],
}
static_assert!(core::mem::size_of::<RegionA>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for RegionA {}

impl RegionA {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
    }
}

#[repr(C, packed)]
pub(crate) struct PrimaryRoot {
    pub(crate) unk_0000: u64,
    pub(crate) ptr_0008: u64,
    pub(crate) pad_0010: [u8; 0x8],
    pub(crate) ptr_0018: u64,
    pub(crate) ptr_0020: u64,
    pub(crate) pad_0028: [u8; 0x4],
    pub(crate) unk_002c: u32,
    pub(crate) unk_0030: u64,
    pub(crate) unk_0038: u64,
    pub(crate) unk_0040: u64,
    pub(crate) unk_0048: u64,
    pub(crate) unk_0050: u64,
    pub(crate) unk_0058: u64,
    pub(crate) unk_0060: u64,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u64,
    pub(crate) unk_0078: u64,
    pub(crate) unk_0080: u64,
    pub(crate) unk_0088: u64,
    pub(crate) pad_0090: [u8; 0x18],
    pub(crate) ptr_00a8: u64,
    pub(crate) ptr_00b0: u64,
    pub(crate) pad_00b8: [u8; 0x3f48],
}
static_assert!(core::mem::size_of::<PrimaryRoot>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for PrimaryRoot {}

pub(crate) struct PrimaryRootConstants {
    pub(crate) unk_0000: u64,
    pub(crate) unk_002c: u32,
    pub(crate) unk_0030: u64,
    pub(crate) unk_0038: u64,
    pub(crate) unk_0040: u64,
    pub(crate) unk_0048: u64,
    pub(crate) unk_0050: u64,
    pub(crate) unk_0058: u64,
    pub(crate) unk_0060: u64,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u64,
    pub(crate) unk_0078: u64,
    pub(crate) unk_0080: u64,
    pub(crate) unk_0088: u64,
}

impl PrimaryRoot {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.unk_0000 = crate::hw::t8142::PRIMARYROOT.unk_0000;
        self.ptr_0008 = crate::hw::t8142::OBJECTS[14].va + 0x0;
        self.ptr_0018 = crate::hw::t8142::OBJECTS[0].va + 0x1ee40;
        self.ptr_0020 = crate::hw::t8142::OBJECTS[2].va + 0x0;
        self.unk_002c = crate::hw::t8142::PRIMARYROOT.unk_002c;
        self.unk_0030 = crate::hw::t8142::PRIMARYROOT.unk_0030;
        self.unk_0038 = crate::hw::t8142::PRIMARYROOT.unk_0038;
        self.unk_0040 = crate::hw::t8142::PRIMARYROOT.unk_0040;
        self.unk_0048 = crate::hw::t8142::PRIMARYROOT.unk_0048;
        self.unk_0050 = crate::hw::t8142::PRIMARYROOT.unk_0050;
        self.unk_0058 = crate::hw::t8142::PRIMARYROOT.unk_0058;
        self.unk_0060 = crate::hw::t8142::PRIMARYROOT.unk_0060;
        self.unk_0068 = crate::hw::t8142::PRIMARYROOT.unk_0068;
        self.unk_0070 = crate::hw::t8142::PRIMARYROOT.unk_0070;
        self.unk_0078 = crate::hw::t8142::PRIMARYROOT.unk_0078;
        self.unk_0080 = crate::hw::t8142::PRIMARYROOT.unk_0080;
        self.unk_0088 = crate::hw::t8142::PRIMARYROOT.unk_0088;
        self.ptr_00a8 = crate::hw::t8142::OBJECTS[1].va + 0xf180;
        self.ptr_00b0 = crate::hw::t8142::OBJECTS[1].va + 0x340;
    }
}

#[repr(C, packed)]
pub(crate) struct SecondaryRoot {
    pub(crate) unk_0000: u64,
    pub(crate) ptr_0008: u64,
    pub(crate) pad_0010: [u8; 0x8],
    pub(crate) ptr_0018: u64,
    pub(crate) ptr_0020: u64,
    pub(crate) unk_0028: u64,
    pub(crate) unk_0030: u64,
    pub(crate) unk_0038: u64,
    pub(crate) unk_0040: u64,
    pub(crate) unk_0048: u64,
    pub(crate) unk_0050: u64,
    pub(crate) unk_0058: u64,
    pub(crate) unk_0060: u64,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u64,
    pub(crate) unk_0078: u64,
    pub(crate) unk_0080: u64,
    pub(crate) unk_0088: u64,
    pub(crate) pad_0090: [u8; 0x18],
    pub(crate) ptr_00a8: u64,
    pub(crate) pad_00b0: [u8; 0x8],
    pub(crate) ptr_00b8: u64,
    pub(crate) ptr_00c0: u64,
    pub(crate) pad_00c8: [u8; 0x3f38],
}
static_assert!(core::mem::size_of::<SecondaryRoot>() == 0x4000);
// SAFETY: All fields are integer firmware records.
unsafe impl Zeroable for SecondaryRoot {}

pub(crate) struct SecondaryRootConstants {
    pub(crate) unk_0000: u64,
    pub(crate) unk_0028: u64,
    pub(crate) unk_0030: u64,
    pub(crate) unk_0038: u64,
    pub(crate) unk_0040: u64,
    pub(crate) unk_0048: u64,
    pub(crate) unk_0050: u64,
    pub(crate) unk_0058: u64,
    pub(crate) unk_0060: u64,
    pub(crate) unk_0068: u64,
    pub(crate) unk_0070: u64,
    pub(crate) unk_0078: u64,
    pub(crate) unk_0080: u64,
    pub(crate) unk_0088: u64,
}

impl SecondaryRoot {
    pub(crate) fn init(&mut self, inputs: &crate::g17::hal300::Inputs) {
        let _ = inputs;
        self.unk_0000 = crate::hw::t8142::SECONDARYROOT.unk_0000;
        self.ptr_0008 = crate::hw::t8142::OBJECTS[14].va + 0x0;
        self.ptr_0018 = crate::hw::t8142::OBJECTS[0].va + 0x23300;
        self.ptr_0020 = crate::hw::t8142::OBJECTS[2].va + 0x0;
        self.unk_0028 = crate::hw::t8142::SECONDARYROOT.unk_0028;
        self.unk_0030 = crate::hw::t8142::SECONDARYROOT.unk_0030;
        self.unk_0038 = crate::hw::t8142::SECONDARYROOT.unk_0038;
        self.unk_0040 = crate::hw::t8142::SECONDARYROOT.unk_0040;
        self.unk_0048 = crate::hw::t8142::SECONDARYROOT.unk_0048;
        self.unk_0050 = crate::hw::t8142::SECONDARYROOT.unk_0050;
        self.unk_0058 = crate::hw::t8142::SECONDARYROOT.unk_0058;
        self.unk_0060 = crate::hw::t8142::SECONDARYROOT.unk_0060;
        self.unk_0068 = crate::hw::t8142::SECONDARYROOT.unk_0068;
        self.unk_0070 = crate::hw::t8142::SECONDARYROOT.unk_0070;
        self.unk_0078 = crate::hw::t8142::SECONDARYROOT.unk_0078;
        self.unk_0080 = crate::hw::t8142::SECONDARYROOT.unk_0080;
        self.unk_0088 = crate::hw::t8142::SECONDARYROOT.unk_0088;
        self.ptr_00a8 = crate::hw::t8142::OBJECTS[1].va + 0xc3440;
        self.ptr_00b8 = crate::hw::t8142::OBJECTS[1].va + 0xea80;
        self.ptr_00c0 = crate::hw::t8142::OBJECTS[1].va + 0x177740;
    }
}

pub(crate) fn fill(
    kind: Object,
    object: &mut crate::g17::object::KernelObject,
    inputs: &crate::g17::hal300::Inputs,
) -> Result {
    match kind {
        Object::HardwareData => object.initialize::<HardwareData>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::PrivateData => object.initialize::<PrivateData>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::RegionC => object.initialize::<RegionC>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::ZeroPage => object.initialize::<ZeroPage>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::ComputeGlobal => object.initialize::<ComputeGlobal>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::PoolDescriptors => object.initialize::<PoolDescriptors>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::RegionA => object.initialize::<RegionA>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::PrimaryRoot => object.initialize::<PrimaryRoot>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::SecondaryRoot => object.initialize::<SecondaryRoot>(0, |value| {
            value.init(inputs);
            Ok(())
        }),
        Object::ComputePool0 => {
            for block in 0..21usize {
                let address = 0x7010210000u64 + block as u64 * 0x100000;
                for page in 0..256usize {
                    object.write((block * 256 + page) * 8, address + page as u64 * 0x1000)?;
                }
                object.write(0x200000 + block * 8, address | (256u64 << 52))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
