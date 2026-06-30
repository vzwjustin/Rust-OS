//! NVMEM layouts subsystem
//!
//! Provides NVMEM layout providers that parse raw NVMEM data into named cells.
//! Mirrors Linux's `drivers/nvmem/layouts/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NVMEM layout (Linux `struct nvmem_layout`).
pub struct NvmemLayout {
    pub id: u32,
    pub name: String,
    pub nvmem_id: u32,
    pub layout_type: NvmemLayoutType,
    pub cells: Vec<NvmemLayoutCell>,
    pub fixed_area: bool,
}

/// NVMEM layout type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmemLayoutType {
    Fixed,
    OnieTlv,
    Sl28vpd,
    Microcode,
}

/// NVMEM layout cell (Linux `struct nvmem_cell_info`).
#[derive(Debug, Clone)]
pub struct NvmemLayoutCell {
    pub name: String,
    pub offset: u32,
    pub bytes: u32,
    pub bit_offset: u8,
    pub nbits: u32,
    pub raw: bool,
}

/// NVMEM layout driver (Linux `struct nvmem_layout_driver`).
pub struct NvmemLayoutDriver {
    pub name: String,
    pub layout_type: NvmemLayoutType,
    pub probe: fn(
        layout_id: u32,
        nvmem_id: u32,
        data: &[u8],
    ) -> Result<Vec<NvmemLayoutCell>, &'static str>,
    pub remove: fn(layout_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static LAYOUT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static NVMEM_LAYOUTS: RwLock<BTreeMap<u32, NvmemLayout>> = RwLock::new(BTreeMap::new());
static NVMEM_LAYOUT_DRIVERS: RwLock<BTreeMap<u32, NvmemLayoutDriver>> =
    RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NVMEM layout driver.
pub fn register_driver(driver: NvmemLayoutDriver) -> Result<u32, &'static str> {
    if driver.name.is_empty() {
        return Err("NVMEM layout driver name is empty");
    }

    let mut drivers = NVMEM_LAYOUT_DRIVERS.write();
    if drivers
        .values()
        .any(|existing| existing.name == driver.name && existing.layout_type == driver.layout_type)
    {
        return Err("NVMEM layout driver already registered");
    }

    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    drivers.insert(id, driver);
    Ok(id)
}

/// Add a layout to an NVMEM device (Linux `nvmem_layout_register`).
pub fn register_layout(
    name: &str,
    nvmem_id: u32,
    layout_type: NvmemLayoutType,
    cells: Vec<NvmemLayoutCell>,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("NVMEM layout name is empty");
    }
    for cell in &cells {
        validate_cell(cell)?;
    }

    let mut layouts = NVMEM_LAYOUTS.write();
    if layouts.values().any(|layout| layout.nvmem_id == nvmem_id) {
        return Err("NVMEM layout already registered for device");
    }

    let id = LAYOUT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let layout = NvmemLayout {
        id,
        name: String::from(name),
        nvmem_id,
        layout_type,
        cells,
        fixed_area: layout_type == NvmemLayoutType::Fixed,
    };
    layouts.insert(id, layout);
    Ok(id)
}

/// Parse raw NVMEM data using a layout driver (Linux `nvmem_layout_probe`).
pub fn parse_layout(driver_id: u32, nvmem_id: u32, data: &[u8]) -> Result<u32, &'static str> {
    let (probe_fn, name, layout_type) = {
        let drivers = NVMEM_LAYOUT_DRIVERS.read();
        let drv = drivers
            .get(&driver_id)
            .ok_or("NVMEM layout driver not found")?;
        (drv.probe, drv.name.clone(), drv.layout_type)
    };

    let cells = (probe_fn)(driver_id, nvmem_id, data)?;
    register_layout(&name, nvmem_id, layout_type, cells)
}

/// Get cells from a layout.
pub fn get_cells(layout_id: u32) -> Result<Vec<NvmemLayoutCell>, &'static str> {
    let layouts = NVMEM_LAYOUTS.read();
    let layout = layouts.get(&layout_id).ok_or("NVMEM layout not found")?;
    Ok(layout.cells.clone())
}

