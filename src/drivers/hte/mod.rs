//! HTE (Hardware Timestamping Engine) subsystem
//!
//! Provides framework for hardware timestamping of GPIO and other events.
//! Mirrors Linux's `drivers/hte/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// HTE device (Linux `struct hte_device`).
pub struct HteDevice {
    pub id: u32,
    pub name: String,
    pub provider_id: u32,
    pub state: HteState,
    pub ops: HteOps,
    pub line_ids: Vec<u32>,
    pub nlines: u32,
}

/// HTE state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HteState {
    Unregistered,
    Registered,
    Enabled,
    Disabled,
}

/// HTE operations (Linux `struct hte_ops`).
pub struct HteOps {
    pub request: fn(dev_id: u32, line_id: u32) -> Result<(), &'static str>,
    pub release: fn(dev_id: u32, line_id: u32) -> Result<(), &'static str>,
    pub enable: fn(dev_id: u32, line_id: u32) -> Result<(), &'static str>,
    pub disable: fn(dev_id: u32, line_id: u32) -> Result<(), &'static str>,
    pub set_edge: fn(dev_id: u32, line_id: u32, edge: HteEdge) -> Result<(), &'static str>,
    pub get_clk_src: fn(dev_id: u32) -> Result<HteClkSrc, &'static str>,
    pub retrieve_ts: fn(dev_id: u32, line_id: u32) -> Result<Vec<HteEvent>, &'static str>,
}

/// HTE edge type (Linux `enum hte_edge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HteEdge {
    Rising,
    Falling,
    Both,
}

/// HTE clock source (Linux `enum hte_clk_src`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HteClkSrc {
    Local,
    Global,
    Tsc,
    Mono,
}

/// HTE timestamp event (Linux `struct hte_ts_data`).
#[derive(Debug, Clone)]
pub struct HteEvent {
    pub line_id: u32,
    pub timestamp_ns: u64,
    pub edge: HteEdge,
    pub seq: u64,
}

/// HTE line (Linux `struct hte_ts_desc`).
pub struct HteLine {
    pub id: u32,
    pub dev_id: u32,
    pub line_id: u32,
    pub name: String,
    pub edge: HteEdge,
    pub state: HteLineState,
    pub event_count: u64,
    pub last_timestamp: u64,
}

/// HTE line state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HteLineState {
    Free,
    Requested,
    Enabled,
    Disabled,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static LINE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HTE_DEVS: RwLock<BTreeMap<u32, HteDevice>> = RwLock::new(BTreeMap::new());
static HTE_LINES: RwLock<BTreeMap<u32, HteLine>> = RwLock::new(BTreeMap::new());
static HTE_EVENTS: RwLock<Vec<HteEvent>> = RwLock::new(Vec::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an HTE device (Linux `hte_register`).
pub fn register_device(
    name: &str,
    provider_id: u32,
    nlines: u32,
    ops: HteOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = HteDevice {
        id,
        name: String::from(name),
        provider_id,
        state: HteState::Registered,
        ops,
        line_ids: Vec::new(),
        nlines,
    };
    HTE_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Request a line for timestamping (Linux `hte_request_ts_ns`).
pub fn request_line(
    dev_id: u32,
    line_id: u32,
    name: &str,
    edge: HteEdge,
) -> Result<u32, &'static str> {
    let request_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        if line_id >= dev.nlines {
            return Err("Line ID out of range");
        }
        dev.ops.request
    };
    (request_fn)(dev_id, line_id)?;

    let id = LINE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let line = HteLine {
        id,
        dev_id,
        line_id,
        name: String::from(name),
        edge,
        state: HteLineState::Requested,
        event_count: 0,
        last_timestamp: 0,
    };
    HTE_LINES.write().insert(id, line);

    let mut devs = HTE_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.line_ids.push(id);
    }
    Ok(id)
}

/// Release a line (Linux `hte_release_ts`).
pub fn release_line(line_handle: u32) -> Result<(), &'static str> {
    let (dev_id, line_id) = {
        let lines = HTE_LINES.read();
        let line = lines.get(&line_handle).ok_or("HTE line not found")?;
        (line.dev_id, line.line_id)
    };

    let release_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.release
    };
    (release_fn)(dev_id, line_id)?;

    HTE_LINES.write().remove(&line_handle);

    let mut devs = HTE_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.line_ids.retain(|&id| id != line_handle);
    }
    Ok(())
}

