//! Block device subsystem
//!
//! Provides block device layer for disk-like I/O with request queues.
//! Mirrors Linux's `block/` core framework.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Block device (Linux `struct block_device` / `struct gendisk`).
pub struct BlockDevice {
    pub id: u32,
    pub name: String,
    pub major: u32,
    pub first_minor: u32,
    pub minors: u32,
    pub sector_size: u32,
    pub num_sectors: u64,
    pub read_only: bool,
    pub removable: bool,
    pub ops: BlockDevOps,
    pub queue_depth: u32,
    pub state: BlockDevState,
    pub partitions: Vec<BlockPartition>,
}

/// Block device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDevState {
    Unregistered,
    Registered,
    Active,
    Suspended,
    Removed,
}

/// Block partition (Linux `struct hd_struct`).
#[derive(Debug, Clone)]
pub struct BlockPartition {
    pub partno: u32,
    pub start_sect: u64,
    pub nr_sects: u64,
    pub name: String,
    pub read_only: bool,
}

/// Block device operations (Linux `struct block_device_operations`).
pub struct BlockDevOps {
    pub open: fn(dev_id: u32, mode: u32) -> Result<(), &'static str>,
    pub release: fn(dev_id: u32) -> Result<(), &'static str>,
    pub read: fn(dev_id: u32, sector: u64, count: u32, buf: &mut [u8]) -> Result<u32, &'static str>,
    pub write: fn(dev_id: u32, sector: u64, data: &[u8]) -> Result<u32, &'static str>,
    pub flush: fn(dev_id: u32) -> Result<(), &'static str>,
    pub discard: fn(dev_id: u32, sector: u64, count: u32) -> Result<(), &'static str>,
    pub ioctl: fn(dev_id: u32, cmd: u32, arg: u64) -> Result<i32, &'static str>,
    pub getgeo: fn(dev_id: u32) -> Result<BlockGeo, &'static str>,
}

/// Block geometry (Linux `struct hd_geometry`).
#[derive(Debug, Clone, Copy)]
pub struct BlockGeo {
    pub heads: u32,
    pub sectors: u32,
    pub cylinders: u32,
    pub start: u64,
}

/// Block I/O request (Linux `struct bio` / `struct request`).
#[derive(Debug, Clone)]
pub struct BlockRequest {
    pub dev_id: u32,
    pub sector: u64,
    pub nr_sectors: u32,
    pub op: BlockReqOp,
    pub data: Vec<u8>,
    pub flags: u32,
}

/// Block request operation (Linux `enum req_op`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReqOp {
    Read,
    Write,
    Flush,
    Discard,
    Reset,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MAJOR_COUNTER: AtomicU32 = AtomicU32::new(200);

static BLOCK_DEVS: RwLock<BTreeMap<u32, BlockDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a block device (Linux `add_disk`).
pub fn register_device(
    name: &str,
    sector_size: u32,
    num_sectors: u64,
    read_only: bool,
    removable: bool,
    ops: BlockDevOps,
) -> Result<u32, &'static str> {
    if sector_size == 0 || num_sectors == 0 {
        return Err("invalid block geometry");
    }
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let major = MAJOR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = BlockDevice {
        id,
        name: String::from(name),
        major,
        first_minor: 0,
        minors: 16,
        sector_size,
        num_sectors,
        read_only,
        removable,
        ops,
        queue_depth: 64,
        state: BlockDevState::Registered,
        partitions: Vec::new(),
    };
    BLOCK_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Open a block device.
pub fn open(dev_id: u32, mode: u32) -> Result<(), &'static str> {
    let open_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        if dev.state == BlockDevState::Suspended {
            return Err("Block device suspended");
        }
        dev.ops.open
    };
    (open_fn)(dev_id, mode)?;

    let mut devs = BLOCK_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = BlockDevState::Active;
    }
    Ok(())
}

/// Release a block device.
pub fn release(dev_id: u32) -> Result<(), &'static str> {
    let release_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        dev.ops.release
    };
    (release_fn)(dev_id)
}

