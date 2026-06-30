//! PECI (Platform Environment Control Interface) subsystem
//!
//! Provides PECI bus for Intel processor thermal and management communication.
//! Mirrors Linux's `drivers/peci/peci.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PECI device (Linux `struct peci_device`).
pub struct PeciDevice {
    pub id: u32,
    pub controller_id: u32,
    pub addr: u8,
    pub name: String,
    pub cpu_family: u32,
    pub cpu_model: u32,
    pub cpu_stepping: u32,
    pub core_mask: u32,
    pub microcode_rev: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// PECI controller (Linux `struct peci_controller`).
pub struct PeciController {
    pub id: u32,
    pub name: String,
    pub ops: PeciCtrlOps,
    pub device_ids: Vec<u32>,
    pub scan_mask: u32,
}

/// PECI controller operations (Linux `struct peci_controller_ops`).
pub struct PeciCtrlOps {
    pub xfer_cmd: fn(ctrl_id: u32, addr: u8, cmd: &PeciCmd) -> Result<Vec<u8>, &'static str>,
    pub xfer_msg: fn(ctrl_id: u32, msg: &PeciMsg) -> Result<(), &'static str>,
}

/// PECI command.
#[derive(Debug, Clone)]
pub struct PeciCmd {
    pub cmd_code: u8,
    pub host_id: u8,
    pub read_length: u8,
    pub write_data: Vec<u8>,
}

/// PECI message (Linux `struct peci_xfer_msg`).
#[derive(Debug, Clone)]
pub struct PeciMsg {
    pub addr: u8,
    pub tx_len: u8,
    pub rx_len: u8,
    pub tx_buf: Vec<u8>,
}

/// PECI driver (Linux `struct peci_driver`).
pub struct PeciDriver {
    pub name: String,
    pub id_table: Vec<PeciDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
}

/// PECI device ID (Linux `struct peci_device_id`).
#[derive(Debug, Clone)]
pub struct PeciDeviceId {
    pub family: u32,
    pub model: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PECI_CTRLS: RwLock<BTreeMap<u32, PeciController>> = RwLock::new(BTreeMap::new());
static PECI_DEVICES: RwLock<BTreeMap<u32, PeciDevice>> = RwLock::new(BTreeMap::new());
static PECI_DRIVERS: RwLock<BTreeMap<u32, PeciDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PECI controller.
pub fn register_controller(
    name: &str,
    ops: PeciCtrlOps,
    scan_mask: u32,
) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = PeciController {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        scan_mask,
    };
    PECI_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Scan for PECI devices on a controller (Linux `peci_controller_scan`).
pub fn scan_devices(ctrl_id: u32) -> Result<Vec<u32>, &'static str> {
    let (scan_mask, xfer_fn) = {
        let ctrls = PECI_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("PECI controller not found")?;
        (ctrl.scan_mask, ctrl.ops.xfer_cmd)
    };

    let mut found = Vec::new();
    for addr in 0..32u8 {
        if scan_mask & (1u32 << addr) == 0 {
            continue;
        }

        // Ping device by sending GetDIB (Device Info Block) command
        let cmd = PeciCmd {
            cmd_code: 0xF7, // PECI_GET_DIB
            host_id: 0,
            read_length: 8,
            write_data: Vec::new(),
        };

        match (xfer_fn)(ctrl_id, addr, &cmd) {
            Ok(dib) if dib.len() >= 8 => {
                let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                let dev = PeciDevice {
                    id: dev_id,
                    controller_id: ctrl_id,
                    addr,
                    name: alloc::format!("peci-{}", addr),
                    cpu_family: (dib[0] as u32) | ((dib[1] as u32) << 8),
                    cpu_model: dib[2] as u32,
                    cpu_stepping: dib[3] as u32,
                    core_mask: (dib[4] as u32)
                        | ((dib[5] as u32) << 8)
                        | ((dib[6] as u32) << 16)
                        | ((dib[7] as u32) << 24),
                    microcode_rev: 0,
                    driver_name: None,
                    bound: false,
                };
                PECI_DEVICES.write().insert(dev_id, dev);

                let mut ctrls = PECI_CTRLS.write();
                if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
                    ctrl.device_ids.push(dev_id);
                }
                found.push(dev_id);
                try_match_driver(dev_id)?;
            }
            _ => continue,
        }
    }
    Ok(found)
}

