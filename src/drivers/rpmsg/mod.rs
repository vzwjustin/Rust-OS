//! RPMsg (Remote Processor Messaging) subsystem
//!
//! Provides virtio-based messaging between application processors and
//! remote coprocessors. Mirrors Linux's `drivers/rpmsg/rpmsg_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// RPMsg endpoint (Linux `struct rpmsg_endpoint`).
pub struct RpmsgEndpoint {
    pub id: u32,
    pub device_id: u32,
    pub src_addr: u32,
    pub dst_addr: u32,
    pub ops: RpmsgEndpointOps,
    pub cb_data: Option<u64>,
    pub active: bool,
}

/// RPMsg endpoint operations (Linux `struct rpmsg_device_ops`).
pub struct RpmsgEndpointOps {
    pub send: fn(ep_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub sendto: fn(ep_id: u32, dst: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub send_offchannel:
        fn(ep_id: u32, src: u32, dst: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub destroy: fn(ep_id: u32) -> Result<(), &'static str>,
}

/// RPMsg device (Linux `struct rpmsg_device`).
pub struct RpmsgDevice {
    pub id: u32,
    pub name: String,
    pub driver_name: Option<String>,
    pub src_addr: u32,
    pub dst_addr: u32,
    pub platform: String,
    pub bound: bool,
    pub endpoint_ids: Vec<u32>,
}

/// RPMsg driver (Linux `struct rpmsg_driver`).
pub struct RpmsgDriver {
    pub name: String,
    pub id_table: Vec<RpmsgDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
    pub callback: Option<fn(device_id: u32, src: u32, data: &[u8])>,
}

/// RPMsg device ID (Linux `struct rpmsg_device_id`).
#[derive(Debug, Clone)]
pub struct RpmsgDeviceId {
    pub name: String,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static EP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static RPMSG_DEVICES: RwLock<BTreeMap<u32, RpmsgDevice>> = RwLock::new(BTreeMap::new());
static RPMSG_ENDPOINTS: RwLock<BTreeMap<u32, RpmsgEndpoint>> = RwLock::new(BTreeMap::new());
static RPMSG_DRIVERS: RwLock<BTreeMap<u32, RpmsgDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an RPMsg device.
pub fn register_device(
    name: &str,
    src_addr: u32,
    dst_addr: u32,
    platform: &str,
) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("RPMsg device name is empty");
    }
    if platform.is_empty() {
        return Err("RPMsg platform name is empty");
    }
    if RPMSG_DEVICES.read().values().any(|dev| {
        dev.name == name
            && dev.platform == platform
            && dev.src_addr == src_addr
            && dev.dst_addr == dst_addr
    }) {
        return Err("RPMsg device already registered");
    }

    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = RpmsgDevice {
        id,
        name: String::from(name),
        driver_name: None,
        src_addr,
        dst_addr,
        platform: String::from(platform),
        bound: false,
        endpoint_ids: Vec::new(),
    };
    RPMSG_DEVICES.write().insert(id, dev);
    if let Err(err) = try_match_driver(id) {
        RPMSG_DEVICES.write().remove(&id);
        return Err(err);
    }
    Ok(id)
}

/// Register an RPMsg driver.
pub fn register_driver(driver: RpmsgDriver) -> Result<u32, &'static str> {
    if driver.name.is_empty() {
        return Err("RPMsg driver name is empty");
    }
    if driver.id_table.is_empty() {
        return Err("RPMsg driver ID table is empty");
    }
    if RPMSG_DRIVERS
        .read()
        .values()
        .any(|existing| existing.name == driver.name)
    {
        return Err("RPMsg driver already registered");
    }

    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    RPMSG_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = RPMSG_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && id_table.iter().any(|id| id.name == d.name))
            .map(|(id, _)| *id)
            .collect()
    };
    for dev_id in device_ids {
        if let Err(err) = try_match_driver(dev_id) {
            RPMSG_DRIVERS.write().remove(&id);
            return Err(err);
        }
    }
    Ok(id)
}

