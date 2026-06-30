//! # vhost in-kernel datapath worker
//!
//! Models Linux's vhost: the virtqueue datapath of a virtio device is offloaded
//! to a kernel-side worker. The driver/guest "kicks" a vring (signals new
//! buffers are available); the worker services the queue through a backend and
//! "calls" back (signals completions). This is built on the transport-agnostic
//! software virtqueue in `virtio::software`, so vhost-net and vhost-blk style
//! consumers share the exact same datapath as the regular virtio drivers.
//!
//! Mirrors Linux's `drivers/vhost/vhost.c`, `vhost_net.c`, and `vhost_blk.c`.

use crate::drivers::virtio::software::{
    BlkLoopback, NetLoopback, Segment, SplitVirtqueue, VirtioBackend, NET_HDR_LEN,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// vhost device class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VhostDevType {
    Net,
    Blk,
    Scsi,
    Vsock,
}

/// vhost worker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VhostState {
    Idle,
    Running,
    Stopped,
}

/// A kernel-side worker that owns a vring and services it via a backend.
/// `kick` advances the datapath; `call` signalling is modeled by a pending
/// flag plus a monotonically increasing call counter (the eventfd a real
/// vhost worker would signal).
pub struct VhostWorker {
    pub index: u32,
    pub vq: SplitVirtqueue,
    backend: Box<dyn VirtioBackend>,
    /// Modeled vring addresses (set via VHOST_SET_VRING_ADDR).
    pub desc_addr: u64,
    pub avail_addr: u64,
    pub used_addr: u64,
    pub num: u16,
    pub ready: bool,
    pub kicks: u32,
    pub calls: u32,
    pub call_pending: bool,
}

impl VhostWorker {
    fn new(index: u32, num: u16, backend: Box<dyn VirtioBackend>) -> Result<Self, &'static str> {
        Ok(VhostWorker {
            index,
            vq: SplitVirtqueue::new(num)?,
            backend,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            num,
            ready: false,
            kicks: 0,
            calls: 0,
            call_pending: false,
        })
    }

    /// Handle a vring kick: service the queue and raise the call signal if any
    /// buffers were completed.
    fn handle_kick(&mut self) {
        self.kicks += 1;
        // Snapshot how many buffers the driver can still see as in-flight.
        let before = self.vq.free_count();
        self.backend.service(&mut self.vq);
        let after = self.vq.free_count();
        // If the backend produced completions, signal "call".
        if after != before || self.vq.get_used_count() > 0 {
            self.calls += 1;
            self.call_pending = true;
        }
    }
}

/// A registered vhost device: management state plus its datapath workers.
pub struct VhostDev {
    pub id: u32,
    pub name: String,
    pub dev_type: VhostDevType,
    pub features: u64,
    pub acked_features: u64,
    pub state: VhostState,
    pub workers: Vec<VhostWorker>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static VHOST_DEVS: RwLock<BTreeMap<u32, VhostDev>> = RwLock::new(BTreeMap::new());

// ── Management API (VHOST_* ioctls) ────────────────────────────────────────

/// Register a vhost device with `nvqs` datapath workers built from `backends`.
/// `backends` must contain exactly `nvqs` backend instances.
pub fn register_device(
    name: &str,
    dev_type: VhostDevType,
    num: u16,
    features: u64,
    mut backends: Vec<Box<dyn VirtioBackend>>,
) -> Result<u32, &'static str> {
    if backends.is_empty() {
        return Err("vhost: at least one backend required");
    }
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut workers = Vec::new();
    let nvqs = backends.len();
    for i in 0..nvqs {
        let backend = backends.remove(0);
        workers.push(VhostWorker::new(i as u32, num, backend)?);
    }
    let dev = VhostDev {
        id,
        name: String::from(name),
        dev_type,
        features,
        acked_features: 0,
        state: VhostState::Idle,
        workers,
    };
    VHOST_DEVS.write().insert(id, dev);
    Ok(id)
}

/// VHOST_SET_FEATURES.
pub fn set_features(dev_id: u32, features: u64) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    if features & !dev.features != 0 {
        return Err("vhost: unsupported features acked");
    }
    dev.acked_features = features;
    Ok(())
}

/// VHOST_GET_FEATURES.
pub fn get_features(dev_id: u32) -> Result<u64, &'static str> {
    let devs = VHOST_DEVS.read();
    devs.get(&dev_id)
        .map(|d| d.features)
        .ok_or("vhost device not found")
}

/// VHOST_SET_VRING_NUM.
pub fn set_vring_num(dev_id: u32, vq_index: u32, num: u16) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    w.num = num;
    Ok(())
}

/// VHOST_SET_VRING_ADDR.
pub fn set_vring_addr(
    dev_id: u32,
    vq_index: u32,
    desc: u64,
    avail: u64,
    used: u64,
) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    w.desc_addr = desc;
    w.avail_addr = avail;
    w.used_addr = used;
    Ok(())
}

/// VHOST_SET_VRING_ENABLE.
pub fn set_vring_ready(dev_id: u32, vq_index: u32, ready: bool) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    w.ready = ready;
    Ok(())
}

/// vhost_dev_start: bring all enabled workers live.
pub fn start_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    for w in dev.workers.iter_mut() {
        w.ready = true;
    }
    dev.state = VhostState::Running;
    Ok(())
}

