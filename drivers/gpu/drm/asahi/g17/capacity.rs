// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Name capacity refusals at their source, before a common submit or fence
//! failure handler loses their provenance. Counters are device-lifetime
//! independent diagnostics, never admission or ownership state.

use core::sync::atomic::{AtomicU64, Ordering};
use kernel::sync::Arc;
use kernel::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Site {
    ComputeReserve,
    PhysicalQids,
    RenderSlots,
    LogicalQueues,
    ComputeSlots,
    QosIds,
    ComputeItems,
    RetiredRenderBindings,
    RenderPoolIds,
    RenderWorkSlots,
    ParameterBuffers,
    MetricsIds,
    LowerVa,
    UpperVa,
    AliasVa,
    ExecutionContexts,
}

const SITE_COUNT: usize = Site::ExecutionContexts as usize + 1;
static REFUSALS: [AtomicU64; SITE_COUNT] = [const { AtomicU64::new(0) }; SITE_COUNT];

/// First and power-of-two refusals per site keep a pressure storm bounded in
/// the log. This does not change whether the caller retries or rejects work.
#[track_caller]
pub(super) fn refused(site: Site) -> Error {
    let count = REFUSALS[site as usize].fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    if count.is_power_of_two() {
        let at = core::panic::Location::caller();
        pr_warn!("G17 capacity refusal {:?} count={} at {}:{}\n", site, count,
            at.file().rsplit("asahi/").next().unwrap_or(at.file()), at.line());
    }
    ENOSPC
}

/// Read-only diagnostics hold the live device session while sampling its pools.
/// They never wake firmware, reclaim a resource, or change admission state.
pub(super) struct View(pub(super) kernel::sync::Arc<super::Shared>);

/// Read-only view of what a render pair left behind: the cursors of both render work rings,
/// the per-QID stamps of the pair and the head of every completion ring. Nothing here writes
/// to firmware memory or changes a selector; it exists because a HAL300 render that does not
/// retire has to be attributed to a stage before anything is changed.
pub(super) struct RenderView(pub(super) Arc<super::Shared>);

/// Read-only HAL300 power view: what the host asked for and what the firmware's own
/// controller applied. Sampling only; the driver publishes no power control on HAL300.
pub(super) struct PowerView(pub(super) Arc<super::Shared>);

impl kernel::debugfs::Writer for PowerView {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        writeln!(f, "version=1 sampled_ns={}", super::now_ns())?;
        let state = self.0.state.lock();
        let Some(firmware) = state.as_deref() else {
            return writeln!(f, "online=0");
        };
        writeln!(f, "online=1 powered={}", firmware.bridge.regs.g17_configuration_powered())?;
        writeln!(f, "render_serialize={} queue_priorities={}",
                 u8::from(super::runtime::render_serialize()),
                 u8::from(super::queue_priorities()))?;
        match (firmware.init.power_sample(), firmware.init.power_identity()) {
            (Ok((c, q)), Ok((region, last))) => {
                writeln!(
                    f,
                    "effort=0x{:x} override={} control_tally={} last_published=0x{:x} \
region_c_va=0x{:x}",
                    c[0], c[1], c[2], last, region
                )?;
                writeln!(
                    f,
                    "applied_pstate={} floor={} floor_reason={} cap={} cap_reason={} \
selected={} selected_reason={} cap_aux={} unk58={}",
                    q[0], q[2], q[1], q[4], q[3], q[6], q[5], q[7], q[8]
                )?;
                writeln!(
                    f,
                    "c88={} c8c={} ceiling_c90={} c94={} c98={} max_c9c={} c_a0={} c_a4={} \
c7f0={} c7f4={}",
                    c[3], c[4], c[5], c[6], c[7], c[8], c[9], c[10], c[11], c[12]
                )
            }
            (Err(error), _) | (_, Err(error)) => writeln!(f, "sample_error={:?}", error),
        }?;
        if let Ok([aggregate, e0, e1, e2]) = firmware.init.power_seed() {
            writeln!(f, "reason4_aggregate={} entry_previous={}/{}/{}", aggregate, e0, e1, e2)?;
        }
        if let Ok([primary, secondary]) = super::hal300::status_a_pub(&firmware.init) {
            writeln!(f, "status_a={:x?} {:x?}", primary, secondary)?;
        }
        // Primary __DATA starts at P+0x50000: live idle permission P+0x107f10 (written by the
        // 0x0a handler and by config refresh from region C +0x30) and the cached idle delay
        // P+0x107f94 (region C +0x9c0, milliseconds).
        if let Some([start, size]) = FIRMWARE_DATA.first() {
            let (start, size) = (start.load(Ordering::Relaxed), size.load(Ordering::Acquire));
            if size >= 0xb8000 {
                if let Ok(copy) = super::hal300::copy_physical(start + 0xb7f00, 0x100) {
                    let word = |o: usize| u32::from_le_bytes([copy[o], copy[o + 1], copy[o + 2], copy[o + 3]]);
                    writeln!(f, "live_permission={} idle_delay_ms={}", word(0x10), word(0x94))?;
                }
            }
        }
        Ok(())
    }
}

