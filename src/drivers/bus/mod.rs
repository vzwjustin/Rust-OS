//! Bus (legacy system bus) driver subsystem
//!
//! Provides a generic system bus framework for device enumeration.
//! Mirrors Linux's `drivers/bus/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// System bus type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    Platform,
    Isa,
    Mca,
    Vlb,
    Eisa,
    Generic,
}

/// System bus (Linux `struct bus_type` for legacy buses).
pub struct SystemBus {
    pub id: u32,
    pub name: String,
    pub bus_type: BusType,
    pub device_ids: Vec<u32>,
    pub driver_ids: Vec<u32>,
}

/// Bus device (Linux `struct bus_device`).
pub struct BusDevice {
    pub id: u32,
    pub bus_id: u32,
    pub name: String,
    pub resource_base: u64,
    pub resource_size: u64,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// Bus driver (Linux `struct bus_driver`).
pub struct BusDriver {
    pub id: u32,
    pub name: String,
    pub bus_id: u32,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static BUSES: RwLock<BTreeMap<u32, SystemBus>> = RwLock::new(BTreeMap::new());
static BUS_DEVS: RwLock<BTreeMap<u32, BusDevice>> = RwLock::new(BTreeMap::new());
static BUS_DRVS: RwLock<BTreeMap<u32, BusDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a system bus.
pub fn register_bus(name: &str, bus_type: BusType) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = SystemBus {
        id,
        name: String::from(name),
        bus_type,
        device_ids: Vec::new(),
        driver_ids: Vec::new(),
    };
    BUSES.write().insert(id, bus);
    Ok(id)
}

/// Register a device on a bus.
pub fn register_device(
    bus_id: u32,
    name: &str,
    resource_base: u64,
    resource_size: u64,
) -> Result<u32, &'static str> {
    if !BUSES.read().contains_key(&bus_id) {
        return Err("Bus not found");
    }
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = BusDevice {
        id,
        bus_id,
        name: String::from(name),
        resource_base,
        resource_size,
        driver_name: None,
        bound: false,
    };
    BUS_DEVS.write().insert(id, dev);
    let mut buses = BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.device_ids.push(id);
    }
    Ok(id)
}

/// Register a bus driver.
pub fn register_driver(
    bus_id: u32,
    name: &str,
    probe: fn(u32) -> Result<(), &'static str>,
    remove: fn(u32) -> Result<(), &'static str>,
) -> Result<u32, &'static str> {
    if !BUSES.read().contains_key(&bus_id) {
        return Err("Bus not found");
    }
    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv = BusDriver {
        id,
        name: String::from(name),
        bus_id,
        probe,
        remove,
    };
    BUS_DRVS.write().insert(id, drv);
    let mut buses = BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.driver_ids.push(id);
    }
    try_match_devices(bus_id)?;
    Ok(id)
}

fn try_match_devices(bus_id: u32) -> Result<(), &'static str> {
    let dev_ids: Vec<u32> = {
        let buses = BUSES.read();
        buses
            .get(&bus_id)
            .map(|b| b.device_ids.clone())
            .unwrap_or_default()
    };
    for dev_id in dev_ids {
        try_match_device(dev_id)?;
    }
    Ok(())
}

fn try_match_device(dev_id: u32) -> Result<(), &'static str> {
    let (bus_id, bound) = {
        let devs = BUS_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Bus device not found")?;
        (dev.bus_id, dev.bound)
    };
    if bound {
        return Ok(());
    }
    let drv_ids: Vec<u32> = {
        let buses = BUSES.read();
        buses
            .get(&bus_id)
            .map(|b| b.driver_ids.clone())
            .unwrap_or_default()
    };
    for drv_id in drv_ids {
        let probe_fn = {
            let drvs = BUS_DRVS.read();
            let drv = drvs.get(&drv_id).ok_or("Bus driver not found")?;
            drv.probe
        };
        if (probe_fn)(dev_id).is_ok() {
            let mut devs = BUS_DEVS.write();
            if let Some(dev) = devs.get_mut(&dev_id) {
                dev.bound = true;
                let drvs = BUS_DRVS.read();
                if let Some(drv) = drvs.get(&drv_id) {
                    dev.driver_name = Some(drv.name.clone());
                }
            }
            break;
        }
    }
    Ok(())
}

/// List all buses.
pub fn list_buses() -> Vec<(u32, String, BusType, usize)> {
    BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone(), b.bus_type, b.device_ids.len()))
        .collect()
}

/// Count registered buses.
pub fn bus_count() -> usize {
    BUSES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !BUSES.read().is_empty() {
        return Ok(());
    }
    let bus_id = register_bus("platform", BusType::Platform)?;
    crate::serial_println!("bus: platform bus registered (id={})", bus_id);
    Ok(())
}
