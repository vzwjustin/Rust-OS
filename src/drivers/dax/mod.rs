//! DAX (Direct Access) subsystem
//!
//! Provides DAX for direct byte-addressable access to persistent memory.
//! Mirrors Linux's `drivers/dax/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// DAX device (Linux `struct dax_device`).
pub struct DaxDevice {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub align: u32,
    pub target_node: u32,
    pub region_id: u32,
    pub ranges: Vec<DaxRange>,
    pub active: bool,
}

/// DAX range (Linux `struct dax_range`).
#[derive(Debug, Clone)]
pub struct DaxRange {
    pub start: u64,
    pub end: u64,
    pub mapping: u64,
}

/// DAX region (Linux `struct dax_region`).
pub struct DaxRegion {
    pub id: u32,
    pub name: String,
    pub res_start: u64,
    pub res_end: u64,
    pub align: u32,
    pub target_node: u32,
    pub device_ids: Vec<u32>,
}

/// DAX operations (Linux `struct dax_operations`).
pub struct DaxOps {
    pub direct_access: fn(dev_id: u32, offset: u64, len: u64) -> Result<u64, &'static str>,
    pub copy_from_iter: fn(dev_id: u32, offset: u64, data: &[u8]) -> Result<usize, &'static str>,
    pub copy_to_iter: fn(dev_id: u32, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub zero_page_range: fn(dev_id: u32, offset: u64, len: u64) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static REGION_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static DAX_DEVS: RwLock<BTreeMap<u32, DaxDevice>> = RwLock::new(BTreeMap::new());
static DAX_REGIONS: RwLock<BTreeMap<u32, DaxRegion>> = RwLock::new(BTreeMap::new());
static DAX_OPS: RwLock<BTreeMap<u32, DaxOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a DAX region.
pub fn register_region(
    name: &str,
    res_start: u64,
    res_end: u64,
    align: u32,
    target_node: u32,
) -> Result<u32, &'static str> {
    let id = REGION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let region = DaxRegion {
        id,
        name: String::from(name),
        res_start,
        res_end,
        align,
        target_node,
        device_ids: Vec::new(),
    };
    DAX_REGIONS.write().insert(id, region);
    Ok(id)
}

/// Register a DAX device within a region.
pub fn register_device(
    name: &str,
    region_id: u32,
    size: u64,
    ops: DaxOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    // Get region info for alignment and target node
    let (align, target_node) = {
        let regions = DAX_REGIONS.read();
        let region = regions.get(&region_id).ok_or("DAX region not found")?;
        (region.align, region.target_node)
    };

    let dev = DaxDevice {
        id,
        name: String::from(name),
        size,
        align,
        target_node,
        region_id,
        ranges: Vec::new(),
        active: false,
    };
    DAX_DEVS.write().insert(id, dev);
    DAX_OPS.write().insert(id, ops);

    let mut regions = DAX_REGIONS.write();
    if let Some(region) = regions.get_mut(&region_id) {
        region.device_ids.push(id);
    }
    Ok(id)
}

/// Add a range to a DAX device.
pub fn add_range(dev_id: u32, start: u64, end: u64, mapping: u64) -> Result<(), &'static str> {
    let mut devs = DAX_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("DAX device not found")?;
    dev.ranges.push(DaxRange {
        start,
        end,
        mapping,
    });
    Ok(())
}

/// Activate a DAX device.
pub fn activate_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = DAX_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("DAX device not found")?;
    dev.active = true;
    Ok(())
}

/// Deactivate a DAX device.
pub fn deactivate_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = DAX_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("DAX device not found")?;
    dev.active = false;
    Ok(())
}

/// Direct access check (Linux `dax_direct_access`).
pub fn direct_access(dev_id: u32, offset: u64, len: u64) -> Result<u64, &'static str> {
    let access_fn = {
        let ops = DAX_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("DAX ops not found")?;
        dev_ops.direct_access
    };
    (access_fn)(dev_id, offset, len)
}

/// Copy data to DAX device (Linux `dax_copy_from_iter`).
pub fn copy_to_dev(dev_id: u32, offset: u64, data: &[u8]) -> Result<usize, &'static str> {
    let copy_fn = {
        let ops = DAX_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("DAX ops not found")?;
        dev_ops.copy_from_iter
    };
    (copy_fn)(dev_id, offset, data)
}

/// Copy data from DAX device (Linux `dax_copy_to_iter`).
pub fn copy_from_dev(dev_id: u32, offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let copy_fn = {
        let ops = DAX_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("DAX ops not found")?;
        dev_ops.copy_to_iter
    };
    (copy_fn)(dev_id, offset, buf)
}

/// Zero a range on a DAX device (Linux `dax_zero_page_range`).
pub fn zero_range(dev_id: u32, offset: u64, len: u64) -> Result<(), &'static str> {
    let zero_fn = {
        let ops = DAX_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("DAX ops not found")?;
        dev_ops.zero_page_range
    };
    (zero_fn)(dev_id, offset, len)
}

/// List all DAX devices.
pub fn list_devices() -> Vec<(u32, String, u64, u32, bool)> {
    DAX_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.size, d.target_node, d.active))
        .collect()
}

/// List all DAX regions.
pub fn list_regions() -> Vec<(u32, String, u64, u64, u32)> {
    DAX_REGIONS
        .read()
        .iter()
        .map(|(id, r)| (*id, r.name.clone(), r.res_start, r.res_end, r.target_node))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    DAX_DEVS.read().len()
}

// ── Software DAX ────────────────────────────────────────────────────────

fn sw_direct_access(dev_id: u32, offset: u64, len: u64) -> Result<u64, &'static str> {
    let devs = DAX_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("DAX device not found")?;
    if offset + len > dev.size {
        return Err("DAX access out of range");
    }
    Ok(len)
}
fn sw_copy_to_dev(_dev_id: u32, _offset: u64, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_copy_from_dev(_dev_id: u32, _offset: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_zero_range(_dev_id: u32, _offset: u64, _len: u64) -> Result<(), &'static str> {
    Ok(())
}

/// Software DAX ops.
pub fn software_dax_ops() -> DaxOps {
    DaxOps {
        direct_access: sw_direct_access,
        copy_from_iter: sw_copy_to_dev,
        copy_to_iter: sw_copy_from_dev,
        zero_page_range: sw_zero_range,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register a DAX region (1GB persistent memory range)
    let region_id = register_region("dax-pmem0", 0x100000000, 0x13FFFFFFF, 4096, 1)?;

    // Register a DAX device (256MB)
    let ops = software_dax_ops();
    let dev_id = register_device("dax0.0", region_id, 256 * 1024 * 1024, ops)?;

    // Add a range
    add_range(dev_id, 0, 256 * 1024 * 1024 - 1, 0x100000000)?;

    // Activate
    activate_device(dev_id)?;

    // Test direct access
    let _ = direct_access(dev_id, 0, 4096)?;
    let _ = zero_range(dev_id, 0, 4096)?;

    Ok(())
}
