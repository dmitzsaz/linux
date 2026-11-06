/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/*
 * Command lifecycle tracepoints of the G17 runtime. Times are joined by
 * (ctx, seq); `counter` is the CPU's virtual counter and `gpu_start`/`gpu_end`
 * come from the firmware's completion record, so a reader can place firmware
 * dispatch and completion on the host timeline.
 */
#undef TRACE_SYSTEM
#define TRACE_SYSTEM asahi_g17

#if !defined(ASAHI_G17_TRACE_H) || defined(TRACE_HEADER_MULTI_READ)
#define ASAHI_G17_TRACE_H

#include <linux/tracepoint.h>

TRACE_EVENT(asahi_g17_publish,
	TP_PROTO(u8 engine, u8 qid, u32 ordinal, u32 ctx, u64 seq,
		 u64 fence_context, u64 fence_seqno, u64 counter),
	TP_ARGS(engine, qid, ordinal, ctx, seq, fence_context, fence_seqno, counter),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u8, qid)
		__field(u32, ordinal)
		__field(u32, ctx)
		__field(u64, seq)
		__field(u64, fence_context)
		__field(u64, fence_seqno)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->qid = qid;
		__entry->ordinal = ordinal;
		__entry->ctx = ctx;
		__entry->seq = seq;
		__entry->fence_context = fence_context;
		__entry->fence_seqno = fence_seqno;
		__entry->counter = counter;
	),
	TP_printk("engine=%u qid=%u ordinal=%u ctx=%u seq=%llu fence=%llu:%llu counter=%llu",
		  __entry->engine, __entry->qid, __entry->ordinal, __entry->ctx,
		  __entry->seq, __entry->fence_context, __entry->fence_seqno,
		  __entry->counter)
);

TRACE_EVENT(asahi_g17_complete,
	TP_PROTO(u8 engine, u8 qid, u32 ordinal, u32 ctx, u64 seq,
		 u64 gpu_start, u64 gpu_end, u64 counter),
	TP_ARGS(engine, qid, ordinal, ctx, seq, gpu_start, gpu_end, counter),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u8, qid)
		__field(u32, ordinal)
		__field(u32, ctx)
		__field(u64, seq)
		__field(u64, gpu_start)
		__field(u64, gpu_end)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->qid = qid;
		__entry->ordinal = ordinal;
		__entry->ctx = ctx;
		__entry->seq = seq;
		__entry->gpu_start = gpu_start;
		__entry->gpu_end = gpu_end;
		__entry->counter = counter;
	),
	TP_printk("engine=%u qid=%u ordinal=%u ctx=%u seq=%llu gpu=%llu..%llu counter=%llu",
		  __entry->engine, __entry->qid, __entry->ordinal, __entry->ctx,
		  __entry->seq, __entry->gpu_start, __entry->gpu_end, __entry->counter)
);

/* Metadata of a published command. submit groups commands from one ioctl;
 * depth counts retained commands on this physical queue, including this one.
 * timestamps is a bitmask of requested TA/CDM start/end and fragment start/end
 * outputs. No GPU addresses or client memory are recorded.
 */
TRACE_EVENT(asahi_g17_command,
	TP_PROTO(u8 engine, u8 qid, u32 ctx, u64 seq, u64 submit, u32 depth,
		 u32 width, u32 height, u32 samples, u64 stream_bytes, u8 timestamps),
	TP_ARGS(engine, qid, ctx, seq, submit, depth, width, height, samples,
		stream_bytes, timestamps),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u8, qid)
		__field(u32, ctx)
		__field(u64, seq)
		__field(u64, submit)
		__field(u32, depth)
		__field(u32, width)
		__field(u32, height)
		__field(u32, samples)
		__field(u64, stream_bytes)
		__field(u8, timestamps)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->qid = qid;
		__entry->ctx = ctx;
		__entry->seq = seq;
		__entry->submit = submit;
		__entry->depth = depth;
		__entry->width = width;
		__entry->height = height;
		__entry->samples = samples;
		__entry->stream_bytes = stream_bytes;
		__entry->timestamps = timestamps;
	),
	TP_printk("engine=%u qid=%u ctx=%u seq=%llu submit=%llu depth=%u size=%ux%u samples=%u stream_bytes=%llu timestamps=%#x",
		  __entry->engine, __entry->qid, __entry->ctx, __entry->seq,
		  __entry->submit, __entry->depth, __entry->width, __entry->height,
		  __entry->samples, __entry->stream_bytes, __entry->timestamps)
);

