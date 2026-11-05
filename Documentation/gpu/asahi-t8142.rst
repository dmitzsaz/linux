.. SPDX-License-Identifier: GPL-2.0

T8142 HAL300 firmware handoff
=============================

The T8142 compatible describes shared hardware, including both J704 and J813.
The driver currently admits the ten-core 26A434 firmware pair by measured TEXT
identity. Other firmware builds and binned topologies fail admission before
coprocessor start. A firmware version string alone does not select a layout.

The hardware node has the same two MMIO resources, two mailbox roles and six
reserved regions as the J813 node. Their order and names remain ``ttbs``,
``pagetables``, ``handoff``, ``shared-l2``, ``gfx-data`` and ``gfx1-data``.
The loader additionally appends ``fw-text`` and ``fw1-text`` containing the
loaded TEXT segments, including their actual relocated DATA pointers. All
firmware-owned allocations must be reserved and unavailable to the linear map.
No source device tree contains firmware addresses or calibration values.

Both firmware roots use the published shared kernel window. Application roots
alias that window, as required by this firmware pair. The private empty upper
application root of a different firmware build is not interchangeable with this
handoff. Existing shared-L2 entries are verified and never cleared by the host.

The loader supplies fourteen available OPPs in performance-state order,
including state zero, through ``operating-points-v2``. ``opp-hz`` is an exact
MHz multiple; core and SRAM voltages are exact mV multiples in microvolts.
A second ``opp-microvolt`` cell carries SRAM voltage; the existing
``apple,sram-microvolt`` property is also accepted. Disabled OPPs are ignored.
The maximum state/frequency in calibration must agree with these OPPs.
``apple,power-sample-period`` is in milliseconds and must match calibration.

Calibration version 1
---------------------

``apple,t8142-gpu-calibration`` is a 512-byte little-endian record. It contains
owner-supplied power coefficients, including fields whose firmware meanings are
unknown. Unknown field names identify their destination within the corresponding
firmware structure; the record does not contain destination addresses or write
instructions. The format is fixed for version 1. A changed field order, width
or interpretation requires a new version and corresponding loader support.
Integer and IEEE floating-point bit-pattern fields retain their original widths.
The header is version 1, chip ID 0x8142, core mask 0x3ff and sample period in ms.
The following table specifies every field's byte position within this property.
It contains no device-specific values.

