//! MFD (Multi-Function Device) subsystem
//!
//! Provides framework for multi-function devices that expose multiple
//! sub-devices (e.g., PMIC with GPIO, regulator, RTC).
//! Mirrors Linux's `drivers/mfd/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// MFD device (Linux `struct mfd_cell` + parent device).
pub struct MfdDevice {
    pub id: u32,
    pub name: String,
    pub parent_id: Option<u32>,
    pub cell_ids: Vec<u32>,
    pub state: MfdState,
    pub irq_base: u32,
    pub reg_base: u64,
    pub reg_size: u64,
}

/// MFD state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfdState {
    Unregistered,
    Registered,
    Probing,
    Active,
    Suspended,
    Removed,
}

/// MFD cell (Linux `struct mfd_cell`).
pub struct MfdCell {
    pub id: u32,
    pub dev_id: u32,
    pub name: String,
    pub cell_type: MfdCellType,
    pub compatible: String,
    pub resources: Vec<MfdResource>,
    pub pdata: Vec<u8>,
    pub probe: fn(dev_id: u32, cell_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32, cell_id: u32) -> Result<(), &'static str>,
    pub suspend: Option<fn(dev_id: u32, cell_id: u32) -> Result<(), &'static str>>,
    pub resume: Option<fn(dev_id: u32, cell_id: u32) -> Result<(), &'static str>>,
}

/// MFD cell type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfdCellType {
    Platform,
    I2c,
    Spi,
    Acpi,
}

/// MFD resource (Linux `struct resource`).
#[derive(Debug, Clone)]
pub struct MfdResource {
    pub start: u64,
    pub end: u64,
    pub flags: u32,
    pub name: String,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CELL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MFD_DEVS: RwLock<BTreeMap<u32, MfdDevice>> = RwLock::new(BTreeMap::new());
static MFD_CELLS: RwLock<BTreeMap<u32, MfdCell>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an MFD device (Linux `mfd_add_devices` parent).
pub fn register_device(
    name: &str,
    reg_base: u64,
    reg_size: u64,
    irq_base: u32,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = MfdDevice {
        id,
        name: String::from(name),
        parent_id: None,
        cell_ids: Vec::new(),
        state: MfdState::Registered,
        irq_base,
        reg_base,
        reg_size,
    };
    MFD_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Add MFD cells to a device (Linux `mfd_add_devices`).
pub fn add_cells(dev_id: u32, cells: Vec<MfdCell>) -> Result<Vec<u32>, &'static str> {
    let mut cell_ids = Vec::new();
    for mut cell in cells {
        let cell_id = CELL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        cell.id = cell_id;
        cell.dev_id = dev_id;
        let probe_fn = cell.probe;
        MFD_CELLS.write().insert(cell_id, cell);

        let mut devs = MFD_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.cell_ids.push(cell_id);
        }
        drop(devs);

        // Probe the cell
        {
            let mut devs = MFD_DEVS.write();
            if let Some(dev) = devs.get_mut(&dev_id) {
                dev.state = MfdState::Probing;
            }
        }
        (probe_fn)(dev_id, cell_id)?;

        let mut devs = MFD_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = MfdState::Active;
        }

        cell_ids.push(cell_id);
    }
    Ok(cell_ids)
}

/// Remove a specific cell (Linux `mfd_remove_devices` single).
pub fn remove_cell(cell_id: u32) -> Result<(), &'static str> {
    let (dev_id, remove_fn) = {
        let cells = MFD_CELLS.read();
        let cell = cells.get(&cell_id).ok_or("MFD cell not found")?;
        (cell.dev_id, cell.remove)
    };
    (remove_fn)(dev_id, cell_id)?;

    MFD_CELLS.write().remove(&cell_id);

    let mut devs = MFD_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.cell_ids.retain(|&id| id != cell_id);
    }
    Ok(())
}

/// Remove all cells from a device (Linux `mfd_remove_devices`).
pub fn remove_all_cells(dev_id: u32) -> Result<(), &'static str> {
    let cell_ids = {
        let devs = MFD_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("MFD device not found")?;
        dev.cell_ids.clone()
    };
    for cell_id in cell_ids {
        let _ = remove_cell(cell_id);
    }
    Ok(())
}

