// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! HAL300 firmware session of T8142 (M5, G17G C0).
//!
//! The M5 firmware uses the same coprocessor pair, RTKit endpoints, initialization messages and
//! TTBAT contexts as the other G17 GPUs, but a different initialization graph (HAL300). This module
//! starts it the way the j704 RAM runtime did on every qualified M5 boot:
//!
//! 1. Identify both loaded firmware images (SHA-256 of TEXT with the entropy and the data pointer
//!    cleared) before any firmware-visible write.
//! 2. Build the startup graph (`hal300_build`, from the generated tables in `hal300_graph` and
//!    the device tree's operating points and power inputs), or, on a device tree without builder
//!    inputs, load the captured one (`apple/t8142-hal300-graph.bin`). When both exist the built
//!    graph is compared with the captured one byte for byte. Every object is Linux-owned RAM at a
//!    fixed address; every window is an admitted SGX or power-telemetry register range.
//! 3. Create the UAT over the reserved tables, which are not System RAM
//!    ([`mmu::Uat::new_published_foreign`]), and map the lower aliases, so the lower root's
//!    top-level entries exist when the coprocessors sample them.
//! 4. Boot both coprocessors, check the kernel window both published, then map the graph objects
//!    and register windows below it.
//! 5. Enable core power gating, hand each coprocessor its root, end the power-on transition and
//!    wait for the settled status both reported in every qualified boot.
//!
//! The firmware cannot be stopped and restarted safely, so everything firmware-visible, the
//! coprocessors and this module are retained until reboot, also when a later step fails.

use core::ops::Range;
use core::sync::atomic::Ordering;

use kernel::{
    c_str,
    device::{self, Core},
    platform,
    prelude::*,
    sync::Arc,
    time::{
        delay::fsleep,
        Delta, //
    },
};

use super::object::{
    CpuMap,
    KernelObject,
    Placement, //
};
use super::{
    channel, freelist, initdata, kick, object, power, recovery, runtime, Bridge, Config,
    Coprocessor, Firmware, Role, Shared,
};
use crate::driver::AsahiDrmDevice;
use crate::{mem, mmu, regs};

/// Firmware file of the startup graph.
const GRAPH_FILE: &CStr = c_str!("apple/t8142-hal300-graph.bin");
const GRAPH_MAGIC: &[u8; 8] = b"T8142H30";
const GRAPH_VERSION: u32 = 1;
const GRAPH_HEADER: usize = 64;
const GRAPH_WINDOW: usize = 32;
const GRAPH_OBJECT: usize = 72;
const SHA256: usize = 32;

/// The driver-managed kernel window, where every graph object lives.
const KERNEL_WINDOW: Range<u64> = 0xfffffc20_00000000..0xfffffc30_00000000;
/// Register windows of the graph's relative PIO table, one MiB per slot.
const REGISTER_WINDOWS: u64 = 0xfffffc22_00000000;
/// IOMapping slot 14, PMSMetrologySensorRegs: physical base and size (see `start`).
const PMS_SLOT: u64 = 14;
const PMS_PHYS: u64 = 0x3_8028_0000;
const PMS_SIZE: u32 = 0x8000;
/// The lower window the firmware reaches graph objects through.
const LOWER_WINDOW: Range<u64> = 0x70_00000000..0x71_00000000;
/// Largest graph object and the whole graph, as admitted by the runtime.
const MAX_OBJECT: usize = 8 << 20;
const MAX_GRAPH: usize = 64 << 20;
/// Bytes of a root the firmware reads at its initialization address.
const ROOT_SIZE: u64 = 0xc8;

/// Time both coprocessors take at most to settle after the power-on transition. The runtime
/// polled for 4 s and always observed the settled status.
const SETTLE_POLLS: usize = 400;
const SETTLE_POLL_MS: i64 = 10;

/// Firmware and graph identity of T8142.
pub(crate) struct Hal300Config {
    /// Qualified firmware image of each coprocessor.
    pub(crate) firmware: [FirmwareImage; 2],
    /// Admitted physical register ranges a graph window may map.
    pub(crate) registers: &'static [Range<u64>],
}

/// A qualified firmware image, identified by its TEXT segment.
pub(crate) struct FirmwareImage {
    pub(crate) name: &'static str,
    pub(crate) text_size: usize,
    pub(crate) data_size: u64,
    /// `GKTS` and `ECAP` entropy records, each followed by 8 random bytes.
    pub(crate) entropy: [usize; 2],
    /// SHA-256 of TEXT with both entropy values and the data pointer cleared.
    pub(crate) sha256: [u8; 32],
}

mod glue {
    use kernel::{device, prelude::*};

    extern "C" {
        fn asahi_hal300_sha256(data: *const u8, len: usize, out: *mut u8);
        fn asahi_hal300_module_get();
        fn asahi_hal300_region(
            dev: *mut core::ffi::c_void,
            name: *const core::ffi::c_char,
            start: *mut u64,
            size: *mut u64,
        ) -> core::ffi::c_int;
    }

    pub(super) fn sha256(data: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        // SAFETY: `data` is valid for its length and `out` holds the 32-byte digest.
        unsafe { asahi_hal300_sha256(data.as_ptr(), data.len(), out.as_mut_ptr()) };
        out
    }

    /// Pins this module until reboot.
    pub(super) fn retain_module() {
        // SAFETY: The module is live while its code runs; the reference is never put.
        unsafe { asahi_hal300_module_get() };
    }