/* The four already-validated firmware timestamps at physical retirement.
 * Render: TA start/end, fragment start/end. Compute: CDM start/end, 0, 0.
 * Envelopes include firmware scheduling and must not be called shader busy time.
 * counter_hz describes the CPU counter; gpu_hz describes the firmware timestamps.
 */
TRACE_EVENT(asahi_g17_timing,
	TP_PROTO(u8 engine, u8 qid, u32 ctx, u64 seq, u32 depth, const u64 *gpu,
		 u64 gpu_hz, u64 counter, u64 counter_hz),
	TP_ARGS(engine, qid, ctx, seq, depth, gpu, gpu_hz, counter, counter_hz),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u8, qid)
		__field(u32, ctx)
		__field(u64, seq)
		__field(u32, depth)
		__array(u64, gpu, 4)
		__field(u64, gpu_hz)
		__field(u64, counter)
		__field(u64, counter_hz)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->qid = qid;
		__entry->ctx = ctx;
		__entry->seq = seq;
		__entry->depth = depth;
		memcpy(__entry->gpu, gpu, sizeof(__entry->gpu));
		__entry->gpu_hz = gpu_hz;
		__entry->counter = counter;
		__entry->counter_hz = counter_hz;
	),
	TP_printk("engine=%u qid=%u ctx=%u seq=%llu depth=%u ta=%llu..%llu fr=%llu..%llu gpu_hz=%llu counter=%llu counter_hz=%llu",
		  __entry->engine, __entry->qid, __entry->ctx, __entry->seq,
		  __entry->depth, __entry->gpu[0], __entry->gpu[1],
		  __entry->gpu[2], __entry->gpu[3], __entry->gpu_hz,
		  __entry->counter, __entry->counter_hz)
);

TRACE_EVENT(asahi_g17_bind,
	TP_PROTO(u8 engine, u8 qid, u32 ctx, u8 kind),
	TP_ARGS(engine, qid, ctx, kind),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u8, qid)
		__field(u32, ctx)
		__field(u8, kind)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->qid = qid;
		__entry->ctx = ctx;
		__entry->kind = kind;
	),
	TP_printk("engine=%u qid=%u ctx=%u kind=%u",
		  __entry->engine, __entry->qid, __entry->ctx, __entry->kind)
);

TRACE_EVENT(asahi_g17_doorbell,
	TP_PROTO(u8 engine, u32 power, u64 counter),
	TP_ARGS(engine, power, counter),
	TP_STRUCT__entry(
		__field(u8, engine)
		__field(u32, power)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->engine = engine;
		__entry->power = power;
		__entry->counter = counter;
	),
	TP_printk("engine=%u power=%#x counter=%llu", __entry->engine, __entry->power,
		  __entry->counter)
);

TRACE_EVENT(asahi_g17_message,
	TP_PROTO(u8 kind, u64 value, u64 counter),
	TP_ARGS(kind, value, counter),
	TP_STRUCT__entry(
		__field(u8, kind)
		__field(u64, value)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->kind = kind;
		__entry->value = value;
		__entry->counter = counter;
	),
	TP_printk("kind=%u value=%#llx counter=%llu", __entry->kind, __entry->value,
		  __entry->counter)
);

/* Failed-context teardown and render slot reuse (kind: see g17/trace.rs teardown()). */
TRACE_EVENT(asahi_g17_teardown,
	TP_PROTO(u8 kind, u64 a, u64 b, u64 c),
	TP_ARGS(kind, a, b, c),
	TP_STRUCT__entry(
		__field(u8, kind)
		__field(u64, a)
		__field(u64, b)
		__field(u64, c)
	),
	TP_fast_assign(
		__entry->kind = kind;
		__entry->a = a;
		__entry->b = b;
		__entry->c = c;
	),
	TP_printk("kind=%u a=%#llx b=%#llx c=%#llx", __entry->kind, __entry->a, __entry->b, __entry->c)
);

