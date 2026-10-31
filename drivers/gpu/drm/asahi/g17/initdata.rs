// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Allocates, initializes and retains the graph shared by the two firmware instances.
//!
//! The owner is installed in the firmware session before construction starts. Even if an
//! allocation fails, the session stops both coprocessors before releasing any published memory.

use super::{
    fw::initdata as abi,
    object::{CpuMap, KernelObject, Placement},
    Config, Role,
};
use crate::{driver::AsahiDrmDevice, hw::t8140, mmu};
use core::ops::Range;
use core::sync::atomic::{fence, AtomicU32, Ordering};
use kernel::{c_str, prelude::*};
use t8140::{kernel_window as high, low};

const EVENT_RING_RECHECK: u32 = 256;

/// Bundle offset of the HAL300 hardware data's completion-ring table: four (low, high) pairs
/// 0x20 bytes apart.
const HAL300_COMPLETION_TABLE: usize = 0x2734;

const INDEX_SIZE: usize = 0x10000;
const CONTEXT_SIZE: usize = 0x20000;
const REGION_A_SIZE: usize = core::mem::size_of::<abi::RegionA>();
const REGION_C_SIZE: usize = 0x1000;

const COMPUTE_BLOCKS_MAPPED: usize = 22;
const COMPUTE_BLOCKS_POPULATED: usize = 21;
const COMPUTE_CONTROL_BLOCKS: u32 = 25;
const COMPUTE_CONTROL_CURSOR: u32 = 0x18;
const COMPUTE_RUN_ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;

/// Timestamp bits of a completion record word (as the queue's observation fold masks them).
const COMPLETION_TIME_MASK: u64 = 0x003f_ffff_ffff_ffff;

/// Compute completion scan state: the slot after the newest record seen, that record's end time
/// (0 before the first full scan) and when a new record was last found.
#[derive(Clone, Copy)]
struct CompletionScan {
    head: usize,
    newest: u64,
    progress_ns: u64,
}

/// A scan that finds no new record for this long while work waits for one rereads the whole ring.
const COMPLETION_STALL_NS: u64 = 100_000_000;

/// Compute completion scan statistics, shown by debugfs `capacity`: scans, full scans, slots
/// read, and exact records latched by a queue.
pub(crate) static COMPLETION_SCANS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
pub(crate) static COMPLETION_FULL_SCANS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
pub(crate) static COMPLETION_SLOTS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
pub(crate) static COMPLETION_LATCHED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// A borrowed completion slot. Read the identity before requesting its timestamps;
/// unrelated QIDs need only the control word. Each access is an aligned live load.
pub(crate) struct ComputeCompletion<'a> {
    object: &'a KernelObject,
    offset: usize,
}

impl ComputeCompletion<'_> {
    pub(crate) fn read(&self, word: usize) -> Result<u64> {
        if word >= 8 {
            return Err(EINVAL);
        }
        // The scanner bounds offset to a 0x40-byte slot in the completion ring.
        Ok(self
            .object
            .dword(self.offset + word * 8)?
            .load(Ordering::Relaxed))
    }
}

/// Unpublished device-wide compute backing, prepared without the device mutex. The cache
/// identity belongs to this allocation, not to any one physical queue or client VM.
pub(super) struct ComputeShared {
    id: u64,
    pages: KernelObject,
    runs: KernelObject,
    blocks: KVec<KernelObject>,
}

impl ComputeShared {
    pub(super) fn new(alloc: &super::object::Allocator<'_>) -> Result<Self> {
        let pages =
            KernelObject::backing(alloc.dev, low::RENDER_PAGE_LIST_SIZE, CpuMap::WriteCombined)?;
        let runs =
            KernelObject::backing(alloc.dev, low::RENDER_RUN_LIST_SIZE, CpuMap::WriteCombined)?;
        let mut blocks = KVec::with_capacity(COMPUTE_BLOCKS_MAPPED, GFP_KERNEL)?;
        for _ in 0..COMPUTE_BLOCKS_MAPPED {
            blocks.push(
                KernelObject::backing(alloc.dev, abi::FREE_LIST_BLOCK_SIZE, CpuMap::WriteCombined)?,
                GFP_KERNEL,
            )?;
        }
        let id = u64::from(super::ids::OBJECTS.object()?);
        Ok(Self {
            id,
            pages,
            runs,
            blocks,
        })
    }
}

/// A device-global scheduler, retained independently of whichever physical queue first
/// needed it. Its generation reservation precedes later graph allocation failures.
pub(super) struct ComputeScheduler {
    object: KernelObject,
}

impl ComputeScheduler {
    pub(super) fn new(alloc: &super::object::Allocator<'_>) -> Result<Self> {
        let mut object = alloc.kernel(
            mmu::UAT_PGSZ,
            mmu::UAT_PGSZ as u64,
            mmu::PROT_GPU_FW_SHARED_RW,
            CpuMap::WriteCombined,
        )?;
        object.initialize::<super::fw::context::Scheduler>(0, |state| {
            state.init();
            Ok(())
        })?;
        fence(Ordering::SeqCst);
        // Scheduler creation reserves a generation even when only its address is used by work.
        super::ids::OBJECTS.object()?;
        Ok(Self { object })
    }
}

/// Checked views used only within one publication call. Every fallible operation completes
/// before either member of a paired render publication is copied into shared memory.
struct WorkPublication<'a> {
    record: &'a super::fw::channels::WorkSlot,
    destination: *mut u8,
    producer: &'a AtomicU32,
    next: u32,
}

impl WorkPublication<'_> {
    fn copy_record(&self) {
        // SAFETY: work_publication checked the complete destination range. The record is a
        // fully initialized padding-free ABI value, and the device mutex owns publication.
        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::from_ref(self.record).cast::<u8>(),
                self.destination,
                core::mem::size_of_val(self.record),
            );
        }
    }

    fn commit(self) -> u32 {
        self.copy_record();
        fence(Ordering::SeqCst);
        self.producer.store(self.next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        self.next
    }
}

/// An object of the adopted HAL300 graph, with its fixed address, its context-0 aliases and
/// its name in the graph.
pub(super) struct Hal300Object {
    pub(super) name: KVec<u8>,
    pub(super) object: KernelObject,
    pub(super) aliases: KVec<mmu::KernelMapping>,
    pub(super) low: [u64; 2],
    /// Firmware address this object is mapped at, from the graph or chosen by the driver for
    /// an object the adopted graph does not contain.
    pub(super) high: u64,
    /// Protection of that firmware mapping.
    pub(super) prot: mmu::Prot,
}

/// What the runtime needs from an adopted HAL300 graph beyond the shared object indices.
struct Hal300Graph {
    /// Object indices and context-0 addresses of the four completion rings.
    completions: [(usize, u64); 4],
    /// Initialization root of each instance.
    roots: [u64; 2],
    /// Identity of the completion aliases installed in client VMs.
    client_id: u64,
    /// Effort input of the last publication, for the read-only power observation.
    effort: core::cell::Cell<u32>,
}

/// Session-long storage and mappings. Aliases are released before their backing objects.
pub(super) struct InitData {
    // Live publication uses interior writes under the device mutex. This owner is Send but not
    // Sync, so safe host code cannot issue those writes concurrently through shared references.
    _single_publisher: core::marker::PhantomData<core::cell::Cell<()>>,
    aliases: KVec<mmu::KernelMapping>,
    objects: KVec<KernelObject>,
    roots: usize,
    cluster: usize,
    bundle: usize,
    region_c: usize,
    qos: usize,
    pb: usize,
    page_pool: usize,
    compute_global: usize,
    boot_control: usize,
    boot_state: usize,
    metrics_fw_va: u64,
    compute_shared: Option<ComputeShared>,
    compute_scheduler: Option<ComputeScheduler>,
    compute_active: core::cell::Cell<bool>,
    event_ring_checked: [core::cell::Cell<(u64, u64, u32)>; 2],
    /// Where the next compute completion scan starts (see `scan_compute_completions`).
    completion_scan: core::cell::Cell<CompletionScan>,
    completion_rings: [usize; 2],
    stamps: [usize; 2],
    /// Offsets of the shared rings and state in this graph.
    layout: &'static abi::Layout,
    /// HAL300: the adopted graph, its completion rings and their client aliases, the root
    /// addresses, the identity of its client mappings and the effort input the host last set.
    hal300: Option<Hal300Graph>,
    render_pages: usize,
    render_runs: usize,
    render_blocks: core::ops::Range<usize>,
}

/// Placement inputs shared by the build steps.
struct BuildEnv<'a> {
    dev: &'a AsahiDrmDevice,
    vm: &'a mmu::Vm,
    lower: &'a mmu::Vm,
    render: &'a mmu::Vm,
    base: u64,
    dynamic: Range<u64>,
}

impl BuildEnv<'_> {
    /// An object at a fixed offset of the kernel window.
    fn fixed(&self, offset: u64, size: usize, prot: mmu::Prot, cpu: CpuMap) -> Result<KernelObject> {
        KernelObject::new(
            self.dev,
            self.vm,
            Placement::At(self.base + offset),
            size,
            prot,
            cpu,
        )
    }

    /// An object placed in the dynamic part of the kernel window.
    fn dynamic(&self, size: usize, align: u64, prot: mmu::Prot, cpu: CpuMap) -> Result<KernelObject> {
        KernelObject::new(
            self.dev,
            self.vm,
            Placement::In(self.dynamic.clone(), align),
            size,
            prot,
            cpu,
        )
    }
}

/// Object indices the firmware-object initialization needs from the table step.
struct DescriptorTables {
    pb: usize,
    page_pool: usize,
    fwctl: usize,
}

/// Object indices and ring addresses the firmware-object initialization needs from the bundle step.
struct BundleObjects {
    zero: usize,
    completion_rings: [abi::CompletionRing; 2],
    region_a: usize,
    qos: usize,
}

impl InitData {
    /// Empty construction state, retained even if graph construction fails.
    pub(super) fn new() -> Self {
        Self {
            _single_publisher: core::marker::PhantomData,
            aliases: KVec::new(),
            objects: KVec::new(),
            roots: 0,
            cluster: 0,
            bundle: 0,
            region_c: 0,
            qos: 0,
            pb: 0,
            page_pool: 0,
            compute_global: 0,
            boot_control: 0,
            boot_state: 0,
            metrics_fw_va: 0,
            compute_shared: None,
            compute_scheduler: None,
            compute_active: core::cell::Cell::new(false),
            event_ring_checked: [const { core::cell::Cell::new((0, 0, 0)) }; 2],
            completion_scan: core::cell::Cell::new(CompletionScan {
                head: 0,
                newest: 0,
                progress_ns: 0,
            }),
            completion_rings: [0; 2],
            stamps: [0; 2],
            layout: &abi::HAL200,
            hal300: None,
            render_pages: 0,
            render_runs: 0,
            render_blocks: 0..0,
        }
    }

    pub(super) fn metrics_fw_va(&self) -> Result<u64> {
        if self.metrics_fw_va == 0 {
            return Err(EIO);
        }
        Ok(self.metrics_fw_va)
    }

    pub(super) fn pb_descriptor_table(&self) -> Result<&KernelObject> {
        self.object(self.pb)
    }

