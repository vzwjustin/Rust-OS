//! MOXTET subsystem
//!
//! Provides Turris Moxtet topology bus for modular switch/router expansion.
//! Mirrors Linux's `drivers/bus/moxtet.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Moxtet module type (Linux `enum moxtet_module_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoxtetModuleType {
    Unknown,
    Pci,
    Usb3,
    Peridot,
    Topaz,
    Sfp,
    Symphony,
}

impl MoxtetModuleType {
    pub fn from_id(id: u8) -> Self {
        match id {
            0x01 => MoxtetModuleType::Pci,
            0x02 => MoxtetModuleType::Usb3,
            0x03 => MoxtetModuleType::Peridot,
            0x04 => MoxtetModuleType::Topaz,
            0x05 => MoxtetModuleType::Sfp,
            0x06 => MoxtetModuleType::Symphony,
            _ => MoxtetModuleType::Unknown,
        }
    }
}

/// Moxtet device (Linux `struct moxtet_device`).
pub struct MoxtetDevice {
    pub id: u32,
    pub bus_id: u32,
    pub module_id: u8,
    pub module_type: MoxtetModuleType,
    pub name: String,
    pub position: u8,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// Moxtet driver (Linux `struct moxtet_driver`).
pub struct MoxtetDriver {
    pub name: String,
    pub id_table: Vec<MoxtetDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// Moxtet device ID (Linux `struct moxtet_device_id`).
#[derive(Debug, Clone)]
pub struct MoxtetDeviceId {
    pub module_id: u8,
}

/// Moxtet bus (Linux `struct moxtet`).
pub struct MoxtetBus {
    pub id: u32,
    pub name: String,
    pub ops: MoxtetBusOps,
    pub device_ids: Vec<u32>,
    pub module_count: u8,
}

/// Moxtet bus operations.
pub struct MoxtetBusOps {
    pub read: fn(bus_id: u32, position: u8, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(bus_id: u32, position: u8, data: &[u8]) -> Result<usize, &'static str>,
    pub detect_modules: fn(bus_id: u32) -> Result<Vec<u8>, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MOXTET_BUSES: RwLock<BTreeMap<u32, MoxtetBus>> = RwLock::new(BTreeMap::new());
static MOXTET_DEVICES: RwLock<BTreeMap<u32, MoxtetDevice>> = RwLock::new(BTreeMap::new());
static MOXTET_DRIVERS: RwLock<BTreeMap<u32, MoxtetDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a Moxtet bus.
pub fn register_bus(name: &str, ops: MoxtetBusOps) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = MoxtetBus {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        module_count: 0,
    };
    MOXTET_BUSES.write().insert(id, bus);
    Ok(id)
}

/// Detect and enumerate modules on a Moxtet bus (Linux `moxtet_find_modules`).
pub fn enumerate_modules(bus_id: u32) -> Result<Vec<u32>, &'static str> {
    let detect_fn = {
        let buses = MOXTET_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("Moxtet bus not found")?;
        bus.ops.detect_modules
    };

    let module_ids = (detect_fn)(bus_id)?;
    let mut registered = Vec::new();

    for (position, &mod_id) in module_ids.iter().enumerate() {
        if mod_id == 0 {
            continue; // Empty slot
        }
        let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mod_type = MoxtetModuleType::from_id(mod_id);
        let name = match mod_type {
            MoxtetModuleType::Pci => String::from("moxtet-pci"),
            MoxtetModuleType::Usb3 => String::from("moxtet-usb3"),
            MoxtetModuleType::Peridot => String::from("moxtet-peridot"),
            MoxtetModuleType::Topaz => String::from("moxtet-topaz"),
            MoxtetModuleType::Sfp => String::from("moxtet-sfp"),
            MoxtetModuleType::Symphony => String::from("moxtet-symphony"),
            MoxtetModuleType::Unknown => alloc::format!("moxtet-unknown-{:02x}", mod_id),
        };
        let dev = MoxtetDevice {
            id: dev_id,
            bus_id,
            module_id: mod_id,
            module_type: mod_type,
            name,
            position: position as u8,
            driver_name: None,
            bound: false,
        };
        MOXTET_DEVICES.write().insert(dev_id, dev);

        let mut buses = MOXTET_BUSES.write();
        if let Some(bus) = buses.get_mut(&bus_id) {
            bus.device_ids.push(dev_id);
            bus.module_count = bus.module_count.saturating_add(1);
        }
        registered.push(dev_id);
        try_match_driver(dev_id)?;
    }

    Ok(registered)
}

/// Read from a Moxtet device at a position.
pub fn read(bus_id: u32, position: u8, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let buses = MOXTET_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("Moxtet bus not found")?;
        bus.ops.read
    };
    (read_fn)(bus_id, position, buf)
}

/// Write to a Moxtet device at a position.
pub fn write(bus_id: u32, position: u8, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let buses = MOXTET_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("Moxtet bus not found")?;
        bus.ops.write
    };
    (write_fn)(bus_id, position, data)
}

/// Register a Moxtet driver.
pub fn register_driver(driver: MoxtetDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    MOXTET_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = MOXTET_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && id_table.iter().any(|id| id.module_id == d.module_id))
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        try_match_driver(dev_id)?;
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = MOXTET_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let mod_id = dev.module_id;

        let drivers = MOXTET_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.module_id == mod_id {
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
        let mut devices = MOXTET_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all Moxtet buses.
pub fn list_buses() -> Vec<(u32, String, u8)> {
    MOXTET_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone(), b.module_count))
        .collect()
}

/// List devices on a bus.
pub fn list_devices(
    bus_id: u32,
) -> Result<Vec<(u32, String, MoxtetModuleType, u8, bool)>, &'static str> {
    let buses = MOXTET_BUSES.read();
    let bus = buses.get(&bus_id).ok_or("Moxtet bus not found")?;
    let devices = MOXTET_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &bus.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((
                dev_id,
                dev.name.clone(),
                dev.module_type,
                dev.position,
                dev.bound,
            ));
        }
    }
    Ok(result)
}

/// Count registered buses.
pub fn bus_count() -> usize {
    MOXTET_BUSES.read().len()
}

// ── Software Moxtet ─────────────────────────────────────────────────────

fn sw_read(_bus_id: u32, _position: u8, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_write(_bus_id: u32, _position: u8, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_detect_modules(_bus_id: u32) -> Result<Vec<u8>, &'static str> {
    // Simulate: PCI module at position 0, SFP at position 1
    let mut ids = Vec::new();
    ids.push(0x01); // PCI
    ids.push(0x05); // SFP
    Ok(ids)
}

/// Software Moxtet bus ops.
pub fn software_moxtet_ops() -> MoxtetBusOps {
    MoxtetBusOps {
        read: sw_read,
        write: sw_write,
        detect_modules: sw_detect_modules,
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
    let ops = software_moxtet_ops();
    let bus_id = register_bus("sw-moxtet-0", ops)?;
    enumerate_modules(bus_id)?;

    // Register a PCI module driver
    let mut id_table = Vec::new();
    id_table.push(MoxtetDeviceId { module_id: 0x01 });
    let driver = MoxtetDriver {
        name: String::from("sw-moxtet-pci-drv"),
        id_table,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    Ok(())
}
