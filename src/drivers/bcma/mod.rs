//! Broadcom AMBA (BCMA) bus subsystem (mirrors Linux `drivers/bcma/`)
//!
//! Enumerates the cores present on a Broadcom AMBA interconnect (as found on
//! many Broadcom WiFi/SoC parts) and exposes per-core MMIO-window metadata.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Core identifiers (subset of Broadcom core IDs) ────────────────────────

pub const BCMA_CORE_CHIPCOMMON: u16 = 0x800;
pub const BCMA_CORE_PCIE: u16 = 0x820;
pub const BCMA_CORE_80211: u16 = 0x812;
pub const BCMA_CORE_ARM_CR4: u16 = 0x83e;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct BcmaCore {
    index: u32,
    core_id: u16,
    rev: u8,
    manuf: u16,
    base_addr: u64,
    enabled: bool,
}

struct BcmaBus {
    id: u32,
    chip_id: u16,
    cores: Vec<BcmaCore>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static BUSES: RwLock<BTreeMap<u32, BcmaBus>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_bus(chip_id: u16) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    BUSES.write().insert(
        id,
        BcmaBus {
            id,
            chip_id,
            cores: Vec::new(),
        },
    );
    id
}

pub fn add_core(
    bus_id: u32,
    core_id: u16,
    rev: u8,
    manuf: u16,
    base_addr: u64,
) -> Result<u32, &'static str> {
    let mut buses = BUSES.write();
    let bus = buses.get_mut(&bus_id).ok_or("bcma: bus not found")?;
    let index = bus.cores.len() as u32;
    bus.cores.push(BcmaCore {
        index,
        core_id,
        rev,
        manuf,
        base_addr,
        enabled: false,
    });
    Ok(index)
}

pub fn enable_core(bus_id: u32, core_index: u32) -> Result<(), &'static str> {
    let mut buses = BUSES.write();
    let bus = buses.get_mut(&bus_id).ok_or("bcma: bus not found")?;
    let core = bus
        .cores
        .iter_mut()
        .find(|c| c.index == core_index)
        .ok_or("bcma: core not found")?;
    core.enabled = true;
    Ok(())
}

pub fn find_core(bus_id: u32, core_id: u16) -> Option<u64> {
    BUSES
        .read()
        .get(&bus_id)
        .and_then(|b| b.cores.iter().find(|c| c.core_id == core_id))
        .map(|c| c.base_addr)
}

pub fn core_count(bus_id: u32) -> usize {
    BUSES
        .read()
        .get(&bus_id)
        .map(|b| b.cores.len())
        .unwrap_or(0)
}

pub fn bus_count() -> usize {
    BUSES.read().len()
}

/// Initialize the BCMA subsystem with a sample chip enumeration.
pub fn init() -> Result<(), &'static str> {
    if !BUSES.read().is_empty() {
        return Ok(());
    }
    let bus = register_bus(0x4331);
    add_core(bus, BCMA_CORE_CHIPCOMMON, 0x2a, 0x4243, 0x1800_0000)?;
    add_core(bus, BCMA_CORE_PCIE, 0x11, 0x4243, 0x1800_2000)?;
    let wlan = add_core(bus, BCMA_CORE_80211, 0x18, 0x4243, 0x1800_4000)?;
    enable_core(bus, wlan)?;
    crate::serial_println!("bcma: chip 0x4331, {} core(s) enumerated", core_count(bus));
    Ok(())
}
