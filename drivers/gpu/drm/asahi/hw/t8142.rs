// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Hardware configuration for t8142 platforms (M5).
//!
//! The M5 GPU (G17G C0, one cluster of ten cores) uses the HAL300 firmware
//! interface. Only its identity and the layout the j704 RAM runtime has
//! verified on hardware are described here. The G17 backend starts the firmware
//! from the startup graph of the RAM runtime (see `g17::hal300`).

#[cfg(CONFIG_DRM_ASAHI_G16)]
use crate::g17::hal300::{
    FirmwareImage,
    Hal300Config, //
};
use crate::g17::{
    Config,
    FixedBuffer,
    Identity, //
};
use crate::hw::GpuVariant;

/// Static configuration of the T8142 GPU.
pub(crate) const CONFIG: Config = Config {
    chip_id: CHIP_ID,
    identity: Identity {
        // ID register 0x0b022000: family 0x0b, variant 2, revision C0.
        family: 0xb,
        variant: 2,
        num_dies: 1,
        gpu_variant: GpuVariant::G,
        usc_generation: 3,
        hal_generation: 300,
    },
    uat_ias: 42,
    uat_oas: 42,
    // The secondary instance's kernel root follows the primary one's 256 KiB
    // in the 512 KiB `pagetables` region, as on T8140.
    secondary_root_offset: 0x40000,
    // HAL300 serves the crash log buffers the firmware places in its own
    // __DATA_SHARED_RO segment (the `crashlog0`/`crashlog1` regions); these
    // are not used.
    crash_buffers: [
        FixedBuffer {
            va: 0x100_0250_4000,
            size: 0x8000,
        },
        FixedBuffer {
            va: 0x100_025c_0000,
            size: 0x8000,
        },
    ],
    pm_metrics: FixedBuffer {
        va: 0x10_0008_0000,
        size: 0x8000,
    },
    #[cfg(CONFIG_DRM_ASAHI_G16)]
    hal300: Some(&HAL300),
};

/// The qualified 26A434 firmware pair and the register ranges its startup graph may map: the SGX
/// block and the two power-telemetry pages (`PushTelemetryDashboard{,Read}Regs`).
#[cfg(CONFIG_DRM_ASAHI_G16)]
static HAL300: Hal300Config = Hal300Config {
    firmware: [
        FirmwareImage {
            name: "gfx-asc",
            text_size: 0x50000,
            data_size: 0xc0000,
            entropy: [302352, 302579],
            sha256: [
            0x85, 0x31, 0x4b, 0xcc, 0x08, 0x3a, 0xcc, 0x5a,
            0xaf, 0xfd, 0xb7, 0xcf, 0xc5, 0xb9, 0xb8, 0x31,
            0xbe, 0x69, 0xa0, 0x8a, 0xeb, 0x42, 0xd6, 0x75,
            0xe5, 0x9c, 0xcb, 0xad, 0x17, 0x0f, 0x6a, 0xdd,
            ],
        },
        FirmwareImage {
            name: "gfx1-asc",
            text_size: 0x4c000,
            data_size: 0xb4000,
            entropy: [301008, 301235],
            sha256: [
            0x08, 0xdc, 0x4b, 0xe7, 0xa2, 0xa1, 0x3f, 0xc1,
            0x60, 0xe3, 0x49, 0xdd, 0x93, 0x16, 0x23, 0xb9,
            0x97, 0x68, 0xbe, 0x3e, 0x61, 0xae, 0x6e, 0xad,
            0x6b, 0x94, 0x77, 0x96, 0x18, 0xf0, 0x04, 0xf2,
            ],
        },
    ],
    registers: &[
        0x5_8000_0000..0x5_8400_0000,
        0x3_803d_0000..0x3_803d_4000,
        0x3_803c_0000..0x3_803c_4000,
        // IOMapping slot 14, PMSMetrologySensorRegs (ADT pmgr reg[25]), read-only.
        0x3_8028_0000..0x3_8028_8000,
    ],
};

/// Chip identifier written to the hardware data.
pub(crate) const CHIP_ID: u32 = 0x8142;
