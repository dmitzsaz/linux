// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Firmware restart handshake and per-VM failure classification.
//!
//! The device mutex serializes this state with publication. Preparations are closed and joined
//! without that mutex before `service`; no queue callback may signal a fence here. Queue
//! owners retain failed work until its retirement witness, a complete pair-drain witness, or
//! processor stop. A failed fence alone never releases mappings.

mod attribution;
mod slot;
mod safety;
pub(crate) use attribution::Sources;
use attribution::Attribution;

pub(super) fn slot_key(slot: usize) -> Option<u64> {
    slot::key(slot).ok()
}

use super::status::VmStatus;
use kernel::{prelude::*, sync::Arc};

const QIDS: u8 = 128;
const GUILTY: u8 = 1 << 1;
const CANCELLED: u8 = 1 << 2;
const MAX_GUILTY_VMS: usize = 8;
const RECOVERY_POLLS: usize = 500;

/// Per-QID classifications from the complete firmware recovery table.
/// Multiple records for a queue contribute all of their classification bits.
#[derive(Default)]
pub(super) struct Entries {
    guilty: u128,
    cancelled: u128,
    /// Every QID with a valid record. On HAL300 each record is one kick the firmware cancelled
    /// in its kick walker (P+0xfe64), whatever its other bits say.
    recorded: u128,
    /// QIDs of valid records flagged as TA queues (bit3).
    ta: u128,
}

impl Entries {
    pub(super) fn include(&mut self, flags: u32) {
        if flags & 1 == 0 {
            return;
        }
        let qid = (flags >> 4) & 0x7f;
        let bit = 1u128 << qid;
        self.recorded |= bit;
        if flags & (1 << 3) != 0 {
            self.ta |= bit;
        }
        if flags & u32::from(GUILTY) != 0 {
            self.guilty |= bit;
        }
        if flags & u32::from(CANCELLED) != 0 {
            self.cancelled |= bit;
        }
    }
}

/// How an error completion affects its VM's sticky status and other jobs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Class {
    /// Only this work's fence fails; VM status and unrelated work remain usable.
    Spared,
    /// This VM owns guilty work.
    Guilty,
}

/// Queue operations under the device mutex. Completion signalling belongs to the caller after
/// that mutex is dropped. `oldest_spared` returns None if the QID has no in-flight owner.
pub(super) trait Queues {
    fn render_scheduler_active(&self) -> bool;
    fn render_pass_started(&self, qid: u8, stamp: u64) -> Option<bool>;
    /// Copy every retained queue's live producer to its recovery target, then
    /// issue a full barrier. A failed graph must not skip subsequent queues.
    fn publish_recovery_targets(&self) -> Option<Error>;
    fn oldest_spared(&self, qid: u8) -> Option<bool>;
    fn visit_owners(&self, qid: u8, visit: &mut dyn FnMut(&Arc<VmStatus>));
    /// Classify every in-flight completion; return QIDs of render slots with spared work.
    fn classify_work(&mut self, class: &dyn Fn(&Arc<VmStatus>) -> Class) -> u128;
    fn settle_blamed_render(&mut self, qid: u8, error: Error);
    fn settle_cancelled_renders(&mut self, qids: u128);
    fn rescan_compute(&mut self) -> Result;
    fn release_retired_compute(&mut self);
    /// Fails every VM that has published work in flight (HAL300 fail-all policy).
    fn fail_all_in_flight(&mut self, error: Error);
    /// Fails every VM that owns work on one of `qids`; returns how many VMs were failed.
    fn fail_qid_owners(&mut self, qids: u128, error: Error) -> usize;
    /// QIDs of every retained render pair (both halves).
    fn render_qids(&self) -> u128;
    /// Clears the TPC of every render pair whose TA QID is in `qids`; returns the pair count.
    fn scrub_render_tpc(&mut self, qids: u128) -> usize;
}

