//! Block I/O layer — generic block device abstraction
//!
//! Ported from Linux block/blk-core.c, block/blk-mq.c concepts.
//! Provides:
//! - Block device registration and lookup
//! - Bio (block I/O) request structure
//! - Request queue with merging and scheduling
//! - I/O scheduler (simple deadline-style)
//! - Read/write/flush operations
//!
//! ## Architecture
//! Each block device has a request queue. Bios are submitted to the queue,
//! merged if possible, and dispatched to the device driver in order.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec; // for vec! macro
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// ── Constants ───────────────────────────────────────────────────────────

pub const SECTOR_SIZE: usize = 512;
pub const BIO_MAX_PAGES: usize = 256; // Max pages per bio (1MB)
pub const BIO_MAX_SECTORS: usize = BIO_MAX_PAGES * (4096 / SECTOR_SIZE);

// ── Bio (Block I/O) ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioDirection {
    Read,
    Write,
    Flush,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioStatus {
    Pending,
    Complete,
    Error,
}

/// A bio represents a single I/O request to a block device.
#[derive(Clone)]
pub struct Bio {
    pub bi_sector: u64,
    pub bi_size: usize, // in bytes
    pub bi_dir: BioDirection,
    pub bi_status: BioStatus,
    pub bi_error: i32,
    /// Inline data buffer for small transfers
    pub bi_data: Vec<u8>,
    /// Callback invoked when bio completes
    pub bi_end_io: Option<fn(&Bio)>,
}

impl Bio {
    pub fn new_read(sector: u64, size: usize) -> Self {
        Self {
            bi_sector: sector,
            bi_size: size,
            bi_dir: BioDirection::Read,
            bi_status: BioStatus::Pending,
            bi_error: 0,
            bi_data: vec![0u8; size],
            bi_end_io: None,
        }
    }

    pub fn new_write(sector: u64, data: Vec<u8>) -> Self {
        let size = data.len();
        Self {
            bi_sector: sector,
            bi_size: size,
            bi_dir: BioDirection::Write,
            bi_status: BioStatus::Pending,
            bi_error: 0,
            bi_data: data,
            bi_end_io: None,
        }
    }

    pub fn new_flush() -> Self {
        Self {
            bi_sector: 0,
            bi_size: 0,
            bi_dir: BioDirection::Flush,
            bi_status: BioStatus::Pending,
            bi_error: 0,
            bi_data: Vec::new(),
            bi_end_io: None,
        }
    }

    pub fn sectors(&self) -> u64 {
        (self.bi_size + SECTOR_SIZE - 1) as u64 / SECTOR_SIZE as u64
    }

    pub fn is_read(&self) -> bool {
        self.bi_dir == BioDirection::Read
    }

    pub fn is_write(&self) -> bool {
        self.bi_dir == BioDirection::Write
    }
}

// ── Block device ────────────────────────────────────────────────────────

/// Operations that a block device driver must implement.
pub struct BlockDeviceOps {
    pub submit_bio: fn(u32, &mut Bio) -> Result<(), &'static str>,
    pub get_capacity: fn(u32) -> u64, // in sectors
    pub get_name: fn(u32) -> &'static str,
    /// Driver-private identifier (e.g. md array id).
    pub driver_data: u32,
}

/// A registered block device.
pub struct BlockDevice {
    pub name: String,
    pub major: u32,
    pub minor: u32,
    pub capacity_sectors: u64,
    pub ops: BlockDeviceOps,
    pub queue: Mutex<RequestQueue>,
    pub stats: BlockDeviceStats,
}

#[derive(Debug, Default)]
pub struct BlockDeviceStats {
    pub read_count: AtomicU64,
    pub read_sectors: AtomicU64,
    pub write_count: AtomicU64,
    pub write_sectors: AtomicU64,
    pub flush_count: AtomicU64,
    pub error_count: AtomicU64,
}

// ── Request queue ───────────────────────────────────────────────────────

/// A request queue collects and dispatches bios to the device.
struct RequestQueue {
    pending: Vec<Bio>,
    in_flight: usize,
    max_in_flight: usize,
    queue_depth: usize,
}

