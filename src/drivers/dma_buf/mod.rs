//! DMA-BUF shared buffer framework
//!
//! Provides file descriptor-based shared memory buffers for passing
//! pixel data, textures, and other large buffers between drivers and
//! userspace. Mirrors Linux's `drivers/dma-buf/dma-buf.c` with
//! exporter/importer registration, attachment tracking, and mapping.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// DMA-BUF access direction (Linux `enum dma_data_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufDirection {
    ToDevice,
    FromDevice,
    Bidirectional,
}

/// Buffer attachment state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufAttachmentState {
    Attached,
    Mapped,
    Detached,
}

/// Operations implemented by a dma-buf exporter (Linux `struct dma_buf_ops`).
pub struct DmaBufOps {
    pub attach: fn(buf_id: u32, importer: &str) -> Result<u64, &'static str>,
    pub detach: fn(buf_id: u32, attachment_id: u32) -> Result<(), &'static str>,
    pub map: fn(
        buf_id: u32,
        attachment_id: u32,
        direction: DmaBufDirection,
    ) -> Result<u64, &'static str>,
    pub unmap: fn(buf_id: u32, attachment_id: u32) -> Result<(), &'static str>,
    pub release: fn(buf_id: u32) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct DmaBuf {
    id: u32,
    name: String,
    size: usize,
    ops: &'static DmaBufOps,
    attachments: BTreeMap<u32, DmaBufAttachment>,
    fd: Option<u32>,
}

struct DmaBufAttachment {
    id: u32,
    importer: String,
    state: DmaBufAttachmentState,
    sg_addr: u64,
    direction: DmaBufDirection,
}

// ── Registry ────────────────────────────────────────────────────────────

static DMA_BUFS: RwLock<BTreeMap<u32, DmaBuf>> = RwLock::new(BTreeMap::new());
static NEXT_BUF_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_ATTACH_ID: AtomicU32 = AtomicU32::new(1);

// ── Default ops for simple memory-backed buffers ────────────────────────

static mut SIMPLE_BUF_ADDR: u64 = 0;
static mut SIMPLE_BUF_SIZE: usize = 0;

fn simple_attach(_buf_id: u32, _importer: &str) -> Result<u64, &'static str> {
    Ok(unsafe { SIMPLE_BUF_ADDR })
}

fn simple_detach(_buf_id: u32, _attachment_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn simple_map(
    _buf_id: u32,
    _attachment_id: u32,
    _direction: DmaBufDirection,
) -> Result<u64, &'static str> {
    Ok(unsafe { SIMPLE_BUF_ADDR })
}

fn simple_unmap(_buf_id: u32, _attachment_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn simple_release(buf_id: u32) -> Result<(), &'static str> {
    let _ = buf_id;
    Ok(())
}

fn simple_name() -> &'static str {
    "simple-dma-buf"
}

pub static SIMPLE_DMA_BUF_OPS: DmaBufOps = DmaBufOps {
    attach: simple_attach,
    detach: simple_detach,
    map: simple_map,
    unmap: simple_unmap,
    release: simple_release,
    get_name: simple_name,
};

// ── Public API ──────────────────────────────────────────────────────────

/// Export a dma-buf (Linux `dma_buf_export`).
pub fn export(name: &str, size: usize, ops: &'static DmaBufOps) -> Result<u32, &'static str> {
    if size == 0 {
        return Err("dma-buf size must be non-zero");
    }
    let id = NEXT_BUF_ID.fetch_add(1, Ordering::SeqCst);
    DMA_BUFS.write().insert(
        id,
        DmaBuf {
            id,
            name: String::from(name),
            size,
            ops,
            attachments: BTreeMap::new(),
            fd: None,
        },
    );
    Ok(id)
}

/// Attach an importer to a dma-buf (Linux `dma_buf_attach`).
pub fn attach(buf_id: u32, importer: &str) -> Result<u32, &'static str> {
    let ops = {
        let bufs = DMA_BUFS.read();
        let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
        buf.ops
    };

    let sg_addr = (ops.attach)(buf_id, importer)?;
    let attach_id = NEXT_ATTACH_ID.fetch_add(1, Ordering::SeqCst);

    let mut bufs = DMA_BUFS.write();
    let buf = bufs.get_mut(&buf_id).ok_or("dma-buf vanished")?;
    buf.attachments.insert(
        attach_id,
        DmaBufAttachment {
            id: attach_id,
            importer: String::from(importer),
            state: DmaBufAttachmentState::Attached,
            sg_addr,
            direction: DmaBufDirection::Bidirectional,
        },
    );
    Ok(attach_id)
}

