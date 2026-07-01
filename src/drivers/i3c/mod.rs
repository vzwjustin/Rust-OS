//! I3C (Improved Inter-Integrated Circuit) bus subsystem
//!
//! Provides I3C bus framework for device discovery, DAA (Dynamic Address Assignment),
//! CCC (Common Command Code) transfers, and IBI (In-Band Interrupt) handling.
//! Mirrors Linux's `drivers/i3c/master.c` and `drivers/i3c/device.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// I3C device status (Linux `enum i3c_dev_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I3cDevState {
    Unknown,
    Init,
    Addressed,
    NoDev,
}

/// I3C device type (Linux `enum i3c_dev_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I3cDevType {
    I3c,
    I2c,
}

/// I3C CCC command direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CccDirection {
    Broadcast,
    Direct,
}

/// I3C CCC command (Linux `struct i3c_ccc_cmd`).
#[derive(Debug, Clone)]
pub struct I3cCccCmd {
    pub id: u8,
    pub direction: CccDirection,
    pub dest: Option<u8>,
    pub payload: Vec<u8>,
    pub rnw: bool,
}

/// I3C device (Linux `struct i3c_device`).
pub struct I3cDevice {
    pub id: u32,
    pub bus_id: u32,
    pub dev_type: I3cDevType,
    pub static_addr: u8,
    pub dynamic_addr: u8,
    pub pid: u64,
    pub state: I3cDevState,
    pub info: I3cDeviceInfo,
}

/// I3C device info (Linux `struct i3c_device_info`).
#[derive(Debug, Clone, Default)]
pub struct I3cDeviceInfo {
    pub pid: u64,
    pub dcr: u8,
    pub bcr: u8,
    pub max_read_ds: u8,
    pub max_write_ds: u8,
    pub max_ibi_len: u8,
}

/// I3C master controller operations (Linux `struct i3c_master_controller_ops`).
pub struct I3cMasterOps {
    pub bus_init: fn(bus_id: u32) -> Result<(), &'static str>,
    pub bus_cleanup: fn(bus_id: u32) -> Result<(), &'static str>,
    pub attach_i3c_dev: fn(bus_id: u32, device_id: u32) -> Result<(), &'static str>,
    pub detach_i3c_dev: fn(bus_id: u32, device_id: u32) -> Result<(), &'static str>,
    pub do_daa: fn(bus_id: u32) -> Result<(), &'static str>,
    pub send_ccc_cmd: fn(bus_id: u32, cmd: &I3cCccCmd) -> Result<Vec<u8>, &'static str>,
    pub priv_xfers:
        fn(bus_id: u32, device_id: u32, xfers: &[I3cPrivXfer]) -> Result<(), &'static str>,
    pub enable_ibi: fn(bus_id: u32, device_id: u32) -> Result<(), &'static str>,
    pub disable_ibi: fn(bus_id: u32, device_id: u32) -> Result<(), &'static str>,
}

/// I3C private transfer (Linux `struct i3c_priv_xfer`).
#[derive(Debug, Clone)]
pub struct I3cPrivXfer {
    pub rnw: bool,
    pub len: u16,
    pub data: Vec<u8>,
}

/// I3C bus (Linux `struct i3c_bus`).
pub struct I3cBus {
    pub id: u32,
    pub name: String,
    pub ops: I3cMasterOps,
    pub device_ids: Vec<u32>,
    pub mode: I3cBusMode,
    pub scl_rate: u32,
}

/// I3C bus mode (Linux `enum i3c_bus_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I3cBusMode {
    Pure,
    MixedFast,
    MixedSlow,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static I3C_BUSES: RwLock<BTreeMap<u32, I3cBus>> = RwLock::new(BTreeMap::new());
static I3C_DEVICES: RwLock<BTreeMap<u32, I3cDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an I3C bus with a master controller.
pub fn register_bus(
    name: &str,
    ops: I3cMasterOps,
    mode: I3cBusMode,
    scl_rate: u32,
) -> Result<u32, &'static str> {
    let init_fn = ops.bus_init;
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = I3cBus {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        mode,
        scl_rate,
    };
    I3C_BUSES.write().insert(id, bus);
    (init_fn)(id)?;
    Ok(id)
}

/// Perform Dynamic Address Assignment (DAA) on a bus.
pub fn do_daa(bus_id: u32) -> Result<(), &'static str> {
    let daa_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.do_daa
    };
    (daa_fn)(bus_id)
}

/// Send a CCC command (Linux `i3c_master_send_ccc_cmd`).
pub fn send_ccc(bus_id: u32, cmd: I3cCccCmd) -> Result<Vec<u8>, &'static str> {
    let ccc_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.send_ccc_cmd
    };
    (ccc_fn)(bus_id, &cmd)
}