impl RequestQueue {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            in_flight: 0,
            max_in_flight: 64,
            queue_depth: 128,
        }
    }

    /// Add a bio to the queue. Attempts to merge with existing requests.
    fn add_bio(&mut self, mut bio: Bio) {
        // Try to merge with the last pending bio if contiguous
        if let Some(last) = self.pending.last_mut() {
            if last.bi_dir == bio.bi_dir
                && last.bi_sector + last.sectors() == bio.bi_sector
                && last.bi_size + bio.bi_size <= 4096 * BIO_MAX_PAGES
            {
                // Merge: append data for writes
                if bio.is_write() {
                    last.bi_data.extend_from_slice(&bio.bi_data);
                    last.bi_size += bio.bi_size;
                } else {
                    // For reads, just extend the size
                    last.bi_size += bio.bi_size;
                    last.bi_data.resize(last.bi_size, 0);
                }
                return;
            }
        }

        self.pending.push(bio);
    }

    /// Pop the next bio to dispatch. Uses simple FIFO for now.
    fn pop_bio(&mut self) -> Option<Bio> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }
}

// ── Global block device registry ────────────────────────────────────────

static BLOCK_DEVICES: RwLock<BTreeMap<(u32, u32), Box<BlockDevice>>> = RwLock::new(BTreeMap::new());
static NEXT_MAJOR: AtomicU64 = AtomicU64::new(200); // Start at 200 for dynamic majors

// ── Registration ────────────────────────────────────────────────────────

/// Register a block device. Returns the (major, minor) pair.
pub fn register_block_device(name: &str, minor: u32, ops: BlockDeviceOps) -> (u32, u32) {
    let major = NEXT_MAJOR.fetch_add(1, Ordering::SeqCst) as u32;
    let capacity = (ops.get_capacity)(ops.driver_data);
    let dev_name = (ops.get_name)(ops.driver_data);

    let dev = Box::new(BlockDevice {
        name: String::from(dev_name),
        major,
        minor,
        capacity_sectors: capacity,
        ops,
        queue: Mutex::new(RequestQueue::new()),
        stats: BlockDeviceStats::default(),
    });

    BLOCK_DEVICES.write().insert((major, minor), dev);

    crate::serial_println!(
        "[block] registered {} (major={}, minor={}, capacity={} sectors)",
        name,
        major,
        minor,
        capacity
    );

    (major, minor)
}

/// Register a block device with a specific major number.
pub fn register_block_device_major(
    name: &str,
    major: u32,
    minor: u32,
    ops: BlockDeviceOps,
) -> Result<(), &'static str> {
    let capacity = (ops.get_capacity)(ops.driver_data);
    let dev_name = (ops.get_name)(ops.driver_data);

    let dev = Box::new(BlockDevice {
        name: String::from(dev_name),
        major,
        minor,
        capacity_sectors: capacity,
        ops,
        queue: Mutex::new(RequestQueue::new()),
        stats: BlockDeviceStats::default(),
    });

    let mut devices = BLOCK_DEVICES.write();
    if devices.contains_key(&(major, minor)) {
        return Err("device already registered");
    }
    devices.insert((major, minor), dev);

    crate::serial_println!(
        "[block] registered {} (major={}, minor={}, capacity={} sectors)",
        name,
        major,
        minor,
        capacity
    );

    Ok(())
}

/// Unregister a block device.
pub fn unregister_block_device(major: u32, minor: u32) {
    BLOCK_DEVICES.write().remove(&(major, minor));
    crate::serial_println!("[block] unregistered major={}, minor={}", major, minor);
}

/// Look up a block device by (major, minor).
pub fn get_block_device(major: u32, minor: u32) -> bool {
    BLOCK_DEVICES.read().contains_key(&(major, minor))
}

/// List all registered block devices.
pub fn list_block_devices() -> Vec<(u32, u32, String, u64)> {
    BLOCK_DEVICES
        .read()
        .iter()
        .map(|(&(major, minor), dev)| (major, minor, dev.name.clone(), dev.capacity_sectors))
        .collect()
}

// ── Bio submission ──────────────────────────────────────────────────────

