//! FPGA (Field Programmable Gate Array) manager subsystem
//!
//! Provides FPGA manager framework for programming and managing FPGA images.
//! Mirrors Linux's `drivers/fpga/fpga-mgr.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// FPGA manager state (Linux `enum fpga_mgr_states`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpgaMgrState {
    Unknown,
    PowerOff,
    PowerUp,
    Reset,
    FirmwareLoad,
    FirmwareLoadErr,
    Operating,
}

/// FPGA image info (Linux `struct fpga_image_info`).
#[derive(Debug, Clone)]
pub struct FpgaImageInfo {
    pub image_type: FpgaImageType,
    pub flags: u32,
    pub config_complete_timeout: u32,
    pub data: Vec<u8>,
    pub region_id: Option<u32>,
}

/// FPGA image type (Linux `enum fpga_mgr_image_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpgaImageType {
    Bitstream,
    Firmware,
    Raw,
}

/// FPGA manager operations (Linux `struct fpga_manager_ops`).
pub struct FpgaMgrOps {
    pub initial_header_size: usize,
    pub state: fn(mgr_id: u32) -> Result<FpgaMgrState, &'static str>,
    pub status: fn(mgr_id: u32) -> Result<u64, &'static str>,
    pub write_init: fn(mgr_id: u32, info: &FpgaImageInfo, buf: &[u8]) -> Result<(), &'static str>,
    pub write: fn(mgr_id: u32, buf: &[u8]) -> Result<(), &'static str>,
    pub write_complete: fn(mgr_id: u32) -> Result<(), &'static str>,
    pub fpga_remove: fn(mgr_id: u32) -> Result<(), &'static str>,
    pub apply_bitstream: fn(mgr_id: u32, buf: &[u8]) -> Result<(), &'static str>,
}

/// FPGA manager (Linux `struct fpga_manager`).
pub struct FpgaManager {
    pub id: u32,
    pub name: String,
    pub ops: FpgaMgrOps,
    pub state: FpgaMgrState,
    pub compatible: String,
}

/// FPGA region (Linux `struct fpga_region`).
pub struct FpgaRegion {
    pub id: u32,
    pub name: String,
    pub mgr_id: u32,
    pub bridge_ids: Vec<u32>,
    pub info: Option<FpgaImageInfo>,
    pub active: bool,
}

/// FPGA bridge (Linux `struct fpga_bridge`).
pub struct FpgaBridge {
    pub id: u32,
    pub name: String,
    pub ops: FpgaBridgeOps,
    pub enable: bool,
}

/// FPGA bridge operations (Linux `struct fpga_bridge_ops`).
pub struct FpgaBridgeOps {
    pub enable_set: fn(bridge_id: u32, enable: bool) -> Result<(), &'static str>,
    pub get_state: fn(bridge_id: u32) -> Result<bool, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static MGR_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static REGION_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static BRIDGE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static FPGA_MGRS: RwLock<BTreeMap<u32, FpgaManager>> = RwLock::new(BTreeMap::new());
static FPGA_REGIONS: RwLock<BTreeMap<u32, FpgaRegion>> = RwLock::new(BTreeMap::new());
static FPGA_BRIDGES: RwLock<BTreeMap<u32, FpgaBridge>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an FPGA manager.
pub fn register_manager(
    name: &str,
    ops: FpgaMgrOps,
    compatible: &str,
) -> Result<u32, &'static str> {
    let id = MGR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mgr = FpgaManager {
        id,
        name: String::from(name),
        ops,
        state: FpgaMgrState::Unknown,
        compatible: String::from(compatible),
    };
    FPGA_MGRS.write().insert(id, mgr);
    Ok(id)
}

/// Register an FPGA bridge.
pub fn register_bridge(name: &str, ops: FpgaBridgeOps) -> Result<u32, &'static str> {
    let id = BRIDGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bridge = FpgaBridge {
        id,
        name: String::from(name),
        ops,
        enable: true,
    };
    FPGA_BRIDGES.write().insert(id, bridge);
    Ok(id)
}

/// Register an FPGA region.
pub fn register_region(name: &str, mgr_id: u32, bridge_ids: Vec<u32>) -> Result<u32, &'static str> {
    let id = REGION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let region = FpgaRegion {
        id,
        name: String::from(name),
        mgr_id,
        bridge_ids,
        info: None,
        active: false,
    };
    FPGA_REGIONS.write().insert(id, region);
    Ok(id)
}

/// Load an FPGA image (Linux `fpga_mgr_load`).
pub fn load_image(mgr_id: u32, info: &FpgaImageInfo) -> Result<(), &'static str> {
    let (write_init_fn, header_size) = {
        let mgrs = FPGA_MGRS.read();
        let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
        (mgr.ops.write_init, mgr.ops.initial_header_size)
    };

    // Set state to firmware load
    {
        let mut mgrs = FPGA_MGRS.write();
        if let Some(mgr) = mgrs.get_mut(&mgr_id) {
            mgr.state = FpgaMgrState::FirmwareLoad;
        }
    }

    // Write initial header
    let header_end = core::cmp::min(header_size, info.data.len());
    (write_init_fn)(mgr_id, info, &info.data[..header_end])?;

    // Write remaining data
    let write_fn = {
        let mgrs = FPGA_MGRS.read();
        let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
        mgr.ops.write
    };
    if info.data.len() > header_end {
        (write_fn)(mgr_id, &info.data[header_end..])?;
    }

    // Complete write
    let complete_fn = {
        let mgrs = FPGA_MGRS.read();
        let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
        mgr.ops.write_complete
    };
    (complete_fn)(mgr_id)?;

    // Set state to operating
    let mut mgrs = FPGA_MGRS.write();
    if let Some(mgr) = mgrs.get_mut(&mgr_id) {
        mgr.state = FpgaMgrState::Operating;
    }
    Ok(())
}

