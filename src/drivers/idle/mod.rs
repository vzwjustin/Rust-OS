//! Idle (Intel idle) driver subsystem
//!
//! Provides CPU idle state management for Intel processors.
//! Mirrors Linux's `drivers/idle/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CPU idle state (Linux `struct cpuidle_state`).
#[derive(Debug)]
pub struct IdleState {
    pub id: u32,
    pub name: String,
    pub desc: String,
    pub exit_latency_ns: u64,
    pub target_residency_ns: u64,
    pub power_usage: u32,
    pub flags: u32,
    pub usage_count: AtomicU64,
    pub time_spent_ns: AtomicU64,
    pub disabled: bool,
}

/// Idle driver (Linux `struct cpuidle_driver`).
pub struct IdleDriver {
    pub id: u32,
    pub name: String,
    pub cpumask: u64,
    pub state_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static STATE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IDLE_STATES: RwLock<BTreeMap<u32, IdleState>> = RwLock::new(BTreeMap::new());
static IDLE_DRIVERS: RwLock<BTreeMap<u32, IdleDriver>> = RwLock::new(BTreeMap::new());

// ── Intel idle state flags ──────────────────────────────────────────────

pub const IDLE_FLAG_POLLING: u32 = 0x01;
pub const IDLE_FLAG_COUPLED: u32 = 0x02;
pub const IDLE_FLAG_TIMER_STOP: u32 = 0x04;

// ── Public API ──────────────────────────────────────────────────────────

/// Register an idle state (Linux `cpuidle_state_register`).
pub fn register_state(
    name: &str,
    desc: &str,
    exit_latency_ns: u64,
    target_residency_ns: u64,
    power_usage: u32,
    flags: u32,
) -> Result<u32, &'static str> {
    if exit_latency_ns == 0 {
        return Err("Idle state exit latency must be non-zero");
    }
    if target_residency_ns == 0 {
        return Err("Idle state target residency must be non-zero");
    }

    let id = STATE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let state = IdleState {
        id,
        name: String::from(name),
        desc: String::from(desc),
        exit_latency_ns,
        target_residency_ns,
        power_usage,
        flags,
        usage_count: AtomicU64::new(0),
        time_spent_ns: AtomicU64::new(0),
        disabled: false,
    };
    IDLE_STATES.write().insert(id, state);
    Ok(id)
}

/// Register an idle driver (Linux `cpuidle_register_driver`).
pub fn register_driver(name: &str, cpumask: u64, state_ids: Vec<u32>) -> Result<u32, &'static str> {
    if cpumask == 0 {
        return Err("Idle driver requires at least one CPU");
    }
    if state_ids.is_empty() {
        return Err("Idle driver requires at least one state");
    }

    for sid in &state_ids {
        if !IDLE_STATES.read().contains_key(sid) {
            return Err("Idle state not found");
        }
    }

    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv = IdleDriver {
        id,
        name: String::from(name),
        cpumask,
        state_ids,
    };
    IDLE_DRIVERS.write().insert(id, drv);
    Ok(id)
}

/// Enter an idle state (Linux `cpuidle_enter`).
pub fn enter_state(state_id: u32) -> Result<(), &'static str> {
    let states = IDLE_STATES.read();
    let state = states.get(&state_id).ok_or("Idle state not found")?;
    if state.disabled {
        return Err("Idle state is disabled");
    }
    state.usage_count.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Disable an idle state.
pub fn disable_state(state_id: u32) -> Result<(), &'static str> {
    let mut states = IDLE_STATES.write();
    let state = states.get_mut(&state_id).ok_or("Idle state not found")?;
    state.disabled = true;
    Ok(())
}

/// Enable an idle state.
pub fn enable_state(state_id: u32) -> Result<(), &'static str> {
    let mut states = IDLE_STATES.write();
    let state = states.get_mut(&state_id).ok_or("Idle state not found")?;
    state.disabled = false;
    Ok(())
}

/// Get usage statistics for an idle state.
pub fn get_state_usage(state_id: u32) -> Result<(u64, u64), &'static str> {
    let states = IDLE_STATES.read();
    let state = states.get(&state_id).ok_or("Idle state not found")?;
    Ok((
        state.usage_count.load(Ordering::SeqCst),
        state.time_spent_ns.load(Ordering::SeqCst),
    ))
}

/// List all idle states.
pub fn list_states() -> Vec<(u32, String, u64, u64, bool)> {
    IDLE_STATES
        .read()
        .iter()
        .map(|(id, s)| {
            (
                *id,
                s.name.clone(),
                s.exit_latency_ns,
                s.target_residency_ns,
                s.disabled,
            )
        })
        .collect()
}

/// List all idle drivers.
pub fn list_drivers() -> Vec<(u32, String, u64, usize)> {
    IDLE_DRIVERS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.cpumask, d.state_ids.len()))
        .collect()
}

/// Count states.
pub fn state_count() -> usize {
    IDLE_STATES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !IDLE_STATES.read().is_empty() {
        return Ok(());
    }

    let poll_id = register_state("POLL", "CPU polling idle state", 0, 0, 0, IDLE_FLAG_POLLING)?;
    let c1_id = register_state("C1", "MWAIT 0x00", 1_000, 10_000, 0, 0)?;
    let c1e_id = register_state("C1E", "MWAIT 0x01", 10_000, 20_000, 0, 0)?;
    let c3_id = register_state("C3", "MWAIT 0x10", 40_000, 100_000, 0, IDLE_FLAG_TIMER_STOP)?;
    let c6_id = register_state(
        "C6",
        "MWAIT 0x20",
        100_000,
        400_000,
        0,
        IDLE_FLAG_TIMER_STOP,
    )?;

    register_driver(
        "intel_idle",
        0x01,
        vec![poll_id, c1_id, c1e_id, c3_id, c6_id],
    )?;

    crate::serial_println!(
        "idle: intel_idle driver registered with 5 states (POLL, C1, C1E, C3, C6)"
    );
    Ok(())
}