/// `echo "idle 1" > power` / `"idle 0"`: publish one counted IdlePowerOff control (allow /
/// keep-and-wake). Only with the HAL300 idle policy (default; debug_flags bit 47 disables it).
impl kernel::debugfs::Reader for PowerView {
    fn read_from_slice(&self, reader: &mut kernel::uaccess::UserSliceReader) -> Result {
        let mut buf = [0u8; 16];
        let n = reader.len();
        if n == 0 {
            return Ok(());
        }
        if n > buf.len() {
            return Err(EINVAL);
        }
        reader.read_slice(&mut buf[..n])?;
        let text = core::str::from_utf8(&buf[..n]).map_err(|_| EINVAL)?.trim();
        if text == "seed" {
            return self.runtime_seed();
        }
        if let Some(value) = text.strip_prefix("prio ") {
            let on = match value {
                "1" => true,
                "0" => false,
                _ => return Err(EINVAL),
            };
            super::set_queue_priorities(on);
            pr_info!("asahi: HAL300 requested queue priorities {} for new queues\n",
                     if on { "kept" } else { "replaced by MEDIUM" });
            return Ok(());
        }
        if let Some(value) = text.strip_prefix("serialize ") {
            let on = match value {
                "1" => true,
                "0" => false,
                _ => return Err(EINVAL),
            };
            super::runtime::set_render_serialize(on);
            pr_info!("asahi: cross-owner render serialization {}\n", if on { "on" } else { "off" });
            return Ok(());
        }
        let allow = match text {
            "idle 1" => true,
            "idle 0" => false,
            _ => return Err(EINVAL),
        };
        if !super::idle_policy_enabled() {
            return Err(EPERM);
        }
        let mut state = self.0.state.lock();
        let firmware = (*state).as_deref_mut().ok_or(ENODEV)?;
        let target = super::power::Control::set_idle(firmware, allow)?;
        pr_info!("asahi: power view published IdlePowerOff allow={} target={}\n", allow, target);
        Ok(())
    }
}

