//! virtio-pci subsystem
//!
//! Provides virtio device transport over PCI bus.
//! Mirrors Linux's `drivers/virtio/virtio_pci_modern.c` and `virtio_pci_legacy.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// virtio-pci transport type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciTransport {
    Legacy,
    Modern,
    Transitional,
}

/// virtio-pci device (Linux `struct virtio_pci_device`).
pub struct VirtioPciDevice {
    pub id: u32,
    pub name: String,
    pub pci_vendor: u16,
    pub pci_device: u16,
    pub transport: VirtioPciTransport,
    pub virtio_device_id: u32,
    pub features: u64,
    pub status: VirtioPciStatus,
    pub queues: Vec<VirtioPciQueue>,
    pub config_gen: u32,
}

/// virtio-pci status (Linux `struct virtio_pci_common_cfg` status bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciStatus {
    Reset,
    Acknowledge,
    Driver,
    DriverOk,
    FeaturesOk,
    DeviceNeedReset,
    Failed,
}

/// virtio-pci queue (Linux `struct virtio_pci_vq_info`).
pub struct VirtioPciQueue {
    pub index: u32,
    pub size: u16,
    pub enabled: bool,
    pub vector: u16,
    pub desc_addr: u64,
    pub avail_addr: u64,
    pub used_addr: u64,
}

/// virtio-pci device operations.
pub struct VirtioPciOps {
    pub read_config: fn(device_id: u32, offset: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write_config: fn(device_id: u32, offset: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub set_status: fn(device_id: u32, status: VirtioPciStatus) -> Result<(), &'static str>,
    pub get_features: fn(device_id: u32) -> Result<u64, &'static str>,
    pub set_features: fn(device_id: u32, features: u64) -> Result<(), &'static str>,
    pub select_queue: fn(device_id: u32, queue_index: u32) -> Result<(), &'static str>,
    pub activate_queue: fn(
        device_id: u32,
        queue_index: u32,
        desc: u64,
        avail: u64,
        used: u64,
    ) -> Result<(), &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static VIRTIO_PCI_DEVICES: RwLock<BTreeMap<u32, VirtioPciDevice>> = RwLock::new(BTreeMap::new());
static VIRTIO_PCI_OPS: RwLock<BTreeMap<u32, VirtioPciOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a virtio-pci device.
pub fn register_device(
    name: &str,
    pci_vendor: u16,
    pci_device: u16,
    transport: VirtioPciTransport,
    virtio_device_id: u32,
    ops: VirtioPciOps,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = VirtioPciDevice {
        id,
        name: String::from(name),
        pci_vendor,
        pci_device,
        transport,
        virtio_device_id,
        features: 0,
        status: VirtioPciStatus::Reset,
        queues: Vec::new(),
        config_gen: 0,
    };
    VIRTIO_PCI_DEVICES.write().insert(id, dev);
    VIRTIO_PCI_OPS.write().insert(id, ops);
    Ok(id)
}

/// Probe a virtio-pci device (Linux `virtio_pci_probe`).
pub fn probe_device(device_id: u32) -> Result<(), &'static str> {
    let get_features_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.get_features
    };

    let features = (get_features_fn)(device_id)?;

    let mut devices = VIRTIO_PCI_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("virtio-pci device not found")?;
    dev.features = features;
    dev.status = VirtioPciStatus::Acknowledge;

    // Create default queues
    for i in 0..2 {
        dev.queues.push(VirtioPciQueue {
            index: i,
            size: 256,
            enabled: false,
            vector: 0,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
        });
    }
    Ok(())
}

/// Set device status (Linux `vp_set_status`).
pub fn set_status(device_id: u32, status: VirtioPciStatus) -> Result<(), &'static str> {
    let set_status_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.set_status
    };
    (set_status_fn)(device_id, status)?;

    let mut devices = VIRTIO_PCI_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.status = status;
    }
    Ok(())
}

/// Negotiate features (Linux `vp_finalize_features`).
pub fn set_features(device_id: u32, features: u64) -> Result<(), &'static str> {
    let set_features_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.set_features
    };
    (set_features_fn)(device_id, features)?;

    let mut devices = VIRTIO_PCI_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.features = features;
    }
    Ok(())
}

