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
                if !pair.idle_for_context(context)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
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

    /// HAL300 with queue reuse (queue_reuse_enabled): a pair that changed to a foreign owner
    /// took new queue memory, and the displaced memory waited for a scheduler release that
    /// HAL300 never publishes, so every closed GL client left one pair graph, parameter buffer
    /// and metrics slot behind (49 retired bindings after 50 clients, asahi-reuse16-boot).
    /// Return it once its owner's execution context was released (closed and idle, see
    /// teardown::take_closed_idle) and no pair still last ran, or is switching away from, that
    /// owner's scheduler: by then the firmware's rows for those QIDs name the new owner's
    /// queues. The render pool stays retained (Context::render_pool).
    pub(in crate::g17::runtime) fn collect_closed_retired(
        &mut self,
        buffers: &mut BufferIds,
        out: &mut ReclaimBatch,
    ) -> Result {
        if !crate::g17::fw::compute::g17g_selected()
            || !crate::g17::runtime::queue_reuse_enabled()
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

    /// HAL300 pool reuse: keep a consumed closed context's render pool for the next owner.
    /// Every render pool is retained until reboot on HAL300 (Context::render_pool), so a
    /// failed push only loses the reuse, never the memory the firmware may name.
    pub(in crate::g17::runtime) fn park_closed_pool(&mut self, context: &Arc<Context>) {
        if let Some(pool) = context.take_render_pool() {
            let _ = self.closed_pools.push(pool, GFP_KERNEL);
        }
    }

    /// HAL300 pool reuse: a context about to build its first render pair takes a spare pool
    /// (same drained registration, same backing aliased into its VM) instead of a new one.
    pub(in crate::g17::runtime) fn adopt_spare_pool(&mut self, context: &Arc<Context>) {
        if !crate::g17::fw::compute::g17g_selected()
            || !crate::g17::runtime::queue_reuse_enabled()
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
            *target = Some(retired);
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
            let target = out.binding_slot()?;
            *target = Some(self.retired.remove(index).map_err(|_| EIO)?);
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
            // HAL300: this pool's owner is gone, so its registration may publish its final
            // free-list release once the pair drains (native AGXUSCPrivMemPool::complete).
            // One boot that published it ended in an event-service timeout right after the
            // render pool's 0x2e, so it stays opt-in (debug_flags bit 59) until the KSM
            // registration question in sol-ksm-queue-descriptor-study.md is settled.
            if crate::g17::fw::compute::g17g_selected()
                && crate::module_parameters::debug_flags.value() & (1 << 59) != 0
            {
                if let Some(id) = entry.pool.as_ref().map(|pool| pool.id()) {
                    self.render.pools.mark_final(id);
                }
            }
        }
        Ok(())
    }
}
