// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! The last HAL300 render publications, always recorded, read through debugfs `asahi-g17/pubs`.
//!
//! The per-pass printk trace (debug_flags bit 62) changes the timing it is meant to observe, and
//! a rare stalled pass (asahi-pw48-boot) never reproduced with it on. This ring keeps the same
//! addresses for the newest publications at the cost of a few relaxed stores, so a stall can be
//! inspected (debugfs `peek`) after the fact with nothing enabled beforehand. Each line has the
//! format of the trace line, so the existing capture tools parse either.

use core::sync::atomic::{AtomicU64, Ordering};

use kernel::prelude::*;

const RING: usize = 32;
/// seq, time, ordinal, qids, kicks[2], descriptors[2], clients[2], kick entries[2], queues[2],
/// pb_state, free_list, scheduler
const FIELDS: usize = 18;

static SLOTS: [[AtomicU64; FIELDS]; RING] = [const { [const { AtomicU64::new(0) }; FIELDS] }; RING];
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Records one render publication (both halves of the pair).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    ordinal: u64,
    qids: [u8; 2],
    kicks: [u64; 2],
    descriptors: [u64; 2],
    clients: [u64; 2],
    kick_entries: [u64; 2],
    queues: [u64; 2],
    pb_state: u64,
    free_list: u64,
    scheduler: u64,
) {
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let slot = &SLOTS[(seq as usize) % RING];
    // Invalidate while the fields change; a reader skipping a torn slot loses one record.
    slot[0].store(0, Ordering::Release);
    let values = [
        super::now_ns(),
        ordinal,
        u64::from(qids[0]) | u64::from(qids[1]) << 8,
        kicks[0],
        kicks[1],
        descriptors[0],
        descriptors[1],
        clients[0],
        clients[1],
        kick_entries[0],
        kick_entries[1],
        queues[0],
        queues[1],
        pb_state,
        free_list,
        scheduler,
        0,
    ];
    for (field, value) in slot[1..].iter().zip(values) {
        field.store(value, Ordering::Relaxed);
    }
    slot[0].store(seq + 1, Ordering::Release);
}

/// debugfs view: the recorded publications, oldest first.
pub(crate) struct View;

impl kernel::debugfs::Writer for View {
    fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        let next = NEXT.load(Ordering::Acquire);
        writeln!(f, "version=1 sampled_ns={} publications={}", super::now_ns(), next)?;
        let first = next.saturating_sub(RING as u64);
        for seq in first..next {
            let slot = &SLOTS[(seq as usize) % RING];
            if slot[0].load(Ordering::Acquire) != seq + 1 {
                continue;
            }
            let v: [u64; FIELDS] = core::array::from_fn(|i| slot[i].load(Ordering::Relaxed));
            if slot[0].load(Ordering::Acquire) != seq + 1 {
                continue;
            }
            writeln!(
                f,
                "t={} asahi: HAL300 render publication ordinal {} qids {}/{} kicks {:#x}/{:#x} \
                 descriptors {:#x}/{:#x} client {:#x}/{:#x} kick_entries {:#x}/{:#x} \
                 queues {:#x}/{:#x} pb_state {:#x} free_list {:#x} scheduler {:#x}",
                v[1],
                v[2],
                v[3] & 0xff,
                (v[3] >> 8) & 0xff,
                v[4],
                v[5],
                v[6],
                v[7],
                v[8],
                v[9],
                v[10],
                v[11],
                v[12],
                v[13],
                v[14],
                v[15],
                v[16]
            )?;
        }
        Ok(())
    }
}