/// Submit a bio to the appropriate block device.
pub fn submit_bio(major: u32, minor: u32, bio: Bio) -> Result<(), &'static str> {
    let pid = crate::process::current_pid();
    if pid != 0 {
        match bio.bi_dir {
            BioDirection::Read => crate::cgroup::charge_blkio_read(pid, bio.bi_size as u64),
            BioDirection::Write => crate::cgroup::charge_blkio_write(pid, bio.bi_size as u64),
            _ => {}
        }
    }
    {
        let devices = BLOCK_DEVICES.read();
        let Some(dev) = devices.get(&(major, minor)) else {
            return Err("device not found");
        };

        // Update stats
        match bio.bi_dir {
            BioDirection::Read => {
                dev.stats.read_count.fetch_add(1, Ordering::Relaxed);
                dev.stats
                    .read_sectors
                    .fetch_add(bio.sectors(), Ordering::Relaxed);
            }
            BioDirection::Write => {
                dev.stats.write_count.fetch_add(1, Ordering::Relaxed);
                dev.stats
                    .write_sectors
                    .fetch_add(bio.sectors(), Ordering::Relaxed);
            }
            BioDirection::Flush => {
                dev.stats.flush_count.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }

        // Add to queue
        dev.queue.lock().add_bio(bio);
    }

    // Process the queue immediately (simplified — no async dispatch yet)
    process_queue(major, minor)
}

/// Process pending bios in a device's request queue.
fn process_queue(major: u32, minor: u32) -> Result<(), &'static str> {
    // Check device exists first
    if !BLOCK_DEVICES.read().contains_key(&(major, minor)) {
        return Err("device not found");
    }

    loop {
        // Pop bio from queue
        let bio_opt: Option<Bio> = {
            let devices = BLOCK_DEVICES.read();
            devices
                .get(&(major, minor))
                .and_then(|dev| dev.queue.lock().pop_bio())
        };

        let Some(mut bio) = bio_opt else { break };

        // Submit to device driver
        let result = {
            let devices = BLOCK_DEVICES.read();
            devices
                .get(&(major, minor))
                .map(|dev| (dev.ops.submit_bio)(dev.ops.driver_data, &mut bio))
                .unwrap_or(Err("device not found"))
        };

        match result {
            Ok(()) => {
                bio.bi_status = BioStatus::Complete;
            }
            Err(e) => {
                bio.bi_status = BioStatus::Error;
                bio.bi_error = -5; // EIO
                let devices = BLOCK_DEVICES.read();
                if let Some(dev) = devices.get(&(major, minor)) {
                    dev.stats.error_count.fetch_add(1, Ordering::Relaxed);
                }
                crate::serial_println!("[block] I/O error on {}: {}", major, e);
            }
        }

        // Invoke completion callback
        if let Some(cb) = bio.bi_end_io {
            cb(&bio);
        }
    }

    Ok(())
}

// ── Convenience read/write API ──────────────────────────────────────────

/// Read sectors from a block device into a buffer.
/// This is a synchronous convenience wrapper around submit_bio.
pub fn read_sectors(
    major: u32,
    minor: u32,
    sector: u64,
    buf: &mut [u8],
) -> Result<(), &'static str> {
    let size = buf.len();
    let mut bio = Bio::new_read(sector, size);
    bio.bi_end_io = Some(|_b| {});

    // Submit and process — submit_bio processes the queue synchronously
    // The device driver's submit_bio handler fills bi_data on read
    let devices = BLOCK_DEVICES.read();
    let dev = devices.get(&(major, minor)).ok_or("device not found")?;
    (dev.ops.submit_bio)(dev.ops.driver_data, &mut bio).map_err(|_| "read I/O error")?;
    drop(devices);

    if bio.bi_status == BioStatus::Complete || bio.bi_status == BioStatus::Pending {
        let copy_len = size.min(bio.bi_data.len());
        buf[..copy_len].copy_from_slice(&bio.bi_data[..copy_len]);
        Ok(())
    } else {
        Err("read I/O error")
    }
}

/// Write sectors to a block device from a buffer.
pub fn write_sectors(major: u32, minor: u32, sector: u64, buf: &[u8]) -> Result<(), &'static str> {
    let bio = Bio::new_write(sector, buf.to_vec());
    submit_bio(major, minor, bio)
}

/// Flush a block device (ensure all writes are persistent).
pub fn flush_block_device(major: u32, minor: u32) -> Result<(), &'static str> {
    let bio = Bio::new_flush();
    submit_bio(major, minor, bio)
}

// ── I/O scheduler (simplified deadline) ─────────────────────────────────

/// Deadline scheduler: dispatches requests by deadline, with read priority.
pub struct DeadlineScheduler {
    read_fifo: Vec<(u64, u64)>,  // (deadline, sector)
    write_fifo: Vec<(u64, u64)>, // (deadline, sector)
    read_batch: usize,
    write_batch: usize,
    fifo_expire_ms: u64,
}

