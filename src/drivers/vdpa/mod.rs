//! vDPA (vhost DataPath Acceleration) subsystem
//!
//! Provides a framework for vDPA devices that expose a virtio datapath
//! with a vendor-specific control path. Mirrors Linux's `drivers/vdpa/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// vDPA device (Linux `struct vdpa_device`).
pub struct VdpaDevice {
    pub id: u32,
    pub name: String,
    pub bus_id: u32,
    pub vendor: u32,
    pub device_id: u32,
    pub features: u64,
    pub num_vqs: u32,
    pub vq_states: Vec<VqState>,
    pub status: VdpaStatus,
    pub config: Vec<u8>,
    pub ops: VdpaOps,
}

/// vDPA virtqueue state (Linux `struct vdpa_vq_state`).
#[derive(Debug, Clone, Default)]
pub struct VqState {
    pub avail_idx: u16,
    pub used_idx: u16,
    pub ready: bool,
}

/// vDPA status flags (Linux `VIRTIO_CONFIG_S_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdpaStatus {
    Reset,
    Acknowledge,
    Driver,
    DriverOk,
    FeaturesOk,
}

/// vDPA device operations (Linux `struct vdpa_config_ops`).
pub struct VdpaOps {
    pub get_features: fn(dev_id: u32) -> u64,
    pub set_features: fn(dev_id: u32, features: u64) -> Result<(), &'static str>,
    pub get_status: fn(dev_id: u32) -> VdpaStatus,
    pub set_status: fn(dev_id: u32, status: VdpaStatus) -> Result<(), &'static str>,
    pub get_config: fn(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub set_config: fn(dev_id: u32, buf: &[u8]) -> Result<(), &'static str>,
    pub get_vq_state: fn(dev_id: u32, vq_idx: u32) -> Result<VqState, &'static str>,
    pub set_vq_state: fn(dev_id: u32, vq_idx: u32, state: &VqState) -> Result<(), &'static str>,
    pub get_vq_num_max: fn(dev_id: u32, vq_idx: u32) -> u32,
    pub get_vq_align: fn(dev_id: u32) -> u32,
    pub get_device_id: fn(dev_id: u32) -> u32,
    pub get_vendor_id: fn(dev_id: u32) -> u32,
}

/// vDPA driver (Linux `struct vdpa_driver`).
pub struct VdpaDriver {
    pub name: String,
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static VDPA_DEVS: RwLock<BTreeMap<u32, VdpaDevice>> = RwLock::new(BTreeMap::new());
static VDPA_DRIVERS: RwLock<BTreeMap<u32, VdpaDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a vDPA device.
pub fn register_device(
    name: &str,
    bus_id: u32,
    vendor: u32,
    device_id: u32,
    num_vqs: u32,
    ops: VdpaOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let vq_states = (0..num_vqs).map(|_| VqState::default()).collect();
    let dev = VdpaDevice {
        id,
        name: String::from(name),
        bus_id,
        vendor,
        device_id,
        features: 0,
        num_vqs,
        vq_states,
        status: VdpaStatus::Reset,
        config: Vec::new(),
        ops,
    };
    VDPA_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Unregister a vDPA device.
pub fn unregister_device(dev_id: u32) -> Result<(), &'static str> {
    VDPA_DEVS
        .write()
        .remove(&dev_id)
        .ok_or("vDPA device not found")?;
    Ok(())
}

/// Register a vDPA driver.
pub fn register_driver(driver: VdpaDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    VDPA_DRIVERS.write().insert(id, driver);
    Ok(id)
}

/// Get device features.
pub fn get_features(dev_id: u32) -> Result<u64, &'static str> {
    let devs = VDPA_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
    Ok((dev.ops.get_features)(dev_id))
}

/// Set device features.
pub fn set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.set_features
    };
    (ops_fn)(dev_id, features)?;
    let mut devs = VDPA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.features = features;
    }
    Ok(())
}

/// Get device status.
pub fn get_status(dev_id: u32) -> Result<VdpaStatus, &'static str> {
    let devs = VDPA_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
    Ok((dev.ops.get_status)(dev_id))
}