/// Live shared-memory operations. The handshake word uses aligned 32-bit accesses; transitions
/// check the expected value, issue a full barrier, store the next value, then another barrier.
pub(super) trait Memory {
    fn recovery_state(&self) -> Result<u32>;
    fn host_recovery(&self) -> Result<u32>;
    fn fault_sources(&self) -> Result<Sources>;
    fn transition(&self, expected: u32, next: u32) -> Result;
    fn clear_timestamps(&self) -> Result;
    fn entries(&self) -> Result<Entries>;
    /// One millisecond wait; this is polled because the firmware sends no state-3 notification.
    fn wait_tick(&self);
    /// Whether this firmware's recovery fails all in-flight work instead of attributing it.
    fn fail_all_policy(&self) -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Request {
    generation: u64,
    blamed: Option<u8>,
}

/// Generation accepted by the host and the coalesced request from either coprocessor.
pub(super) struct State {
    accepted: u64,
    pending: Option<Request>,
    timeouts: safety::Storm,
    restarts: safety::Storm,
}

/// Non-fatal post-recovery failures. The firmware handshake has already closed successfully.
pub(super) struct ServiceResult {
    pub(super) scan_error: Option<Error>,
    pub(super) target_error: Option<Error>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            accepted: 0,
            pending: None,
            timeouts: safety::Storm::new(),
            restarts: safety::Storm::new(),
        }
    }
    pub(super) fn timeout_limit(&mut self, now: u64) -> bool { self.timeouts.note(now) }
    pub(super) fn restart_limit(&mut self, now: u64) -> bool { self.restarts.note(now) }
    pub(super) fn generation(&self) -> u64 {
        self.accepted
    }
    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns false for a consumed duplicate from an older generation, true when the event
    /// drain must stop until the current request is serviced. The first blamed QID wins.
    pub(super) fn request(&mut self, generation: u64, stamp: i32) -> Result<bool> {
        if generation < self.accepted {
            return Ok(false);
        }
        if generation != self.accepted {
            return Err(EIO);
        }
        if stamp != -1 && !(0..i32::from(QIDS)).contains(&stamp) {
            return Err(EIO);
        }
        let blamed = if (0..i32::from(QIDS)).contains(&stamp) {
            Some(stamp as u8)
        } else {
            None
        };
        if let Some(pending) = self.pending.as_mut() {
            if pending.generation != generation {
                return Err(EBUSY);
            }
            pending.blamed = pending.blamed.or(blamed);
        } else {
            self.pending = Some(Request { generation, blamed });
        }
        Ok(true)
    }
}

/// The firmware owner implements memory and queue access without splitting its
/// mutable borrow or duplicating the authoritative restart generation.
pub(super) trait Host: Memory + Queues {
    fn recovery(&self) -> &State;
    fn recovery_mut(&mut self) -> &mut State;
}

/// Performs one pending restart. The caller has closed preparation admission and joined all
/// builders off-lock. Work remains mapped throughout every callback, including on error.
pub(super) fn service<H: Host>(
    host: &mut H,
    crashed: impl Fn() -> bool,
) -> Result<ServiceResult> {
    let request = host.recovery().pending.ok_or(EINVAL)?;
    if request.generation != host.recovery().accepted || host.host_recovery()? != 0 {
        return Err(EIO);
    }
    // The restart record is published before firmware announces state 1.
    // Its ring cursor is already consumed; wait only for that readiness word,
    // without advancing the host generation or acknowledging an early event.
    let mut ready = false;
    for _ in 0..RECOVERY_POLLS {
        if crashed() {
            return Err(EIO);
        }
        match host.recovery_state()? {
            1 => {
                ready = true;
                break;
            }
            0 => host.wait_tick(),
            _ => return Err(EIO),
        }
    }
    if !ready {
        return Err(ETIMEDOUT);
    }
    // Recheck ownership after the off-CPU waits, before any handshake mutation.
    if host.host_recovery()? != 0 {
        return Err(EIO);
    }
    host.recovery_mut().accepted = host.recovery().accepted.wrapping_add(1);
    if request.generation == 0 && !host.render_scheduler_active() {
        host.clear_timestamps()?;
    }
    let mut sources = host.fault_sources()?;
    // Native restartWorkQueue copies producers to recovery targets only for host-initiated
    // recovery; the firmware-initiated (MMU, hang) path HAL300 serves bypasses that copy, and its
    // reconstruction never reads the targets (sol-hal300-recovery-innocents-study.md section 1).
    let target_error = if host.fail_all_policy() { None } else { host.publish_recovery_targets() };
    host.transition(1, 2)?;
    let mut recovered = false;
    for _ in 0..RECOVERY_POLLS {
        if host.recovery_state()? == 3 {
            recovered = true;
            break;
        }
        if crashed() {
            return Err(EIO);
        }
        host.wait_tick();
    }
    if !recovered {
        return Err(ETIMEDOUT);
    }
    // A failed table read leaves only the explicit blamed QID as evidence of guilt.
    let entries = host.entries().unwrap_or_default();
    // HAL300: before releasing the firmware, clear the tail-pointer cache of every render pair
    // whose TA kick was cancelled, as native processFirmwareTACommandsRecovery does at state 3
    // (sol-hal300-recovery-innocents-study.md section 2).
    let scrubbed = if host.fail_all_policy() { host.scrub_render_tpc(entries.ta) } else { 0 };
    host.transition(3, 0)?;
    if scrubbed != 0 {
        pr_info!("G17: recovery cleared the tail pointers of {} cancelled render pair(s)\n", scrubbed);
    }
    if host.recovery_state()? != 0 {
        return Err(EIO);
    }
    host.recovery_mut().pending = None;

    if host.fail_all_policy() {
        // HAL300 (asahi-pw7/pw8-boot): the firmware kept running, reconstructed its queues and
        // left one cleanup record per kick it cancelled - exactly the faulting kick, with bit1
        // (data master and hardware slot match the report) and its QID - while every other
        // queue's in-flight work simply continued and retired. So only the owners of recorded
        // or blamed QIDs lose their work. If none can be resolved, or debug_flags bit 52 asks
        // for the first conservative policy, every VM with work in flight loses it. Nothing is
        // replayed either way.
        // A record with bit1 (data master and hardware slot match the report) or the event's
        // QID is guilty. Other records are innocent kicks the firmware cancelled: a cancelled
        // render pass fails alone (ECANCELED, its VM stays usable, asahi-pw13-boot showed that
        // failing those VMs kills the compositor on every recovery that catches it mid-frame);
        // a cancelled compute kick has no partial-failure path here, so its owner fails too.
        let guilty = entries.guilty | request.blamed.map_or(0, |qid| 1u128 << qid);
        let cancelled = entries.recorded & !guilty;
        let cancelled_render = cancelled & host.render_qids();
        let qids = guilty | (cancelled & !cancelled_render);
        let fail_all = crate::module_parameters::debug_flags.value() & (1 << 52) != 0;
        let failed = if fail_all || qids == 0 { 0 } else { host.fail_qid_owners(qids, EIO) };
        if failed != 0 && cancelled_render != 0 {
            host.classify_work(&|status| {
                if status.get() != 0 { Class::Guilty } else { Class::Spared }
            });
            host.settle_cancelled_renders(cancelled_render);
        }
        let action = if failed != 0 {
            "failing the owners of those queues"
        } else {
            "failing all in-flight work"
        };
        pr_warn!(
            "G17: GPU recovery (reason {:?}, firmware blamed queue {:?}, recorded queues {:#x}, guilty {:#x}, cancelled renders {:#x}): {}\n",
            sources.reason,
            request.blamed,
            entries.recorded,
            guilty,
            cancelled_render,
            action
        );
        if failed == 0 {
            host.fail_all_in_flight(EIO);
        }
        let scan_error = host.rescan_compute().err();
        host.release_retired_compute();
        return Ok(ServiceResult { scan_error, target_error });
    }

    let (cancelled, blame) = classify(host, entries, request.blamed, &mut sources);
    if let Some(qid) = request.blamed {
        pr_warn!(
            "G17: GPU recovery (reason {:?}): firmware blamed queue {}, {}\n",
            sources.reason,
            qid,
            if blame { "failing its oldest render" } else { "attribution cleared it; spared" }
        );
        if blame {
            host.settle_blamed_render(qid, EIO);
        }
    }
    if cancelled != 0 {
        host.settle_cancelled_renders(cancelled);
    }
    let scan_error = host.rescan_compute().err();
    host.release_retired_compute();
    Ok(ServiceResult { scan_error, target_error })
}

