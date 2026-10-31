// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Read-only account of what this host told the firmware about each hardware
//! queue, next to the queue state the firmware's own scheduler reconstruction
//! reads back.
//!
//! The HAL300 blocker for releasing client resources is an invariant of that
//! reconstruction: it walks a saved per-engine queue mask and, for every queue
//! in it, reads an indexed descriptor whose top bits carry `data_master + 1`.
//! A zero there underflows its bitmap index and the firmware takes a data
//! abort (observed for KSM QID 10 right after a scheduler release). The
//! registration path that types a descriptor is the tag-15 queue
//! configuration, so attributing a zero needs both halves: which queues this
//! host ever configured and with what, and which queues the firmware is
//! actually scanning.
//!
//! Everything here is observation. No register is written, no selector is
//! moved, and nothing is published or reclaimed: the descriptor of a queue
//! other than the one the firmware currently selects cannot be read without
//! writing that selector, which would race the firmware's own scheduling, so
//! this file deliberately reports only the selected one.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use kernel::prelude::*;

use super::fw::kick::QID_MAX;

/// One queue's configuration history, as this host wrote it.
///
/// Counters only grow; addresses and identities hold what the newest record
/// carried. `engines` is a mask of the data masters seen, so a queue that was
/// reused for a different engine is visible as more than one bit rather than
/// as a silently overwritten field.
struct Slot {
    records: AtomicU32,
    installs: AtomicU32,
    engines: AtomicU32,
    priority: AtomicU32,
    context: AtomicU32,
    scheduler_va: AtomicU64,
    kick_ring_va: AtomicU64,
    last_ns: AtomicU64,
}

impl Slot {
    const fn new() -> Self {
        Self {
            records: AtomicU32::new(0),
            installs: AtomicU32::new(0),
            engines: AtomicU32::new(0),
            priority: AtomicU32::new(0),
            context: AtomicU32::new(0),
            scheduler_va: AtomicU64::new(0),
            kick_ring_va: AtomicU64::new(0),
            last_ns: AtomicU64::new(0),
        }
    }
}

const SLOTS: usize = QID_MAX as usize + 1;
static QUEUES: [Slot; SLOTS] = [const { Slot::new() }; SLOTS];

/// Records one built tag-15 queue configuration.
///
/// Called from the single constructor of that record, so it covers compute and
/// both render stages without touching their publishers. A built record is not
/// proof of a consumed one; the counters are named accordingly.
pub(crate) fn configured(
    qid: u8,
    data_master: u16,
    install: bool,
    priority: u16,
    context_id: u16,
    scheduler_va: u64,
    kick_ring_va: u64,
) {
    let Some(slot) = QUEUES.get(usize::from(qid)) else {
        return;
    };
    slot.records.fetch_add(1, Ordering::Relaxed);
    if install {
        slot.installs.fetch_add(1, Ordering::Relaxed);
    }
    slot.engines
        .fetch_or(1u32 << (u32::from(data_master) & 31), Ordering::Relaxed);
    slot.priority.store(u32::from(priority), Ordering::Relaxed);
    slot.context.store(u32::from(context_id), Ordering::Relaxed);
    slot.scheduler_va.store(scheduler_va, Ordering::Relaxed);
    slot.kick_ring_va.store(kick_ring_va, Ordering::Relaxed);
    slot.last_ns.store(super::now_ns(), Ordering::Relaxed);
}

/// Read-only view of the queue-registration evidence.
pub(crate) struct View(pub(crate) crate::regs::G17Resources);

impl kernel::debugfs::Writer for View {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        writeln!(f, "version=1 sampled_ns={}", super::now_ns())?;

        // Firmware side first: the two masks the reconstruction iterates, the
        // two the registration path publishes, and the descriptor of whichever
        // queue is selected right now.
        let state = self.0.queue_state();
        match state {
            Ok(state) => {
                let [scan_low, scan_high, published_low, published_high, selector, descriptor] =
                    state;
                writeln!(f, "powered=1")?;
                writeln!(f, "scan_mask={:#018x} {:#018x}", scan_low, scan_high)?;
                writeln!(
                    f,
                    "published_mask={:#018x} {:#018x}",
                    published_low, published_high
                )?;
                writeln!(
                    f,
                    "selected qid={} descriptor={:#018x} type={}",
                    selector,
                    descriptor,
                    (descriptor >> 60) & 3
                )?;
                for qid in 0..128u32 {
                    let word = if qid < 64 { scan_low } else { scan_high };
                    if word & (1u64 << (qid % 64)) == 0 {
                        continue;
                    }
                    let slot = QUEUES.get(qid as usize);
                    let records = slot.map_or(0, |slot| slot.records.load(Ordering::Relaxed));
                    if records == 0 {
                        // Scanned, but this host never configured it: the
                        // adopted startup graph or the firmware itself owns
                        // this queue, and its typing is not ours to explain.
                        writeln!(f, "scanned qid={} host_records=0", qid)?;
                    } else {
                        writeln!(f, "scanned qid={} host_records={}", qid, records)?;
                    }
                }
            }
            Err(error) => writeln!(f, "powered=0 unavailable={:?}", error)?,
        }

        // Host side: every queue this boot ever configured.
        for (qid, slot) in QUEUES.iter().enumerate() {
            let records = slot.records.load(Ordering::Relaxed);
            if records == 0 {
                continue;
            }
            writeln!(
                f,
                "config qid={} records={} installs={} engines={:#x} priority={} context={} \
                 scheduler={:#x} kick_ring={:#x} last_ns={}",
                qid,
                records,
                slot.installs.load(Ordering::Relaxed),
                slot.engines.load(Ordering::Relaxed),
                slot.priority.load(Ordering::Relaxed),
                slot.context.load(Ordering::Relaxed),
                slot.scheduler_va.load(Ordering::Relaxed),
                slot.kick_ring_va.load(Ordering::Relaxed),
                slot.last_ns.load(Ordering::Relaxed)
            )?;
        }
        Ok(())
    }
}