/// Try to match a device with a driver.
fn try_match_driver(device_id: u32) -> Result<(), &'static str> {
    let matched = {
        let devices = RPMSG_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let dev_name = dev.name.clone();

        let drivers = RPMSG_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id_entry in &drv.id_table {
                if dev_name == id_entry.name {
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
        let mut devices = RPMSG_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Create an RPMsg endpoint (Linux `rpmsg_create_ept`).
pub fn create_endpoint(
    device_id: u32,
    src_addr: u32,
    dst_addr: u32,
    ops: RpmsgEndpointOps,
) -> Result<u32, &'static str> {
    {
        let devices = RPMSG_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("RPMsg device not found")?;
        if dev.endpoint_ids.iter().any(|ep_id| {
            RPMSG_ENDPOINTS
                .read()
                .get(ep_id)
                .is_some_and(|ep| ep.src_addr == src_addr && ep.dst_addr == dst_addr)
        }) {
            return Err("RPMsg endpoint already registered");
        }
    }

    let ep_id = EP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ep = RpmsgEndpoint {
        id: ep_id,
        device_id,
        src_addr,
        dst_addr,
        ops,
        cb_data: None,
        active: true,
    };
    RPMSG_ENDPOINTS.write().insert(ep_id, ep);

    let mut devices = RPMSG_DEVICES.write();
    match devices.get_mut(&device_id) {
        Some(dev) => dev.endpoint_ids.push(ep_id),
        None => {
            RPMSG_ENDPOINTS.write().remove(&ep_id);
            return Err("RPMsg device not found");
        }
    }
    Ok(ep_id)
}

/// Destroy an RPMsg endpoint.
pub fn destroy_endpoint(ep_id: u32) -> Result<(), &'static str> {
    let (destroy_fn, device_id) = {
        let endpoints = RPMSG_ENDPOINTS.read();
        let ep = endpoints.get(&ep_id).ok_or("RPMsg endpoint not found")?;
        (ep.ops.destroy, ep.device_id)
    };
    (destroy_fn)(ep_id)?;

    RPMSG_ENDPOINTS.write().remove(&ep_id);
    let mut devices = RPMSG_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.endpoint_ids.retain(|&id| id != ep_id);
    }
    Ok(())
}

/// Send data on an RPMsg endpoint (Linux `rpmsg_send`).
pub fn send(ep_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    if data.is_empty() {
        return Err("RPMsg payload is empty");
    }
    let send_fn = {
        let endpoints = RPMSG_ENDPOINTS.read();
        let ep = endpoints.get(&ep_id).ok_or("RPMsg endpoint not found")?;
        if !ep.active {
            return Err("RPMsg endpoint not active");
        }
        ep.ops.send
    };
    (send_fn)(ep_id, data)
}

/// Send data to a specific destination (Linux `rpmsg_sendto`).
pub fn sendto(ep_id: u32, dst: u32, data: &[u8]) -> Result<usize, &'static str> {
    if data.is_empty() {
        return Err("RPMsg payload is empty");
    }
    let sendto_fn = {
        let endpoints = RPMSG_ENDPOINTS.read();
        let ep = endpoints.get(&ep_id).ok_or("RPMsg endpoint not found")?;
        ep.ops.sendto
    };
    (sendto_fn)(ep_id, dst, data)
}

/// Send data on an off-channel endpoint (Linux `rpmsg_send_offchannel`).
pub fn send_offchannel(ep_id: u32, src: u32, dst: u32, data: &[u8]) -> Result<usize, &'static str> {
    let send_fn = {
        let endpoints = RPMSG_ENDPOINTS.read();
        let ep = endpoints.get(&ep_id).ok_or("RPMsg endpoint not found")?;
        ep.ops.send_offchannel
    };
    (send_fn)(ep_id, src, dst, data)
}

/// Deliver a received message to the device's callback (called by transport).
pub fn rx_callback(device_id: u32, src: u32, data: &[u8]) {
    let cb_fn = {
        let drivers = RPMSG_DRIVERS.read();
        let devices = RPMSG_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) => d,
            None => return,
        };
        let drv_name = match &dev.driver_name {
            Some(n) => n.clone(),
            None => return,
        };
        drivers
            .iter()
            .find(|(_, d)| d.name == drv_name)
            .and_then(|(_, d)| d.callback)
    };
    if let Some(cb) = cb_fn {
        cb(device_id, src, data);
    }
}

/// List all RPMsg devices.
pub fn list_devices() -> Vec<(u32, String, bool)> {
    RPMSG_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.bound))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    RPMSG_DEVICES.read().len()
}

// ── Software RPMsg ──────────────────────────────────────────────────────

fn sw_send(_ep_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_sendto(_ep_id: u32, _dst: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_send_offchannel(
    _ep_id: u32,
    _src: u32,
    _dst: u32,
    data: &[u8],
) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_destroy(_ep_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software RPMsg endpoint ops.
pub fn software_rpmsg_ops() -> RpmsgEndpointOps {
    RpmsgEndpointOps {
        send: sw_send,
        sendto: sw_sendto,
        send_offchannel: sw_send_offchannel,
        destroy: sw_destroy,
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
    if !RPMSG_DEVICES.read().is_empty() {
        return Ok(());
    }

    let dev_id = register_device("sw-rpmsg", 0x400, 0x401, "virtio")?;
    crate::serial_println!("rpmsg: software device registered (id={})", dev_id);
    Ok(())
}
