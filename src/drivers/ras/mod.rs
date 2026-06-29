//! RAS (Reliability, Availability, Serviceability) subsystem
//!
//! Provides framework for hardware error reporting and RAS features.
//! Mirrors Linux's `drivers/ras/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// RAS controller (Linux `struct ras_controller`).
pub struct RasController {
    pub id: u32,
    pub name: String,
    pub state: RasState,
    pub ops: RasOps,
    pub error_count: u64,
    pub corrected_count: u64,
    pub uncorrected_count: u64,
    pub ce_threshold: u32,
}

/// RAS state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasState {
    Unregistered,
    Registered,
    Active,
    Disabled,
}

/// RAS operations.
pub struct RasOps {
    pub enable: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub disable: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub poll_errors: fn(ctrl_id: u32) -> Result<Vec<RasError>, &'static str>,
    pub inject_error: fn(ctrl_id: u32, error: &RasError) -> Result<(), &'static str>,
    pub clear_errors: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub get_status: fn(ctrl_id: u32) -> Result<RasStatus, &'static str>,
}

/// RAS error (Linux `struct ras_error`).
#[derive(Debug, Clone)]
pub struct RasError {
    pub error_type: RasErrorType,
    pub severity: RasSeverity,
    pub component: String,
    pub address: u64,
    pub syndrome: u32,
    pub timestamp: u64,
    pub message: String,
}

/// RAS error type (Linux `enum ras_error_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasErrorType {
    Correctable,
    Uncorrectable,
    Fatal,
    Deferred,
    Poison,
}

/// RAS severity (Linux `enum ras_severity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasSeverity {
    None,
    Corrected,
    Recoverable,
    Fatal,
    Panic,
}

/// RAS status (Linux `struct ras_status`).
#[derive(Debug, Clone)]
pub struct RasStatus {
    pub active: bool,
    pub ce_count: u64,
    pub ue_count: u64,
    pub last_error_type: Option<RasErrorType>,
    pub last_error_time: u64,
}

/// RAS memory controller (Linux `struct ras_mem_controller`).
pub struct RasMemController {
    pub id: u32,
    pub ctrl_id: u32,
    pub name: String,
    pub dimm_ids: Vec<u32>,
    pub scrubber_active: bool,
    pub patrol_scrub_interval: u64,
}

/// RAS DIMM (Linux `struct ras_dimm`).
pub struct RasDimm {
    pub id: u32,
    pub mem_ctrl_id: u32,
    pub label: String,
    pub ce_count: u64,
    pub ue_count: u64,
    pub size_mb: u64,
    pub rank: u8,
    pub device: u16,
    pub manufacturer: u16,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MEM_CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DIMM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static RAS_CTRLS: RwLock<BTreeMap<u32, RasController>> = RwLock::new(BTreeMap::new());
static RAS_MEM_CTRLS: RwLock<BTreeMap<u32, RasMemController>> = RwLock::new(BTreeMap::new());
static RAS_DIMMS: RwLock<BTreeMap<u32, RasDimm>> = RwLock::new(BTreeMap::new());
static RAS_ERROR_LOG: RwLock<Vec<RasError>> = RwLock::new(Vec::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a RAS controller.
pub fn register_controller(
    name: &str,
    ce_threshold: u32,
    ops: RasOps,
) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = RasController {
        id,
        name: String::from(name),
        state: RasState::Registered,
        ops,
        error_count: 0,
        corrected_count: 0,
        uncorrected_count: 0,
        ce_threshold,
    };
    RAS_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Enable RAS on a controller.
pub fn enable(ctrl_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.enable
    };
    (enable_fn)(ctrl_id)?;

    let mut ctrls = RAS_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.state = RasState::Active;
    }
    Ok(())
}

/// Disable RAS on a controller.
pub fn disable(ctrl_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.disable
    };
    (disable_fn)(ctrl_id)?;

    let mut ctrls = RAS_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.state = RasState::Disabled;
    }
    Ok(())
}

/// Poll for errors (Linux `ras_poll_errors`).
pub fn poll_errors(ctrl_id: u32) -> Result<Vec<RasError>, &'static str> {
    let poll_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.poll_errors
    };
    let errors = (poll_fn)(ctrl_id)?;

    let mut ctrls = RAS_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        for err in &errors {
            ctrl.error_count += 1;
            match err.error_type {
                RasErrorType::Correctable => ctrl.corrected_count += 1,
                RasErrorType::Uncorrectable | RasErrorType::Fatal | RasErrorType::Poison => {
                    ctrl.uncorrected_count += 1;
                }
                RasErrorType::Deferred => {}
            }
        }
    }

    // Log errors
    let mut log = RAS_ERROR_LOG.write();
    for err in &errors {
        log.push(err.clone());
    }
    // Keep log bounded
    if log.len() > 1024 {
        let drain = log.len() - 1024;
        log.drain(0..drain);
    }

    Ok(errors)
}