/// Enable timestamping on a line (Linux `hte_enable_ts`).
pub fn enable_line(line_handle: u32) -> Result<(), &'static str> {
    let (dev_id, line_id) = {
        let lines = HTE_LINES.read();
        let line = lines.get(&line_handle).ok_or("HTE line not found")?;
        (line.dev_id, line.line_id)
    };

    let enable_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.enable
    };
    (enable_fn)(dev_id, line_id)?;

    let mut lines = HTE_LINES.write();
    if let Some(line) = lines.get_mut(&line_handle) {
        line.state = HteLineState::Enabled;
    }
    Ok(())
}

/// Disable timestamping on a line (Linux `hte_disable_ts`).
pub fn disable_line(line_handle: u32) -> Result<(), &'static str> {
    let (dev_id, line_id) = {
        let lines = HTE_LINES.read();
        let line = lines.get(&line_handle).ok_or("HTE line not found")?;
        (line.dev_id, line.line_id)
    };

    let disable_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.disable
    };
    (disable_fn)(dev_id, line_id)?;

    let mut lines = HTE_LINES.write();
    if let Some(line) = lines.get_mut(&line_handle) {
        line.state = HteLineState::Disabled;
    }
    Ok(())
}

/// Set edge type for a line (Linux `hte_set_edge`).
pub fn set_edge(line_handle: u32, edge: HteEdge) -> Result<(), &'static str> {
    let (dev_id, line_id) = {
        let lines = HTE_LINES.read();
        let line = lines.get(&line_handle).ok_or("HTE line not found")?;
        (line.dev_id, line.line_id)
    };

    let set_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.set_edge
    };
    (set_fn)(dev_id, line_id, edge)?;

    let mut lines = HTE_LINES.write();
    if let Some(line) = lines.get_mut(&line_handle) {
        line.edge = edge;
    }
    Ok(())
}

/// Retrieve timestamp events (Linux `hte_retrieve_ts_ns`).
pub fn retrieve_events(line_handle: u32) -> Result<Vec<HteEvent>, &'static str> {
    let (dev_id, line_id) = {
        let lines = HTE_LINES.read();
        let line = lines.get(&line_handle).ok_or("HTE line not found")?;
        (line.dev_id, line.line_id)
    };

    let retrieve_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.retrieve_ts
    };
    let events = (retrieve_fn)(dev_id, line_id)?;

    let mut lines = HTE_LINES.write();
    if let Some(line) = lines.get_mut(&line_handle) {
        line.event_count += events.len() as u64;
        if let Some(last) = events.last() {
            line.last_timestamp = last.timestamp_ns;
        }
    }

    // Log events
    let mut log = HTE_EVENTS.write();
    for ev in &events {
        log.push(ev.clone());
    }
    if log.len() > 4096 {
        let drain = log.len() - 4096;
        log.drain(0..drain);
    }

    Ok(events)
}

/// Get clock source (Linux `hte_get_clk_src`).
pub fn get_clk_src(dev_id: u32) -> Result<HteClkSrc, &'static str> {
    let get_fn = {
        let devs = HTE_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HTE device not found")?;
        dev.ops.get_clk_src
    };
    (get_fn)(dev_id)
}

/// List all HTE devices.
pub fn list_devices() -> Vec<(u32, String, HteState, u32, usize)> {
    HTE_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state, d.nlines, d.line_ids.len()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    HTE_DEVS.read().len()
}

// ── Software HTE ────────────────────────────────────────────────────────

static SW_TS_COUNTER: AtomicU32 = AtomicU32::new(1000);

fn sw_request(_dev_id: u32, _line_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_dev_id: u32, _line_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable(_dev_id: u32, _line_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_dev_id: u32, _line_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_edge(_dev_id: u32, _line_id: u32, _edge: HteEdge) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_clk_src(_dev_id: u32) -> Result<HteClkSrc, &'static str> {
    Ok(HteClkSrc::Mono)
}
fn sw_retrieve_ts(_dev_id: u32, line_id: u32) -> Result<Vec<HteEvent>, &'static str> {
    let ts = SW_TS_COUNTER.fetch_add(100_000, Ordering::SeqCst) as u64;
    Ok(alloc::vec![
        HteEvent {
            line_id,
            timestamp_ns: ts,
            edge: HteEdge::Rising,
            seq: 1
        },
        HteEvent {
            line_id,
            timestamp_ns: ts + 50_000,
            edge: HteEdge::Falling,
            seq: 2
        },
        HteEvent {
            line_id,
            timestamp_ns: ts + 100_000,
            edge: HteEdge::Rising,
            seq: 3
        },
    ])
}

/// Software HTE ops.
pub fn software_hte_ops() -> HteOps {
    HteOps {
        request: sw_request,
        release: sw_release,
        enable: sw_enable,
        disable: sw_disable,
        set_edge: sw_set_edge,
        get_clk_src: sw_get_clk_src,
        retrieve_ts: sw_retrieve_ts,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("hte: subsystem ready");
    Ok(())
}
