//! NVMEM (Non-Volatile Memory) subsystem
//!
//! Provides cell-based access to non-volatile memory devices like EEPROMs,
//! EFUSE, and OTP memory. Mirrors Linux's `drivers/nvmem/core.c` with
//! provider registration, cell lookup, and read/write operations.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NVMEM read/write access type (Linux `enum nvmem_access`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmemAccess {
    pub read: bool,
    pub write: bool,
}

impl NvmemAccess {
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
        }
    }

    pub const fn read_write() -> Self {
        Self {
            read: true,
            write: true,
        }
    }
}

/// NVMEM cell configuration (Linux `struct nvmem_cell_info`).
#[derive(Debug, Clone)]
pub struct NvmemCellInfo {
    pub name: String,
    pub offset: u32,
    pub bytes: u32,
    pub bit_offset: u8,
    pub nbits: u32,
    pub access: NvmemAccess,
}

/// Operations implemented by an NVMEM provider (Linux `struct nvmem_config`).
#[derive(Clone, Copy)]
pub struct NvmemOps {
    pub read: fn(offset: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(offset: u32, buf: &[u8]) -> Result<usize, &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_size: fn() -> u32,
}

struct NvmemDevice {
    id: u32,
    name: String,
    size: u32,
    ops: NvmemOps,
    cells: BTreeMap<String, NvmemCellInfo>,
}

// ── Software NVMEM (backed by in-memory array) ──────────────────────────

static SW_NVMEM_DATA: RwLock<Vec<u8>> = RwLock::new(Vec::new());

fn sw_read(offset: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let data = SW_NVMEM_DATA.read();
    let start = offset as usize;
    if start >= data.len() {
        return Err("NVMEM read offset out of range");
    }
    let available = data.len() - start;
    let to_read = buf.len().min(available);
    buf[..to_read].copy_from_slice(&data[start..start + to_read]);
    Ok(to_read)
}

fn sw_write(offset: u32, buf: &[u8]) -> Result<usize, &'static str> {
    let mut data = SW_NVMEM_DATA.write();
    let start = offset as usize;
    if start >= data.len() {
        return Err("NVMEM write offset out of range");
    }
    let available = data.len() - start;
    let to_write = buf.len().min(available);
    data[start..start + to_write].copy_from_slice(&buf[..to_write]);
    Ok(to_write)
}

fn sw_name() -> &'static str {
    "software-nvmem"
}

fn sw_size() -> u32 {
    256
}

const SOFTWARE_NVMEM_OPS: NvmemOps = NvmemOps {
    read: sw_read,
    write: sw_write,
    get_name: sw_name,
    get_size: sw_size,
};

// ── Registry ────────────────────────────────────────────────────────────

static NVMEM_DEVICES: RwLock<BTreeMap<u32, NvmemDevice>> = RwLock::new(BTreeMap::new());
static NEXT_NVMEM_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NVMEM provider (Linux `nvmem_register`).
pub fn register_device(name: &str, ops: NvmemOps) -> Result<u32, &'static str> {
    let size = (ops.get_size)();
    if size == 0 {
        return Err("NVMEM device size must be non-zero");
    }
    let id = NEXT_NVMEM_ID.fetch_add(1, Ordering::SeqCst);
    NVMEM_DEVICES.write().insert(
        id,
        NvmemDevice {
            id,
            name: String::from(name),
            size,
            ops,
            cells: BTreeMap::new(),
        },
    );
    Ok(id)
}

/// Add a cell to an NVMEM device (Linux `nvmem_add_cells`).
pub fn add_cell(device_id: u32, cell: NvmemCellInfo) -> Result<(), &'static str> {
    let mut devices = NVMEM_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("NVMEM device not found")?;

    if cell.bytes == 0 {
        return Err("NVMEM cell size must be non-zero");
    }
    if cell.bit_offset >= 8 {
        return Err("NVMEM cell bit offset out of range");
    }
    let bit_len = cell
        .bytes
        .checked_mul(8)
        .and_then(|bits| bits.checked_sub(cell.bit_offset as u32))
        .ok_or("NVMEM cell bit range overflow")?;
    if cell.nbits == 0 || cell.nbits > bit_len {
        return Err("NVMEM cell bit length out of range");
    }
    let end = cell
        .offset
        .checked_add(cell.bytes)
        .ok_or("NVMEM cell range overflow")?;
    if end > dev.size {
        return Err("NVMEM cell extends beyond device size");
    }

    dev.cells.insert(cell.name.clone(), cell);
    Ok(())
}