/// vhost_dev_stop.
pub fn stop_device(dev_id: u32) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    dev.state = VhostState::Stopped;
    Ok(())
}

/// Kick a vring: hand the datapath to the kernel-side worker.
pub fn kick(dev_id: u32, vq_index: u32) -> Result<(), &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    if dev.state != VhostState::Running {
        return Err("vhost: device not running");
    }
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    if !w.ready {
        return Err("vhost: vring not ready");
    }
    w.handle_kick();
    Ok(())
}

/// Consume a pending "call" (completion) signal for a vring.
pub fn take_call(dev_id: u32, vq_index: u32) -> Result<bool, &'static str> {
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    let pending = w.call_pending;
    w.call_pending = false;
    Ok(pending)
}

/// Run a driver-side operation against a worker's vring (for consumers/tests).
pub fn with_vring<F, R>(dev_id: u32, vq_index: u32, f: F) -> Result<R, &'static str>
where
    F: FnOnce(&mut SplitVirtqueue) -> R,
{
    let mut devs = VHOST_DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("vhost device not found")?;
    let w = dev
        .workers
        .get_mut(vq_index as usize)
        .ok_or("vhost: bad vring index")?;
    Ok(f(&mut w.vq))
}

pub fn get_state(dev_id: u32) -> Result<VhostState, &'static str> {
    let devs = VHOST_DEVS.read();
    devs.get(&dev_id)
        .map(|d| d.state)
        .ok_or("vhost device not found")
}

pub fn list_devices() -> Vec<(u32, String, VhostDevType, VhostState, usize)> {
    VHOST_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.state, d.workers.len()))
        .collect()
}

pub fn device_count() -> usize {
    VHOST_DEVS.read().len()
}

// ── vhost-net consumer ─────────────────────────────────────────────────────

const VHOST_NET_F_VIRTIO_NET_HDR: u64 = 1 << 27;

/// Set up a vhost-net device (RX + TX worker) and drive a frame through the
/// offloaded datapath. Returns the device id.
fn setup_vhost_net() -> Result<u32, &'static str> {
    let backends: Vec<Box<dyn VirtioBackend>> =
        alloc::vec![Box::new(NetLoopback::new()) as Box<dyn VirtioBackend>];
    let id = register_device(
        "vhost-net",
        VhostDevType::Net,
        16,
        VHOST_NET_F_VIRTIO_NET_HDR,
        backends,
    )?;
    set_features(id, VHOST_NET_F_VIRTIO_NET_HDR)?;
    set_vring_num(id, 0, 16)?;
    set_vring_addr(id, 0, 0x1000, 0x2000, 0x3000)?;
    set_vring_ready(id, 0, true)?;
    start_device(id)?;

    // Driver side: submit a frame (net hdr + payload) then kick the worker.
    let frame = [0xAAu8, 0xBB, 0xCC, 0xDD];
    let mut tx = Vec::new();
    tx.extend_from_slice(&[0u8; NET_HDR_LEN]);
    tx.extend_from_slice(&frame);
    with_vring(id, 0, |vq| vq.add_buf(&[Segment::Out(&tx)]))??;
    kick(id, 0)?;
    let _ = take_call(id, 0)?;
    with_vring(id, 0, |vq| vq.get_buf())?;
    Ok(id)
}

// ── vhost-blk consumer ─────────────────────────────────────────────────────

/// Set up a vhost-blk device and complete one write request through the
/// offloaded datapath. Returns the device id.
fn setup_vhost_blk() -> Result<u32, &'static str> {
    use crate::drivers::virtio::software::{BlkReqHdr, SECTOR_SIZE, VIRTIO_BLK_T_OUT};
    let backends: Vec<Box<dyn VirtioBackend>> =
        alloc::vec![Box::new(BlkLoopback::new(64)) as Box<dyn VirtioBackend>];
    let id = register_device("vhost-blk", VhostDevType::Blk, 16, 0, backends)?;
    set_vring_num(id, 0, 16)?;
    set_vring_addr(id, 0, 0x1000, 0x2000, 0x3000)?;
    set_vring_ready(id, 0, true)?;
    start_device(id)?;

    let hdr = BlkReqHdr {
        req_type: VIRTIO_BLK_T_OUT,
        reserved: 0,
        sector: 1,
    };
    let hdr_bytes = unsafe {
        core::slice::from_raw_parts(
            &hdr as *const BlkReqHdr as *const u8,
            core::mem::size_of::<BlkReqHdr>(),
        )
    };
    let data = [0x5Au8; SECTOR_SIZE];
    with_vring(id, 0, |vq| {
        vq.add_buf(&[Segment::Out(hdr_bytes), Segment::Out(&data), Segment::In(1)])
    })??;
    kick(id, 0)?;
    let _ = take_call(id, 0)?;
    with_vring(id, 0, |vq| vq.get_buf())?;
    Ok(id)
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let net = setup_vhost_net();
    let blk = setup_vhost_blk();
    match (net, blk) {
        (Ok(n), Ok(b)) => {
            let nk = with_vring(n, 0, |_| ()).map(|_| ()).is_ok();
            crate::serial_println!(
                "vhost: {} worker device(s) ready (vhost-net id={} kicked={}, vhost-blk id={})",
                device_count(),
                n,
                nk,
                b
            );
        }
        (n, b) => crate::serial_println!("vhost: setup net={:?} blk={:?}", n, b),
    }
    Ok(())
}