impl PowerView {
    /// `echo seed > power` (idle policy on, idle GPU, no clients): one runtime descent and
    /// wake to seed the reason-4 DVFS controller, instead of the startup permission. Permit
    /// (C+0x30 = 1, force-config snapshot 0x15/0x2a, allow = 1), wait for the GPU to power down
    /// and stay down 100 ms, then withdraw (C+0x30 = 0, snapshot) and wake (allow = 0).
    fn runtime_seed(&self) -> Result {
        use super::power::Control;
        if !super::idle_policy_enabled() {
            return Err(EPERM);
        }
        let snapshot = |firmware: &mut super::Firmware, permission: u32| -> Result {
            firmware.init.set_startup_idle_permission(permission)?;
            firmware.init.publish_role_control(super::Role::Primary, 0x15)?;
            firmware.init.publish_role_control(super::Role::Secondary, 0x2a)?;
            core::sync::atomic::fence(Ordering::SeqCst);
            firmware.primary.notify(super::MSG_CONTROL_NOTIFY)?;
            firmware.secondary.notify(super::MSG_CONTROL_NOTIFY)?;
            Ok(())
        };
        {
            let mut state = self.0.state.lock();
            let firmware = (*state).as_deref_mut().ok_or(ENODEV)?;
            snapshot(firmware, 1)?;
            firmware.set_idle(true)?;
        }
        let mut down_since = None;
        let mut descended = false;
        for _ in 0..300 {
            kernel::time::delay::fsleep(kernel::time::Delta::from_millis(10));
            let powered = {
                let state = self.0.state.lock();
                let firmware = (*state).as_deref().ok_or(ENODEV)?;
                firmware.powered()
            };
            if powered {
                down_since = None;
            } else {
                let since = *down_since.get_or_insert(super::now_ns());
                if super::now_ns().saturating_sub(since) >= 100_000_000 {
                    descended = true;
                    break;
                }
            }
        }
        {
            let mut state = self.0.state.lock();
            let firmware = (*state).as_deref_mut().ok_or(ENODEV)?;
            snapshot(firmware, 0)?;
            firmware.set_idle(false)?;
        }
        let mut woken = false;
        for _ in 0..300 {
            kernel::time::delay::fsleep(kernel::time::Delta::from_millis(10));
            let state = self.0.state.lock();
            if (*state).as_deref().ok_or(ENODEV)?.powered() {
                woken = true;
                break;
            }
        }
        pr_info!(
            "asahi: runtime DVFS seed: descended {} (held 100 ms), woken {}\n",
            descended,
            woken
        );
        Ok(())
    }
}

impl kernel::debugfs::Writer for RenderView {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        use super::fw::queue::DataMaster;
        writeln!(f, "version=1 sampled_ns={}", super::now_ns())?;
        let state = self.0.state.lock();
        let Some(firmware) = state.as_deref() else {
            return writeln!(f, "online=0");
        };
        writeln!(f, "online=1")?;
        let init = &firmware.init;
        for (name, engine) in [
            ("tiling", DataMaster::Tiling),
            ("fragment", DataMaster::Fragment),
            ("compute", DataMaster::Compute),
        ] {
            for priority in 0..4u8 {
                match init.work_cursors(priority, engine) {
                    Ok([consumer, intermediate, producer]) => {
                        if consumer != 0 || intermediate != 0 || producer != 0 {
                            writeln!(
                                f,
                                "work_ring {} priority={} consumer={} intermediate={} producer={}",
                                name, priority, consumer, intermediate, producer
                            )?;
                            // The newest records before the producer, oldest first.
                            for back in (1..=producer.min(4)).rev() {
                                let index = producer - back;
                                match init.work_record(priority, engine, index) {
                                    Ok(record) => {
                                        write!(f, "work_record {} priority={} index={}:", name,
                                               priority, index)?;
                                        for byte in record {
                                            write!(f, " {:02x}", byte)?;
                                        }
                                        writeln!(f)?;
                                    }
                                    Err(error) => writeln!(f, "work_record {} index={} unavailable={:?}",
                                                           name, index, error)?,
                                }
                            }
                        }
                    }
                    Err(error) => writeln!(f, "work_ring {} priority={} unavailable={:?}",
                                           name, priority, error)?,
                }
            }
        }
        for qid in 0..8u8 {
            match init.stamp(qid) {
                Ok(stamp) => {
                    let value = stamp.load(Ordering::Relaxed);
                    if value != 0 {
                        writeln!(f, "stamp qid={} value={:#x}", qid, value)?;
                    }
                }
                Err(error) => writeln!(f, "stamp qid={} unavailable={:?}", qid, error)?,
            }
        }
        firmware.queues.write_render_observation(f)?;
        Ok(())
    }
}

