//! CoreSight subsystem
//!
//! Provides framework for ARM CoreSight hardware tracing and debug.
//! Mirrors Linux's `drivers/hwtracing/coresight/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CoreSight device (Linux `struct coresight_device`).
pub struct CoresightDevice {
    pub id: u32,
    pub name: String,
    pub dev_type: CoresightDevType,
    pub subtype: u8,
    pub access: CoresightAccess,
    pub state: CoresightState,
    pub ops: CoresightOps,
    pub ref_count: u32,
    pub cpu: u32,
    pub connections: Vec<u32>,
}

/// CoreSight device type (Linux `enum coresight_dev_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoresightDevType {
    Source, // ETM/PTM/STM
    Sink,   // ETF/ETR/ETB
    Link,   // Replicator/Funnel
    Misc,   // TPIU/TMC
}

/// CoreSight access method (Linux `enum coresight_access`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoresightAccess {
    Mmio,
    Memory,
    System,
}

/// CoreSight state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoresightState {
    Unregistered,
    Registered,
    Enabled,
    Disabled,
}

/// CoreSight operations (Linux `struct coresight_ops`).
pub struct CoresightOps {
    pub enable: fn(dev_id: u32, mode: CoresightMode) -> Result<(), &'static str>,
    pub disable: fn(dev_id: u32) -> Result<(), &'static str>,
    pub config: fn(dev_id: u32, config: &CoresightConfig) -> Result<(), &'static str>,
    pub read: fn(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub reset: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// CoreSight trace mode (Linux `enum coresight_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoresightMode {
    Disabled,
    Sys,
    Perf,
    Cpu,
}

/// CoreSight configuration (Linux `struct coresight_config`).
#[derive(Debug, Clone)]
pub struct CoresightConfig {
    pub mode: CoresightMode,
    pub cpu: u32,
    pub trace_id: u32,
    pub cycle_acc: bool,
    pub data_size: u32,
    pub flags: u32,
}

/// CoreSight trace path (Linux `struct coresight_path`).
pub struct CoresightPath {
    pub id: u32,
    pub source_id: u32,
    pub sink_id: u32,
    pub link_ids: Vec<u32>,
    pub enabled: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PATH_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CS_DEVS: RwLock<BTreeMap<u32, CoresightDevice>> = RwLock::new(BTreeMap::new());
static CS_PATHS: RwLock<BTreeMap<u32, CoresightPath>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a CoreSight device (Linux `coresight_register`).
pub fn register_device(
    name: &str,
    dev_type: CoresightDevType,
    subtype: u8,
    access: CoresightAccess,
    cpu: u32,
    ops: CoresightOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = CoresightDevice {
        id,
        name: String::from(name),
        dev_type,
        subtype,
        access,
        state: CoresightState::Registered,
        ops,
        ref_count: 0,
        cpu,
        connections: Vec::new(),
    };
    CS_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Connect two CoreSight devices (Linux `coresight_add_link`).
pub fn connect_devices(from_id: u32, to_id: u32) -> Result<(), &'static str> {
    let mut devs = CS_DEVS.write();
    let from = devs
        .get_mut(&from_id)
        .ok_or("CoreSight source device not found")?;
    from.connections.push(to_id);
    Ok(())
}

/// Enable a CoreSight device (Linux `coresight_enable_device`).
pub fn enable_device(dev_id: u32, mode: CoresightMode) -> Result<(), &'static str> {
    let enable_fn = {
        let devs = CS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CoreSight device not found")?;
        dev.ops.enable
    };
    (enable_fn)(dev_id, mode)?;

    let mut devs = CS_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CoresightState::Enabled;
        dev.ref_count += 1;
    }
    Ok(())
}

/// Disable a CoreSight device (Linux `coresight_disable_device`).
pub fn disable_device(dev_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let devs = CS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CoreSight device not found")?;
        dev.ops.disable
    };
    (disable_fn)(dev_id)?;

    let mut devs = CS_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.ref_count = dev.ref_count.saturating_sub(1);
        if dev.ref_count == 0 {
            dev.state = CoresightState::Disabled;
        }
    }
    Ok(())
}

/// Configure a CoreSight device (Linux `coresight_config_device`).
pub fn config_device(dev_id: u32, config: &CoresightConfig) -> Result<(), &'static str> {
    let config_fn = {
        let devs = CS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CoreSight device not found")?;
        dev.ops.config
    };
    (config_fn)(dev_id, config)
}

/// Read trace data (Linux `coresight_read_device`).
pub fn read_trace(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let devs = CS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CoreSight device not found")?;
        if dev.dev_type != CoresightDevType::Sink {
            return Err("Can only read from sink devices");
        }
        dev.ops.read
    };
    (read_fn)(dev_id, buf)
}

/// Reset a CoreSight device.
pub fn reset_device(dev_id: u32) -> Result<(), &'static str> {
    let reset_fn = {
        let devs = CS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CoreSight device not found")?;
        dev.ops.reset
    };
    (reset_fn)(dev_id)
}

