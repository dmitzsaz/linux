// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! The HAL300 startup graph, built by the driver instead of loaded from a captured image.
//!
//! The layout, native constants, explicit host policy, relocations and the lab compute pool come
//! from the generated tables in `hal300_graph`; the machine's inputs come from the device tree:
//! the SGX register base (its `sgx` resource), the GPU operating points with their core and SRAM
//! voltages, the power sample period, and the power coefficients the native constructor derives
//! from the ADT and the calibration words (`apple,hal300-derived`, one record of
//! `<object offset width value-lo value-hi>` for every entry of `hal300_graph::DERIVED`).

use super::hal300_graph as graph;
use kernel::{c_str, device, prelude::*};

/// One GPU performance state of the operating-point table.
#[derive(Clone, Copy)]
struct PState {
    hz: u64,
    core_mv: u32,
    sram_mv: u32,
}

/// The device-tree inputs of the startup graph.
pub(crate) struct Inputs {
    sgx_base: u64,
    states: KVec<PState>,
    sample_period_ms: u32,
    /// (object, offset, width, value), exactly the entries of `graph::DERIVED`.
    derived: KVec<(u8, u32, u8, u64)>,
}

impl Inputs {
    /// Reads the inputs, or returns `None` when the device tree carries no builder inputs
    /// (`apple,hal300-derived` absent): the driver then needs the captured graph image.
    pub(crate) fn load(dev: &device::Device, sgx_base: u64) -> Result<Option<Self>> {
        let node = dev.of_node().ok_or(EIO)?;
        let raw: Option<KVec<u32>> = node.get_opt_property(c_str!("apple,hal300-derived"))?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        if raw.len() != graph::DERIVED.len() * 5 {
            dev_err!(dev, "HAL300: apple,hal300-derived has {} cells, expected {}\n",
                     raw.len(), graph::DERIVED.len() * 5);
            return Err(EINVAL);
        }
        let mut derived = KVec::with_capacity(graph::DERIVED.len(), GFP_KERNEL)?;
        for (cells, &(object, offset, width)) in raw.chunks_exact(5).zip(graph::DERIVED.iter()) {
            let value = u64::from(cells[3]) | u64::from(cells[4]) << 32;
            if cells[0] != u32::from(object) || cells[1] != offset || cells[2] != u32::from(width)
                || (width < 8 && value >> (8 * width) != 0)
            {
                dev_err!(dev, "HAL300: apple,hal300-derived record {}:{:#x} does not match\n",
                         cells[0], cells[1]);
                return Err(EINVAL);
            }
            derived.push((object, offset, width, value), GFP_KERNEL)?;
        }

        let opps = node.parse_phandle(c_str!("operating-points-v2"), 0).ok_or(EINVAL)?;
        let mut states = KVec::new();
        for opp in opps.children() {
            let hz: u64 = opp.get_property(c_str!("opp-hz"))?;
            let core_uv: u32 = opp.get_property(c_str!("opp-microvolt"))?;
            let sram_uv: u32 = opp.get_property(c_str!("apple,sram-microvolt"))?;
            states.push(
                PState {
                    hz,
                    core_mv: core_uv / 1000,
                    sram_mv: sram_uv / 1000,
                },
                GFP_KERNEL,
            )?;
        }
        // An overlay-applied OPP table lists its children in reverse; the performance states are
        // the operating points by ascending frequency, state 0 (0 Hz) first.
        states.sort_unstable_by_key(|state| state.hz);
        if states.is_empty() || states.len() > graph::MAX_PSTATES {
            dev_err!(dev, "HAL300: {} GPU operating points\n", states.len());
            return Err(EINVAL);
        }
        let sample_period_ms: u32 = node.get_property(c_str!("apple,power-sample-period"))?;
        Ok(Some(Inputs {
            sgx_base,
            states,
            sample_period_ms,
            derived,
        }))
    }
}

