#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Capture a private GPU/DCP trace, or summarize command and presentation timing.

A render pass, a submit ioctl and a displayed frame are distinct units. GPU
stage intervals include firmware setup/scheduling; they are not shader busy time.
"""
import argparse
import collections
from decimal import Decimal
import json
import os
from pathlib import Path
import re
import select
import signal as signals
import time

LINE = re.compile(r'^\s*(.*?)\s+\[(\d+)\]\s+\S+\s+(\d+\.\d+): (\w+): (.*)$')
FIELDS = re.compile(r'(\w+)=([^ ,]+)')
EVENTS = ['asahi_g17/' + e for e in (
    'asahi_g17_schedule', 'asahi_g17_inputs', 'asahi_g17_publish',
    'asahi_g17_complete', 'asahi_g17_command', 'asahi_g17_timing',
    'asahi_g17_bind', 'asahi_g17_event')]
EVENTS += ['dcp/iomfb_swap_submit', 'dcp/iomfb_swap_complete',
           'dma_fence/dma_fence_signaled', 'drm/drm_vblank_event',
           'gpu_scheduler/drm_sched_job_queue', 'gpu_scheduler/drm_sched_job_run',
           'gpu_scheduler/drm_sched_job_done']


def distribution(values):
    values = sorted(values)
    if not values:
        return None
    def at(q):
        pos = (len(values) - 1) * q
        lo = int(pos)
        hi = min(lo + 1, len(values) - 1)
        return round(values[lo] + (values[hi] - values[lo]) * (pos - lo), 6)
    return dict(n=len(values), min=at(0), p50=at(.5), p90=at(.9), p99=at(.99), max=at(1))


def parse(lines):
    for line in lines:
        m = LINE.match(line)
        if m:
            yield dict(task=m[1], cpu=int(m[2]), ns=int(Decimal(m[3]) * 10**9),
                       event=m[4], fields=dict(FIELDS.findall(m[5])))


def pair(text):
    return tuple(map(int, text.split('..')))


def analyze(lines, gpu_hz=24_000_000, host_hz=None, common_epoch=False,
            refresh_hz=120, active=False, compositor_context=None):
    jobs = collections.defaultdict(dict)
    events = collections.Counter()
    swaps = collections.defaultdict(dict)
    done = collections.defaultdict(list)
    submitted = collections.defaultdict(list)
    swap_latencies = collections.defaultdict(list)
    signal = {}
    workers = {}
    worker_times = []
    ambiguous = set()
    first = last = None
    loss = dict(overrun=0, dropped_events=0, counters_available=False)
    def checked_lines():
        for line in lines:
            m = re.match(r'^(overrun|dropped events): (\d+)', line)
            if m:
                loss['counters_available'] = True
                loss['overrun' if m[1] == 'overrun' else 'dropped_events'] += int(m[2])
            yield line
    for e in parse(checked_lines()):
        d, event, ns = e['fields'], e['event'], e['ns']
        events[event] += 1
        first = ns if first is None else min(first, ns)
        last = ns if last is None else max(last, ns)
        if all(k in d for k in ('engine', 'ctx', 'seq')):
            key = tuple(int(d[k]) for k in ('engine', 'ctx', 'seq'))
            j = jobs[key]
            if event == 'asahi_g17_schedule' and d['phase'] == '0':
                j['enqueue'] = ns
                j['owner'] = e['task']
            if event == 'asahi_g17_publish':
                if 'publish' in j:
                    ambiguous.add(key)
                j['publish'], j['pub_fields'] = ns, d
            if event == 'asahi_g17_command':
                j['command'] = d
            if event == 'asahi_g17_complete':
                if 'complete' in j:
                    ambiguous.add(key)
                j['complete'], j['complete_fields'] = ns, d
            if event == 'asahi_g17_timing':
                j['timing'] = d
        if event == 'dma_fence_signaled' and 'context' in d and 'seqno' in d:
            signal[(d['context'], d['seqno'])] = ns
        if event == 'asahi_g17_event':
            # A worker can migrate. Match by task/PID rather than CPU.
            if d['phase'] == '0':
                workers[e['task']] = ns
            elif d['phase'] == '2' and e['task'] in workers:
                worker_times.append((ns - workers.pop(e['task'])) / 1e6)
        if event.startswith('iomfb_swap_'):
            dcp, sid = d['dcp'], d['swap_id']
            if event == 'iomfb_swap_submit':
                swaps[dcp][sid] = ns
                submitted[dcp].append(ns)
            else:
                done[dcp].append(ns)
                if sid in swaps[dcp]:
                    swap_latencies[dcp].append((ns - swaps[dcp].pop(sid)) / 1e6)
    groups = collections.defaultdict(lambda: collections.defaultdict(list))
    owners = collections.defaultdict(collections.Counter)
    metadata = collections.defaultdict(collections.Counter)
    ioctls = collections.defaultdict(list)
    missing_pub = missing_complete = invalid_gpu = unmatched_signal = 0
    clock_violations = 0
    for key, j in jobs.items():
        engine, ctx, seq = key
        group = f'{ctx}:{engine}'
        g = groups[group]
        if 'owner' in j:
            owners[group][j['owner']] += 1
        if 'command' in j:
            cmd = j['command']
            g['publish_retained_depth'].append(int(cmd['depth']))
            detail = ('size=' + cmd['size'] + ' samples=' + cmd['samples'] if engine == 0
                      else 'stream_bytes=' + cmd['stream_bytes'])
            metadata[group][detail + ' timestamps=' + cmd['timestamps']] += 1
        missing_pub += 'complete' in j and 'publish' not in j
        missing_complete += 'publish' in j and 'complete' not in j
        if key in ambiguous or 'publish' not in j or 'complete' not in j:
            continue
        g['publish_to_retire_ms'].append((j['complete'] - j['publish']) / 1e6)
        if 'enqueue' in j:
            g['enqueue_to_publish_ms'].append((j['publish'] - j['enqueue']) / 1e6)
        fence = j['pub_fields']['fence'].split(':')
        signalled = signal.get(tuple(fence))
        if signalled is not None:
            g['publish_to_fence_ms'].append((signalled - j['publish']) / 1e6)
            g['retire_to_fence_ms'].append((signalled - j['complete']) / 1e6)
        else:
            unmatched_signal += 1
        timing = j.get('timing')
        hz = int(timing['gpu_hz']) if timing else gpu_hz
        if timing:
            ta, fr = pair(timing['ta']), pair(timing['fr'])
            start, end = ta[0], fr[1] if engine == 0 else ta[1]
            g['retire_retained_depth'].append(int(timing['depth']))
        else:
            start, end = pair(j['complete_fields']['gpu'])
        if start == 0 or end < start:
            invalid_gpu += 1
            continue
        g['gpu_envelope_ms'].append((end - start) * 1000 / hz)
        if timing and engine == 0:
            if ta[1] >= ta[0] and fr[1] >= fr[0] and ta[1] and fr[0]:
                g['tiler_envelope_ms'].append((ta[1] - ta[0]) * 1000 / hz)
                g['fragment_envelope_ms'].append((fr[1] - fr[0]) * 1000 / hz)
                # Preserve overlap as a negative gap.
                g['tiler_end_to_fragment_start_ms'].append((fr[0] - ta[1]) * 1000 / hz)
        if common_epoch:
            chz = int(timing['counter_hz']) if timing else host_hz
            if not chz:
                raise ValueError('old trace needs --host-counter-hz for cross-clock estimates')
            pc = int(j['pub_fields']['counter'])
            cc = int((timing or j['complete_fields'])['counter'])
            launch = (start / hz - pc / chz) * 1000
            retire = (cc / chz - end / hz) * 1000
            if launch < -.01 or retire < -.01:
                clock_violations += 1
            g['publish_to_gpu_start_ms_estimate'].append(launch)
            g['gpu_end_to_retire_ms_estimate'].append(retire)
        if 'command' in j:
            ioctls[(ctx, int(j['command']['submit']))].append((engine, start / hz, end / hz))
    ioctl_groups = collections.defaultdict(lambda: collections.defaultdict(list))
    for (ctx, _), samples in ioctls.items():
        dest = ioctl_groups[str(ctx)]
        dest['render_commands'].append(sum(e == 0 for e, _, _ in samples))
        dest['compute_commands'].append(sum(e == 1 for e, _, _ in samples))
        dest['gpu_envelope_ms'].append((max(b for _, _, b in samples) -
                                        min(a for _, a, _ in samples)) * 1000)
    presentation = {}
    for dcp, times in done.items():
        times.sort()
        intervals = [(b - a) / 1e6 for a, b in zip(times, times[1:])]
        p = dict(completions=len(times), interval_ms=distribution(intervals),
                 submit_to_complete_ms=distribution(swap_latencies[dcp]),
                 observed_rate_hz=(len(times) - 1) * 1e9 / (times[-1] - times[0])
                 if len(times) > 1 and times[-1] > times[0] else None)
        if active and intervals:
            p['intervals_over_1_5_period_fraction'] = sum(
                v > 1500 / refresh_hz for v in intervals) / len(intervals)
        presentation[dcp] = p
    # This is deliberately labelled a heuristic. DCP and GPU UAPIs do not
    # provide a common frame ID. Multiple outputs cannot be attributed safely.
    windows = None
    if compositor_context is None:
        candidates = {key[1] for key, job in jobs.items()
                      if 'Hyprland' in job.get('owner', '')}
        if len(candidates) == 1:
            compositor_context = candidates.pop()
    if compositor_context is not None and len(submitted) == 1:
        metrics = collections.defaultdict(list)
        rejected = 0
        # Count every retirement in the window, then reject a window if any
        # observed command lacks a matching publication or crossed its start.
        ordered = sorted((job['complete'], key, job) for key, job in jobs.items()
                         if key[1] == compositor_context and 'complete' in job)
        cursor = 0
        times = sorted(next(iter(submitted.values())))
        for left, right in zip(times, times[1:]):
            selected = []
            while cursor < len(ordered) and ordered[cursor][0] <= right:
                end_ns, key, job = ordered[cursor]
                cursor += 1
                if end_ns > left:
                    selected.append((key, job))
            if not selected:
                continue
            if any(key in ambiguous or 'publish' not in job or 'enqueue' not in job
                   or job['enqueue'] < left or job['publish'] < left
                   for key, job in selected):
                rejected += 1
                continue
            intervals = [pair(job['complete_fields']['gpu']) for _, job in selected]
            if any(a == 0 or b < a for a, b in intervals):
                rejected += 1
                continue
            metrics['render_commands'].append(sum(key[0] == 0 for key, _ in selected))
            metrics['compute_commands'].append(sum(key[0] == 1 for key, _ in selected))
            metrics['gpu_envelope_ms'].append((max(b for _, b in intervals) -
                                               min(a for a, _ in intervals)) * 1000 / gpu_hz)
            metrics['first_enqueue_to_swap_submit_ms'].append(
                (right - min(job['enqueue'] for _, job in selected)) / 1e6)
            metrics['last_retire_to_swap_submit_ms'].append(
                (right - max(job['complete'] for _, job in selected)) / 1e6)
        windows = dict(context=compositor_context,
                       method='Heuristic: compositor jobs retiring between consecutive DCP submits; no shared frame ID.',
                       rejected_windows=rejected,
                       metrics={k: distribution(v) for k, v in metrics.items()})
    return dict(format=1, duration_s=(last - first) / 1e9 if first is not None else 0,
                warning='Passes, ioctls and DCP swaps are distinct; stage envelopes include scheduling.',
                cross_clock_estimates=common_epoch, cross_clock_violations=clock_violations,
                events=dict(events), replay_or_duplicate_jobs_excluded=len(ambiguous),
                trace_loss=loss,
                unmatched_completions=missing_pub, unfinished_publications=missing_complete,
                invalid_gpu_intervals=invalid_gpu, unmatched_fence_signals=unmatched_signal,
                groups={k: dict(metrics={m: distribution(v) for m, v in g.items()},
                                owners=dict(owners[k]), metadata=dict(metadata[k])) for k, g in groups.items()},
                submit_ioctls={k: {m: distribution(v) for m, v in g.items()}
                              for k, g in ioctl_groups.items()},
                event_worker_ms=distribution(worker_times), presentation=presentation,
                compositor_windows=windows)


def capture(destination, seconds, tracefs):
    if not 0 < seconds <= 120:
        raise ValueError('--seconds must be in (0,120]')
    instance = tracefs / 'instances' / f'gpu120-{os.getpid()}'
    instance.mkdir()  # Exclusive: never reuse another collector's instance.
    stats = dict(enabled=[], unavailable=[])
    try:
        (instance / 'tracing_on').write_text('0')
        (instance / 'buffer_size_kb').write_text('2048')
        (instance / 'trace_clock').write_text('mono')
        for event in EVENTS:
            path = instance / 'events' / event / 'enable'
            if path.exists():
                path.write_text('1')
                stats['enabled'].append(event)
            else:
                stats['unavailable'].append(event)
        required = ['dcp/iomfb_swap_submit', 'dcp/iomfb_swap_complete']
        if any(e not in stats['enabled'] for e in required):
            raise ValueError('required DCP events are unavailable')
        # Refuse an overwrite before starting collection, and drain continuously.
        with destination.open('xb') as output:
            fd = os.open(instance / 'trace_pipe', os.O_RDONLY | os.O_NONBLOCK)
            try:
                (instance / 'tracing_on').write_text('1')
                deadline = time.monotonic() + seconds
                while time.monotonic() < deadline:
                    ready, _, _ = select.select([fd], [], [], min(.2, max(0, deadline - time.monotonic())))
                    if ready:
                        try:
                            output.write(os.read(fd, 1 << 20))
                        except BlockingIOError:
                            pass
                (instance / 'tracing_on').write_text('0')
                while True:
                    try:
                        data = os.read(fd, 1 << 20)
                    except BlockingIOError:
                        break
                    if not data:
                        break
                    output.write(data)
            finally:
                os.close(fd)
        stats['per_cpu'] = {p.parent.name: p.read_text()
                            for p in (instance / 'per_cpu').glob('cpu*/stats')}
        stats['boot_id'] = Path('/proc/sys/kernel/random/boot_id').read_text().strip()
        stats['kernelrelease'] = os.uname().release
        stats['seconds_requested'] = seconds
        destination.with_suffix(destination.suffix + '.stats.json').write_text(json.dumps(stats, indent=2) + '\n')
    finally:
        (instance / 'tracing_on').write_text('0')
        instance.rmdir()
    return stats


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest='command', required=True)
    cap = sub.add_parser('capture', help='run as root on the J704; only a private tracefs instance is changed')
    cap.add_argument('trace', type=Path)
    cap.add_argument('--seconds', type=float, default=15)
    cap.add_argument('--tracefs', type=Path, default=Path('/sys/kernel/tracing'))
    ana = sub.add_parser('analyze')
    ana.add_argument('trace', type=Path)
    ana.add_argument('--gpu-hz', type=int, default=24_000_000)
    ana.add_argument('--host-counter-hz', type=int)
    ana.add_argument('--common-clock-epoch', action='store_true', help='opt in only after CPU/GPU clock epoch validation')
    ana.add_argument('--active', action='store_true', help='capture was continuous animation; report long swap intervals')
    ana.add_argument('--refresh-hz', type=float, default=120)
    ana.add_argument('--compositor-context', type=int, help='otherwise infer a unique Hyprland context')
    args = ap.parse_args()
    if args.command == 'capture':
        def terminate(_signum, _frame):
            raise SystemExit('capture interrupted; removing private trace instance')
        signals.signal(signals.SIGTERM, terminate)
        result = capture(args.trace, args.seconds, args.tracefs)
    else:
        if args.gpu_hz <= 0 or args.refresh_hz <= 0 or (args.host_counter_hz is not None and args.host_counter_hz <= 0):
            ap.error('frequencies must be positive')
        with args.trace.open() as source:
            result = analyze(source, args.gpu_hz, args.host_counter_hz,
                             args.common_clock_epoch, args.refresh_hz, args.active,
                             args.compositor_context)
        stats_path = args.trace.with_suffix(args.trace.suffix + '.stats.json')
        if stats_path.exists():
            result['capture'] = json.loads(stats_path.read_text())
            for stats in result['capture'].get('per_cpu', {}).values():
                for name, value in re.findall(r'^(overrun|dropped events): (\d+)', stats, re.M):
                    result['trace_loss']['counters_available'] = True
                    result['trace_loss']['overrun' if name == 'overrun' else 'dropped_events'] += int(value)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