TRACE_EVENT(asahi_g17_event,
	TP_PROTO(u8 phase, u64 counter),
	TP_ARGS(phase, counter),
	TP_STRUCT__entry(
		__field(u8, phase)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->phase = phase;
		__entry->counter = counter;
	),
	TP_printk("phase=%u counter=%llu", __entry->phase, __entry->counter)
);

/* Input conversion: phase 0 entry, 1 result. converted=0 also covers no live input. */
TRACE_EVENT(asahi_g17_inputs,
    TP_PROTO(u32 ctx, u64 seq, u8 phase, u32 inputs, u32 sources, u32 waits,
	     bool converted, u64 counter),
    TP_ARGS(ctx, seq, phase, inputs, sources, waits, converted, counter),
    TP_STRUCT__entry(
	__field(u32, ctx)
	__field(u64, seq)
	__field(u8, phase)
	__field(u32, inputs)
	__field(u32, sources)
	__field(u32, waits)
	__field(bool, converted)
	__field(u64, counter)
    ),
    TP_fast_assign(
	__entry->ctx = ctx;
	__entry->seq = seq;
	__entry->phase = phase;
	__entry->inputs = inputs;
	__entry->sources = sources;
	__entry->waits = waits;
	__entry->converted = converted;
	__entry->counter = counter;
    ),
    TP_printk("ctx=%u seq=%llu phase=%u inputs=%u sources=%u waits=%u converted=%u counter=%llu",
	      __entry->ctx, __entry->seq, __entry->phase, __entry->inputs,
	      __entry->sources, __entry->waits, __entry->converted, __entry->counter)
);

/* phase: 0 enqueue, 1 inline published, 2 scheduler fallback, 3 run begin,
 * 4 run end, 5 prepare begin, 6 prepare end. detail: dependencies, error or wait. */
TRACE_EVENT(asahi_g17_schedule,
    TP_PROTO(u8 engine, u32 ctx, u64 seq, u8 phase, s32 detail, u64 counter),
    TP_ARGS(engine, ctx, seq, phase, detail, counter),
    TP_STRUCT__entry(
	__field(u8, engine)
	__field(u32, ctx)
	__field(u64, seq)
	__field(u8, phase)
	__field(s32, detail)
	__field(u64, counter)
    ),
    TP_fast_assign(
	__entry->engine = engine;
	__entry->ctx = ctx;
	__entry->seq = seq;
	__entry->phase = phase;
	__entry->detail = detail;
	__entry->counter = counter;
    ),
    TP_printk("engine=%u ctx=%u seq=%llu phase=%u detail=%d counter=%llu",
	      __entry->engine, __entry->ctx, __entry->seq, __entry->phase,
	      __entry->detail, __entry->counter)
);

/* Raw 18-word records: trace=0, statistics=1, event=2. No unit inference. */
TRACE_EVENT(asahi_g17_report,
	TP_PROTO(u8 role, u8 ring, const u32 *words, u64 counter),
	TP_ARGS(role, ring, words, counter),
	TP_STRUCT__entry(
		__field(u8, role)
		__field(u8, ring)
		__array(u32, words, 18)
		__field(u64, counter)
	),
	TP_fast_assign(
		__entry->role = role;
		__entry->ring = ring;
		memcpy(__entry->words, words, sizeof(__entry->words));
		__entry->counter = counter;
	),
	TP_printk("role=%u ring=%u words=%s counter=%llu", __entry->role,
		  __entry->ring, __print_array(__entry->words, 18, sizeof(u32)),
		  __entry->counter)
);

#endif

#undef TRACE_INCLUDE_PATH
#define TRACE_INCLUDE_PATH .
#undef TRACE_INCLUDE_FILE
#define TRACE_INCLUDE_FILE trace
#include <trace/define_trace.h>
