//! EDAC (Error Detection And Correction) subsystem
//!
//! Provides memory error reporting, CE/UE counting, and DIMM-level error tracking.
//! Mirrors Linux's `drivers/edac/edac_device.c` and `edac_mc.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// EDAC error type (Linux `enum edac_error_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdacErrorType {
    Ce, // Correctable Error
    Ue, // Uncorrectable Error
}

/// EDAC error severity (Linux `enum hw_event_mc_err_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdacSeverity {
    Corrected,
    Uncorrected,
    Fatal,
    Info,
}

/// Memory error event (Linux `struct edac_raw_error_desc`).
#[derive(Debug, Clone)]
pub struct MemErrorEvent {
    pub error_type: EdacErrorType,
    pub severity: EdacSeverity,
    pub mc_id: u32,
    pub csrow: u32,
    pub channel: u32,
    pub syndrome: u32,
    pub page_frame_number: u64,
    pub offset_in_page: u64,
    pub grain: u64,
    pub msg: String,
    pub label: String,
    pub timestamp_ns: u64,
}

/// EDAC memory controller (Linux `struct mem_ctl_info`).
pub struct MemCtlInfo {
    pub id: u32,
    pub name: String,
    pub mc_name: String,
    pub n_csrows: u32,
    pub n_channels: u32,
    pub ce_count: u64,
    pub ue_count: u64,
    pub ce_noinfo_count: u64,
    pub ue_noinfo_count: u64,
    pub csrow_ce: Vec<Vec<u64>>,
    pub csrow_ue: Vec<Vec<u64>>,
    pub start_time_ns: u64,
}

/// EDAC device (Linux `struct edac_device_ctl_info`).
pub struct EdacDevice {
    pub id: u32,
    pub name: String,
    pub dev_name: String,
    pub ce_count: AtomicU64,
    pub ue_count: AtomicU64,
    pub blocks: Vec<EdacBlock>,
}

/// EDAC device block (Linux `struct edac_device_block`).
pub struct EdacBlock {
    pub name: String,
    pub ce_count: AtomicU64,
    pub ue_count: AtomicU64,
}

/// EDAC ops (Linux `struct edac_mc_ops` / `struct edac_device_ops`).
pub struct EdacOps {
    pub init: fn(mc_id: u32) -> Result<(), &'static str>,
    pub check: fn(mc_id: u32) -> Result<Vec<MemErrorEvent>, &'static str>,
    pub clear: fn(mc_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static MC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static EDAC_MCS: RwLock<BTreeMap<u32, MemCtlInfo>> = RwLock::new(BTreeMap::new());
static EDAC_DEVICES: RwLock<BTreeMap<u32, EdacDevice>> = RwLock::new(BTreeMap::new());
static EDAC_OPS: RwLock<BTreeMap<u32, EdacOps>> = RwLock::new(BTreeMap::new());
static ERROR_LOG: RwLock<Vec<MemErrorEvent>> = RwLock::new(Vec::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a memory controller for EDAC monitoring.
pub fn register_mc(
    name: &str,
    mc_name: &str,
    n_csrows: u32,
    n_channels: u32,
    ops: EdacOps,
) -> Result<u32, &'static str> {
    let id = MC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let mut csrow_ce = Vec::new();
    let mut csrow_ue = Vec::new();
    for _ in 0..n_csrows {
        csrow_ce.push({
            let mut v = Vec::new();
            v.resize(n_channels as usize, 0u64);
            v
        });
        csrow_ue.push({
            let mut v = Vec::new();
            v.resize(n_channels as usize, 0u64);
            v
        });
    }

    let mc = MemCtlInfo {
        id,
        name: String::from(name),
        mc_name: String::from(mc_name),
        n_csrows,
        n_channels,
        ce_count: 0,
        ue_count: 0,
        ce_noinfo_count: 0,
        ue_noinfo_count: 0,
        csrow_ce,
        csrow_ue,
        start_time_ns: crate::time::uptime_ns(),
    };
    EDAC_MCS.write().insert(id, mc);
    EDAC_OPS.write().insert(id, ops);
    Ok(id)
}

/// Register an EDAC device (non-memory controller device).
pub fn register_device(
    name: &str,
    dev_name: &str,
    block_names: &[&str],
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut blocks = Vec::new();
    for &bn in block_names {
        blocks.push(EdacBlock {
            name: String::from(bn),
            ce_count: AtomicU64::new(0),
            ue_count: AtomicU64::new(0),
        });
    }
    let dev = EdacDevice {
        id,
        name: String::from(name),
        dev_name: String::from(dev_name),
        ce_count: AtomicU64::new(0),
        ue_count: AtomicU64::new(0),
        blocks,
    };
    EDAC_DEVICES.write().insert(id, dev);
    Ok(id)
}

/// Report a memory error to the EDAC framework (Linux `edac_mc_handle_error`).
pub fn handle_error(event: MemErrorEvent) -> Result<(), &'static str> {
    let mut mcs = EDAC_MCS.write();
    let mc = mcs
        .get_mut(&event.mc_id)
        .ok_or("Memory controller not found")?;

    match event.error_type {
        EdacErrorType::Ce => {
            mc.ce_count += 1;
            if event.csrow < mc.n_csrows && event.channel < mc.n_channels {
                mc.csrow_ce[event.csrow as usize][event.channel as usize] += 1;
            } else {
                mc.ce_noinfo_count += 1;
            }
        }
        EdacErrorType::Ue => {
            mc.ue_count += 1;
            if event.csrow < mc.n_csrows && event.channel < mc.n_channels {
                mc.csrow_ue[event.csrow as usize][event.channel as usize] += 1;
            } else {
                mc.ue_noinfo_count += 1;
            }
        }
    }

    ERROR_LOG.write().push(event);
    Ok(())
}

/// Run a check on a memory controller (poll for errors).
pub fn check_mc(mc_id: u32) -> Result<Vec<MemErrorEvent>, &'static str> {
    let check_fn = {
        let ops = EDAC_OPS.read();
        let mc_ops = ops.get(&mc_id).ok_or("EDAC ops not found")?;
        mc_ops.check
    };
    let events = (check_fn)(mc_id)?;