/// Register an I3C device on a bus.
pub fn register_device(
    bus_id: u32,
    dev_type: I3cDevType,
    static_addr: u8,
    dynamic_addr: u8,
    pid: u64,
    info: I3cDeviceInfo,
) -> Result<u32, &'static str> {
    if matches!(dev_type, I3cDevType::I3c) && !(0x08..=0x77).contains(&dynamic_addr) {
        return Err("I3C dynamic address out of range");
    }

    {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        let devices = I3C_DEVICES.read();
        for &dev_id in &bus.device_ids {
            if let Some(dev) = devices.get(&dev_id) {
                if matches!(dev_type, I3cDevType::I3c) && dev.dynamic_addr == dynamic_addr {
                    return Err("I3C dynamic address already in use");
                }
                if matches!(dev_type, I3cDevType::I2c) && dev.static_addr == static_addr {
                    return Err("I2C static address already in use");
                }
            }
        }
    }

    let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = I3cDevice {
        id: dev_id,
        bus_id,
        dev_type,
        static_addr,
        dynamic_addr,
        pid,
        state: I3cDevState::Init,
        info,
    };
    I3C_DEVICES.write().insert(dev_id, dev);

    let attach_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.attach_i3c_dev
    };
    if let Err(err) = (attach_fn)(bus_id, dev_id) {
        I3C_DEVICES.write().remove(&dev_id);
        return Err(err);
    }

    let mut buses = I3C_BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.device_ids.push(dev_id);
    }

    let mut devices = I3C_DEVICES.write();
    if let Some(dev) = devices.get_mut(&dev_id) {
        dev.state = I3cDevState::Addressed;
    }
    Ok(dev_id)
}

/// Perform private transfers to an I3C device.
pub fn priv_xfers(bus_id: u32, device_id: u32, xfers: &[I3cPrivXfer]) -> Result<(), &'static str> {
    let xfer_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.priv_xfers
    };
    (xfer_fn)(bus_id, device_id, xfers)
}

/// Enable In-Band Interrupts for a device.
pub fn enable_ibi(bus_id: u32, device_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.enable_ibi
    };
    (enable_fn)(bus_id, device_id)
}

/// Disable In-Band Interrupts for a device.
pub fn disable_ibi(bus_id: u32, device_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let buses = I3C_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
        bus.ops.disable_ibi
    };
    (disable_fn)(bus_id, device_id)
}

/// List all I3C buses.
pub fn list_buses() -> Vec<(u32, String)> {
    I3C_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone()))
        .collect()
}

/// List devices on a bus.
pub fn list_bus_devices(bus_id: u32) -> Result<Vec<(u32, I3cDevType, u8, u64)>, &'static str> {
    let buses = I3C_BUSES.read();
    let bus = buses.get(&bus_id).ok_or("I3C bus not found")?;
    let devices = I3C_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &bus.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.dev_type, dev.dynamic_addr, dev.pid));
        }
    }
    Ok(result)
}

/// Count registered buses.
pub fn bus_count() -> usize {
    I3C_BUSES.read().len()
}

// ── Software I3C ────────────────────────────────────────────────────────

fn sw_bus_init(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_bus_cleanup(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_attach(_bus_id: u32, _dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_detach(_bus_id: u32, _dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_daa(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_ccc(_bus_id: u32, cmd: &I3cCccCmd) -> Result<Vec<u8>, &'static str> {
    Ok(cmd.payload.clone())
}
fn sw_xfers(_bus_id: u32, _dev_id: u32, _xfers: &[I3cPrivXfer]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable_ibi(_bus_id: u32, _dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable_ibi(_bus_id: u32, _dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software I3C master ops.
pub fn software_i3c_ops() -> I3cMasterOps {
    I3cMasterOps {
        bus_init: sw_bus_init,
        bus_cleanup: sw_bus_cleanup,
        attach_i3c_dev: sw_attach,
        detach_i3c_dev: sw_detach,
        do_daa: sw_daa,
        send_ccc_cmd: sw_ccc,
        priv_xfers: sw_xfers,
        enable_ibi: sw_enable_ibi,
        disable_ibi: sw_disable_ibi,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !I3C_BUSES.read().is_empty() {
        return Ok(());
    }

    let ops = software_i3c_ops();
    let bus_id = register_bus("sw-i3c", ops, I3cBusMode::Pure, 12_500_000)?;
    crate::serial_println!("i3c: software bus registered (id={}, 12.5 MHz)", bus_id);
    Ok(())
}
