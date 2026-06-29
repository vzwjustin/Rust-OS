//! FFA (ARM Firmware Framework - A) subsystem
//!
//! Provides FF-A partition messaging for secure world communication on ARM.
//! Mirrors Linux's `drivers/firmware/arm_ffa/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// FFA partition (Linux `struct ffa_partition`).
pub struct FfaPartition {
    pub id: u16,
    pub uuid: [u32; 4],
    pub name: String,
    pub properties: u32,
    pub vcpu_count: u16,
    pub message_size: u32,
    pub info_regs: u32,
}

/// FFA device (Linux `struct ffa_dev`).
pub struct FfaDevice {
    pub id: u32,
    pub partition_id: u16,
    pub name: String,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// FFA driver (Linux `struct ffa_driver`).
pub struct FfaDriver {
    pub name: String,
    pub id_table: Vec<FfaDeviceId>,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// FFA device ID (Linux `struct ffa_device_id`).
#[derive(Debug, Clone)]
pub struct FfaDeviceId {
    pub partition_id: u16,
    pub uuid: [u32; 4],
}

/// FFA message (Linux `struct ffa_msg`).
#[derive(Debug, Clone)]
pub struct FfaMsg {
    pub src_partition: u16,
    pub dst_partition: u16,
    pub data: Vec<u8>,
}

/// FFA memory region (Linux `struct ffa_mem_region`).
pub struct FfaMemRegion {
    pub id: u32,
    pub handle: u64,
    pub size: u64,
    pub flags: u32,
    pub attrs: u32,
    pub sender: u16,
    pub receiver: u16,
    pub buffer: Vec<u8>,
}

/// FFA operations (Linux `struct ffa_ops`).
pub struct FfaOps {
    pub version_get: fn() -> Result<u32, &'static str>,
    pub partition_info_get: fn(uuid: &[u32; 4]) -> Result<Vec<FfaPartition>, &'static str>,
    pub msg_send: fn(src: u16, dst: u16, data: &[u8]) -> Result<(), &'static str>,
    pub msg_recv: fn(buf: &mut [u8]) -> Result<(u16, u16, usize), &'static str>,
    pub mem_share: fn(region: &FfaMemRegion) -> Result<u64, &'static str>,
    pub mem_reclaim: fn(handle: u64, flags: u32) -> Result<(), &'static str>,
    pub rxtx_map: fn(tx_buf: &[u8], rx_buf: &[u8]) -> Result<(), &'static str>,
    pub rxtx_unmap: fn() -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MEM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static FFA_PARTITIONS: RwLock<BTreeMap<u16, FfaPartition>> = RwLock::new(BTreeMap::new());
static FFA_DEVICES: RwLock<BTreeMap<u32, FfaDevice>> = RwLock::new(BTreeMap::new());
static FFA_DRIVERS: RwLock<BTreeMap<u32, FfaDriver>> = RwLock::new(BTreeMap::new());
static FFA_MEM_REGIONS: RwLock<BTreeMap<u32, FfaMemRegion>> = RwLock::new(BTreeMap::new());
static FFA_OPS: RwLock<Option<FfaOps>> = RwLock::new(None);

// ── Public API ──────────────────────────────────────────────────────────

/// Register FFA transport operations.
pub fn register_ops(ops: FfaOps) -> Result<(), &'static str> {
    *FFA_OPS.write() = Some(ops);
    Ok(())
}

/// Discover FFA partitions (Linux `ffa_partition_info_get`).
pub fn discover_partitions(uuid: &[u32; 4]) -> Result<Vec<u16>, &'static str> {
    let info_get_fn = {
        let ops = FFA_OPS.read();
        let ffa_ops = ops.as_ref().ok_or("FFA ops not registered")?;
        ffa_ops.partition_info_get
    };

    let partitions = (info_get_fn)(uuid)?;
    let mut ids = Vec::new();
    for p in partitions {
        let pid = p.id;
        FFA_PARTITIONS.write().insert(pid, p);
        ids.push(pid);
    }
    Ok(ids)
}

/// Register an FFA device for a partition.
pub fn register_device(partition_id: u16, name: &str) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = FfaDevice {
        id,
        partition_id,
        name: String::from(name),
        driver_name: None,
        bound: false,
    };
    FFA_DEVICES.write().insert(id, dev);
    try_match_driver(id)?;
    Ok(id)
}

/// Send a message to an FFA partition (Linux `ffa_msg_send`).
pub fn msg_send(dev_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let (partition_id, send_fn) = {
        let devices = FFA_DEVICES.read();
        let dev = devices.get(&dev_id).ok_or("FFA device not found")?;
        let ops = FFA_OPS.read();
        let ffa_ops = ops.as_ref().ok_or("FFA ops not registered")?;
        (dev.partition_id, ffa_ops.msg_send)
    };
    // Sender is always partition 0 (OS)
    (send_fn)(0, partition_id, data)
}

/// Receive a message from an FFA partition (Linux `ffa_msg_recv`).
pub fn msg_recv(buf: &mut [u8]) -> Result<(u16, u16, usize), &'static str> {
    let recv_fn = {
        let ops = FFA_OPS.read();
        let ffa_ops = ops.as_ref().ok_or("FFA ops not registered")?;
        ffa_ops.msg_recv
    };
    (recv_fn)(buf)
}

