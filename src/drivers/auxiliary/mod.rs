//! Auxiliary bus subsystem
//!
//! Provides a lightweight bus for creating auxiliary devices from parent devices.
//! Mirrors Linux's `drivers/base/auxiliary.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Auxiliary device (Linux `struct auxiliary_device`).
pub struct AuxiliaryDevice {
    pub id: u32,
    pub name: String,
    pub parent_id: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
    pub match_modalias: String,
}

/// Auxiliary driver (Linux `struct auxiliary_driver`).
pub struct AuxiliaryDriver {
    pub name: String,
    pub id_table: Vec<AuxiliaryDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
    pub shutdown: Option<fn(device_id: u32)>,
}

/// Auxiliary device ID (Linux `struct auxiliary_device_id`).
#[derive(Debug, Clone)]
pub struct AuxiliaryDeviceId {
    pub name: String,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static AUX_DEVICES: RwLock<BTreeMap<u32, AuxiliaryDevice>> = RwLock::new(BTreeMap::new());
static AUX_DRIVERS: RwLock<BTreeMap<u32, AuxiliaryDriver>> = RwLock::new(BTreeMap::new());
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an auxiliary device on the auxiliary bus.
pub fn register_device(name: &str, parent_id: u32, modalias: &str) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("Auxiliary device name is empty");
    }
    if modalias.is_empty() {
        return Err("Auxiliary device modalias is empty");
    }
    if AUX_DEVICES
        .read()
        .values()
        .any(|dev| dev.name == name && dev.parent_id == parent_id)
    {
        return Err("Auxiliary device already registered");
    }

    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = AuxiliaryDevice {
        id,
        name: String::from(name),
        parent_id,
        driver_name: None,
        bound: false,
        match_modalias: String::from(modalias),
    };
    AUX_DEVICES.write().insert(id, dev);

    // Try to match with existing drivers
    if let Err(err) = try_match_driver(id) {
        AUX_DEVICES.write().remove(&id);
        return Err(err);
    }
    Ok(id)
}

/// Unregister an auxiliary device.
pub fn unregister_device(device_id: u32) -> Result<(), &'static str> {
    // Call remove on bound driver
    {
        let devices = AUX_DEVICES.read();
        let dev = devices
            .get(&device_id)
            .ok_or("Auxiliary device not found")?;
        if dev.bound {
            if let Some(ref drv_name) = dev.driver_name {
                let drivers = AUX_DRIVERS.read();
                for (_, drv) in drivers.iter() {
                    if drv.name == *drv_name {
                        (drv.remove)(device_id).ok();
                        break;
                    }
                }
            }
        }
    }
    AUX_DEVICES.write().remove(&device_id);
    Ok(())
}

/// Register an auxiliary driver.
pub fn register_driver(driver: AuxiliaryDriver) -> Result<u32, &'static str> {
    if driver.name.is_empty() {
        return Err("Auxiliary driver name is empty");
    }
    if driver.id_table.is_empty() {
        return Err("Auxiliary driver ID table is empty");
    }
    if AUX_DRIVERS
        .read()
        .values()
        .any(|existing| existing.name == driver.name)
    {
        return Err("Auxiliary driver already registered");
    }

    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let driver_name = driver.name.clone();
    AUX_DRIVERS.write().insert(id, driver);

    // Try to match with existing devices
    let device_ids: Vec<u32> = {
        let devices = AUX_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && d.match_modalias.contains(&driver_name))
            .map(|(id, _)| *id)
            .collect()
    };

    for dev_id in device_ids {
        if let Err(err) = try_match_driver(dev_id) {
            AUX_DRIVERS.write().remove(&id);
            return Err(err);
        }
    }

    Ok(id)
}

/// Unregister an auxiliary driver.
pub fn unregister_driver(driver_id: u32) -> Result<(), &'static str> {
    // Unbind all devices bound to this driver
    let driver_name = {
        let drivers = AUX_DRIVERS.read();
        drivers.get(&driver_id).map(|d| d.name.clone())
    };

    if let Some(dname) = driver_name {
        let bound_devices: Vec<u32> = {
            let devices = AUX_DEVICES.read();
            devices
                .iter()
                .filter(|(_, d)| d.bound && d.driver_name.as_ref() == Some(&dname))
                .map(|(id, _)| *id)
                .collect()
        };

        for dev_id in bound_devices {
            let remove_fn = {
                let drivers = AUX_DRIVERS.read();
                drivers.get(&driver_id).map(|d| d.remove)
            };
            if let Some(rm) = remove_fn {
                (rm)(dev_id).ok();
            }
            let mut devices = AUX_DEVICES.write();
            if let Some(dev) = devices.get_mut(&dev_id) {
                dev.bound = false;
                dev.driver_name = None;
            }
        }
    }

    AUX_DRIVERS.write().remove(&driver_id);
    Ok(())
}

/// Try to match a device with a registered driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched_driver = {
        let devices = AUX_DEVICES.read();
        let dev = devices.get(&device_id);
        let Some(dev) = dev else {
            return Ok(());
        };

        if dev.bound {
            return Ok(());
        }
        let modalias = dev.match_modalias.clone();

        let drivers = AUX_DRIVERS.read();
        let mut found: Option<(u32, fn(u32) -> Result<(), &'static str>)> = None;
        for (drv_id, drv) in drivers.iter() {
            for id_entry in &drv.id_table {
                if modalias.contains(&id_entry.name) {
                    found = Some((*drv_id, drv.probe));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };

    if let Some((drv_id, probe_fn)) = matched_driver {
        (probe_fn)(device_id)?;

        let drv_name = {
            let drivers = AUX_DRIVERS.read();
            drivers.get(&drv_id).map(|d| d.name.clone())
        };

        let mut devices = AUX_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = drv_name;
        }
    }
    Ok(())
}

/// List all auxiliary devices.
pub fn list_devices() -> Vec<(u32, String, u32, bool)> {
    AUX_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.parent_id, d.bound))
        .collect()
}

/// List all auxiliary drivers.
pub fn list_drivers() -> Vec<(u32, String)> {
    AUX_DRIVERS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    AUX_DEVICES.read().len()
}

/// Count bound devices.
pub fn bound_count() -> usize {
    AUX_DEVICES.read().values().filter(|d| d.bound).count()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("auxiliary: framework ready");
    Ok(())
}
