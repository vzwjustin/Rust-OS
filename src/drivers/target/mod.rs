//! Target (SCSI target) driver subsystem
//!
//! Provides SCSI target framework for exposing block devices as SCSI LUNs.
//! Mirrors Linux's `drivers/target/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Target fabric type (Linux `enum target_fabric_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FabricType {
    Loopback,
    Iscsi,
    FcoE,
    Iser,
    Qlax,
    Tcmu,
    Virtual,
}

/// Target LUN type (Linux `enum transport_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LunType {
    Fileio,
    Blockio,
    Iblock,
    Pscsi,
    Ramdisk,
}

/// Target portal group (Linux `struct tpg_node`).
pub struct TargetPortalGroup {
    pub id: u32,
    pub name: String,
    pub fabric: FabricType,
    lun_ids: Vec<u32>,
    pub enabled: bool,
}

/// Target LUN (Linux `struct se_lun`).
pub struct TargetLun {
    pub id: u32,
    pub tpg_id: u32,
    pub lun_number: u32,
    pub lun_type: LunType,
    pub backing_device: String,
    pub size_bytes: u64,
    pub read_only: bool,
}

/// Target command (Linux `struct se_cmd`).
#[derive(Debug, Clone, Copy)]
pub struct TargetCommand {
    pub lun_id: u32,
    pub cdb: [u8; 16],
    pub data_length: u32,
    pub read: bool,
    pub write: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static TPG_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static LUN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static TARGET_TPGS: RwLock<BTreeMap<u32, TargetPortalGroup>> = RwLock::new(BTreeMap::new());
static TARGET_LUNS: RwLock<BTreeMap<u32, TargetLun>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Create a target portal group (Linux `core_tpg_register`).
pub fn create_tpg(name: &str, fabric: FabricType) -> Result<u32, &'static str> {
    let id = TPG_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tpg = TargetPortalGroup {
        id,
        name: String::from(name),
        fabric,
        lun_ids: Vec::new(),
        enabled: false,
    };
    TARGET_TPGS.write().insert(id, tpg);
    Ok(id)
}

/// Enable a target portal group.
pub fn enable_tpg(tpg_id: u32) -> Result<(), &'static str> {
    let mut tpgs = TARGET_TPGS.write();
    let tpg = tpgs.get_mut(&tpg_id).ok_or("TPG not found")?;
    tpg.enabled = true;
    Ok(())
}

/// Disable a target portal group.
pub fn disable_tpg(tpg_id: u32) -> Result<(), &'static str> {
    let mut tpgs = TARGET_TPGS.write();
    let tpg = tpgs.get_mut(&tpg_id).ok_or("TPG not found")?;
    tpg.enabled = false;
    Ok(())
}

/// Create a LUN on a target portal group (Linux `core_tpg_add_lun`).
pub fn create_lun(
    tpg_id: u32,
    lun_number: u32,
    lun_type: LunType,
    backing_device: &str,
    size_bytes: u64,
    read_only: bool,
) -> Result<u32, &'static str> {
    if !TARGET_TPGS.read().contains_key(&tpg_id) {
        return Err("TPG not found");
    }
    let id = LUN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let lun = TargetLun {
        id,
        tpg_id,
        lun_number,
        lun_type,
        backing_device: String::from(backing_device),
        size_bytes,
        read_only,
    };
    TARGET_LUNS.write().insert(id, lun);
    let mut tpgs = TARGET_TPGS.write();
    if let Some(tpg) = tpgs.get_mut(&tpg_id) {
        tpg.lun_ids.push(id);
    }
    Ok(id)
}

/// Remove a LUN (Linux `core_tpg_remove_lun`).
pub fn remove_lun(lun_id: u32) -> Result<(), &'static str> {
    let tpg_id = {
        let luns = TARGET_LUNS.read();
        let lun = luns.get(&lun_id).ok_or("LUN not found")?;
        lun.tpg_id
    };
    TARGET_LUNS.write().remove(&lun_id);
    let mut tpgs = TARGET_TPGS.write();
    if let Some(tpg) = tpgs.get_mut(&tpg_id) {
        tpg.lun_ids.retain(|&id| id != lun_id);
    }
    Ok(())
}

/// List all TPGs.
pub fn list_tpgs() -> Vec<(u32, String, FabricType, bool, usize)> {
    TARGET_TPGS
        .read()
        .iter()
        .map(|(id, t)| (*id, t.name.clone(), t.fabric, t.enabled, t.lun_ids.len()))
        .collect()
}

/// List all LUNs.
pub fn list_luns() -> Vec<(u32, u32, u32, LunType, String, u64)> {
    TARGET_LUNS
        .read()
        .iter()
        .map(|(id, l)| {
            (
                *id,
                l.tpg_id,
                l.lun_number,
                l.lun_type,
                l.backing_device.clone(),
                l.size_bytes,
            )
        })
        .collect()
}

/// Count TPGs.
pub fn tpg_count() -> usize {
    TARGET_TPGS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !TARGET_TPGS.read().is_empty() {
        return Ok(());
    }

    let tpg_id = create_tpg("sw-tpg", FabricType::Loopback)?;
    enable_tpg(tpg_id)?;
    create_lun(
        tpg_id,
        0,
        LunType::Ramdisk,
        "sw-ramdisk0",
        64 * 1024 * 1024,
        false,
    )?;

    crate::serial_println!(
        "target: software loopback TPG registered (tpg_id={}, 1 LUN, 64MB ramdisk)",
        tpg_id
    );
    Ok(())
}
