//! DPLL (Digital Phase-Locked Loop) subsystem
//!
//! Provides DPLL framework for clock synchronization and phase control.
//! Mirrors Linux's `drivers/dpll/dpll_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// DPLL device (Linux `struct dpll_device`).
pub struct DpllDevice {
    pub id: u32,
    pub name: String,
    pub ops: DpllOps,
    pub type_: DpllType,
    pub state: DpllState,
    pub lock_status: DpllLockStatus,
    pub mode: DpllMode,
    pub source_pins: Vec<u32>,
    pub output_pins: Vec<u32>,
}

/// DPLL type (Linux `enum dpll_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllType {
    Pps,
    Eec,
    Fsync,
    Phy,
}

/// DPLL state (Linux `enum dpll_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllState {
    Unset,
    Locked,
    LockedHoAcq,
    Holdover,
    Freerun,
    Unlocked,
}

/// DPLL lock status (Linux `enum dpll_lock_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllLockStatus {
    None,
    Calibrating,
    Locked,
    Holdover,
    Failed,
}

/// DPLL mode (Linux `enum dpll_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllMode {
    Manual,
    Automatic,
}

/// DPLL pin (Linux `struct dpll_pin`).
pub struct DpllPin {
    pub id: u32,
    pub name: String,
    pub type_: DpllPinType,
    pub direction: DpllPinDirection,
    pub frequency: u64,
    pub priority: u32,
    pub state: DpllPinState,
    pub parent_dpll: u32,
}

/// DPLL pin type (Linux `enum dpll_pin_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllPinType {
    Mux,
    Ext,
    SynceEthPort,
    Gpio,
}

/// DPLL pin direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllPinDirection {
    Input,
    Output,
}

/// DPLL pin state (Linux `enum dpll_pin_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpllPinState {
    Connected,
    Disconnected,
    Selectable,
}

/// DPLL operations (Linux `struct dpll_device_ops`).
pub struct DpllOps {
    pub state_get: fn(dev_id: u32) -> Result<DpllState, &'static str>,
    pub state_set: fn(dev_id: u32, state: DpllState) -> Result<(), &'static str>,
    pub lock_status_get: fn(dev_id: u32) -> Result<DpllLockStatus, &'static str>,
    pub mode_get: fn(dev_id: u32) -> Result<DpllMode, &'static str>,
    pub mode_set: fn(dev_id: u32, mode: DpllMode) -> Result<(), &'static str>,
    pub source_pin_select: fn(dev_id: u32, pin_id: u32) -> Result<(), &'static str>,
    pub output_pin_select: fn(dev_id: u32, pin_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PIN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static DPLL_DEVS: RwLock<BTreeMap<u32, DpllDevice>> = RwLock::new(BTreeMap::new());
static DPLL_PINS: RwLock<BTreeMap<u32, DpllPin>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a DPLL device.
pub fn register_device(name: &str, ops: DpllOps, type_: DpllType) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = DpllDevice {
        id,
        name: String::from(name),
        ops,
        type_,
        state: DpllState::Unset,
        lock_status: DpllLockStatus::None,
        mode: DpllMode::Automatic,
        source_pins: Vec::new(),
        output_pins: Vec::new(),
    };
    DPLL_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Register a DPLL pin.
pub fn register_pin(
    name: &str,
    type_: DpllPinType,
    direction: DpllPinDirection,
    frequency: u64,
    parent_dpll: u32,
) -> Result<u32, &'static str> {
    let id = PIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pin = DpllPin {
        id,
        name: String::from(name),
        type_,
        direction,
        frequency,
        priority: 0,
        state: DpllPinState::Selectable,
        parent_dpll,
    };
    DPLL_PINS.write().insert(id, pin);

    let mut devs = DPLL_DEVS.write();
    if let Some(dev) = devs.get_mut(&parent_dpll) {
        if direction == DpllPinDirection::Input {
            dev.source_pins.push(id);
        } else {
            dev.output_pins.push(id);
        }
    }
    Ok(id)
}

/// Get DPLL state (Linux `dpll_state_get`).
pub fn get_state(dev_id: u32) -> Result<DpllState, &'static str> {
    let get_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.state_get
    };
    (get_fn)(dev_id)
}

/// Set DPLL state (Linux `dpll_state_set`).
pub fn set_state(dev_id: u32, state: DpllState) -> Result<(), &'static str> {
    let set_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.state_set
    };
    (set_fn)(dev_id, state)?;

    let mut devs = DPLL_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = state;
    }
    Ok(())
}