    /// A named no-map memory region of `dev` outside System RAM.
    pub(super) fn region(dev: &device::Device, name: &CStr) -> Result<core::ops::Range<u64>> {
        let (mut start, mut size) = (0, 0);
        // SAFETY: The device and the C string are valid for the call; the outputs are live.
        kernel::error::to_result(unsafe {
            asahi_hal300_region(dev.as_raw().cast(), name.as_char_ptr(), &mut start, &mut size)
        })?;
        Ok(start..start.checked_add(size).ok_or(EINVAL)?)
    }
}

fn region(dev: &device::Device, name: &CStr) -> Result<Range<u64>> {
    glue::region(dev, name).inspect_err(|e| {
        dev_err!(dev, "HAL300: memory region {:?} unavailable: {:?}\n", name, e)
    })
}

/// Copies `size` bytes at physical `phys`, outside System RAM, through a temporary mapping.
pub(super) fn copy_physical(phys: u64, size: usize) -> Result<KVVec<u8>> {
    // SAFETY: memremap() of a reserved range creates a temporary mapping without side effects.
    let map = unsafe { kernel::bindings::memremap(phys, size, kernel::bindings::MEMREMAP_WC as _) };
    if map.is_null() {
        return Err(ENOMEM);
    }
    let copy = KVVec::from_elem(0u8, size, GFP_KERNEL).map(|mut copy| {
        // SAFETY: `map` maps `size` bytes, `copy` holds `size` bytes; they do not overlap.
        unsafe { core::ptr::copy_nonoverlapping(map as *const u8, copy.as_mut_ptr(), size) };
        copy
    });
    // SAFETY: `map` came from memremap() above and is not used afterwards.
    unsafe { kernel::bindings::memunmap(map) };
    Ok(copy?)
}

fn le32(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data.get(at..at + 4).ok_or(EINVAL)?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| EINVAL)?))
}

fn le64(data: &[u8], at: usize) -> Result<u64> {
    let bytes = data.get(at..at + 8).ok_or(EINVAL)?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| EINVAL)?))
}

/// Identifies a loaded firmware image before anything firmware-visible is written.
fn identify(dev: &device::Device, image: &FirmwareImage, text: &Range<u64>, data: &Range<u64>) -> Result {
    let size = usize::try_from(text.end - text.start)?;
    if size != image.text_size || data.end - data.start != image.data_size {
        dev_err!(
            dev,
            "HAL300: {} segments {:#x}/{:#x} do not match the qualified image\n",
            image.name,
            size,
            data.end - data.start
        );
        return Err(ENODEV);
    }
    let mut copy = copy_physical(text.start, size)?;
    let again = copy_physical(text.start, size)?;
    if copy.as_slice() != again.as_slice() {
        dev_err!(dev, "HAL300: {} TEXT changed while stopped\n", image.name);
        return Err(EAGAIN);
    }
    if le32(&copy, 0x200)? != 0x1400021a
        || le32(&copy, 0x204)? != 0x64697575
        || le32(&copy, 0x208)? != 5
        || le64(&copy, 0x23c)? != data.start
    {
        dev_err!(dev, "HAL300: {} TEXT header or data pointer mismatch\n", image.name);
        return Err(ENODEV);
    }
    copy[0x23c..0x244].fill(0);
    for (at, tag) in image.entropy.iter().zip([b"GKTS", b"ECAP"]) {
        if copy.get(*at..*at + 4) != Some(tag.as_slice()) || le32(&copy, at + 4)? != 8 {
            dev_err!(dev, "HAL300: {} entropy record missing at {:#x}\n", image.name, at);
            return Err(ENODEV);
        }
        copy.get_mut(at + 8..at + 16).ok_or(EINVAL)?.fill(0);
    }
    if glue::sha256(&copy) != image.sha256 {
        dev_err!(dev, "HAL300: {} TEXT is not the qualified build\n", image.name);
        return Err(ENODEV);
    }
    dev_info!(dev, "HAL300: {} firmware identified\n", image.name);
    Ok(())
}

/// A register window of the graph.
struct Window {
    va: u64,
    phys: u64,
    size: u64,
    slot: u32,
}

/// An object of the graph: its fixed addresses and the non-zero runs of its contents.
struct GraphObject<'a> {
    va: u64,
    size: usize,
    aliases: [u64; 2],
    name: &'a [u8],
    runs: KVec<(usize, &'a [u8])>,
}

/// The parsed, validated startup graph.
struct Graph<'a> {
    windows: KVec<Window>,
    objects: KVec<GraphObject<'a>>,
    roots: [u64; 2],
}

fn page_aligned(value: u64) -> bool {
    value & mmu::UAT_PGMSK as u64 == 0
}

