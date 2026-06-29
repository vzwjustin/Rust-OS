//! UFS (Universal Flash Storage) subsystem
//!
//! Provides UFS host controller and UFS device management framework.
//! Mirrors Linux's `drivers/scsi/ufs/ufshcd.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// UFS device descriptor (subset of UFS 3.0 spec).
#[derive(Debug, Clone)]
pub struct UfsDevDesc {
    pub device_type: u8,
    pub wmanufacturer_id: u16,
    pub serial_number: String,
    pub oem_id: [u8; 2],
    pub model_name: String,
    pub lu_count: u8,
    pub max_lu: u8,
    pub boot_lu_count: u8,
    pub rpmb_lu: u8,
    pub total_capacity: u64,
}

/// UFS logical unit (LU).
#[derive(Debug, Clone)]
pub struct UfsLu {
    pub index: u8,
    pub lu_type: UfsLuType,
    pub capacity: u64,
    pub block_size: u32,
    pub boot_lu: bool,
    pub rpmb: bool,
    pub active: bool,
}

/// UFS LU type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UfsLuType {
    General,
    Wlun,
    Boot,
    Rpmb,
}

/// UFS host controller operations (Linux `struct ufshcd_ops`).
pub struct UfsHcOps {
    pub init: fn(host_id: u32) -> Result<(), &'static str>,
    pub shutdown: fn(host_id: u32) -> Result<(), &'static str>,
    pub enable: fn(host_id: u32) -> Result<(), &'static str>,
    pub disable: fn(host_id: u32) -> Result<(), &'static str>,
    pub read_descriptor:
        fn(host_id: u32, desc_id: u8, index: u8, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write_descriptor:
        fn(host_id: u32, desc_id: u8, index: u8, data: &[u8]) -> Result<usize, &'static str>,
    pub query_attr:
        fn(host_id: u32, attr_id: u8, read: bool, val: &mut u32) -> Result<(), &'static str>,
    pub read: fn(host_id: u32, lun: u8, lba: u64, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write: fn(host_id: u32, lun: u8, lba: u64, data: &[u8]) -> Result<usize, &'static str>,
}

/// UFS host controller (Linux `struct ufs_hba`).
pub struct UfsHost {
    pub id: u32,
    pub name: String,
    pub ops: UfsHcOps,
    pub dev_desc: Option<UfsDevDesc>,
    pub lus: Vec<UfsLu>,
    pub active: bool,
    pub link_state: UfsLinkState,
    pub power_mode: UfsPowerMode,
}

/// UFS link state (Linux `enum uic_link_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UfsLinkState {
    Off,
    Active,
    Hibern8,
    Sleep,
}

/// UFS power mode (Linux `enum ufs_pm_level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UfsPowerMode {
    Off,
    Active,
    Sleep,
    Hibernate,
}

// ── Registry ────────────────────────────────────────────────────────────

static HOST_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static UFS_HOSTS: RwLock<BTreeMap<u32, UfsHost>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a UFS host controller.
pub fn register_host(name: &str, ops: UfsHcOps) -> Result<u32, &'static str> {
    let id = HOST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let host = UfsHost {
        id,
        name: String::from(name),
        ops,
        dev_desc: None,
        lus: Vec::new(),
        active: false,
        link_state: UfsLinkState::Off,
        power_mode: UfsPowerMode::Off,
    };
    UFS_HOSTS.write().insert(id, host);
    Ok(id)
}

/// Initialize a UFS host controller (Linux `ufshcd_init`).
pub fn init_host(host_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        host.ops.init
    };
    (init_fn)(host_id)?;

    // Read device descriptor
    let mut desc_buf = [0u8; 64];
    let read_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        host.ops.read_descriptor
    };
    let _ = (read_fn)(host_id, 0, 0, &mut desc_buf);

    let mut hosts = UFS_HOSTS.write();
    if let Some(host) = hosts.get_mut(&host_id) {
        host.active = true;
        host.link_state = UfsLinkState::Active;
        host.power_mode = UfsPowerMode::Active;
        host.dev_desc = Some(UfsDevDesc {
            device_type: 0,
            wmanufacturer_id: 0,
            serial_number: String::from("sw-ufs"),
            oem_id: [0, 0],
            model_name: String::from("sw-ufs-128g"),
            lu_count: 1,
            max_lu: 8,
            boot_lu_count: 0,
            rpmb_lu: 0,
            total_capacity: 128 * 1024 * 1024 * 1024,
        });

        // Create a general-purpose LU
        host.lus.push(UfsLu {
            index: 0,
            lu_type: UfsLuType::General,
            capacity: 128 * 1024 * 1024 * 1024,
            block_size: 4096,
            boot_lu: false,
            rpmb: false,
            active: true,
        });
    }
    Ok(())
}