/// Get DPLL lock status.
pub fn get_lock_status(dev_id: u32) -> Result<DpllLockStatus, &'static str> {
    let get_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.lock_status_get
    };
    (get_fn)(dev_id)
}

/// Set DPLL mode.
pub fn set_mode(dev_id: u32, mode: DpllMode) -> Result<(), &'static str> {
    let set_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.mode_set
    };
    (set_fn)(dev_id, mode)?;

    let mut devs = DPLL_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.mode = mode;
    }
    Ok(())
}

/// Select source pin (Linux `dpll_source_pin_select`).
pub fn select_source_pin(dev_id: u32, pin_id: u32) -> Result<(), &'static str> {
    let select_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.source_pin_select
    };
    (select_fn)(dev_id, pin_id)?;

    let mut pins = DPLL_PINS.write();
    if let Some(pin) = pins.get_mut(&pin_id) {
        pin.state = DpllPinState::Connected;
    }
    Ok(())
}

/// Select output pin (Linux `dpll_output_pin_select`).
pub fn select_output_pin(dev_id: u32, pin_id: u32) -> Result<(), &'static str> {
    let select_fn = {
        let devs = DPLL_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
        dev.ops.output_pin_select
    };
    (select_fn)(dev_id, pin_id)?;

    let mut pins = DPLL_PINS.write();
    if let Some(pin) = pins.get_mut(&pin_id) {
        pin.state = DpllPinState::Connected;
    }
    Ok(())
}

/// Set pin priority.
pub fn set_pin_priority(pin_id: u32, priority: u32) -> Result<(), &'static str> {
    let mut pins = DPLL_PINS.write();
    let pin = pins.get_mut(&pin_id).ok_or("DPLL pin not found")?;
    pin.priority = priority;
    Ok(())
}

/// List all DPLL devices.
pub fn list_devices() -> Vec<(u32, String, DpllType, DpllState)> {
    DPLL_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.type_, d.state))
        .collect()
}

/// List pins for a DPLL.
pub fn list_pins(
    dev_id: u32,
) -> Result<Vec<(u32, String, DpllPinType, DpllPinDirection, DpllPinState)>, &'static str> {
    let devs = DPLL_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("DPLL device not found")?;
    let pins = DPLL_PINS.read();
    let mut result = Vec::new();
    for &pid in dev.source_pins.iter().chain(dev.output_pins.iter()) {
        if let Some(pin) = pins.get(&pid) {
            result.push((
                pin.id,
                pin.name.clone(),
                pin.type_,
                pin.direction,
                pin.state,
            ));
        }
    }
    Ok(result)
}

/// Count registered devices.
pub fn device_count() -> usize {
    DPLL_DEVS.read().len()
}

// ── Software DPLL ───────────────────────────────────────────────────────

fn sw_state_get(dev_id: u32) -> Result<DpllState, &'static str> {
    let devs = DPLL_DEVS.read();
    Ok(devs
        .get(&dev_id)
        .map(|d| d.state)
        .unwrap_or(DpllState::Unset))
}
fn sw_state_set(_dev_id: u32, _state: DpllState) -> Result<(), &'static str> {
    Ok(())
}
fn sw_lock_status_get(dev_id: u32) -> Result<DpllLockStatus, &'static str> {
    let devs = DPLL_DEVS.read();
    Ok(devs
        .get(&dev_id)
        .map(|d| d.lock_status)
        .unwrap_or(DpllLockStatus::None))
}
fn sw_mode_get(dev_id: u32) -> Result<DpllMode, &'static str> {
    let devs = DPLL_DEVS.read();
    Ok(devs
        .get(&dev_id)
        .map(|d| d.mode)
        .unwrap_or(DpllMode::Automatic))
}
fn sw_mode_set(_dev_id: u32, _mode: DpllMode) -> Result<(), &'static str> {
    Ok(())
}
fn sw_source_pin_select(_dev_id: u32, _pin_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_output_pin_select(_dev_id: u32, _pin_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software DPLL ops.
pub fn software_dpll_ops() -> DpllOps {
    DpllOps {
        state_get: sw_state_get,
        state_set: sw_state_set,
        lock_status_get: sw_lock_status_get,
        mode_get: sw_mode_get,
        mode_set: sw_mode_set,
        source_pin_select: sw_source_pin_select,
        output_pin_select: sw_output_pin_select,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("dpll: subsystem ready");
    Ok(())
}
