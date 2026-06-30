//! SLIMbus subsystem
//!
//! Provides SLIMbus (Serial Low-power Inter-chip Media Bus) framework for
//! audio and baseband chip communication. Mirrors Linux's `drivers/slimbus/slim-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SLIMbus device (Linux `struct slim_device`).
pub struct SlimDevice {
    pub id: u32,
    pub ctrl_id: u32,
    pub name: String,
    pub e_addr: [u8; 6], // Enumeration address
    pub l_addr: u8,      // Logical address
    pub dev_type: u8,
    pub state: SlimDevState,
    pub driver_name: Option<String>,
}

/// SLIMbus device state (Linux `enum slim_device_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimDevState {
    Unknown,
    Down,
    Awake,
    Active,
    Sleep,
}

/// SLIMbus controller (Linux `struct slim_controller`).
pub struct SlimController {
    pub id: u32,
    pub name: String,
    pub ops: SlimCtrlOps,
    pub device_ids: Vec<u32>,
    pub max_channels: u32,
    pub framer: Option<u32>,
    pub active: bool,
}

/// SLIMbus controller operations (Linux `struct slim_controller_ops`).
pub struct SlimCtrlOps {
    pub xfer_msg: fn(ctrl_id: u32, msg: &SlimMsg) -> Result<(), &'static str>,
    pub boot: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub power_up: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub power_down: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub enable_stream:
        fn(ctrl_id: u32, stream_id: u32, channel: u32, rate: u32) -> Result<(), &'static str>,
    pub disable_stream: fn(ctrl_id: u32, stream_id: u32) -> Result<(), &'static str>,
}

/// SLIMbus message (Linux `struct slim_val_inf`).
#[derive(Debug, Clone)]
pub struct SlimMsg {
    pub start_offset: u32,
    pub num_bytes: u32,
    pub destination: u8,
    pub source: u8,
    pub data: Vec<u8>,
    pub read: bool,
}

/// SLIMbus stream (Linux `struct slim_stream_runtime`).
pub struct SlimStream {
    pub id: u32,
    pub ctrl_id: u32,
    pub name: String,
    pub channel: u32,
    pub rate: u32,
    pub direction: SlimStreamDir,
    pub active: bool,
}

/// Stream direction (Linux `enum slim_stream_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlimStreamDir {
    Playback,
    Capture,
    Duplex,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SLIM_CTRLS: RwLock<BTreeMap<u32, SlimController>> = RwLock::new(BTreeMap::new());
static SLIM_DEVICES: RwLock<BTreeMap<u32, SlimDevice>> = RwLock::new(BTreeMap::new());
static SLIM_STREAMS: RwLock<BTreeMap<u32, SlimStream>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a SLIMbus controller.
pub fn register_controller(
    name: &str,
    ops: SlimCtrlOps,
    max_channels: u32,
) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = SlimController {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        max_channels,
        framer: None,
        active: false,
    };
    SLIM_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Boot a SLIMbus controller (Linux `slim_ctrl_clk_pause` + boot).
pub fn boot_controller(ctrl_id: u32) -> Result<(), &'static str> {
    let boot_fn = {
        let ctrls = SLIM_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("SLIMbus controller not found")?;
        ctrl.ops.boot
    };
    (boot_fn)(ctrl_id)?;

    let mut ctrls = SLIM_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.active = true;
    }
    Ok(())
}

/// Register a device on a SLIMbus controller.
pub fn register_device(
    ctrl_id: u32,
    name: &str,
    e_addr: [u8; 6],
    dev_type: u8,
) -> Result<u32, &'static str> {
    let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = SlimDevice {
        id: dev_id,
        ctrl_id,
        name: String::from(name),
        e_addr,
        l_addr: 0,
        dev_type,
        state: SlimDevState::Down,
        driver_name: None,
    };
    SLIM_DEVICES.write().insert(dev_id, dev);

    let mut ctrls = SLIM_CTRLS.write();
    if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
        ctrl.device_ids.push(dev_id);
    }
    Ok(dev_id)
}

/// Assign a logical address to a device (Linux `slim_get_logical_addr`).
pub fn assign_logical_addr(device_id: u32, l_addr: u8) -> Result<(), &'static str> {
    let mut devices = SLIM_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("SLIMbus device not found")?;
    dev.l_addr = l_addr;
    dev.state = SlimDevState::Awake;
    Ok(())
}

