//! # VirtIO Block Device Driver
//!
//! Implements virtio-blk for paravirtualized block storage in QEMU/KVM.
//! Uses a virtqueue for read/write/flush requests with scatter-gather DMA.

use super::*;
use alloc::vec::Vec;
use spin::Mutex;

/// virtio-blk feature bits
const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;
const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;
const VIRTIO_BLK_F_RO: u64 = 1 << 5;
const VIRTIO_BLK_F_BLK_SIZE: u64 = 1 << 6;
const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;

/// virtio-blk request types
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_T_FLUSH: u32 = 4;

/// virtio-blk request status codes
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

/// virtio-blk request header
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioBlkReq {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

/// virtio-blk device config
#[repr(C)]
pub struct VirtioBlkConfig {
    pub capacity: u64,
    pub size_max: u32,
    pub seg_max: u32,
    pub blk_size: u32,
    pub physical_block_exp: u8,
    pub alignment_offset: u8,
    pub min_io_size: u16,
    pub opt_io_size: u32,
    pub writeback: u8,
    pub _padding: [u8; 3],
    pub max_discard_sectors: u32,
    pub max_discard_seg: u32,
    pub discard_sector_alignment: u32,
    pub max_write_zeroes_sectors: u32,
    pub max_write_zeroes_seg: u32,
    pub write_zeroes_may_unmap: u8,
    pub _padding2: [u8; 3],
}

/// Queue index for virtio-blk
const BLK_QUEUE: u16 = 0;

/// Default queue size
const QUEUE_SIZE: u16 = 32;

/// Sector size (512 bytes)
const SECTOR_SIZE: usize = 512;

/// VirtIO block driver state
pub struct VirtioBlk {
    transport: VirtioTransport,
    queue: Option<VirtQueue>,
    capacity_sectors: u64,
    read_only: bool,
    block_size: u32,
}

impl VirtioBlk {
    /// Create and initialize a virtio-blk device
    pub fn new(transport: VirtioTransport) -> Result<Self, &'static str> {
        // Negotiate features
        let driver_features = VIRTIO_BLK_F_FLUSH | VIRTIO_BLK_F_BLK_SIZE | VIRTIO_BLK_F_SEG_MAX;
        transport.init_device(driver_features)?;

        // Read device config
        let capacity = transport.read_device_config32(0) as u64
            | ((transport.read_device_config32(4) as u64) << 32);

        let read_only = (transport.read_device_features() & VIRTIO_BLK_F_RO) != 0;

        let block_size = if (transport.read_device_features() & VIRTIO_BLK_F_BLK_SIZE) != 0 {
            transport.read_device_config32(20)
        } else {
            SECTOR_SIZE as u32
        };

        crate::serial_println!(
            "virtio-blk: capacity={} sectors ({} MB), block_size={}, ro={}",
            capacity,
            capacity * SECTOR_SIZE as u64 / (1024 * 1024),
            block_size,
            read_only
        );

        // Set up the request queue
        let q_size = transport.select_queue(BLK_QUEUE);
        let q_size = if q_size == 0 {
            QUEUE_SIZE
        } else {
            q_size.min(QUEUE_SIZE)
        };
        let notify_off =
            unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
        let queue = VirtQueue::new(q_size, notify_off)?;
        transport.setup_queue(&queue);

        // Set DRIVER_OK
        transport.set_driver_ok();

        crate::serial_println!("virtio-blk: initialized (queue_size={})", q_size);

        Ok(VirtioBlk {
            transport,
            queue: Some(queue),
            capacity_sectors: capacity,
            read_only,
            block_size,
        })
    }

    /// Get capacity in sectors
    pub fn capacity_sectors(&self) -> u64 {
        self.capacity_sectors
    }

    /// Get capacity in bytes
    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_sectors * SECTOR_SIZE as u64
    }

    /// Check if device is read-only
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Read sectors from the device
    pub fn read_sectors(&mut self, sector: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
        let queue = self.queue.as_mut().unwrap();
        let sector_count = (buf.len() / SECTOR_SIZE) as u32;
        if sector_count == 0 {
            return Err("virtio-blk: buffer too small for a sector");
        }

        // Allocate descriptors: 1 for request header, 1 for data, 1 for status
        let desc_hdr = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;
        let desc_data = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;
        let desc_status = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;

        // Set up request header
        let req = VirtioBlkReq {
            req_type: VIRTIO_BLK_T_IN,
            reserved: 0,
            sector,
        };
        let req_phys = super::virt_to_phys(&req as *const _ as usize);
        queue.set_desc(
            desc_hdr,
            req_phys,
            core::mem::size_of::<VirtioBlkReq>() as u32,
            desc_flags::NEXT,
            desc_data,
        );

        // Set up data descriptor (write — device writes to it)
        let data_phys = super::virt_to_phys(buf.as_ptr() as usize);
        queue.set_desc(
            desc_data,
            data_phys,
            buf.len() as u32,
            desc_flags::NEXT | desc_flags::WRITE,
            desc_status,
        );

        // Set up status descriptor (write)
        let mut status = 0xFFu8;
        let status_phys = super::virt_to_phys(&status as *const _ as usize);
        queue.set_desc(desc_status, status_phys, 1, desc_flags::WRITE, 0);

        // Submit and notify
        queue.submit(desc_hdr);
        self.transport.notify(queue);

        // Wait for completion (poll the used ring)
        let mut timeout = 1_000_000u32;
        loop {
            if queue.has_used() {
                let (id, _len) = queue.pop_used().unwrap();
                // Free descriptors
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_data);
                queue.free_desc(desc_status);

                if status == VIRTIO_BLK_S_OK {
                    return Ok(buf.len());
                } else {
                    return Err("virtio-blk: read I/O error");
                }
            }
            if timeout == 0 {
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_data);
                queue.free_desc(desc_status);
                return Err("virtio-blk: read timeout");
            }
            timeout -= 1;
            core::hint::spin_loop();
        }
    }

    /// Write sectors to the device
    pub fn write_sectors(&mut self, sector: u64, buf: &[u8]) -> Result<usize, &'static str> {
        if self.read_only {
            return Err("virtio-blk: device is read-only");
        }

        let queue = self.queue.as_mut().unwrap();
        let sector_count = (buf.len() / SECTOR_SIZE) as u32;
        if sector_count == 0 {
            return Err("virtio-blk: buffer too small for a sector");
        }

        let desc_hdr = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;
        let desc_data = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;
        let desc_status = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;

        let req = VirtioBlkReq {
            req_type: VIRTIO_BLK_T_OUT,
            reserved: 0,
            sector,
        };
        let req_phys = super::virt_to_phys(&req as *const _ as usize);
        queue.set_desc(
            desc_hdr,
            req_phys,
            core::mem::size_of::<VirtioBlkReq>() as u32,
            desc_flags::NEXT,
            desc_data,
        );

        let data_phys = super::virt_to_phys(buf.as_ptr() as usize);
        queue.set_desc(
            desc_data,
            data_phys,
            buf.len() as u32,
            desc_flags::NEXT,
            desc_status,
        );

        let mut status = 0xFFu8;
        let status_phys = super::virt_to_phys(&status as *const _ as usize);
        queue.set_desc(desc_status, status_phys, 1, desc_flags::WRITE, 0);

        queue.submit(desc_hdr);
        self.transport.notify(queue);

        // Wait for completion
        let mut timeout = 1_000_000u32;
        loop {
            if queue.has_used() {
                let (id, _len) = queue.pop_used().unwrap();
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_data);
                queue.free_desc(desc_status);

                if status == VIRTIO_BLK_S_OK {
                    return Ok(buf.len());
                } else {
                    return Err("virtio-blk: write I/O error");
                }
            }
            if timeout == 0 {
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_data);
                queue.free_desc(desc_status);
                return Err("virtio-blk: write timeout");
            }
            timeout -= 1;
            core::hint::spin_loop();
        }
    }

    /// Flush pending writes to the device
    pub fn flush(&mut self) -> Result<(), &'static str> {
        let queue = self.queue.as_mut().unwrap();

        let desc_hdr = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;
        let desc_status = queue
            .alloc_desc()
            .ok_or("virtio-blk: no free descriptors")?;

        let req = VirtioBlkReq {
            req_type: VIRTIO_BLK_T_FLUSH,
            reserved: 0,
            sector: 0,
        };
        let req_phys = super::virt_to_phys(&req as *const _ as usize);
        queue.set_desc(
            desc_hdr,
            req_phys,
            core::mem::size_of::<VirtioBlkReq>() as u32,
            desc_flags::NEXT,
            desc_status,
        );

        let mut status = 0xFFu8;
        let status_phys = super::virt_to_phys(&status as *const _ as usize);
        queue.set_desc(desc_status, status_phys, 1, desc_flags::WRITE, 0);

        queue.submit(desc_hdr);
        self.transport.notify(queue);

        let mut timeout = 1_000_000u32;
        loop {
            if queue.has_used() {
                let (id, _len) = queue.pop_used().unwrap();
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_status);

                if status == VIRTIO_BLK_S_OK {
                    return Ok(());
                } else {
                    return Err("virtio-blk: flush I/O error");
                }
            }
            if timeout == 0 {
                queue.free_desc(desc_hdr);
                queue.free_desc(desc_status);
                return Err("virtio-blk: flush timeout");
            }
            timeout -= 1;
            core::hint::spin_loop();
        }
    }
}

