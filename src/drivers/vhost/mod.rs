//! vhost subsystem
//!
//! Provides vhost virtio backend for in-kernel virtio device emulation.
//! Mirrors Linux's `drivers/vhost/vhost.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// vhost device (Linux `struct vhost_dev`).
pub struct VhostDev {
    pub id: u32,
    pub name: String,
    pub dev_type: VhostDevType,
    pub vqs: Vec<VhostVirtqueue>,
    pub nvqs: u32,
    pub features: u64,
    pub acked_features: u64,
    pub state: VhostState,
    pub memory: Option<VhostMemory>,
}

/// vhost device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VhostDevType {
    Net,
    Scsi,
    Blk,
    Crypto,
    Vsock,
    Iotlb,
}

/// vhost state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VhostState {
    Idle,
    Running,
    Stopped,
}

/// vhost virtqueue (Linux `struct vhost_virtqueue`).
pub struct VhostVirtqueue {
    pub index: u32,
    pub num: u16,
    pub enabled: bool,
    pub desc_addr: u64,
    pub avail_addr: u64,
    pub used_addr: u64,
    pub log_addr: u64,
    pub last_avail_idx: u16,
    pub last_used_idx: u16,
    pub signalled_used: u16,
    pub call_fd: Option<u32>,
    pub kick_fd: Option<u32>,
    pub err_fd: Option<u32>,
}

/// vhost memory mapping (Linux `struct vhost_memory`).
pub struct VhostMemory {
    pub nregions: u32,
    pub regions: Vec<VhostMemRegion>,
}

/// vhost memory region (Linux `struct vhost_memory_region`).
#[derive(Debug, Clone)]
pub struct VhostMemRegion {
    pub guest_phys_addr: u64,
    pub memory_size: u64,
    pub userspace_addr: u64,
    pub flags_padding: u64,
}

/// vhost operations.
pub struct VhostOps {
    pub start: fn(dev_id: u32) -> Result<(), &'static str>,
    pub stop: fn(dev_id: u32) -> Result<(), &'static str>,
    pub set_features: fn(dev_id: u32, features: u64) -> Result<(), &'static str>,
    pub set_mem_table: fn(dev_id: u32, mem: &VhostMemory) -> Result<(), &'static str>,
    pub set_vring: fn(dev_id: u32, vq_index: u32, vq: &VhostVirtqueue) -> Result<(), &'static str>,
    pub handle_kick: fn(dev_id: u32, vq_index: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static VHOST_DEVS: RwLock<BTreeMap<u32, VhostDev>> = RwLock::new(BTreeMap::new());
static VHOST_OPS: RwLock<BTreeMap<u32, VhostOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a vhost device.
pub fn register_device(
    name: &str,
    dev_type: VhostDevType,
    nvqs: u32,
    ops: VhostOps,
) -> Result<u32, &'static str> {
    if name.trim().is_empty() {
        return Err("vhost device name required");
    }
    if nvqs == 0 || nvqs > 1024 {
        return Err("invalid vhost virtqueue count");
    }

    let mut devs = VHOST_DEVS.write();
    if devs.values().any(|dev| dev.name == name) {
        return Err("vhost device already registered");
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut vqs = Vec::new();
    for i in 0..nvqs {
        vqs.push(VhostVirtqueue {
            index: i,
            num: 256,
            enabled: false,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            log_addr: 0,
            last_avail_idx: 0,
            last_used_idx: 0,
            signalled_used: 0,
            call_fd: None,
            kick_fd: None,
            err_fd: None,
        });
    }

    let dev = VhostDev {
        id,
        name: String::from(name),
        dev_type,
        vqs,
        nvqs,
        features: 0,
        acked_features: 0,
        state: VhostState::Idle,
        memory: None,
    };
    devs.insert(id, dev);
    VHOST_OPS.write().insert(id, ops);
    Ok(id)
}

/// Set device features (Linux `VHOST_SET_FEATURES`).
pub fn set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let set_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        if dev.state == VhostState::Running {
            return Err("cannot change vhost features while running");
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.set_features
    };
    (set_fn)(dev_id, features)?;

    let mut devs = VHOST_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.features = features;
        dev.acked_features = features;
    }
    Ok(())
}

