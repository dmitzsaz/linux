#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Interleaved trace vectors for the production G17 timing analyzer."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('timing', Path(__file__).with_name('gpu-frame-timing.py'))
timing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(timing)


def line(t, event, fields, task='client-42', cpu=0):
    return f'{task:>20} [{cpu:03}] ..... {t:.6f}: {event}: {fields}\n'


def command(engine=0, seq=1, submit=10, qid=3):
    return [line(1, 'asahi_g17_schedule', f'engine={engine} ctx=5 seq={seq} phase=0 detail=0 counter=1000000000'),
            line(1.001, 'asahi_g17_command', f'engine={engine} qid={qid} ctx=5 seq={seq} submit={submit} depth=2 size=3024x1890 samples=1 stream_bytes=64 timestamps=0x3'),
            line(1.001, 'asahi_g17_publish', f'engine={engine} qid={qid} ordinal=999 ctx=5 seq={seq} fence=123:{seq + engine * 100} counter=1001000000'),
            line(1.004, 'asahi_g17_complete', f'engine={engine} qid={qid} ordinal=998 ctx=5 seq={seq} gpu=24048000..24084000 counter=1004000000')]


class TimingTests(unittest.TestCase):
    def test_join_includes_engine_and_ignores_physical_ordinal(self):
        result = timing.analyze(command() + command(engine=1, qid=4))
        self.assertEqual(result['groups']['5:0']['metrics']['publish_to_retire_ms']['n'], 1)
        self.assertEqual(result['groups']['5:1']['metrics']['publish_to_retire_ms']['p50'], 3)
        self.assertEqual(result['submit_ioctls']['5']['compute_commands']['p50'], 1)
        self.assertEqual(result['submit_ioctls']['5']['render_commands']['p50'], 1)

    def test_raw_stage_intervals_and_overlap(self):
        trace = command() + [line(1.004, 'asahi_g17_timing', 'engine=0 qid=3 ctx=5 seq=1 depth=2 ta=24048000..24072000 fr=24060000..24084000 gpu_hz=24000000 counter=1004000000 counter_hz=1000000000')]
        metrics = timing.analyze(trace)['groups']['5:0']['metrics']
        self.assertEqual(metrics['tiler_envelope_ms']['p50'], 1)
        self.assertEqual(metrics['fragment_envelope_ms']['p50'], 1)
        self.assertEqual(metrics['tiler_end_to_fragment_start_ms']['p50'], -.5)
        self.assertEqual(metrics['gpu_envelope_ms']['p50'], 1.5)
        self.assertNotIn('gpu_end_to_retire_ms_estimate', metrics)

    def test_cross_clock_estimates_are_explicit(self):
        with self.assertRaises(ValueError):
            timing.analyze(command(), common_epoch=True)
        result = timing.analyze(command(), host_hz=1_000_000_000, common_epoch=True)
        metrics = result['groups']['5:0']['metrics']
        self.assertEqual(metrics['publish_to_gpu_start_ms_estimate']['p50'], 1)
        self.assertEqual(metrics['gpu_end_to_retire_ms_estimate']['p50'], .5)
        self.assertEqual(result['cross_clock_violations'], 0)

    def test_clock_mismatch_is_visible(self):
        self.assertEqual(timing.analyze(command(), host_hz=24_000_000, common_epoch=True)['cross_clock_violations'], 1)

    def test_replay_is_excluded(self):
        trace = command()
        result = timing.analyze(trace + [trace[2]])
        self.assertEqual(result['replay_or_duplicate_jobs_excluded'], 1)
        self.assertNotIn('publish_to_retire_ms', result['groups']['5:0']['metrics'])

    def test_capture_edges_and_loss_are_reported(self):
        trace = command()[3:] + [line(1.005, 'asahi_g17_publish', 'engine=0 qid=3 ordinal=1 ctx=5 seq=2 fence=123:2 counter=1005000000')]
        result = timing.analyze(trace + ['overrun: 3\n', 'dropped events: 2\n'])
        self.assertEqual(result['unmatched_completions'], 1)
        self.assertEqual(result['unfinished_publications'], 1)
        self.assertEqual(result['trace_loss']['overrun'], 3)
        self.assertEqual(result['trace_loss']['dropped_events'], 2)

    def test_worker_can_migrate(self):
        trace = [line(1, 'asahi_g17_event', 'phase=0 counter=1', 'kworker-7', 0),
                 line(1.002, 'asahi_g17_event', 'phase=2 counter=2', 'kworker-7', 3)]
        self.assertEqual(timing.analyze(trace)['event_worker_ms']['p50'], 2)

    def test_presentation_is_per_dcp_and_not_pass_count(self):
        trace = command()
        for dcp in ['aaa', 'bbb']:
            for i, t in enumerate([1, 1.008333, 1.025]):
                trace += [line(t - .001, 'iomfb_swap_submit', f'dcp={dcp}, swap_id={i}'),
                          line(t, 'iomfb_swap_complete', f'dcp={dcp}, swap_id={i}')]
        result = timing.analyze(trace)
        self.assertEqual(result['presentation']['aaa']['completions'], 3)
        self.assertEqual(result['presentation']['bbb']['submit_to_complete_ms']['p50'], 1)
        self.assertNotIn('intervals_over_1_5_period_fraction', result['presentation']['aaa'])
        self.assertEqual(timing.analyze(trace, active=True)['presentation']['aaa']['intervals_over_1_5_period_fraction'], .5)

    def test_fence_signalling_is_distinct_from_retirement(self):
        trace = command() + [line(1.0042, 'dma_fence_signaled', 'driver=aurora timeline=test context=123 seqno=1')]
        metrics = timing.analyze(trace)['groups']['5:0']['metrics']
        self.assertEqual(metrics['retire_to_fence_ms']['p50'], .2)
        self.assertEqual(metrics['publish_to_fence_ms']['p50'], 3.2)

    def test_zero_gpu_timestamp_is_not_a_duration(self):
        trace = command()
        trace[-1] = trace[-1].replace('24048000..24084000', '0..0')
        result = timing.analyze(trace)
        self.assertEqual(result['invalid_gpu_intervals'], 1)
        self.assertNotIn('gpu_envelope_ms', result['groups']['5:0']['metrics'])

    def test_compositor_windows_are_explicitly_heuristic(self):
        trace = [line(.99, 'iomfb_swap_submit', 'dcp=aaa, swap_id=1')]
        trace += command() + command(engine=1)
        trace += [line(1.005, 'iomfb_swap_submit', 'dcp=aaa, swap_id=2')]
        result = timing.analyze(trace, compositor_context=5)
        windows = result['compositor_windows']
        self.assertIn('Heuristic', windows['method'])
        self.assertEqual(windows['metrics']['render_commands']['p50'], 1)
        self.assertEqual(windows['metrics']['compute_commands']['p50'], 1)
        self.assertEqual(windows['metrics']['gpu_envelope_ms']['p50'], 1.5)
        self.assertEqual(windows['metrics']['first_enqueue_to_swap_submit_ms']['p50'], 5)

    def test_multiple_outputs_are_not_assigned_to_one_compositor_frame(self):
        trace = command()
        trace += [line(1.005, 'iomfb_swap_submit', f'dcp={dcp}, swap_id=1') for dcp in ['aaa', 'bbb']]
        self.assertIsNone(timing.analyze(trace, compositor_context=5)['compositor_windows'])

    def test_presentation_only_backend_has_no_invented_gpu_timings(self):
        trace = [line(1, 'iomfb_swap_submit', 'dcp=aaa, swap_id=1'),
                 line(1.005, 'iomfb_swap_complete', 'dcp=aaa, swap_id=1')]
        result = timing.analyze(trace)
        self.assertEqual(result['groups'], {})
        self.assertIsNone(result['compositor_windows'])
        self.assertEqual(result['presentation']['aaa']['submit_to_complete_ms']['p50'], 5)


if __name__ == '__main__':
    unittest.main()
