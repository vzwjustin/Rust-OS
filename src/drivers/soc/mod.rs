//! SoC (System-on-Chip) bus subsystem
//!
//! Provides SoC device identification, syscon regmap access, and SoC-specific
//! attribute exports. Mirrors Linux's `drivers/soc/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SoC device (Linux `struct soc_device`).
pub struct SocDevice {
    pub id: u32,
    pub name: String,
    pub family: String,
    pub revision: String,
    pub soc_id: String,
    pub serial_number: u64,
    pub attributes: BTreeMap<String, String>,
}

/// Syscon regmap (Linux `struct syscon`).
pub struct Syscon {
    pub id: u32,
    pub name: String,
    pub base: u64,
    pub size: u64,
    pub cells: Vec<SysconCell>,
}

/// Syscon register cell.
#[derive(Debug, Clone)]
pub struct SysconCell {
    pub name: String,
    pub offset: u32,
    pub width: u8,
    pub value: u32,
}

/// SoC driver operations.
pub struct SocOps {
    pub identify: fn() -> Result<SocInfo, &'static str>,
    pub get_attribute: fn(name: &str) -> Result<String, &'static str>,
}

/// SoC identification info.
#[derive(Debug, Clone)]
pub struct SocInfo {
    pub name: String,
    pub family: String,
    pub revision: String,
    pub soc_id: String,
    pub serial_number: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static SOC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SYSCON_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SOC_DEVICES: RwLock<BTreeMap<u32, SocDevice>> = RwLock::new(BTreeMap::new());
static SYSCONS: RwLock<BTreeMap<u32, Syscon>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a SoC device.
pub fn register_device(info: SocInfo) -> Result<u32, &'static str> {
    let id = SOC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = SocDevice {
        id,
        name: info.name.clone(),
        family: info.family,
        revision: info.revision,
        soc_id: info.soc_id,
        serial_number: info.serial_number,
        attributes: BTreeMap::new(),
    };
    SOC_DEVICES.write().insert(id, dev);
    Ok(id)
}

/// Add a SoC attribute (Linux `soc_device_register_attribute`).
pub fn add_attribute(soc_id: u32, name: &str, value: &str) -> Result<(), &'static str> {
    let mut devices = SOC_DEVICES.write();
    let dev = devices.get_mut(&soc_id).ok_or("SoC device not found")?;
    dev.attributes
        .insert(String::from(name), String::from(value));
    Ok(())
}

/// Get a SoC attribute.
pub fn get_attribute(soc_id: u32, name: &str) -> Result<String, &'static str> {
    let devices = SOC_DEVICES.read();
    let dev = devices.get(&soc_id).ok_or("SoC device not found")?;
    dev.attributes
        .get(name)
        .cloned()
        .ok_or("Attribute not found")
}

/// Register a syscon regmap (Linux `syscon_register`).
pub fn register_syscon(name: &str, base: u64, size: u64) -> Result<u32, &'static str> {
    let id = SYSCON_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let syscon = Syscon {
        id,
        name: String::from(name),
        base,
        size,
        cells: Vec::new(),
    };
    SYSCONS.write().insert(id, syscon);
    Ok(id)
}

/// Add a cell to a syscon.
pub fn add_syscon_cell(
    syscon_id: u32,
    name: &str,
    offset: u32,
    width: u8,
    value: u32,
) -> Result<(), &'static str> {
    let mut syscons = SYSCONS.write();
    let syscon = syscons.get_mut(&syscon_id).ok_or("Syscon not found")?;
    syscon.cells.push(SysconCell {
        name: String::from(name),
        offset,
        width,
        value,
    });
    Ok(())
}

/// Read a syscon register cell.
pub fn read_syscon_cell(syscon_id: u32, name: &str) -> Result<u32, &'static str> {
    let syscons = SYSCONS.read();
    let syscon = syscons.get(&syscon_id).ok_or("Syscon not found")?;
    syscon
        .cells
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.value)
        .ok_or("Cell not found")
}

/// Write a syscon register cell.
pub fn write_syscon_cell(syscon_id: u32, name: &str, value: u32) -> Result<(), &'static str> {
    let mut syscons = SYSCONS.write();
    let syscon = syscons.get_mut(&syscon_id).ok_or("Syscon not found")?;
    let cell = syscon
        .cells
        .iter_mut()
        .find(|c| c.name == name)
        .ok_or("Cell not found")?;
    cell.value = value;
    Ok(())
}

/// Find a syscon by name.
pub fn find_syscon(name: &str) -> Option<u32> {
    let syscons = SYSCONS.read();
    syscons
        .iter()
        .find(|(_, s)| s.name == name)
        .map(|(id, _)| *id)
}

/// List all SoC devices.
pub fn list_devices() -> Vec<(u32, String, String, String)> {
    SOC_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.family.clone(), d.revision.clone()))
        .collect()
}

/// List all syscons.
pub fn list_syscons() -> Vec<(u32, String, u64, u64)> {
    SYSCONS
        .read()
        .iter()
        .map(|(id, s)| (*id, s.name.clone(), s.base, s.size))
        .collect()
}

/// Count registered SoC devices.
pub fn device_count() -> usize {
    SOC_DEVICES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("soc: subsystem ready");
    Ok(())
}