impl kernel::debugfs::Writer for View {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        writeln!(f, "version=1 sampled_ns={} crashed={}", super::now_ns(),
                 u8::from(self.0.crashed.load(Ordering::Acquire)))?;
        {
            use super::initdata::{
                COMPLETION_FULL_SCANS, COMPLETION_LATCHED, COMPLETION_SCANS, COMPLETION_SLOTS,
            };
            writeln!(f, "compute_completions scans={} full={} slots={} latched={}",
                     COMPLETION_SCANS.load(Ordering::Relaxed),
                     COMPLETION_FULL_SCANS.load(Ordering::Relaxed),
                     COMPLETION_SLOTS.load(Ordering::Relaxed),
                     COMPLETION_LATCHED.load(Ordering::Relaxed))?;
        }
        let uat = {
            let state = self.0.state.lock();
            let Some(firmware) = state.as_deref() else { return writeln!(f, "online=0"); };
            writeln!(f, "online=1")?;
            firmware.queues.write_capacity(f)?;
            firmware.pool.write_capacity(f)?;
            writeln!(f, "render_pool_ids used={} total=127", firmware.render_ids.used())?;
            firmware.uat.clone()
        };
        // Mapping destruction can issue firmware flushes while holding VM
        // locks. Never take a VM/TTBAT lock with the device mutex held here.
        // The retained UAT owns valid allocator metadata even during shutdown.
        writeln!(f, "execution_contexts used={} total={}", uat.execution_contexts_used(), crate::mmu::MAX_EXECUTION_CONTEXTS)?;
        let mut upper = uat.geometry().kernel_range();
        upper.start += crate::hw::t8140::dynamic::KERNEL_OFFSET;
        for (name, vm, range) in [
            ("dynamic_lower_bytes", uat.kernel_lower_vm(), crate::hw::t8140::dynamic::LOWER),
            ("dynamic_upper_bytes", uat.kernel_vm(), upper),
        ] {
            match vm.capacity(range.clone()) {
                Ok((used, largest)) => writeln!(f, "{} used={} total={} largest_hole={}", name, used, range.end - range.start, largest)?,
                Err(error) => writeln!(f, "{} unavailable={:?}", name, error)?,
            }
        }
        for (site, count) in [
            Site::ComputeReserve, Site::PhysicalQids, Site::RenderSlots, Site::LogicalQueues,
            Site::ComputeSlots, Site::QosIds, Site::ComputeItems, Site::RetiredRenderBindings,
            Site::RenderPoolIds, Site::RenderWorkSlots, Site::ParameterBuffers, Site::MetricsIds,
            Site::LowerVa, Site::UpperVa, Site::AliasVa, Site::ExecutionContexts,
        ].iter().zip(REFUSALS.iter()) {
            writeln!(f, "refusals site={:?} count={}", site, count.load(Ordering::Relaxed))?;
        }
        Ok(())
    }
}

/// Physical ranges of both coprocessors' `__DATA` segments, recorded when HAL300 boot admits
/// them. Zero sizes mean "not recorded".
static FIRMWARE_DATA: [[AtomicU64; 2]; 2] = [const { [const { AtomicU64::new(0) }; 2] }; 2];

/// Read-only text dump of one coprocessor's `__DATA` segment (non-zero 32-byte lines only),
/// copied through a temporary mapping at read time. The firmware's scheduler state lives here;
/// a stalled pass is attributed by comparing this snapshot with the original firmware image.
pub(super) struct FirmwareData(pub(super) usize);

impl FirmwareData {
    pub(super) fn record(role: usize, range: &core::ops::Range<u64>) {
        if let Some([start, size]) = FIRMWARE_DATA.get(role) {
            start.store(range.start, Ordering::Relaxed);
            size.store(range.end - range.start, Ordering::Release);
        }
    }
}

const DUMP_LINE: usize = 32;

fn write_hex(f: &mut kernel::fmt::Formatter<'_>, base: u64, data: &[u8]) -> kernel::fmt::Result {
    for (index, line) in data.chunks(DUMP_LINE).enumerate() {
        if line.iter().all(|&byte| byte == 0) {
            continue;
        }
        write!(f, "{:016x}:", base + (index * DUMP_LINE) as u64)?;
        for byte in line {
            write!(f, " {:02x}", byte)?;
        }
        writeln!(f)?;
    }
    Ok(())
}

impl kernel::debugfs::Writer for FirmwareData {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        let Some([start, size]) = FIRMWARE_DATA.get(self.0) else {
            return writeln!(f, "role={} unavailable", self.0);
        };
        let size = size.load(Ordering::Acquire);
        let start = start.load(Ordering::Relaxed);
        writeln!(f, "version=1 role={} phys={:#x} size={:#x} sampled_ns={}", self.0, start, size,
                 super::now_ns())?;
        if size == 0 {
            return Ok(());
        }
        match super::hal300::copy_physical(start, size as usize) {
            Ok(copy) => write_hex(f, 0, &copy),
            Err(error) => writeln!(f, "copy failed {:?}", error),
        }
    }
}

