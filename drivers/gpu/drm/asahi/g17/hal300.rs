// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! T8142 HAL300 input admission, initialization and retained session ownership.

use super::{
    fw::initdata::hal300 as abi,
    initdata::{Hal300Object, InitData},
    object::{CpuMap, KernelObject},
    Firmware, Role,
};
use crate::{driver::AsahiDrmDevice, hw::t8142 as hw, mmu};
use core::ops::{Deref, DerefMut, Range};
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use kernel::{
    device,
    prelude::*,
    sync::Arc,
    time::{delay::fsleep, Delta},
};

pub(super) fn region(dev: &device::Device, name: &CStr) -> Result<Range<u64>> {
    let node = dev.of_node().ok_or(EINVAL)?;
    let res = node.reserved_mem_region_to_resource_byname(name)?;
    let end = res.start().checked_add(res.size()).ok_or(EINVAL)?;
    if res.size() == 0
        || res.start() & mmu::UAT_PGMSK as u64 != 0
        || res.size() & mmu::UAT_PGMSK as u64 != 0
    {
        return Err(EINVAL);
    }
    Ok(res.start()..end)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct PState {
    pub(crate) mhz: u32,
    pub(crate) core_mv: u32,
    pub(crate) sram_mv: u32,
}
pub(crate) struct Inputs {
    pub(crate) sgx_base: u64,
    pub(crate) states: KVec<PState>,
    pub(crate) calibration: abi::Calibration,
    pub(crate) sample_period_ms: u32,
}
impl Inputs {
    pub(crate) fn state(&self, index: usize) -> PState {
        self.states.get(index).copied().unwrap_or_default()
    }
    pub(super) fn load(dev: &AsahiDrmDevice, sgx_base: u64, cores: u32) -> Result<Self> {
        let node = dev.as_ref().as_ref().of_node().ok_or(EINVAL)?;
        let bytes: KVec<u8> = node.get_property(c"apple,t8142-gpu-calibration")?;
        if bytes.len() != size_of::<abi::Calibration>() {
            return Err(EINVAL);
        }
        let calibration = crate::util::Reader::new(&bytes).read::<abi::Calibration>()?;
        let period: u32 = node.get_property(c"apple,power-sample-period")?;
        if calibration.version != 1
            || calibration.chip_id != 0x8142
            || calibration.core_mask != 0x3ff
            || cores != 10
            || period == 0
            || period > 10000
            || calibration.sample_period_ms != period
        {
            return Err(EINVAL);
        }
        let opps = node
            .parse_phandle(c"operating-points-v2", 0)
            .ok_or(EINVAL)?;
        let mut states = KVec::new();
        for opp in opps.children() {
            let status: Option<KVec<u8>> = opp.get_opt_property(c"status")?;
            if status.is_some_and(|status| {
                status.as_slice() != b"okay\0" && status.as_slice() != b"ok\0"
            }) {
                continue;
            }
            let hz: u64 = opp.get_property(c"opp-hz")?;
            let voltages: KVec<u32> = opp.get_property(c"opp-microvolt")?;
            if voltages.is_empty() || voltages.len() > 2 {
                return Err(EINVAL);
            }
            let sram = match opp.get_opt_property::<u32>(c"apple,sram-microvolt")? {
                Some(value) => value,
                None => *voltages.get(1).ok_or(EINVAL)?,
            };
            let core = voltages[0];
            if hz % 1_000_000 != 0
                || hz > 4_000_000_000
                || core % 1000 != 0
                || sram % 1000 != 0
                || (hz != 0 && (core < 300000 || sram < 300000))
                || core > 2000000
                || sram > 2000000
            {
                return Err(EINVAL);
            }
            states.push(
                PState {
                    mhz: (hz / 1_000_000) as u32,
                    core_mv: core / 1000,
                    sram_mv: sram / 1000,
                },
                GFP_KERNEL,
            )?;
        }
        states.sort_unstable_by_key(|state| state.mhz);
        if states.len() != 14
            || states[0].mhz != 0
            || states.windows(2).any(|p| p[0].mhz >= p[1].mhz)
            || calibration.maximum_pstate != states.len() as u32 - 1
            || calibration.effort_max_frequency_mhz != states.last().ok_or(EINVAL)?.mhz
        {
            return Err(EINVAL);
        }
        Ok(Self {
            sgx_base,
            states,
            calibration,
            sample_period_ms: period,
        })
    }
}

pub(super) struct Prepared {
    objects: KVec<Hal300Object>,
}
impl Prepared {
    pub(super) fn new(dev: &AsahiDrmDevice, uat: &mmu::Uat, inputs: &Inputs) -> Result<Self> {
        let mut objects = KVec::with_capacity(hw::OBJECTS.len() + 3, GFP_KERNEL)?;
        for spec in &hw::OBJECTS {
            let mut object = KernelObject::backing(dev, spec.size, CpuMap::WriteCombined)?;
            object.evict_cpu_caches();
            abi::fill(spec.kind, &mut object, inputs)?;
            let mut aliases = KVec::with_capacity(1, GFP_KERNEL)?;
            for address in spec.aliases.into_iter().filter(|address| *address != 0) {
                aliases.push(
                    object.map_alias(uat.kernel_lower_vm(), address, mmu::PROT_GPU_FW_SHARED_RW)?,
                    GFP_KERNEL,
                )?;
            }
            objects.push(
                Hal300Object {
                    kind: spec.kind,
                    object,
                    aliases,
                    low: spec.aliases,
                    high: spec.va,
                    prot: mmu::PROT_GPU_FW_SHARED_RW,
                },
                GFP_KERNEL,
            )?;
        }
        for (kind, high, low, prot) in [
            (
                abi::Object::PbDescriptors,
                hw::PB_DESCRIPTORS,
                Some(hw::PB_LOW),
                mmu::PROT_FW_SHARED_RW,
            ),
            (
                abi::Object::StampA,
                hw::STAMPS[0],
                None,
                mmu::PROT_GPU_FW_SHARED_RW,
            ),
            (
                abi::Object::StampB,
                hw::STAMPS[1],
                None,
                mmu::PROT_GPU_FW_PRIV_RW,
            ),
        ] {
            let mut object = KernelObject::backing(dev, mmu::UAT_PGSZ, CpuMap::WriteCombined)?;
            object.evict_cpu_caches();
            let mut aliases = KVec::new();
            if let Some(low) = low {
                aliases.push(
                    object.map_alias(uat.kernel_lower_vm(), low, mmu::PROT_GPU_SHARED_RW)?,
                    GFP_KERNEL,
                )?;
            }
            objects.push(
                Hal300Object {
                    kind,
                    object,
                    aliases,
                    low: [low.unwrap_or(0), 0],
                    high,
                    prot,
                },
                GFP_KERNEL,
            )?;
        }
        // Named PB pointers of the primary and secondary configurations.
        let hardware = &mut objects[abi::Object::HardwareData as usize].object;
        for (base, offset, value) in [
            (0x1ee40, 0x2d8, hw::PB_LOW),
            (0x1ee40, 0x2e0, hw::PB_DESCRIPTORS),
            (0x23300, 0x2d8, hw::PB_LOW),
        ] {
            hardware.write(base + offset, value)?;
        }
        crate::mem::sync();
        Ok(Self { objects })
    }
    pub(super) fn install(
        &mut self,
        dev: &AsahiDrmDevice,
        uat: &mmu::Uat,
        metrics: &KernelObject,
        init: &mut InitData,
    ) -> Result {
        for item in &mut self.objects {
            item.object
                .map_primary_at(uat.kernel_vm(), item.high, item.prot)?;
        }
        let metrics =
            metrics.map_alias(uat.kernel_vm(), hw::METRICS, mmu::PROT_GPU_FW_SHARED_RW)?;
        for window in &hw::WINDOWS {
            let prot = if window.slot == 14 {
                mmu::PROT_FW_MMIO_RO
            } else {
                mmu::PROT_FW_MMIO_RW
            };
            init.retain_mapping(uat.kernel_vm().map_io(
                dev.as_ref().as_ref(),
                window.va,
                window.phys as usize,
                window.size as usize,
                prot,
            )?)?;
        }
        init.adopt_hal300(
            core::mem::take(&mut self.objects),
            [hw::OBJECTS[15].va, hw::OBJECTS[16].va],
            hw::METRICS,
        )?;
        init.retain_mapping(metrics)?;
        init.set_io_mapping(14, 0x380280000, 0xfffffc2200e00000, 0x8000, 0x8000, 0)?;
        crate::mem::sync();
        crate::mem::tlbi_all();
        crate::mem::sync();
        Ok(())
    }
}

const SETTLE_POLLS: usize = 400;
const POLL_MS: i64 = 10;
fn status(session: &Firmware) -> Result<[[u32; 8]; 2]> {
    Ok([
        session.init.status_read_words(Role::Primary, 0)?,
        session.init.status_read_words(Role::Secondary, 0)?,
    ])
}
fn crashed(session: &Firmware) -> bool {
    session.primary.state.crashed.load(Ordering::Acquire)
        || session
            .secondary
            .as_ref()
            .is_some_and(|cpu| cpu.state.crashed.load(Ordering::Acquire))
}

/// Bounded initialization, descent, wake and configuration-refresh transitions.
pub(super) fn finish_start(session: &mut Firmware) -> Result {
    let mut initialized = false;
    for _ in 0..SETTLE_POLLS {
        if crashed(session) {
            return Err(EIO);
        }
        let now = status(session)?;
        if now[0][1] == 1
            && now[0][5] == 1
            && now[1][1] == 1
            && now[1][5] == 1
            && session.init.opening_retired(Role::Primary)?
            && session.init.opening_retired(Role::Secondary)?
        {
            initialized = true;
            break;
        }
        fsleep(Delta::from_millis(POLL_MS));
    }
    if !initialized {
        return Err(ETIMEDOUT);
    }
    use super::fw::channels::{ControlRecord, IdlePolicy, IdlePowerOff};
    let wake = |session: &mut Firmware| -> Result<u32> {
        let target = super::channel::Rings::publish_control(
            &session.init,
            &ControlRecord::IdlePowerOff(IdlePowerOff::new(IdlePolicy::KeepPowered)),
        )?;
        session.primary.notify(super::MSG_CONTROL_NOTIFY)?;
        Ok(target)
    };
    let mut target = wake(session)?;
    let mut attempts = 1;
    let mut retired_since = None;
    let mut awake = false;
    for poll in 0..SETTLE_POLLS {
        if crashed(session) {
            return Err(EIO);
        }
        let now = status(session)?;
        let retired = session.init.control_retired(Role::Primary, target)?;
        if retired && now[0][7] != 0 && now[0][4] == 0 {
            awake = true;
            break;
        }
        if retired {
            let since = *retired_since.get_or_insert(poll);
            if poll - since >= 50 && attempts < 4 {
                target = wake(session)?;
                attempts += 1;
                retired_since = None;
            }
        }
        fsleep(Delta::from_millis(POLL_MS));
    }
    if !awake {
        return Err(ETIMEDOUT);
    }
    // Keep the producer cursors returned by ring publication, including wrap.
    let targets = [
        session.init.publish_role_control(Role::Primary, 0x15)?,
        session.init.publish_role_control(Role::Secondary, 0x2a)?,
    ];
    session.primary.notify(super::MSG_CONTROL_NOTIFY)?;
    session
        .secondary
        .as_ref()
        .ok_or(EINVAL)?
        .notify(super::MSG_CONTROL_NOTIFY)?;
    for _ in 0..SETTLE_POLLS {
        if crashed(session) {
            return Err(EIO);
        }
        if session.init.control_retired(Role::Primary, targets[0])?
            && session.init.control_retired(Role::Secondary, targets[1])?
        {
            return Ok(());
        }
        fsleep(Delta::from_millis(POLL_MS));
    }
    Err(ETIMEDOUT)
}

// One physical T8142 session owns this slot after DMA starts. A failed probe
// before CPU start releases its reservation; later failures keep the session.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static RETAINED: AtomicPtr<Firmware> = AtomicPtr::new(core::ptr::null_mut());
extern "C" {
    fn asahi_hal300_pin_module();
}
pub(super) fn retain(session: KBox<Firmware>) {
    let pointer = KBox::into_raw(session);
    // ACTIVE excludes a second HAL300 boot, so this slot has one producer.
    assert!(RETAINED
        .compare_exchange(
            core::ptr::null_mut(),
            pointer,
            Ordering::AcqRel,
            Ordering::Acquire
        )
        .is_ok());
}
pub(super) struct Session {
    inner: Option<KBox<Firmware>>,
    retained: bool,
    started: bool,
}
impl Session {
    pub(super) fn new(inner: KBox<Firmware>, retained: bool) -> Result<Self> {
        if retained
            && ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err(EBUSY);
        }
        Ok(Self {
            inner: Some(inner),
            retained,
            started: false,
        })
    }
    pub(super) fn starting(&mut self) {
        if self.retained && !self.started {
            // SAFETY: The current module is live; retained DMA owners pin its code until reboot.
            unsafe {
                asahi_hal300_pin_module();
            }
        }
        self.started = true;
    }
    pub(super) fn into_inner(mut self) -> KBox<Firmware> {
        self.inner.take().unwrap()
    }
}
impl Deref for Session {
    type Target = Firmware;
    fn deref(&self) -> &Firmware {
        self.inner.as_deref().unwrap()
    }
}
impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Firmware {
        self.inner.as_deref_mut().unwrap()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.retained {
            if let Some(inner) = self.inner.take() {
                if self.started {
                    retain(inner);
                } else {
                    ACTIVE.store(false, Ordering::Release);
                    drop(inner);
                }
            }
        }
    }
}