/// Program an FPGA region (Linux `fpga_region_program_fpga`).
pub fn program_region(region_id: u32, info: FpgaImageInfo) -> Result<(), &'static str> {
    let (mgr_id, bridge_ids) = {
        let regions = FPGA_REGIONS.read();
        let region = regions.get(&region_id).ok_or("FPGA region not found")?;
        (region.mgr_id, region.bridge_ids.clone())
    };

    // Disable bridges before programming
    for &bid in &bridge_ids {
        let enable_fn = {
            let bridges = FPGA_BRIDGES.read();
            let bridge = bridges.get(&bid).ok_or("FPGA bridge not found")?;
            bridge.ops.enable_set
        };
        (enable_fn)(bid, false)?;
    }

    // Load the image
    load_image(mgr_id, &info)?;

    // Re-enable bridges after programming
    for &bid in &bridge_ids {
        let enable_fn = {
            let bridges = FPGA_BRIDGES.read();
            let bridge = bridges.get(&bid).ok_or("FPGA bridge not found")?;
            bridge.ops.enable_set
        };
        (enable_fn)(bid, true)?;
    }

    let mut regions = FPGA_REGIONS.write();
    if let Some(region) = regions.get_mut(&region_id) {
        region.info = Some(info);
        region.active = true;
    }
    Ok(())
}

/// Get FPGA manager state.
pub fn get_mgr_state(mgr_id: u32) -> Result<FpgaMgrState, &'static str> {
    let mgrs = FPGA_MGRS.read();
    let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
    Ok(mgr.state)
}

/// Get FPGA manager status.
pub fn get_mgr_status(mgr_id: u32) -> Result<u64, &'static str> {
    let status_fn = {
        let mgrs = FPGA_MGRS.read();
        let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
        mgr.ops.status
    };
    (status_fn)(mgr_id)
}

/// Enable/disable a bridge.
pub fn set_bridge_enable(bridge_id: u32, enable: bool) -> Result<(), &'static str> {
    let enable_fn = {
        let bridges = FPGA_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("FPGA bridge not found")?;
        bridge.ops.enable_set
    };
    (enable_fn)(bridge_id, enable)?;

    let mut bridges = FPGA_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        bridge.enable = enable;
    }
    Ok(())
}

/// List all FPGA managers.
pub fn list_managers() -> Vec<(u32, String, FpgaMgrState)> {
    FPGA_MGRS
        .read()
        .iter()
        .map(|(id, m)| (*id, m.name.clone(), m.state))
        .collect()
}

/// List all FPGA regions.
pub fn list_regions() -> Vec<(u32, String, u32, bool)> {
    FPGA_REGIONS
        .read()
        .iter()
        .map(|(id, r)| (*id, r.name.clone(), r.mgr_id, r.active))
        .collect()
}

/// Count registered managers.
pub fn manager_count() -> usize {
    FPGA_MGRS.read().len()
}

// ── Software FPGA ───────────────────────────────────────────────────────

fn sw_state(mgr_id: u32) -> Result<FpgaMgrState, &'static str> {
    let mgrs = FPGA_MGRS.read();
    let mgr = mgrs.get(&mgr_id).ok_or("FPGA manager not found")?;
    Ok(mgr.state)
}
fn sw_status(_mgr_id: u32) -> Result<u64, &'static str> {
    Ok(0)
}
fn sw_write_init(_mgr_id: u32, _info: &FpgaImageInfo, _buf: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write(_mgr_id: u32, _buf: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write_complete(_mgr_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_fpga_remove(_mgr_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_apply_bitstream(_mgr_id: u32, _buf: &[u8]) -> Result<(), &'static str> {
    Ok(())
}

/// Software FPGA manager ops.
pub fn software_fpga_mgr_ops() -> FpgaMgrOps {
    FpgaMgrOps {
        initial_header_size: 0,
        state: sw_state,
        status: sw_status,
        write_init: sw_write_init,
        write: sw_write,
        write_complete: sw_write_complete,
        fpga_remove: sw_fpga_remove,
        apply_bitstream: sw_apply_bitstream,
    }
}

fn sw_bridge_enable_set(_bridge_id: u32, _enable: bool) -> Result<(), &'static str> {
    Ok(())
}
fn sw_bridge_get_state(bridge_id: u32) -> Result<bool, &'static str> {
    let bridges = FPGA_BRIDGES.read();
    let bridge = bridges.get(&bridge_id).ok_or("FPGA bridge not found")?;
    Ok(bridge.enable)
}

/// Software FPGA bridge ops.
pub fn software_fpga_bridge_ops() -> FpgaBridgeOps {
    FpgaBridgeOps {
        enable_set: sw_bridge_enable_set,
        get_state: sw_bridge_get_state,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("fpga: subsystem ready");
    Ok(())
}