/// Read-only peek into the firmware's (kernel) address space: write "VA LENGTH" in hex, then
/// read. Only mapped System RAM is returned; nothing is written to GPU memory or page tables.
pub(super) struct Peek {
    pub(super) shared: Arc<super::Shared>,
    pub(super) va: AtomicU64,
    pub(super) length: AtomicU64,
}

const PEEK_MAX: u64 = 0x10000;

/// HAL300 keeps its UAT until reboot; diagnostics keep one leaked reference so a pass can still
/// be inspected after the runtime gave up on it (the firmware and its memory stay retained).
static RETAINED_UAT: core::sync::atomic::AtomicPtr<crate::mmu::Uat> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// The UAT HAL300 boot retained for diagnostics, if any.
fn retained_uat() -> Option<&'static crate::mmu::Uat> {
    let retained = RETAINED_UAT.load(Ordering::Acquire);
    // SAFETY: a non-null pointer holds a leaked strong reference that is never dropped.
    (!retained.is_null()).then(|| unsafe { &*retained })
}

pub(super) fn retain_uat(uat: &Arc<crate::mmu::Uat>) {
    let raw = Arc::into_raw(uat.clone()).cast_mut();
    if RETAINED_UAT
        .compare_exchange(core::ptr::null_mut(), raw, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        // SAFETY: `raw` came from Arc::into_raw() above and was not published.
        drop(unsafe { Arc::from_raw(raw) });
    }
}

impl kernel::debugfs::Reader for Peek {
    fn read_from_slice(&self, reader: &mut kernel::uaccess::UserSliceReader) -> Result {
        let mut buf = [0u8; 64];
        let n = reader.len();
        if n == 0 {
            // A shell may follow its write with an empty one.
            return Ok(());
        }
        if n > buf.len() {
            return Err(EINVAL);
        }
        reader.read_slice(&mut buf[..n])?;
        let text = core::str::from_utf8(&buf[..n]).map_err(|_| EINVAL)?;
        let mut words = text.split_whitespace();
        let mut hex = || -> Result<u64> {
            let word = words.next().ok_or(EINVAL)?;
            u64::from_str_radix(word.trim_start_matches("0x"), 16).map_err(|_| EINVAL)
        };
        let (va, length) = (hex()?, hex()?);
        if length == 0 || length > PEEK_MAX {
            return Err(EINVAL);
        }
        self.va.store(va, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        Ok(())
    }
}

impl kernel::debugfs::Writer for Peek {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        let (va, length) = (self.va.load(Ordering::Relaxed), self.length.load(Ordering::Relaxed));
        writeln!(f, "version=1 va={:#x} length={:#x} sampled_ns={}", va, length, super::now_ns())?;
        if length == 0 {
            return Ok(());
        }
        let online = self.shared.state.lock().is_some();
        let Some(uat) = retained_uat() else { return writeln!(f, "online=0"); };
        writeln!(f, "online={}", u8::from(online))?;
        // As for View: never take a VM lock with the device mutex held.
        let vm = if va >> 63 != 0 { uat.kernel_vm() } else { uat.kernel_lower_vm() };
        let mut data = match KVVec::from_elem(0u8, length as usize, GFP_KERNEL) {
            Ok(data) => data,
            Err(_) => return writeln!(f, "allocation failed"),
        };
        // Page by page, so one unmapped page does not hide the rest.
        let mut offset = 0usize;
        while offset < data.len() {
            let address = va + offset as u64;
            let count = (0x4000 - (address as usize & 0x3fff)).min(data.len() - offset);
            if let Err(error) = vm.read_diagnostic(address, &mut data[offset..offset + count]) {
                writeln!(f, "unreadable va={:#x} length={:#x} error={:?}", address, count, error)?;
            }
            offset += count;
        }
        write_hex(f, va, &data)
    }
}

