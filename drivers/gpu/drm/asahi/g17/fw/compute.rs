// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Compute work descriptor.
//!
//! A compute kick launches one [`ComputeDescriptor`]. The descriptor embeds
//! the two register arrays its kick entry binds: the main array and a
//! four-entry array that writes the operand-state registers with their
//! regions exchanged.

use kernel::prelude::*;

use super::kick::{KickTimestamp, RegisterArray, RegisterArrayBinding, RegisterWrite};
use super::queue::CommandTag;
use super::{
    clear, offset_va, sampler_max, stamp_value, timestamp_end_va, work_key, ObjectIds, UNK_0A599,
    UNK_0D411, UNK_101D9, UNK_10791, UNK_1A0E9, UNK_1C8F8,
};

/// Size of a compute descriptor.
pub(crate) const COMPUTE_DESCRIPTOR_SIZE: usize = 0x4000;
/// QoS class of compute kicks.
pub(crate) const COMPUTE_QOS_CLASS: u8 = 0x08;
/// Event mask of compute kicks.
pub(crate) const COMPUTE_KICK_EVENT_MASK: [u64; 4] = [0, 0, 4, 0];

const COMPUTE_REGISTERS: usize = 41;
const COMPUTE_REGISTERS_MAX: usize = COMPUTE_REGISTERS + 1;

/// The G17G (M5, HAL300) register program instead of the G17P one; selected once at boot.
static G17G: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Selects the G17G compute register program for this device.
pub(crate) fn select_g17g() {
    G17G.store(true, core::sync::atomic::Ordering::Relaxed);
}

fn g17g() -> bool {
    G17G.load(core::sync::atomic::Ordering::Relaxed)
}

/// Whether this device runs the G17G (M5, HAL300) compute protocol.
pub(crate) fn g17g_selected() -> bool {
    g17g()
}

