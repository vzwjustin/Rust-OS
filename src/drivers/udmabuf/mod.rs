//! UDMABUF (Userspace DMA Buffer) subsystem
//!
//! Provides userspace-created DMA buffers backed by memfd pages.
//! Mirrors Linux's `drivers/dma-buf/udmabuf.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// UDMABUF creation flags (Linux `struct udmabuf_create`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdmabufFlags(pub u32);

impl UdmabufFlags {
    pub const NONE: Self = UdmabufFlags(0);
    pub const SYNC_WRITE: Self = UdmabufFlags(1);
    pub const SYNC_READ: Self = UdmabufFlags(2);
    pub const SYNC_RW: Self = UdmabufFlags(3);
}

/// UDMABUF create request (Linux `struct udmabuf_create`).
#[derive(Debug, Clone)]
pub struct UdmabufCreate {
    pub memfd_fd: u32,
    pub offset: u64,
    pub size: u64,
    pub flags: UdmabufFlags,
}

/// UDMABUF create list (Linux `struct udmabuf_create_list`).
#[derive(Debug, Clone)]
pub struct UdmabufCreateList {
    pub list: Vec<UdmabufCreate>,
    pub flags: UdmabufFlags,
}

/// UDMABUF instance (Linux `struct udmabuf`).
pub struct Udmabuf {
    pub id: u32,
    pub dmabuf_fd: u32,
    pub size: u64,
    pub page_count: u64,
    pub flags: UdmabufFlags,
    pub offsets: Vec<(u64, u64)>, // (offset, length) per memfd
    pub pinned: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static UDMABUF_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DMABUF_FD_COUNTER: AtomicU32 = AtomicU32::new(100);

static UDMABUFS: RwLock<BTreeMap<u32, Udmabuf>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Create a UDMABUF from a memfd (Linux `UDMABUF_CREATE` ioctl).
pub fn create(req: UdmabufCreate) -> Result<u32, &'static str> {
    if req.size == 0 {
        return Err("UDMABUF size must be non-zero");
    }
    if req.size % 4096 != 0 {
        return Err("UDMABUF size must be page-aligned");
    }

    let id = UDMABUF_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dmabuf_fd = DMABUF_FD_COUNTER.fetch_add(1, Ordering::SeqCst);
    let page_count = req.size / 4096;

    let mut offsets = Vec::new();
    offsets.push((req.offset, req.size));

    let buf = Udmabuf {
        id,
        dmabuf_fd,
        size: req.size,
        page_count,
        flags: req.flags,
        offsets,
        pinned: false,
    };
    UDMABUFS.write().insert(id, buf);
    Ok(dmabuf_fd)
}

/// Create a UDMABUF from multiple memfd segments (Linux `UDMABUF_CREATE_LIST` ioctl).
pub fn create_list(req: UdmabufCreateList) -> Result<u32, &'static str> {
    if req.list.is_empty() {
        return Err("UDMABUF list must be non-empty");
    }

    let total_size: u64 = req.list.iter().map(|r| r.size).sum();
    if total_size == 0 {
        return Err("UDMABUF total size must be non-zero");
    }

    let id = UDMABUF_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dmabuf_fd = DMABUF_FD_COUNTER.fetch_add(1, Ordering::SeqCst);
    let page_count = total_size / 4096;

    let mut offsets = Vec::new();
    for item in &req.list {
        offsets.push((item.offset, item.size));
    }

    let buf = Udmabuf {
        id,
        dmabuf_fd,
        size: total_size,
        page_count,
        flags: req.flags,
        offsets,
        pinned: false,
    };
    UDMABUFS.write().insert(id, buf);
    Ok(dmabuf_fd)
}

/// Pin UDMABUF pages for DMA transfer.
pub fn pin(udmabuf_id: u32) -> Result<(), &'static str> {
    let mut bufs = UDMABUFS.write();
    let buf = bufs.get_mut(&udmabuf_id).ok_or("UDMABUF not found")?;
    buf.pinned = true;
    Ok(())
}

/// Unpin UDMABUF pages.
pub fn unpin(udmabuf_id: u32) -> Result<(), &'static str> {
    let mut bufs = UDMABUFS.write();
    let buf = bufs.get_mut(&udmabuf_id).ok_or("UDMABUF not found")?;
    buf.pinned = false;
    Ok(())
}

/// Get UDMABUF info by dmabuf fd.
pub fn get_by_fd(dmabuf_fd: u32) -> Result<(u32, u64, u64), &'static str> {
    let bufs = UDMABUFS.read();
    for (id, buf) in bufs.iter() {
        if buf.dmabuf_fd == dmabuf_fd {
            return Ok((*id, buf.size, buf.page_count));
        }
    }
    Err("UDMABUF not found for fd")
}

/// Destroy a UDMABUF.
pub fn destroy(udmabuf_id: u32) -> Result<(), &'static str> {
    if UDMABUFS.write().remove(&udmabuf_id).is_none() {
        return Err("UDMABUF not found");
    }
    Ok(())
}

/// List all UDMABUFs.
pub fn list_buffers() -> Vec<(u32, u32, u64, u64, bool)> {
    UDMABUFS
        .read()
        .iter()
        .map(|(id, b)| (*id, b.dmabuf_fd, b.size, b.page_count, b.pinned))
        .collect()
}

/// Count registered buffers.
pub fn buffer_count() -> usize {
    UDMABUFS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Create a sample 1MB UDMABUF
    let req = UdmabufCreate {
        memfd_fd: 3,
        offset: 0,
        size: 1024 * 1024,
        flags: UdmabufFlags::SYNC_RW,
    };
    let fd = create(req)?;
    if let Ok((id, _, _)) = get_by_fd(fd) {
        pin(id)?;
    }
    Ok(())
}