/// Resolves guilty QIDs before changing any completion's classification. Unknown ownership or
/// too many distinct guilty VMs leaves all prior classifications intact. The oldest spared work
/// of a QID makes later blame collateral to its previous recovery, rather than new VM guilt.
/// Also returns whether the firmware's blamed QID is still guilty after attribution: a blamed
/// render that had not started while others had (or with no culprit at all) is innocent, and its
/// work is spared like that of every other innocent VM instead of failing with EIO.
fn classify<Q: Queues>(queues: &mut Q, entries: Entries, blamed: Option<u8>, sources: &mut Sources) -> (u128, bool) {
    let mut guilty_qids = entries.guilty | blamed.map_or(0, |qid| 1u128 << qid);
    let cancelled = entries.cancelled;
    let attribution = sources.attribute(blamed, |qid, stamp| queues.render_pass_started(qid, stamp));
    match attribution {
        Attribution::Firmware => {}
        Attribution::Queues(qids) => guilty_qids = qids,
        Attribution::NoCulprit => guilty_qids = 0,
    }
    let no_culprit = attribution == Attribution::NoCulprit;
    let blame = blamed.is_some_and(|qid| guilty_qids & (1u128 << qid) != 0);
    if guilty_qids == 0 && !no_culprit {
        return (0, blame);
    }
    let mut guilty: [Option<Arc<VmStatus>>; MAX_GUILTY_VMS] = Default::default();
    let mut count = 0;
    let mut collateral = 0u128;
    let mut unresolved = false;
    for qid in 0..QIDS {
        if guilty_qids & (1u128 << qid) == 0 {
            continue;
        }
        match queues.oldest_spared(qid) {
            None => unresolved = true,
            Some(true) => collateral |= 1u128 << qid,
            Some(false) => {
                let mut found = false;
                queues.visit_owners(qid, &mut |status| {
                    found = true;
                    if guilty[..count]
                        .iter()
                        .flatten()
                        .any(|known| Arc::ptr_eq(known, status))
                    {
                        return;
                    }
                    if let Some(slot) = guilty.get_mut(count) {
                        *slot = Some(status.clone());
                        count += 1;
                    } else {
                        unresolved = true;
                    }
                });
                unresolved |= !found;
            }
        }
    }
    if unresolved || (count == 0 && collateral == 0 && !no_culprit) {
        return (0, blame);
    }
    let spared = queues.classify_work(&|status| {
        guilty[..count]
            .iter()
            .find_map(|known| {
                known
                    .as_ref()
                    .filter(|known| Arc::ptr_eq(known, status))
                    .map(|_| Class::Guilty)
            })
            .unwrap_or(Class::Spared)
    });
    (cancelled & !(guilty_qids & !collateral) & spared, blame)
}
