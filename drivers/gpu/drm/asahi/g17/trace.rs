// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Command lifecycle tracepoints (tracefs `events/asahi_g17`). Each call is a
//! static-key test in C while its event is disabled.

use kernel::{bindings, dma_fence::RawDmaFence, sync::Arc};

unsafe extern "C" {
    fn asahi_g17_trace_publish_enabled() -> bool;
    fn asahi_g17_trace_publish(engine: u8, qid: u8, ordinal: u32, ctx: u32, seq: u64,
        fence: *mut bindings::dma_fence);
    fn asahi_g17_trace_complete(engine: u8, qid: u8, ordinal: u32, ctx: u32, seq: u64,
        gpu_start: u64, gpu_end: u64);
    fn asahi_g17_trace_command_enabled() -> bool;
    fn asahi_g17_trace_command(engine: u8, qid: u8, ctx: u32, seq: u64, submit: u64,
        depth: u32, width: u32, height: u32, samples: u32, stream_bytes: u64,
        timestamps: u8);
    fn asahi_g17_trace_timing(engine: u8, qid: u8, ctx: u32, seq: u64, depth: u32,
        gpu: *const u64, gpu_hz: u64);
    fn asahi_g17_trace_bind(engine: u8, qid: u8, ctx: u32, kind: u8);
    fn asahi_g17_trace_message(kind: u8, value: u64);
    fn asahi_g17_trace_doorbell_enabled() -> bool;
    fn asahi_g17_trace_doorbell(engine: u8, power: u32);
    fn asahi_g17_trace_event(phase: u8);
    fn asahi_g17_trace_teardown(kind: u8, a: u64, b: u64, c: u64);
}

pub(super) const RENDER: u8 = 0;
pub(super) const COMPUTE: u8 = 1;

/// Called once the command's doorbell has been rung.
pub(super) fn publish(engine: u8, qid: u8, ordinal: u32, packet: &Arc<super::job::Packet>,
    depth: usize)
{
    command(engine, qid, packet, depth);
    // SAFETY: Pure static-key query.
    if !unsafe { asahi_g17_trace_publish_enabled() } {
        return;
    }
    let fence = packet.completion.fence();
    // SAFETY: `fence` holds a reference for the duration of the call.
    unsafe {
        asahi_g17_trace_publish(engine, qid, ordinal, packet.context.id(), packet.order.sequence,
            fence.raw())
    };
}

/// Called when exact completion with GPU timestamps is retired.
pub(super) fn complete(engine: u8, qid: u8, ordinal: u32, packet: &super::job::Packet,
    gpu: [u64; 4], depth: usize)
{
    // SAFETY: Plain values only.
    unsafe {
        asahi_g17_trace_complete(engine, qid, ordinal, packet.context.id(), packet.order.sequence,
            gpu[0], gpu[if engine == RENDER { 3 } else { 1 }]);
        // The four initialized timestamps live until C synchronously copies them.
        // Queue depth is bounded by the render/compute admission limit.
        asahi_g17_trace_timing(engine, qid, packet.context.id(), packet.order.sequence,
            depth as u32, gpu.as_ptr(), crate::hw::t8140::resources::COMMAND_TIMESTAMP_HZ)
    };
}

fn command(engine: u8, qid: u8, packet: &super::job::Packet, depth: usize) {
    // SAFETY: Pure static-key query. Do not inspect command metadata when disabled.
    if !unsafe { asahi_g17_trace_command_enabled() } {
        return;
    }
    let outputs = |ts: &kernel::uapi::drm_asahi_timestamps| {
        u8::from(ts.start.handle != 0) | (u8::from(ts.end.handle != 0) << 1)
    };
    let (width, height, samples, stream_bytes, timestamps) = match &packet.command {
        super::command::Validated::Render { pass, timestamps, .. } => (
            pass.width, pass.height, 1u32 << pass.samples_log2, 0,
            outputs(&timestamps[0]) | (outputs(&timestamps[1]) << 2),
        ),
        super::command::Validated::Compute { cdm_va, cdm_end_va, timestamps, .. } => (
            0, 0, 0, cdm_end_va.saturating_sub(*cdm_va), outputs(timestamps),
        ),
    };
    // SAFETY: Bounded metadata and scalar values only, with no client pointers.
    unsafe {
        asahi_g17_trace_command(engine, qid, packet.context.id(), packet.order.sequence,
            packet.id, depth as u32, width, height, samples, stream_bytes, timestamps)
    };
}