/// Enabled USCs of the selected G17G device, for the native storage-demand arithmetic. The
/// identity is fixed for the device and read on every submission, so it is published once with
/// the firmware rather than threaded through the submission path.
static G17G_CORES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Publishes the enabled USC count of this device. Called once, with the started firmware.
pub(crate) fn select_g17g_cores(cores: u32) {
    G17G_CORES.store(cores, core::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn g17g_cores() -> u32 {
    G17G_CORES.load(core::sync::atomic::Ordering::Relaxed)
}

/// Size of the operand-state buffer of one compute queue.
pub(crate) fn operand_state_size() -> usize {
    if g17g() {
        G17G_OPERAND_SIZE
    } else {
        0x14000
    }
}

/// Main writes of the G17G program without a scratch request.
const G17G_REGISTERS: usize = 40;
/// Registers an optional shader-context request prepends: the value-1 writes of 0x017e1
/// (`0xfffffe0008bd5360`) and 0x017d9 (`0xfffffe0008bd62f4`), gated on descriptor +0x5d8 bit 0.
const G17G_SCRATCH_PREFIX: usize = 2;
/// Longest G17G program: the scratch-free writes plus that prefix.
const G17G_REGISTERS_MAX: usize = G17G_REGISTERS + G17G_SCRATCH_PREFIX;
/// Checkpoints of the G17G program: after the first 0x0a5c9 write, after the final 0x1a440
/// write (twice), and the full count. A scratch request shifts every checkpoint by its prefix.
const G17G_CHECKPOINTS: [u16; 4] = [37, 39, 39, G17G_REGISTERS as u16];
/// Native G17G value of register 0x101d1 for a scratch-free dispatch, packed from the command
/// words +0x5a0 = 0 and +0x5a8 = 0xffffffff. With zero, no thread of a launch runs (auto61-68).
const G17G_SCRATCH_DISABLED: u64 = 0xffff_ffff_0000;
/// The four G17G operand-state regions, as the qualified M5 dispatches laid them out: each
/// pair region spans one per-core 0x1800 record for ten cores.
const G17G_OPERAND_10099: u64 = 0x0_0005;
const G17G_OPERAND_10091: u64 = 0x0_f000;
const G17G_OPERAND_0A5C1: u64 = 0x2_0005;
const G17G_OPERAND_0A5C9: u64 = 0x2_f000;
const G17G_OPERAND_10229: u64 = 0x4_0000;
const G17G_OPERAND_140A8: u64 = 0x4_8000;
const G17G_OPERAND_SIZE: usize = 0x5_0000;

/// Fixed per-core primary storage the native demand calculation always includes.
const M5_PRIMARY_FIXED: u32 = 0x4380;
/// Threads of one clique.
const M5_THREADS_PER_CLIQUE: u32 = 32;
/// Default occupancy reserve of the native demand accumulator (`0x216109634`).
const M5_RESERVE: u32 = 96;
/// Small page of the demand calculation.
const M5_SMALL_PAGE: u32 = 4096;
/// Large page of the demand calculation; the ordinary counts use 64 KiB units.
const M5_LARGE_PAGE: u32 = 0x10000;
/// Pages the native engine reserve adds per enabled USC: 52 for a nonzero UMA descriptor word
/// plus the 24 of its special case. Which one the device selects is not established, so the
/// larger is used; the only cost is pool pages.
const M5_ENGINE_RESERVE_PAGES_PER_USC: u32 = 52 + 24;
/// Constant term of the native demand: `42 + K * (96 + 11)`.
const fn m5_baseline(cores: u32) -> u32 {
    42 + cores * (M5_RESERVE + 11)
}

/// One storage declaration of `DRM_ASAHI_SET_COMPUTE_RESOURCES`: Mesa's
/// `struct drm_asahi_compute_program`. All sizes are bytes.
#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct ComputeProgram {
    /// Per-thread primary storage.
    pub(crate) primary_bytes: u32,
    /// Per-thread secondary storage.
    pub(crate) secondary_bytes: u32,
    /// Additional primary storage after linking.
    pub(crate) linked_bytes: u32,
    /// Minimum primary storage across variants.
    pub(crate) minimum_primary_bytes: u32,
    /// Storage shared by a workgroup.
    pub(crate) threadgroup_bytes: u32,
    /// Invocations of a workgroup.
    pub(crate) threads: u32,
    /// Reserved; must be zero.
    pub(crate) flags: u32,
}

static_assert!(size_of::<ComputeProgram>() == 28);

// SAFETY: `ComputeProgram` is a packed-free struct of `u32` fields with no padding.
unsafe impl crate::util::AnyBitPattern for ComputeProgram {}

/// Folded demand of a command's storage declarations.
///
/// The native fold evaluates every declaration first and then takes componentwise maxima of the
/// derived words (`smax` at `0x216109974`, merge `0x215e9b7dc..0x215e9b918`); folding the raw
/// byte counts first would be a different calculation. Duplicate declarations are therefore
/// free, which is what Mesa's deduplication relies on.
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct ComputeStorage {
    /// 64 KiB-unit counts of primary, secondary and threadgroup storage: the three words the
    /// packed register carries.
    large: [u32; 3],
    /// Pages the pool must hold for the kick to run.
    minimum_pages: u32,
    /// Pages the native policy would like it to hold.
    ideal_pages: u32,
    /// Whether any declaration asked for storage at all.
    any: bool,
}

impl ComputeStorage {
    pub(crate) const NONE: Self = Self {
        large: [0; 3],
        minimum_pages: 0,
        ideal_pages: 0,
        any: false,
    };

    /// Folds one declaration for a device of `cores` enabled USCs in one cluster.
    ///
    /// The arithmetic is the native demand calculation of Metal `0x21610978c`
    /// (`0x2161097cc..0x2161099cc`), recovered in `gpu-bringup/sol-scratch-study.md`, for the
    /// branch that does not take the alternate occupancy policy and without the shared-size
    /// cap, whose device limit word is not established. Both omissions can only raise the
    /// demand, never lower it.
    pub(crate) fn fold(&mut self, program: &ComputeProgram, cores: u32) -> Result {
        if program.flags != 0
            || cores == 0
            || program.threadgroup_bytes > 32 * 1024
            || program.primary_bytes > u32::from(u16::MAX) * 4
            || program.secondary_bytes > u32::from(u16::MAX) * 4
            || program.linked_bytes > u32::from(u16::MAX) * 4
            || program.threads > MAX_WORKGROUP_INVOCATIONS
        {
            return Err(EINVAL);
        }
        let primary = program
            .primary_bytes
            .checked_add(program.linked_bytes)
            .ok_or(EINVAL)?
            .max(program.minimum_primary_bytes);
        let fixed = primary
            .checked_mul(M5_THREADS_PER_CLIQUE)
            .and_then(|bytes| bytes.checked_add(M5_PRIMARY_FIXED))
            .ok_or(EINVAL)?;
        let secondary = program
            .secondary_bytes
            .checked_mul(M5_THREADS_PER_CLIQUE)
            .ok_or(EINVAL)?;
        let shared = program.threadgroup_bytes;
        let [ps, pl] = [fixed.div_ceil(M5_SMALL_PAGE), fixed.div_ceil(M5_LARGE_PAGE)];
        let [ss, sl] = [secondary.div_ceil(M5_SMALL_PAGE), secondary.div_ceil(M5_LARGE_PAGE)];
        let [gs, gl] = [shared.div_ceil(M5_SMALL_PAGE), shared.div_ceil(M5_LARGE_PAGE)];
        let cliques = program.threads.div_ceil(M5_THREADS_PER_CLIQUE).max(1);
        let occupancy = M5_RESERVE / cliques;
        let resident = cliques.checked_mul(occupancy).ok_or(EINVAL)?;
        // Pages of one core at the native occupancy, state +0x20.
        let demand = resident.min(M5_RESERVE) * (ps + ss)
            + resident.min(10) * (pl + sl)
            + occupancy * gs
            + occupancy.min(8) * gl;
        let least = program.threads.div_ceil(M5_THREADS_PER_CLIQUE).max(1);
        let large = least.min(10) * (pl + sl);
        let group =
            (least * (ps + ss) + large + gs + gl).saturating_sub(M5_RESERVE) + gs + gl;
        let baseline = m5_baseline(cores);
        let reserve = M5_ENGINE_RESERVE_PAGES_PER_USC
            .checked_mul(cores)
            .ok_or(EINVAL)?;
        let up8 = |pages: u32| (pages + 7) & !7;
        let minimum = up8((large + gl) * cores + group * cores + baseline + pl + sl + gl + 64)
            .checked_add(reserve)
            .ok_or(EINVAL)?;
        let ideal = up8(baseline + cores * demand.saturating_sub(M5_RESERVE) + pl + sl + gl + 64)
            .checked_add(reserve)
            .ok_or(EINVAL)?;
        for (slot, value) in self.large.iter_mut().zip([pl, sl, gl]) {
            *slot = (*slot).max(value);
        }
        self.minimum_pages = self.minimum_pages.max(minimum);
        self.ideal_pages = self.ideal_pages.max(ideal);
        self.any = true;
        Ok(())
    }

    /// Whether any declaration needs private memory from the pool.
    pub(crate) fn enabled(&self) -> bool {
        self.any && (self.minimum_pages != 0 || self.large.iter().any(|count| *count != 0))
    }

    /// Value of register 0x0d4a1: the packed allocation counts the native publisher
    /// (`0x21610a1d4..0x21610a204`) assembles from the derived words. The two words this
    /// ordinary path leaves zero are the imageblock and other-stage counts.
    pub(crate) fn packed_counts(&self) -> u64 {
        let [primary, secondary, shared] = self.large;
        0x14b
            | (u64::from(primary & 0xff) << 10)
            | (u64::from(secondary & 0xf) << 18)
            | (u64::from(shared & 0x3) << 22)
    }

    /// Pages the pool must hold, for the whole device.
    ///
    /// The native policy keeps two sizes: a minimum and an ideal (`getMinPoolSize` /
    /// `getIdealPoolSize`). The minimum is only enough at reduced occupancy, which the native
    /// firmware negotiates through the per-kick usage records; this runtime does not take part
    /// in that negotiation, so it provisions the full-occupancy size. With the minimum, an
    /// 8 KiB-per-thread kick completed on hardware but 384 of 4096 results were wrong
    /// (boot asahi-mesa4-boot) - threads sharing frames, with no fault anywhere.
    pub(crate) fn pool_pages(&self) -> u32 {
        self.ideal_pages.max(self.minimum_pages)
    }
}

/// Invocations of one workgroup, as reported by `GET_PARAMS`.
const MAX_WORKGROUP_INVOCATIONS: u32 = 1024;

/// Validated per-command shader-context scratch request.
///
/// The layout word is Apple's packed SCS layout: depth (bits 0:15), levels
/// (16:23), cliques (24:31), user (32:47) and driver (48:63).  The region the
/// hardware needs is derived with G17P's Gen1 section model: fixed prefixes
/// totalling [`Self::FIXED_BYTES`], then per-thread stack, per-thread driver
/// and per-thread user bytes. Gen1 has no per-clique level payload. The scratch
/// registers the program writes are the native `generateRegisterList`
/// formulas use `com.apple.AGXG17P`; the
/// disabled program keeps the native disabled constants.
#[derive(Clone, Copy)]
pub(crate) struct ScratchRequest {
    pages: u16,
    window: u64,
    stack: u64,
    driver: u64,
    tail: u64,
    /// G17G: packed ordinary allocation counts for register 0x0d4a1, zero when no declaration
    /// asked for private memory.
    counts: u64,
    /// Whether the optional shader-context (SCS) mechanism is requested. It drives the two
    /// extra register writes and the shifted checkpoints; ordinary private memory does not.
    scs: bool,
}

impl ScratchRequest {
    /// Native ScsLayoutGen1::getSectionEnd(4).
    const FIXED_BYTES: u64 = 0x58000;
    const THREADS_PER_CLIQUE: u64 = 32;
    const BYTES_PER_STACK_ENTRY: u64 = 8;
    const PAGE_SHIFT: u32 = 12;
    const PAGE_SIZE: u64 = 1 << Self::PAGE_SHIFT;
    const PAGES_PER_MIB: u64 = 1 << (20 - Self::PAGE_SHIFT);
    /// Window word when enabled: bit 24 plus the clique count at 16:23.
    const WINDOW_ENABLED: u64 = 1 << 24;
    /// Native disabled window word, [`UNK_10791`].
    const WINDOW_DISABLED: u64 = UNK_10791;
    /// Native disabled stack word, [`UNK_1A0E9`].
    const STACK_DISABLED: u64 = UNK_1A0E9;
    /// Native tail reservation when enabled.
    const TAIL_ENABLED: u64 = 8;

    /// Payload-free layouts request no scratch even if depth/cliques are set.
    /// Otherwise derive the Gen1 end and round it up with the native two-page
    /// guard; the per-register words are the native formulas, so they are
    /// checked here against Apple's own values before they reach hardware.
    pub(crate) fn new(layout: u64) -> Result<Self> {
        let depth = layout & u64::from(u16::MAX);
        let cliques = (layout >> 24) & u64::from(u8::MAX);
        let user = (layout >> 32) & u64::from(u16::MAX);
        let driver = (layout >> 48) & u64::from(u16::MAX);
        if user == 0 && driver == 0 {
            return Ok(Self {
                pages: 0,
                window: Self::WINDOW_DISABLED,
                stack: Self::STACK_DISABLED,
                driver: 0,
                tail: 0,
                counts: 0,
                scs: false,
            });
        }
        let threads = cliques.checked_mul(Self::THREADS_PER_CLIQUE).ok_or(EINVAL)?;
        let stack_bytes = threads
            .checked_mul(depth.checked_mul(Self::BYTES_PER_STACK_ENTRY).ok_or(EINVAL)?)
            .ok_or(EINVAL)?;
        let driver_bytes = threads.checked_mul(driver).ok_or(EINVAL)?;
        let user_bytes = threads.checked_mul(user).ok_or(EINVAL)?;
        let end10 = Self::FIXED_BYTES
            .checked_add(stack_bytes)
            .and_then(|bytes| bytes.checked_add(driver_bytes))
            .ok_or(EINVAL)?;
        let end = end10
            .checked_add(user_bytes)
            .and_then(|bytes| bytes.checked_add(2 * Self::PAGE_SIZE - 1))
            .ok_or(EINVAL)?;
        let pages = end >> Self::PAGE_SHIFT;
        Ok(Self {
            pages: u16::try_from(pages).map_err(|_| EINVAL)?,
            window: Self::WINDOW_ENABLED | (cliques << 16),
            stack: depth & !7,
            // G17P's native register list does not publish the Gen2 level
            // region register. Preserve the historical zero slot.
            driver: 0,
            tail: Self::TAIL_ENABLED,
            counts: 0,
            scs: true,
        })
    }

    /// Whether the optional shader-context mechanism is requested. Ordinary private memory is
    /// not this: it changes neither the register count nor the checkpoints.
    pub(crate) fn enabled(self) -> bool {
        self.scs
    }

    pub(crate) fn pages(self) -> u16 { self.pages }

    pub(crate) fn size_word(self) -> u64 {
        let pages = u64::from(self.pages);
        pages | (pages.div_ceil(Self::PAGES_PER_MIB) << 16)
    }

    pub(crate) fn window_word(self) -> u64 {
        self.window
    }

    pub(crate) fn stack_word(self) -> u64 {
        self.stack
    }

    fn driver_word(self) -> u64 {
        self.driver
    }

    pub(crate) fn tail_word(self) -> u64 {
        self.tail
    }

    fn checkpoints(self) -> [u16; 4] {
        CHECKPOINTS.map(|count| count + u16::from(self.enabled()))
    }

    /// M5 request from the per-dispatch storage declarations of
    /// `DRM_ASAHI_SET_COMPUTE_RESOURCES`.
    ///
    /// Ordinary private memory and the optional shader-context mechanism are separate native
    /// inputs (`gpu-bringup/sol-scratch-study.md`): the declarations feed the demand
    /// accumulator whose packed counts the constructor publishes as register 0x0d4a1
    /// (descriptor +0x550), while register 0x101d1 carries the SCS page inventory
    /// (descriptor +0x5a0/+0x5a8) of a packed layout userspace does not send here. Every other
    /// register therefore keeps the scratch-free values that auto64..auto72 and every Mesa
    /// dispatch so far published.
    ///
    /// `pages` is the per-core page count the stock pool model expects, converted from the
    /// native whole-device demand, so the existing admission and growth paths size the queue's
    /// private-memory pool without a second model.
    pub(crate) fn m5(storage: &ComputeStorage, cores: u32) -> Result<Self> {
        if cores == 0 {
            return Err(EINVAL);
        }
        let per_core = u16::try_from(storage.pool_pages().div_ceil(cores)).map_err(|_| EINVAL)?;
        Ok(Self {
            pages: if storage.enabled() { per_core } else { 0 },
            window: Self::WINDOW_DISABLED,
            stack: Self::STACK_DISABLED,
            driver: 0,
            tail: 0,
            counts: if storage.enabled() { storage.packed_counts() } else { 0 },
            scs: false,
        })
    }

    /// Value of register 0x0d4a1 on G17G.
    pub(crate) fn g17g_counts_word(self) -> u64 {
        self.counts
    }

    /// Value of register 0x101d1 on G17G: the SCS inventory, or the native scratch-free
    /// sentinel that one hardware bisect proved both necessary and sufficient (auto66..auto68).
    pub(crate) fn g17g_inventory_word(self) -> u64 {
        if !self.scs {
            return G17G_SCRATCH_DISABLED;
        }
        u64::from(self.pages) | (u64::from(self.pages).div_ceil(Self::PAGES_PER_MIB) << 16)
    }
}
const ALT_REGISTERS: usize = 4;

/// Object IDs of every compute kick.
const OBJECT_IDS: ObjectIds = ObjectIds {
    predecessor: 0x0100_01d7,
    current: 0x0200_01dc,
};
/// Values of the first and second write of register 0x1a440.
const UNK_1A440: u64 = 0x0000_0001_5402_4201;
const UNK_1A440_FINAL: u64 = 0x0000_0001_5402_4209;
/// Value of register 0x1a458: [`UNK_1C8F8`] below bits 31:20 = 0x10c.
const UNK_1A458: u64 = 0x10c0_0000 | UNK_1C8F8;

/// Offsets in the preempt buffer of the four state words the program names.
const PREEMPT_STATE: [u64; 4] = [0x1480, 0x1488, 0x1490, 0x1498];
/// Offsets in the operand-state buffer of the regions the program names.
const OPERAND_STATE_10229: u64 = 0x12800;
const OPERAND_STATE_140A8: u64 = 0x13000;
const OPERAND_STATE_10099: u64 = 0x9405;
const OPERAND_STATE_10091: u64 = 0x12400;
const OPERAND_STATE_0A5C1: u64 = 0x0005;
const OPERAND_STATE_0A5C9: u64 = 0x9000;

/// Checkpoints into the main register array, as entry counts: after the
/// first 0x0a5c9 write, after the final 0x1a440 write (twice), and the full
/// count.
const CHECKPOINTS: [u16; 4] = [39, 40, 40, COMPUTE_REGISTERS as u16];

#[derive(Clone, Copy)]
pub(crate) struct IntersectionTables {
    pub(crate) pool: u64,
    pub(crate) pool_bytes: u64,
    auxiliary: u32,
    first: u32,
    count: u32,
    pub(crate) default_program: u64,
    default_control: u64,
}

impl IntersectionTables {
    pub(crate) const NONE: Self = Self {
        pool: 0, pool_bytes: 0, auxiliary: 0, first: 0, count: 0,
        default_program: 0, default_control: 0,
    };

    pub(crate) fn new(pool: u64, pool_bytes: u64, first: u32, count: u32,
                      default_program: u64, default_control: u64) -> Result<Self> {
        if pool == 0 {
            if pool_bytes != 0 || first != 0 || count != 0
                || default_program != 0 || default_control != 0 {
                return Err(EINVAL);
            }
            return Ok(Self::NONE);
        }
        // G17P HAL200 Gen1 publishes a byte-sized table length (25 bits),
        // with a plain aligned address. Unlike G17X HAL300, its address has
        // no bit 51 and its two pool registers use fixed native values.
        // Keep the shared UAPI's 128-byte blocks and 16 KiB allocation units.
        let end = pool.checked_add(pool_bytes).ok_or(EINVAL)?;
        let blocks_end = u64::from(first) + u64::from(count);
        let code = default_program & 0x0000_ffff_ffff_ffff;
        if pool & 0x3fff != 0 || pool_bytes == 0 || pool_bytes & 0x3fff != 0
            || end > (1u64 << 48) || (pool_bytes >> 14) >= (1u64 << 25)
            || count == 0 || count >= (1 << 18) || blocks_end * 128 > pool_bytes
            || code & 63 != 0
            || default_control & !0x0000_7fff_0000_3fff != 0
            || (code == 0 && (default_program != 0 || default_control != 0))
            || (code != 0 && default_control & (1u64 << 40) == 0) {
            return Err(EINVAL);
        }
        Ok(Self { pool, pool_bytes, auxiliary: 0, first, count, default_program, default_control })
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.get(48..).is_some_and(|tail| tail.iter().any(|&b| b != 0)) {
            return Err(EINVAL);
        }
        let mut tail = [0u8; 48];
        let count = bytes.len().min(tail.len());
        tail[..count].copy_from_slice(&bytes[..count]);
        let auxiliary = u32::from_le_bytes(tail[..4].try_into().unwrap());
        if tail[4..8].iter().any(|&b| b != 0)
            || (auxiliary != 0 && !(8..=32).contains(&auxiliary)) {
            return Err(EINVAL);
        }
        let mut tables = Self::new(
            u64::from_le_bytes(tail[8..16].try_into().unwrap()),
            u64::from_le_bytes(tail[16..24].try_into().unwrap()),
            u32::from_le_bytes(tail[24..28].try_into().unwrap()),
            u32::from_le_bytes(tail[28..32].try_into().unwrap()),
            u64::from_le_bytes(tail[32..40].try_into().unwrap()),
            u64::from_le_bytes(tail[40..48].try_into().unwrap()),
        )?;
        if auxiliary != 0 && !tables.enabled() { return Err(EINVAL); }
        tables.auxiliary = auxiliary;
        Ok(tables)
    }

    pub(crate) fn enabled(self) -> bool { self.pool != 0 }

    pub(crate) fn words(self) -> [u64; 6] {
        if !self.enabled() { return [0; 6]; }
        // AGXMetalG17P 26A428 endComputePass, both command encodings;
        // pinned AGXG17P processComputeSetup copies these six words as-is.
        [self.pool + u64::from(self.first) * 128,
         0xc0000000 | (u64::from(self.count) << 7), 0x6f00000000,
         0x3feff, self.default_program, self.default_control]
    }
}


/// Inputs of one compute descriptor.
pub(crate) struct ComputeArgs {
    pub(crate) intersection: IntersectionTables,
    /// Scratch request validated with the userspace command.
    pub(crate) scratch: ScratchRequest,
    /// Base of the queue's USC window.
    pub(crate) usc_base: u64,
    /// GPU address of the control stream.
    pub(crate) cdm_va: u64,
    /// GPU address of the control-stream terminator.
    pub(crate) cdm_end_va: u64,
    /// GPU address of the sampler heap, or zero.
    pub(crate) sampler_heap_va: u64,
    /// Number of samplers in the heap.
    pub(crate) sampler_count: u32,
    /// GPU address of this descriptor, as seen by the work's VM.
    pub(crate) descriptor_va: u64,
    /// UAT context the work runs in.
    pub(crate) context_id: u16,
    /// Allocation generation of that context.
    pub(crate) context_generation: u8,
    /// State word of the logical queue's work-state node.
    pub(crate) work_state: u32,
    /// GPU address of the logical queue's work-state node.
    pub(crate) work_state_va: u64,
    /// Queue the work is kicked on.
    pub(crate) qid: u8,
    /// Number of earlier kicks of the queue.
    pub(crate) kick_count: u32,
    /// Timestamp the work is kicked at.
    pub(crate) kick: KickTimestamp,
    /// GPU address of the preempt buffer, as seen by the work's VM.
    pub(crate) preempt_va: u64,
    /// GPU address of the operand-state buffer, as seen by the work's VM.
    pub(crate) operand_state_va: u64,
    /// GPU address of the private-memory usage record, as seen by the work's
    /// VM.
    pub(crate) usage_va: u64,
    /// Firmware alias of the private-memory usage record.
    pub(crate) usage_fw_va: u64,
    /// Buffer slot of the USC private-memory free list.
    pub(crate) free_list_slot: u32,
    /// GPU address of the free list's shared control object.
    pub(crate) free_list_control_va: u64,
    /// GPU address the completion stamp value is stored to.
    pub(crate) stamp_va: u64,
    /// GPU address of the word following the completion stamp.
    pub(crate) aux_stamp_va: u64,
    /// GPU addresses of the two status words.
    pub(crate) status_va: [u64; 2],
    /// GPU addresses of two more status words the G17G descriptor names at 0xf9c.
    pub(crate) extra_status_va: [u64; 2],
    /// GPU address of the start/end timestamp pair.
    pub(crate) timestamp_va: u64,
    /// G17G: addresses of this kick's raw 24 MHz start and end words, in an object the
    /// firmware CPU can write (a kernel-VM object, not a client mapping).
    pub(crate) timestamp_raw_va: [u64; 2],
}

fn compute_registers(args: &ComputeArgs) -> Result<[RegisterWrite; COMPUTE_REGISTERS_MAX]> {
    let preempt = args.preempt_va;
    let operand = args.operand_state_va;
    let key = work_key(args.context_id, args.work_state);
    let ids = OBJECT_IDS.word();
    let intersection = args.intersection.words();

    Ok([
        RegisterWrite::new(0x017d9, 1),
        RegisterWrite::new(0x1a510, preempt),
        RegisterWrite::new(0x1a420, args.cdm_va),
        RegisterWrite::new(0x1a4d0, offset_va(preempt, PREEMPT_STATE[0])?),
        RegisterWrite::new(0x1a4d8, offset_va(preempt, PREEMPT_STATE[1])?),
        RegisterWrite::new(0x1a4e0, offset_va(preempt, PREEMPT_STATE[2])?),
        RegisterWrite::new(0x1a4e8, offset_va(preempt, PREEMPT_STATE[3])?),
        RegisterWrite::new(0x10071, args.usc_base),
        // Helper program binary, data and configuration: none.
        RegisterWrite::new(0x11841, 0),
        RegisterWrite::new(0x11849, 0),
        RegisterWrite::new(0x11f81, 0),
        RegisterWrite::new(0x1a440, UNK_1A440),
        RegisterWrite::new(0x1a458, UNK_1A458),
        RegisterWrite::new(0x101d9, UNK_101D9),
        RegisterWrite::new(0x1a089, intersection[0]),
        RegisterWrite::new(0x1a091, intersection[1]),
        RegisterWrite::new(0x1a059, intersection[2]),
        RegisterWrite::new(0x1a061, intersection[3]),
        RegisterWrite::new(0x1a0b9, intersection[4]),
        RegisterWrite::new(0x1a0c1, intersection[5]),
        RegisterWrite::new(0x101d1, args.scratch.size_word()),
        RegisterWrite::new(0x0d479, if args.intersection.auxiliary == 0 {
            args.scratch.tail_word()
        } else {
            u64::from(args.intersection.auxiliary)
        }),
        RegisterWrite::new(0x1a0e9, args.scratch.stack_word()),
        RegisterWrite::new(0x1a141, args.scratch.driver_word()),
        RegisterWrite::new(0x107a1, args.scratch.window_word()),
        RegisterWrite::new(0x0a599, UNK_0A599),
        RegisterWrite::new(0x0d411, UNK_0D411),
        RegisterWrite::new(0x1a540, ids),
        RegisterWrite::new(0x014a9, ids),
        RegisterWrite::new(0x0a351, ids),
        RegisterWrite::new(0x10201, key),
        RegisterWrite::new(0x10428, key),
        RegisterWrite::new(0x14028, u64::from(args.free_list_slot)),
        RegisterWrite::new(0x14070, args.usage_va | 1),
        RegisterWrite::new(0x10229, offset_va(operand, OPERAND_STATE_10229)?),
        RegisterWrite::new(0x140a8, offset_va(operand, OPERAND_STATE_140A8)?),
        RegisterWrite::new(0x10099, offset_va(operand, OPERAND_STATE_10099)?),
        RegisterWrite::new(0x10091, offset_va(operand, OPERAND_STATE_10091)?),
        RegisterWrite::new(0x0a5c1, offset_va(operand, OPERAND_STATE_0A5C1)?),
        RegisterWrite::new(0x0a5c9, offset_va(operand, OPERAND_STATE_0A5C9)?),
        RegisterWrite::new(0x1a440, UNK_1A440_FINAL),
        RegisterWrite::new(0x0a599, UNK_0A599),
    ])
}

/// The G17G (M5) program, in the order of the qualified native constructor
/// (`AGX-PI-300-G` `generateRegisterList`, auto64..auto72): no USC base or helper-program
/// registers, the 0x0d4a1 and two 0x0a731 writes, 0x0d411/0x0a599 before 0x1a458, and the
/// native scratch-free inventory word.
fn g17g_registers(
    args: &ComputeArgs,
) -> Result<([RegisterWrite; G17G_REGISTERS_MAX], usize)> {
    let preempt = args.preempt_va;
    let operand = args.operand_state_va;
    let key = work_key(args.context_id, args.work_state);
    let ids = OBJECT_IDS.word();
    let scratch = args.scratch;
    let mut writes = [RegisterWrite::new(0, 0); G17G_REGISTERS_MAX];
    let mut used = 0;
    let mut push = |write: RegisterWrite| {
        writes[used] = write;
        used += 1;
    };
    // The native constructor emits this gate only for a request with private memory; a
    // scratch-free kick has no such write (auto64..auto72 published exactly forty).
    if scratch.enabled() {
        push(RegisterWrite::new(0x017d9, 1));
    }
    for write in [
        RegisterWrite::new(0x1a510, preempt),
        RegisterWrite::new(0x1a420, args.cdm_va),
        RegisterWrite::new(0x1a4d0, offset_va(preempt, PREEMPT_STATE[0])?),
        RegisterWrite::new(0x1a4d8, offset_va(preempt, PREEMPT_STATE[1])?),
        RegisterWrite::new(0x1a4e0, offset_va(preempt, PREEMPT_STATE[2])?),
        RegisterWrite::new(0x1a4e8, offset_va(preempt, PREEMPT_STATE[3])?),
        RegisterWrite::new(0x1a440, UNK_1A440),
        // Ordinary private-memory counts (sol-scratch-study.md: wire +0x178 -> descriptor
        // +0x550 -> 0x0d4a1); zero for a scratch-free kick, the forty-write baseline.
        RegisterWrite::new(0x0d4a1, scratch.g17g_counts_word()),
        RegisterWrite::new(0x0d411, UNK_0D411),
        RegisterWrite::new(0x0a599, UNK_0A599),
        RegisterWrite::new(0x1a458, UNK_1A458),
        RegisterWrite::new(0x101d9, UNK_101D9),
        RegisterWrite::new(0x1a089, 0),
        RegisterWrite::new(0x1a091, 0),
        RegisterWrite::new(0x1a059, 0),
        RegisterWrite::new(0x1a061, 0),
        RegisterWrite::new(0x1a0b9, 0),
        RegisterWrite::new(0x1a0c1, 0),
        RegisterWrite::new(0x101d1, scratch.g17g_inventory_word()),
        RegisterWrite::new(0x0d479, scratch.tail_word()),
        RegisterWrite::new(0x1a0e9, scratch.stack_word()),
        RegisterWrite::new(0x1a141, scratch.driver_word()),
        RegisterWrite::new(0x107a1, scratch.window_word()),
        RegisterWrite::new(0x1a540, ids),
        RegisterWrite::new(0x014a9, ids),
        RegisterWrite::new(0x0a351, ids),
        RegisterWrite::new(0x0a731, 0),
        RegisterWrite::new(0x10201, key),
        RegisterWrite::new(0x10428, key),
        RegisterWrite::new(0x14028, u64::from(args.free_list_slot)),
        RegisterWrite::new(0x14070, args.usage_va | 1),
        RegisterWrite::new(0x10229, offset_va(operand, G17G_OPERAND_10229)?),
        RegisterWrite::new(0x140a8, offset_va(operand, G17G_OPERAND_140A8)?),
        RegisterWrite::new(0x10099, offset_va(operand, G17G_OPERAND_10099)?),
        RegisterWrite::new(0x10091, offset_va(operand, G17G_OPERAND_10091)?),
        RegisterWrite::new(0x0a5c1, offset_va(operand, G17G_OPERAND_0A5C1)?),
        RegisterWrite::new(0x0a5c9, offset_va(operand, G17G_OPERAND_0A5C9)?),
        RegisterWrite::new(0x0a731, 0),
        RegisterWrite::new(0x1a440, UNK_1A440_FINAL),
        RegisterWrite::new(0x0a599, UNK_0A599),
    ] {
        push(write);
    }
    Ok((writes, used))
}

/// Checkpoints of a G17G program: a scratch request shifts each one by its prefix.
fn g17g_checkpoints(scratch: ScratchRequest) -> [u16; 4] {
    let prefix = if scratch.enabled() { G17G_SCRATCH_PREFIX as u16 } else { 0 };
    G17G_CHECKPOINTS.map(|count| count + prefix)
}

/// The second array exchanges the operand-state regions of registers
/// 0x10099/0x0a5c1 and 0x10091/0x0a5c9.
fn alt_registers(args: &ComputeArgs) -> Result<[RegisterWrite; ALT_REGISTERS]> {
    let operand = args.operand_state_va;
    let [r10099, r10091, r0a5c1, r0a5c9] = if g17g() {
        [G17G_OPERAND_10099, G17G_OPERAND_10091, G17G_OPERAND_0A5C1, G17G_OPERAND_0A5C9]
    } else {
        [OPERAND_STATE_10099, OPERAND_STATE_10091, OPERAND_STATE_0A5C1, OPERAND_STATE_0A5C9]
    };
    Ok([
        RegisterWrite::new(0x10099, offset_va(operand, r0a5c1)?),
        RegisterWrite::new(0x10091, offset_va(operand, r0a5c9)?),
        RegisterWrite::new(0x0a5c1, offset_va(operand, r10099)?),
        RegisterWrite::new(0x0a5c9, offset_va(operand, r10091)?),
    ])
}

/// Compute work descriptor (tag 3).
#[repr(C, packed)]
#[derive(Debug)]
pub(crate) struct ComputeDescriptor {
    /// [`CommandTag::Compute`].
    pub(crate) tag: u32,
    pub(crate) unk_04: u64,
    /// UAT context of the work.
    pub(crate) context_id: u32,
    /// GPU address of the logical queue's work-state node.
    pub(crate) work_state_va: u64,
    /// Checkpoints into the main register array.
    pub(crate) checkpoints: [u16; 4],
    pub(crate) unk_20: [u8; 0x20],
    /// Main register array.
    pub(crate) registers: RegisterArray,
    /// Second register array.
    pub(crate) alt_registers: RegisterArray,
    pub(crate) unk_e80: [u8; 0x58],
    /// GPU address of the preempt buffer.
    pub(crate) preempt_va: u64,
    /// GPU address of the control-stream terminator.
    pub(crate) cdm_end_va: u64,
    pub(crate) unk_ee8: [u8; 0x20],
    /// First value of register 0x1a440.
    pub(crate) unk_f08: u64,
    pub(crate) unk_f10: [u8; 0x10],
    /// Predecessor object ID.
    pub(crate) predecessor_id: u32,
    pub(crate) unk_f24: u32,
    pub(crate) unk_f28: u32,
    /// GPU address of the sampler heap.
    pub(crate) sampler_heap_va: u64,
    /// Number of samplers.
    pub(crate) sampler_count: u32,
    /// Number of samplers plus one, or zero without samplers.
    pub(crate) sampler_max: u32,
    pub(crate) unk_f3c: u32,
    /// GPU address the firmware stores `stamp_value` to on completion.
    pub(crate) stamp_va: u64,
    /// GPU address of the word following the completion stamp.
    pub(crate) aux_stamp_va: u64,
    /// Completion stamp value of the kick timestamp.
    pub(crate) stamp_value: u32,
    /// QID of the compute queue.
    pub(crate) qid: u32,
    pub(crate) unk_f58: u32,
    pub(crate) unk_f5c: u32,
    /// Kick timestamp.
    pub(crate) kick_timestamp: u64,
    /// Current object ID.
    pub(crate) current_id: u64,
    /// Number of earlier kicks of the queue.
    pub(crate) kick_count: u32,
    pub(crate) unk_f74: [u8; 8],
    /// G17P: two status words. G17G (HAL300, sol-hal300-timestamp-study.md): the raw
    /// 24 MHz command timestamp pair, written while the firmware consumes an eligible
    /// hardware completion (minimum into the start word, maximum into the end word), with
    /// no address-aperture restriction.
    pub(crate) status_va: [u64; 2],
    /// GPU address the start timestamp is written to.
    pub(crate) timestamp_start_va: u64,
    /// GPU address the end timestamp is written to.
    pub(crate) timestamp_end_va: u64,
    /// G17G: `{u32 start_count, u32 end_count}` at 0xf9c/0xfa0 and, at 0xfa4, the address
    /// list of the firmware's additional raw timestamp writes (at most eight). Zero on
    /// G17P and left zero here: a nonzero pair would be consumed as those counts.
    pub(crate) extra_status_va: [u64; 2],
    pub(crate) unk_fac: [u8; 4],
    pub(crate) unk_fb0: u16,
    /// GPU address of the USC free list's shared control object.
    pub(crate) free_list_control_va: u64,
    pub(crate) unk_fba: u32,
    pub(crate) unk_fbe: [u8; 7],
    pub(crate) unk_fc5: u8,
    pub(crate) unk_fc6: [u8; 5],
    /// Firmware alias of the private-memory usage record.
    pub(crate) usage_va: u64,
    /// Allocation generation of the UAT context.
    pub(crate) context_generation: u8,
    pub(crate) unk_fd4: [u8; 0x302c],
}

static_assert!(size_of::<ComputeDescriptor>() == COMPUTE_DESCRIPTOR_SIZE);
static_assert!(core::mem::offset_of!(ComputeDescriptor, registers) == 0x40);
static_assert!(core::mem::offset_of!(ComputeDescriptor, alt_registers) == 0x760);
static_assert!(core::mem::offset_of!(ComputeDescriptor, preempt_va) == 0xed8);
static_assert!(core::mem::offset_of!(ComputeDescriptor, unk_f08) == 0xf08);
static_assert!(core::mem::offset_of!(ComputeDescriptor, predecessor_id) == 0xf20);
static_assert!(core::mem::offset_of!(ComputeDescriptor, sampler_heap_va) == 0xf2c);
static_assert!(core::mem::offset_of!(ComputeDescriptor, stamp_va) == 0xf40);
static_assert!(core::mem::offset_of!(ComputeDescriptor, kick_timestamp) == 0xf60);
static_assert!(core::mem::offset_of!(ComputeDescriptor, current_id) == 0xf68);
static_assert!(core::mem::offset_of!(ComputeDescriptor, status_va) == 0xf7c);
static_assert!(core::mem::offset_of!(ComputeDescriptor, extra_status_va) == 0xf9c);
static_assert!(core::mem::offset_of!(ComputeDescriptor, free_list_control_va) == 0xfb2);
static_assert!(core::mem::offset_of!(ComputeDescriptor, usage_va) == 0xfcb);
static_assert!(core::mem::offset_of!(ComputeDescriptor, context_generation) == 0xfd3);

// SAFETY: `ComputeDescriptor` consists of integers and arrays of integers only.
unsafe impl Zeroable for ComputeDescriptor {}

impl ComputeDescriptor {
    const REGISTERS_OFFSET: u64 = core::mem::offset_of!(Self, registers) as u64;
    const ALT_REGISTERS_OFFSET: u64 = core::mem::offset_of!(Self, alt_registers) as u64;
    const UNK_F28: u32 = u32::MAX;
    const UNK_FB0: u16 = 0x1a;
    const UNK_FBA: u32 = 0xe0a0_0001;
    const UNK_FC5: u8 = 0x9f;
    const G17G_UNK_F58: u32 = 2;
    const G17G_UNK_F5C: u32 = 1;

    /// Writes the complete descriptor.
    pub(crate) fn write(&mut self, args: &ComputeArgs) -> Result {
        if args.cdm_end_va < args.cdm_va
            || (args.sampler_heap_va == 0) != (args.sampler_count == 0)
            || args.sampler_heap_va & 7 != 0
        {
            return Err(EINVAL);
        }
        let g17g = g17g();
        if g17g && args.intersection.enabled() {
            // The HAL300 intersection encoding is not qualified.
            return Err(EINVAL);
        }
        let alt_registers = alt_registers(args)?;
        let registers_va = offset_va(args.descriptor_va, Self::REGISTERS_OFFSET)?;
        let alt_registers_va = offset_va(args.descriptor_va, Self::ALT_REGISTERS_OFFSET)?;
        let timestamp_end_va = timestamp_end_va(args.timestamp_va)?;
        let sampler_max = sampler_max(args.sampler_count)?;

        clear(self);
        self.tag = CommandTag::Compute as u32;
        self.context_id = args.context_id.into();
        self.work_state_va = args.work_state_va;
        if g17g {
            let (registers, used) = g17g_registers(args)?;
            self.checkpoints = g17g_checkpoints(args.scratch);
            self.registers.set(registers_va, &registers[..used])?;
        } else {
            let registers = compute_registers(args)?;
            self.checkpoints = args.scratch.checkpoints();
            // The enabled program prepends one register; disabled descriptors keep
            // the same forty writes, trailer count and resume checkpoints.
            let first = usize::from(!args.scratch.enabled());
            self.registers.set(registers_va, &registers[first..])?;
        }
        self.alt_registers.set(alt_registers_va, &alt_registers)?;
        self.preempt_va = args.preempt_va;
        self.cdm_end_va = args.cdm_end_va;
        self.unk_f08 = UNK_1A440;
        self.predecessor_id = OBJECT_IDS.predecessor;
        self.unk_f28 = Self::UNK_F28;
        self.sampler_heap_va = args.sampler_heap_va;
        self.sampler_count = args.sampler_count;
        self.sampler_max = sampler_max;
        self.stamp_va = args.stamp_va;
        self.aux_stamp_va = args.aux_stamp_va;
        self.stamp_value = stamp_value(args.kick.get() as u32);
        self.qid = args.qid.into();
        self.kick_timestamp = args.kick.get();
        self.current_id = OBJECT_IDS.current.into();
        self.kick_count = args.kick_count;
        self.status_va = args.status_va;
        self.timestamp_start_va = args.timestamp_va;
        self.timestamp_end_va = timestamp_end_va;
        self.free_list_control_va = args.free_list_control_va;
        self.usage_va = args.usage_fw_va;
        self.context_generation = args.context_generation;
        if g17g {
            // As the native M5 footer wrote them in every qualified dispatch: the data master
            // and one, and no 0xfb0/0xfba/0xfc5 tail. The firmware writes 0xf8c/0xf94
            // (nanoseconds) only to addresses inside its own 64 MiB timestamp aperture, which
            // a client page is not, so this command's pair goes into the raw 0xf7c/0xf84
            // interface instead and 0xf9c/0xfa4 stay clear: they are the additional-write
            // counts and address list, not status words (sol-hal300-timestamp-study.md).
            self.unk_f58 = Self::G17G_UNK_F58;
            self.unk_f5c = Self::G17G_UNK_F5C;
            self.status_va = args.timestamp_raw_va;
            self.timestamp_start_va = 0;
            self.timestamp_end_va = 0;
            self.extra_status_va = [0; 2];
        } else {
            self.unk_fb0 = Self::UNK_FB0;
            self.unk_fba = Self::UNK_FBA;
            self.unk_fc5 = Self::UNK_FC5;
        }
        Ok(())
    }

    /// Returns the register arrays a compute kick binds; `descriptor_va` is
    /// the descriptor's address as seen by the work's VM.
    pub(crate) fn register_bindings(
        descriptor_va: u64,
        scratch: ScratchRequest,
    ) -> Result<[Option<RegisterArrayBinding>; 4]> {
        let checkpoint = if g17g() {
            g17g_checkpoints(scratch)[0]
        } else {
            scratch.checkpoints()[0]
        };
        let registers = RegisterArrayBinding::new(
            offset_va(descriptor_va, Self::REGISTERS_OFFSET)?,
            checkpoint as u8,
        )?;
        let alt_registers = RegisterArrayBinding::new(
            offset_va(descriptor_va, Self::ALT_REGISTERS_OFFSET)?,
            ALT_REGISTERS as u8,
        )?;
        Ok([Some(registers), Some(alt_registers), None, None])
    }
}

/// One retained queue-context record. The descriptor pointer is refreshed for each compute
/// publication; the remaining fields are installed once with the queue.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct QueueContextItem {
    unk_00: u64,
    unk_08: u64,
    descriptor_va: u64,
    queue_va: u64,
    unk_20: u64,
    qid: u64,
    unk_30: [u8; 0x100],
    unk_130: u64,
    unk_138: u64,
    unk_140: [u8; 0x10],
    unk_150: u64,
    unk_158: u64,
    unk_160: [u8; 0x18],
    unk_178: u64,
    unk_180: [u8; 0x80],
}
impl QueueContextItem {
    pub(crate) fn new(descriptor_va: u64, queue_va: u64, qid: u8) -> Self {
        Self {
            unk_00: 0x1000_1000_0000_0004,
            unk_08: 0,
            descriptor_va,
            queue_va,
            unk_20: 0xffff_0801_0000_0001,
            qid: u64::from(qid) << 40,
            unk_30: [0; 0x100],
            unk_130: 0,
            unk_138: 4,
            unk_140: [0; 0x10],
            unk_150: 0x0001_1003_8001_a002,
            unk_158: 0x0000_2003_8001_a03b,
            unk_160: [0; 0x18],
            unk_178: 0x003f_ffff_ffff_ffff,
            unk_180: [0; 0x80],
        }
    }
}
static_assert!(size_of::<QueueContextItem>() == 0x200);