    /// The adopted graph's pool rows use the G17P packing (unit 0x40000 at D0 bit 41, lanes
    /// shifted by 5/33). Rewrite them, before the firmware starts, in the native G17G packing
    /// that every row the driver writes now uses (PagePoolDescriptor::g17g).
    fn convert_g17g_pool_rows(&self) -> Result {
        let object = self.object(self.page_pool)?;
        let size = core::mem::size_of::<abi::PagePoolDescriptor>();
        for row in 0..mmu::UAT_PGSZ / size {
            let word = |index: usize| object.dword(row * size + index * 8);
            let d0 = word(0)?.load(Ordering::Relaxed);
            // Only G17P-packed rows (unit 0x40000 above the page-list address).
            if d0 >> 41 != 0x4_0000 {
                continue;
            }
            let d1 = word(1)?.load(Ordering::Relaxed);
            let d2 = word(2)?.load(Ordering::Relaxed);
            let va = (d0 & ((1u64 << 41) - 1)) << 7;
            let row_g17g = abi::PagePoolDescriptor::g17g(
                va,
                ((d1 >> 5) & 0x0fff_ffff) as u32,
                ((d1 >> 33) & 0x0fff_ffff) as u32,
                (d1 >> 61) as u32,
                (d2 & 0x0fff_ffff) as u32,
                0x4_0000,
            );
            pr_info!("asahi: HAL300 pool row {} {:#x} {:#x} {:#x} -> {:#x} {:#x} {:#x}\n",
                     row, d0, d1, d2, row_g17g.page_list, row_g17g.counters, row_g17g.page_count);
            word(0)?.store(row_g17g.page_list, Ordering::Relaxed);
            word(1)?.store(row_g17g.counters, Ordering::Relaxed);
            word(2)?.store(row_g17g.page_count, Ordering::Relaxed);
            word(3)?.store(0, Ordering::Relaxed);
        }
        core::sync::atomic::fence(Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn page_pool_descriptor_table(&self) -> Result<&KernelObject> {
        self.object(self.page_pool)
    }

    pub(super) fn stamp_addresses(&self, qid: u8) -> Result<[u64; 2]> {
        if qid > super::fw::kick::QID_MAX {
            return Err(EINVAL);
        }
        let offset = u64::from(qid) * core::mem::size_of::<u32>() as u64;
        Ok([
            self.object(self.stamps[0])?.gpu_va() + offset,
            self.object(self.stamps[1])?.gpu_va() + offset,
        ])
    }

    /// The caller reads both engine lanes before its acquire fence. Page A is the completed
    /// GPU-maintenance witness; page B stays under firmware ownership.
    pub(super) fn stamp(&self, qid: u8) -> Result<&AtomicU32> {
        if qid > super::fw::kick::QID_MAX {
            return Err(EINVAL);
        }
        self.object(self.stamps[0])?
            .word(usize::from(qid) * core::mem::size_of::<u32>())
    }

    /// Initialize a fresh physical compute QID's row before exposing its queue. An installed
    /// or rebound queue keeps the row's firmware-owned counters and must never call this again.
    /// Called for a fresh row or after its free-list release was consumed.
    pub(super) fn initialize_compute_pool_blocks(&self, slot: u16, page_list_va: u64, blocks: usize) -> Result {
        if slot == 0 || slot > u16::from(super::fw::kick::QID_MAX) + 1 {
            return Err(EINVAL);
        }
        let entry = abi::PagePoolDescriptor::compute_blocks(page_list_va, blocks);
        let size = core::mem::size_of::<abi::PagePoolDescriptor>();
        let offset = usize::from(slot) * size;
        let object = self.object(self.page_pool)?;
        let pointer = object.pointer(offset, size)?;
        let page_list = object.dword(offset)?;
        let counters =
            object.dword(offset + core::mem::offset_of!(abi::PagePoolDescriptor, counters))?;
        let count =
            object.dword(offset + core::mem::offset_of!(abi::PagePoolDescriptor, page_count))?;
        // SAFETY: the whole fresh row was bounds checked, this is a fresh QID or its release
        // was consumed, and the device mutex excludes another host publisher. No other row is touched.
        unsafe {
            pointer.write_bytes(0, size);
        }
        page_list.store(entry.page_list, Ordering::Relaxed);
        counters.store(entry.counters, Ordering::Relaxed);
        count.store(entry.page_count, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn qos(&self) -> Result<&KernelObject> {
        self.object(self.qos)
    }

    pub(super) fn has_compute_scheduler(&self) -> bool {
        self.compute_scheduler.is_some()
    }

    /// Only the reserved singleton builder may install a scheduler. Retain it before building
    /// the first descriptor graph so later construction failure does not allocate it again.
    pub(super) fn install_compute_scheduler(&mut self, scheduler: ComputeScheduler) -> Result {
        if self.compute_scheduler.is_some() {
            return Err(EBUSY);
        }
        self.compute_scheduler = Some(scheduler);
        Ok(())
    }

    pub(super) fn has_compute_shared(&self) -> bool {
        // The HAL300 graph has no shared compute aperture; clients only need its completion
        // rings, which cache_compute_vm() maps.
        self.hal300.is_some() || self.compute_shared.is_some()
    }

    /// Install a prepared aperture once while holding the device mutex. A competing preparer
    /// may have installed its allocation first; the redundant unpublished backing then drops.
    pub(super) fn install_compute_shared(&mut self, mut shared: ComputeShared) -> Result {
        if self.compute_shared.is_some() {
            return Ok(());
        }
        let state_va = self.object(self.boot_state)?.gpu_va();
        let mut control = abi::FreeListControl::new(&abi::FreeListArgs {
            buffer_slot: u32::MAX,
            class: abi::FreeListClass::Two,
            page_list_va: low::COMPUTE_POOL_PAGE_LIST,
            run_list_va: low::COMPUTE_POOL_RUN_LIST,
            blocks: COMPUTE_CONTROL_BLOCKS,
            state_va,
        });
        control.run_cursor = COMPUTE_CONTROL_CURSOR;
        self.object_mut(self.boot_control)?.bytes_mut().fill(0);
        self.object_mut(self.boot_control)?.write(0, control)?;
        self.object_mut(self.boot_state)?.bytes_mut().fill(0);
        shared
            .pages
            .initialize::<[u64; low::RENDER_PAGE_LIST_SIZE / 8]>(0, |pages| {
                abi::fill_page_list(pages, (0..COMPUTE_BLOCKS_POPULATED).map(low::compute_block))
            })?;
        shared
            .runs
            .initialize::<[u64; low::RENDER_RUN_LIST_SIZE / 8]>(0, |runs| {
                for (index, run) in runs.iter_mut().take(COMPUTE_BLOCKS_POPULATED).enumerate() {
                    *run = (low::compute_block(index) & COMPUTE_RUN_ADDRESS_MASK)
                        | (u64::from(abi::FREE_LIST_BLOCK_PAGES) << 52);
                }
                Ok(())
            })?;
        fence(Ordering::SeqCst);
        self.compute_shared = Some(shared);
        Ok(())
    }

    pub(super) fn cache_compute_vm(&self, vm: &mmu::Vm) -> Result {
        if let Some(graph) = self.hal300.as_ref() {
            if vm.compute_pool_mappings_match(graph.client_id) {
                return Ok(());
            }
            vm.clear_compute_pool_mappings();
            let mut mappings = KVec::with_capacity(graph.completions.len(), GFP_KERNEL)?;
            for (index, low) in graph.completions {
                mappings.push(
                    self.object(index)?.map_alias(vm, low, mmu::PROT_GPU_SHARED_RW)?,
                    GFP_KERNEL,
                )?;
            }
            return vm.install_compute_pool_mappings(graph.client_id, mappings);
        }
        let shared = self.compute_shared.as_ref().ok_or(EIO)?;
        if vm.compute_pool_mappings_match(shared.id) {
            return Ok(());
        }
        vm.clear_compute_pool_mappings();
        let mut mappings = KVec::with_capacity(26, GFP_KERNEL)?;
        mappings.push(
            shared
                .pages
                .map_alias(vm, low::COMPUTE_POOL_PAGE_LIST, mmu::PROT_GPU_SHARED_RW)?,
            GFP_KERNEL,
        )?;
        mappings.push(
            shared
                .runs
                .map_alias(vm, low::COMPUTE_POOL_RUN_LIST, mmu::PROT_GPU_SHARED_RW)?,
            GFP_KERNEL,
        )?;
        for (index, block) in shared.blocks.iter().take(COMPUTE_BLOCKS_MAPPED).enumerate() {
            mappings.push(
                block.map_alias(vm, low::compute_block(index), mmu::PROT_GPU_SHARED_RW)?,
                GFP_KERNEL,
            )?;
        }
        for slot in 0..2 {
            mappings.push(
                self.object(self.completion_rings[slot])?.map_alias(
                    vm,
                    low::COMPLETION_RINGS[slot],
                    mmu::PROT_GPU_SHARED_RW,
                )?,
                GFP_KERNEL,
            )?;
        }
        vm.install_compute_pool_mappings(shared.id, mappings)
    }

    /// The descriptor-table views remain immutable for the firmware session.
    pub(super) fn validate_descriptor_views(&self) -> Result {
        if self.hal300.is_some() {
            // The adopted graph names only the page-pool descriptor table.
            let base = self.layout.main_config[Role::Primary as usize]
                + core::mem::offset_of!(abi::MainConfig, page_pool_descriptors_low_va);
            let bundle = self.object(self.bundle)?;
            let table = self.object(self.page_pool)?.gpu_va();
            let low = bundle.dword(base)?.load(Ordering::Relaxed);
            let high = bundle.dword(base + 8)?.load(Ordering::Relaxed);
            fence(Ordering::SeqCst);
            return if low == 0 || high != table { Err(EIO) } else { Ok(()) };
        }
        let values = [
            low::PB_DESCRIPTORS,
            self.object(self.pb)?.gpu_va(),
            low::PAGE_POOL_DESCRIPTORS,
            self.object(self.page_pool)?.gpu_va(),
        ];
        let base = abi::bundle::MAIN_CONFIG[Role::Primary as usize]
            + core::mem::offset_of!(abi::MainConfig, pb_descriptors_low_va);
        let bundle = self.object(self.bundle)?;
        let mut changed = false;
        for (index, value) in values.into_iter().enumerate() {
            changed |= bundle.dword(base + index * 8)?.load(Ordering::Relaxed) != value;
        }
        fence(Ordering::SeqCst);
        if changed {
            Err(EIO)
        } else {
            Ok(())
        }
    }

    pub(super) fn activate_compute(&self) -> Result {
        self.validate_descriptor_views()?;
        if self.compute_active.get() {
            return Ok(());
        }
        if self.hal300.is_some() {
            // The qualified HAL300 dispatches never wrote a compute global state; the queue
            // configuration and the QoS publication name the scheduler instead.
            self.compute_active.set(true);
            return Ok(());
        }
        let scheduler = self.compute_scheduler.as_ref().ok_or(EIO)?.object.gpu_va();
        let object = self.object(self.compute_global)?;
        let state = object
            .pointer(0, core::mem::size_of::<abi::ComputeGlobalState>())?
            .cast::<abi::ComputeGlobalState>();
        // SAFETY: The complete typed view is in the retained page-aligned object. InitData is
        // not Sync and its caller holds the sole publication mutex. Each named field is
        // naturally aligned and written individually; no reference aliases firmware state.
        unsafe {
            core::ptr::addr_of_mut!((*state).unk_0).write_volatile(0xff);
            core::ptr::addr_of_mut!((*state).unk_8).write_volatile(0xff);
            core::ptr::addr_of_mut!((*state).unk_10).write_volatile(0x0f01);
            core::ptr::addr_of_mut!((*state).unk_18).write_volatile(0x0f01);
            core::ptr::addr_of_mut!((*state).unk_408).write_volatile(2);
            core::ptr::addr_of_mut!((*state).unk_40c).write_volatile(2);
            core::ptr::addr_of_mut!((*state).unk_604).write_volatile(4);
            core::ptr::addr_of_mut!((*state).scheduler_va).write_volatile(scheduler);
            core::ptr::addr_of_mut!((*state).unk_c04).write_volatile(0x1000);
            core::ptr::addr_of_mut!((*state).unk_e00).write_volatile(7);
            core::ptr::addr_of_mut!((*state).unk_e08).write_volatile(0x0000_0001_0e22_618a);
            core::ptr::addr_of_mut!((*state).unk_e20).write_volatile(4);
            core::ptr::addr_of_mut!((*state).unk_e28).write_volatile(4);
        }
        self.compute_active.set(true);
        Ok(())
    }

    /// The sole host publisher holds the device mutex through the following
    /// inner and outer publication. Firmware consumers can only free capacity;
    /// neither this check nor waiting for room reserves or limits other queues.
    /// `priority` selects the engine's ring of that priority class.
    pub(super) fn work_ready(
        &self,
        priority: u8,
        engine: super::fw::queue::DataMaster,
    ) -> Result<bool> {
        let [consumer, consumer_b, producer] = self.work_cursors(priority, engine)?;
        let next = (producer + 1) % super::fw::channels::WORK_RING_SLOTS;
        Ok(next != consumer && next != consumer_b)
    }

    fn work_publication<'a>(
        &'a self,
        priority: u8,
        record: &'a super::fw::channels::WorkSlot,
    ) -> Result<WorkPublication<'a>> {
        use super::fw::{channels, queue::DataMaster};
        let engine = match record.data_master {
            0 => DataMaster::Tiling,
            1 => DataMaster::Fragment,
            2 => DataMaster::Compute,
            _ => return Err(EINVAL),
        };
        let channel = channels::work_ring_index(priority, engine).ok_or(EINVAL)?;
        let [consumer, consumer_b, producer] = self.work_cursors(priority, engine)?;
        let next = (producer + 1) % channels::WORK_RING_SLOTS;
        if next == consumer || next == consumer_b {
            return Err(EINVAL);
        }
        let destination = self.object(self.bundle)?.pointer(
            self.layout.work_ring_offset(channel) + producer as usize * core::mem::size_of_val(record),
            core::mem::size_of_val(record),
        )?;
        let producer = self.object(self.cluster)?.word(
            self.state_offset(Role::Primary)
                + abi::work_state_offset(channel)
                + abi::ChannelState::PRODUCER,
        )?;
        Ok(WorkPublication {
            record,
            destination,
            producer,
            next,
        })
    }

    /// Two firmware consumers and the host producer, sampled in that order.
    pub(super) fn work_cursors(
        &self,
        priority: u8,
        engine: super::fw::queue::DataMaster,
    ) -> Result<[u32; 3]> {
        use super::fw::channels;
        let channel = channels::work_ring_index(priority, engine).ok_or(EINVAL)?;
        let base = self.state_offset(Role::Primary) + abi::work_state_offset(channel);
        let state = self.object(self.cluster)?;
        let mut values = [0; 3];
        for (value, offset) in values.iter_mut().zip([
            abi::ChannelState::CONSUMER,
            abi::ChannelState::UNK_10,
            abi::ChannelState::PRODUCER,
        ]) {
            *value = state.word(base + offset)?.load(Ordering::Relaxed);
        }
        if values
            .iter()
            .any(|&value| value >= channels::WORK_RING_SLOTS)
        {
            return Err(EINVAL);
        }
        Ok(values)
    }

    /// Copy of the outer work-ring record at `index` of the channel (read-only diagnostics).
    pub(super) fn work_record(
        &self,
        priority: u8,
        engine: super::fw::queue::DataMaster,
        index: u32,
    ) -> Result<[u8; core::mem::size_of::<super::fw::channels::WorkSlot>()]> {
        use super::fw::channels;
        let channel = channels::work_ring_index(priority, engine).ok_or(EINVAL)?;
        if index >= channels::WORK_RING_SLOTS {
            return Err(EINVAL);
        }
        let mut record = [0u8; core::mem::size_of::<super::fw::channels::WorkSlot>()];
        let source = self.object(self.bundle)?.pointer(
            self.layout.work_ring_offset(channel) + index as usize * record.len(),
            record.len(),
        )?;
        // SAFETY: `pointer()` checked the range against the live mapping; this is a plain copy.
        unsafe { core::ptr::copy_nonoverlapping(source, record.as_mut_ptr(), record.len()) };
        Ok(record)
    }

    /// Inner queue records must be visible before this outer publication. The returned
    /// producer is the retained consumer witness for this slot.
    pub(super) fn publish_work(
        &self,
        priority: u8,
        record: &super::fw::channels::WorkSlot,
    ) -> Result<u32> {
        Ok(self.work_publication(priority, record)?.commit())
    }

    /// Validate both destinations before exposing fragment, then tiling. The accounting token
    /// is committed after copying fragment; late tiling state is published before its slot.
    /// Deferred pairs retain both slot bodies without exposing either producer.
    pub(super) fn publish_work_pair(
        &self,
        priority: u8,
        fragment: &super::fw::channels::WorkSlot,
        tiling: &super::fw::channels::WorkSlot,
        deferred: bool,
        commit_accounting: impl FnOnce() -> Result,
        publish_late_tiling: impl FnOnce(),
    ) -> Result<[u32; 2]> {
        use super::fw::queue::DataMaster;
        if fragment.data_master != DataMaster::Fragment as u32
            || tiling.data_master != DataMaster::Tiling as u32
        {
            return Err(EINVAL);
        }
        let fragment = self.work_publication(priority, fragment)?;
        let tiling = self.work_publication(priority, tiling)?;
        fragment.copy_record();
        commit_accounting()?;
        fence(Ordering::SeqCst);
        if !deferred {
            fragment.producer.store(fragment.next, Ordering::Relaxed);
        }
        publish_late_tiling();
        tiling.copy_record();
        fence(Ordering::SeqCst);
        if !deferred {
            tiling.producer.store(tiling.next, Ordering::Relaxed);
        }
        fence(Ordering::SeqCst);
        Ok([fragment.next, tiling.next])
    }

    fn retained_work_publication<'a>(
        &'a self,
        priority: u8,
        record: &'a super::fw::channels::WorkSlot,
        next: u32,
    ) -> Result<WorkPublication<'a>> {
        use super::fw::{channels, queue::DataMaster};
        let engine = match record.data_master {
            0 => DataMaster::Tiling,
            1 => DataMaster::Fragment,
            _ => return Err(EINVAL),
        };
        let channel = channels::work_ring_index(priority, engine).ok_or(EINVAL)?;
        let producer = self.object(self.cluster)?.word(
            self.state_offset(Role::Primary)
                + abi::work_state_offset(channel)
                + abi::ChannelState::PRODUCER,
        )?;
        let current = producer.load(Ordering::Relaxed);
        if current >= channels::WORK_RING_SLOTS || next != (current + 1) % channels::WORK_RING_SLOTS
        {
            return Err(EBUSY);
        }
        let destination = self.object(self.bundle)?.pointer(
            self.layout.work_ring_offset(channel) + current as usize * core::mem::size_of_val(record),
            core::mem::size_of_val(record),
        )?;
        Ok(WorkPublication {
            record,
            destination,
            producer,
            next,
        })
    }

