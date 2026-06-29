//! Generic DMA engine (dmaengine) framework
//!
//! Provides channel allocation, memory copy, and slave/peripheral configuration
//! similar to Linux's dmaengine subsystem. Includes a software memcpy engine that
//! performs synchronous copies when no hardware DMA controller is present.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaDirection {
    MemToMem,
    MemToDev,
    DevToMem,
    DevToDev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBusWidth {
    OneByte,
    TwoBytes,
    FourBytes,
    EightBytes,
}

impl DmaBusWidth {
    pub fn bytes(self) -> usize {
        match self {
            DmaBusWidth::OneByte => 1,
            DmaBusWidth::TwoBytes => 2,
            DmaBusWidth::FourBytes => 4,
            DmaBusWidth::EightBytes => 8,
        }
    }
}

/// Peripheral/slave configuration for a DMA channel.
#[derive(Debug, Clone, Copy)]
pub struct DmaSlaveConfig {
    pub direction: DmaDirection,
    pub src_addr: u64,
    pub dst_addr: u64,
    pub src_width: DmaBusWidth,
    pub dst_width: DmaBusWidth,
    pub src_maxburst: u32,
    pub dst_maxburst: u32,
    pub device_width: DmaBusWidth,
}

impl Default for DmaSlaveConfig {
    fn default() -> Self {
        Self {
            direction: DmaDirection::MemToMem,
            src_addr: 0,
            dst_addr: 0,
            src_width: DmaBusWidth::FourBytes,
            dst_width: DmaBusWidth::FourBytes,
            src_maxburst: 1,
            dst_maxburst: 1,
            device_width: DmaBusWidth::FourBytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaTransferStatus {
    Pending,
    Complete,
    Error,
}

/// Result of a DMA transfer.
#[derive(Debug, Clone, Copy)]
pub struct DmaTransferResult {
    pub status: DmaTransferStatus,
    pub bytes_transferred: usize,
}

/// Operations implemented by a DMA controller driver.
pub struct DmaEngineOps {
    pub request_channel: fn() -> Result<u32, &'static str>,
    pub release_channel: fn(u32) -> Result<(), &'static str>,
    pub config_slave: fn(u32, &DmaSlaveConfig) -> Result<(), &'static str>,
    pub memcpy: fn(u32, *mut u8, *const u8, usize) -> Result<DmaTransferResult, &'static str>,
    pub get_name: fn() -> &'static str,
}

struct DmaEngine {
    name: String,
    channel_count: u32,
    ops: DmaEngineOps,
}

struct DmaChannelState {
    engine_id: u32,
    channel_id: u32,
    in_use: bool,
    slave_config: DmaSlaveConfig,
}

// ── Registry ────────────────────────────────────────────────────────────

static DMA_ENGINES: RwLock<BTreeMap<u32, DmaEngine>> = RwLock::new(BTreeMap::new());
static DMA_CHANNELS: RwLock<BTreeMap<u32, DmaChannelState>> = RwLock::new(BTreeMap::new());
static NEXT_ENGINE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_CHANNEL_HANDLE: AtomicU32 = AtomicU32::new(1);

// ── Software memcpy engine ────────────────────────────────────────────────

struct SoftwareChannel {
    configured: DmaSlaveConfig,
}

static SOFTWARE_CHANNELS: RwLock<BTreeMap<u32, SoftwareChannel>> = RwLock::new(BTreeMap::new());

fn software_request_channel() -> Result<u32, &'static str> {
    let handle = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    SOFTWARE_CHANNELS.write().insert(
        handle,
        SoftwareChannel {
            configured: DmaSlaveConfig::default(),
        },
    );
    DMA_CHANNELS.write().insert(
        handle,
        DmaChannelState {
            engine_id: 0,
            channel_id: handle,
            in_use: true,
            slave_config: DmaSlaveConfig::default(),
        },
    );
    Ok(handle)
}

fn software_release_channel(handle: u32) -> Result<(), &'static str> {
    SOFTWARE_CHANNELS
        .write()
        .remove(&handle)
        .ok_or("DMA channel not found")?;
    DMA_CHANNELS.write().remove(&handle);
    Ok(())
}

fn software_config_slave(handle: u32, config: &DmaSlaveConfig) -> Result<(), &'static str> {
    let mut channels = SOFTWARE_CHANNELS.write();
    let ch = channels.get_mut(&handle).ok_or("DMA channel not found")?;
    ch.configured = *config;
    if let Some(state) = DMA_CHANNELS.write().get_mut(&handle) {
        state.slave_config = *config;
    }
    Ok(())
}