/// Set memory table (Linux `VHOST_SET_MEM_TABLE`).
pub fn set_mem_table(dev_id: u32, mem: VhostMemory) -> Result<(), &'static str> {
    validate_mem_table(&mem)?;

    let set_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        if dev.state == VhostState::Running {
            return Err("cannot change vhost memory while running");
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.set_mem_table
    };
    (set_fn)(dev_id, &mem)?;

    let mut devs = VHOST_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.memory = Some(mem);
    }
    Ok(())
}

/// Set virtqueue configuration (Linux `VHOST_SET_VRING_*`).
pub fn set_vring(dev_id: u32, vq_index: u32, vq: VhostVirtqueue) -> Result<(), &'static str> {
    let set_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        validate_vring(dev, vq_index, &vq)?;
        if dev.state == VhostState::Running {
            return Err("cannot change vhost vring while running");
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.set_vring
    };
    (set_fn)(dev_id, vq_index, &vq)?;

    let mut devs = VHOST_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        if let Some(existing) = dev.vqs.get_mut(vq_index as usize) {
            *existing = vq;
        }
    }
    Ok(())
}

/// Enable a virtqueue (Linux `VHOST_SET_VRING_ENABLE`).
pub fn enable_vring(dev_id: u32, vq_index: u32, enabled: bool) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    if vq_index >= dev.nvqs {
        return Err("Virtqueue index out of range");
    }
    let vq = dev
        .vqs
        .get_mut(vq_index as usize)
        .ok_or("Virtqueue index out of range")?;
    if enabled {
        validate_vring_ready(vq)?;
    }
    vq.enabled = enabled;
    Ok(())
}

/// Start vhost device (Linux `vhost_dev_start`).
pub fn start_device(dev_id: u32) -> Result<(), &'static str> {
    let start_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        if dev.state == VhostState::Running {
            return Ok(());
        }
        if dev.memory.is_none() {
            return Err("vhost memory table not configured");
        }
        if dev.vqs.iter().any(|vq| !vq.enabled) {
            return Err("vhost virtqueue not enabled");
        }
        for vq in &dev.vqs {
            validate_vring_ready(vq)?;
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.start
    };
    (start_fn)(dev_id)?;

    let mut devs = VHOST_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = VhostState::Running;
    }
    Ok(())
}

/// Stop vhost device (Linux `vhost_dev_stop`).
pub fn stop_device(dev_id: u32) -> Result<(), &'static str> {
    let stop_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        if dev.state != VhostState::Running {
            return Ok(());
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.stop
    };
    (stop_fn)(dev_id)?;

    let mut devs = VHOST_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = VhostState::Stopped;
    }
    Ok(())
}

/// Handle a virtqueue kick (Linux `vhost_vring_handle_kick`).
pub fn handle_kick(dev_id: u32, vq_index: u32) -> Result<(), &'static str> {
    let kick_fn = {
        let devs = VHOST_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
        if dev.state != VhostState::Running {
            return Err("vhost device not running");
        }
        let vq = dev
            .vqs
            .get(vq_index as usize)
            .ok_or("Virtqueue index out of range")?;
        if !vq.enabled {
            return Err("vhost virtqueue not enabled");
        }

        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.handle_kick
    };
    (kick_fn)(dev_id, vq_index)
}