impl<'a> Graph<'a> {
    fn parse(data: &'a [u8], cfg: &Hal300Config) -> Result<Self> {
        if data.len() < GRAPH_HEADER + SHA256 || data.get(..8) != Some(GRAPH_MAGIC.as_slice()) {
            return Err(EINVAL);
        }
        let (signed, digest) = data.split_at(data.len() - SHA256);
        if glue::sha256(signed) != digest {
            return Err(EBADMSG);
        }
        if le32(data, 8)? != GRAPH_VERSION
            || le32(data, 12)? as usize != GRAPH_HEADER
            || le64(data, 0x28)? as usize != signed.len() - GRAPH_HEADER
        {
            return Err(EINVAL);
        }
        let windows = le32(data, 0x10)? as usize;
        let objects = le32(data, 0x14)? as usize;
        let roots = [le64(data, 0x18)?, le64(data, 0x20)?];
        let mut graph = Graph {
            windows: KVec::new(),
            objects: KVec::new(),
            roots,
        };
        let mut at = GRAPH_HEADER;
        for _ in 0..windows {
            let window = Window {
                va: le64(signed, at)?,
                phys: le64(signed, at + 8)?,
                size: le64(signed, at + 16)?,
                slot: le32(signed, at + 24)?,
            };
            at += GRAPH_WINDOW;
            let admitted = cfg.registers.iter().any(|range| {
                window.phys >= range.start
                    && window.size <= range.end - range.start
                    && window.phys - range.start <= range.end - range.start - window.size
            });
            if window.slot >= 64
                || window.va != REGISTER_WINDOWS + u64::from(window.slot) * 0x100000
                || window.size == 0
                || window.size > 0x100000
                || !page_aligned(window.phys | window.size)
                || !admitted
                || graph.windows.iter().any(|other| other.slot == window.slot)
            {
                return Err(EINVAL);
            }
            graph.windows.push(window, GFP_KERNEL)?;
        }
        let mut total = 0usize;
        for _ in 0..objects {
            let va = le64(signed, at)?;
            let size = usize::try_from(le64(signed, at + 8)?)?;
            let aliases = [le64(signed, at + 16)?, le64(signed, at + 24)?];
            let count = le32(signed, at + 32)? as usize;
            let name = signed.get(at + 40..at + 72).ok_or(EINVAL)?;
            let name = &name[..name.iter().position(|&b| b == 0).ok_or(EINVAL)?];
            at += GRAPH_OBJECT;
            total = total.checked_add(size).ok_or(EOVERFLOW)?;
            let end = va.checked_add(size as u64).ok_or(EOVERFLOW)?;
            if size == 0
                || size > MAX_OBJECT
                || total > MAX_GRAPH
                || !page_aligned(va | size as u64)
                || va < KERNEL_WINDOW.start
                || end > REGISTER_WINDOWS
                || graph
                    .objects
                    .iter()
                    .any(|other| va < other.va + other.size as u64 && other.va < end)
            {
                return Err(EINVAL);
            }
            for alias in aliases.iter().filter(|&&alias| alias != 0) {
                let alias_end = alias.checked_add(size as u64).ok_or(EOVERFLOW)?;
                if !page_aligned(*alias) || *alias < LOWER_WINDOW.start || alias_end > LOWER_WINDOW.end {
                    return Err(EINVAL);
                }
            }
            let table = signed.get(at..at + count * 8).ok_or(EINVAL)?;
            at += count * 8;
            let mut runs = KVec::with_capacity(count, GFP_KERNEL)?;
            for i in 0..count {
                let offset = le32(table, i * 8)? as usize;
                let length = le32(table, i * 8 + 4)? as usize;
                if offset.checked_add(length).ok_or(EOVERFLOW)? > size {
                    return Err(EINVAL);
                }
                runs.push((offset, signed.get(at..at + length).ok_or(EINVAL)?), GFP_KERNEL)?;
                at += length;
            }
            at = (at + 7) & !7;
            graph.objects.push(
                GraphObject {
                    va,
                    size,
                    aliases,
                    name,
                    runs,
                },
                GFP_KERNEL,
            )?;
        }
        if at != signed.len() {
            return Err(EINVAL);
        }
        for root in roots {
            if !graph.objects.iter().any(|object| {
                root >= object.va && root & 7 == 0 && root - object.va + ROOT_SIZE <= object.size as u64
            }) {
                return Err(EINVAL);
            }
        }
        Ok(graph)
    }
}

impl Graph<'static> {
    /// The graph the driver builds (`hal300_graph`), with the same admission as a loaded one.
    fn from_tables(cfg: &Hal300Config) -> Result<Self> {
        let mut graph = Graph {
            windows: KVec::new(),
            objects: KVec::new(),
            roots: super::hal300_graph::ROOTS,
        };
        for spec in super::hal300_graph::WINDOWS.iter() {
            let admitted = cfg.registers.iter().any(|range| {
                spec.phys >= range.start
                    && spec.size <= range.end - range.start
                    && spec.phys - range.start <= range.end - range.start - spec.size
            });
            if spec.slot >= 64
                || spec.va != REGISTER_WINDOWS + u64::from(spec.slot) * 0x100000
                || spec.size == 0
                || spec.size > 0x100000
                || !page_aligned(spec.phys | spec.size)
                || !admitted
                || graph.windows.iter().any(|other| other.slot == spec.slot)
            {
                return Err(EINVAL);
            }
            graph.windows.push(
                Window {
                    va: spec.va,
                    phys: spec.phys,
                    size: spec.size,
                    slot: spec.slot,
                },
                GFP_KERNEL,
            )?;
        }
        for spec in super::hal300_graph::OBJECTS.iter() {
            let end = spec.va.checked_add(spec.size as u64).ok_or(EOVERFLOW)?;
            if spec.size == 0
                || spec.size > MAX_OBJECT
                || !page_aligned(spec.va | spec.size as u64)
                || spec.va < KERNEL_WINDOW.start
                || end > REGISTER_WINDOWS
                || graph
                    .objects
                    .iter()
                    .any(|other| spec.va < other.va + other.size as u64 && other.va < end)
                || spec.aliases.iter().any(|&alias| {
                    alias != 0
                        && (!page_aligned(alias)
                            || alias < LOWER_WINDOW.start
                            || alias + spec.size as u64 > LOWER_WINDOW.end)
                })
            {
                return Err(EINVAL);
            }
            graph.objects.push(
                GraphObject {
                    va: spec.va,
                    size: spec.size,
                    aliases: spec.aliases,
                    name: spec.name,
                    runs: KVec::new(),
                },
                GFP_KERNEL,
            )?;
        }
        Ok(graph)
    }
}