/// A physical queue was bound to a logical owner: 0 fresh, 1 rebound, 2 same VM.
pub(super) fn bind(engine: u8, qid: u8, ctx: u32, kind: u8) {
    // SAFETY: Plain values only.
    unsafe { asahi_g17_trace_bind(engine, qid, ctx, kind) };
}

/// A work doorbell is about to ring; `power` samples the core power state
/// only while the event is enabled.
pub(super) fn doorbell(engine: u8, power: impl FnOnce() -> u32) {
    // SAFETY: Pure static-key query.
    if !unsafe { asahi_g17_trace_doorbell_enabled() } {
        return;
    }
    // SAFETY: Plain values only.
    unsafe { asahi_g17_trace_doorbell(engine, power()) };
}

pub(super) const MAILBOX: u8 = 0;
pub(super) const CONTROL: u8 = 1;
pub(super) const EFFORT: u8 = 2;

/// A host-to-firmware message: a mailbox message, a device-control record by
/// opcode, or a new performance effort value.
pub(super) fn message(kind: u8, value: u64) {
    // SAFETY: Plain values only.
    unsafe { asahi_g17_trace_message(kind, value) };
}

/// Failed-context teardown: 1 compute fault (qid, stamp), 2 unstamped render retire (slot,
/// ordinal), 3 kill published (cookie, context, held slots | 1 if it skipped the wait for other
/// contexts' compute under memory pressure; slot 0 is never held), 4 kill acknowledged (cookie,
/// qid, or 0x100 for a render-only kill), 5 detach (context, slots, linked), 6 render slot to a
/// new owner (slot, old, new | killed << 63), 7 render-only kill published (as 3).
pub(crate) fn teardown(kind: u8, a: u64, b: u64, c: u64) {
    // SAFETY: Plain values only.
    unsafe { asahi_g17_trace_teardown(kind, a, b, c) };
}

/// Event worker pass boundaries: 0 begin, 1 fences signalled, 2 end.
pub(super) fn event(phase: u8) {
    // SAFETY: Plain values only.
    unsafe { asahi_g17_trace_event(phase) };
}

unsafe extern "C" {
    fn asahi_g17_trace_report_enabled() -> bool;
    fn asahi_g17_trace_report(role: u8, ring: u8, words: *const u32);
}

/// Copies a complete firmware record while the host still owns its slot.
/// Static-key gated; no MMIO reads or firmware requests are issued.
pub(super) fn report(role: u8, ring: u8, words: &[u32; super::fw::channels::EVENT_WORDS]) {
    // SAFETY: Pure static-key query.
    if !unsafe { asahi_g17_trace_report_enabled() } {
        return;
    }
    // SAFETY: C synchronously copies the complete initialized 18-word array.
    unsafe { asahi_g17_trace_report(role, ring, words.as_ptr()) };
}


unsafe extern "C" {
    fn asahi_g17_trace_inputs(ctx: u32, seq: u64, phase: u8, inputs: u32,
        sources: u32, waits: u32, converted: bool);
    fn asahi_g17_trace_schedule(engine: u8, ctx: u32, seq: u64, phase: u8, detail: i32);
}

/// Input conversion boundaries. No fence status is polled for tracing.
pub(super) fn inputs(ctx: u32, seq: u64, phase: u8, inputs: usize,
    sources: Option<&super::sync_input::Sources>)
{
    let (count, waits) = sources.map_or((0, 0),
        |s| (s.sources.len(), s.publication_waits().len()));
    // SAFETY: Bounded UAPI counts and plain values only. C reads the timer only
    // behind this event's static key; no GPU registers or pointers are exposed.
    unsafe { asahi_g17_trace_inputs(ctx, seq, phase, inputs as u32,
        count as u32, waits as u32, sources.is_some()) };
}

/// Host scheduling boundaries, joined to publish/complete by engine/context/sequence.
pub(super) fn schedule(packet: &super::job::Packet, phase: u8, detail: i32) {
    let engine = match packet.command {
        super::command::Validated::Render { .. } => RENDER,
        super::command::Validated::Compute { .. } => COMPUTE,
    };
    // SAFETY: Plain values only; counter sampling is static-key gated in C.
    unsafe { asahi_g17_trace_schedule(engine, packet.context.id(),
        packet.order.sequence, phase, detail) };
}