/// Read sectors from a block device (Linux `submit_bio` READ).
pub fn read_sectors(
    dev_id: u32,
    sector: u64,
    count: u32,
    buf: &mut [u8],
) -> Result<u32, &'static str> {
    let read_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        if dev.state != BlockDevState::Active {
            return Err("Block device not active");
        }
        let bytes = (count as usize)
            .checked_mul(dev.sector_size as usize)
            .ok_or("Block read too large")?;
        if buf.len() < bytes {
            return Err("Block read buffer too small");
        }
        let end = sector
            .checked_add(count as u64)
            .ok_or("Block read sector overflow")?;
        if end > dev.num_sectors {
            return Err("Block read beyond device");
        }
        dev.ops.read
    };
    let read = (read_fn)(dev_id, sector, count, buf)?;
    let pid = crate::process::current_pid();
    if pid != 0 {
        crate::cgroup::charge_blkio_read(pid, read as u64 * 512);
    }
    Ok(read)
}

/// Write sectors to a block device (Linux `submit_bio` WRITE).
pub fn write_sectors(dev_id: u32, sector: u64, data: &[u8]) -> Result<u32, &'static str> {
    let write_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        if dev.state != BlockDevState::Active {
            return Err("Block device not active");
        }
        if dev.read_only {
            return Err("Block device is read-only");
        }
        if data.is_empty() || data.len() % dev.sector_size as usize != 0 {
            return Err("Block write data is not sector aligned");
        }
        let count = data.len() / dev.sector_size as usize;
        let end = sector
            .checked_add(count as u64)
            .ok_or("Block write sector overflow")?;
        if end > dev.num_sectors {
            return Err("Block write beyond device");
        }
        dev.ops.write
    };
    let written = (write_fn)(dev_id, sector, data)?;
    let pid = crate::process::current_pid();
    if pid != 0 {
        crate::cgroup::charge_blkio_write(pid, written as u64 * 512);
    }
    Ok(written)
}

/// Flush a block device (Linux `submit_bio` FLUSH).
pub fn flush(dev_id: u32) -> Result<(), &'static str> {
    let flush_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        dev.ops.flush
    };
    (flush_fn)(dev_id)
}

/// Discard sectors (Linux `submit_bio` DISCARD).
pub fn discard(dev_id: u32, sector: u64, count: u32) -> Result<(), &'static str> {
    let discard_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        dev.ops.discard
    };
    (discard_fn)(dev_id, sector, count)
}

/// Submit a block I/O request (Linux `submit_bio`).
pub fn submit_request(req: &BlockRequest) -> Result<u32, &'static str> {
    match req.op {
        BlockReqOp::Read => {
            let mut buf = alloc::vec![0u8; (req.nr_sectors as usize) * 512];
            read_sectors(req.dev_id, req.sector, req.nr_sectors, &mut buf)
        }
        BlockReqOp::Write => write_sectors(req.dev_id, req.sector, &req.data),
        BlockReqOp::Flush => {
            flush(req.dev_id)?;
            Ok(0)
        }
        BlockReqOp::Discard => {
            discard(req.dev_id, req.sector, req.nr_sectors)?;
            Ok(0)
        }
        BlockReqOp::Reset => Err("Reset not supported"),
    }
}

/// Add a partition (Linux `add_partition`).
pub fn add_partition(
    dev_id: u32,
    partno: u32,
    start_sect: u64,
    nr_sects: u64,
    name: &str,
    read_only: bool,
) -> Result<(), &'static str> {
    let mut devs = BLOCK_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("Block device not found")?;
    if start_sect + nr_sects > dev.num_sectors {
        return Err("Partition extends beyond device");
    }
    dev.partitions.push(BlockPartition {
        partno,
        start_sect,
        nr_sects,
        name: String::from(name),
        read_only,
    });
    Ok(())
}

/// Remove a partition (Linux `delete_partition`).
pub fn remove_partition(dev_id: u32, partno: u32) -> Result<(), &'static str> {
    let mut devs = BLOCK_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("Block device not found")?;
    dev.partitions.retain(|p| p.partno != partno);
    Ok(())
}

/// Get device geometry.
pub fn get_geometry(dev_id: u32) -> Result<BlockGeo, &'static str> {
    let geo_fn = {
        let devs = BLOCK_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("Block device not found")?;
        dev.ops.getgeo
    };
    (geo_fn)(dev_id)
}

/// Get device size in bytes.
pub fn get_size(dev_id: u32) -> Result<u64, &'static str> {
    let devs = BLOCK_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("Block device not found")?;
    Ok(dev.sector_size as u64 * dev.num_sectors)
}

