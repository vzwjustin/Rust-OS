//! Perf (Hardware Performance Counters) subsystem
//!
//! Provides a framework for PMU (Performance Monitoring Unit) drivers
//! that expose hardware performance counters. Mirrors Linux's `drivers/perf/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PMU event type (Linux `enum perf_hw_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfHwId {
    CpuCycles,
    Instructions,
    CacheReferences,
    CacheMisses,
    BranchInstructions,
    BranchMisses,
    BusCycles,
    StalledCyclesFrontend,
    StalledCyclesBackend,
    RefCpuCycles,
}

impl PerfHwId {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// PMU event (Linux `struct perf_event`).
pub struct PerfEvent {
    pub id: u32,
    pub pmu_id: u32,
    pub event_type: PerfHwId,
    pub config: u64,
    pub counter: u64,
    pub enabled: bool,
    pub sample_period: u64,
    pub sample_count: u64,
}

/// PMU device (Linux `struct pmu`).
pub struct Pmu {
    pub id: u32,
    pub name: String,
    pub num_counters: u32,
    pub supported_events: Vec<PerfHwId>,
    pub ops: PmuOps,
    pub event_ids: Vec<u32>,
}

/// PMU operations (Linux `struct pmu` callbacks).
pub struct PmuOps {
    pub event_init: fn(pmu_id: u32, event_type: PerfHwId, config: u64) -> Result<(), &'static str>,
    pub event_add: fn(pmu_id: u32, event_id: u32) -> Result<(), &'static str>,
    pub event_del: fn(pmu_id: u32, event_id: u32) -> Result<(), &'static str>,
    pub event_start: fn(pmu_id: u32, event_id: u32) -> Result<(), &'static str>,
    pub event_stop: fn(pmu_id: u32, event_id: u32) -> Result<(), &'static str>,
    pub event_read: fn(pmu_id: u32, event_id: u32) -> Result<u64, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static PMU_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static EVENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PMUS: RwLock<BTreeMap<u32, Pmu>> = RwLock::new(BTreeMap::new());
static EVENTS: RwLock<BTreeMap<u32, PerfEvent>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PMU.
pub fn register_pmu(
    name: &str,
    num_counters: u32,
    supported_events: Vec<PerfHwId>,
    ops: PmuOps,
) -> Result<u32, &'static str> {
    let id = PMU_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pmu = Pmu {
        id,
        name: String::from(name),
        num_counters,
        supported_events,
        ops,
        event_ids: Vec::new(),
    };
    PMUS.write().insert(id, pmu);
    Ok(id)
}

/// Unregister a PMU.
pub fn unregister_pmu(pmu_id: u32) -> Result<(), &'static str> {
    PMUS.write().remove(&pmu_id).ok_or("PMU not found")?;
    Ok(())
}

/// Create a perf event (Linux `perf_event_create`).
pub fn create_event(
    pmu_id: u32,
    event_type: PerfHwId,
    config: u64,
    sample_period: u64,
) -> Result<u32, &'static str> {
    let init_fn = {
        let pmus = PMUS.read();
        let pmu = pmus.get(&pmu_id).ok_or("PMU not found")?;
        if !pmu.supported_events.contains(&event_type) {
            return Err("Event type not supported by PMU");
        }
        if pmu.event_ids.len() >= pmu.num_counters as usize {
            return Err("No free counter slots");
        }
        pmu.ops.event_init
    };
    (init_fn)(pmu_id, event_type, config)?;

    let event_id = EVENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let event = PerfEvent {
        id: event_id,
        pmu_id,
        event_type,
        config,
        counter: 0,
        enabled: false,
        sample_period,
        sample_count: 0,
    };
    EVENTS.write().insert(event_id, event);

    let mut pmus = PMUS.write();
    if let Some(pmu) = pmus.get_mut(&pmu_id) {
        pmu.event_ids.push(event_id);
    }
    Ok(event_id)
}

/// Add and start an event (Linux `perf_event_enable`).
pub fn enable_event(event_id: u32) -> Result<(), &'static str> {
    let (pmu_id, add_fn, start_fn) = {
        let events = EVENTS.read();
        let event = events.get(&event_id).ok_or("Perf event not found")?;
        let pmus = PMUS.read();
        let pmu = pmus.get(&event.pmu_id).ok_or("PMU not found")?;
        (event.pmu_id, pmu.ops.event_add, pmu.ops.event_start)
    };
    (add_fn)(pmu_id, event_id)?;
    (start_fn)(pmu_id, event_id)?;
    let mut events = EVENTS.write();
    if let Some(event) = events.get_mut(&event_id) {
        event.enabled = true;
    }
    Ok(())
}

