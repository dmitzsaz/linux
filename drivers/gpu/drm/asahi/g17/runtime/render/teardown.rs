// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Exact logical ownership gates for scheduler release and physical-pair parking.

use super::RenderState;
use crate::g17::{
    buffer::BufferIds,
    context::Context,
    initdata::InitData,
    runtime::{teardown::ReclaimBatch, Registry},
};
use kernel::{prelude::*, sync::Arc};

impl RenderState {
    pub(in crate::g17::runtime) fn observe_teardown_releases(&mut self, init: &InitData) -> Result {
        self.pools.observe(init)
    }

    pub(in crate::g17::runtime) fn teardown_idle(&self, context: &Arc<Context>) -> Result<bool> {
        // A pair that changed owner keeps running this scheduler's queues until
        // the new owner's first command leaves it, whichever context owns it now.
        let scheduler = Some(context.scheduler_va());
        if self.entries.iter().flatten().any(|entry| match entry.pair.as_ref() {
            Some(pair) => pair.switch_in_flight() == scheduler,
            None => (entry.preparing.is_some() || entry.growing) && entry.borrowed_switch == scheduler,
        }) {
            return Ok(false);
        }
        for entry in self.entries.iter().flatten() {
            if !entry
                .context
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, context))
            {
                continue;
            }
            if entry.preparing.is_some() || entry.growing {
                return Ok(false);
            }
            if let Some(pair) = entry.pair.as_ref() {
                if !pair.idle_for_context(context)? && !pair.executed_for_kill(context) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Some render pair runs another context's commands.
    pub(in crate::g17::runtime) fn foreign_in_flight(&self, context: &Arc<Context>) -> bool {
        self.entries.iter().flatten().any(|entry| {
            !entry.context.as_ref().is_some_and(|owner| Arc::ptr_eq(owner, context))
                && entry.pair.as_ref().is_some_and(|pair| pair.in_flight())
        })
    }

    pub(in crate::g17::runtime) fn teardown_pools_released(&self, context: &Arc<Context>) -> bool {
        context.render_pools_released(|id, control| self.pools.released(id, control))
            && self
                .retired
                .iter()
                .filter(|old| Arc::ptr_eq(old.context(), context))
                .all(|old| {
                    self.pools
                        .released(u32::from(old.pool().id()), old.pool().control_va())
                })
    }

    /// Warm superseded roots need only their exact executed replacement witness.
    /// Graphs with their own memory remain until that Context's scheduler ack.
    pub(in crate::g17::runtime) fn collect_reclaimable(
        &mut self,
        out: &mut ReclaimBatch,
    ) -> Result {
        for slot in 0..self.entries.len() {
            self.collect_retired(slot)?;
        }
        let mut index = 0;
        while index < self.retired.len() {
            if self.retired[index].buffer_id().is_some() {
                index += 1;
                continue;
            }
            let Ok(target) = out.binding_slot() else {
                break;
            };
            *target = Some(self.retired.remove(index).map_err(|_| EIO)?);
        }
        Ok(())
    }

    pub(in crate::g17::runtime) fn collect_closed_retired(
        &mut self,
        buffers: &mut BufferIds,
        out: &mut ReclaimBatch,
    ) -> Result {
        if !self.hal300
        {
            return Ok(());
        }
        let mut index = 0;
        while index < self.retired.len() {
            let context = self.retired[index].context();
            let scheduler = context.scheduler_va();
            let busy = context.is_current()
                || self.entries.iter().flatten().any(|entry| {
                    entry.borrowed_switch == Some(scheduler)
                        || entry
                            .context
                            .as_ref()
                            .is_some_and(|owner| Arc::ptr_eq(owner, context))
                        || entry.pair.as_ref().is_some_and(|pair| {
                            pair.executed_scheduler() == scheduler
                                || pair.switch_in_flight() == Some(scheduler)
                        })
                });
            if busy {
                index += 1;
                continue;
            }
            let Ok(target) = out.binding_slot() else {
                break;
            };
            if let Some(id) = self.retired[index].buffer_id() {
                buffers.unreserve_owner(id)?;
            }
            *target = Some(self.retired.remove(index).map_err(|_| EIO)?);
        }
        // A pool becomes spare once no pair, borrowed pair or retired binding names it.
        if self
            .entries
            .iter()
            .flatten()
            .any(|entry| entry.constructing || entry.growing || entry.preparing.is_some())
        {
            return Ok(());
        }
        let mut index = 0;
        while index < self.closed_pools.len() {
            let pool = &self.closed_pools[index];
            let named = self.entries.iter().flatten().any(|entry| {
                entry.pool.as_ref().is_some_and(|bound| Arc::ptr_eq(bound, pool))
                    || entry.pair.as_ref().is_some_and(|pair| Arc::ptr_eq(pair.pool(), pool))
            }) || self.retired.iter().any(|old| Arc::ptr_eq(old.pool(), pool));
            if named {
                index += 1;
                continue;
            }
            let pool = self.closed_pools.remove(index).map_err(|_| EIO)?;
            // On allocation failure the pool simply stays retained (Context::render_pool).
            if self.spare_pools.push(pool, GFP_KERNEL).is_err() {
                break;
            }
        }
        Ok(())
    }

    pub(in crate::g17::runtime) fn park_closed_pool(&mut self, context: &Arc<Context>) {
        if let Some(pool) = context.take_render_pool() {
            let _ = self.closed_pools.push(pool, GFP_KERNEL);
        }
    }

    pub(in crate::g17::runtime) fn adopt_spare_pool(&mut self, context: &Arc<Context>) {
        if !self.hal300
            || context.has_render_pool()
        {
            return;
        }
        let Some(index) = self
            .spare_pools
            .iter()
            .position(|pool| pool.blocks() >= context.render_blocks())
        else {
            return;
        };
        let Ok(pool) = self.spare_pools.remove(index) else {
            return;
        };
        if let Err(pool) = context.adopt_render_pool(pool) {
            let _ = self.spare_pools.push(pool, GFP_KERNEL);
        }
    }

    /// Preflight the entire Context before detaching its first graph. Successor
    /// graphs on the same VM have a different Arc and remain installed.
    pub(in crate::g17::runtime) fn detach_context(
        &mut self,
        context: &Arc<Context>,
        buffers: &mut BufferIds,
        out: &mut ReclaimBatch,
    ) -> Result {
        let mut count = self
            .retired
            .iter()
            .filter(|old| Arc::ptr_eq(old.context(), context))
            .count();
        for entry in self.entries.iter().flatten() {
            if !entry
                .context
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, context))
            {
                continue;
            }
            if entry.preparing.is_some() || entry.growing || entry.parked.is_some() {
                return Err(EBUSY);
            }
            if let Some(pair) = entry.pair.as_ref() {
                if !pair.can_park(context)? {
                    return Err(EBUSY);
                }
                count += 1;
            }
        }
        if !out.has_room(count, 1) {
            return Err(EBUSY);
        }
        let linked = context.firmware_may_link();
        for entry in self.entries.iter_mut().flatten() {
            if !entry
                .context
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, context))
            {
                continue;
            }
            let Some(pair) = entry.pair.as_ref() else {
                continue;
            };
            buffers.unreserve_owner(pair.buffer_id())?;
            let target = out.binding_slot()?;
            let pair = entry.pair.take().ok_or(EIO)?;
            let (mut parked, retired) = KBox::into_inner(pair).park();
            // This park follows the consumed release of `context`.
            parked.release_consumed(context.scheduler_va());
            if linked {
                // Guard (kill before free): the firmware may still walk this context's work
                // list into the graph, so it is not freed or recycled before the firmware stops.
                kernel::pr_warn_once!("G17: keeping render graphs of an unkilled context that still links work\n");
                keep_graph(&mut self.kept_graphs, retired);
            } else {
                *target = Some(retired);
            }
            entry.parked = Some(parked);
            // Detached backing retains these references until the worker runs.
            entry.context = None;
            entry.pool = None;
        }
        let mut index = 0;
        while index < self.retired.len() {
            if !Arc::ptr_eq(self.retired[index].context(), context) {
                index += 1;
                continue;
            }
            if let Some(id) = self.retired[index].buffer_id() {
                buffers.unreserve_owner(id)?;
            }
            let retired = self.retired.remove(index).map_err(|_| EIO)?;
            if linked {
                keep_graph(&mut self.kept_graphs, retired);
            } else {
                *out.binding_slot()? = Some(retired);
            }
        }
        Ok(())
    }
}