/// Read a cell by name (Linux `nvmem_cell_read`).
pub fn read_cell(device_id: u32, cell_name: &str) -> Result<Vec<u8>, &'static str> {
    let (ops, cell) = {
        let devices = NVMEM_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
        let cell = dev.cells.get(cell_name).ok_or("NVMEM cell not found")?;
        if !cell.access.read {
            return Err("NVMEM cell is not readable");
        }
        (dev.ops, cell.clone())
    };

    let mut buf = vec![0u8; cell.bytes as usize];
    let read = (ops.read)(cell.offset, &mut buf)?;
    if read != cell.bytes as usize {
        return Err("NVMEM cell read returned partial data");
    }
    Ok(buf)
}

/// Write a cell by name (Linux `nvmem_cell_write`).
pub fn write_cell(device_id: u32, cell_name: &str, data: &[u8]) -> Result<usize, &'static str> {
    let (ops, cell) = {
        let devices = NVMEM_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
        let cell = dev.cells.get(cell_name).ok_or("NVMEM cell not found")?;
        if !cell.access.write {
            return Err("NVMEM cell is not writable");
        }
        (dev.ops, cell.clone())
    };

    if data.len() != cell.bytes as usize {
        return Err("NVMEM cell write size mismatch");
    }

    (ops.write)(cell.offset, data)
}

/// Read raw bytes from an NVMEM device (Linux `nvmem_device_read`).
pub fn read_raw(device_id: u32, offset: u32, len: usize) -> Result<Vec<u8>, &'static str> {
    let (ops, size) = {
        let devices = NVMEM_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
        (dev.ops, dev.size)
    };

    let end = offset
        .checked_add(len as u32)
        .ok_or("NVMEM read range overflow")?;
    if end > size {
        return Err("NVMEM read extends beyond device size");
    }

    let mut buf = vec![0u8; len];
    let read = (ops.read)(offset, &mut buf)?;
    if read != len {
        buf.truncate(read);
    }
    Ok(buf)
}

/// Write raw bytes to an NVMEM device (Linux `nvmem_device_write`).
pub fn write_raw(device_id: u32, offset: u32, data: &[u8]) -> Result<usize, &'static str> {
    let (ops, size) = {
        let devices = NVMEM_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
        (dev.ops, dev.size)
    };

    let end = offset
        .checked_add(data.len() as u32)
        .ok_or("NVMEM write range overflow")?;
    if end > size {
        return Err("NVMEM write extends beyond device size");
    }

    (ops.write)(offset, data)
}

/// Get device size.
pub fn get_size(device_id: u32) -> Result<u32, &'static str> {
    let devices = NVMEM_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
    Ok(dev.size)
}

/// List all cells on a device.
pub fn list_cells(device_id: u32) -> Result<Vec<String>, &'static str> {
    let devices = NVMEM_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("NVMEM device not found")?;
    Ok(dev.cells.keys().cloned().collect())
}

/// Number of registered NVMEM devices.
pub fn device_count() -> usize {
    NVMEM_DEVICES.read().len()
}

/// Total number of cells across all devices.
pub fn total_cells() -> usize {
    NVMEM_DEVICES.read().values().map(|d| d.cells.len()).sum()
}

/// Initialize NVMEM subsystem with a software device.
pub fn init() -> Result<(), &'static str> {
    if !NVMEM_DEVICES.read().is_empty() {
        return Ok(());
    }

    let size = (SOFTWARE_NVMEM_OPS.get_size)();
    *SW_NVMEM_DATA.write() = vec![0u8; size as usize];

    let dev_id = register_device("software-nvmem", SOFTWARE_NVMEM_OPS)?;

    add_cell(
        dev_id,
        NvmemCellInfo {
            name: String::from("serial-number"),
            offset: 0,
            bytes: 16,
            bit_offset: 0,
            nbits: 128,
            access: NvmemAccess::read_only(),
        },
    )?;
    add_cell(
        dev_id,
        NvmemCellInfo {
            name: String::from("mac-address"),
            offset: 16,
            bytes: 6,
            bit_offset: 0,
            nbits: 48,
            access: NvmemAccess::read_only(),
        },
    )?;
    add_cell(
        dev_id,
        NvmemCellInfo {
            name: String::from("config"),
            offset: 32,
            bytes: 64,
            bit_offset: 0,
            nbits: 512,
            access: NvmemAccess::read_write(),
        },
    )?;

    crate::serial_println!(
        "nvmem: software device registered ({} bytes, {} cells)",
        size,
        total_cells()
    );
    Ok(())
}
