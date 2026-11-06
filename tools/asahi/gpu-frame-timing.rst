GPU desktop frame timing
=======================

``gpu-frame-timing.py`` records the existing command lifecycle events and the
optional ``asahi_g17_command`` / ``asahi_g17_timing`` events. It writes only a
new private tracefs instance and the requested output files. It never changes
GPU policy, display settings or debugfs controls. Run capture as root on the
J704; analysis also runs on another machine with Python 3::

    python3 gpu-frame-timing.py capture /tmp/telegram-scroll.trace --seconds 15
    python3 gpu-frame-timing.py analyze /tmp/telegram-scroll.trace --active > scroll.json

Arrange continuous scrolling for the whole capture with the desktop user before
starting it. Do not interpret idle intervals as missed frames. Keep the same
window geometry, scene, blur configuration, panel mode and competing applications
between runs. Three runs of the same action are preferable to a long mixed run.

The collector drains trace_pipe continuously, records the per-CPU loss counters
in ``.trace.stats.json``, then removes its instance on normal exit, Ctrl-C or
SIGTERM. It refuses to reuse another instance or overwrite a trace. An uncatchable
process kill or machine failure can leave an instance: remove only the exact
``gpu120-PID`` instance owned by that capture, never another session's tracing.
The analyzer automatically loads the sibling statistics file. Reject a comparison
with nonzero overruns or dropped events, or materially different missing matches.

Joining the records
-------------------

Join by ``(engine, ctx, seq)``; render and compute can have equal sequence numbers.
Physical QIDs and ordinals can be reused. The publication ordinal in kernels
before this diagnostic change is one ahead of the retired render ordinal.
Replayed/duplicate identities are excluded rather than silently double counted.

The command event describes already validated data at publication: submit ioctl
ID, retained physical-queue depth, target width/height and sample count, compute
control-stream length, and the requested timestamp-output mask. Mask bits 0/1
are TA or CDM start/end, and bits 2/3 fragment start/end. A timestamp request is
evidence of timestamp output, not proof that all compute work is query-only.
The submit ID groups commands from one ioctl, not necessarily one displayed
frame. Metadata contains no command addresses or client data.

The timing event copies the four timestamps the retirement path already read
and validated. It performs no additional firmware-memory or register reads.
For render these are TA start/end and fragment start/end; for compute CDM
start/end followed by zeros. Retained depth includes the retiring command and
can include work not yet runnable. The events are disabled by default; metadata
inspection and timestamp copying are behind their static keys.

Metrics and limits
------------------

* ``enqueue_to_publish_ms`` measures the host submission path, including waits.
* ``publish_to_retire_ms`` ends at physical retirement, before deferred fence
  signalling. ``publish_to_fence_ms`` and ``retire_to_fence_ms`` are available
  if the matching dma_fence_signaled event is available.
* GPU, tiler and fragment intervals are firmware timestamp envelopes. They can
  include setup, scheduling, waits and preemption. They are not shader busy time.
  A negative TA-end to fragment-start gap preserves real stage overlap.
* ``submit_ioctls`` groups fully matched observed commands by context/submit ID.
  Capture edges or loss can leave an incomplete group; counts are observed
  commands, not an assertion that the entire ioctl was captured.
* DCP completions and submit-to-complete latencies are kept per DCP instance;
  multiple monitors must not be mixed into one frame-rate figure. A pass rate
  or submit rate must not be reported as displayed fps.
* ``--active`` reports the fraction of swap intervals over 1.5 refresh periods
  (120 Hz by default). This counts long intervals, not a precise number of lost
  frames, and is meaningful only for continuous animation.

The collector also works without the G17 lifecycle events (for example on the
upstream experimental m5 backend), collecting DCP and generic DRM/fence events.
It then reports presentation timing and leaves unavailable GPU timing empty.
The new command/timing events are optional on the existing kernel.

For a single DCP and a unique Hyprland context, ``compositor_windows`` associates
jobs retiring between successive DCP submissions. This is explicitly a heuristic:
there is no common GPU/DCP frame ID. Windows with missing publications, replay,
invalid timestamps or commands crossing their start are excluded. Its envelope
spans all associated compositor render/compute work including gaps/overlap;
it is not the sum of individual pass durations or shader utilization. Multiple
DCP outputs disable this association. ``--compositor-context`` can supply the
context if task names do not identify it.

The timing event records both clock frequencies. Existing lifecycle events
sample CNTVCT; firmware command timestamps use the driver's 24 MHz domain.
Do not subtract clocks solely because their units are known. Cross-clock
launch/retirement estimates require explicitly validated common epochs::

    python3 gpu-frame-timing.py analyze trace --common-clock-epoch \
        --host-counter-hz 1000000000

The host frequency argument is needed only for old traces without timing events.
The new timing event supplies it. ``cross_clock_violations`` counts jobs with
an estimated negative delay below -10 us and must be investigated. Default
analysis avoids cross-clock subtraction entirely. Stage durations and host
publish-to-retire measurements do not require an epoch assumption.

Choosing a change
-----------------

Measure the affected scrolling workload first. If enqueue-to-publish dominates,
inspect scheduling/dependencies and queue pressure. If GPU-end to fence dominates
with qualified clocks, inspect stamp observation and retirement. If the compositor
GPU envelope itself exceeds 8.333 ms, reducing host polling cannot recover that
budget: inspect stage geometry, dependencies and Mesa command generation. Keep
retirement stamps, work-state ownership and cache-maintenance witnesses intact.
Do not change TA forcing or remove compute jobs based on job counts alone.