/// Find a cell by name within a layout.
pub fn find_cell(layout_id: u32, name: &str) -> Result<NvmemLayoutCell, &'static str> {
    let layouts = NVMEM_LAYOUTS.read();
    let layout = layouts.get(&layout_id).ok_or("NVMEM layout not found")?;
    layout
        .cells
        .iter()
        .find(|c| c.name == name)
        .cloned()
        .ok_or("Cell not found")
}

/// Read cell data from raw NVMEM buffer.
pub fn read_cell(layout_id: u32, cell_name: &str, data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let cell = find_cell(layout_id, cell_name)?;
    if cell.offset as usize >= data.len() {
        return Err("Cell offset out of bounds");
    }
    let end = (cell.offset as usize)
        .checked_add(cell.bytes as usize)
        .ok_or("Cell range overflow")?;
    if end > data.len() {
        return Err("Cell range out of bounds");
    }
    Ok(data[cell.offset as usize..end].to_vec())
}

fn validate_cell(cell: &NvmemLayoutCell) -> Result<(), &'static str> {
    if cell.name.is_empty() {
        return Err("NVMEM layout cell name is empty");
    }
    if cell.bytes == 0 {
        return Err("NVMEM layout cell has zero length");
    }
    if cell.bit_offset >= 8 {
        return Err("NVMEM layout cell bit offset out of bounds");
    }
    if cell.nbits == 0 {
        return Err("NVMEM layout cell has zero bit length");
    }
    let total_bits = cell
        .bytes
        .checked_mul(8)
        .ok_or("NVMEM layout cell bit range overflow")?;
    let usable_bits = total_bits
        .checked_sub(cell.bit_offset as u32)
        .ok_or("NVMEM layout cell bit range overflow")?;
    if cell.nbits > usable_bits {
        return Err("NVMEM layout cell bit range out of bounds");
    }
    cell.offset
        .checked_add(cell.bytes)
        .ok_or("NVMEM layout cell range overflow")?;
    Ok(())
}

/// List all layouts.
pub fn list_layouts() -> Vec<(u32, String, u32, NvmemLayoutType, usize)> {
    NVMEM_LAYOUTS
        .read()
        .iter()
        .map(|(id, l)| {
            (
                *id,
                l.name.clone(),
                l.nvmem_id,
                l.layout_type,
                l.cells.len(),
            )
        })
        .collect()
}

/// List layout drivers.
pub fn list_drivers() -> Vec<(u32, String, NvmemLayoutType)> {
    NVMEM_LAYOUT_DRIVERS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.layout_type))
        .collect()
}

/// Count registered layouts.
pub fn layout_count() -> usize {
    NVMEM_LAYOUTS.read().len()
}

// ── Software NVMEM layout ───────────────────────────────────────────────

fn sw_fixed_probe(
    _layout_id: u32,
    _nvmem_id: u32,
    _data: &[u8],
) -> Result<Vec<NvmemLayoutCell>, &'static str> {
    let mut cells = Vec::new();
    cells.push(NvmemLayoutCell {
        name: String::from("mac-address"),
        offset: 0,
        bytes: 6,
        bit_offset: 0,
        nbits: 48,
        raw: false,
    });
    cells.push(NvmemLayoutCell {
        name: String::from("serial-number"),
        offset: 6,
        bytes: 8,
        bit_offset: 0,
        nbits: 64,
        raw: false,
    });
    cells.push(NvmemLayoutCell {
        name: String::from("calibration"),
        offset: 16,
        bytes: 4,
        bit_offset: 0,
        nbits: 32,
        raw: true,
    });
    Ok(cells)
}
fn sw_fixed_remove(_layout_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software fixed layout driver.
pub fn software_fixed_layout_driver() -> NvmemLayoutDriver {
    NvmemLayoutDriver {
        name: String::from("sw-fixed-layout"),
        layout_type: NvmemLayoutType::Fixed,
        probe: sw_fixed_probe,
        remove: sw_fixed_remove,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !NVMEM_LAYOUT_DRIVERS.read().is_empty() {
        return Ok(());
    }

    let driver = software_fixed_layout_driver();
    let drv_id = register_driver(driver)?;
    crate::serial_println!(
        "nvmem_layouts: fixed layout driver registered (id={})",
        drv_id
    );
    Ok(())
}
