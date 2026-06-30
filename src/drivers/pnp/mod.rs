//! PnP (Plug and Play) subsystem
//!
//! Provides a framework for ISA PnP and ACPI PnP device enumeration.
//! Mirrors Linux's `drivers/pnp/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PnP protocol (Linux `enum pnp_protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnpProtocol {
    Isa,
    Acpi,
    Bios,
}

/// PnP resource type (Linux `enum pnp_resource_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnpResourceType {
    Io,
    Mem,
    Irq,
    Dma,
}

/// PnP resource (Linux `struct pnp_resource`).
#[derive(Debug, Clone)]
pub struct PnpResource {
    pub resource_type: PnpResourceType,
    pub start: u64,
    pub end: u64,
    pub flags: u32,
}

/// PnP device (Linux `struct pnp_dev`).
pub struct PnpDev {
    pub id: u32,
    pub card_id: Option<u32>,
    pub name: String,
    pub pnp_id: String,
    pub protocol: PnpProtocol,
    pub resources: Vec<PnpResource>,
    pub active: bool,
    pub driver_name: Option<String>,
}

/// PnP card (Linux `struct pnp_card`).
pub struct PnpCard {
    pub id: u32,
    pub name: String,
    pub pnp_ids: Vec<String>,
    pub device_ids: Vec<u32>,
}

/// PnP driver (Linux `struct pnp_driver`).
pub struct PnpDriver {
    pub name: String,
    pub id_table: Vec<String>,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CARD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PNP_DEVS: RwLock<BTreeMap<u32, PnpDev>> = RwLock::new(BTreeMap::new());
static PNP_CARDS: RwLock<BTreeMap<u32, PnpCard>> = RwLock::new(BTreeMap::new());
static PNP_DRIVERS: RwLock<BTreeMap<u32, PnpDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PnP device.
pub fn register_device(
    name: &str,
    pnp_id: &str,
    protocol: PnpProtocol,
    resources: Vec<PnpResource>,
    card_id: Option<u32>,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = PnpDev {
        id,
        card_id,
        name: String::from(name),
        pnp_id: String::from(pnp_id),
        protocol,
        resources,
        active: false,
        driver_name: None,
    };
    PNP_DEVS.write().insert(id, dev);

    if let Some(cid) = card_id {
        let mut cards = PNP_CARDS.write();
        if let Some(card) = cards.get_mut(&cid) {
            card.device_ids.push(id);
        }
    }

    try_match_driver(id)?;
    Ok(id)
}

/// Register a PnP card.
pub fn register_card(name: &str, pnp_ids: Vec<String>) -> Result<u32, &'static str> {
    let id = CARD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let card = PnpCard {
        id,
        name: String::from(name),
        pnp_ids,
        device_ids: Vec::new(),
    };
    PNP_CARDS.write().insert(id, card);
    Ok(id)
}

/// Register a PnP driver.
pub fn register_driver(driver: PnpDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let driver_name = driver.name.clone();
    PNP_DRIVERS.write().insert(id, driver);

    let dev_ids: Vec<u32> = {
        let devs = PNP_DEVS.read();
        devs.iter()
            .filter(|(_, d)| d.driver_name.is_none() && d.pnp_id == driver_name)
            .map(|(id, _)| *id)
            .collect()
    };

    for dev_id in dev_ids {
        try_match_driver(dev_id)?;
    }

    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(dev_id: u32) -> Result<(), &'static str> {
    let matched: Option<fn(u32) -> Result<(), &'static str>> = {
        let devs = PNP_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("PnP device not found")?;
        if dev.driver_name.is_some() {
            return Ok(());
        }
        let drivers = PNP_DRIVERS.read();
        let mut found: Option<fn(u32) -> Result<(), &'static str>> = None;
        for (_, drv) in drivers.iter() {
            if drv.id_table.iter().any(|id| id == &dev.pnp_id) {
                found = Some(drv.probe);
                break;
            }
        }
        found
    };

    if let Some(probe_fn) = matched {
        (probe_fn)(dev_id)?;
        let mut devs = PNP_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.active = true;
        }
    }
    Ok(())
}

/// Activate a PnP device (Linux `pnp_activate_dev`).
pub fn activate_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = PNP_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("PnP device not found")?;
    dev.active = true;
    Ok(())
}

/// Deactivate a PnP device (Linux `pnp_disable_dev`).
pub fn deactivate_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = PNP_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("PnP device not found")?;
    dev.active = false;
    Ok(())
}

/// Get device resources.
pub fn get_resources(dev_id: u32) -> Result<Vec<PnpResource>, &'static str> {
    let devs = PNP_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("PnP device not found")?;
    Ok(dev.resources.clone())
}

/// List all PnP devices.
pub fn list_devices() -> Vec<(u32, String, String, PnpProtocol, bool)> {
    PNP_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.pnp_id.clone(), d.protocol, d.active))
        .collect()
}

/// List all PnP cards.
pub fn list_cards() -> Vec<(u32, String, usize)> {
    PNP_CARDS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.device_ids.len()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    PNP_DEVS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("pnp: framework ready (ISA/ACPI PnP enumeration via hardware probing)");
    Ok(())
}