/// Transfer a SLIMbus message.
pub fn xfer_msg(ctrl_id: u32, msg: &SlimMsg) -> Result<(), &'static str> {
    let xfer_fn = {
        let ctrls = SLIM_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("SLIMbus controller not found")?;
        ctrl.ops.xfer_msg
    };
    (xfer_fn)(ctrl_id, msg)
}

/// Create a SLIMbus stream.
pub fn create_stream(
    ctrl_id: u32,
    name: &str,
    channel: u32,
    rate: u32,
    direction: SlimStreamDir,
) -> Result<u32, &'static str> {
    let stream_id = STREAM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let stream = SlimStream {
        id: stream_id,
        ctrl_id,
        name: String::from(name),
        channel,
        rate,
        direction,
        active: false,
    };
    SLIM_STREAMS.write().insert(stream_id, stream);
    Ok(stream_id)
}

/// Enable a SLIMbus stream.
pub fn enable_stream(stream_id: u32) -> Result<(), &'static str> {
    let (ctrl_id, channel, rate) = {
        let streams = SLIM_STREAMS.read();
        let stream = streams.get(&stream_id).ok_or("SLIMbus stream not found")?;
        (stream.ctrl_id, stream.channel, stream.rate)
    };

    let enable_fn = {
        let ctrls = SLIM_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("SLIMbus controller not found")?;
        ctrl.ops.enable_stream
    };
    (enable_fn)(ctrl_id, stream_id, channel, rate)?;

    let mut streams = SLIM_STREAMS.write();
    if let Some(stream) = streams.get_mut(&stream_id) {
        stream.active = true;
    }
    Ok(())
}

/// Disable a SLIMbus stream.
pub fn disable_stream(stream_id: u32) -> Result<(), &'static str> {
    let ctrl_id = {
        let streams = SLIM_STREAMS.read();
        let stream = streams.get(&stream_id).ok_or("SLIMbus stream not found")?;
        stream.ctrl_id
    };

    let disable_fn = {
        let ctrls = SLIM_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("SLIMbus controller not found")?;
        ctrl.ops.disable_stream
    };
    (disable_fn)(ctrl_id, stream_id)?;

    let mut streams = SLIM_STREAMS.write();
    if let Some(stream) = streams.get_mut(&stream_id) {
        stream.active = false;
    }
    Ok(())
}

/// List all SLIMbus controllers.
pub fn list_controllers() -> Vec<(u32, String, bool)> {
    SLIM_CTRLS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.active))
        .collect()
}

/// List devices on a controller.
pub fn list_devices(ctrl_id: u32) -> Result<Vec<(u32, String, u8, SlimDevState)>, &'static str> {
    let ctrls = SLIM_CTRLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("SLIMbus controller not found")?;
    let devices = SLIM_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &ctrl.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.name.clone(), dev.l_addr, dev.state));
        }
    }
    Ok(result)
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    SLIM_CTRLS.read().len()
}

// ── Software SLIMbus ────────────────────────────────────────────────────

fn sw_xfer_msg(_ctrl_id: u32, _msg: &SlimMsg) -> Result<(), &'static str> {
    Ok(())
}
fn sw_boot(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_power_up(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_power_down(_ctrl_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable_stream(
    _ctrl_id: u32,
    _stream_id: u32,
    _channel: u32,
    _rate: u32,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable_stream(_ctrl_id: u32, _stream_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software SLIMbus controller ops.
pub fn software_slimbus_ops() -> SlimCtrlOps {
    SlimCtrlOps {
        xfer_msg: sw_xfer_msg,
        boot: sw_boot,
        power_up: sw_power_up,
        power_down: sw_power_down,
        enable_stream: sw_enable_stream,
        disable_stream: sw_disable_stream,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !SLIM_CTRLS.read().is_empty() {
        return Ok(());
    }

    let ops = software_slimbus_ops();
    let ctrl_id = register_controller("sw-slimbus", ops, 32)?;
    boot_controller(ctrl_id)?;
    crate::serial_println!(
        "slimbus: software controller registered and booted (id={})",
        ctrl_id
    );
    Ok(())
}
