//! SSB (Sonics Silicon Backplane) driver subsystem
//!
//! Provides bus enumeration for Broadcom SSB-based chips (older WiFi NICs, etc.).
//! Mirrors Linux's `drivers/ssb/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SSB bus type (Linux `enum ssb_bustype`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsbBusType {
    Pci,
    Pcmcia,
    Sdio,
    Mips,
    Broadcom,
}

/// SSB core (Linux `struct ssb_device`).
pub struct SsbCore {
    pub id: u32,
    pub bus_id: u32,
    pub core_index: u16,
    pub core_id: u16,
    pub core_rev: u8,
    pub vendor: u16,
    pub name: String,
    pub reg_base: u32,
    pub reg_size: u32,
    pub irq: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// SSB bus (Linux `struct ssb_bus`).
pub struct SsbBus {
    pub id: u32,
    pub bus_type: SsbBusType,
    pub chip_id: u16,
    pub chip_rev: u8,
    pub chip_pkg: u8,
    pub core_ids: Vec<u32>,
}

/// SSB driver (Linux `struct ssb_driver`).
pub struct SsbDriver {
    pub id: u32,
    pub name: String,
    pub core_ids: Vec<u16>,
    pub probe: fn(core_id: u32) -> Result<(), &'static str>,
    pub remove: fn(core_id: u32) -> Result<(), &'static str>,
}

// ── Well-known SSB core IDs ─────────────────────────────────────────────

pub const SSB_CORE_80211: u16 = 0x812;
pub const SSB_CORE_ETHERNET: u16 = 0x822;
pub const SSB_CORE_USB20_HOST: u16 = 0x817;
pub const SSB_CORE_PCIE: u16 = 0x820;
pub const SSB_CORE_SOC_RAM: u16 = 0x80E;

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CORE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SSB_BUSES: RwLock<BTreeMap<u32, SsbBus>> = RwLock::new(BTreeMap::new());
static SSB_CORES: RwLock<BTreeMap<u32, SsbCore>> = RwLock::new(BTreeMap::new());
static SSB_DRIVERS: RwLock<BTreeMap<u32, SsbDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an SSB bus (Linux `ssb_bus_register`).
pub fn register_bus(
    bus_type: SsbBusType,
    chip_id: u16,
    chip_rev: u8,
    chip_pkg: u8,
) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = SsbBus {
        id,
        bus_type,
        chip_id,
        chip_rev,
        chip_pkg,
        core_ids: Vec::new(),
    };
    SSB_BUSES.write().insert(id, bus);
    Ok(id)
}

/// Register a core on an SSB bus.
pub fn register_core(
    bus_id: u32,
    core_index: u16,
    core_id: u16,
    core_rev: u8,
    vendor: u16,
    name: &str,
    reg_base: u32,
    reg_size: u32,
    irq: u32,
) -> Result<u32, &'static str> {
    if !SSB_BUSES.read().contains_key(&bus_id) {
        return Err("SSB bus not found");
    }

    let id = CORE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let core = SsbCore {
        id,
        bus_id,
        core_index,
        core_id,
        core_rev,
        vendor,
        name: String::from(name),
        reg_base,
        reg_size,
        irq,
        driver_name: None,
        bound: false,
    };
    SSB_CORES.write().insert(id, core);
    let mut buses = SSB_BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.core_ids.push(id);
    }
    try_match_core(id)?;
    Ok(id)
}

/// Register an SSB driver (Linux `ssb_driver_register`).
pub fn register_driver(
    name: &str,
    core_ids: Vec<u16>,
    probe: fn(u32) -> Result<(), &'static str>,
    remove: fn(u32) -> Result<(), &'static str>,
) -> Result<u32, &'static str> {
    if core_ids.is_empty() {
        return Err("SSB driver requires at least one core ID");
    }

    let id = DRV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let drv = SsbDriver {
        id,
        name: String::from(name),
        core_ids,
        probe,
        remove,
    };
    SSB_DRIVERS.write().insert(id, drv);
    try_match_all()?;
    Ok(id)
}

fn try_match_core(core_id: u32) -> Result<(), &'static str> {
    let (core_id_val, bound) = {
        let cores = SSB_CORES.read();
        let core = cores.get(&core_id).ok_or("SSB core not found")?;
        (core.core_id, core.bound)
    };
    if bound {
        return Ok(());
    }

    let driver_match = {
        let drivers = SSB_DRIVERS.read();
        drivers
            .values()
            .find(|d| d.core_ids.contains(&core_id_val))
            .map(|d| (d.id, d.probe, d.name.clone()))
    };

    if let Some((_, probe_fn, drv_name)) = driver_match {
        if (probe_fn)(core_id).is_ok() {
            let mut cores = SSB_CORES.write();
            if let Some(core) = cores.get_mut(&core_id) {
                core.bound = true;
                core.driver_name = Some(drv_name);
            }
        }
    }
    Ok(())
}

fn try_match_all() -> Result<(), &'static str> {
    let core_ids: Vec<u32> = SSB_CORES.read().keys().copied().collect();
    for cid in core_ids {
        try_match_core(cid)?;
    }
    Ok(())
}

/// List all SSB buses.
pub fn list_buses() -> Vec<(u32, SsbBusType, u16, u8, usize)> {
    SSB_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.bus_type, b.chip_id, b.chip_rev, b.core_ids.len()))
        .collect()
}

/// List all cores.
pub fn list_cores() -> Vec<(u32, u32, u16, String, bool)> {
    SSB_CORES
        .read()
        .iter()
        .map(|(id, c)| (*id, c.bus_id, c.core_id, c.name.clone(), c.bound))
        .collect()
}

/// Count buses.
pub fn bus_count() -> usize {
    SSB_BUSES.read().len()
}

// ── Software stubs ──────────────────────────────────────────────────────

fn sw_probe(_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_remove(_id: u32) -> Result<(), &'static str> {
    Ok(())
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !SSB_BUSES.read().is_empty() {
        return Ok(());
    }

    let bus_id = register_bus(SsbBusType::Pci, 0x4318, 2, 0)?;
    register_core(
        bus_id,
        0,
        SSB_CORE_80211,
        15,
        0x14E4,
        "ssb-wifi",
        0x1000,
        0x1000,
        16,
    )?;
    register_core(
        bus_id,
        1,
        SSB_CORE_ETHERNET,
        4,
        0x14E4,
        "ssb-eth",
        0x2000,
        0x1000,
        17,
    )?;

    register_driver("ssb-wifi", vec![SSB_CORE_80211], sw_probe, sw_remove)?;
    register_driver("ssb-eth", vec![SSB_CORE_ETHERNET], sw_probe, sw_remove)?;

    crate::serial_println!(
        "ssb: bus registered (chip=0x{:04X}, 2 cores: wifi+ethernet)",
        0x4318
    );
    Ok(())
}
