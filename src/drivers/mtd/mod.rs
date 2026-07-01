//! MTD (Memory Technology Devices) subsystem
//!
//! Provides flash memory abstraction with read/write/erase operations,
//! partition management, and NOR/NAND device registration. Mirrors
//! Linux's `drivers/mtd/mtdcore.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── Types ───────────────────────────────────────────────────────────────

/// MTD device type (Linux `enum mtd_dev_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtdType {
    Nor,
    Nand,
    Onenand,
    Rom,
    Absent,
}

/// MTD write modes (Linux `enum mtd_write_modes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtdWriteMode {
    Normal,
    Oob,
    Raw,
}

/// MTD operations (Linux `struct mtd_info` function pointers).
pub struct MtdOps {
    pub read: fn(offset: u64, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(offset: u64, buf: &[u8]) -> Result<usize, &'static str>,
    pub erase: fn(offset: u64, len: u64) -> Result<(), &'static str>,
    pub is_bad_block: fn(offset: u64) -> bool,
    pub mark_bad_block: fn(offset: u64) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct MtdDevice {
    id: u32,
    name: String,
    mtd_type: MtdType,
    size: u64,
    erasesize: u32,
    writesize: u32,
    oobsize: u32,
    ops: &'static MtdOps,
    write_protected: bool,
    partitions: Vec<MtdPartition>,
}

/// MTD partition (Linux `struct mtd_partition`).
#[derive(Debug, Clone)]
pub struct MtdPartition {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub mask_flags: u32,
}

// ── Software NOR flash (in-memory backing) ──────────────────────────────

static SW_FLASH_DATA: Mutex<Vec<u8>> = Mutex::new(Vec::new());
const SW_FLASH_SIZE: u64 = 8 * 1024 * 1024; // 8 MiB
const SW_FLASH_ERASESIZE: u32 = 64 * 1024; // 64 KiB erase blocks
const SW_FLASH_WRITESIZE: u32 = 1; // Byte-writeable (NOR)

fn sw_read(offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let data = SW_FLASH_DATA.lock();
    let start = offset as usize;
    if start >= data.len() {
        return Err("MTD read offset out of range");
    }
    let available = data.len() - start;
    let to_read = buf.len().min(available);
    buf[..to_read].copy_from_slice(&data[start..start + to_read]);
    Ok(to_read)
}

fn sw_write(offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
    let mut data = SW_FLASH_DATA.lock();
    let start = offset as usize;
    if start + buf.len() > data.len() {
        return Err("MTD write extends beyond device");
    }
    // NOR flash: bits can only go from 1→0 without erase. Simulate.
    for (i, &byte) in buf.iter().enumerate() {
        data[start + i] &= byte; // AND to simulate NOR write semantics
    }
    Ok(buf.len())
}

fn sw_erase(offset: u64, len: u64) -> Result<(), &'static str> {
    let mut data = SW_FLASH_DATA.lock();
    let start = offset as usize;
    let end = (offset + len) as usize;
    if end > data.len() {
        return Err("MTD erase extends beyond device");
    }
    // Erase sets all bits to 1 (0xFF).
    for byte in &mut data[start..end] {
        *byte = 0xFF;
    }
    Ok(())
}

fn sw_is_bad(_offset: u64) -> bool {
    false
}
fn sw_mark_bad(_offset: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_name() -> &'static str {
    "software-nor-flash"
}

pub static SW_NOR_OPS: MtdOps = MtdOps {
    read: sw_read,
    write: sw_write,
    erase: sw_erase,
    is_bad_block: sw_is_bad,
    mark_bad_block: sw_mark_bad,
    get_name: sw_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static MTD_DEVICES: RwLock<BTreeMap<u32, MtdDevice>> = RwLock::new(BTreeMap::new());
static NEXT_MTD_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an MTD device (Linux `add_mtd_device`).
pub fn register_device(
    name: &str,
    mtd_type: MtdType,
    size: u64,
    erasesize: u32,
    writesize: u32,
    oobsize: u32,
    ops: &'static MtdOps,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("MTD device name is empty");
    }
    if size == 0 || erasesize == 0 || writesize == 0 {
        return Err("MTD device size, erasesize, and writesize must be non-zero");
    }
    if size % erasesize as u64 != 0 {
        return Err("MTD device size must be erasesize-aligned");
    }
    if writesize as u64 > size {
        return Err("MTD writesize exceeds device size");
    }
    if oobsize != 0 && mtd_type == MtdType::Nor {
        return Err("NOR MTD device cannot expose OOB data");
    }

    let mut devices = MTD_DEVICES.write();
    if devices.values().any(|dev| dev.name == name) {
        return Err("MTD device already registered");
    }

    let id = NEXT_MTD_ID.fetch_add(1, Ordering::SeqCst);
    devices.insert(
        id,
        MtdDevice {
            id,
            name: String::from(name),
            mtd_type,
            size,
            erasesize,
            writesize,
            oobsize,
            ops,
            write_protected: false,
            partitions: Vec::new(),
        },
    );
    Ok(id)
}

/// Add partitions to an MTD device (Linux `add_mtd_partitions`).
pub fn add_partitions(device_id: u32, partitions: &[MtdPartition]) -> Result<(), &'static str> {
    let mut devices = MTD_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("MTD device not found")?;
    if partitions.is_empty() {
        return Err("MTD partition list is empty");
    }

