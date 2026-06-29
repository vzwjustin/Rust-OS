//! AMBA (Advanced Microcontroller Bus Architecture) bus subsystem
//!
//! Provides AMBA bus for ARM PrimeCell peripheral device enumeration and management.
//! Mirrors Linux's `drivers/amba/bus.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// AMBA device (Linux `struct amba_device`).
pub struct AmbaDevice {
    pub id: u32,
    pub name: String,
    pub dev_id: u32,    // PrimeCell peripheral ID
    pub vendor_id: u16, // PrimeCell vendor (JEP106)
    pub res_start: u64,
    pub res_end: u64,
    pub irq: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
    pub periphid: u32,
    pub cid: u32, // Component ID (0xB105F00D)
}

/// AMBA driver (Linux `struct amba_driver`).
pub struct AmbaDriver {
    pub name: String,
    pub id_table: Vec<AmbaId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// AMBA device ID (Linux `struct amba_id`).
#[derive(Debug, Clone)]
pub struct AmbaId {
    pub id: u32,
    pub mask: u32,
    pub data: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static AMBA_DEVICES: RwLock<BTreeMap<u32, AmbaDevice>> = RwLock::new(BTreeMap::new());
static AMBA_DRIVERS: RwLock<BTreeMap<u32, AmbaDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an AMBA device.
pub fn register_device(
    name: &str,
    dev_id: u32,
    vendor_id: u16,
    res_start: u64,
    res_end: u64,
    irq: u32,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = AmbaDevice {
        id,
        name: String::from(name),
        dev_id,
        vendor_id,
        res_start,
        res_end,
        irq,
        driver_name: None,
        bound: false,
        periphid: dev_id,
        cid: 0xB105F00D,
    };
    AMBA_DEVICES.write().insert(id, dev);
    try_match_driver(id)?;
    Ok(id)
}

/// Register an AMBA driver.
pub fn register_driver(driver: AmbaDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    AMBA_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = AMBA_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.bound
                    && id_table
                        .iter()
                        .any(|aid| (d.dev_id & aid.mask) == (aid.id & aid.mask))
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
        let devices = AMBA_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let dev_id = dev.dev_id;

        let drivers = AMBA_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for aid in &drv.id_table {
                if (dev_id & aid.mask) == (aid.id & aid.mask) {
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
        let mut devices = AMBA_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Get device resource range.
pub fn get_resource(device_id: u32) -> Result<(u64, u64, u32), &'static str> {
    let devices = AMBA_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("AMBA device not found")?;
    Ok((dev.res_start, dev.res_end, dev.irq))
}

/// List all AMBA devices.
pub fn list_devices() -> Vec<(u32, String, u32, bool)> {
    AMBA_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_id, d.bound))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    AMBA_DEVICES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    // PL011 UART: dev_id 0x00141011, vendor 0x41 (ARM)
    register_device("pl011-uart", 0x00141011, 0x41, 0x09000000, 0x09000FFF, 37)?;

    // PL110 LCD: dev_id 0x00141110, vendor 0x41 (ARM)
    register_device("pl110-lcd", 0x00141110, 0x41, 0x10020000, 0x10020FFF, 44)?;

    // Register a PL011 driver
    let mut id_table = Vec::new();
    id_table.push(AmbaId {
        id: 0x00141011,
        mask: 0x000FFFFF,
        data: 0,
    });
    let driver = AmbaDriver {
        name: String::from("pl011-drv"),
        id_table,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    Ok(())
}