/// Detach an importer from a dma-buf (Linux `dma_buf_detach`).
pub fn detach(buf_id: u32, attachment_id: u32) -> Result<(), &'static str> {
    let ops = {
        let bufs = DMA_BUFS.read();
        let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
        buf.ops
    };

    (ops.detach)(buf_id, attachment_id)?;

    let mut bufs = DMA_BUFS.write();
    let buf = bufs.get_mut(&buf_id).ok_or("dma-buf vanished")?;
    buf.attachments.remove(&attachment_id);
    Ok(())
}

/// Map an attachment for DMA access (Linux `dma_buf_map_attachment`).
pub fn map_attachment(
    buf_id: u32,
    attachment_id: u32,
    direction: DmaBufDirection,
) -> Result<u64, &'static str> {
    let ops = {
        let bufs = DMA_BUFS.read();
        let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
        buf.ops
    };

    let addr = (ops.map)(buf_id, attachment_id, direction)?;

    let mut bufs = DMA_BUFS.write();
    let buf = bufs.get_mut(&buf_id).ok_or("dma-buf vanished")?;
    let attach = buf
        .attachments
        .get_mut(&attachment_id)
        .ok_or("Attachment not found")?;
    attach.state = DmaBufAttachmentState::Mapped;
    attach.direction = direction;
    Ok(addr)
}

/// Unmap an attachment (Linux `dma_buf_unmap_attachment`).
pub fn unmap_attachment(buf_id: u32, attachment_id: u32) -> Result<(), &'static str> {
    let ops = {
        let bufs = DMA_BUFS.read();
        let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
        buf.ops
    };

    (ops.unmap)(buf_id, attachment_id)?;

    let mut bufs = DMA_BUFS.write();
    let buf = bufs.get_mut(&buf_id).ok_or("dma-buf vanished")?;
    let attach = buf
        .attachments
        .get_mut(&attachment_id)
        .ok_or("Attachment not found")?;
    attach.state = DmaBufAttachmentState::Attached;
    Ok(())
}

/// Get buffer size.
pub fn get_size(buf_id: u32) -> Result<usize, &'static str> {
    let bufs = DMA_BUFS.read();
    let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
    Ok(buf.size)
}

/// Get buffer name.
pub fn get_name(buf_id: u32) -> Result<String, &'static str> {
    let bufs = DMA_BUFS.read();
    let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
    Ok(buf.name.clone())
}

/// Release a dma-buf (Linux `dma_buf_put`).
pub fn release(buf_id: u32) -> Result<(), &'static str> {
    let ops = {
        let bufs = DMA_BUFS.read();
        let buf = bufs.get(&buf_id).ok_or("dma-buf not found")?;
        buf.ops
    };

    (ops.release)(buf_id)?;
    DMA_BUFS
        .write()
        .remove(&buf_id)
        .ok_or("dma-buf not found")?;
    Ok(())
}

/// Number of active dma-bufs.
pub fn buf_count() -> usize {
    DMA_BUFS.read().len()
}

/// Total number of attachments across all buffers.
pub fn total_attachments() -> usize {
    DMA_BUFS.read().values().map(|b| b.attachments.len()).sum()
}

/// Initialize dma-buf subsystem.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("dma_buf: subsystem ready");
    Ok(())
}