/// Read device config space.
pub fn read_config(device_id: u32, offset: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.read_config
    };
    (read_fn)(device_id, offset, buf)
}

/// Write device config space.
pub fn write_config(device_id: u32, offset: u32, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.write_config
    };
    (write_fn)(device_id, offset, data)
}

/// Activate a virtqueue (Linux `vp_active_vq`).
pub fn activate_queue(
    device_id: u32,
    queue_index: u32,
    desc: u64,
    avail: u64,
    used: u64,
) -> Result<(), &'static str> {
    let activate_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.activate_queue
    };
    (activate_fn)(device_id, queue_index, desc, avail, used)?;

    let mut devices = VIRTIO_PCI_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        if let Some(q) = dev.queues.get_mut(queue_index as usize) {
            q.desc_addr = desc;
            q.avail_addr = avail;
            q.used_addr = used;
            q.enabled = true;
        }
    }
    Ok(())
}

/// Select a virtqueue (Linux `vp_find_vq`).
pub fn select_queue(device_id: u32, queue_index: u32) -> Result<(), &'static str> {
    let select_fn = {
        let ops = VIRTIO_PCI_OPS.read();
        let dev_ops = ops.get(&device_id).ok_or("virtio-pci ops not found")?;
        dev_ops.select_queue
    };
    (select_fn)(device_id, queue_index)
}

/// List all virtio-pci devices.
pub fn list_devices() -> Vec<(u32, String, VirtioPciTransport, u32)> {
    VIRTIO_PCI_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.transport, d.virtio_device_id))
        .collect()
}

/// Get device status.
pub fn get_status(device_id: u32) -> Result<VirtioPciStatus, &'static str> {
    let devices = VIRTIO_PCI_DEVICES.read();
    let dev = devices
        .get(&device_id)
        .ok_or("virtio-pci device not found")?;
    Ok(dev.status)
}

/// Count registered devices.
pub fn device_count() -> usize {
    VIRTIO_PCI_DEVICES.read().len()
}

// ── Software virtio-pci ─────────────────────────────────────────────────

fn sw_read_config(_dev_id: u32, _offset: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_write_config(_dev_id: u32, _offset: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_set_status(_dev_id: u32, _status: VirtioPciStatus) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_features(_dev_id: u32) -> Result<u64, &'static str> {
    Ok(0)
}
fn sw_set_features(_dev_id: u32, _features: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_select_queue(_dev_id: u32, _queue_index: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_activate_queue(
    _dev_id: u32,
    _queue_index: u32,
    _desc: u64,
    _avail: u64,
    _used: u64,
) -> Result<(), &'static str> {
    Ok(())
}

/// Software virtio-pci ops.
pub fn software_virtio_pci_ops() -> VirtioPciOps {
    VirtioPciOps {
        read_config: sw_read_config,
        write_config: sw_write_config,
        set_status: sw_set_status,
        get_features: sw_get_features,
        set_features: sw_set_features,
        select_queue: sw_select_queue,
        activate_queue: sw_activate_queue,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_virtio_pci_ops();
    // virtio-net: PCI vendor 0x1AF4, device 0x1000, virtio device ID 1 (network)
    let dev_id = register_device(
        "sw-virtio-net-pci",
        0x1AF4,
        0x1000,
        VirtioPciTransport::Transitional,
        1,
        ops,
    )?;
    probe_device(dev_id)?;
    set_status(dev_id, VirtioPciStatus::DriverOk)?;

    // virtio-blk: PCI vendor 0x1AF4, device 0x1001, virtio device ID 2 (block)
    let ops2 = software_virtio_pci_ops();
    let dev_id2 = register_device(
        "sw-virtio-blk-pci",
        0x1AF4,
        0x1001,
        VirtioPciTransport::Transitional,
        2,
        ops2,
    )?;
    probe_device(dev_id2)?;
    set_status(dev_id2, VirtioPciStatus::DriverOk)?;

    Ok(())
}