impl DeadlineScheduler {
    pub fn new() -> Self {
        Self {
            read_fifo: Vec::new(),
            write_fifo: Vec::new(),
            read_batch: 16,
            write_batch: 4,
            fifo_expire_ms: 500,
        }
    }

    pub fn add_read(&mut self, sector: u64, now: u64) {
        self.read_fifo.push((now + self.fifo_expire_ms, sector));
    }

    pub fn add_write(&mut self, sector: u64, now: u64) {
        self.write_fifo.push((now + self.fifo_expire_ms, sector));
    }

    /// Get the next sector to dispatch, preferring reads.
    pub fn next_request(&mut self, now: u64) -> Option<u64> {
        // Check for expired reads
        if let Some(&(deadline, sector)) = self.read_fifo.first() {
            if now >= deadline {
                self.read_fifo.remove(0);
                return Some(sector);
            }
        }

        // Check for expired writes
        if let Some(&(deadline, sector)) = self.write_fifo.first() {
            if now >= deadline {
                self.write_fifo.remove(0);
                return Some(sector);
            }
        }

        // Prefer reads (read batching)
        if let Some((_, sector)) = self.read_fifo.first().copied() {
            self.read_fifo.remove(0);
            return Some(sector);
        }

        // Then writes
        if let Some((_, sector)) = self.write_fifo.first().copied() {
            self.write_fifo.remove(0);
            return Some(sector);
        }

        None
    }
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    // Register virtio-blk as a block device if available
    if crate::drivers::virtio::blk::is_available() {
        register_virtio_blk();
    }

    crate::serial_println!("[block] block I/O layer initialized");
}

// ── VirtIO-blk adapter ──────────────────────────────────────────────────

/// driver_data value used to identify the virtio-blk adapter in BlockDeviceOps.
const VIRTIO_BLK_DRIVER_DATA: u32 = 0xDEAD_BEEF;

fn virtio_blk_submit_bio(_dev_id: u32, bio: &mut Bio) -> Result<(), &'static str> {
    match bio.bi_dir {
        BioDirection::Read => {
            let n = crate::drivers::virtio::blk::read_sectors(bio.bi_sector, &mut bio.bi_data)?;
            bio.bi_status = BioStatus::Complete;
            let _ = n;
            Ok(())
        }
        BioDirection::Write => {
            let n = crate::drivers::virtio::blk::write_sectors(bio.bi_sector, &bio.bi_data)?;
            bio.bi_status = BioStatus::Complete;
            let _ = n;
            Ok(())
        }
        BioDirection::Flush => {
            crate::drivers::virtio::blk::flush()?;
            bio.bi_status = BioStatus::Complete;
            Ok(())
        }
        BioDirection::Discard => {
            bio.bi_status = BioStatus::Error;
            Err("virtio-blk discard not supported")
        }
    }
}

fn virtio_blk_get_capacity(_dev_id: u32) -> u64 {
    crate::drivers::virtio::blk::capacity_sectors().unwrap_or(0)
}

fn virtio_blk_get_name(_dev_id: u32) -> &'static str {
    "virtio-blk"
}

/// Register the virtio-blk device in the block I/O layer.
fn register_virtio_blk() {
    let ops = BlockDeviceOps {
        submit_bio: virtio_blk_submit_bio,
        get_capacity: virtio_blk_get_capacity,
        get_name: virtio_blk_get_name,
        driver_data: VIRTIO_BLK_DRIVER_DATA,
    };
    let (major, minor) = register_block_device("virtio-blk", 0, ops);
    crate::serial_println!(
        "[block] virtio-blk registered (major={}, minor={}, capacity={} sectors)",
        major,
        minor,
        virtio_blk_get_capacity(0)
    );
}

// ── Stats ───────────────────────────────────────────────────────────────

pub fn block_device_stats(major: u32, minor: u32) -> Option<(u64, u64, u64, u64, u64, u64)> {
    let devices = BLOCK_DEVICES.read();
    devices.get(&(major, minor)).map(|dev| {
        (
            dev.stats.read_count.load(Ordering::Relaxed),
            dev.stats.read_sectors.load(Ordering::Relaxed),
            dev.stats.write_count.load(Ordering::Relaxed),
            dev.stats.write_sectors.load(Ordering::Relaxed),
            dev.stats.flush_count.load(Ordering::Relaxed),
            dev.stats.error_count.load(Ordering::Relaxed),
        )
    })
}