/// Share memory with an FFA partition (Linux `ffa_mem_share`).
pub fn mem_share(
    sender: u16,
    receiver: u16,
    buffer: Vec<u8>,
    flags: u32,
    attrs: u32,
) -> Result<(u32, u64), &'static str> {
    let share_fn = {
        let ops = FFA_OPS.read();
        let ffa_ops = ops.as_ref().ok_or("FFA ops not registered")?;
        ffa_ops.mem_share
    };

    let mem_id = MEM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let size = buffer.len() as u64;
    let region = FfaMemRegion {
        id: mem_id,
        handle: 0,
        size,
        flags,
        attrs,
        sender,
        receiver,
        buffer,
    };
    let handle = (share_fn)(&region)?;

    let mut stored = region;
    stored.handle = handle;
    FFA_MEM_REGIONS.write().insert(mem_id, stored);
    Ok((mem_id, handle))
}

/// Reclaim shared memory (Linux `ffa_mem_reclaim`).
pub fn mem_reclaim(mem_id: u32) -> Result<(), &'static str> {
    let (handle, flags) = {
        let regions = FFA_MEM_REGIONS.read();
        let region = regions.get(&mem_id).ok_or("FFA memory region not found")?;
        (region.handle, region.flags)
    };

    let reclaim_fn = {
        let ops = FFA_OPS.read();
        let ffa_ops = ops.as_ref().ok_or("FFA ops not registered")?;
        ffa_ops.mem_reclaim
    };
    (reclaim_fn)(handle, flags)?;

    FFA_MEM_REGIONS.write().remove(&mem_id);
    Ok(())
}

/// Register an FFA driver.
pub fn register_driver(driver: FfaDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    FFA_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = FFA_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.bound
                    && id_table.iter().any(|id| {
                        id.partition_id == d.partition_id
                            || FFA_PARTITIONS
                                .read()
                                .get(&d.partition_id)
                                .map(|p| p.uuid == id.uuid)
                                .unwrap_or(false)
                    })
            })
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        try_match_driver(dev_id)?;
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = FFA_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let pid = dev.partition_id;
        let uuid = {
            let parts = FFA_PARTITIONS.read();
            parts.get(&pid).map(|p| p.uuid).unwrap_or([0; 4])
        };

        let drivers = FFA_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.partition_id == pid || id.uuid == uuid {
                    found = Some((drv.probe, drv.name.clone()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };

    if let Some((probe_fn, drv_name)) = matched {
        (probe_fn)(device_id)?;
        let mut devices = FFA_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all FFA devices.
pub fn list_devices() -> Vec<(u32, u16, String, bool)> {
    FFA_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.partition_id, d.name.clone(), d.bound))
        .collect()
}

/// List discovered partitions.
pub fn list_partitions() -> Vec<(u16, String, u32)> {
    FFA_PARTITIONS
        .read()
        .iter()
        .map(|(pid, p)| (*pid, p.name.clone(), p.properties))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    FFA_DEVICES.read().len()
}

// ── Software FFA ────────────────────────────────────────────────────────

fn sw_version_get() -> Result<u32, &'static str> {
    Ok(0x0001_0000)
}
fn sw_partition_info_get(uuid: &[u32; 4]) -> Result<Vec<FfaPartition>, &'static str> {
    let mut parts = Vec::new();
    // Return a sample secure partition
    parts.push(FfaPartition {
        id: 0x8001,
        uuid: *uuid,
        name: String::from("sw-ffa-secure-os"),
        properties: 0,
        vcpu_count: 1,
        message_size: 4096,
        info_regs: 1,
    });
    Ok(parts)
}
fn sw_msg_send(_src: u16, _dst: u16, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_msg_recv(buf: &mut [u8]) -> Result<(u16, u16, usize), &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok((0x8001, 0, buf.len()))
}
fn sw_mem_share(region: &FfaMemRegion) -> Result<u64, &'static str> {
    Ok(region.id as u64 | 0x8000_0000_0000_0000)
}
fn sw_mem_reclaim(_handle: u64, _flags: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_rxtx_map(_tx: &[u8], _rx: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_rxtx_unmap() -> Result<(), &'static str> {
    Ok(())
}

/// Software FFA ops.
pub fn software_ffa_ops() -> FfaOps {
    FfaOps {
        version_get: sw_version_get,
        partition_info_get: sw_partition_info_get,
        msg_send: sw_msg_send,
        msg_recv: sw_msg_recv,
        mem_share: sw_mem_share,
        mem_reclaim: sw_mem_reclaim,
        rxtx_map: sw_rxtx_map,
        rxtx_unmap: sw_rxtx_unmap,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    let ops = software_ffa_ops();
    register_ops(ops)?;

    // Discover partitions
    let uuid = [0x12345678, 0x9ABCDEF0, 0x11112222, 0x33334444];
    let partition_ids = discover_partitions(&uuid)?;

    // Register devices for discovered partitions
    for &pid in &partition_ids {
        register_device(pid, "sw-ffa-dev")?;
    }

    // Register a driver
    let mut id_table = Vec::new();
    id_table.push(FfaDeviceId {
        partition_id: 0x8001,
        uuid,
    });
    let driver = FfaDriver {
        name: String::from("sw-ffa-drv"),
        id_table,
        probe: null_probe,
        remove: null_remove,
    };
    register_driver(driver)?;

    // Share memory with the secure partition
    let buf = alloc::vec![0u8; 4096];
    let (mem_id, _handle) = mem_share(0, 0x8001, buf, 0, 0)?;
    mem_reclaim(mem_id)?;

    Ok(())
}
