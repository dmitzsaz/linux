// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Device-owned firmware references whose native release is not yet understood.
//! Reserve space and charge backing before allocation; closing a client never allocates.

use super::{context::FirmwarePages, freelist::RenderPool};
use kernel::{
    prelude::*,
    sync::{Arc, Mutex},
};

const ENTRIES: usize = 8192;
const BYTE_LIMIT: usize = 1024 * 1024 * 1024;

#[allow(dead_code)] // Ownership is the payload: firmware may still follow either reference.
enum Entry {
    Reserved,
    Pages(FirmwarePages),
    Pool(Arc<RenderPool>),
}
struct State {
    entries: KVec<Option<Entry>>,
    bytes: usize,
}
#[pin_data]
pub(super) struct Quarantine {
    #[pin]
    state: Mutex<State>,
}
pub(super) struct Reservation {
    owner: Arc<Quarantine>,
    index: usize,
    bytes: usize,
}
impl Quarantine {
    pub(super) fn write(&self, f: &mut kernel::fmt::Formatter<'_>) -> kernel::fmt::Result {
        let state = self.state.lock();
        let used = state.entries.iter().filter(|entry| entry.is_some()).count();
        writeln!(
            f,
            "hal300_quarantine entries={}/{} backing_bytes={}/{}",
            used, ENTRIES, state.bytes, BYTE_LIMIT
        )
    }

    pub(super) fn new() -> Result<Arc<Self>> {
        let mut entries = KVec::with_capacity(ENTRIES, GFP_KERNEL)?;
        for _ in 0..ENTRIES {
            entries.push(None, GFP_KERNEL)?;
        }
        Arc::pin_init(
            pin_init!(Self {
                state <- kernel::new_mutex!(
                    State { entries, bytes: 0 },
                    "HAL300 retained DMA owners"
                )
            }),
            GFP_KERNEL,
        )
    }
    pub(super) fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Reservation> {
        let mut state = self.state.lock();
        let total = state.bytes.checked_add(bytes).ok_or(ENOSPC)?;
        if total > BYTE_LIMIT {
            return Err(ENOSPC);
        }
        let index = state
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(ENOSPC)?;
        state.entries[index] = Some(Entry::Reserved);
        state.bytes = total;
        Ok(Reservation {
            owner: self.clone(),
            index,
            bytes,
        })
    }
}
impl Reservation {
    pub(super) fn pages(&self, pages: FirmwarePages) {
        let mut state = self.owner.state.lock();
        if matches!(state.entries[self.index], Some(Entry::Reserved)) {
            state.entries[self.index] = Some(Entry::Pages(pages));
        }
    }
    pub(super) fn pool(&self, pool: Arc<RenderPool>) {
        self.owner.state.lock().entries[self.index] = Some(Entry::Pool(pool));
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.owner.state.lock();
        if matches!(state.entries[self.index], Some(Entry::Reserved)) {
            state.entries[self.index] = None;
            state.bytes -= self.bytes;
        }
    }
}