fn software_memcpy(
    handle: u32,
    dst: *mut u8,
    src: *const u8,
    len: usize,
) -> Result<DmaTransferResult, &'static str> {
    if dst.is_null() || src.is_null() {
        return Err("DMA memcpy: null pointer");
    }
    if len == 0 {
        return Ok(DmaTransferResult {
            status: DmaTransferStatus::Complete,
            bytes_transferred: 0,
        });
    }

    let channels = SOFTWARE_CHANNELS.read();
    let _ch = channels.get(&handle).ok_or("DMA channel not found")?;

    // SAFETY: Caller guarantees valid, non-overlapping regions for the transfer length.
    unsafe {
        core::ptr::copy_nonoverlapping(src, dst, len);
    }

    Ok(DmaTransferResult {
        status: DmaTransferStatus::Complete,
        bytes_transferred: len,
    })
}

const SOFTWARE_DMA_OPS: DmaEngineOps = DmaEngineOps {
    request_channel: software_request_channel,
    release_channel: software_release_channel,
    config_slave: software_config_slave,
    memcpy: software_memcpy,
    get_name: || "software-memcpy",
};

// ── Public API ──────────────────────────────────────────────────────────

/// Register a DMA engine with the subsystem.
pub fn register_engine(
    name: &str,
    channel_count: u32,
    ops: DmaEngineOps,
) -> Result<u32, &'static str> {
    if channel_count == 0 {
        return Err("DMA engine must expose at least one channel");
    }
    let id = NEXT_ENGINE_ID.fetch_add(1, Ordering::SeqCst);
    DMA_ENGINES.write().insert(
        id,
        DmaEngine {
            name: String::from(name),
            channel_count,
            ops,
        },
    );
    Ok(id)
}

/// Allocate a channel from the named engine (defaults to software engine id 0).
pub fn request_channel(engine_id: u32) -> Result<u32, &'static str> {
    let engines = DMA_ENGINES.read();
    let engine = engines.get(&engine_id).ok_or("DMA engine not found")?;
    (engine.ops.request_channel)()
}

/// Release a previously allocated channel handle.
pub fn release_channel(handle: u32) -> Result<(), &'static str> {
    let channels = DMA_CHANNELS.read();
    let state = channels
        .get(&handle)
        .ok_or("DMA channel handle not found")?;
    let engine_id = state.engine_id;
    drop(channels);

    let engines = DMA_ENGINES.read();
    let engine = engines.get(&engine_id).ok_or("DMA engine not found")?;
    (engine.ops.release_channel)(handle)
}

/// Configure slave/peripheral parameters for a channel.
pub fn config_slave(handle: u32, config: &DmaSlaveConfig) -> Result<(), &'static str> {
    let channels = DMA_CHANNELS.read();
    let state = channels
        .get(&handle)
        .ok_or("DMA channel handle not found")?;
    let engine_id = state.engine_id;
    drop(channels);

    let engines = DMA_ENGINES.read();
    let engine = engines.get(&engine_id).ok_or("DMA engine not found")?;
    (engine.ops.config_slave)(handle, config)
}

/// Perform a memory-to-memory copy on a channel.
pub fn dma_memcpy(
    handle: u32,
    dst: *mut u8,
    src: *const u8,
    len: usize,
) -> Result<DmaTransferResult, &'static str> {
    let channels = DMA_CHANNELS.read();
    let state = channels
        .get(&handle)
        .ok_or("DMA channel handle not found")?;
    let engine_id = state.engine_id;
    drop(channels);

    let engines = DMA_ENGINES.read();
    let engine = engines.get(&engine_id).ok_or("DMA engine not found")?;
    (engine.ops.memcpy)(handle, dst, src, len)
}

/// Convenience wrapper: allocate a channel, copy, and release.
pub fn dma_memcpy_one_shot(
    dst: *mut u8,
    src: *const u8,
    len: usize,
) -> Result<DmaTransferResult, &'static str> {
    let handle = request_channel(0)?;
    let result = dma_memcpy(handle, dst, src, len);
    let _ = release_channel(handle);
    result
}

/// Number of registered DMA engines.
pub fn engine_count() -> usize {
    DMA_ENGINES.read().len()
}

/// Initialize the DMA subsystem and register the software memcpy engine.
pub fn init() -> Result<(), &'static str> {
    if !DMA_ENGINES.read().is_empty() {
        return Ok(());
    }

    register_engine("software-memcpy", 8, SOFTWARE_DMA_OPS)?;
    crate::serial_println!("dma: registered software-memcpy engine (8 channels)");
    Ok(())
}