fn put(bytes: &mut [u8], offset: usize, width: usize, value: u64) -> Result {
    let field = bytes.get_mut(offset..offset + width).ok_or(EINVAL)?;
    field.copy_from_slice(&value.to_le_bytes()[..width]);
    Ok(())
}

/// Writes object `index` of the graph into `bytes`, which must be zeroed and exactly the
/// object's size.
pub(crate) fn fill(index: usize, bytes: &mut [u8], inputs: &Inputs) -> Result {
    let spec = graph::OBJECTS.get(index).ok_or(EINVAL)?;
    if bytes.len() != spec.size {
        return Err(EINVAL);
    }
    let object = index as u8;
    for &(_, offset, length, at) in graph::LITERALS.iter().filter(|l| l.0 == object) {
        let data = graph::LITERAL_DATA
            .get(at as usize..(at + length) as usize)
            .ok_or(EINVAL)?;
        bytes
            .get_mut(offset as usize..(offset + length) as usize)
            .ok_or(EINVAL)?
            .copy_from_slice(data);
    }
    for &(_, offset, width, target, kind, slot, target_offset) in
        graph::RELOCS.iter().filter(|r| r.0 == object)
    {
        let base = match kind {
            graph::RELOC_HIGH => graph::OBJECTS.get(target as usize).ok_or(EINVAL)?.va,
            graph::RELOC_LOW => graph::OBJECTS.get(target as usize).ok_or(EINVAL)?.aliases[0],
            graph::RELOC_WINDOW => {
                graph::WINDOWS
                    .iter()
                    .find(|w| w.slot == u32::from(slot))
                    .ok_or(EINVAL)?
                    .va
            }
            _ => return Err(EINVAL),
        };
        if base == 0 {
            return Err(EINVAL);
        }
        put(bytes, offset as usize, width as usize, base + target_offset)?;
    }
    for &(_, offset, delta) in graph::SGX_FIELDS.iter().filter(|f| f.0 == object) {
        put(bytes, offset as usize, 8, inputs.sgx_base + delta)?;
    }
    if index == graph::POOL_OBJECT {
        let first = graph::POOL_LOW + graph::POOL_TABLE;
        for block in 0..graph::POOL_BLOCKS {
            let block_va = first + block as u64 * graph::POOL_BLOCK;
            for page in 0..graph::POOL_PAGES {
                put(bytes, (block * graph::POOL_PAGES + page) * 8, 8,
                    block_va + page as u64 * 0x1000)?;
            }
            put(bytes, graph::POOL_RUNS + block * 8, 8,
                block_va | (graph::POOL_PAGES as u64) << 52)?;
        }
    }
    if index == graph::POOL_DESCRIPTOR.0 {
        let pages = (graph::POOL_BLOCKS * graph::POOL_PAGES) as u64;
        let row = graph::POOL_DESCRIPTOR.1;
        put(bytes, row, 8, (0x40000u64 << 41) | (graph::POOL_LOW >> 7))?;
        put(bytes, row + 8, 8, pages << 33)?;
        put(bytes, row + 16, 8, pages)?;
        put(bytes, row + 24, 8, 0)?;
    }
    if index == graph::POWER_OBJECT {
        put(bytes, graph::PSTATE_MAX, 4, inputs.states.len() as u64 - 1)?;
        for (s, state) in inputs.states.iter().enumerate() {
            put(bytes, graph::PSTATE_MHZ + 4 * s, 4, state.hz / 1_000_000)?;
            put(bytes, graph::PSTATE_CORE_MV + graph::PSTATE_STRIDE * s, 4, state.core_mv.into())?;
            put(bytes, graph::PSTATE_SRAM_MV + graph::PSTATE_STRIDE * s, 4, state.sram_mv.into())?;
        }
        for offset in graph::SAMPLE_PERIOD {
            put(bytes, offset, 4, u64::from(inputs.sample_period_ms) * 24000)?;
        }
    }
    for &(_, offset, width, value) in inputs.derived.iter().filter(|d| d.0 == object) {
        put(bytes, offset as usize, width as usize, value)?;
    }
    Ok(())
}