fn validate_mem_table(mem: &VhostMemory) -> Result<(), &'static str> {
    if mem.nregions as usize != mem.regions.len() {
        return Err("vhost memory region count mismatch");
    }
    if mem.regions.is_empty() {
        return Err("vhost memory table is empty");
    }

    for (idx, region) in mem.regions.iter().enumerate() {
        if region.memory_size == 0 {
            return Err("vhost memory region has zero size");
        }
        let guest_end = region
            .guest_phys_addr
            .checked_add(region.memory_size)
            .ok_or("vhost guest memory range overflow")?;
        region
            .userspace_addr
            .checked_add(region.memory_size)
            .ok_or("vhost userspace memory range overflow")?;

        for other in mem.regions.iter().skip(idx + 1) {
            let other_end = other
                .guest_phys_addr
                .checked_add(other.memory_size)
                .ok_or("vhost guest memory range overflow")?;
            if region.guest_phys_addr < other_end && other.guest_phys_addr < guest_end {
                return Err("vhost memory regions overlap");
            }
        }
    }

    Ok(())
}

fn validate_vring(dev: &VhostDev, vq_index: u32, vq: &VhostVirtqueue) -> Result<(), &'static str> {
    if vq_index >= dev.nvqs {
        return Err("Virtqueue index out of range");
    }
    if vq.index != vq_index {
        return Err("vhost virtqueue index mismatch");
    }
    if vq.num == 0 || !vq.num.is_power_of_two() || vq.num > 32768 {
        return Err("invalid vhost virtqueue size");
    }
    if vq.enabled {
        validate_vring_ready(vq)?;
    }
    Ok(())
}

fn validate_vring_ready(vq: &VhostVirtqueue) -> Result<(), &'static str> {
    if vq.num == 0 || !vq.num.is_power_of_two() || vq.num > 32768 {
        return Err("invalid vhost virtqueue size");
    }
    if vq.desc_addr == 0 || vq.avail_addr == 0 || vq.used_addr == 0 {
        return Err("vhost virtqueue ring address missing");
    }

    let num = vq.num as u64;
    let desc_len = num.checked_mul(16).ok_or("vhost desc ring overflow")?;
    let avail_len = 4u64
        .checked_add(num.checked_mul(2).ok_or("vhost avail ring overflow")?)
        .and_then(|len| len.checked_add(2))
        .ok_or("vhost avail ring overflow")?;
    let used_len = 4u64
        .checked_add(num.checked_mul(8).ok_or("vhost used ring overflow")?)
        .and_then(|len| len.checked_add(2))
        .ok_or("vhost used ring overflow")?;

    vq.desc_addr
        .checked_add(desc_len)
        .ok_or("vhost desc ring overflow")?;
    vq.avail_addr
        .checked_add(avail_len)
        .ok_or("vhost avail ring overflow")?;
    vq.used_addr
        .checked_add(used_len)
        .ok_or("vhost used ring overflow")?;

    Ok(())
}

/// Get device state.
pub fn get_state(dev_id: u32) -> Result<VhostState, &'static str> {
    let devs = VHOST_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("vhost device not found")?;
    Ok(dev.state)
}

/// List all vhost devices.
pub fn list_devices() -> Vec<(u32, String, VhostDevType, VhostState, u32)> {
    VHOST_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.state, d.nvqs))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    VHOST_DEVS.read().len()
}

// ── Software vhost ──────────────────────────────────────────────────────

fn sw_start(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_stop(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_features(_dev_id: u32, _features: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_mem_table(_dev_id: u32, _mem: &VhostMemory) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_vring(_dev_id: u32, _vq_index: u32, _vq: &VhostVirtqueue) -> Result<(), &'static str> {
    Ok(())
}
fn sw_handle_kick(_dev_id: u32, _vq_index: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software vhost ops.
pub fn software_vhost_ops() -> VhostOps {
    VhostOps {
        start: sw_start,
        stop: sw_stop,
        set_features: sw_set_features,
        set_mem_table: sw_set_mem_table,
        set_vring: sw_set_vring,
        handle_kick: sw_handle_kick,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !VHOST_DEVS.read().is_empty() {
        return Ok(());
    }

    let ops = software_vhost_ops();
    let dev_id = register_device("sw-vhost-net", VhostDevType::Net, 2, ops)?;
    crate::serial_println!(
        "vhost: software vhost-net registered (id={}, 2 vqs)",
        dev_id
    );
    Ok(())
}