impl Registry {
    /// Close while a builder owns a pair records intent. Its return path applies
    /// the release before admitting further commands from either logical owner.
    pub(in crate::g17::runtime) fn release_render(&mut self, owner: u64) -> Result {
        for entry in self.render.entries.iter_mut().flatten() {
            if entry.pending_owner == Some(owner) {
                entry.pending_owner_closed = true;
            }
            if entry.reservation.owner != owner {
                continue;
            }
            entry.closed = true;
            if let Some(pair) = entry.pair.as_mut() {
                pair.release_owner();
            }
        }
        Ok(())
    }
}

/// Graphs that could not be recorded in `kept_graphs` and were leaked instead, for debugfs.
pub(super) static LEAKED_GRAPHS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Keeps `graph` until the firmware stops; if even that cannot be recorded, it is leaked
/// rather than freed.
fn keep_graph(
    kept: &mut KVec<crate::g17::queue::render::RetiredBinding>,
    graph: crate::g17::queue::render::RetiredBinding,
) {
    if kept.reserve(1, GFP_KERNEL).is_ok() {
        if let Err(error) = kept.push_within_capacity(graph) {
            LEAKED_GRAPHS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            core::mem::forget(error.0);
        }
    } else {
        LEAKED_GRAPHS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        core::mem::forget(graph);
    }
}