/// Inject an error (Linux `ras_inject_error`).
pub fn inject_error(ctrl_id: u32, error: &RasError) -> Result<(), &'static str> {
    let inject_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.inject_error
    };
    (inject_fn)(ctrl_id, error)?;

    let mut log = RAS_ERROR_LOG.write();
    log.push(error.clone());
    Ok(())
}

/// Clear error log.
pub fn clear_errors(ctrl_id: u32) -> Result<(), &'static str> {
    let clear_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.clear_errors
    };
    (clear_fn)(ctrl_id)?;

    let mut ctrls = RAS_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.error_count = 0;
        ctrl.corrected_count = 0;
        ctrl.uncorrected_count = 0;
    }
    Ok(())
}

/// Get RAS status.
pub fn get_status(ctrl_id: u32) -> Result<RasStatus, &'static str> {
    let status_fn = {
        let ctrls = RAS_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
        ctrl.ops.get_status
    };
    (status_fn)(ctrl_id)
}

/// Register a memory controller.
pub fn register_mem_controller(ctrl_id: u32, name: &str) -> Result<u32, &'static str> {
    let id = MEM_CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mem_ctrl = RasMemController {
        id,
        ctrl_id,
        name: String::from(name),
        dimm_ids: Vec::new(),
        scrubber_active: false,
        patrol_scrub_interval: 24 * 3600, // 24 hours
    };
    RAS_MEM_CTRLS.write().insert(id, mem_ctrl);
    Ok(id)
}

/// Register a DIMM.
pub fn register_dimm(
    mem_ctrl_id: u32,
    label: &str,
    size_mb: u64,
    rank: u8,
    device: u16,
    manufacturer: u16,
) -> Result<u32, &'static str> {
    let id = DIMM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dimm = RasDimm {
        id,
        mem_ctrl_id,
        label: String::from(label),
        ce_count: 0,
        ue_count: 0,
        size_mb,
        rank,
        device,
        manufacturer,
    };
    RAS_DIMMS.write().insert(id, dimm);

    let mut mem_ctrls = RAS_MEM_CTRLS.write();
    if let Some(mc) = mem_ctrls.get_mut(&mem_ctrl_id) {
        mc.dimm_ids.push(id);
    }
    Ok(id)
}

/// Record a DIMM error.
pub fn record_dimm_error(dimm_id: u32, is_correctable: bool) -> Result<(), &'static str> {
    let mut dimms = RAS_DIMMS.write();
    let dimm = dimms.get_mut(&dimm_id).ok_or("DIMM not found")?;
    if is_correctable {
        dimm.ce_count += 1;
    } else {
        dimm.ue_count += 1;
    }
    Ok(())
}

/// Get error log.
pub fn get_error_log() -> Vec<RasError> {
    RAS_ERROR_LOG.read().clone()
}

/// List all RAS controllers.
pub fn list_controllers() -> Vec<(u32, String, RasState, u64, u64, u64)> {
    RAS_CTRLS
        .read()
        .iter()
        .map(|(id, c)| {
            (
                *id,
                c.name.clone(),
                c.state,
                c.error_count,
                c.corrected_count,
                c.uncorrected_count,
            )
        })
        .collect()
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    RAS_CTRLS.read().len()
}

// ── Software RAS ────────────────────────────────────────────────────────

fn sw_enable(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_poll_errors(_ctrl_id: u32) -> Result<Vec<RasError>, &'static str> {
    Ok(Vec::new())
}
fn sw_inject_error(_ctrl_id: u32, _error: &RasError) -> Result<(), &'static str> {
    Ok(())
}
fn sw_clear_errors(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_status(ctrl_id: u32) -> Result<RasStatus, &'static str> {
    let ctrls = RAS_CTRLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("RAS controller not found")?;
    Ok(RasStatus {
        active: ctrl.state == RasState::Active,
        ce_count: ctrl.corrected_count,
        ue_count: ctrl.uncorrected_count,
        last_error_type: None,
        last_error_time: 0,
    })
}

/// Software RAS ops.
pub fn software_ras_ops() -> RasOps {
    RasOps {
        enable: sw_enable,
        disable: sw_disable,
        poll_errors: sw_poll_errors,
        inject_error: sw_inject_error,
        clear_errors: sw_clear_errors,
        get_status: sw_get_status,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ras: subsystem ready");
    Ok(())
}