/// List all block devices.
pub fn list_devices() -> Vec<(u32, String, u32, u64, bool, BlockDevState)> {
    BLOCK_DEVS
        .read()
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.name.clone(),
                d.sector_size,
                d.num_sectors,
                d.read_only,
                d.state,
            )
        })
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    BLOCK_DEVS.read().len()
}

// ── Software block device ───────────────────────────────────────────────

fn sw_open(_dev_id: u32, _mode: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read(_dev_id: u32, sector: u64, count: u32, buf: &mut [u8]) -> Result<u32, &'static str> {
    let offset = (sector * 512) as usize;
    let len = (count * 512) as usize;
    for (i, b) in buf.iter_mut().enumerate() {
        *b = ((offset + i) & 0xFF) as u8;
    }
    let _ = len;
    Ok(count)
}
fn sw_write(_dev_id: u32, _sector: u64, data: &[u8]) -> Result<u32, &'static str> {
    Ok((data.len() / 512) as u32)
}
fn sw_flush(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_discard(_dev_id: u32, _sector: u64, _count: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_ioctl(_dev_id: u32, _cmd: u32, _arg: u64) -> Result<i32, &'static str> {
    Ok(0)
}
fn sw_getgeo(_dev_id: u32) -> Result<BlockGeo, &'static str> {
    Ok(BlockGeo {
        heads: 255,
        sectors: 63,
        cylinders: 1024,
        start: 0,
    })
}

/// Software block device ops.
pub fn software_block_ops() -> BlockDevOps {
    BlockDevOps {
        open: sw_open,
        release: sw_release,
        read: sw_read,
        write: sw_write,
        flush: sw_flush,
        discard: sw_discard,
        ioctl: sw_ioctl,
        getgeo: sw_getgeo,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register only hardware-backed boot block devices.  The software block
    // ops remain available to tests but are not published as real disks.
    if crate::drivers::virtio::blk::is_available() {
        let cap = crate::drivers::virtio::blk::capacity_sectors().unwrap_or(0);
        if cap == 0 {
            return Err("virtio-blk capacity is zero");
        }
        let vops = virtio_blk_block_ops();
        let vid = register_device("virtio-blk0", 512, cap, false, false, vops)?;
        crate::serial_println!(
            "block: virtio-blk0 registered (id={}, {} sectors / {} MB)",
            vid,
            cap,
            cap * 512 / (1024 * 1024)
        );
    }

    crate::serial_println!(
        "block: subsystem ready ({} hardware device(s))",
        device_count()
    );

    Ok(())
}

// ── VirtIO-blk block device adapter ──────────────────────────────────────

fn vblk_open(_dev_id: u32, _mode: u32) -> Result<(), &'static str> {
    Ok(())
}
fn vblk_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn vblk_read(_dev_id: u32, sector: u64, count: u32, buf: &mut [u8]) -> Result<u32, &'static str> {
    let _n = crate::drivers::virtio::blk::read_sectors(sector, buf)?;
    Ok(count)
}
fn vblk_write(_dev_id: u32, sector: u64, data: &[u8]) -> Result<u32, &'static str> {
    let _n = crate::drivers::virtio::blk::write_sectors(sector, data)?;
    Ok((data.len() / 512) as u32)
}
fn vblk_flush(_dev_id: u32) -> Result<(), &'static str> {
    crate::drivers::virtio::blk::flush()
}
fn vblk_discard(_dev_id: u32, _sector: u64, _count: u32) -> Result<(), &'static str> {
    Err("virtio-blk discard not supported")
}
fn vblk_ioctl(_dev_id: u32, _cmd: u32, _arg: u64) -> Result<i32, &'static str> {
    Ok(0)
}
fn vblk_getgeo(_dev_id: u32) -> Result<BlockGeo, &'static str> {
    Ok(BlockGeo {
        heads: 255,
        sectors: 63,
        cylinders: (crate::drivers::virtio::blk::capacity_sectors().unwrap_or(0) / (255 * 63))
            as u32,
        start: 0,
    })
}

/// Block device ops backed by the VirtIO-blk driver.
pub fn virtio_blk_block_ops() -> BlockDevOps {
    BlockDevOps {
        open: vblk_open,
        release: vblk_release,
        read: vblk_read,
        write: vblk_write,
        flush: vblk_flush,
        discard: vblk_discard,
        ioctl: vblk_ioctl,
        getgeo: vblk_getgeo,
    }
}