    /// Release a retained pair after its control transaction. Revalidate the original producer
    /// witnesses before either copy; both bodies precede the fragment and tiling publications.
    pub(super) fn publish_retained_work_pair(
        &self,
        priority: u8,
        fragment: &super::fw::channels::WorkSlot,
        tiling: &super::fw::channels::WorkSlot,
        next: [u32; 2],
    ) -> Result {
        use super::fw::queue::DataMaster;
        if fragment.data_master != DataMaster::Fragment as u32
            || tiling.data_master != DataMaster::Tiling as u32
        {
            return Err(EINVAL);
        }
        let fragment = self.retained_work_publication(priority, fragment, next[0])?;
        let tiling = self.retained_work_publication(priority, tiling, next[1])?;
        fragment.copy_record();
        tiling.copy_record();
        fence(Ordering::SeqCst);
        fragment.producer.store(fragment.next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        tiling.producer.store(tiling.next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// Snapshot the shared compute completion ring once per service transaction. The queue
    /// scanner latches exact identities; firmware retains sole ownership of these slots.
    ///
    /// HAL300 scans incrementally: the firmware fills slots in order, so only slots past the
    /// newest record seen last time, or from slot 0 after a restart, can hold new ones. Reading
    /// all 2048 write-combined slots took 0.2-0.7 ms per event turn with the device mutex held,
    /// which every submission waited for (Arch desktop 2026-10-31: run_job to doorbell up to
    /// 1.5 ms). A record missed here only costs the recordless one-pass grace; `full`, the first
    /// scan and a scan without a new record for `COMPLETION_STALL_NS` read the whole ring.
    pub(super) fn scan_compute_completions(
        &self,
        full: bool,
        mut record: impl FnMut(ComputeCompletion<'_>) -> Result,
    ) -> Result {
        let ring = match self.hal300.as_ref() {
            Some(graph) => graph.completions[self.layout.compute_completions].0,
            None => self.completion_rings[self.layout.compute_completions],
        };
        let object = self.object(ring)?;
        fence(Ordering::Acquire);
        let slots = (abi::COMPLETION_RING_SIZE / 0x40).min(0x800);
        if self.hal300.is_none() {
            for slot in 0..slots {
                record(ComputeCompletion {
                    object,
                    offset: slot * 0x40,
                })?;
            }
            return Ok(());
        }
        // A record's start (word 2) and end (word 5) in 24 MHz ticks.
        let times = |slot: usize| -> Result<(u64, u64)> {
            let word = |index: usize| -> Result<u64> {
                Ok(object.dword(slot * 0x40 + index * 8)?.load(Ordering::Relaxed)
                    & COMPLETION_TIME_MASK)
            };
            Ok((word(2)?, word(5)?))
        };
        // G17G never measures a timestamp-only command; a zero interval there is not the G17P
        // proof that the command did not execute, so such a record stays unlatched and the
        // command keeps the recordless path it always had.
        let mut visit = |slot: usize, (start, end): (u64, u64)| -> Result {
            if start != 0 && start == end {
                return Ok(());
            }
            record(ComputeCompletion {
                object,
                offset: slot * 0x40,
            })
        };
        let scan = self.completion_scan.get();
        let now = super::now_ns();
        COMPLETION_SCANS.fetch_add(1, Ordering::Relaxed);
        if full
            || scan.newest == 0
            || scan.head >= slots
            || now.saturating_sub(scan.progress_ns) > COMPLETION_STALL_NS
        {
            COMPLETION_FULL_SCANS.fetch_add(1, Ordering::Relaxed);
            let mut best = (0, 0);
            for slot in 0..slots {
                let times = times(slot)?;
                if times.1 > best.1 {
                    best = (slot, times.1);
                }
                visit(slot, times)?;
            }
            COMPLETION_SLOTS.fetch_add(slots as u64, Ordering::Relaxed);
            self.completion_scan.set(CompletionScan {
                head: (best.0 + 1) % slots,
                newest: best.1,
                progress_ns: now,
            });
            return Ok(());
        }
        // The firmware restarts at slot 0 whenever the GPU powers up again (idle power-off
        // descends many times a second on an idle desktop) and otherwise continues after the
        // last record. So walk from the head and, if slot 0 holds a record newer than the newest
        // seen, from slot 0 as well; each walk ends at a run of records no newer than that, which
        // is older data. A few slots before the head are revisited for records that were still
        // being written. Visiting a slot twice is harmless: latching keeps the newest identity.
        const LOOKBACK: usize = 16;
        const OLD_RUN: usize = 8;
        let mut visited = 0u64;
        for back in (1..=LOOKBACK).rev() {
            let slot = (scan.head + slots - back) % slots;
            visit(slot, times(slot)?)?;
            visited += 1;
        }
        let mut best = (scan.head, scan.newest);
        let mut walk = |start: usize, best: &mut (usize, u64), visited: &mut u64| -> Result {
            let mut old = 0;
            for step in 0..slots {
                let slot = (start + step) % slots;
                let times = times(slot)?;
                visit(slot, times)?;
                *visited += 1;
                if times.1 > scan.newest {
                    old = 0;
                    if times.1 > best.1 {
                        *best = ((slot + 1) % slots, times.1);
                    }
                } else {
                    old += 1;
                    if old >= OLD_RUN {
                        break;
                    }
                }
            }
            Ok(())
        };
        walk(scan.head, &mut best, &mut visited)?;
        if scan.head != 0 && times(0)?.1 > scan.newest {
            walk(0, &mut best, &mut visited)?;
        }
        COMPLETION_SLOTS.fetch_add(visited, Ordering::Relaxed);
        self.completion_scan.set(CompletionScan {
            head: best.0,
            newest: best.1,
            progress_ns: if best.1 > scan.newest { now } else { scan.progress_ns },
        });
        Ok(())
    }

    /// Adopts the HAL300 graph the firmware was started with, instead of building the HAL200
    /// graph. Every address the runtime derives from [`abi::HAL300`] is first checked against
    /// what the graph itself names, so a layout mistake fails here, before any publication.
    pub(super) fn adopt_hal300(
        &mut self,
        objects: KVec<Hal300Object>,
        roots: [u64; 2],
        metrics_fw_va: u64,
    ) -> Result {
        if !self.objects.is_empty() || self.hal300.is_some() {
            return Err(EBUSY);
        }
        let layout = &abi::HAL300;
        let mut completions = [(usize::MAX, 0u64); 4];
        let (mut bundle, mut cluster, mut region_c, mut qos, mut page_pool) =
            (None, None, None, None, None);
        let (mut pb, mut stamps) = (None, [None, None]);
        for item in objects {
            let index = self.objects.len();
            let name = item.name.as_slice();
            let size = item.object.size();
            match name {
                b"allocation0" if size == abi::bundle::SIZE => bundle = Some(index),
                b"allocation1" if size == abi::private::SIZE => cluster = Some(index),
                b"allocation2" if size >= layout.power_assert_tally + 4 => region_c = Some(index),
                b"compute-global" if size >= abi::QOS_SIZE => qos = Some(index),
                b"compute-pool-descriptors" if size == mmu::UAT_PGSZ => page_pool = Some(index),
                // Driver-created render tables: the adopted compute graph has neither the
                // parameter-buffer descriptor table nor the two stamp pages.
                b"pb-descriptors" if size == mmu::UAT_PGSZ => pb = Some(index),
                b"render-stamp-a" if size == mmu::UAT_PGSZ => stamps[0] = Some(index),
                b"render-stamp-b" if size == mmu::UAT_PGSZ => stamps[1] = Some(index),
                [b'c', b'o', b'm', b'p', b'l', b'e', b't', b'i', b'o', b'n', ring]
                    if (b'0'..=b'3').contains(ring) && size == abi::COMPLETION_RING_SIZE =>
                {
                    let slot = usize::from(ring - b'0');
                    if completions[slot].0 != usize::MAX || item.low[0] == 0 || item.low[1] != 0 {
                        return Err(EINVAL);
                    }
                    completions[slot] = (index, item.low[0]);
                }
                _ => {}
            }
            self.objects.push(item.object, GFP_KERNEL)?;
            for alias in item.aliases {
                self.aliases.push(alias, GFP_KERNEL)?;
            }
        }
        let (Some(bundle), Some(cluster), Some(region_c), Some(qos), Some(page_pool)) =
            (bundle, cluster, region_c, qos, page_pool)
        else {
            pr_err!("asahi: HAL300 graph lacks a shared object\n");
            return Err(EINVAL);
        };
        if completions.iter().any(|(index, _)| *index == usize::MAX) {
            pr_err!("asahi: HAL300 graph lacks a completion ring\n");
            return Err(EINVAL);
        }
        self.bundle = bundle;
        self.cluster = cluster;
        self.region_c = region_c;
        self.qos = qos;
        self.page_pool = page_pool;
        self.convert_g17g_pool_rows()?;
        if crate::g17::startup_seed_enabled() {
            // Startup idle permission. Native initFirmwareData copies the accelerator's flag
            // bit here (H+0x8b99454); the captured graph has 0, so the firmware never descends
            // and its performance controller never takes the wake catch-up path that seeds the
            // reason-4 entries (sol-hal300-seed-study.md).
            // HARDWARE RESULT (asahi-pw3/pw4-boot): with this set the power-on transition ends
            // in the DESCENDED status A ([0,1,0,0,0,1,0,0] for both) instead of the settled
            // one, and firmware start times out - even when one IdlePowerOff{allow=0} is
            // published immediately after POWER_ON_DONE. So it needs a start sequence that
            // can wake a descended firmware, not just this word; kept on its own flag (bit 55)
            // so the finding is reproducible without affecting the idle policy (bit 56).
            self.object(self.region_c)?
                .word(0x30)?
                .store(1, Ordering::Relaxed);
            fence(Ordering::SeqCst);
        }
        // Render needs all three; compute does not touch them, so a graph without them still
        // starts and only a render command fails its own validation.
        if let (Some(pb), [Some(a), Some(b)]) = (pb, stamps) {
            self.pb = pb;
            self.stamps = [a, b];
        }
        self.metrics_fw_va = metrics_fw_va;
        self.layout = layout;
        let bundle_va = self.object(self.bundle)?.gpu_va();
        let cluster_va = self.object(self.cluster)?.gpu_va();
        if bundle_va == 0 || cluster_va == 0 {
            pr_err!("asahi: HAL300 graph objects are not mapped\n");
            return Err(EINVAL);
        }
        let bundle = self.object(self.bundle)?;
        // Some graph words are only 4-byte aligned (the completion table at 0x2734).
        let read = |offset: usize| -> Result<u64> {
            let [low, high] = bundle.read_words::<2>(offset)?;
            Ok(u64::from(low) | u64::from(high) << 32)
        };
        let mismatch = |what: &str, offset: usize, found: u64, expected: u64| {
            pr_err!(
                "asahi: HAL300 graph layout mismatch: {} at {:#x}: {:#x}, expected {:#x}\n",
                what,
                offset,
                found,
                expected
            );
            EINVAL
        };
        // Each root names this layout's main configuration and status A block.
        for (role, root) in roots.iter().enumerate() {
            let object = self
                .objects
                .iter()
                .find(|object| {
                    let va = object.gpu_va();
                    va != 0 && *root >= va && *root + 0xc8 <= va + object.size() as u64
                })
                .ok_or_else(|| {
                    pr_err!("asahi: HAL300 root {:#x} is not in a graph object\n", root);
                    EINVAL
                })?;
            let at = (*root - object.gpu_va()) as usize;
            let word = |offset: usize| -> Result<u64> {
                let [low, high] = object.read_words::<2>(at + offset)?;
                Ok(u64::from(low) | u64::from(high) << 32)
            };
            for (offset, expected) in [
                (0x18, bundle_va + layout.main_config[role] as u64),
                (0xa8, cluster_va + layout.status_a[role] as u64),
            ] {
                let found = word(offset)?;
                if found != expected {
                    return Err(mismatch("root", offset, found, expected));
                }
            }
        }
        // The channel tables name the work rings, the control rings and the event rings exactly
        // where the runtime will publish and consume them.
        let channels = core::mem::offset_of!(abi::MainConfig, channels);
        let entry = |role: usize, channel: usize, word: usize| {
            read(layout.main_config[role] + channels + channel * 0x20 + word * 8)
        };
        let check = |role: usize, channel: usize, word: usize, expected: u64| -> Result {
            let found = entry(role, channel, word)?;
            if found != expected {
                let offset = layout.main_config[role] + channels + channel * 0x20 + word * 8;
                return Err(mismatch("channel", offset, found, expected));
            }
            Ok(())
        };
        for channel in 0..abi::WORK_CHANNELS {
            let state = cluster_va + (layout.state[0] + abi::work_state_offset(channel)) as u64;
            check(0, channel, 0, state + abi::ChannelState::CONSUMER as u64)?;
            check(0, channel, 2, state + abi::ChannelState::PRODUCER as u64)?;
            check(0, channel, 3, bundle_va + layout.work_ring_offset(channel) as u64)?;
        }
        for role in 0..2 {
            let state = cluster_va + (layout.state[role] + abi::CONTROL_STATE_OFFSET) as u64;
            let status = cluster_va + layout.status_a[role] as u64;
            let ring = bundle_va + (layout.main_config[role] + abi::MainConfig::CONTROL_RING) as u64;
            check(role, abi::CONTROL_CHANNEL, 2, state + abi::ChannelState::PRODUCER as u64)?;
            check(role, abi::CONTROL_CHANNEL, 3, ring)?;
            check(role, abi::EVENT_CHANNEL, 0, status + abi::status::EVENT_STATE)?;
            check(role, abi::EVENT_CHANNEL, 1, status + abi::status::EVENT_RING)?;
        }
        // The hardware data's completion table names the four rings and their context-0 aliases.
        for (ring, (index, low)) in completions.iter().enumerate() {
            let high = self.object(*index)?.gpu_va();
            let offset = HAL300_COMPLETION_TABLE + ring * 0x20;
            for (at, expected) in [(offset, *low), (offset + 8, high)] {
                let found = read(at)?;
                if found != expected {
                    return Err(mismatch("completion table", at, found, expected));
                }
            }
        }
        self.hal300 = Some(Hal300Graph {
            completions,
            roots,
            client_id: u64::from(super::ids::OBJECTS.object()?),
            effort: core::cell::Cell::new(0),
        });
        Ok(())
    }

    /// Whether device-control records with `opcode` are qualified for this firmware. On HAL300
    /// the USC cache control and the three compute teardown records are: their kinds and
    /// layouts match the native M5 publishers (submitReleaseResource 0x13, the free-list
    /// releases 0x2e, processExitTimeoutHandler 0x29). Idle power and parameter-buffer replies
    /// are not used on HAL300.
    pub(super) fn control_qualified(&self, opcode: super::fw::channels::DeviceControlOp) -> bool {
        use super::fw::channels::DeviceControlOp as Op;
        // SchedulerStateRelease (0x13) crashed the firmware in asahi-render62/63/64 (slot
        // bookkeeping at P+0x120e0: a descriptor word whose bits 60:61 were zero indexes its
        // 16-byte table at -1). Those boots published it for contexts whose render pool had
        // never been released, because HAL300 suppressed every free-list release. The order is
        // native again (the pool releases on its final reference, then the scheduler), so the
        // record is admitted behind debug_flags bit 60 until a boot proves that order.
        self.hal300.is_none()
            || matches!(
                opcode,
                Op::UscCacheInvalidate
                    | Op::FreeListRelease
                    | Op::ContextKill
                    | Op::TvbGrowReply
            )
            // IdlePowerOff (0x0a) is the native permission/wake control; its 64 bytes
            // ({kind, allow, zero[56]}) and power-counted policy already match the native
            // publisher. HAL300 keeps it opt-in (debug_flags bit 56) until a boot proves one
            // complete descent and wake: sol-hal300-seed-study.md shows that wake catch-up is
            // the legitimate route to the reason-4 seed that today leaves this M5 at pstate 1.
            || (matches!(opcode, Op::IdlePowerOff)
                && (super::idle_policy_enabled() || super::startup_seed_enabled()))
            || (matches!(opcode, Op::SchedulerStateRelease)
                && crate::module_parameters::debug_flags.value() & (1 << 60) != 0)
    }

    /// Retains a mapping the firmware uses for the whole session, such as a register window.
    pub(super) fn retain_mapping(&mut self, mapping: mmu::KernelMapping) -> Result {
        self.aliases.push(mapping, GFP_KERNEL)?;
        Ok(())
    }

    fn object(&self, index: usize) -> Result<&KernelObject> {
        self.objects.get(index).ok_or(EINVAL)
    }

    fn object_mut(&mut self, index: usize) -> Result<&mut KernelObject> {
        self.objects.get_mut(index).ok_or(EINVAL)
    }

    fn add(&mut self, object: KernelObject) -> Result<usize> {
        let index = self.objects.len();
        self.objects.push(object, GFP_KERNEL)?;
        Ok(index)
    }

    fn alias(&mut self, index: usize, vm: &mmu::Vm, va: u64, prot: mmu::Prot) -> Result {
        let alias = self.object_mut(index)?.map_alias(vm, va, prot)?;
        self.aliases.push(alias, GFP_KERNEL)?;
        Ok(())
    }

    /// Constructs the graph after the firmware has published the high-root entries.
    pub(super) fn build(
        &mut self,
        dev: &AsahiDrmDevice,
        cfg: &Config,
        uat: &mmu::Uat,
        render: &mmu::Vm,
        metrics: &KernelObject,
    ) -> Result {
        let range = uat.geometry().kernel_range();
        let env = BuildEnv {
            dev,
            vm: uat.kernel_vm(),
            lower: uat.kernel_lower_vm(),
            render,
            base: range.start,
            dynamic: range.start + t8140::dynamic::KERNEL_OFFSET..range.end,
        };
        self.map_register_windows(&env)?;
        let tables = self.build_descriptor_tables(&env)?;
        self.build_render_free_lists(&env)?;
        let first_block = self.build_render_pool(&env)?;
        self.build_compute_pools(&env, first_block)?;
        let bundle = self.build_bundle(&env, cfg, metrics)?;
        self.initialize_firmware_objects(&env, cfg, &tables, &bundle)
    }

    fn map_register_windows(&mut self, env: &BuildEnv<'_>) -> Result {
        for window in &t8140::REGISTER_WINDOWS {
            let (phys, va, size) = window.mapping().ok_or(EINVAL)?;
            self.aliases.push(
                env.vm.map_io(
                    env.dev.as_ref().as_ref(),
                    va,
                    phys.try_into()?,
                    size.try_into()?,
                    mmu::PROT_FW_MMIO_RW,
                )?,
                GFP_KERNEL,
            )?;
        }
        Ok(())
    }

    /// The private cluster, zero page and the three descriptor tables with their low aliases.
    fn build_descriptor_tables(&mut self, env: &BuildEnv<'_>) -> Result<DescriptorTables> {
        use mmu::{
            PROT_FW_PRIV_RW as FP, PROT_FW_SHARED_RW as FS, PROT_GPU_FW_SHARED_RW as GFS,
            PROT_GPU_SHARED_RW as GS,
        };
        use CpuMap::{WriteBack as WB, WriteCombined as WC};
        self.cluster = self.add(env.fixed(high::PRIVATE_CLUSTER, abi::private::SIZE, FS, WC)?)?;
        self.compute_global = self.add(env.fixed(high::ZERO_PAGE, mmu::UAT_PGSZ, FP, WC)?)?;
        let pb = self.add(env.fixed(high::PB_DESCRIPTORS, mmu::UAT_PGSZ, FS, WC)?)?;
        self.object_mut(pb)?
            .initialize::<abi::PbDescriptorTable>(0, |table| {
                table.init();
                Ok(())
            })?;
        let page_pool = self.add(env.fixed(high::PAGE_POOL_DESCRIPTORS, mmu::UAT_PGSZ, FS, WC)?)?;
        self.object_mut(page_pool)?
            .initialize::<abi::PagePoolDescriptorTable>(0, |table| {
                table.init(low::RENDER_PAGE_LIST, low::COMPUTE_POOL_PAGE_LIST);
                Ok(())
            })?;
        self.pb = pb;
        self.page_pool = page_pool;
        self.alias(pb, env.lower, low::PB_DESCRIPTORS, GS)?;
        self.alias(page_pool, env.lower, low::PAGE_POOL_DESCRIPTORS, GS)?;
        let index = self.add(env.fixed(high::INDEX, INDEX_SIZE, GFS, WB)?)?;
        self.object_mut(index)?
            .initialize::<abi::IndexTable>(0, |table| {
                table.init();
                Ok(())
            })?;
        self.alias(index, env.lower, low::INDEX, GFS)?;
        let fwctl = self.add(env.fixed(high::FWCTL, abi::FWCTL_SIZE, FS, WB)?)?;
        Ok(DescriptorTables {
            pb,
            page_pool,
            fwctl,
        })
    }

    /// Render and boot free-list controls, descriptor pages and the two queue contexts.
    fn build_render_free_lists(&mut self, env: &BuildEnv<'_>) -> Result {
        use mmu::{
            PROT_FW_PRIV_RW as FP, PROT_FW_SHARED_RW as FS, PROT_GPU_FW_PRIV_RW as GFP,
            PROT_GPU_FW_SHARED_RW as GFS,
        };
        use CpuMap::WriteCombined as WC;
        for (offset, state, class, prot, initial) in [
            (
                high::RENDER_FREE_LIST,
                high::RENDER_FREE_LIST_STATE,
                abi::FreeListClass::One,
                FP,
                abi::FreeListState::RENDER,
            ),
            (
                high::BOOT_FREE_LIST,
                high::BOOT_FREE_LIST_STATE,
                abi::FreeListClass::Two,
                GFP,
                abi::FreeListState::BOOT,
            ),
        ] {
            let control = self.add(env.fixed(offset, mmu::UAT_PGSZ, prot, WC)?)?;
            self.object_mut(control)?.write(
                0,
                abi::FreeListControl::new(&abi::FreeListArgs {
                    buffer_slot: 0,
                    class,
                    page_list_va: low::RENDER_PAGE_LIST,
                    run_list_va: low::RENDER_RUN_LIST,
                    blocks: low::RENDER_BLOCKS_INITIAL,
                    state_va: env.base + state,
                }),
            )?;
            let state_prot = if class == abi::FreeListClass::One {
                FS
            } else {
                GFS
            };
            let state = self.add(env.fixed(state, mmu::UAT_PGSZ, state_prot, WC)?)?;
            self.object_mut(state)?.write(0, initial)?;
            if class == abi::FreeListClass::Two {
                self.boot_control = control;
                self.boot_state = state;
            }
        }
        self.stamps[0] = self.add(env.fixed(high::DESCRIPTOR_PAGE_A, mmu::UAT_PGSZ, GFS, WC)?)?;
        self.stamps[1] = self.add(env.fixed(high::DESCRIPTOR_PAGE_B, mmu::UAT_PGSZ, GFP, WC)?)?;
        self.add(env.fixed(high::TA_CONTEXT, CONTEXT_SIZE, FS, WC)?)?;
        self.add(env.fixed(high::FRAGMENT_CONTEXT, CONTEXT_SIZE, FS, WC)?)?;
        Ok(())
    }

    /// Render page list, run list and blocks aliased into the render-global root; returns
    /// the index of the first block.
    fn build_render_pool(&mut self, env: &BuildEnv<'_>) -> Result<usize> {
        use mmu::PROT_GPU_SHARED_RW as GS;
        use CpuMap::WriteBack as WB;
        let pages = self.add(KernelObject::backing(env.dev, low::RENDER_PAGE_LIST_SIZE, WB)?)?;
        self.object_mut(pages)?
            .initialize::<[u64; low::RENDER_PAGE_LIST_SIZE / 8]>(0, |list| {
                abi::fill_page_list(
                    list,
                    (0..low::RENDER_BLOCKS_INITIAL as usize).map(low::render_block),
                )
            })?;
        let runs = self.add(KernelObject::backing(env.dev, low::RENDER_RUN_LIST_SIZE, WB)?)?;
        let first_block = self.objects.len();
        for _ in 0..low::RENDER_BLOCKS_ALLOCATED {
            self.add(KernelObject::backing(env.dev, abi::FREE_LIST_BLOCK_SIZE, WB)?)?;
        }
        self.render_pages = pages;
        self.render_runs = runs;
        self.render_blocks = first_block..self.objects.len();
        self.alias(pages, env.render, low::RENDER_PAGE_LIST, GS)?;
        self.alias(runs, env.render, low::RENDER_RUN_LIST, GS)?;
        for block in 0..low::RENDER_BLOCKS_MAPPED {
            self.alias(first_block + block, env.render, low::render_block(block), GS)?;
        }
        Ok(first_block)
    }

    /// Compute free-list controls and states, zero blocks, run list and the compute page
    /// list inside the render blocks.
    fn build_compute_pools(&mut self, env: &BuildEnv<'_>, first_block: usize) -> Result {
        use mmu::PROT_GPU_FW_SHARED_RW as GFS;
        use CpuMap::{WriteBack as WB, WriteCombined as WC};
        let compute = self.add(env.fixed(high::COMPUTE_FREE_LIST, mmu::UAT_PGSZ, GFS, WC)?)?;
        self.object_mut(compute)?.write(
            0,
            abi::FreeListControl::new(&abi::FreeListArgs {
                buffer_slot: 0,
                class: abi::FreeListClass::One,
                page_list_va: low::COMPUTE_PAGE_LIST,
                run_list_va: low::COMPUTE_RUN_LIST,
                blocks: low::COMPUTE_BLOCKS,
                state_va: env.base + high::COMPUTE_FREE_LIST_STATE,
            }),
        )?;
        let compute_state = self.add(KernelObject::backing(env.dev, mmu::UAT_PGSZ, WB)?)?;
        self.object_mut(compute_state)?
            .write(0, abi::FreeListState::COMPUTE)?;
        let compute_runs = self.add(KernelObject::new(
            env.dev,
            env.lower,
            Placement::At(low::COMPUTE_RUN_LIST),
            mmu::UAT_PGSZ,
            GFS,
            WC,
        )?)?;
        self.object_mut(compute_runs)?
            .initialize::<[abi::FreeListRunSlot; mmu::UAT_PGSZ / 0x40]>(0, |list| {
                abi::fill_run_list(list, low::COMPUTE_FREE_LIST_BLOCKS)
            })?;
        let ready = self.add(env.fixed(
            high::COMPUTE_READY_FREE_LIST,
            mmu::UAT_PGSZ,
            GFS,
            WC,
        )?)?;
        self.object_mut(ready)?.write(
            0,
            abi::FreeListControl::new(&abi::FreeListArgs {
                buffer_slot: 1,
                class: abi::FreeListClass::One,
                page_list_va: low::COMPUTE_POOL_PAGE_LIST,
                run_list_va: low::COMPUTE_POOL_RUN_LIST,
                blocks: low::COMPUTE_READY_BLOCKS,
                state_va: env.base + high::COMPUTE_READY_FREE_LIST_STATE,
            }),
        )?;
        for state in [high::COMPUTE_READY_FREE_LIST_STATE, high::COMPUTE_STATE] {
            let obj = self.add(env.fixed(state, mmu::UAT_PGSZ, GFS, WC)?)?;
            self.object_mut(obj)?
                .write(0, abi::FreeListState::COMPUTE)?;
        }
        self.object_mut(first_block + low::COMPUTE_PAGE_LIST_BLOCK)?
            .initialize::<[u64; abi::FREE_LIST_BLOCK_SIZE / 8]>(0, |list| {
                abi::fill_page_list(list, low::COMPUTE_FREE_LIST_BLOCKS)
            })?;
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// The bundle with its zero page and completion rings, the QoS table, the metrics alias,
    /// the roots and regions A and C.
    fn build_bundle(
        &mut self,
        env: &BuildEnv<'_>,
        cfg: &Config,
        metrics: &KernelObject,
    ) -> Result<BundleObjects> {
        use mmu::{
            PROT_FW_PRIV_RW as FP, PROT_FW_SHARED_RW as FS, PROT_GPU_FW_SHARED_RW as GFS,
            PROT_GPU_SHARED_RW as GS,
        };
        use CpuMap::{WriteBack as WB, WriteCombined as WC};
        self.bundle = self.add(env.fixed(high::BUNDLE, abi::bundle::SIZE, FP, WC)?)?;
        let zero = self.add(env.fixed(
            high::BUNDLE + abi::bundle::ZERO_PAGE as u64,
            mmu::UAT_PGSZ,
            GFS,
            WC,
        )?)?;
        let mut completion_rings = [abi::CompletionRing {
            low_va: 0,
            high_va: 0,
        }; 2];
        for (slot, completion_ring) in completion_rings.iter_mut().enumerate() {
            let offset = high::BUNDLE + abi::bundle::COMPLETION_RINGS[slot] as u64;
            let ring = self.add(env.fixed(offset, abi::COMPLETION_RING_SIZE, FP, WB)?)?;
            self.completion_rings[slot] = ring;
            self.alias(ring, env.lower, low::COMPLETION_RINGS[slot], GS)?;
            *completion_ring = abi::CompletionRing {
                low_va: low::COMPLETION_RINGS[slot],
                high_va: env.base + offset,
            };
        }
        let qos = self.add(env.dynamic(abi::QOS_SIZE, mmu::UAT_PGSZ as u64, FS, WC)?)?;
        self.qos = qos;
        fence(Ordering::SeqCst);
        let metrics_alias =
            metrics.alias_in(env.vm, env.dynamic.clone(), cfg.pm_metrics.size as u64, GFS)?;
        self.metrics_fw_va = metrics_alias.iova();
        self.aliases.push(metrics_alias, GFP_KERNEL)?;
        fence(Ordering::SeqCst);
        self.roots = self.add(env.dynamic(
            abi::ROOTS_SIZE,
            abi::SECONDARY_ROOT_OFFSET as u64,
            mmu::PROT_FW_SHARED_RO,
            WB,
        )?)?;
        let region_a = self.add(env.dynamic(
            REGION_A_SIZE,
            mmu::UAT_PGSZ as u64,
            mmu::PROT_FW_SHARED_RO,
            WB,
        )?)?;
        self.region_c = self.add(env.dynamic(REGION_C_SIZE, mmu::UAT_PGSZ as u64, FS, WB)?)?;
        Ok(BundleObjects {
            zero,
            completion_rings,
            region_a,
            qos,
        })
    }

    /// Writes every firmware object the host builds from constants and addresses, then the
    /// opening control-ring state for both roles.
    fn initialize_firmware_objects(
        &mut self,
        env: &BuildEnv<'_>,
        cfg: &Config,
        tables: &DescriptorTables,
        bundle: &BundleObjects,
    ) -> Result {
        let base = env.base;
        let opps = Self::opps(env.dev)?;
        let hw_args = abi::HwDataArgs {
            chip_id: cfg.chip_id,
            windows: &t8140::REGISTER_WINDOWS,
            reserved_slots: &t8140::RESERVED_REGISTER_SLOTS,
            opps: &opps,
            perf: &t8140::PERF,
            completion_rings: bundle.completion_rings,
            qos_va: self.object(bundle.qos)?.gpu_va(),
        };
        self.object_mut(self.bundle)?
            .initialize::<abi::HwData>(abi::bundle::HW_DATA, |hw| hw.init(&hw_args))?;
        self.object_mut(self.region_c)?
            .initialize::<abi::RegionC>(0, |region| {
                region.init();
                Ok(())
            })?;
        self.object_mut(self.cluster)?
            .write(abi::private::PRIMARY_STATUS_A, abi::StatusBlock::new())?;
        self.object_mut(self.cluster)?
            .write(abi::private::SECONDARY_STATUS_A, abi::StatusBlock::new())?;
        let fwctl_va = self.object(tables.fwctl)?.gpu_va();
        self.object_mut(self.cluster)?
            .initialize::<abi::PrimaryStatusB>(abi::private::STATUS_B, |status| {
                status.init(fwctl_va);
                Ok(())
            })?;
        self.object_mut(self.cluster)?
            .initialize::<abi::SecondaryConfig>(
                abi::private::SECONDARY_STATE + abi::private::STATUS_B,
                |config| {
                    config.init();
                    Ok(())
                },
            )?;
        self.object_mut(self.bundle)?
            .initialize::<abi::PowerConfig>(abi::bundle::POWER_CONFIG, |power| {
                power.init();
                Ok(())
            })?;
        let secondary_status = base
            + high::PRIVATE_CLUSTER
            + (abi::private::SECONDARY_STATE + abi::private::SECONDARY_STATUS) as u64;
        self.object_mut(self.bundle)?
            .initialize::<abi::HwDataAux>(abi::bundle::AUX, |aux| {
                aux.init(secondary_status);
                Ok(())
            })?;
        self.object_mut(self.bundle)?
            .initialize::<abi::PowerConfigHead>(abi::bundle::POWER_CONFIG_COPY, |power| {
                power.init();
                Ok(())
            })?;
        let views = abi::PrimaryViews {
            zero_page_va: self.object(bundle.zero)?.gpu_va(),
            pb_descriptors_va: self.object(tables.pb)?.gpu_va(),
            page_pool_descriptors_va: self.object(tables.page_pool)?.gpu_va(),
        };
        for role in [Role::Primary, Role::Secondary] {
            let slot = role as usize;
            let main_offset = abi::bundle::MAIN_CONFIG[slot];
            let main_va = base + high::BUNDLE + main_offset as u64;
            let state_va = base + high::PRIVATE_CLUSTER + self.state_offset(role) as u64;
            let status_va = base + high::PRIVATE_CLUSTER + self.status_offset(role) as u64;
            let args = abi::MainConfigArgs {
                bundle_va: base + high::BUNDLE,
                main_config_va: main_va,
                state_grid_va: state_va,
                status_a_va: status_va,
                pb_descriptors_low_va: low::PB_DESCRIPTORS,
                page_pool_descriptors_low_va: low::PAGE_POOL_DESCRIPTORS,
            };
            self.object_mut(self.bundle)?
                .initialize::<abi::MainConfig>(main_offset, |main| {
                    match role {
                        Role::Primary => main.init_primary(&args, &views),
                        Role::Secondary => main.init_secondary(&args),
                    };
                    Ok(())
                })?;
            let abi_role = match role {
                Role::Primary => abi::Role::Primary,
                Role::Secondary => abi::Role::Secondary,
            };
            self.object_mut(self.bundle)?.write(
                main_offset + abi::MainConfig::CONTROL_RING,
                abi::OpeningRecord::new(abi_role),
            )?;
            let root = abi::RootArgs {
                region_a_va: self.object(bundle.region_a)?.gpu_va(),
                region_c_va: self.object(self.region_c)?.gpu_va(),
                main_config_va: main_va,
                status_a_va: status_va,
                ias: cfg.uat_ias,
                oas: cfg.uat_oas,
            };
            match role {
                Role::Primary => self.object_mut(self.roots)?.write(
                    0,
                    abi::Root::primary(&root, state_va + abi::private::STATUS_B as u64),
                )?,
                Role::Secondary => self.object_mut(self.roots)?.write(
                    abi::SECONDARY_ROOT_OFFSET,
                    abi::SecondaryRoot::new(
                        &root,
                        base + high::PRIVATE_CLUSTER
                            + (abi::private::STATUS_B + abi::PrimaryStatusB::POWER_STATE) as u64,
                        state_va + abi::private::STATUS_B as u64,
                    ),
                )?,
            }
        }
        for role in [Role::Primary, Role::Secondary] {
            for (offset, value) in [
                (0, 0),
                (0x10, 0),
                (abi::ChannelState::PRODUCER, abi::OPENING_PRODUCER),
            ] {
                self.object(self.cluster)?
                    .word(self.state_offset(role) + abi::CONTROL_STATE_OFFSET + offset)?
                    .store(value, Ordering::Relaxed);
            }
        }
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// Retained fixed aliases for an ordinary client VM. The final block of the render-global
    /// pool overlaps completion mappings in a client root and is deliberately not installed.
    pub(super) fn user_aliases(
        &mut self,
        vm: &mmu::Vm,
        metrics: &KernelObject,
    ) -> Result<KVec<mmu::KernelMapping>> {
        let mut mappings = KVec::with_capacity(24, GFP_KERNEL)?;
        if self.hal300.is_some() {
            // The adopted graph has no render pool, so only the PM metrics page is aliased. TA
            // writes scene metrics through a compact pointer relative to the architectural base
            // 0x1000000000 (register 0x1c910, scene metric alias 0x1000080004 + 4 * slot): in
            // render13 the TA faulted writing context 5 VA 0x1000080000 (fault info
            // 0x81400bc091) because this alias was missing. Mesa's M5 driver range is the
            // client VM's kernel range ([0x1000000000, 0x1800000000)), so the alias belongs
            // there; compute never touches it.
            mappings.push(
                metrics.map_alias(vm, t8140::CONFIG.pm_metrics.va, mmu::PROT_GPU_FW_SHARED_RW)?,
                GFP_KERNEL,
            )?;
            return Ok(mappings);
        }
        mappings.push(
            self.object_mut(self.render_pages)?.map_alias(
                vm,
                low::RENDER_PAGE_LIST,
                mmu::PROT_GPU_SHARED_RW,
            )?,
            GFP_KERNEL,
        )?;
        mappings.push(
            self.object_mut(self.render_runs)?.map_alias(
                vm,
                low::RENDER_RUN_LIST,
                mmu::PROT_GPU_SHARED_RW,
            )?,
            GFP_KERNEL,
        )?;
        for (block, index) in self.render_blocks.clone().take(21).enumerate() {
            mappings.push(
                self.object_mut(index)?.map_alias(
                    vm,
                    low::render_block(block),
                    mmu::PROT_GPU_SHARED_RW,
                )?,
                GFP_KERNEL,
            )?;
        }
        mappings.push(
            metrics.map_alias(vm, t8140::CONFIG.pm_metrics.va, mmu::PROT_GPU_FW_SHARED_RW)?,
            GFP_KERNEL,
        )?;
        Ok(mappings)
    }

    /// Writes the effort input in firmware order, then cleans its cache line to coherency.
    pub(super) fn publish_effort(&self, effort: u32) -> Result {
        if let Some(graph) = self.hal300.as_ref() {
            graph.effort.set(effort);
        }
        let region = self.object(self.region_c)?;
        let effort_offset = self.layout.effort.ok_or(EIO)?;
        let override_word = region.word(effort_offset + 4)?;
        let effort_word = region.word(effort_offset)?;
        let line = region.pointer(effort_offset, 4)?;
        override_word.store(0, Ordering::Relaxed);
        effort_word.store(effort, Ordering::Relaxed);
        // SAFETY: line points into the retained kernel mapping; cleaning this host-owned cache
        // line and the following system barrier publish the two naturally aligned stores.
        unsafe {
            core::arch::asm!("dc cvac, {line}", "dsb sy", line = in(reg) line,
            options(nostack, preserves_flags))
        };
        Ok(())
    }

    /// Read-only HAL300 power observation (sol-hal300-power-study.md,
    /// sol-hal300-pstate-cap-study.md): the host's inputs in region C, the counted-control
    /// tally, and the firmware's own arbitration in object Q (bundle +0x5380) - its applied
    /// pstate and the floor/cap/selected values with the reason codes that attribute them.
    /// Nothing here writes firmware memory.
    pub(super) fn power_sample(&self) -> Result<([u64; 13], [u64; 9])> {
        const Q: usize = 0x5380;
        let region = self.object(self.region_c)?;
        let effort_offset = self.layout.effort.ok_or(EIO)?;
        let tally = region.pointer(self.layout.power_assert_tally, 4)?;
        // SAFETY: the checked pointer stays inside the retained region-C mapping; the tally is
        // unaligned, so it is read with the same four-byte copy the publisher uses.
        let tally = unsafe {
            let mut bytes = [0u8; 4];
            core::ptr::copy_nonoverlapping(tally, bytes.as_mut_ptr(), 4);
            u64::from(u32::from_le_bytes(bytes))
        };
        let c = |offset: usize| {
            region
                .word(offset)
                .map(|w| u64::from(w.load(Ordering::Relaxed)))
        };
        let bundle = self.object(self.bundle)?;
        let q = |offset: usize| {
            bundle
                .word(Q + offset)
                .map(|w| u64::from(w.load(Ordering::Relaxed)))
        };
        Ok((
            [
                c(effort_offset)?,
                c(effort_offset + 4)?,
                tally,
                c(0x88)?,
                c(0x8c)?,
                c(0x90)?,
                c(0x94)?,
                c(0x98)?,
                c(0x9c)?,
                c(0xa0)?,
                c(0xa4)?,
                c(0x7f0)?,
                c(0x7f4)?,
            ],
            [
                q(0x38)?,
                q(0x3c)?,
                q(0x40)?,
                q(0x44)?,
                q(0x48)?,
                q(0x4c)?,
                q(0x50)?,
                q(0x54)?,
                q(0x58)?,
            ],
        ))
    }

    /// Reason-4 seed witnesses (sol-hal300-seed-study.md): the aggregate output Q+0x2928 and
    /// the three enabled entries' previous candidates at Q+0x298a + 0x20*i + 0x18. The entry
    /// words are only 2-byte aligned, so they are copied, never loaded as atomics.
    pub(super) fn power_seed(&self) -> Result<[u32; 4]> {
        const Q: usize = 0x5380;
        let bundle = self.object(self.bundle)?;
        let copy = |offset: usize| -> Result<u32> {
            let pointer = bundle.pointer(Q + offset, 4)?;
            let mut bytes = [0u8; 4];
            // SAFETY: the checked pointer covers four bytes of the retained bundle mapping.
            unsafe { core::ptr::copy_nonoverlapping(pointer, bytes.as_mut_ptr(), 4) };
            Ok(u32::from_le_bytes(bytes))
        };
        Ok([copy(0x2928)?, copy(0x29a2)?, copy(0x29c2)?, copy(0x29e2)?])
    }

    /// Firmware VA of region C and the host's last published effort, for the power view.
    pub(super) fn power_identity(&self) -> Result<(u64, u32)> {
        Ok((
            self.object(self.region_c)?.gpu_va(),
            self.hal300.as_ref().map_or(0, |graph| graph.effort.get()),
        ))
    }

    pub(super) fn effort(&self) -> Result<u32> {
        Ok(self
            .object(self.region_c)?
            .word(self.layout.effort.ok_or(EIO)?)?
            .load(Ordering::Relaxed))
    }

    fn opps(dev: &AsahiDrmDevice) -> Result<KVec<abi::Opp>> {
        let node = dev.as_ref().as_ref().of_node().ok_or(EIO)?;
        let table = node
            .parse_phandle(c_str!("operating-points-v2"), 0)
            .ok_or(EIO)?;
        let mut opps = KVec::new();
        for opp in table.children() {
            opps.push(
                abi::Opp {
                    freq_hz: opp.get_property(c_str!("opp-hz"))?,
                    volt_uv: opp.get_property(c_str!("opp-microvolt"))?,
                },
                GFP_KERNEL,
            )?;
        }
        if opps.len() != t8140::OPPS {
            return Err(EINVAL);
        }
        Ok(opps)
    }

    fn state_offset(&self, role: Role) -> usize {
        self.layout.state[role as usize]
    }

    fn status_offset(&self, role: Role) -> usize {
        self.layout.status_a[role as usize]
    }

    /// Recheck the host-authored event pointer pair after a bounded number of drains.
    /// The device mutex serializes the cache and all event consumption.
    pub(super) fn validate_event_ring(&self, role: Role) -> Result {
        let status = self.object(self.cluster)?.gpu_va() + self.status_offset(role) as u64;
        let expected = (
            status + abi::status::EVENT_STATE,
            status + abi::status::EVENT_RING,
        );
        let cache = &self.event_ring_checked[role as usize];
        let checked = cache.get();
        if checked.2 != 0 && (checked.0, checked.1) == expected {
            cache.set((checked.0, checked.1, checked.2 - 1));
            return Ok(());
        }
        let offset = self.layout.main_config[role as usize]
            + core::mem::offset_of!(abi::MainConfig, channels)
            + abi::EVENT_CHANNEL * size_of::<abi::ChannelEntry>();
        let bundle = self.object(self.bundle)?;
        let actual = (
            bundle.dword(offset)?.load(Ordering::Relaxed),
            bundle
                .dword(offset + size_of::<u64>())?
                .load(Ordering::Relaxed),
        );
        if actual != expected {
            cache.set((0, 0, 0));
            return Err(EINVAL);
        }
        cache.set((actual.0, actual.1, EVENT_RING_RECHECK));
        Ok(())
    }

    pub(super) fn status_word(&self, role: Role, offset: usize) -> Result<&AtomicU32> {
        self.object(self.cluster)?.word(
            self.status_offset(role)
                .checked_add(offset)
                .ok_or(EINVAL)?,
        )
    }

    pub(super) fn status_read_words<const N: usize>(
        &self,
        role: Role,
        offset: usize,
    ) -> Result<[u32; N]> {
        self.object(self.cluster)?.read_words(
            self.status_offset(role)
                .checked_add(offset)
                .ok_or(EINVAL)?,
        )
    }

    pub(super) fn control_counters(&self) -> Result<[u32; 3]> {
        let state = self.object(self.cluster)?;
        let mut counters = [0; 3];
        fence(Ordering::Acquire);
        for (counter, offset) in counters.iter_mut().zip([
            abi::ChannelState::CONSUMER,
            abi::ChannelState::UNK_10,
            abi::ChannelState::PRODUCER,
        ]) {
            *counter = state
                .word(abi::CONTROL_STATE_OFFSET + offset)?
                .load(Ordering::Relaxed);
        }
        Ok(counters)
    }

    pub(super) fn control_consumer(&self) -> Result<&AtomicU32> {
        self.object(self.cluster)?
            .word(abi::CONTROL_STATE_OFFSET + abi::ChannelState::CONSUMER)
    }

    pub(super) fn control_producer(&self) -> Result<&AtomicU32> {
        self.object(self.cluster)?
            .word(abi::CONTROL_STATE_OFFSET + abi::ChannelState::PRODUCER)
    }

    /// Publishes a control record under the device lock. All checked views are acquired before
    /// the first write; record and packed power tally precede the aligned producer store.
    pub(super) fn publish_control(
        &self,
        slot: u32,
        record: &[u8; 64],
        asserts_power: bool,
        next: u32,
    ) -> Result {
        let offset = self.layout.main_config[Role::Primary as usize]
            + abi::MainConfig::CONTROL_RING
            + (slot as usize).checked_mul(record.len()).ok_or(EINVAL)?;
        let ring = self.object(self.bundle)?.pointer(offset, record.len())?;
        let power = self
            .object(self.region_c)?
            .pointer(self.layout.power_assert_tally, 4)?;
        let producer = self.control_producer()?;
        // SAFETY: The checked pointers remain valid for the lifetime of this non-Sync owner.
        // Firmware only reads records after the producer store and the device mutex serializes
        // host publishers. The unaligned power word uses the interface's four-byte copy access.
        unsafe {
            core::ptr::copy_nonoverlapping(record.as_ptr(), ring, record.len());
            if asserts_power {
                let mut tally = [0u8; 4];
                core::ptr::copy_nonoverlapping(power, tally.as_mut_ptr(), 4);
                let value = u32::from_le_bytes(tally).wrapping_add(1).to_le_bytes();
                core::ptr::copy_nonoverlapping(value.as_ptr(), power, 4);
            }
        }
        fence(Ordering::SeqCst);
        producer.store(next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn root_va(&self, role: Role) -> Result<u64> {
        if let Some(graph) = self.hal300.as_ref() {
            return Ok(graph.roots[role as usize]);
        }
        Ok(self.object(self.roots)?.gpu_va() + role as u64 * abi::SECONDARY_ROOT_OFFSET as u64)
    }

    /// Publishes one uncounted, otherwise zero 64-byte control record of `kind` on `role`'s own
    /// control ring (native AGXArmFirmware::submitCmdFirmwareForceConfigSnapshot builds primary
    /// kind 0x15 and secondary kind 0x2a this way; sol-hal300-gated-context-study.md 2.4).
    /// Returns the new producer. Startup only: the device lock is not yet shared.
    pub(super) fn publish_role_control(&self, role: Role, kind: u32) -> Result<u32> {
        let base = self.state_offset(role) + abi::CONTROL_STATE_OFFSET;
        let cluster = self.object(self.cluster)?;
        fence(Ordering::Acquire);
        let consumer = cluster.word(base + abi::ChannelState::CONSUMER)?.load(Ordering::Relaxed);
        let producer_word = cluster.word(base + abi::ChannelState::PRODUCER)?;
        let producer = producer_word.load(Ordering::Relaxed);
        if consumer >= super::fw::channels::DEVICE_CONTROL_SLOTS || producer >= super::fw::channels::DEVICE_CONTROL_SLOTS {
            return Err(EIO);
        }
        let next = (producer + 1) & (super::fw::channels::DEVICE_CONTROL_SLOTS - 1);
        if next == consumer {
            return Err(EAGAIN);
        }
        let mut record = [0u8; 64];
        record[..4].copy_from_slice(&kind.to_le_bytes());
        let offset = self.layout.main_config[role as usize]
            + abi::MainConfig::CONTROL_RING
            + (producer as usize) * record.len();
        let ring = self.object(self.bundle)?.pointer(offset, record.len())?;
        // SAFETY: checked pointer into the retained bundle; firmware reads it only after the
        // producer store below.
        unsafe { core::ptr::copy_nonoverlapping(record.as_ptr(), ring, record.len()) };
        fence(Ordering::SeqCst);
        producer_word.store(next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        pr_info!("asahi: {:?} control slot {} kind {:#x}\n", role, producer, kind);
        Ok(next)
    }

    /// Withdraws the startup idle permission (region C +0x30), so later configuration refreshes
    /// keep the GPU awake after the one startup descent and wake.
    pub(super) fn clear_startup_idle_permission(&self) -> Result {
        self.set_startup_idle_permission(0)
    }

    /// Host-owned region C +0x30 (startup idle permission), consumed by the coprocessors at their
    /// next configuration refresh (a force-config snapshot makes that immediate).
    pub(super) fn set_startup_idle_permission(&self, value: u32) -> Result {
        self.object(self.region_c)?.word(0x30)?.store(value, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// Fills one empty host IOMapping record of the adopted HAL300 graph (allocation0 +0x640,
    /// 0x28 bytes per slot: phys, firmware VA, total and element size, relative offset, rw)
    /// before the coprocessors initialize. A record the graph already carries is left alone.
    pub(super) fn set_io_mapping(
        &mut self,
        slot: usize,
        phys: u64,
        va: u64,
        total: u32,
        element: u32,
        rw: u32,
    ) -> Result {
        const IO_MAPPINGS: usize = 0x640;
        const IO_MAPPING_SIZE: usize = 0x28;
        const IO_MAPPING_SLOTS: usize = 60;
        if self.hal300.is_none() || slot >= IO_MAPPING_SLOTS {
            return Err(EINVAL);
        }
        let offset = IO_MAPPINGS + slot * IO_MAPPING_SIZE;
        let bundle = self.object_mut(self.bundle)?;
        let current = bundle.read_words::<4>(offset)?;
        if current.iter().any(|word| *word != 0) {
            return Err(EEXIST);
        }
        bundle.write(offset, phys)?;
        bundle.write(offset + 8, va)?;
        bundle.write(offset + 0x10, total)?;
        bundle.write(offset + 0x14, element)?;
        bundle.write(offset + 0x18, 0u64)?;
        bundle.write(offset + 0x20, rw)?;
        bundle.write(offset + 0x24, 0u32)?;
        fence(Ordering::SeqCst);
        Ok(())
    }

    pub(super) fn opening_retired(&self, role: Role) -> Result<bool> {
        self.control_retired(role, abi::OPENING_PRODUCER)
    }

    /// Whether this role's control ring has produced and retired exactly `expected` records.
    pub(super) fn control_retired(&self, role: Role, expected: u32) -> Result<bool> {
        for offset in [0, 0x10, abi::ChannelState::PRODUCER] {
            if self
                .object(self.cluster)?
                .word(self.state_offset(role) + abi::CONTROL_STATE_OFFSET + offset)?
                .load(Ordering::SeqCst)
                != expected
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// HAL300 native pre/post-recovery handshake in the primary status B
/// (`sol-render-hang-study.md`): the firmware publishes 1 at +0x4ae0 after incrementing the epoch
/// at +0x4ad0 and spins until the host writes 2 (`resumeFirmwarePreRecovery`,
/// 0xfffffe0008bb6ce0); later it publishes 3 and spins until 0. Its summary precedes it.
pub(super) const HAL300_RECOVERY_SUMMARY: usize = 0x4a90;
/// The recovery report is the primary main configuration's fourth view (packed pointer at
/// main +0x26c), bundle +0xc500 on HAL300.
const HAL300_REPORT_VIEW: usize = 3;

impl InitData {
    /// Firmware addresses of the HAL300 recovery summary in status B and of the recovery report
    /// view, after checking the report pointer the primary main configuration publishes.
    pub(super) fn hal300_recovery_vas(&self) -> Result<(u64, u64)> {
        let cluster = self.object(self.cluster)?;
        let bundle = self.object(self.bundle)?;
        let view = *self.layout.views.get(HAL300_REPORT_VIEW).ok_or(EINVAL)?;
        let pointer_at = self.layout.main_config[Role::Primary as usize]
            + core::mem::offset_of!(abi::MainConfig, view_va)
            + HAL300_REPORT_VIEW * size_of::<u64>();
        fence(Ordering::Acquire);
        let pointer = bundle.read_words::<2>(pointer_at)?;
        let actual = u64::from(pointer[0]) | u64::from(pointer[1]) << 32;
        if actual != bundle.gpu_va() + view as u64 {
            return Err(EINVAL);
        }
        Ok((
            cluster.gpu_va() + (abi::private::STATUS_B + HAL300_RECOVERY_SUMMARY) as u64,
            actual,
        ))
    }

    /// Validate the serialized view before reading its retained host mapping.
    fn recovery_view_words<const N: usize>(&self, view: usize, offset: usize) -> Result<[u32; N]> {
        let bundle = self.object(self.bundle)?;
        let base = *self.layout.views.get(view).ok_or(EINVAL)?;
        let extent = *abi::recovery::VIEW_EXTENTS.get(view).ok_or(EINVAL)?;
        let end = offset.checked_add(N.checked_mul(size_of::<u32>()).ok_or(EOVERFLOW)?)
            .ok_or(EOVERFLOW)?;
        if end > extent {
            return Err(EINVAL);
        }
        let pointer_at = self.layout.main_config[Role::Primary as usize]
            + core::mem::offset_of!(abi::MainConfig, view_va) + view * size_of::<u64>();
        fence(Ordering::Acquire);
        let pointer = bundle.read_words::<2>(pointer_at)?;
        let actual = u64::from(pointer[0]) | u64::from(pointer[1]) << 32;
        if actual != bundle.gpu_va().checked_add(base as u64).ok_or(EOVERFLOW)? {
            return Err(EINVAL);
        }
        bundle.read_words(base + offset)
    }
}

impl InitData {
    /// Whether this firmware's recovery handshake may be acknowledged. HAL300 acknowledgement is
    /// opt-in (debug_flags bit 61): writing the native 1 -> 2 made the firmware run its
    /// destructive cleanup and crash in its kick walker at P+0xfe80 (`ldr x8,[x1,#0x10]` of a
    /// work slot whose descriptor pointer is zero - this runtime publishes a config-only outer
    /// slot for a queue's first kick). Without it the firmware only stops, which the event
    /// service reports as a lost device (asahi-render62-boot).
    fn recovery_enabled(&self) -> bool {
        // HAL300 firmware-initiated recovery is on by default since asahi-pw8-boot (three
        // CDM-fault recoveries, desktop and new clients kept working); debug_flags bit 51
        // restores the old behaviour (device lost at the first fault). Bit 61 is accepted for
        // compatibility and no longer needed.
        let flags = crate::module_parameters::debug_flags.value();
        self.layout.recovery
            && (!self.layout.recovery_fail_all || flags & (1 << 61) != 0 || flags & (1 << 51) == 0)
    }
}

impl super::recovery::Memory for InitData {
    fn fault_sources(&self) -> Result<super::recovery::Sources> {
        if !self.recovery_enabled() {
            return Ok(super::recovery::Sources::sample(0, |_| None));
        }
        if self.layout.recovery_fail_all {
            // HAL300: per-slot attribution reads T8140 physical slot keys; only the class is used.
            let mut sources = super::recovery::Sources::sample(0, |_| None);
            sources.reason = self
                .recovery_view_words::<1>(abi::recovery::REPORT_VIEW, self.layout.recovery_reason)
                .ok()
                .map(|words| words[0]);
            return Ok(sources);
        }
        let mask = self.object(self.cluster)?
            .dword(abi::private::STATUS_B + self.layout.recovery_b[1])?
            .load(Ordering::Relaxed);
        let mut sources = super::recovery::Sources::sample(mask as u32, |slot| {
            let key = super::recovery::slot_key(slot)?;
            let monitor = self.recovery_view_words::<6>(abi::recovery::PROGRESS_VIEW,
                abi::recovery::PROGRESS_START + slot * abi::recovery::PROGRESS_STRIDE)
                .ok().map(|words| words[0])?;
            Some((key, monitor))
        });
        sources.reason = self.recovery_view_words::<1>(abi::recovery::REPORT_VIEW, self.layout.recovery_reason)
            .ok().map(|words| words[0]);
        Ok(sources)
    }

    fn recovery_state(&self) -> Result<u32> {
        // HAL300's recovery handshake words are not known; it never requests a restart here.
        if !self.recovery_enabled() {
            return Ok(0);
        }
        fence(Ordering::Acquire);
        Ok(self
            .object(self.cluster)?
            .word(abi::private::STATUS_B + self.layout.recovery_b[3])?
            .load(Ordering::Relaxed))
    }

    fn host_recovery(&self) -> Result<u32> {
        if !self.recovery_enabled() {
            return Ok(0);
        }
        Ok(self
            .object(self.cluster)?
            .word(abi::private::STATUS_B + self.layout.recovery_b[4])?
            .load(Ordering::Relaxed))
    }

    fn transition(&self, expected: u32, next: u32) -> Result {
        if !self.recovery_enabled() {
            return Err(ENODEV);
        }
        let state = self
            .object(self.cluster)?
            .word(abi::private::STATUS_B + self.layout.recovery_b[3])?;
        fence(Ordering::Acquire);
        if state.load(Ordering::Relaxed) != expected {
            return Err(EBUSY);
        }
        fence(Ordering::SeqCst);
        state.store(next, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        Ok(())
    }

    fn clear_timestamps(&self) -> Result {
        if !self.recovery_enabled() {
            return Err(ENODEV);
        }
        // HAL300 firmware-initiated recovery skips the host timestamp override (native worker
        // 10b3c..bc0); never clear view 4 under an active renderer there.
        if self.layout.recovery_fail_all {
            return Ok(());
        }
        let bundle = self.object(self.bundle)?;
        // Validate the complete table before publishing its first live word.
        const ENTRIES: usize = 128;
        const STRIDE: usize = 0x10;
        let offset = self.layout.views[4];
        bundle.pointer(offset, ENTRIES * STRIDE)?;
        for qid in 0..ENTRIES {
            bundle
                .word(offset + qid * STRIDE)?
                .store(0, Ordering::Relaxed);
            bundle
                .dword(offset + qid * STRIDE + 8)?
                .store(0, Ordering::Relaxed);
        }
        fence(Ordering::SeqCst);
        Ok(())
    }

    fn entries(&self) -> Result<super::recovery::Entries> {
        if !self.recovery_enabled() {
            return Ok(Default::default());
        }
        let cluster = self.object(self.cluster)?;
        let base = abi::private::STATUS_B + self.layout.recovery_b[0];
        fence(Ordering::Acquire);
        let mut entries = super::recovery::Entries::default();
        for index in 0..256 {
            let offset = base + index * core::mem::size_of::<abi::RecoveryInfoEntry>();
            let flags = cluster.word(offset)?.load(Ordering::Relaxed);
            entries.include(flags);
        }
        Ok(entries)
    }

    fn wait_tick(&self) {
        kernel::time::delay::fsleep(kernel::time::Delta::from_millis(1));
    }

    fn fail_all_policy(&self) -> bool {
        self.layout.recovery_fail_all && self.recovery_enabled()
    }
}