/// Global virtio-blk driver instance
static VIRTIO_BLK: Mutex<Option<VirtioBlk>> = Mutex::new(None);

/// Initialize virtio-blk from a transport
pub fn init_virtio_blk(transport: VirtioTransport) -> Result<(), &'static str> {
    let blk = VirtioBlk::new(transport)?;
    *VIRTIO_BLK.lock() = Some(blk);
    Ok(())
}

/// Get the global virtio-blk driver (closure-based access)
pub fn with_virtio_blk<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut VirtioBlk) -> R,
{
    let mut guard = VIRTIO_BLK.lock();
    guard.as_mut().map(f)
}

/// Read sectors from the virtio-blk device
pub fn read_sectors(sector: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    match VIRTIO_BLK.lock().as_mut() {
        Some(blk) => blk.read_sectors(sector, buf),
        None => Err("virtio-blk: not initialized"),
    }
}

/// Write sectors to the virtio-blk device
pub fn write_sectors(sector: u64, buf: &[u8]) -> Result<usize, &'static str> {
    match VIRTIO_BLK.lock().as_mut() {
        Some(blk) => blk.write_sectors(sector, buf),
        None => Err("virtio-blk: not initialized"),
    }
}

/// Check if virtio-blk is available
pub fn is_available() -> bool {
    VIRTIO_BLK.lock().is_some()
}

/// Get capacity in sectors
pub fn capacity_sectors() -> Option<u64> {
    VIRTIO_BLK.lock().as_ref().map(|blk| blk.capacity_sectors())
}

/// Flush pending writes to the device
pub fn flush() -> Result<(), &'static str> {
    match VIRTIO_BLK.lock().as_mut() {
        Some(blk) => blk.flush(),
        None => Err("virtio-blk: not initialized"),
    }
}