/// Build a trace path from source to sink (Linux `coresight_build_path`).
pub fn build_path(source_id: u32, sink_id: u32) -> Result<u32, &'static str> {
    // Find path through connections
    let mut link_ids = Vec::new();
    let mut current = source_id;
    let mut visited = Vec::new();

    loop {
        if current == sink_id {
            break;
        }
        visited.push(current);

        let next = {
            let devs = CS_DEVS.read();
            let dev = devs
                .get(&current)
                .ok_or("CoreSight device not found in path")?;
            if dev.connections.is_empty() {
                return Err("No path to sink");
            }
            dev.connections[0]
        };

        if visited.contains(&next) {
            return Err("Cycle detected in path");
        }

        if next != sink_id {
            link_ids.push(next);
        }
        current = next;
    }

    let path_id = PATH_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = CoresightPath {
        id: path_id,
        source_id,
        sink_id,
        link_ids,
        enabled: false,
    };
    CS_PATHS.write().insert(path_id, path);
    Ok(path_id)
}

/// Enable a trace path (Linux `coresight_enable_path`).
pub fn enable_path(path_id: u32, mode: CoresightMode) -> Result<(), &'static str> {
    let (source_id, sink_id, link_ids) = {
        let paths = CS_PATHS.read();
        let path = paths.get(&path_id).ok_or("CoreSight path not found")?;
        (path.source_id, path.sink_id, path.link_ids.clone())
    };

    // Enable sink first
    enable_device(sink_id, mode)?;

    // Enable links
    for &link_id in &link_ids {
        enable_device(link_id, mode)?;
    }

    // Enable source last
    enable_device(source_id, mode)?;

    let mut paths = CS_PATHS.write();
    if let Some(path) = paths.get_mut(&path_id) {
        path.enabled = true;
    }
    Ok(())
}

/// Disable a trace path (Linux `coresight_disable_path`).
pub fn disable_path(path_id: u32) -> Result<(), &'static str> {
    let (source_id, sink_id, link_ids) = {
        let paths = CS_PATHS.read();
        let path = paths.get(&path_id).ok_or("CoreSight path not found")?;
        (path.source_id, path.sink_id, path.link_ids.clone())
    };

    // Disable source first
    disable_device(source_id)?;

    // Disable links
    for &link_id in &link_ids {
        disable_device(link_id)?;
    }

    // Disable sink last
    disable_device(sink_id)?;

    let mut paths = CS_PATHS.write();
    if let Some(path) = paths.get_mut(&path_id) {
        path.enabled = false;
    }
    Ok(())
}

/// List all CoreSight devices.
pub fn list_devices() -> Vec<(u32, String, CoresightDevType, CoresightState, u32)> {
    CS_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.state, d.ref_count))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    CS_DEVS.read().len()
}

// ── Software CoreSight ──────────────────────────────────────────────────

fn sw_enable(_dev_id: u32, _mode: CoresightMode) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_config(_dev_id: u32, _config: &CoresightConfig) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read(_dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_reset(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software CoreSight ops.
pub fn software_coresight_ops() -> CoresightOps {
    CoresightOps {
        enable: sw_enable,
        disable: sw_disable,
        config: sw_config,
        read: sw_read,
        reset: sw_reset,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register an ETM (source)
    let etm = register_device(
        "sw-etm0",
        CoresightDevType::Source,
        0,
        CoresightAccess::Mmio,
        0,
        software_coresight_ops(),
    )?;

    // Register a funnel (link)
    let funnel = register_device(
        "sw-funnel0",
        CoresightDevType::Link,
        0,
        CoresightAccess::Mmio,
        0,
        software_coresight_ops(),
    )?;

    // Register an ETR (sink)
    let etr = register_device(
        "sw-etr0",
        CoresightDevType::Sink,
        0,
        CoresightAccess::Mmio,
        0,
        software_coresight_ops(),
    )?;

    // Connect ETM -> Funnel -> ETR
    connect_devices(etm, funnel)?;
    connect_devices(funnel, etr)?;

    // Build a trace path
    let path_id = build_path(etm, etr)?;

    // Configure the source
    let config = CoresightConfig {
        mode: CoresightMode::Sys,
        cpu: 0,
        trace_id: 0x10,
        cycle_acc: false,
        data_size: 4096,
        flags: 0,
    };
    config_device(etm, &config)?;

    // Enable the path
    enable_path(path_id, CoresightMode::Sys)?;

    // Read trace data from sink
    let mut buf = [0u8; 4096];
    let n = read_trace(etr, &mut buf)?;
    if n == 0 {
        return Err("CoreSight: no trace data");
    }

    // Disable the path
    disable_path(path_id)?;

    // Reset all devices
    reset_device(etm)?;
    reset_device(funnel)?;
    reset_device(etr)?;

    Ok(())
}
