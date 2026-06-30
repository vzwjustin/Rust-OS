//! # vDPA (vhost Data Path Acceleration) bus
//!
//! Models the vDPA bus: devices that expose a virtqueue-compatible datapath
//! together with a management interface for feature negotiation, vring
//! programming, and status control. Unlike full vhost offload, a vDPA device's
//! datapath is "accelerated" (here, serviced directly by a backend) while its
//! control plane is mediated by the bus. The datapath is the transport-agnostic
//! software virtqueue from `virtio::software`.
//!
//! Mirrors Linux's `drivers/vdpa/vdpa.c` and the `vdpa_config_ops` interface.

use crate::drivers::virtio::software::{
    NetLoopback, Segment, SplitVirtqueue, VirtioBackend, NET_HDR_LEN,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// vDPA device class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdpaClass {
    Net,
    Block,
}

/// VirtIO status bits used by the vDPA control plane.
pub mod status {
    pub const RESET: u8 = 0;
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const FAILED: u8 = 128;
}

/// A single accelerated virtqueue of a vDPA device.
pub struct VdpaVq {
    pub index: u32,
    pub num: u16,
    pub ready: bool,
    pub desc_addr: u64,
    pub driver_addr: u64,
    pub device_addr: u64,
    pub vq: SplitVirtqueue,
    backend: Box<dyn VirtioBackend>,
    pub kicks: u32,
}

impl VdpaVq {
    fn new(index: u32, num: u16, backend: Box<dyn VirtioBackend>) -> Result<Self, &'static str> {
        Ok(VdpaVq {
            index,
            num,
            ready: false,
            desc_addr: 0,
            driver_addr: 0,
            device_addr: 0,
            vq: SplitVirtqueue::new(num)?,
            backend,
            kicks: 0,
        })
    }
}

/// A vDPA device registered on the bus.
pub struct VdpaDevice {
    pub id: u32,
    pub name: String,
    pub class: VdpaClass,
    pub device_features: u64,
    pub driver_features: u64,
    pub status: u8,
    pub vqs: Vec<VdpaVq>,
}

// ── Bus registry ──────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static VDPA_DEVICES: RwLock<BTreeMap<u32, VdpaDevice>> = RwLock::new(BTreeMap::new());

/// Register a vDPA device on the bus. `backends` provides one backend per
/// accelerated virtqueue.
pub fn register_device(
    name: &str,
    class: VdpaClass,
    num: u16,
    device_features: u64,
    mut backends: Vec<Box<dyn VirtioBackend>>,
) -> Result<u32, &'static str> {
    if backends.is_empty() {
        return Err("vdpa: at least one virtqueue backend required");
    }
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut vqs = Vec::new();
    let nvqs = backends.len();
    for i in 0..nvqs {
        let backend = backends.remove(0);
        vqs.push(VdpaVq::new(i as u32, num, backend)?);
    }
    let dev = VdpaDevice {
        id,
        name: String::from(name),
        class,
        device_features,
        driver_features: 0,
        status: status::RESET,
        vqs,
    };
    VDPA_DEVICES.write().insert(id, dev);
    Ok(id)
}

// ── Management ops (vdpa_config_ops) ───────────────────────────────────────

/// get_device_features.
pub fn get_features(dev_id: u32) -> Result<u64, &'static str> {
    let devs = VDPA_DEVICES.read();
    devs.get(&dev_id)
        .map(|d| d.device_features)
        .ok_or("vdpa device not found")
}

/// set_driver_features — only bits the device offers may be acked.
pub fn set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    if features & !dev.device_features != 0 {
        return Err("vdpa: unsupported features requested");
    }
    dev.driver_features = features;
    Ok(())
}

pub fn driver_features(dev_id: u32) -> Result<u64, &'static str> {
    let devs = VDPA_DEVICES.read();
    devs.get(&dev_id)
        .map(|d| d.driver_features)
        .ok_or("vdpa device not found")
}

/// set_status. Clears FEATURES_OK if the driver acked unsupported features.
pub fn set_status(dev_id: u32, value: u8) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    let mut effective = value;
    if value & status::FEATURES_OK != 0 && dev.driver_features & !dev.device_features != 0 {
        effective &= !status::FEATURES_OK;
    }
    dev.status = effective;
    Ok(())
}

pub fn get_status(dev_id: u32) -> Result<u8, &'static str> {
    let devs = VDPA_DEVICES.read();
    devs.get(&dev_id)
        .map(|d| d.status)
        .ok_or("vdpa device not found")
}

/// set_vq_num.
pub fn set_vq_num(dev_id: u32, idx: u32, num: u16) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    let vq = dev.vqs.get_mut(idx as usize).ok_or("vdpa: bad vq index")?;
    vq.num = num;
    Ok(())
}