/// Firmware addresses of the HAL300 recovery summary (status B +0x4a90) and report view,
/// recorded by HAL300 boot.
static RECOVERY_VAS: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
const RECOVERY_SUMMARY_BYTES: usize = 0x70;
const RECOVERY_REPORT_BYTES: usize = 0x80;
/// Offsets from the summary start: the epoch (u64) and the handshake state (u32).
const RECOVERY_EPOCH: u64 = 0x40;
const RECOVERY_STATE: u64 = 0x50;
static RECOVERY_ACKNOWLEDGED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// HAL300 recovery handshake: read shows the status B summary/handshake words and the
/// firmware's recovery report; writing "ack-pre EPOCH" performs the native host's
/// pre-recovery acknowledgement (1 -> 2) once per boot, only while the firmware is stopped at
/// state 1 of that epoch. State 3 (post-recovery) is never acknowledged here.
pub(super) struct Recovery;

impl Recovery {
    pub(super) fn record(summary: u64, report: u64) {
        RECOVERY_VAS[0].store(summary, Ordering::Relaxed);
        RECOVERY_VAS[1].store(report, Ordering::Release);
    }
}

impl kernel::debugfs::Writer for Recovery {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        let report = RECOVERY_VAS[1].load(Ordering::Acquire);
        let summary = RECOVERY_VAS[0].load(Ordering::Relaxed);
        writeln!(f, "version=1 sampled_ns={} summary_va={:#x} report_va={:#x} acknowledged={}",
                 super::now_ns(), summary, report,
                 u8::from(RECOVERY_ACKNOWLEDGED.load(Ordering::Relaxed)))?;
        let Some(uat) = retained_uat() else { return writeln!(f, "uat=none"); };
        if summary == 0 || report == 0 {
            return writeln!(f, "unrecorded");
        }
        let vm = uat.kernel_vm();
        let mut words = [0u8; RECOVERY_SUMMARY_BYTES];
        match vm.read_diagnostic(summary, &mut words) {
            Ok(()) => {
                let word = |at: usize| u32::from_le_bytes([words[at], words[at + 1], words[at + 2], words[at + 3]]);
                writeln!(f, "state={} epoch={:#x}", word(RECOVERY_STATE as usize),
                         u64::from(word(RECOVERY_EPOCH as usize)) | u64::from(word(RECOVERY_EPOCH as usize + 4)) << 32)?;
                write_hex(f, summary, &words)?;
            }
            Err(error) => writeln!(f, "summary unreadable {:?}", error)?,
        }
        let mut bytes = [0u8; RECOVERY_REPORT_BYTES];
        match vm.read_diagnostic(report, &mut bytes) {
            Ok(()) => {
                writeln!(f, "report:")?;
                write_hex(f, report, &bytes)
            }
            Err(error) => writeln!(f, "report unreadable {:?}", error),
        }
    }
}

impl kernel::debugfs::Reader for Recovery {
    fn read_from_slice(&self, reader: &mut kernel::uaccess::UserSliceReader) -> Result {
        let mut buf = [0u8; 64];
        let n = reader.len();
        if n == 0 {
            // A shell may follow its write with an empty one.
            return Ok(());
        }
        if n > buf.len() {
            return Err(EINVAL);
        }
        reader.read_slice(&mut buf[..n])?;
        let text = core::str::from_utf8(&buf[..n]).map_err(|_| EINVAL)?;
        let mut words = text.split_whitespace();
        if words.next() != Some("ack-pre") {
            return Err(EINVAL);
        }
        let epoch = words.next().ok_or(EINVAL)?;
        let epoch = u64::from_str_radix(epoch.trim_start_matches("0x"), 16).map_err(|_| EINVAL)?;
        let summary = RECOVERY_VAS[0].load(Ordering::Acquire);
        let uat = retained_uat().ok_or(ENODEV)?;
        if summary == 0 {
            return Err(ENODEV);
        }
        if RECOVERY_ACKNOWLEDGED.swap(true, Ordering::AcqRel) {
            return Err(EALREADY);
        }
        let result = uat.kernel_vm().diagnostic_compare_store_u32(
            summary + RECOVERY_STATE, 1, 2, summary + RECOVERY_EPOCH, epoch);
        pr_warn!("asahi: HAL300 pre-recovery acknowledgement (1 -> 2) epoch {:#x}: {:?}\n",
                 epoch, result);
        if result.is_err() {
            RECOVERY_ACKNOWLEDGED.store(false, Ordering::Release);
        }
        result
    }
}