/// Bytes of `built` that differ from the captured object `reference` (its runs, zero elsewhere):
/// their count and the first offset.
fn differences(built: &[u8], reference: &GraphObject<'_>) -> (usize, Option<usize>) {
    let (mut count, mut first, mut at) = (0usize, None, 0usize);
    let mut check = |offset: usize, got: u8, want: u8| {
        if got != want {
            count += 1;
            first.get_or_insert(offset);
        }
    };
    for (offset, data) in reference.runs.iter() {
        for i in at..(*offset).min(built.len()) {
            check(i, built[i], 0);
        }
        for (j, want) in data.iter().enumerate() {
            if let Some(got) = built.get(offset + j) {
                check(offset + j, *got, *want);
            }
        }
        at = offset + data.len();
    }
    for i in at..built.len() {
        check(i, built[i], 0);
    }
    (count, first)
}

/// Settled status both coprocessors reported after every qualified M5 power-on.
fn settled(status: &[[u32; 8]; 2]) -> bool {
    let [primary, secondary] = status;
    primary[1] == 1
        && primary[5] == 1
        && primary[4] == 0
        && primary[7] != 0
        && secondary[1] == 1
        && secondary[4] == 1
        && secondary[5] == 1
        && secondary[6] == 1
}

/// Admits the firmware, its reserved regions and the startup graph, then starts the HAL300
/// firmware and returns the retained session.
///
/// Nothing firmware-visible is written before both images are identified and the graph is
/// validated. Once either coprocessor may run, the session and this module are retained until
/// reboot, also when a later step fails.
pub(super) fn boot(
    pdev: &platform::Device<Core<'_>>,
    dev: &AsahiDrmDevice,
    cfg: &'static Config,
    hal: &'static Hal300Config,
    regs: regs::G17Resources,
    shared: &Arc<Shared>,
) -> Result<KBox<Firmware>> {
    let pdevice = pdev.as_ref();

    // Admission: reserved regions, firmware identity and graph, before any write.
    let regions = mmu::ForeignRegions {
        handoff: region(pdevice, c_str!("handoff"))?,
        ttbs: region(pdevice, c_str!("ttbs"))?,
        pagetables: region(pdevice, c_str!("pagetables"))?,
        l2_pagetables: region(pdevice, c_str!("l2-pagetables"))?,
    };
    let crash_logs = [region(pdevice, c_str!("crashlog0"))?, region(pdevice, c_str!("crashlog1"))?];
    for (role, (text, data)) in [
        (c_str!("fw-text0"), c_str!("fw-data0")),
        (c_str!("fw-text1"), c_str!("fw-data1")),
    ]
    .into_iter()
    .enumerate()
    {
        let data = region(pdevice, data)?;
        identify(pdevice, &hal.firmware[role], &region(pdevice, text)?, &data)?;
        super::capacity::FirmwareData::record(role, &data);
    }
    let sgx_base = pdev.resource_by_name(c_str!("sgx")).ok_or(EINVAL)?.start() as u64;
    let inputs = super::hal300_build::Inputs::load(pdevice, sgx_base)?;
    // The captured graph is required only without builder inputs; with them it is the reference
    // the built graph is compared with.
    let file = match inputs {
        Some(_) => kernel::firmware::Firmware::request(GRAPH_FILE, pdevice).ok(),
        None => Some(kernel::firmware::Firmware::request(GRAPH_FILE, pdevice)?),
    };
    let reference = match &file {
        Some(file) => Some(Graph::parse(file.data(), hal).inspect_err(|e| {
            dev_err!(pdevice, "HAL300: captured startup graph rejected: {:?}\n", e)
        })?),
        None => None,
    };
    let built = match inputs {
        Some(_) => Some(Graph::from_tables(hal).inspect_err(|e| {
            dev_err!(pdevice, "HAL300: built startup graph rejected: {:?}\n", e)
        })?),
        None => None,
    };
    let graph = match (&built, &reference) {
        (Some(built), _) => built,
        (None, Some(reference)) => reference,
        (None, None) => return Err(ENOENT),
    };
    if let (Some(built), Some(reference)) = (&built, &reference) {
        let same_shape = built.roots == reference.roots
            && built.windows.len() == reference.windows.len()
            && built.windows.iter().zip(reference.windows.iter()).all(|(a, b)| {
                a.va == b.va && a.phys == b.phys && a.size == b.size && a.slot == b.slot
            })
            && built.objects.len() == reference.objects.len()
            && built.objects.iter().zip(reference.objects.iter()).all(|(a, b)| {
                a.va == b.va && a.size == b.size && a.aliases == b.aliases && a.name == b.name
            });
        if !same_shape {
            dev_err!(pdevice, "HAL300: built and captured startup graphs differ in layout\n");
            return Err(EINVAL);
        }
    }
    let origin = if built.is_some() { "built by the driver" } else { "captured image" };
    dev_info!(
        pdevice,
        "HAL300: startup graph with {} objects and {} register windows ({})\n",
        graph.objects.len(),
        graph.windows.len(),
        origin
    );

    // UAT, lower mappings and objects. Nothing runs yet; errors release everything.
    let geometry = mmu::UatGeometry {
        ias: cfg.uat_ias,
        oas: cfg.uat_oas,
    };
    let uat = Arc::new(
        mmu::Uat::new_published_foreign(dev, geometry, &regions, cfg.secondary_root_offset)?,
        GFP_KERNEL,
    )?;
    super::capacity::retain_uat(&uat);
    // The firmware only samples the lower root's top-level entries when it starts, so the
    // metrics page and every lower alias are mapped first.
    let mut pm_metrics = KernelObject::new(
        dev,
        uat.kernel_lower_vm(),
        Placement::At(cfg.pm_metrics.va),
        cfg.pm_metrics.size,
        mmu::PROT_GPU_FW_SHARED_RW,
        CpuMap::WriteCombined,
    )?;
    let mut objects = KVec::with_capacity(graph.objects.len(), GFP_KERNEL)?;
    let (mut differing, mut use_reference) = (0usize, false);
    for (index, item) in graph.objects.iter().enumerate() {
        let mut object = KernelObject::backing(dev, item.size, CpuMap::WriteCombined)?;
        // M5 reports its CPU caches as coherent (CLIDR_EL1.LoC == 0), so DMA mapping performs
        // no maintenance; the RAM runtime cleaned every object explicitly.
        object.evict_cpu_caches();
        let bytes = object.bytes_mut();
        if let Some(inputs) = &inputs {
            let mut staged = KVVec::from_elem(0u8, item.size, GFP_KERNEL)?;
            super::hal300_build::fill(index, &mut staged, inputs).inspect_err(|e| {
                dev_err!(pdevice, "HAL300: building {} failed: {:?}\n",
                         core::str::from_utf8(item.name).unwrap_or("?"), e)
            })?;
            if let Some(reference) = reference.as_ref().and_then(|r| r.objects.get(index)) {
                let (count, first) = differences(&staged, reference);
                if count != 0 {
                    differing += 1;
                    dev_err!(
                        pdevice,
                        "HAL300: built {} differs from the captured graph in {} bytes, first at {:#x}\n",
                        core::str::from_utf8(item.name).unwrap_or("?"),
                        count,
                        first.unwrap_or(0)
                    );
                }
            }
            bytes.copy_from_slice(&staged);
        } else {
            for (offset, data) in item.runs.iter() {
                bytes[*offset..*offset + data.len()].copy_from_slice(data);
            }
        }
        let mut aliases = KVec::new();
        for alias in item.aliases.iter().filter(|&&alias| alias != 0) {
            aliases.push(
                object.map_alias(uat.kernel_lower_vm(), *alias, mmu::PROT_GPU_FW_SHARED_RW)?,
                GFP_KERNEL,
            )?;
        }
        let mut name = KVec::with_capacity(item.name.len(), GFP_KERNEL)?;
        name.extend_from_slice(item.name, GFP_KERNEL)?;
        objects.push(
            initdata::Hal300Object {
                name,
                object,
                aliases,
                low: item.aliases,
                high: item.va,
                prot: mmu::PROT_GPU_FW_SHARED_RW,
            },
            GFP_KERNEL,
        )?;
    }
    if differing != 0 {
        // A built graph that does not equal the qualified captured one is not used unless
        // debug_flags bit 48 asks for it: the captured objects replace it.
        use_reference = crate::module_parameters::debug_flags.value() & (1 << 48) == 0;
        if use_reference {
            let reference = reference.as_ref().ok_or(EINVAL)?;
            for (object, item) in objects.iter_mut().zip(reference.objects.iter()) {
                let bytes = object.object.bytes_mut();
                bytes.fill(0);
                for (offset, data) in item.runs.iter() {
                    bytes[*offset..*offset + data.len()].copy_from_slice(data);
                }
            }
        }
    }
    if built.is_some() {
        let verdict = if reference.is_none() {
            "no captured graph to compare, using the built one"
        } else if differing == 0 {
            "identical to the captured graph"
        } else if use_reference {
            "using the captured graph instead"
        } else {
            "using it anyway (debug_flags bit 48)"
        };
        dev_info!(
            pdevice,
            "HAL300: built startup graph: {} ({} of {} objects differ from the captured graph)\n",
            verdict,
            differing,
            graph.objects.len()
        );
    }
    // Tables the adopted compute graph does not contain. Render needs the parameter-buffer
    // descriptor table, which the firmware learns from both main configurations
    // (0xfffffe0008bc3478/0x8bc34d0/0x8bc3588), and the two stamp pages its descriptors name.
    // Compute never touches them, so adding them cannot change the qualified compute path.
    let pb = render_table(dev, &uat, b"pb-descriptors", PB_DESCRIPTORS, mmu::PROT_FW_SHARED_RW,
                          Some((PB_DESCRIPTORS_LOW, mmu::PROT_GPU_SHARED_RW)))?;
    let pb_low = PB_DESCRIPTORS_LOW;
    objects.push(pb, GFP_KERNEL)?;
    objects.push(render_table(dev, &uat, b"render-stamp-a", RENDER_STAMP_A,
                              mmu::PROT_GPU_FW_SHARED_RW, None)?, GFP_KERNEL)?;
    objects.push(render_table(dev, &uat, b"render-stamp-b", RENDER_STAMP_B,
                              mmu::PROT_GPU_FW_PRIV_RW, None)?, GFP_KERNEL)?;
    // Publish both parameter-buffer pointers in the main configurations before either
    // coprocessor starts: the primary names the context-0 alias and the firmware view, the
    // secondary only the alias.
    let bundle = objects
        .iter_mut()
        .find(|object| object.name.as_slice() == b"allocation0")
        .ok_or(EINVAL)?;
    for (main, pairs) in [
        (super::fw::initdata::HAL300.main_config[Role::Primary as usize],
         &[(MAIN_PB_LOW, pb_low), (MAIN_PB, PB_DESCRIPTORS)][..]),
        (super::fw::initdata::HAL300.main_config[Role::Secondary as usize],
         &[(MAIN_PB_LOW, pb_low)][..]),
    ] {
        for (offset, value) in pairs {
            let at = main + offset;
            let bytes = bundle.object.bytes_mut();
            bytes
                .get_mut(at..at + 8)
                .ok_or(EINVAL)?
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    // KSM scheduling-pause acknowledgement limit in microseconds (agfKSMKickQueuesSchedulingPause,
    // P+23a54 reads allocation0+0x2714). The adopted graph has 150 us; heavy passes (250k quads,
    // Minecraft's first world frame) missed it and the firmware requested recovery with
    // report+0xc = 1 (sol-render-heavy-study.md). Allow 10 ms.
    {
        let bytes = bundle.object.bytes_mut();
        let word = bytes.get_mut(KSM_PAUSE_LIMIT..KSM_PAUSE_LIMIT + 4).ok_or(EINVAL)?;
        let old = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        if old == 150 {
            word.copy_from_slice(&KSM_PAUSE_LIMIT_US.to_le_bytes());
        }
        pr_info!("asahi: HAL300 KSM scheduling-pause limit {} us -> {} us\n", old,
                  u32::from_le_bytes([word[0], word[1], word[2], word[3]]));
    }
    mem::sync();

    let primary = Coprocessor::new_retained(pdev, dev, shared, Role::Primary, crash_logs[0].clone())?;
    let secondary =
        Coprocessor::new_retained(pdev, dev, shared, Role::Secondary, crash_logs[1].clone())?;
    let render_global = uat.new_vm(0, geometry.user_range())?;
    let bridge = Bridge::enable(regs)?;
    let firmware = KBox::try_init(
        kernel::try_init!(Firmware {
            secondary,
            primary,
            bridge,
            rings: channel::Rings::new(),
            recovery: recovery::State::new(),
            effort: power::Effort::new(),
            registration_power_release_pending: false,
            queues <- runtime::Registry::new(shared.clusters, shared.descriptor_flags),
            pool: object::Pool::new()?,
            render_rings: kick::RenderRings::new()?,
            render_ids: Arc::new(freelist::RenderIds::new(), GFP_KERNEL)?,
            init: initdata::InitData::new(),
            _render_global: render_global,
            _pm_metrics: Arc::new(pm_metrics, GFP_KERNEL)?,
            uat,
            retained: true,
        }),
        GFP_KERNEL,
    )?;

    // The queues of this device use the G17G compute register program, whose storage-demand
    // arithmetic needs the enabled USC count.
    super::fw::compute::select_g17g();
    super::fw::compute::select_g17g_cores(shared.active_cores);
    // From here on the firmware may run: retain the session and this module until reboot.
    glue::retain_module();
    let firmware = KBox::into_raw(firmware);
    // SAFETY: `firmware` came from KBox::into_raw() and is only reclaimed below on success.
    let session = unsafe { &mut *firmware };
    match start(pdevice, session, &graph, &regions, objects) {
        // SAFETY: Ownership returns to the caller, which retains the session until reboot.
        Ok(()) => Ok(unsafe { KBox::from_raw(firmware) }),
        Err(error) => {
            dev_err!(
                pdevice,
                "HAL300: firmware start failed: {:?}; session retained until reboot\n",
                error
            );
            Err(error)
        }
    }
}

pub(super) fn status_a_pub(init: &initdata::InitData) -> Result<[[u32; 8]; 2]> {
    status_a(init)
}

fn status_a(init: &initdata::InitData) -> Result<[[u32; 8]; 2]> {
    Ok([
        init.status_read_words(Role::Primary, 0)?,
        init.status_read_words(Role::Secondary, 0)?,
    ])
}

/// Firmware and context-0 addresses of the tables the driver adds to the adopted graph.
/// Each is a free page of the graph's own windows: the compute objects occupy
/// 0x400000..0x1810000 and 0x2000000.., and 0x1838000 onwards is unused in both views.
const PB_DESCRIPTORS: u64 = 0xfffffc2001838000;
const PB_DESCRIPTORS_LOW: u64 = 0x7001838000;
const RENDER_STAMP_A: u64 = 0xfffffc200183c000;
const RENDER_STAMP_B: u64 = 0xfffffc2001840000;
const METRICS_FIRMWARE: u64 = 0xfffffc2001844000;
/// Offsets of the parameter-buffer pointers in a main configuration: the context-0 alias and,
/// for the primary only, the firmware view ([`abi::MainConfig`]).
const MAIN_PB_LOW: usize = 0x2d8;
const MAIN_PB: usize = 0x2e0;
/// allocation0 offset of the KSM scheduling-pause acknowledgement limit (microseconds).
const KSM_PAUSE_LIMIT: usize = 0x2714;
const KSM_PAUSE_LIMIT_US: u32 = 10_000;

/// One driver-created table of the adopted graph, zeroed, with an optional context-0 alias.
fn render_table(
    dev: &AsahiDrmDevice,
    uat: &mmu::Uat,
    name: &[u8],
    high: u64,
    prot: mmu::Prot,
    low: Option<(u64, mmu::Prot)>,
) -> Result<initdata::Hal300Object> {
    let mut object = KernelObject::backing(dev, mmu::UAT_PGSZ, CpuMap::WriteCombined)?;
    object.evict_cpu_caches();
    object.bytes_mut().fill(0);
    let mut aliases = KVec::new();
    let mut lows = [0, 0];
    if let Some((va, alias_prot)) = low {
        aliases.push(
            object.map_alias(uat.kernel_lower_vm(), va, alias_prot)?,
            GFP_KERNEL,
        )?;
        lows[0] = va;
    }
    let mut owned = KVec::with_capacity(name.len(), GFP_KERNEL)?;
    owned.extend_from_slice(name, GFP_KERNEL)?;
    Ok(initdata::Hal300Object {
        name: owned,
        object,
        aliases,
        low: lows,
        high,
        prot,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "The driver-created metrics alias is retained with the adopted graph."
)]
fn start(
    dev: &device::Device,
    session: &mut Firmware,
    graph: &Graph<'_>,
    regions: &mmu::ForeignRegions,
    mut objects: KVec<initdata::Hal300Object>,
) -> Result {
    session.primary.boot()?;
    session.secondary.boot()?;
    dev_info!(dev, "HAL300: both coprocessors booted\n");

    let snapshot = session.uat.check_foreign_window(regions.l2_pagetables.start)?;
    for object in objects.iter_mut() {
        let prot = object.prot;
        let high = object.high;
        object.object.map_primary_at(session.uat.kernel_vm(), high, prot)?;
    }
    // The render metrics lease needs the firmware view of the metrics page, which the HAL200
    // bundle builder would have created. Like every upper-half mapping it goes in after the
    // coprocessors booted: mapped before boot it landed in a table the firmware's root no
    // longer names, and the first render's scene metric update faulted on 0xfffffc2001844004
    // (render14, primary data abort at P+0xd134).
    let metrics_fw_va = METRICS_FIRMWARE;
    let metrics_alias = session._pm_metrics.map_alias(
        session.uat.kernel_vm(),
        metrics_fw_va,
        mmu::PROT_GPU_FW_SHARED_RW,
    )?;
    for window in graph.windows.iter() {
        let mapping = session.uat.kernel_vm().map_io(
            dev,
            window.va,
            window.phys as usize,
            window.size as usize,
            mmu::PROT_FW_MMIO_RW,
        )?;
        session.init.retain_mapping(mapping)?;
    }
    // IOMapping slot 14, PMSMetrologySensorRegs (native G17G: 0x380280000, 0x8000, read-only;
    // J704 ADT pmgr reg[25]; sol-hal300-iomap-study.md). The lab graph omitted it, and the
    // secondary's thermal fallback reads slot14 + 0x4800 without a null check once the GPU
    // takes a power transition (asahi-pw12-boot: crash at S+0x14224, FAR 0x4800).
    let pms_va = REGISTER_WINDOWS + PMS_SLOT * 0x10_0000;
    if graph.windows.iter().any(|window| window.va == pms_va) {
        return Err(EINVAL);
    }
    let pms = session.uat.kernel_vm().map_io(
        dev,
        pms_va,
        PMS_PHYS as usize,
        PMS_SIZE as usize,
        mmu::PROT_FW_MMIO_RO,
    )?;
    session.uat.confirm_firmware_root_entries(snapshot)?;
    mem::sync();
    mem::tlbi_all();
    mem::sync();
    session.init.adopt_hal300(objects, graph.roots, metrics_fw_va)?;
    session.init.retain_mapping(metrics_alias)?;
    session.init.retain_mapping(pms)?;
    session.init.set_io_mapping(PMS_SLOT as usize, PMS_PHYS, pms_va, PMS_SIZE, PMS_SIZE, 0)?;
    match session.init.hal300_recovery_vas() {
        Ok((summary, report)) => super::capacity::Recovery::record(summary, report),
        Err(e) => dev_warn!(dev, "HAL300: recovery report view not recorded: {:?}\n", e),
    }
    dev_info!(
        dev,
        "HAL300: {} objects and {} register windows mapped; graph layout verified\n",
        graph.objects.len(),
        graph.windows.len()
    );

    session.bridge.regs.enable_g17_gating()?;
    session.primary.init(session.init.root_va(Role::Primary)?)?;
    session.secondary.init(session.init.root_va(Role::Secondary)?)?;
    if session.primary.state.crashed.load(Ordering::Acquire)
        || session.secondary.state.crashed.load(Ordering::Acquire)
    {
        return Err(EIO);
    }
    session.primary.notify(super::MSG_POWER_ON_DONE)?;
    session.secondary.notify(super::MSG_POWER_ON_DONE)?;
    dev_info!(dev, "HAL300: both coprocessors initialized, power-on transition ended (debug_flags {:#x})\n",
        crate::module_parameters::debug_flags.value());
    if crate::g17::startup_seed_enabled() {
        return start_idle_permitted(dev, session);
    }

    let mut last = status_a(&session.init)?;
    for poll in 0..=SETTLE_POLLS {
        if session.primary.state.crashed.load(Ordering::Acquire)
            || session.secondary.state.crashed.load(Ordering::Acquire)
        {
            return Err(EIO);
        }
        let now = status_a(&session.init)?;
        if now != last || poll == 0 {
            dev_info!(dev, "HAL300: status A {:x?} {:x?}\n", now[0], now[1]);
            last = now;
        }
        if settled(&now) {
            let opening = session.init.opening_retired(Role::Primary)?
                && session.init.opening_retired(Role::Secondary)?;
            dev_info!(
                dev,
                "HAL300: firmware running; opening records retired: {}\n",
                opening
            );
            return if opening { Ok(()) } else { Err(EIO) };
        }
        if poll < SETTLE_POLLS {
            fsleep(Delta::from_millis(SETTLE_POLL_MS));
        }
    }
    Err(ETIMEDOUT)
}

/// Start with the native startup idle permission C+0x30 = 1 (debug_flags bit 55;
/// sol-hal300-descent-study.md section 6). Both coprocessors then cache permission 1 and the GPU
/// may descend as soon as it is initialized, so the awake-only `settled` tuple is not a start
/// criterion (asahi-pw3/pw4-boot). Admission is "initialized and communicating": A[1] = A[5] = 1
/// on both roles and both opening records retired. Then one counted IdlePowerOff{allow=0} wakes
/// and holds the GPU (the ordinary acquire/release lifecycle takes over afterwards), and the start
/// waits for that record's retirement and one completed primary framework transition.
fn start_idle_permitted(dev: &device::Device, session: &mut Firmware) -> Result {
    let crashed = |session: &Firmware| {
        session.primary.state.crashed.load(Ordering::Acquire)
            || session.secondary.state.crashed.load(Ordering::Acquire)
    };
    let mut last = [[u32::MAX; 8]; 2];
    let mut initialized = false;
    for poll in 0..=SETTLE_POLLS {
        if crashed(&*session) {
            return Err(EIO);
        }
        let now = status_a(&session.init)?;
        if now != last {
            dev_info!(dev, "HAL300: status A {:x?} {:x?}\n", now[0], now[1]);
            last = now;
        }
        if now[0][1] == 1
            && now[0][5] == 1
            && now[1][1] == 1
            && now[1][5] == 1
            && session.init.opening_retired(Role::Primary)?
            && session.init.opening_retired(Role::Secondary)?
        {
            initialized = true;
            break;
        }
        if poll < SETTLE_POLLS {
            fsleep(Delta::from_millis(SETTLE_POLL_MS));
        }
    }
    if !initialized {
        return Err(ETIMEDOUT);
    }
    dev_info!(dev, "HAL300: firmware initialized with startup idle permission; waking it\n");
    if !super::idle_policy_enabled() {
        // Seed-only mode: withdraw the startup permission BEFORE the wake. Both coprocessors'
        // wake paths refresh their configuration (primary P+0x306c, secondary S+0x2fc8), so
        // both cached permissions become 0 and the GPU stays awake from now on. Withdrawn only
        // after the wake (phase2n44/45, asahi-pw23/24-boot), the secondary kept 1 and still
        // gated the idle GPU, with the client-transition faults of the full idle policy.
        session.init.clear_startup_idle_permission()?;
    }
    use super::fw::channels::{ControlRecord, IdlePolicy, IdlePowerOff};
    let wake = |session: &mut Firmware| -> Result<u32> {
        let target = super::channel::Rings::publish_control(
            &session.init,
            &ControlRecord::IdlePowerOff(IdlePowerOff::new(IdlePolicy::KeepPowered)),
        )?;
        core::sync::atomic::fence(Ordering::SeqCst);
        session.primary.notify(super::MSG_CONTROL_NOTIFY)?;
        Ok(target)
    };
    // A keep-powered record that retires while the descent is still in progress leaves the GPU
    // down with nothing pending to wake it (asahi-pw54-boot: retired, no framework transition for
    // 4 s, start failed). Like power::acquire, publish another one; at most WAKES in all.
    const WAKES: usize = 4;
    const WAKE_GRACE_POLLS: usize = 50;
    let mut target = wake(session)?;
    let (mut wakes, mut retired_at) = (1usize, None::<usize>);
    for poll in 0..=SETTLE_POLLS {
        if crashed(&*session) {
            return Err(EIO);
        }
        let now = status_a(&session.init)?;
        if now != last {
            dev_info!(dev, "HAL300: status A {:x?} {:x?}\n", now[0], now[1]);
            last = now;
        }
        // asahi-pw11-boot: the wake retires and the primary completes one framework transition
        // (A0[7] 0 -> 1), but with no work queued the secondary leaves the GPU down (A1[6] = 0,
        // A1[4] = 0); work notifications and the acquire/release lifecycle power it on demand.
        let retired = session.init.control_retired(Role::Primary, target)?;
        if retired && now[0][7] != 0 && now[0][4] == 0 {
            dev_info!(
                dev,
                "HAL300: firmware running (startup idle permission; wake retired, A1[6]={})\n",
                now[1][6]
            );
            if !super::idle_policy_enabled() {
                // A wake refresh is dirty-gated (P+0x306c tests P+0x107ee0, S+0x2fc8 tests
                // S+0xf8e50), so C+0x30 = 0 alone did not reach the secondary's cached
                // permission. The native force-config snapshot (primary kind 0x15, secondary
                // kind 0x2a, uncounted) dirties and refreshes both.
                session.init.publish_role_control(Role::Primary, 0x15)?;
                session.init.publish_role_control(Role::Secondary, 0x2a)?;
                core::sync::atomic::fence(Ordering::SeqCst);
                session.primary.notify(super::MSG_CONTROL_NOTIFY)?;
                session.secondary.notify(super::MSG_CONTROL_NOTIFY)?;
                let mut refreshed = false;
                for _ in 0..SETTLE_POLLS {
                    if crashed(&*session) {
                        return Err(EIO);
                    }
                    if session
                        .init
                        .control_retired(Role::Primary, target.wrapping_add(1))?
                        && session.init.control_retired(
                            Role::Secondary,
                            crate::g17::fw::initdata::OPENING_PRODUCER + 1,
                        )?
                    {
                        refreshed = true;
                        break;
                    }
                    fsleep(Delta::from_millis(SETTLE_POLL_MS));
                }
                dev_info!(
                    dev,
                    "HAL300: startup idle permission withdrawn; configuration snapshot {}\n",
                    if refreshed { "retired on both coprocessors" } else { "NOT retired" }
                );
            }
            return Ok(());
        }
        if retired {
            let since = *retired_at.get_or_insert(poll);
            if poll - since >= WAKE_GRACE_POLLS && wakes < WAKES {
                dev_info!(dev, "HAL300: wake retired without a power transition; waking again\n");
                target = wake(session)?;
                wakes += 1;
                retired_at = None;
            }
        }
        if poll < SETTLE_POLLS {
            fsleep(Delta::from_millis(SETTLE_POLL_MS));
        }
    }
    dev_err!(dev, "HAL300: wake after startup idle did not complete\n");
    Err(ETIMEDOUT)
}
