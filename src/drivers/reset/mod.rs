//! Reset controller framework
//!
//! Provides reset line control for peripheral devices (assert/deassert,
//! status query). Mirrors Linux's `drivers/reset/core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Reset controller operations (Linux `struct reset_control_ops`).
pub struct ResetOps {
    pub assert: fn(line: u32) -> Result<(), &'static str>,
    pub deassert: fn(line: u32) -> Result<(), &'static str>,
    pub status: fn(line: u32) -> Result<bool, &'static str>,
    pub reset: fn(line: u32) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_nresets: fn() -> u32,
}

struct ResetController {
    id: u32,
    name: String,
    nresets: u32,
    ops: &'static ResetOps,
}

/// Per-reset line state.
struct ResetLineState {
    controller_id: u32,
    line: u32,
    asserted: bool,
    consumer: Option<String>,
}

// ── Software reset controller ───────────────────────────────────────────

static SW_RESET_STATES: RwLock<Vec<bool>> = RwLock::new(Vec::new());

fn sw_assert(line: u32) -> Result<(), &'static str> {
    let mut states = SW_RESET_STATES.write();
    let idx = line as usize;
    if idx >= states.len() {
        return Err("Reset line out of range");
    }
    states[idx] = true;
    Ok(())
}

fn sw_deassert(line: u32) -> Result<(), &'static str> {
    let mut states = SW_RESET_STATES.write();
    let idx = line as usize;
    if idx >= states.len() {
        return Err("Reset line out of range");
    }
    states[idx] = false;
    Ok(())
}

fn sw_status(line: u32) -> Result<bool, &'static str> {
    let states = SW_RESET_STATES.read();
    let idx = line as usize;
    if idx >= states.len() {
        return Err("Reset line out of range");
    }
    Ok(states[idx])
}

fn sw_reset(line: u32) -> Result<(), &'static str> {
    // Assert then deassert (pulse reset).
    sw_assert(line)?;
    // Small delay via spin loop.
    let mut i = 0u32;
    while i < 1000 {
        core::hint::spin_loop();
        i += 1;
    }
    sw_deassert(line)
}

fn sw_name() -> &'static str {
    "software-reset"
}
fn sw_nresets() -> u32 {
    16
}

pub static SW_RESET_OPS: ResetOps = ResetOps {
    assert: sw_assert,
    deassert: sw_deassert,
    status: sw_status,
    reset: sw_reset,
    get_name: sw_name,
    get_nresets: sw_nresets,
};

// ── Registry ────────────────────────────────────────────────────────────

static RESET_CONTROLLERS: RwLock<BTreeMap<u32, ResetController>> = RwLock::new(BTreeMap::new());
static RESET_LINES: RwLock<BTreeMap<(u32, u32), ResetLineState>> = RwLock::new(BTreeMap::new());
static NEXT_CTRL_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a reset controller (Linux `reset_controller_register`).
pub fn register_controller(name: &str, ops: &'static ResetOps) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("Reset controller name is empty");
    }
    let nresets = (ops.get_nresets)();
    if nresets == 0 {
        return Err("Reset controller must have at least one line");
    }
    if RESET_CONTROLLERS
        .read()
        .values()
        .any(|controller| controller.name == name)
    {
        return Err("Reset controller already registered");
    }

    let id = NEXT_CTRL_ID.fetch_add(1, Ordering::SeqCst);
    RESET_CONTROLLERS.write().insert(
        id,
        ResetController {
            id,
            name: String::from(name),
            nresets,
            ops,
        },
    );

    // Initialize line states (all deasserted by default).
    let mut lines = RESET_LINES.write();
    for line in 0..nresets {
        lines.insert(
            (id, line),
            ResetLineState {
                controller_id: id,
                line,
                asserted: false,
                consumer: None,
            },
        );
    }
    Ok(id)
}

/// Request a reset control (Linux `reset_control_get`).
pub fn request_line(ctrl_id: u32, line: u32, consumer: &str) -> Result<(), &'static str> {
    if consumer.is_empty() {
        return Err("Reset consumer name is empty");
    }
    let mut lines = RESET_LINES.write();
    let state = lines
        .get_mut(&(ctrl_id, line))
        .ok_or("Reset line not found")?;
    if state.consumer.is_some() {
        return Err("Reset line already requested");
    }
    state.consumer = Some(String::from(consumer));
    Ok(())
}

/// Assert a reset line (Linux `reset_control_assert`).
pub fn assert_line(ctrl_id: u32, line: u32) -> Result<(), &'static str> {
    let ops = {
        let ctrls = RESET_CONTROLLERS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("Reset controller not found")?;
        ctrl.ops
    };
    (ops.assert)(line)?;
    let mut lines = RESET_LINES.write();
    if let Some(state) = lines.get_mut(&(ctrl_id, line)) {
        state.asserted = true;
    }
    Ok(())
}

/// Deassert a reset line (Linux `reset_control_deassert`).
pub fn deassert_line(ctrl_id: u32, line: u32) -> Result<(), &'static str> {
    let ops = {
        let ctrls = RESET_CONTROLLERS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("Reset controller not found")?;
        ctrl.ops
    };
    (ops.deassert)(line)?;
    let mut lines = RESET_LINES.write();
    if let Some(state) = lines.get_mut(&(ctrl_id, line)) {
        state.asserted = false;
    }
    Ok(())
}

/// Pulse a reset line (Linux `reset_control_reset`).
pub fn reset_line(ctrl_id: u32, line: u32) -> Result<(), &'static str> {
    let ops = {
        let ctrls = RESET_CONTROLLERS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("Reset controller not found")?;
        ctrl.ops
    };
    (ops.reset)(line)?;
    // After a reset pulse, line is deasserted.
    let mut lines = RESET_LINES.write();
    if let Some(state) = lines.get_mut(&(ctrl_id, line)) {
        state.asserted = false;
    }
    Ok(())
}

/// Get reset line status.
pub fn get_status(ctrl_id: u32, line: u32) -> Result<bool, &'static str> {
    let lines = RESET_LINES.read();
    let state = lines.get(&(ctrl_id, line)).ok_or("Reset line not found")?;
    Ok(state.asserted)
}

/// Free a requested reset line (Linux `reset_control_put`).
pub fn free_line(ctrl_id: u32, line: u32) -> Result<(), &'static str> {
    let mut lines = RESET_LINES.write();
    let state = lines
        .get_mut(&(ctrl_id, line))
        .ok_or("Reset line not found")?;
    state.consumer = None;
    Ok(())
}

/// Number of registered reset controllers.
pub fn controller_count() -> usize {
    RESET_CONTROLLERS.read().len()
}

/// Total number of reset lines across all controllers.
pub fn total_lines() -> usize {
    RESET_LINES.read().len()
}

/// Initialize reset controller subsystem with software controller.
pub fn init() -> Result<(), &'static str> {
    if !RESET_CONTROLLERS.read().is_empty() {
        return Ok(());
    }

    let nresets = (SW_RESET_OPS.get_nresets)();
    *SW_RESET_STATES.write() = alloc::vec![false; nresets as usize];

    register_controller("software-reset", &SW_RESET_OPS)?;
    crate::serial_println!("reset: software controller registered ({} lines)", nresets);
    Ok(())
}
