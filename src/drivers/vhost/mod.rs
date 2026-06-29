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
    VHOST_DEVS.write().insert(id, dev);
    VHOST_OPS.write().insert(id, ops);
    Ok(id)
}

/// Set device features (Linux `VHOST_SET_FEATURES`).
pub fn set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let set_fn = {
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
    let set_fn = {
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
    let vq = dev
        .vqs
        .get_mut(vq_index as usize)
        .ok_or("Virtqueue index out of range")?;
    vq.enabled = enabled;
    Ok(())
}

/// Start vhost device (Linux `vhost_dev_start`).
pub fn start_device(dev_id: u32) -> Result<(), &'static str> {
    let start_fn = {
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
        let ops = VHOST_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("vhost ops not found")?;
        dev_ops.handle_kick
    };
    (kick_fn)(dev_id, vq_index)
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
    crate::serial_println!("vhost: subsystem ready");
    Ok(())
}