/// Set device status.
pub fn set_status(dev_id: u32, status: VdpaStatus) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.set_status
    };
    (ops_fn)(dev_id, status)?;
    let mut devs = VDPA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.status = status;
    }
    Ok(())
}

/// Get device config.
pub fn get_config(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.get_config
    };
    (ops_fn)(dev_id, buf)
}

/// Set device config.
pub fn set_config(dev_id: u32, buf: &[u8]) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.set_config
    };
    (ops_fn)(dev_id, buf)
}

/// Get virtqueue state.
pub fn get_vq_state(dev_id: u32, vq_idx: u32) -> Result<VqState, &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.get_vq_state
    };
    (ops_fn)(dev_id, vq_idx)
}

/// Set virtqueue state.
pub fn set_vq_state(dev_id: u32, vq_idx: u32, state: &VqState) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = VDPA_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
        dev.ops.set_vq_state
    };
    (ops_fn)(dev_id, vq_idx, state)?;
    let mut devs = VDPA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        if let Some(vq) = dev.vq_states.get_mut(vq_idx as usize) {
            *vq = state.clone();
        }
    }
    Ok(())
}

/// List all vDPA devices.
pub fn list_devices() -> Vec<(u32, String, u32, u32, u32)> {
    VDPA_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.bus_id, d.vendor, d.device_id))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    VDPA_DEVS.read().len()
}

// ── Software vDPA ───────────────────────────────────────────────────────

fn sw_get_features(_dev_id: u32) -> u64 {
    0x1 | 0x2 // VIRTIO_F_VERSION_1 | RING_INDIRECT_DESC
}

fn sw_set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.features = features;
    }
    Ok(())
}

fn sw_get_status(dev_id: u32) -> VdpaStatus {
    let devs = VDPA_DEVS.read();
    devs.get(&dev_id).map_or(VdpaStatus::Reset, |d| d.status)
}

fn sw_set_status(dev_id: u32, status: VdpaStatus) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.status = status;
    }
    Ok(())
}

fn sw_get_config(_dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}

fn sw_set_config(_dev_id: u32, _buf: &[u8]) -> Result<(), &'static str> {
    Ok(())
}

fn sw_get_vq_state(dev_id: u32, vq_idx: u32) -> Result<VqState, &'static str> {
    let devs = VDPA_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("vDPA device not found")?;
    dev.vq_states
        .get(vq_idx as usize)
        .cloned()
        .ok_or("VQ index out of range")
}

fn sw_set_vq_state(dev_id: u32, vq_idx: u32, state: &VqState) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vDPA device not found")?;
    let vq = dev
        .vq_states
        .get_mut(vq_idx as usize)
        .ok_or("VQ index out of range")?;
    *vq = state.clone();
    Ok(())
}

fn sw_get_vq_num_max(_dev_id: u32, _vq_idx: u32) -> u32 {
    256
}

fn sw_get_vq_align(_dev_id: u32) -> u32 {
    4096
}

fn sw_get_device_id(_dev_id: u32) -> u32 {
    1 // VIRTIO_ID_NET
}

fn sw_get_vendor_id(_dev_id: u32) -> u32 {
    0x1AF4 // Red Hat / virtio
}

/// Software vDPA ops.
pub fn software_vdpa_ops() -> VdpaOps {
    VdpaOps {
        get_features: sw_get_features,
        set_features: sw_set_features,
        get_status: sw_get_status,
        set_status: sw_set_status,
        get_config: sw_get_config,
        set_config: sw_set_config,
        get_vq_state: sw_get_vq_state,
        set_vq_state: sw_set_vq_state,
        get_vq_num_max: sw_get_vq_num_max,
        get_vq_align: sw_get_vq_align,
        get_device_id: sw_get_device_id,
        get_vendor_id: sw_get_vendor_id,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !VDPA_DEVS.read().is_empty() {
        return Ok(());
    }

    let ops = software_vdpa_ops();
    let dev_id = register_device("sw-vdpa-net", 0, 0x1AF4, 1, 2, ops)?;
    crate::serial_println!(
        "vdpa: software net device registered (id={}, 2 vqs)",
        dev_id
    );
    Ok(())
}