/// Shutdown a UFS host controller.
pub fn shutdown_host(host_id: u32) -> Result<(), &'static str> {
    let shutdown_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        host.ops.shutdown
    };
    (shutdown_fn)(host_id)?;

    let mut hosts = UFS_HOSTS.write();
    if let Some(host) = hosts.get_mut(&host_id) {
        host.active = false;
        host.link_state = UfsLinkState::Off;
        host.power_mode = UfsPowerMode::Off;
    }
    Ok(())
}

/// Read from a UFS logical unit.
pub fn read(host_id: u32, lun: u8, lba: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        if !host.active {
            return Err("UFS host not active");
        }
        host.ops.read
    };
    (read_fn)(host_id, lun, lba, buf)
}

/// Write to a UFS logical unit.
pub fn write(host_id: u32, lun: u8, lba: u64, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        if !host.active {
            return Err("UFS host not active");
        }
        host.ops.write
    };
    (write_fn)(host_id, lun, lba, data)
}

/// Query a UFS attribute.
pub fn query_attr(host_id: u32, attr_id: u8, val: &mut u32) -> Result<(), &'static str> {
    let query_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        host.ops.query_attr
    };
    (query_fn)(host_id, attr_id, true, val)
}

/// Set a UFS attribute.
pub fn set_attr(host_id: u32, attr_id: u8, val: u32) -> Result<(), &'static str> {
    let query_fn = {
        let hosts = UFS_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("UFS host not found")?;
        host.ops.query_attr
    };
    let mut v = val;
    (query_fn)(host_id, attr_id, false, &mut v)
}

/// Get device descriptor.
pub fn get_dev_desc(host_id: u32) -> Result<UfsDevDesc, &'static str> {
    let hosts = UFS_HOSTS.read();
    let host = hosts.get(&host_id).ok_or("UFS host not found")?;
    host.dev_desc
        .clone()
        .ok_or("Device descriptor not available")
}

/// List logical units.
pub fn list_lus(host_id: u32) -> Result<Vec<(u8, UfsLuType, u64, bool)>, &'static str> {
    let hosts = UFS_HOSTS.read();
    let host = hosts.get(&host_id).ok_or("UFS host not found")?;
    Ok(host
        .lus
        .iter()
        .map(|lu| (lu.index, lu.lu_type, lu.capacity, lu.active))
        .collect())
}

/// List all UFS hosts.
pub fn list_hosts() -> Vec<(u32, String, bool)> {
    UFS_HOSTS
        .read()
        .iter()
        .map(|(id, h)| (*id, h.name.clone(), h.active))
        .collect()
}

/// Count registered hosts.
pub fn host_count() -> usize {
    UFS_HOSTS.read().len()
}

// ── Software UFS ────────────────────────────────────────────────────────

fn sw_init(_host_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_shutdown(_host_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable(_host_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_host_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_desc(
    _host_id: u32,
    _desc_id: u8,
    _index: u8,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_write_desc(
    _host_id: u32,
    _desc_id: u8,
    _index: u8,
    data: &[u8],
) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_query_attr(
    _host_id: u32,
    _attr_id: u8,
    _read: bool,
    _val: &mut u32,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read(_host_id: u32, _lun: u8, _lba: u64, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_write(_host_id: u32, _lun: u8, _lba: u64, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}

/// Software UFS host ops.
pub fn software_ufs_ops() -> UfsHcOps {
    UfsHcOps {
        init: sw_init,
        shutdown: sw_shutdown,
        enable: sw_enable,
        disable: sw_disable,
        read_descriptor: sw_read_desc,
        write_descriptor: sw_write_desc,
        query_attr: sw_query_attr,
        read: sw_read,
        write: sw_write,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ufs: subsystem ready");
    Ok(())
}
