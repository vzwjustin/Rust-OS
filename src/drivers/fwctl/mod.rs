//! Fwctl (Firmware Control) driver subsystem
//!
//! Provides a framework for secure firmware communication channels.
//! Mirrors Linux's `drivers/fwctl/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Firmware control device type (Linux `enum fwctl_device_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwctlDeviceType {
    IntelMei,
    AmdPsp,
    DellWmi,
    ThinkpadAcpi,
    Generic,
}

/// Firmware control device (Linux `struct fwctl_device`).
pub struct FwctlDevice {
    pub id: u32,
    pub name: String,
    pub dev_type: FwctlDeviceType,
    pub ops: FwctlOps,
    pub capabilities: u32,
    pub initialized: bool,
}

/// Firmware control operations (Linux `struct fwctl_ops`).
pub struct FwctlOps {
    pub init: fn(dev_id: u32) -> Result<(), &'static str>,
    pub send: fn(dev_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub recv: fn(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub get_info: fn(dev_id: u32) -> Result<FwctlInfo, &'static str>,
}

/// Firmware control device info.
#[derive(Debug, Clone)]
pub struct FwctlInfo {
    pub name: String,
    pub version: String,
    pub features: u32,
    pub max_msg_size: usize,
}

/// Firmware control command (Linux `struct fwctl_command`).
#[derive(Debug, Clone)]
pub struct FwctlCommand {
    pub dev_id: u32,
    pub opcode: u32,
    pub payload: Vec<u8>,
    pub response: Vec<u8>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static FWCTL_DEVICES: RwLock<BTreeMap<u32, FwctlDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a firmware control device (Linux `fwctl_register`).
pub fn register_device(
    name: &str,
    dev_type: FwctlDeviceType,
    ops: FwctlOps,
    capabilities: u32,
) -> Result<u32, &'static str> {
    if name.trim().is_empty() {
        return Err("Fwctl device name required");
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = FwctlDevice {
        id,
        name: String::from(name),
        dev_type,
        ops,
        capabilities,
        initialized: false,
    };
    FWCTL_DEVICES.write().insert(id, dev);
    Ok(id)
}

/// Initialize a firmware control device.
pub fn init_device(dev_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let devs = FWCTL_DEVICES.read();
        let dev = devs.get(&dev_id).ok_or("Fwctl device not found")?;
        dev.ops.init
    };
    (init_fn)(dev_id)?;
    let mut devs = FWCTL_DEVICES.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.initialized = true;
    }
    Ok(())
}

/// Send data to a firmware control device.
pub fn send(dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let send_fn = {
        let devs = FWCTL_DEVICES.read();
        let dev = devs.get(&dev_id).ok_or("Fwctl device not found")?;
        if !dev.initialized {
            return Err("Fwctl device not initialized");
        }
        dev.ops.send
    };
    (send_fn)(dev_id, data)
}

/// Receive data from a firmware control device.
pub fn recv(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let recv_fn = {
        let devs = FWCTL_DEVICES.read();
        let dev = devs.get(&dev_id).ok_or("Fwctl device not found")?;
        if !dev.initialized {
            return Err("Fwctl device not initialized");
        }
        dev.ops.recv
    };
    (recv_fn)(dev_id, buf)
}

/// Execute a firmware command (send + recv).
pub fn execute_command(cmd: &mut FwctlCommand) -> Result<usize, &'static str> {
    let sent = send(cmd.dev_id, &cmd.payload)?;
    let mut buf = alloc::vec![0u8; 4096];
    let received = recv(cmd.dev_id, &mut buf)?;
    cmd.response = buf[..received].to_vec();
    Ok(sent)
}

/// Get device info.
pub fn get_info(dev_id: u32) -> Result<FwctlInfo, &'static str> {
    let info_fn = {
        let devs = FWCTL_DEVICES.read();
        let dev = devs.get(&dev_id).ok_or("Fwctl device not found")?;
        if !dev.initialized {
            return Err("Fwctl device not initialized");
        }
        dev.ops.get_info
    };
    (info_fn)(dev_id)
}

/// List all firmware control devices.
pub fn list_devices() -> Vec<(u32, String, FwctlDeviceType, bool)> {
    FWCTL_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.initialized))
        .collect()
}

/// Count devices.
pub fn device_count() -> usize {
    FWCTL_DEVICES.read().len()
}

// ── Software firmware control ───────────────────────────────────────────

fn sw_init(_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send(_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_recv(_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_get_info(_id: u32) -> Result<FwctlInfo, &'static str> {
    Ok(FwctlInfo {
        name: String::from("sw-fwctl"),
        version: String::from("1.0"),
        features: 0,
        max_msg_size: 4096,
    })
}

/// Software fwctl ops.
pub fn software_fwctl_ops() -> FwctlOps {
    FwctlOps {
        init: sw_init,
        send: sw_send,
        recv: sw_recv,
        get_info: sw_get_info,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !FWCTL_DEVICES.read().is_empty() {
        return Ok(());
    }

    let ops = software_fwctl_ops();
    let dev_id = register_device("sw-fwctl", FwctlDeviceType::Generic, ops, 0)?;
    init_device(dev_id)?;

    crate::serial_println!(
        "fwctl: software firmware control device registered (id={})",
        dev_id
    );
    Ok(())
}
