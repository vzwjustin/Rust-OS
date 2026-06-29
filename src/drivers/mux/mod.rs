//! Mux (multiplexer) subsystem
//!
//! Provides multiplexer framework for selecting between multiple signal paths.
//! Mirrors Linux's `drivers/mux/mux-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Mux state type (Linux `enum mux_state_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuxState {
    Idle,
    Active(u32),
    Error,
}

/// Mux control operations (Linux `struct mux_control_ops`).
pub struct MuxControlOps {
    pub set: fn(control_id: u32, state: u32) -> Result<(), &'static str>,
    pub get: fn(control_id: u32) -> Result<u32, &'static str>,
}

/// Mux control (Linux `struct mux_control`).
pub struct MuxControl {
    pub name: String,
    pub ops: MuxControlOps,
    pub states: u32,
    pub idle_state: u32,
    pub current_state: MuxState,
    pub cached_state: Option<u32>,
    pub lock_held: bool,
}

/// Mux chip (Linux `struct mux_chip`).
pub struct MuxChip {
    pub name: String,
    pub controls: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CONTROL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CHIP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MUX_CONTROLS: RwLock<BTreeMap<u32, MuxControl>> = RwLock::new(BTreeMap::new());
static MUX_CHIPS: RwLock<BTreeMap<u32, MuxChip>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a mux chip with controls.
pub fn register_chip(
    name: &str,
    ops: MuxControlOps,
    num_controls: u32,
    states: u32,
    idle_state: u32,
) -> Result<u32, &'static str> {
    let chip_id = CHIP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut control_ids = Vec::new();

    for _ in 0..num_controls {
        let ctrl_id = CONTROL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ctrl = MuxControl {
            name: String::from(name),
            ops: MuxControlOps {
                set: ops.set,
                get: ops.get,
            },
            states,
            idle_state,
            current_state: MuxState::Idle,
            cached_state: None,
            lock_held: false,
        };
        MUX_CONTROLS.write().insert(ctrl_id, ctrl);
        control_ids.push(ctrl_id);
    }

    let chip = MuxChip {
        name: String::from(name),
        controls: control_ids,
    };
    MUX_CHIPS.write().insert(chip_id, chip);
    Ok(chip_id)
}

/// Select a mux state (Linux `mux_control_select`).
pub fn select(control_id: u32, state: u32) -> Result<(), &'static str> {
    let set_fn = {
        let mut controls = MUX_CONTROLS.write();
        let ctrl = controls
            .get_mut(&control_id)
            .ok_or("Mux control not found")?;
        if ctrl.lock_held {
            return Err("Mux control already locked");
        }
        if state >= ctrl.states {
            return Err("Mux state out of range");
        }
        ctrl.lock_held = true;
        ctrl.ops.set
    };

    (set_fn)(control_id, state)?;

    let mut controls = MUX_CONTROLS.write();
    if let Some(ctrl) = controls.get_mut(&control_id) {
        ctrl.current_state = MuxState::Active(state);
        ctrl.cached_state = Some(state);
    }
    Ok(())
}

/// Deselect a mux (return to idle state) (Linux `mux_control_deselect`).
pub fn deselect(control_id: u32) -> Result<(), &'static str> {
    let (set_fn, idle_state) = {
        let mut controls = MUX_CONTROLS.write();
        let ctrl = controls
            .get_mut(&control_id)
            .ok_or("Mux control not found")?;
        if !ctrl.lock_held {
            return Err("Mux control not locked");
        }
        ctrl.lock_held = false;
        (ctrl.ops.set, ctrl.idle_state)
    };

    (set_fn)(control_id, idle_state)?;

    let mut controls = MUX_CONTROLS.write();
    if let Some(ctrl) = controls.get_mut(&control_id) {
        ctrl.current_state = MuxState::Idle;
    }
    Ok(())
}

/// Try to select a mux state (non-blocking).
pub fn try_select(control_id: u32, state: u32) -> Result<(), &'static str> {
    let mut controls = MUX_CONTROLS.write();
    let ctrl = controls
        .get_mut(&control_id)
        .ok_or("Mux control not found")?;
    if ctrl.lock_held {
        return Err("Mux control busy");
    }
    if state >= ctrl.states {
        return Err("Mux state out of range");
    }
    ctrl.lock_held = true;
    ctrl.current_state = MuxState::Active(state);
    ctrl.cached_state = Some(state);
    Ok(())
}

/// Get the current mux state.
pub fn get_state(control_id: u32) -> Result<MuxState, &'static str> {
    let controls = MUX_CONTROLS.read();
    let ctrl = controls.get(&control_id).ok_or("Mux control not found")?;
    Ok(ctrl.current_state)
}

/// Get the cached state if available.
pub fn get_cached_state(control_id: u32) -> Result<Option<u32>, &'static str> {
    let controls = MUX_CONTROLS.read();
    let ctrl = controls.get(&control_id).ok_or("Mux control not found")?;
    Ok(ctrl.cached_state)
}

/// List all registered mux chips.
pub fn list_chips() -> Vec<(u32, String, usize)> {
    MUX_CHIPS
        .read()
        .iter()
        .map(|(id, chip)| (*id, chip.name.clone(), chip.controls.len()))
        .collect()
}

/// Count registered controls.
pub fn control_count() -> usize {
    MUX_CONTROLS.read().len()
}

// ── Software mux ────────────────────────────────────────────────────────

fn sw_set(_control_id: u32, _state: u32) -> Result<(), &'static str> {
    Err("software mux control not available")
}
fn sw_get(_control_id: u32) -> Result<u32, &'static str> {
    Err("software mux control not available")
}

/// Software mux ops for callers that need an explicit unsupported backend.
pub fn software_mux_ops() -> MuxControlOps {
    MuxControlOps {
        set: sw_set,
        get: sw_get,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mux: subsystem ready");
    Ok(())
}