/// Send a PECI command to a device.
pub fn send_cmd(device_id: u32, cmd: &PeciCmd) -> Result<Vec<u8>, &'static str> {
    let (ctrl_id, addr, xfer_fn) = {
        let devices = PECI_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("PECI device not found")?;
        let ctrls = PECI_CTRLS.read();
        let ctrl = ctrls
            .get(&dev.controller_id)
            .ok_or("PECI controller not found")?;
        (dev.controller_id, dev.addr, ctrl.ops.xfer_cmd)
    };
    (xfer_fn)(ctrl_id, addr, cmd)
}

/// Get CPU temperature via PECI (Linux `peci_temp_read`).
pub fn get_cpu_temp(device_id: u32) -> Result<i32, &'static str> {
    let cmd = PeciCmd {
        cmd_code: 0x01, // PECI_PCS_TEMP
        host_id: 0,
        read_length: 4,
        write_data: {
            let mut d = Vec::new();
            d.push(0x01); // Index
            d
        },
    };
    let resp = send_cmd(device_id, &cmd)?;
    if resp.len() >= 2 {
        // Temperature in 1/64 degree Celsius
        let raw = (resp[0] as u16) | ((resp[1] as u16) << 8);
        Ok((raw as i32) / 64)
    } else {
        Err("PECI temperature read incomplete")
    }
}

/// Register a PECI driver.
pub fn register_driver(driver: PeciDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    PECI_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = PECI_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.bound
                    && id_table
                        .iter()
                        .any(|id| id.family == d.cpu_family && id.model == d.cpu_model)
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
        let devices = PECI_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let family = dev.cpu_family;
        let model = dev.cpu_model;

        let drivers = PECI_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.family == family && id.model == model {
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
        let mut devices = PECI_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all PECI controllers.
pub fn list_controllers() -> Vec<(u32, String, usize)> {
    PECI_CTRLS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.device_ids.len()))
        .collect()
}

/// List devices on a controller.
pub fn list_devices(ctrl_id: u32) -> Result<Vec<(u32, String, u8, u32, bool)>, &'static str> {
    let ctrls = PECI_CTRLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("PECI controller not found")?;
    let devices = PECI_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &ctrl.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((
                dev_id,
                dev.name.clone(),
                dev.addr,
                dev.cpu_family,
                dev.bound,
            ));
        }
    }
    Ok(result)
}

/// Count registered controllers.
pub fn controller_count() -> usize {
    PECI_CTRLS.read().len()
}

// ── Software PECI ───────────────────────────────────────────────────────

fn sw_xfer_cmd(_ctrl_id: u32, _addr: u8, _cmd: &PeciCmd) -> Result<Vec<u8>, &'static str> {
    Err("software PECI transport not available")
}
fn sw_xfer_msg(_ctrl_id: u32, _msg: &PeciMsg) -> Result<(), &'static str> {
    Err("software PECI transport not available")
}

/// Software PECI controller ops.
pub fn software_peci_ops() -> PeciCtrlOps {
    PeciCtrlOps {
        xfer_cmd: sw_xfer_cmd,
        xfer_msg: sw_xfer_msg,
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
    if !PECI_CTRLS.read().is_empty() {
        return Ok(());
    }

    let ops = software_peci_ops();
    let ctrl_id = register_controller("sw-peci", ops, 0x1)?;
    crate::serial_println!("peci: software controller registered (id={})", ctrl_id);
    Ok(())
}
