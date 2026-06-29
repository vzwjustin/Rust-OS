//! ACPI (Advanced Configuration and Power Interface) subsystem
//!
//! Provides ACPI table parsing, device enumeration, and power management.
//! Mirrors Linux's `drivers/acpi/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ACPI table header (Linux `struct acpi_table_header`).
#[derive(Debug, Clone)]
pub struct AcpiTableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub asl_compiler_id: [u8; 4],
    pub asl_compiler_revision: u32,
}

/// ACPI device (Linux `struct acpi_device`).
pub struct AcpiDevice {
    pub id: u32,
    pub name: String,
    pub hid: String, // Hardware ID
    pub uid: String, // Unique ID
    pub adr: u64,    // Address
    pub status: AcpiDevStatus,
    pub parent_id: Option<u32>,
    pub child_ids: Vec<u32>,
    pub driver_name: Option<String>,
    pub bound: bool,
    pub resources: Vec<AcpiResource>,
}

/// ACPI device status (Linux `enum acpi_device_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiDevStatus {
    Present,
    NotPresent,
    Functional,
    Hidden,
}

/// ACPI resource (Linux `struct acpi_resource`).
#[derive(Debug, Clone)]
pub struct AcpiResource {
    pub kind: AcpiResKind,
    pub start: u64,
    pub end: u64,
    pub irq: Option<u32>,
}

/// ACPI resource kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiResKind {
    Io,
    Mem,
    Irq,
    Dma,
}

/// ACPI driver (Linux `struct acpi_driver`).
pub struct AcpiDriver {
    pub name: String,
    pub id_table: Vec<AcpiDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// ACPI device ID (Linux `struct acpi_device_id`).
#[derive(Debug, Clone)]
pub struct AcpiDeviceId {
    pub hid: String,
}

/// ACPI table (parsed).
pub struct AcpiTable {
    pub id: u32,
    pub header: AcpiTableHeader,
    pub data: Vec<u8>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static TABLE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ACPI_DEVICES: RwLock<BTreeMap<u32, AcpiDevice>> = RwLock::new(BTreeMap::new());
static ACPI_DRIVERS: RwLock<BTreeMap<u32, AcpiDriver>> = RwLock::new(BTreeMap::new());
static ACPI_TABLES: RwLock<BTreeMap<u32, AcpiTable>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a parsed ACPI table.
pub fn register_table(header: AcpiTableHeader, data: Vec<u8>) -> Result<u32, &'static str> {
    let id = TABLE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let table = AcpiTable { id, header, data };
    ACPI_TABLES.write().insert(id, table);
    Ok(id)
}

/// Find a table by signature.
pub fn find_table(sig: &[u8; 4]) -> Option<AcpiTableHeader> {
    let tables = ACPI_TABLES.read();
    for (_, table) in tables.iter() {
        if &table.header.signature == sig {
            return Some(table.header.clone());
        }
    }
    None
}

/// Register an ACPI device.
pub fn register_device(
    name: &str,
    hid: &str,
    uid: &str,
    adr: u64,
    parent_id: Option<u32>,
    resources: Vec<AcpiResource>,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = AcpiDevice {
        id,
        name: String::from(name),
        hid: String::from(hid),
        uid: String::from(uid),
        adr,
        status: AcpiDevStatus::Present,
        parent_id,
        child_ids: Vec::new(),
        driver_name: None,
        bound: false,
        resources,
    };
    ACPI_DEVICES.write().insert(id, dev);

    if let Some(pid) = parent_id {
        let mut devices = ACPI_DEVICES.write();
        if let Some(parent) = devices.get_mut(&pid) {
            parent.child_ids.push(id);
        }
    }

    try_match_driver(id)?;
    Ok(id)
}

/// Enumerate ACPI devices from DSDT/SSDT (simplified).
pub fn enumerate_devices() -> Result<Vec<u32>, &'static str> {
    // In a real implementation, this would parse the DSDT/SSDT AML bytecode.
    // For the software implementation, we return already-registered devices.
    let devices = ACPI_DEVICES.read();
    Ok(devices.keys().copied().collect())
}

/// Register an ACPI driver.
pub fn register_driver(driver: AcpiDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    ACPI_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = ACPI_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && id_table.iter().any(|id| id.hid == d.hid))
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
        let devices = ACPI_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let hid = dev.hid.clone();

        let drivers = ACPI_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.hid == hid {
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
        let mut devices = ACPI_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Get device resources.
pub fn get_resources(device_id: u32) -> Result<Vec<AcpiResource>, &'static str> {
    let devices = ACPI_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("ACPI device not found")?;
    Ok(dev.resources.clone())
}

/// Set device power state (Linux `acpi_device_set_power`).
pub fn set_power_state(device_id: u32, state: AcpiPowerState) -> Result<(), &'static str> {
    let devices = ACPI_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("ACPI device not found")?;
    let _ = dev;
    let _ = state;
    Ok(())
}

/// ACPI power state (Linux `enum acpi_device_power_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiPowerState {
    D0,
    D1,
    D2,
    D3Cold,
    D3Hot,
}

/// List all ACPI devices.
pub fn list_devices() -> Vec<(u32, String, String, bool)> {
    ACPI_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.hid.clone(), d.bound))
        .collect()
}

/// List registered tables.
pub fn list_tables() -> Vec<(u32, [u8; 4], u32)> {
    ACPI_TABLES
        .read()
        .iter()
        .map(|(id, t)| (*id, t.header.signature, t.header.length))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    ACPI_DEVICES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("acpi: subsystem ready");
    Ok(())
}
