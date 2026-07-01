//! Memory (memory controller) driver subsystem
//!
//! Provides memory controller device framework for ECC, memory hotplug, etc.
//! Mirrors Linux's `drivers/memory/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Memory controller type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemControllerType {
    Ddr3,
    Ddr4,
    Ddr5,
    Lpddr4,
    Lpddr5,
    Hbm,
    Generic,
}

/// Memory controller (Linux `struct mem_ctlr`).
pub struct MemController {
    pub id: u32,
    pub name: String,
    pub ctrl_type: MemControllerType,
    pub ops: MemControllerOps,
    pub dimm_count: u32,
    pub ecc_support: bool,
    pub max_capacity_mb: u64,
}

/// Memory controller operations.
pub struct MemControllerOps {
    pub init: fn(ctrl_id: u32) -> Result<(), &'static str>,
    pub read_ecc_errors: fn(ctrl_id: u32) -> EccErrorCount,
    pub get_temperature: fn(ctrl_id: u32) -> Result<u32, &'static str>,
    pub set_scrub_rate: fn(ctrl_id: u32, bps: u64) -> Result<(), &'static str>,
}

/// ECC error count.
#[derive(Debug, Clone, Copy, Default)]
pub struct EccErrorCount {
    pub correctable: u64,
    pub uncorrectable: u64,
}

/// Memory region (Linux `struct memory_block`).
pub struct MemoryBlock {
    pub id: u32,
    pub phys_start: u64,
    pub size_mb: u64,
    pub state: MemBlockState,
    pub online: bool,
}

/// Memory block state (Linux `enum mem_block_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemBlockState {
    Offline,
    Online,
    GoingOffline,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static BLOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MEM_CONTROLLERS: RwLock<BTreeMap<u32, MemController>> = RwLock::new(BTreeMap::new());
static MEM_BLOCKS: RwLock<BTreeMap<u32, MemoryBlock>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a memory controller.
pub fn register_controller(
    name: &str,
    ctrl_type: MemControllerType,
    ops: MemControllerOps,
    dimm_count: u32,
    ecc_support: bool,
    max_capacity_mb: u64,
) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = MemController {
        id,
        name: String::from(name),
        ctrl_type,
        ops,
        dimm_count,
        ecc_support,
        max_capacity_mb,
    };
    MEM_CONTROLLERS.write().insert(id, ctrl);
    Ok(id)
}

/// Register a memory block (for memory hotplug).
pub fn register_block(phys_start: u64, size_mb: u64) -> Result<u32, &'static str> {
    let id = BLOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let block = MemoryBlock {
        id,
        phys_start,
        size_mb,
        state: MemBlockState::Online,
        online: true,
    };
    MEM_BLOCKS.write().insert(id, block);
    Ok(id)
}

/// Read ECC error counts from a controller.
pub fn read_ecc_errors(ctrl_id: u32) -> Result<EccErrorCount, &'static str> {
    let controllers = MEM_CONTROLLERS.read();
    let ctrl = controllers
        .get(&ctrl_id)
        .ok_or("Memory controller not found")?;
    Ok((ctrl.ops.read_ecc_errors)(ctrl_id))
}

/// Get temperature from a memory controller.
pub fn get_temperature(ctrl_id: u32) -> Result<u32, &'static str> {
    let controllers = MEM_CONTROLLERS.read();
    let ctrl = controllers
        .get(&ctrl_id)
        .ok_or("Memory controller not found")?;
    (ctrl.ops.get_temperature)(ctrl_id)
}

/// Set memory scrub rate.
pub fn set_scrub_rate(ctrl_id: u32, bps: u64) -> Result<(), &'static str> {
    let controllers = MEM_CONTROLLERS.read();
    let ctrl = controllers
        .get(&ctrl_id)
        .ok_or("Memory controller not found")?;
    (ctrl.ops.set_scrub_rate)(ctrl_id, bps)
}

/// Online a memory block (Linux `memory_block_online`).
pub fn online_block(block_id: u32) -> Result<(), &'static str> {
    let mut blocks = MEM_BLOCKS.write();
    let block = blocks.get_mut(&block_id).ok_or("Memory block not found")?;
    block.state = MemBlockState::Online;
    block.online = true;
    Ok(())
}

/// Offline a memory block (Linux `memory_block_offline`).
pub fn offline_block(block_id: u32) -> Result<(), &'static str> {
    let mut blocks = MEM_BLOCKS.write();
    let block = blocks.get_mut(&block_id).ok_or("Memory block not found")?;
    block.state = MemBlockState::Offline;
    block.online = false;
    Ok(())
}

/// List all memory controllers.
pub fn list_controllers() -> Vec<(u32, String, MemControllerType, bool)> {
    MEM_CONTROLLERS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.ctrl_type, c.ecc_support))
        .collect()
}

/// List all memory blocks.
pub fn list_blocks() -> Vec<(u32, u64, u64, bool)> {
    MEM_BLOCKS
        .read()
        .iter()
        .map(|(id, b)| (*id, b.phys_start, b.size_mb, b.online))
        .collect()
}

/// Count controllers.
pub fn controller_count() -> usize {
    MEM_CONTROLLERS.read().len()
}

// ── Software memory controller ──────────────────────────────────────────

fn sw_init(_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_ecc(_id: u32) -> EccErrorCount {
    EccErrorCount::default()
}
fn sw_temp(_id: u32) -> Result<u32, &'static str> {
    Ok(45)
}
fn sw_scrub(_id: u32, _bps: u64) -> Result<(), &'static str> {
    Ok(())
}

/// Software memory controller ops.
pub fn software_memctrl_ops() -> MemControllerOps {
    MemControllerOps {
        init: sw_init,
        read_ecc_errors: sw_ecc,
        get_temperature: sw_temp,
        set_scrub_rate: sw_scrub,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !MEM_CONTROLLERS.read().is_empty() {
        return Ok(());
    }

    let ops = software_memctrl_ops();
    let ctrl_id = register_controller(
        "sw-memctrl",
        MemControllerType::Ddr4,
        ops,
        2,
        true,
        16 * 1024,
    )?;

    register_block(0, 256)?;
    register_block(256 * 1024 * 1024, 256)?;

    crate::serial_println!(
        "memory: software DDR4 controller registered (ctrl_id={}, 2 DIMMs, ECC, 16GB max)",
        ctrl_id
    );
    Ok(())
}
