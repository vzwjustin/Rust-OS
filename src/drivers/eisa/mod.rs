//! EISA (Extended Industry Standard Architecture) bus subsystem
//!
//! Provides EISA bus enumeration and device management.
//! Mirrors Linux's `drivers/eisa/eisa-bus.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// EISA device (Linux `struct eisa_device`).
pub struct EisaDevice {
    pub id: u32,
    pub slot: u8,
    pub name: String,
    pub sig: [u8; 4], // EISA signature (compressed manufacturer code)
    pub product_id: u16,
    pub driver_name: Option<String>,
    pub bound: bool,
    pub resource_flags: u32,
}

/// EISA device ID (Linux `struct eisa_device_id`).
#[derive(Debug, Clone)]
pub struct EisaDeviceId {
    pub sig: [u8; 4],
    pub product_id: u16,
}

/// EISA driver (Linux `struct eisa_driver`).
pub struct EisaDriver {
    pub name: String,
    pub id_table: Vec<EisaDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// EISA bus root (Linux `struct eisa_root_device`).
pub struct EisaRoot {
    pub id: u32,
    pub name: String,
    pub bus_base_addr: u64,
    pub num_slots: u8,
    pub device_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static ROOT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static EISA_ROOTS: RwLock<BTreeMap<u32, EisaRoot>> = RwLock::new(BTreeMap::new());
static EISA_DEVICES: RwLock<BTreeMap<u32, EisaDevice>> = RwLock::new(BTreeMap::new());
static EISA_DRIVERS: RwLock<BTreeMap<u32, EisaDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an EISA bus root.
pub fn register_root(name: &str, bus_base_addr: u64, num_slots: u8) -> Result<u32, &'static str> {
    let id = ROOT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = EisaRoot {
        id,
        name: String::from(name),
        bus_base_addr,
        num_slots,
        device_ids: Vec::new(),
    };
    EISA_ROOTS.write().insert(id, root);
    Ok(id)
}

/// Enumerate EISA devices on a bus root (Linux `eisa_enumerate`).
pub fn enumerate(root_id: u32) -> Result<Vec<u32>, &'static str> {
    let roots = EISA_ROOTS.read();
    roots.get(&root_id).ok_or("EISA root not found")?;
    Ok(Vec::new())
}

/// Register an EISA driver.
pub fn register_driver(driver: EisaDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    EISA_DRIVERS.write().insert(id, driver);

    // Try to match with existing devices
    let device_ids: Vec<u32> = {
        let devices = EISA_DEVICES.read();
        devices
            .iter()
            .filter(|(_, dev)| {
                !dev.bound
                    && id_table
                        .iter()
                        .any(|id| dev.sig == id.sig && dev.product_id == id.product_id)
            })
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
        let devices = EISA_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let sig = dev.sig;
        let pid = dev.product_id;

        let drivers = EISA_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id_entry in &drv.id_table {
                if sig == id_entry.sig && pid == id_entry.product_id {
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
        let mut devices = EISA_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all EISA roots.
pub fn list_roots() -> Vec<(u32, String, u8)> {
    EISA_ROOTS
        .read()
        .iter()
        .map(|(id, r)| (*id, r.name.clone(), r.num_slots))
        .collect()
}

/// List devices on a root.
pub fn list_devices(root_id: u32) -> Result<Vec<(u32, String, u8, bool)>, &'static str> {
    let roots = EISA_ROOTS.read();
    let root = roots.get(&root_id).ok_or("EISA root not found")?;
    let devices = EISA_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &root.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.name.clone(), dev.slot, dev.bound));
        }
    }
    Ok(result)
}

/// Count registered roots.
pub fn root_count() -> usize {
    EISA_ROOTS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    let root_id = register_root("sw-eisa-0", 0x1000, 8)?;
    enumerate(root_id)?;

    // Register a generic EISA driver
    let mut id_table = Vec::new();
    id_table.push(EisaDeviceId {
        sig: [0, 0, 0, 0],
        product_id: 0,
    });
    let driver = EisaDriver {
        name: String::from("sw-eisa-generic"),
        id_table,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    Ok(())
}
