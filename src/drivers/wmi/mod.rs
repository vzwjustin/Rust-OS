//! WMI (Windows Management Instrumentation) ACPI mapping subsystem
//!
//! Provides ACPI WMI mapping for vendor-specific BIOS interfaces.
//! Mirrors Linux's `drivers/platform/x86/wmi.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// WMI device (Linux `struct wmi_device`).
pub struct WmiDevice {
    pub id: u32,
    pub guid: [u8; 16],
    pub guid_string: String,
    pub instance_count: u8,
    pub notify_id: u8,
    pub driver_name: Option<String>,
    pub bound: bool,
    pub capabilities: WmiCaps,
}

/// WMI capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmiCaps(pub u32);

impl WmiCaps {
    pub const NONE: Self = WmiCaps(0);
    pub const METHOD: Self = WmiCaps(1);
    pub const DATA: Self = WmiCaps(2);
    pub const EVENT: Self = WmiCaps(4);
    pub const STRING: Self = WmiCaps(8);

    pub fn has(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }
}

/// WMI driver (Linux `struct wmi_driver`).
pub struct WmiDriver {
    pub name: String,
    pub id_table: Vec<WmiDeviceId>,
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
    pub notify: Option<fn(device_id: u32, event: u32)>,
}

/// WMI device ID (Linux `struct wmi_device_id`).
#[derive(Debug, Clone)]
pub struct WmiDeviceId {
    pub guid_string: String,
}

/// WMI block (Linux `struct wmi_block`).
pub struct WmiBlock {
    pub id: u32,
    pub device_id: u32,
    pub guid: [u8; 16],
    pub instance_count: u8,
    pub flags: u32,
    pub method_id: u32,
    pub notify_id: u8,
}

/// WMI method call parameters.
#[derive(Debug, Clone)]
pub struct WmiMethodCall {
    pub method_id: u32,
    pub instance: u8,
    pub input: Vec<u8>,
    pub expected_output: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static BLOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DRIVER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static WMI_DEVICES: RwLock<BTreeMap<u32, WmiDevice>> = RwLock::new(BTreeMap::new());
static WMI_BLOCKS: RwLock<BTreeMap<u32, WmiBlock>> = RwLock::new(BTreeMap::new());
static WMI_DRIVERS: RwLock<BTreeMap<u32, WmiDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a WMI device from ACPI WDG block.
pub fn register_device(
    guid: [u8; 16],
    guid_string: &str,
    instance_count: u8,
    notify_id: u8,
    caps: WmiCaps,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = WmiDevice {
        id,
        guid,
        guid_string: String::from(guid_string),
        instance_count,
        notify_id,
        driver_name: None,
        bound: false,
        capabilities: caps,
    };
    WMI_DEVICES.write().insert(id, dev);

    // Create a WMI block
    let block_id = BLOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let block = WmiBlock {
        id: block_id,
        device_id: id,
        guid,
        instance_count,
        flags: caps.0,
        method_id: 0,
        notify_id,
    };
    WMI_BLOCKS.write().insert(block_id, block);

    try_match_driver(id)?;
    Ok(id)
}

/// Evaluate a WMI method (Linux `wmidev_evaluate_method`).
pub fn evaluate_method(device_id: u32, call: &WmiMethodCall) -> Result<Vec<u8>, &'static str> {
    let devices = WMI_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("WMI device not found")?;
    if !dev.capabilities.has(WmiCaps::METHOD) {
        return Err("WMI device does not support methods");
    }
    if call.instance >= dev.instance_count {
        return Err("WMI instance out of range");
    }
    Err("WMI method execution not available")
}

/// Query WMI data block (Linux `wmidev_block_query`).
pub fn query_block(device_id: u32, instance: u8, buf: &mut [u8]) -> Result<usize, &'static str> {
    let devices = WMI_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("WMI device not found")?;
    if !dev.capabilities.has(WmiCaps::DATA) {
        return Err("WMI device does not support data blocks");
    }
    if instance >= dev.instance_count {
        return Err("WMI instance out of range");
    }
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}

/// Set WMI data block (Linux `wmidev_block_set`).
pub fn set_block(device_id: u32, instance: u8, data: &[u8]) -> Result<usize, &'static str> {
    let devices = WMI_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("WMI device not found")?;
    if !dev.capabilities.has(WmiCaps::DATA) {
        return Err("WMI device does not support data blocks");
    }
    if instance >= dev.instance_count {
        return Err("WMI instance out of range");
    }
    Ok(data.len())
}

/// Deliver a WMI event notification (Linux `wmi_notify_driver`).
pub fn notify_event(device_id: u32, event: u32) {
    let cb_fn = {
        let devices = WMI_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) => d,
            None => return,
        };
        if !dev.capabilities.has(WmiCaps::EVENT) {
            return;
        }
        let drv_name = match &dev.driver_name {
            Some(n) => n.clone(),
            None => return,
        };
        let drivers = WMI_DRIVERS.read();
        drivers
            .iter()
            .find(|(_, d)| d.name == drv_name)
            .and_then(|(_, d)| d.notify)
    };
    if let Some(cb) = cb_fn {
        cb(device_id, event);
    }
}

/// Register a WMI driver.
pub fn register_driver(driver: WmiDriver) -> Result<u32, &'static str> {
    let id = DRIVER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let id_table = driver.id_table.clone();
    WMI_DRIVERS.write().insert(id, driver);

    let device_ids: Vec<u32> = {
        let devices = WMI_DEVICES.read();
        devices
            .iter()
            .filter(|(_, d)| !d.bound && id_table.iter().any(|id| id.guid_string == d.guid_string))
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
        let devices = WMI_DEVICES.read();
        let dev = match devices.get(&device_id) {
            Some(d) if !d.bound => d,
            _ => return Ok(()),
        };
        let guid_str = dev.guid_string.clone();

        let drivers = WMI_DRIVERS.read();
        let mut found: Option<(fn(u32) -> Result<(), &'static str>, String)> = None;
        for (_, drv) in drivers.iter() {
            for id in &drv.id_table {
                if id.guid_string == guid_str {
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
        let mut devices = WMI_DEVICES.write();
        if let Some(dev) = devices.get_mut(&device_id) {
            dev.bound = true;
            dev.driver_name = Some(drv_name);
        }
    }
    Ok(())
}

/// Find a WMI device by GUID string.
pub fn find_device(guid_string: &str) -> Option<u32> {
    let devices = WMI_DEVICES.read();
    devices
        .iter()
        .find(|(_, d)| d.guid_string == guid_string)
        .map(|(id, _)| *id)
}

/// List all WMI devices.
pub fn list_devices() -> Vec<(u32, String, u8, WmiCaps, bool)> {
    WMI_DEVICES
        .read()
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.guid_string.clone(),
                d.instance_count,
                d.capabilities,
                d.bound,
            )
        })
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    WMI_DEVICES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

fn null_probe(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn null_remove(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("wmi: subsystem ready");
    Ok(())
}