/// Stop and remove an event (Linux `perf_event_disable`).
pub fn disable_event(event_id: u32) -> Result<(), &'static str> {
    let (pmu_id, stop_fn, del_fn) = {
        let events = EVENTS.read();
        let event = events.get(&event_id).ok_or("Perf event not found")?;
        let pmus = PMUS.read();
        let pmu = pmus.get(&event.pmu_id).ok_or("PMU not found")?;
        (event.pmu_id, pmu.ops.event_stop, pmu.ops.event_del)
    };
    (stop_fn)(pmu_id, event_id)?;
    (del_fn)(pmu_id, event_id)?;
    let mut events = EVENTS.write();
    if let Some(event) = events.get_mut(&event_id) {
        event.enabled = false;
    }
    Ok(())
}

/// Read a counter value (Linux `perf_event_read`).
pub fn read_event(event_id: u32) -> Result<u64, &'static str> {
    let (pmu_id, read_fn) = {
        let events = EVENTS.read();
        let event = events.get(&event_id).ok_or("Perf event not found")?;
        let pmus = PMUS.read();
        let pmu = pmus.get(&event.pmu_id).ok_or("PMU not found")?;
        (event.pmu_id, pmu.ops.event_read)
    };
    let value = (read_fn)(pmu_id, event_id)?;
    let mut events = EVENTS.write();
    if let Some(event) = events.get_mut(&event_id) {
        event.counter = value;
        event.sample_count += 1;
    }
    Ok(value)
}

/// List all PMUs.
pub fn list_pmus() -> Vec<(u32, String, u32, usize)> {
    PMUS.read()
        .iter()
        .map(|(id, p)| (*id, p.name.clone(), p.num_counters, p.event_ids.len()))
        .collect()
}

/// List all events.
pub fn list_events() -> Vec<(u32, u32, PerfHwId, bool, u64)> {
    EVENTS
        .read()
        .iter()
        .map(|(id, e)| (*id, e.pmu_id, e.event_type, e.enabled, e.counter))
        .collect()
}

/// Count registered PMUs.
pub fn pmu_count() -> usize {
    PMUS.read().len()
}

// ── Software PMU ────────────────────────────────────────────────────────

static SW_COUNTERS: RwLock<BTreeMap<(u32, u32), u64>> = RwLock::new(BTreeMap::new());

fn sw_event_init(_pmu_id: u32, _event_type: PerfHwId, _config: u64) -> Result<(), &'static str> {
    Ok(())
}

fn sw_event_add(_pmu_id: u32, _event_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_event_del(pmu_id: u32, event_id: u32) -> Result<(), &'static str> {
    SW_COUNTERS.write().remove(&(pmu_id, event_id));
    Ok(())
}

fn sw_event_start(pmu_id: u32, event_id: u32) -> Result<(), &'static str> {
    SW_COUNTERS.write().insert((pmu_id, event_id), 0);
    Ok(())
}

fn sw_event_stop(_pmu_id: u32, _event_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_event_read(pmu_id: u32, event_id: u32) -> Result<u64, &'static str> {
    let mut counters = SW_COUNTERS.write();
    let entry = counters.entry((pmu_id, event_id)).or_insert(0);
    *entry += 1000;
    Ok(*entry)
}

/// Software PMU ops.
pub fn software_pmu_ops() -> PmuOps {
    PmuOps {
        event_init: sw_event_init,
        event_add: sw_event_add,
        event_del: sw_event_del,
        event_start: sw_event_start,
        event_stop: sw_event_stop,
        event_read: sw_event_read,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !PMUS.read().is_empty() {
        return Ok(());
    }

    let ops = software_pmu_ops();
    let supported = alloc::vec![
        PerfHwId::CpuCycles,
        PerfHwId::Instructions,
        PerfHwId::CacheReferences,
        PerfHwId::CacheMisses,
        PerfHwId::BranchInstructions,
        PerfHwId::BranchMisses,
    ];
    let pmu_id = register_pmu("sw-pmu", 6, supported, ops)?;
    crate::serial_println!("perf: software PMU registered (id={}, 6 counters)", pmu_id);
    Ok(())
}