    for event in &events {
        handle_error(event.clone())?;
    }
    Ok(events)
}

/// Get error counts for a memory controller.
pub fn get_mc_counts(mc_id: u32) -> Result<(u64, u64), &'static str> {
    let mcs = EDAC_MCS.read();
    let mc = mcs.get(&mc_id).ok_or("Memory controller not found")?;
    Ok((mc.ce_count, mc.ue_count))
}

/// Get per-csrow/channel CE counts.
pub fn get_csrow_ce(mc_id: u32) -> Result<Vec<Vec<u64>>, &'static str> {
    let mcs = EDAC_MCS.read();
    let mc = mcs.get(&mc_id).ok_or("Memory controller not found")?;
    Ok(mc.csrow_ce.clone())
}

/// Report a device-level CE error.
pub fn device_inc_ce(dev_id: u32, block_index: u32) -> Result<(), &'static str> {
    let devs = EDAC_DEVICES.read();
    let dev = devs.get(&dev_id).ok_or("EDAC device not found")?;
    dev.ce_count.fetch_add(1, Ordering::SeqCst);
    if let Some(block) = dev.blocks.get(block_index as usize) {
        block.ce_count.fetch_add(1, Ordering::SeqCst);
    }
    Ok(())
}

/// Report a device-level UE error.
pub fn device_inc_ue(dev_id: u32, block_index: u32) -> Result<(), &'static str> {
    let devs = EDAC_DEVICES.read();
    let dev = devs.get(&dev_id).ok_or("EDAC device not found")?;
    dev.ue_count.fetch_add(1, Ordering::SeqCst);
    if let Some(block) = dev.blocks.get(block_index as usize) {
        block.ue_count.fetch_add(1, Ordering::SeqCst);
    }
    Ok(())
}

/// Get recent error log entries.
pub fn get_error_log(max_entries: usize) -> Vec<MemErrorEvent> {
    let log = ERROR_LOG.read();
    let start = if log.len() > max_entries {
        log.len() - max_entries
    } else {
        0
    };
    log[start..].to_vec()
}

/// List all registered memory controllers.
pub fn list_mcs() -> Vec<(u32, String, String)> {
    EDAC_MCS
        .read()
        .iter()
        .map(|(id, mc)| (*id, mc.name.clone(), mc.mc_name.clone()))
        .collect()
}

/// List all registered EDAC devices.
pub fn list_devices() -> Vec<(u32, String, String)> {
    EDAC_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_name.clone()))
        .collect()
}

// ── Software EDAC (no errors) ───────────────────────────────────────────

fn sw_init(_mc_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_check(_mc_id: u32) -> Result<Vec<MemErrorEvent>, &'static str> {
    Ok(Vec::new())
}
fn sw_clear(_mc_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software EDAC ops (no errors reported).
pub fn software_edac_ops() -> EdacOps {
    EdacOps {
        init: sw_init,
        check: sw_check,
        clear: sw_clear,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_edac_ops();
    register_mc("sw-mc0", "sw-edac", 2, 2, ops)?;

    let block_names: &[&str] = &["L1", "L2", "L3"];
    let _ = block_names;
    crate::serial_println!("edac: subsystem ready");
    return Ok(());

    Ok(())
}
