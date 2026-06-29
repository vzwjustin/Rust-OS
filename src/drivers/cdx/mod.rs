//! CDX bus subsystem
//!
//! Provides CDX bus for AMD FPGA-based accelerator device enumeration and management.
//! Mirrors Linux's `drivers/cdx/cdx.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CDX device (Linux `struct cdx_device`).
pub struct CdxDevice {
    pub id: u32,
    pub bus_num: u8,
    pub dev_num: u8,
    pub name: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u32,
    pub res_start: u64,
    pub res_end: u64,
    pub msi_count: u32,
    pub driver_name: Option<String>,
    pub bound: bool,
}

/// CDX driver (Linux `struct cdx_driver`).
pub struct CdxDriver {
    pub name: String,
    pub id_table: Vec<CdxDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
    pub shutdown: Option<fn(device_id: u32)>,
}

/// CDX device ID (Linux `struct cdx_device_id`).
#[derive(Debug, Clone)]
pub struct CdxDeviceId {
    pub vendor: u16,
    pub device: u16,
    pub subvendor: u16,
    pub subdevice: u16,
    pub class: u32,
    pub class_mask: u32,
}

/// CDX controller (Linux `struct cdx_controller`).
pub struct CdxController {
    pub id: u32,
    pub name: String,
    pub ops: CdxCtrlOps,
    pub bus_num: u8,
    pub device_ids: Vec<u32>,
}

/// CDX controller operations (Linux `struct cdx_controller_ops`).
pub struct CdxCtrlOps {
    pub scan: fn(ctrl_id: u32) -> Result<Vec<CdxDevInfo>, &'static str>,
    pub dev_reset: fn(ctrl_id: u32, bus_num: u8, dev_num: u8) -> Result<(), &'static str>,
    pub bus_enable: fn(ctrl_id: u32, bus_num: u8) -> Result<(), &'static str>,
    pub bus_disable: fn(ctrl_id: u32, bus_num: u8) -> Result<(), &'static str>,
}

/// CDX device info returned by scan.
#[derive(Debug, Clone)]
pub struct CdxDevInfo {
    pub bus_num: u8,
    pub dev_num: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u32,
    pub res_start: u64,
    pub res_end: u64,
    pub msi_count: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static CTRL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CDX_CTRLS: RwLock<BTreeMap<u32, CdxController>> = RwLock::new(BTreeMap::new());
static CDX_DEVICES: RwLock<BTreeMap<u32, CdxDevice>> = RwLock::new(BTreeMap::new());
static CDX_DRIVERS: RwLock<BTreeMap<u32, CdxDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a CDX controller.
pub fn register_controller(name: &str, ops: CdxCtrlOps, bus_num: u8) -> Result<u32, &'static str> {
    let id = CTRL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = CdxController {
        id,
        name: String::from(name),
        ops,
        bus_num,
        device_ids: Vec::new(),
    };
    CDX_CTRLS.write().insert(id, ctrl);
    Ok(id)
}

/// Scan a CDX bus for devices (Linux `cdx_scan_devices`).
pub fn scan_bus(ctrl_id: u32) -> Result<Vec<u32>, &'static str> {
    let (scan_fn, bus_num) = {
        let ctrls = CDX_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("CDX controller not found")?;
        (ctrl.ops.scan, ctrl.bus_num)
    };

    let dev_infos = (scan_fn)(ctrl_id)?;
    let mut registered = Vec::new();

    for info in dev_infos {
        let dev_id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dev = CdxDevice {
            id: dev_id,
            bus_num: info.bus_num,
            dev_num: info.dev_num,
            name: alloc::format!("cdx-{}-{}", info.bus_num, info.dev_num),
            vendor_id: info.vendor_id,
            device_id: info.device_id,
            class_code: info.class_code,
            res_start: info.res_start,
            res_end: info.res_end,
            msi_count: info.msi_count,
            driver_name: None,
            bound: false,
        };
        CDX_DEVICES.write().insert(dev_id, dev);

        let mut ctrls = CDX_CTRLS.write();
        if let Some(ctrl) = ctrls.get_mut(&ctrl_id) {
            ctrl.device_ids.push(dev_id);
        }
        registered.push(dev_id);
        try_match_driver(dev_id)?;
    }

    let _ = bus_num;
    Ok(registered)
}

/// Reset a CDX device.
pub fn reset_device(ctrl_id: u32, bus_num: u8, dev_num: u8) -> Result<(), &'static str> {
    let reset_fn = {
        let ctrls = CDX_CTRLS.read();
        let ctrl = ctrls.get(&ctrl_id).ok_or("CDX controller not found")?;
        ctrl.ops.dev_reset
    };
    (reset_fn)(ctrl_id, bus_num, dev_num)
}

/// Register a CDX driver.
pub fn register_driver(driver: CdxDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    CDX_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = CDX_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| {
                !d.bound
                    && id_table.iter().any(|id| {
                        (id.vendor == 0xFFFF || id.vendor == d.vendor_id)
                            && (id.device == 0xFFFF || id.device == d.device_id)
                            && (d.class_code & id.class_mask) == (id.class & id.class_mask)
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
        let devices = CDX_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let vid = dev.vendor_id;
        let did = dev.device_id;
        let cls = dev.class_code;

        let drivers = CDX_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if (id.vendor == 0xFFFF || id.vendor == vid)
                    && (id.device == 0xFFFF || id.device == did)
                    && (cls & id.class_mask) == (id.class & id.class_mask)
                {
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
        let mut devices = CDX_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// List all CDX controllers.
pub fn list_controllers() -> Vec<(u32, String, u8)> {
    CDX_CTRLS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.bus_num))
        .collect()
}

/// List devices on a controller.
pub fn list_devices(ctrl_id: u32) -> Result<Vec<(u32, String, u16, u16, bool)>, &'static str> {
    let ctrls = CDX_CTRLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("CDX controller not found")?;
    let devices = CDX_DEVICES.read();
    let mut result = Vec::new();
    for &dev_id in &ctrl.device_ids {
        if let Some(dev) = devices.get(&dev_id) {
            result.push((
                dev_id,
                dev.name.clone(),
                dev.vendor_id,
                dev.device_id,
                dev.bound,
            ));
        }
    }
    Ok(result)
}

/// Count registered devices.
pub fn device_count() -> usize {
    CDX_DEVICES.read().len()
}

// ── Software CDX ────────────────────────────────────────────────────────

fn sw_scan(_ctrl_id: u32) -> Result<Vec<CdxDevInfo>, &'static str> {
    let mut infos = Vec::new();
    infos.push(CdxDevInfo {
        bus_num: 0,
        dev_num: 0,
        vendor_id: 0x1022, // AMD
        device_id: 0x1001,
        class_code: 0x0B4000, // Processing accelerator
        res_start: 0xF0000000,
        res_end: 0xF000FFFF,
        msi_count: 4,
    });
    Ok(infos)
}
fn sw_dev_reset(_ctrl_id: u32, _bus: u8, _dev: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_bus_enable(_ctrl_id: u32, _bus: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_bus_disable(_ctrl_id: u32, _bus: u8) -> Result<(), &'static str> {
    Ok(())
}

/// Software CDX controller ops.
pub fn software_cdx_ops() -> CdxCtrlOps {
    CdxCtrlOps {
        scan: sw_scan,
        dev_reset: sw_dev_reset,
        bus_enable: sw_bus_enable,
        bus_disable: sw_bus_disable,
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
    crate::serial_println!("cdx: subsystem ready");
    Ok(())
}