    for (idx, part) in partitions.iter().enumerate() {
        if part.name.is_empty() {
            return Err("MTD partition name is empty");
        }
        if part.size == 0 {
            return Err("MTD partition size is zero");
        }
        let end = part
            .offset
            .checked_add(part.size)
            .ok_or("MTD partition range overflow")?;
        if end > dev.size {
            return Err("MTD partition extends beyond device");
        }
        if part.offset % dev.erasesize as u64 != 0 || part.size % dev.erasesize as u64 != 0 {
            return Err("MTD partition must be erasesize-aligned");
        }
        if dev.partitions.iter().any(|existing| {
            let existing_end = existing.offset.saturating_add(existing.size);
            existing.name == part.name || (part.offset < existing_end && existing.offset < end)
        }) {
            return Err("MTD partition overlaps or duplicates existing partition");
        }
        for other in &partitions[..idx] {
            let other_end = other
                .offset
                .checked_add(other.size)
                .ok_or("MTD partition range overflow")?;
            if other.name == part.name || (part.offset < other_end && other.offset < end) {
                return Err("MTD partition overlaps or duplicates another new partition");
            }
        }
    }

    for part in partitions {
        dev.partitions.push(part.clone());
    }
    Ok(())
}

/// Read from an MTD device (Linux `mtd_read`).
pub fn read(device_id: u32, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let ops = {
        let devices = MTD_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("MTD device not found")?;
        let end = offset
            .checked_add(buf.len() as u64)
            .ok_or("MTD read range overflow")?;
        if end > dev.size {
            return Err("MTD read extends beyond device");
        }
        dev.ops
    };
    (ops.read)(offset, buf)
}

/// Write to an MTD device (Linux `mtd_write`).
pub fn write(device_id: u32, offset: u64, buf: &[u8]) -> Result<usize, &'static str> {
    let (ops, wp) = {
        let devices = MTD_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("MTD device not found")?;
        if dev.write_protected {
            return Err("MTD device is write-protected");
        }
        let end = offset
            .checked_add(buf.len() as u64)
            .ok_or("MTD write range overflow")?;
        if end > dev.size {
            return Err("MTD write extends beyond device");
        }
        if offset % dev.writesize as u64 != 0 || buf.len() as u64 % dev.writesize as u64 != 0 {
            return Err("MTD write must be writesize-aligned");
        }
        (dev.ops, dev.write_protected)
    };
    let _ = wp;
    (ops.write)(offset, buf)
}

/// Erase a region (Linux `mtd_erase`).
pub fn erase(device_id: u32, offset: u64, len: u64) -> Result<(), &'static str> {
    let (ops, erasesize) = {
        let devices = MTD_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("MTD device not found")?;
        if dev.write_protected {
            return Err("MTD device is write-protected");
        }
        if offset % dev.erasesize as u64 != 0 {
            return Err("MTD erase offset must be erasesize-aligned");
        }
        if len % dev.erasesize as u64 != 0 {
            return Err("MTD erase length must be multiple of erasesize");
        }
        let end = offset.checked_add(len).ok_or("MTD erase range overflow")?;
        if end > dev.size {
            return Err("MTD erase extends beyond device");
        }
        (dev.ops, dev.erasesize)
    };
    let _ = erasesize;
    (ops.erase)(offset, len)
}

/// Check if a block is bad (Linux `mtd_block_isbad`).
pub fn is_bad_block(device_id: u32, offset: u64) -> Result<bool, &'static str> {
    let ops = {
        let devices = MTD_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("MTD device not found")?;
        dev.ops
    };
    Ok((ops.is_bad_block)(offset))
}

/// Get device info.
pub fn get_info(device_id: u32) -> Result<(String, MtdType, u64, u32, u32), &'static str> {
    let devices = MTD_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("MTD device not found")?;
    Ok((
        dev.name.clone(),
        dev.mtd_type,
        dev.size,
        dev.erasesize,
        dev.writesize,
    ))
}

/// Get partitions for a device.
pub fn get_partitions(device_id: u32) -> Result<Vec<MtdPartition>, &'static str> {
    let devices = MTD_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("MTD device not found")?;
    Ok(dev.partitions.clone())
}

/// Set write protection (Linux `mtd_write_protect`).
pub fn set_write_protected(device_id: u32, wp: bool) -> Result<(), &'static str> {
    let mut devices = MTD_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("MTD device not found")?;
    dev.write_protected = wp;
    Ok(())
}

/// Number of registered MTD devices.
pub fn device_count() -> usize {
    MTD_DEVICES.read().len()
}

/// Initialize MTD subsystem with software NOR flash.
pub fn init() -> Result<(), &'static str> {
    if device_count() == 0 {
        *SW_FLASH_DATA.lock() = alloc::vec![0xFF; SW_FLASH_SIZE as usize];
        register_device(
            "software-nor-flash",
            MtdType::Nor,
            SW_FLASH_SIZE,
            SW_FLASH_ERASESIZE,
            SW_FLASH_WRITESIZE,
            0, // oobsize
            &SW_NOR_OPS,
        )?;
        crate::serial_println!("mtd: software NOR flash registered");
    }
    crate::serial_println!("mtd: subsystem ready");
    Ok(())
}
