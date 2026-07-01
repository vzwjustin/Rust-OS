//! SPMI (System Power Management Interface) bus subsystem
//!
//! Provides SPMI bus framework for PMIC (Power Management IC) communication.
//! Mirrors Linux's `drivers/spmi/spmi.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SPMI device (Linux `struct spmi_device`).
pub struct SpmiDevice {
    pub id: u32,
    pub bus_id: u32,
    pub name: String,
    pub sid: u8, // Slave ID
    pub dev_type: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// SPMI controller operations (Linux `struct spmi_controller_ops`).
pub struct SpmiCtrlOps {
    pub read_cmd:
        fn(ctrl_id: u32, sid: u8, addr: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write_cmd: fn(ctrl_id: u32, sid: u8, addr: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub register_read:
        fn(ctrl_id: u32, sid: u8, addr: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub register_write:
        fn(ctrl_id: u32, sid: u8, addr: u32, data: &[u8]) -> Result<usize, &'static str>,
}

/// SPMI controller (Linux `struct spmi_controller`).
pub struct SpmiController {
    pub id: u32,
    pub name: String,
    pub ops: SpmiCtrlOps,
    pub num_devices: u32,
    pub device_ids: Vec<u32>,
}

/// SPMI driver (Linux `struct spmi_driver`).
pub struct SpmiDriver {
    pub name: String,
    pub id_table: Vec<SpmiDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// SPMI device ID (Linux `struct spmi_device_id`).
#[derive(Debug, Clone)]
pub struct SpmiDeviceId {
    pub name: String,
    pub sid: u8,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SPMI_CONTROLLERS: RwLock<BTreeMap<u32, SpmiController>> = RwLock::new(BTreeMap::new());
static SPMI_DEVICES: RwLock<BTreeMap<u32, SpmiDevice>> = RwLock::new(BTreeMap::new());
static SPMI_DRIVERS: RwLock<BTreeMap<u32, SpmiDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an SPMI controller.
pub fn register_controller(
    name: &str,
    ops: SpmiCtrlOps,
    num_devices: u32,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("SPMI controller name is empty");
    }
    if num_devices == 0 {
        return Err("SPMI controller has no device slots");
    }

    let mut ctrls = SPMI_CONTROLLERS.write();
    if ctrls.values().any(|ctrl| ctrl.name == name) {
        return Err("SPMI controller already registered");
    }

    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = SpmiController {
        id,
        name: String::from(name),
        ops,
        num_devices,
        device_ids: Vec::new(),
    };
    ctrls.insert(id, ctrl);
    Ok(id)
}

/// Register an SPMI device on a controller.
pub fn register_device(
    bus_id: u32,
    name: &str,
    sid: u8,
    dev_type: u32,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("SPMI device name is empty");
    }
    {
        let ctrls = SPMI_CONTROLLERS.read();
        let ctrl = ctrls.get(&bus_id).ok_or("SPMI controller not found")?;
        if sid as u32 >= ctrl.num_devices || sid > 0x0f {
            return Err("SPMI slave id out of range");
        }
    }
    {
        let devices = SPMI_DEVICES.read();
        if devices
            .values()
            .any(|dev| dev.bus_id == bus_id && (dev.sid == sid || dev.name == name))
        {
            return Err("SPMI device already registered");
        }
    }

    let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = SpmiDevice {
        id: dev_id,
        bus_id,
        name: String::from(name),
        sid,
        dev_type,
        driver_name: None,
        bound: false,
    };
    SPMI_DEVICES.write().insert(dev_id, dev);

    let mut ctrls = SPMI_CONTROLLERS.write();
    if let Some(ctrl) = ctrls.get_mut(&bus_id) {
        ctrl.device_ids.push(dev_id);
    } else {
        SPMI_DEVICES.write().remove(&dev_id);
        return Err("SPMI controller not found");
    }

    // Try to match with existing drivers
    if let Err(err) = try_match_driver(dev_id) {
        SPMI_DEVICES.write().remove(&dev_id);
        let mut ctrls = SPMI_CONTROLLERS.write();
        if let Some(ctrl) = ctrls.get_mut(&bus_id) {
            ctrl.device_ids.retain(|id| *id != dev_id);
        }
        return Err(err);
    }
    Ok(dev_id)
}

/// Register an SPMI driver.
pub fn register_driver(driver: SpmiDriver) -> Result<u32, &'static str> {
    if driver.name.is_empty() {
        return Err("SPMI driver name is empty");
    }
    if driver.id_table.is_empty() {
        return Err("SPMI driver id table is empty");
    }

    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv_name = driver.name.clone();
    {
        let mut drivers = SPMI_DRIVERS.write();
        if drivers.values().any(|drv| drv.name == drv_name) {
            return Err("SPMI driver already registered");
        }
        drivers.insert(id, driver);
    }

    // Try to match with existing devices
    let device_ids: Vec<u32> = {
        let devices = SPMI_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound)
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        if let Err(err) = try_match_driver(dev_id) {
            SPMI_DRIVERS.write().remove(&id);
            let mut devices = SPMI_DEVICES.write();
            for dev in devices.values_mut() {
                if dev.driver_name.as_deref() == Some(drv_name.as_str()) {
                    dev.bound = false;
                    dev.driver_name = None;
                }
            }
            return Err(err);
        }
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = SPMI_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let dev_name = dev.name.clone();
        let dev_sid = dev.sid;

        let drivers = SPMI_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id_entry in &drv.id_table {
                if dev_name == id_entry.name && (id_entry.sid == 0xFF || id_entry.sid == dev_sid) {
                    found = Some((drv.probe, drv.name.clone()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };

    if let Some((probe_fn, drv_name)) = matched {
        (probe_fn)(device_id)?;
        let mut devices = SPMI_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Read registers from an SPMI device.
pub fn register_read(device_id: u32, addr: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    if buf.is_empty() {
        return Err("SPMI read buffer is empty");
    }
    if buf.len() > 16 {
        return Err("SPMI read length too large");
    }
    if addr > 0xffff {
        return Err("SPMI register address out of range");
    }

    let (ctrl_id, sid, read_fn) = {
        let devices = SPMI_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("SPMI device not found")?;
        let ctrls = SPMI_CONTROLLERS.read();
        let ctrl = ctrls.get(&dev.bus_id).ok_or("SPMI controller not found")?;
        (dev.bus_id, dev.sid, ctrl.ops.register_read)
    };
    (read_fn)(ctrl_id, sid, addr, buf)
}

/// Write registers to an SPMI device.
pub fn register_write(device_id: u32, addr: u32, data: &[u8]) -> Result<usize, &'static str> {
    if data.is_empty() {
        return Err("SPMI write data is empty");
    }
    if data.len() > 16 {
        return Err("SPMI write length too large");
    }
    if addr > 0xffff {
        return Err("SPMI register address out of range");
    }

    let (ctrl_id, sid, write_fn) = {
        let devices = SPMI_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("SPMI device not found")?;
        let ctrls = SPMI_CONTROLLERS.read();
        let ctrl = ctrls.get(&dev.bus_id).ok_or("SPMI controller not found")?;
        (dev.bus_id, dev.sid, ctrl.ops.register_write)
    };
    (write_fn)(ctrl_id, sid, addr, data)
}

/// List all SPMI controllers.
pub fn list_controllers() -> Vec<(u32, String, u32)> {
    SPMI_CONTROLLERS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.num_devices))
        .collect()
}

/// List devices on a controller.
pub fn list_devices(bus_id: u32) -> Result<Vec<(u32, String, u8, bool)>, &'static str> {
    let ctrls = SPMI_CONTROLLERS.read();
    let ctrl = ctrls.get(&bus_id).ok_or("SPMI controller not found")?;
    let devices = SPMI_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &ctrl.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.name.clone(), dev.sid, dev.bound));
        }
    }
    Ok(result)
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    SPMI_CONTROLLERS.read().len()
}

// ── Software SPMI ───────────────────────────────────────────────────────

fn sw_read(_ctrl_id: u32, _sid: u8, _addr: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_write(_ctrl_id: u32, _sid: u8, _addr: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}

/// Software SPMI controller ops.
pub fn software_spmi_ops() -> SpmiCtrlOps {
    SpmiCtrlOps {
        read_cmd: sw_read,
        write_cmd: sw_write,
        register_read: sw_read,
        register_write: sw_write,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    if !SPMI_CONTROLLERS.read().is_empty() {
        return Ok(());
    }

    let ops = software_spmi_ops();
    let ctrl_id = register_controller("sw-spmi", ops, 4)?;
    register_device(ctrl_id, "sw-pmic", 0, 0)?;
    crate::serial_println!("spmi: controller {} registered with PMIC device", ctrl_id);
    Ok(())
}