/// set_vq_address.
pub fn set_vq_address(
    dev_id: u32,
    idx: u32,
    desc: u64,
    driver: u64,
    device: u64,
) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    let vq = dev.vqs.get_mut(idx as usize).ok_or("vdpa: bad vq index")?;
    vq.desc_addr = desc;
    vq.driver_addr = driver;
    vq.device_addr = device;
    Ok(())
}

/// set_vq_ready.
pub fn set_vq_ready(dev_id: u32, idx: u32, ready: bool) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    let vq = dev.vqs.get_mut(idx as usize).ok_or("vdpa: bad vq index")?;
    vq.ready = ready;
    Ok(())
}

/// get_vq_ready.
pub fn get_vq_ready(dev_id: u32, idx: u32) -> Result<bool, &'static str> {
    let devs = VDPA_DEVICES.read();
    let dev = devs.get(&dev_id).ok_or("vdpa device not found")?;
    let vq = dev.vqs.get(idx as usize).ok_or("vdpa: bad vq index")?;
    Ok(vq.ready)
}

/// kick_vq — drive the accelerated datapath for one virtqueue.
pub fn kick_vq(dev_id: u32, idx: u32) -> Result<(), &'static str> {
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    if dev.status & status::DRIVER_OK == 0 {
        return Err("vdpa: device not DRIVER_OK");
    }
    let vq = dev.vqs.get_mut(idx as usize).ok_or("vdpa: bad vq index")?;
    if !vq.ready {
        return Err("vdpa: vq not ready");
    }
    vq.kicks += 1;
    vq.backend.service(&mut vq.vq);
    Ok(())
}

/// Run a driver-side operation against a vq's datapath (for consumers/tests).
pub fn with_vq<F, R>(dev_id: u32, idx: u32, f: F) -> Result<R, &'static str>
where
    F: FnOnce(&mut SplitVirtqueue) -> R,
{
    let mut devs = VDPA_DEVICES.write();
    let dev = devs.get_mut(&dev_id).ok_or("vdpa device not found")?;
    let vq = dev.vqs.get_mut(idx as usize).ok_or("vdpa: bad vq index")?;
    Ok(f(&mut vq.vq))
}

pub fn list_devices() -> Vec<(u32, String, VdpaClass, u8, usize)> {
    VDPA_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.class, d.status, d.vqs.len()))
        .collect()
}

pub fn device_count() -> usize {
    VDPA_DEVICES.read().len()
}

// ── vDPA-net consumer + handshake ──────────────────────────────────────────

const VIRTIO_NET_F_MAC: u64 = 1 << 5;
const VIRTIO_NET_F_STATUS: u64 = 1 << 16;

/// Bind a vDPA-net device through the standard control-plane handshake and
/// drive one frame through its accelerated datapath. Returns the device id.
fn setup_vdpa_net() -> Result<u32, &'static str> {
    let dev_features = VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS;
    let backends: Vec<Box<dyn VirtioBackend>> =
        alloc::vec![Box::new(NetLoopback::new()) as Box<dyn VirtioBackend>];
    let id = register_device("vdpa-net0", VdpaClass::Net, 16, dev_features, backends)?;

    // Control-plane handshake.
    set_status(id, status::ACKNOWLEDGE)?;
    set_status(id, status::ACKNOWLEDGE | status::DRIVER)?;
    let offered = get_features(id)?;
    set_features(id, offered & dev_features)?;
    set_status(
        id,
        status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK,
    )?;
    if get_status(id)? & status::FEATURES_OK == 0 {
        return Err("vdpa-net: FEATURES_OK rejected");
    }

    // Program and ready the datapath vq.
    set_vq_num(id, 0, 16)?;
    set_vq_address(id, 0, 0x1000, 0x2000, 0x3000)?;
    set_vq_ready(id, 0, true)?;
    set_status(
        id,
        status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK | status::DRIVER_OK,
    )?;

    // Driver side: submit a frame then kick the accelerated datapath.
    let frame = [0x11u8, 0x22, 0x33, 0x44];
    let mut tx = Vec::new();
    tx.extend_from_slice(&[0u8; NET_HDR_LEN]);
    tx.extend_from_slice(&frame);
    with_vq(id, 0, |vq| vq.add_buf(&[Segment::Out(&tx)]))??;
    kick_vq(id, 0)?;
    with_vq(id, 0, |vq| vq.get_buf())?;
    Ok(id)
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    match setup_vdpa_net() {
        Ok(id) => {
            let feat = driver_features(id).unwrap_or(0);
            let st = get_status(id).unwrap_or(0);
            crate::serial_println!(
                "vdpa: {} device(s) on bus (vdpa-net id={} feat=0x{:X} status=0x{:02X})",
                device_count(),
                id,
                feat,
                st
            );
        }
        Err(e) => crate::serial_println!("vdpa: setup failed: {}", e),
    }
    Ok(())
}