.. list-table:: Calibration record
   :header-rows: 1
   :widths: 12 12 76

   * - Byte
     - Width
     - Field
   * - 0x000
     - 4
     - ``version``
   * - 0x004
     - 4
     - ``chip_id``
   * - 0x008
     - 4
     - ``core_mask``
   * - 0x00c
     - 4
     - ``sample_period_ms``
   * - 0x010
     - 4
     - ``hardwaredata_unk_0ff4``
   * - 0x014
     - 4
     - ``maximum_pstate``
   * - 0x018
     - 4
     - ``hardwaredata_unk_19e8``
   * - 0x01c
     - 4
     - ``hardwaredata_unk_19ec``
   * - 0x020
     - 4
     - ``hardwaredata_unk_19f0``
   * - 0x024
     - 4
     - ``hardwaredata_unk_19f4``
   * - 0x028
     - 4
     - ``hardwaredata_unk_19f8``
   * - 0x02c
     - 4
     - ``hardwaredata_unk_19fc``
   * - 0x030
     - 4
     - ``hardwaredata_unk_1a00``
   * - 0x034
     - 4
     - ``hardwaredata_unk_1a04``
   * - 0x038
     - 4
     - ``hardwaredata_unk_1a08``
   * - 0x03c
     - 4
     - ``hardwaredata_unk_1a0c``
   * - 0x040
     - 4
     - ``hardwaredata_unk_1a10``
   * - 0x044
     - 4
     - ``hardwaredata_unk_1a14``
   * - 0x048
     - 4
     - ``hardwaredata_unk_1a18``
   * - 0x04c
     - 8
     - ``hardwaredata_unk_1a28``
   * - 0x054
     - 8
     - ``hardwaredata_unk_1a30``
   * - 0x05c
     - 8
     - ``hardwaredata_unk_1a38``
   * - 0x064
     - 8
     - ``hardwaredata_unk_1a40``
   * - 0x06c
     - 8
     - ``hardwaredata_unk_1a48``
   * - 0x074
     - 8
     - ``hardwaredata_unk_1a50``
   * - 0x07c
     - 4
     - ``hardwaredata_unk_53c0``
   * - 0x080
     - 4
     - ``hardwaredata_unk_53c8``
   * - 0x084
     - 4
     - ``hardwaredata_unk_5400``
   * - 0x088
     - 4
     - ``hardwaredata_unk_5408``
   * - 0x08c
     - 8
     - ``hardwaredata_unk_5d6c``
   * - 0x094
     - 4
     - ``hardwaredata_unk_5d7c``
   * - 0x098
     - 8
     - ``hardwaredata_unk_5d80``
   * - 0x0a0
     - 4
     - ``hardwaredata_unk_5d88``
   * - 0x0a4
     - 4
     - ``hardwaredata_unk_5d94``
   * - 0x0a8
     - 4
     - ``hardwaredata_unk_5dc8``
   * - 0x0ac
     - 4
     - ``hardwaredata_unk_5e00``
   * - 0x0b0
     - 4
     - ``hardwaredata_unk_5e0c``
   * - 0x0b4
     - 4
     - ``hardwaredata_unk_5e14``
   * - 0x0b8
     - 4
     - ``hardwaredata_unk_5e1c``
   * - 0x0bc
     - 8
     - ``hardwaredata_unk_5e38``
   * - 0x0c4
     - 4
     - ``hardwaredata_unk_5e4c``
   * - 0x0c8
     - 8
     - ``hardwaredata_unk_5e60``
   * - 0x0d0
     - 8
     - ``hardwaredata_unk_5ec0``
   * - 0x0d8
     - 8
     - ``hardwaredata_unk_5ec8``
   * - 0x0e0
     - 4
     - ``hardwaredata_unk_5f04``
   * - 0x0e4
     - 8
     - ``hardwaredata_unk_5f08``
   * - 0x0ec
     - 4
     - ``hardwaredata_unk_5f10``
   * - 0x0f0
     - 8
     - ``hardwaredata_unk_5f50``
   * - 0x0f8
     - 4
     - ``effort_normalization``
   * - 0x0fc
     - 4
     - ``effort_floor_min``
   * - 0x100
     - 4
     - ``effort_floor_max``
   * - 0x104
     - 4
     - ``effort_max_frequency_mhz``
   * - 0x108
     - 4
     - ``hardwaredata_unk_7648``
   * - 0x10c
     - 4
     - ``hardwaredata_unk_7654``
   * - 0x110
     - 8
     - ``hardwaredata_unk_76e8``
   * - 0x118
     - 4
     - ``hardwaredata_unk_76f0``
   * - 0x11c
     - 4
     - ``hardwaredata_unk_7730``
   * - 0x120
     - 4
     - ``hardwaredata_unk_7b92``
   * - 0x124
     - 4
     - ``hardwaredata_unk_7bd6``
   * - 0x128
     - 4
     - ``hardwaredata_unk_7ca8``
   * - 0x12c
     - 4
     - ``hardwaredata_unk_7e18``
   * - 0x130
     - 4
     - ``hardwaredata_unk_7e20``
   * - 0x134
     - 4
     - ``hardwaredata_unk_7e24``
   * - 0x138
     - 4
     - ``hardwaredata_unk_7e28``
   * - 0x13c
     - 4
     - ``hardwaredata_unk_7e40``
   * - 0x140
     - 8
     - ``hardwaredata_unk_7e44``
   * - 0x148
     - 4
     - ``hardwaredata_unk_7f14``
   * - 0x14c
     - 4
     - ``hardwaredata_unk_7f20``
   * - 0x150
     - 4
     - ``hardwaredata_unk_7f28``
   * - 0x154
     - 4
     - ``hardwaredata_unk_7f30``
   * - 0x158
     - 8
     - ``hardwaredata_unk_7f4c``
   * - 0x160
     - 4
     - ``hardwaredata_unk_7f54``
   * - 0x164
     - 4
     - ``hardwaredata_unk_7f5c``
   * - 0x168
     - 4
     - ``hardwaredata_unk_7f60``
   * - 0x16c
     - 4
     - ``hardwaredata_unk_7f6c``
   * - 0x170
     - 8
     - ``hardwaredata_unk_7f74``
   * - 0x178
     - 4
     - ``hardwaredata_unk_7f94``
   * - 0x17c
     - 4
     - ``hardwaredata_unk_80c0``
   * - 0x180
     - 4
     - ``hardwaredata_unk_80c4``
   * - 0x184
     - 8
     - ``hardwaredata_unk_80dc``
   * - 0x18c
     - 4
     - ``hardwaredata_unk_80e4``
   * - 0x190
     - 4
     - ``hardwaredata_unk_80ec``
   * - 0x194
     - 4
     - ``hardwaredata_unk_80fc``
   * - 0x198
     - 4
     - ``hardwaredata_unk_8100``
   * - 0x19c
     - 8
     - ``hardwaredata_unk_8104``
   * - 0x1a4
     - 8
     - ``hardwaredata_unk_810c``
   * - 0x1ac
     - 4
     - ``hardwaredata_unk_8140``
   * - 0x1b0
     - 4
     - ``hardwaredata_unk_8144``
   * - 0x1b4
     - 4
     - ``hardwaredata_unk_819c``
   * - 0x1b8
     - 4
     - ``hardwaredata_unk_8334``
   * - 0x1bc
     - 4
     - ``hardwaredata_unk_8338``
   * - 0x1c0
     - 4
     - ``hardwaredata_unk_839c``
   * - 0x1c4
     - 4
     - ``hardwaredata_unk_83a0``
   * - 0x1c8
     - 4
     - ``hardwaredata_unk_83a4``
   * - 0x1cc
     - 4
     - ``hardwaredata_unk_83b0``
   * - 0x1d0
     - 4
     - ``hardwaredata_unk_83e4``
   * - 0x1d4
     - 4
     - ``hardwaredata_unk_8430``
   * - 0x1d8
     - 4
     - ``hardwaredata_unk_bfec``
   * - 0x1dc
     - 4
     - ``hardwaredata_unk_c02c``
   * - 0x1e0
     - 4
     - ``hardwaredata_unk_c06c``
   * - 0x1e4
     - 4
     - ``privatedata_unk_ea90``
   * - 0x1e8
     - 4
     - ``privatedata_unk_ea94``
   * - 0x1ec
     - 4
     - ``privatedata_unk_ebac``
   * - 0x1f0
     - 4
     - ``privatedata_unk_ebb4``
   * - 0x1f4
     - 4
     - ``privatedata_unk_f0a8``
   * - 0x1f8
     - 4
     - ``privatedata_unk_f0b8``
   * - 0x1fc
     - 4
     - ``regionc_unk_0088``

Ownership and stopping
----------------------

HAL300 scheduler release and processor stop are not yet implemented from a
verified native contract. Closed, drained clients return their finite execution
identities. Scheduler/work pages and native render pools remain in one device
quarantine. Reservations precede allocation and are bounded by 8192 entries and
1 GiB of backing. Failed unpublished allocations return their reservations;
retained entries last until reboot. Drained pools and queue registrations are
reused across closed owners. The capacity snapshot reports entries and charged
backing bytes. Reaching either limit rejects additional allocations.

Unbind and device loss close host admission, join publishers and fail every
owned command fence with EIO. They preserve the single firmware session, its
DMA mappings and its module reference until reboot. Rebinding after CPU start
is refused. Failed admission before CPU start rolls back reserved-table owners.
There is no verified hibernation or kexec handover; neither is supported while
this firmware session exists. The existing idle and s2idle firmware behavior
is preserved and requires hardware requalification after driver changes.

Only medium priority is admitted. Matrix, ray-traversal, SCS and render-resource
extensions remain unadvertised. Userspace timestamp-only commands must execute
a real carrier dispatch: the driver reports a measured raw/record interval only
after the normal post-maintenance completion witness.
