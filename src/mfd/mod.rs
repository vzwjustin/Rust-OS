//! Multi-function device (MFD) registration framework.
//!
//! Composite chips expose multiple logical cells; drivers register an MFD
//! parent and enumerate cells for child drivers to bind.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

/// Resource window exported by one MFD cell.
#[derive(Debug, Clone, Copy)]
pub struct MfdResource {
    pub start: u64,
    pub end: u64,
    pub flags: u32,
}

/// One logical function within an MFD chip.
#[derive(Debug, Clone)]
pub struct MfdCell {
    pub name: String,
    pub id: u32,
    pub resources: Vec<MfdResource>,
    pub enabled: bool,
}

/// Parent MFD device.
#[derive(Debug, Clone)]
pub struct MfdDevice {
    pub name: String,
    pub parent_id: String,
    pub cells: BTreeMap<u32, MfdCell>,
}

static DEVICES: RwLock<BTreeMap<String, MfdDevice>> = RwLock::new(BTreeMap::new());

/// Register an MFD parent device.
pub fn register_mfd_device(name: &str, parent_id: &str) -> bool {
    let mut devs = DEVICES.write();
    if devs.contains_key(name) {
        return false;
    }
    devs.insert(
        String::from(name),
        MfdDevice {
            name: String::from(name),
            parent_id: String::from(parent_id),
            cells: BTreeMap::new(),
        },
    );
    true
}

/// Add a cell to a registered MFD device.
pub fn add_cell(device: &str, cell_name: &str, id: u32, resources: Vec<MfdResource>) -> bool {
    let mut devs = DEVICES.write();
    let Some(dev) = devs.get_mut(device) else {
        return false;
    };
    dev.cells.insert(
        id,
        MfdCell {
            name: String::from(cell_name),
            id,
            resources,
            enabled: true,
        },
    );
    true
}

/// Lookup a cell by parent device name and cell id.
pub fn get_cell(device: &str, id: u32) -> Option<MfdCell> {
    DEVICES
        .read()
        .get(device)
        .and_then(|d| d.cells.get(&id))
        .cloned()
}

/// Enable or disable a cell.
pub fn set_cell_enabled(device: &str, id: u32, enabled: bool) -> bool {
    let mut devs = DEVICES.write();
    let Some(dev) = devs.get_mut(device) else {
        return false;
    };
    let Some(cell) = dev.cells.get_mut(&id) else {
        return false;
    };
    cell.enabled = enabled;
    true
}

/// List all registered MFD devices.
pub fn list_devices() -> Vec<String> {
    DEVICES.read().keys().cloned().collect()
}

/// Initialize MFD registry with a platform placeholder.
pub fn init() {
    DEVICES.write().clear();
    if register_mfd_device("pmic0", "acpi:PMIC0001") {
        add_cell(
            "pmic0",
            "rtc",
            0,
            vec![MfdResource {
                start: 0,
                end: 0xFF,
                flags: 0x200,
            }],
        );
        add_cell(
            "pmic0",
            "regulator",
            1,
            vec![MfdResource {
                start: 0x100,
                end: 0x1FF,
                flags: 0x200,
            }],
        );
    }
    crate::serial_println!("[mfd] initialized ({} devices)", DEVICES.read().len());
}
