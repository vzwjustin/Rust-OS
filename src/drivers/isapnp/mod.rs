//! ISA PnP subsystem
//!
//! Provides ISA Plug-and-Play device enumeration and resource allocation.
//! Mirrors Linux's `drivers/pnp/isapnp/core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ISA PnP device (Linux `struct pnp_dev`).
pub struct IsaPnpDev {
    pub id: u32,
    pub name: String,
    pub card_id: u32,
    pub vendor_id: [u8; 3], // Compressed vendor ID (3 chars)
    pub product_id: u16,
    pub serial: u32,
    pub resources: Vec<IsaPnpResource>,
    pub active: bool,
    pub driver_name: Option<String>,
}

/// ISA PnP resource (Linux `struct pnp_resource`).
#[derive(Debug, Clone)]
pub struct IsaPnpResource {
    pub kind: IsaPnpResKind,
    pub start: u64,
    pub end: u64,
    pub flags: u32,
}

/// Resource kind (Linux `enum pnp_resource_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsaPnpResKind {
    Io,
    Mem,
    Irq,
    Dma,
}

/// ISA PnP card (Linux `struct pnp_card`).
pub struct IsaPnpCard {
    pub id: u32,
    pub name: String,
    pub vendor_id: [u8; 3],
    pub serial: u32,
    pub device_ids: Vec<u32>,
}

/// ISA PnP driver (Linux `struct pnp_driver`).
pub struct IsaPnpDriver {
    pub name: String,
    pub id_table: Vec<IsaPnpDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// ISA PnP device ID (Linux `struct pnp_device_id`).
#[derive(Debug, Clone)]
pub struct IsaPnpDeviceId {
    pub vendor_id: [u8; 3],
    pub product_id: u16,
}

// ── Registry ────────────────────────────────────────────────────────────

static CARD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ISA_PNP_CARDS: RwLock<BTreeMap<u32, IsaPnpCard>> = RwLock::new(BTreeMap::new());
static ISA_PNP_DEVICES: RwLock<BTreeMap<u32, IsaPnpDev>> = RwLock::new(BTreeMap::new());
static ISA_PNP_DRIVERS: RwLock<BTreeMap<u32, IsaPnpDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an ISA PnP card.
pub fn register_card(name: &str, vendor_id: [u8; 3], serial: u32) -> Result<u32, &'static str> {
    let id = CARD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let card = IsaPnpCard {
        id,
        name: String::from(name),
        vendor_id,
        serial,
        device_ids: Vec::new(),
    };
    ISA_PNP_CARDS.write().insert(id, card);
    Ok(id)
}

/// Register a device on an ISA PnP card.
pub fn register_device(
    card_id: u32,
    name: &str,
    vendor_id: [u8; 3],
    product_id: u16,
    serial: u32,
    resources: Vec<IsaPnpResource>,
) -> Result<u32, &'static str> {
    let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = IsaPnpDev {
        id: dev_id,
        name: String::from(name),
        card_id,
        vendor_id,
        product_id,
        serial,
        resources,
        active: false,
        driver_name: None,
    };
    ISA_PNP_DEVICES.write().insert(dev_id, dev);

    let mut cards = ISA_PNP_CARDS.write();
    if let Some(card) = cards.get_mut(&card_id) {
        card.device_ids.push(dev_id);
    }

    try_match_driver(dev_id)?;
    Ok(dev_id)
}

/// Activate an ISA PnP device (Linux `pnp_activate_dev`).
pub fn activate_device(device_id: u32) -> Result<(), &'static str> {
    let mut devices = ISA_PNP_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("ISA PnP device not found")?;
    dev.active = true;
    Ok(())
}

/// Deactivate an ISA PnP device (Linux `pnp_disable_dev`).
pub fn deactivate_device(device_id: u32) -> Result<(), &'static str> {
    let mut devices = ISA_PNP_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("ISA PnP device not found")?;
    dev.active = false;
    Ok(())
}

/// Register an ISA PnP driver.
pub fn register_driver(driver: IsaPnpDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    ISA_PNP_DRIVERS.write().insert(id, driver);

    // Try to match with existing devices
    let device_ids: Vec<u32> = {
        let devices = ISA_PNP_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.driver_name.is_some()
                    && id_table
                        .iter()
                        .any(|id| id.vendor_id == d.vendor_id && id.product_id == d.product_id)
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
        let devices = ISA_PNP_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if d.driver_name.is_none() => d,
            _ => return Ok(()),
        };
        let vid = dev.vendor_id;
        let pid = dev.product_id;

        let drivers = ISA_PNP_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id_entry in &drv.id_table {
                if vid == id_entry.vendor_id && pid == id_entry.product_id {
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
        let mut devices = ISA_PNP_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Get device resources.
pub fn get_resources(device_id: u32) -> Result<Vec<IsaPnpResource>, &'static str> {
    let devices = ISA_PNP_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("ISA PnP device not found")?;
    Ok(dev.resources.clone())
}

/// List all ISA PnP cards.
pub fn list_cards() -> Vec<(u32, String)> {
    ISA_PNP_CARDS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone()))
        .collect()
}

/// List devices on a card.
pub fn list_card_devices(card_id: u32) -> Result<Vec<(u32, String, bool)>, &'static str> {
    let cards = ISA_PNP_CARDS.read();
    let card = cards.get(&card_id).ok_or("ISA PnP card not found")?;
    let devices = ISA_PNP_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &card.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((dev_id, dev.name.clone(), dev.active));
        }
    }
    Ok(result)
}

/// Count registered cards.
pub fn card_count() -> usize {
    ISA_PNP_CARDS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("isapnp: subsystem ready");
    Ok(())
}