/// Suspend an MFD device.
pub fn suspend_device(dev_id: u32) -> Result<(), &'static str> {
    let cell_ids = {
        let mut devs = MFD_DEVS.write();
        let dev = devs.get_mut(&dev_id).ok_or("MFD device not found")?;
        dev.state = MfdState::Suspended;
        dev.cell_ids.clone()
    };

    let cells = MFD_CELLS.read();
    for &cell_id in &cell_ids {
        if let Some(cell) = cells.get(&cell_id) {
            if let Some(suspend_fn) = cell.suspend {
                (suspend_fn)(dev_id, cell_id)?;
            }
        }
    }
    Ok(())
}

/// Resume an MFD device.
pub fn resume_device(dev_id: u32) -> Result<(), &'static str> {
    let cell_ids = {
        let devs = MFD_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("MFD device not found")?;
        dev.cell_ids.clone()
    };

    let cells = MFD_CELLS.read();
    for &cell_id in &cell_ids {
        if let Some(cell) = cells.get(&cell_id) {
            if let Some(resume_fn) = cell.resume {
                (resume_fn)(dev_id, cell_id)?;
            }
        }
    }

    let mut devs = MFD_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = MfdState::Active;
    }
    Ok(())
}

/// Get cell by name.
pub fn get_cell_by_name(name: &str) -> Result<u32, &'static str> {
    let cells = MFD_CELLS.read();
    cells
        .iter()
        .find(|(_, c)| c.name == name)
        .map(|(id, _)| *id)
        .ok_or("MFD cell not found")
}

/// List all MFD devices.
pub fn list_devices() -> Vec<(u32, String, MfdState, usize)> {
    MFD_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state, d.cell_ids.len()))
        .collect()
}

/// List cells for a device.
pub fn list_cells(dev_id: u32) -> Result<Vec<(u32, String, MfdCellType)>, &'static str> {
    let devs = MFD_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("MFD device not found")?;
    let cells = MFD_CELLS.read();
    let mut result = Vec::new();
    for &cell_id in &dev.cell_ids {
        if let Some(cell) = cells.get(&cell_id) {
            result.push((cell.id, cell.name.clone(), cell.cell_type));
        }
    }
    Ok(result)
}

/// Count registered devices.
pub fn device_count() -> usize {
    MFD_DEVS.read().len()
}

// ── Software MFD ────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32, _cell_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32, _cell_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software MFD cells for a PMIC-like device.
pub fn software_mfd_cells() -> Vec<MfdCell> {
    alloc::vec![
        MfdCell {
            id: 0,
            dev_id: 0,
            name: String::from("sw-pmic-gpio"),
            cell_type: MfdCellType::Platform,
            compatible: String::from("sw,pmic-gpio"),
            resources: alloc::vec![MfdResource {
                start: 0x100,
                end: 0x1FF,
                flags: 0x200,
                name: String::from("gpio-reg")
            }],
            pdata: Vec::new(),
            probe: null_probe,
            remove: null_remove,
            suspend: None,
            resume: None,
        },
        MfdCell {
            id: 0,
            dev_id: 0,
            name: String::from("sw-pmic-regulator"),
            cell_type: MfdCellType::Platform,
            compatible: String::from("sw,pmic-regulator"),
            resources: alloc::vec![MfdResource {
                start: 0x200,
                end: 0x2FF,
                flags: 0x200,
                name: String::from("reg-reg")
            }],
            pdata: Vec::new(),
            probe: null_probe,
            remove: null_remove,
            suspend: None,
            resume: None,
        },
        MfdCell {
            id: 0,
            dev_id: 0,
            name: String::from("sw-pmic-rtc"),
            cell_type: MfdCellType::Platform,
            compatible: String::from("sw,pmic-rtc"),
            resources: alloc::vec![MfdResource {
                start: 0x300,
                end: 0x3FF,
                flags: 0x200,
                name: String::from("rtc-reg")
            }],
            pdata: Vec::new(),
            probe: null_probe,
            remove: null_remove,
            suspend: None,
            resume: None,
        },
    ]
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mfd: subsystem ready");
    Ok(())
}
