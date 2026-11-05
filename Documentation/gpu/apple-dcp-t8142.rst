T8142 external display handoff
=============================

The shared SoC description includes DCPEXT1, its mailbox, firmware DART,
scanout/PIODMA DART, retained PMGR islands and the four ATC display crossbars.
J704 connects source 4 to ATC0/1/3 for Type-C, or ATC2 for its onboard
DisplayPort-to-HDMI converter. The converter power controls are SMC gP0f
and gP1a, with hotplug on AP GPIO41. Its reset and maintenance interface
remain owned by firmware, as on the T602x/T604x MacBook Pro boards.
J813 connects the same source to ATC0/1 and has no fixed HDMI path.

Use the generic apple-display-crossbar and ATC PHY drivers. Each PHY needs
its LIOA bank and the loader's DP LIOA/common calibration. The fixed ATC2
PHY and crossbar depend on the acknowledged PMP report domain, whose
request bit is the soc-device record index 26.

The static controller, DARTs, mailbox, Type-C routes and retained PMGR
providers stay disabled until aura admits this boot's running firmware.
Aura must retain the firmware storage, live SID23 mappings, SID0 scanout
and SID16 PIODMA mappings, and all three streams' root/L2 pages. An idle
external pipe normally has valid, locked, translating SID0 and SID16
roots with no mapped pages. Those empty tables must be preserved and
admitted for hotplug; an empty firmware stream, unlocked or invalid root,
or failed table reservation must still refuse admission. The external
failure must not prevent the internal panel from binding.

The loader publishes firmware version/compatibility, measured video and
reference clocks, DCPEXT1's data/shared carveouts and actual high-window
IOVA reservations. It records the external handoff result under
``/chosen/aurora,dcpext1-handoff``. A disabled controller in the live tree
is a loader refusal, so loading another display or Thunderbolt module
cannot repair it.

Display audio
-------------

T8142 uses dp-audio2 -> display ADMAC channel 0 -> ISOC DART SID30,
not the older boards' SIO channel 0x68. The shared description preserves
this graph for dcp-dp-audio. It stays disabled: the current AV audio
service does not support firmware 27, and the display ADMAC handoff is
not implemented. Setting the hdmi_audio module parameter does not supply
either missing contract. Video admission must not depend on audio.

Regression checks
-----------------

Run the compiled-tree topology check after a display/DT rework::

  python3 tools/asahi/t8142-display-dt-tests.py --output /tmp/t8142-display-dt

It also runs in the normal Aurora host-test workflow. It checks both
boards, controller and DMA suppliers, retained power chains, reciprocal
connector graphs, PHY banks, source selection and the separate audio
chain. Aura's ``tests/python/test_t8142_pcie.py`` covers admission with
empty external scanout/PIODMA tables and refusal of empty firmware.

Hardware qualification requires a fresh boot with both loader and DT
changes: test boot-connected and late-connected HDMI, each Type-C port
in both cable orientations, unplug/replug, and switching between HDMI
and Type-C. This DCPEXT1 description provides one external pipeline;
simultaneous independent external displays need another controller.
Check for current modes/EDID and completed flips, no DART faults, and no
crossbar/PHY timeouts. DP audio requires its separate firmware/DMA work
before playback can be qualified.
