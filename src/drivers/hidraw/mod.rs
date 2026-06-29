//! HID raw subsystem
//!
//! Provides raw HID device access for low-level report communication.
//! Mirrors Linux's `drivers/hid/hidraw.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// HID raw device (Linux `struct hidraw`).
pub struct HidrawDevice {
    pub id: u32,
    pub minor: u32,
    pub name: String,
    pub hid_dev_id: u32,
    pub vendor_id: u16,
    pub product_id: u16,
    pub report_desc: Vec<u8>,
    pub open_count: u32,
    pub output_buf: Vec<u8>,
    pub feature_buf: Vec<u8>,
}

/// HID raw operations.
pub struct HidrawOps {
    pub open: fn(dev_id: u32) -> Result<(), &'static str>,
    pub close: fn(dev_id: u32) -> Result<(), &'static str>,
    pub send_output_report: fn(dev_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub send_feature_report: fn(dev_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub get_feature_report: fn(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MINOR_COUNTER: AtomicU32 = AtomicU32::new(0);

static HIDRAW_DEVICES: RwLock<BTreeMap<u32, HidrawDevice>> = RwLock::new(BTreeMap::new());
static HIDRAW_OPS: RwLock<BTreeMap<u32, HidrawOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a HID raw device.
pub fn register_device(
    name: &str,
    hid_dev_id: u32,
    vendor_id: u16,
    product_id: u16,
    report_desc: Vec<u8>,
    ops: HidrawOps,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let minor = MINOR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = HidrawDevice {
        id,
        minor,
        name: String::from(name),
        hid_dev_id,
        vendor_id,
        product_id,
        report_desc,
        open_count: 0,
        output_buf: Vec::new(),
        feature_buf: Vec::new(),
    };
    HIDRAW_DEVICES.write().insert(id, dev);
    HIDRAW_OPS.write().insert(id, ops);
    Ok(id)
}

/// Open a HID raw device (Linux `hidraw_open`).
pub fn open_device(dev_id: u32) -> Result<(), &'static str> {
    let open_fn = {
        let ops = HIDRAW_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("HID raw ops not found")?;
        dev_ops.open
    };
    (open_fn)(dev_id)?;

    let mut devices = HIDRAW_DEVICES.write();
    if let Some(dev) = devices.get_mut(&dev_id) {
        dev.open_count += 1;
    }
    Ok(())
}

/// Close a HID raw device (Linux `hidraw_release`).
pub fn close_device(dev_id: u32) -> Result<(), &'static str> {
    let close_fn = {
        let ops = HIDRAW_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("HID raw ops not found")?;
        dev_ops.close
    };
    (close_fn)(dev_id)?;

    let mut devices = HIDRAW_DEVICES.write();
    if let Some(dev) = devices.get_mut(&dev_id) {
        if dev.open_count > 0 {
            dev.open_count -= 1;
        }
    }
    Ok(())
}

/// Send an output report (Linux `hidraw_write`).
pub fn write_output_report(dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let send_fn = {
        let ops = HIDRAW_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("HID raw ops not found")?;
        dev_ops.send_output_report
    };
    let n = (send_fn)(dev_id, data)?;

    let mut devices = HIDRAW_DEVICES.write();
    if let Some(dev) = devices.get_mut(&dev_id) {
        dev.output_buf.clear();
        dev.output_buf.extend_from_slice(data);
    }
    Ok(n)
}

/// Send a feature report (Linux `hidraw_send_feature_report`).
pub fn send_feature_report(dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let send_fn = {
        let ops = HIDRAW_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("HID raw ops not found")?;
        dev_ops.send_feature_report
    };
    let n = (send_fn)(dev_id, data)?;

    let mut devices = HIDRAW_DEVICES.write();
    if let Some(dev) = devices.get_mut(&dev_id) {
        dev.feature_buf.clear();
        dev.feature_buf.extend_from_slice(data);
    }
    Ok(n)
}

/// Get a feature report (Linux `hidraw_get_feature_report`).
pub fn get_feature_report(dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let get_fn = {
        let ops = HIDRAW_OPS.read();
        let dev_ops = ops.get(&dev_id).ok_or("HID raw ops not found")?;
        dev_ops.get_feature_report
    };
    (get_fn)(dev_id, buf)
}

/// Get the report descriptor for a device.
pub fn get_report_descriptor(dev_id: u32) -> Result<Vec<u8>, &'static str> {
    let devices = HIDRAW_DEVICES.read();
    let dev = devices.get(&dev_id).ok_or("HID raw device not found")?;
    Ok(dev.report_desc.clone())
}

/// List all HID raw devices.
pub fn list_devices() -> Vec<(u32, u32, String, u16, u16)> {
    HIDRAW_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.minor, d.name.clone(), d.vendor_id, d.product_id))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    HIDRAW_DEVICES.read().len()
}

// ── Software HID raw ────────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_close(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_output(_dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_send_feature(_dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_get_feature(_dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}

/// Software HID raw ops.
pub fn software_hidraw_ops() -> HidrawOps {
    HidrawOps {
        open: sw_open,
        close: sw_close,
        send_output_report: sw_send_output,
        send_feature_report: sw_send_feature,
        get_feature_report: sw_get_feature,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_hidraw_ops();
    // Standard USB HID keyboard report descriptor (simplified)
    let report_desc = [
        0x05, 0x01, // Usage Page (Generic Desktop)
        0x09, 0x06, // Usage (Keyboard)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x01, // Report ID (1)
        0x05, 0x07, // Usage Page (Keyboard)
        0x19, 0xE0, // Usage Minimum (Left Control)
        0x29, 0xE7, // Usage Maximum (Right GUI)
        0x15, 0x00, // Logical Minimum (0)
        0x25, 0x01, // Logical Maximum (1)
        0x75, 0x01, // Report Size (1)
        0x95, 0x08, // Report Count (8)
        0x81, 0x02, // Input (Data,Var,Abs)
        0xC0, // End Collection
    ];
    register_device(
        "sw-hidraw-keyboard",
        0,
        0x1234,
        0x5678,
        report_desc.to_vec(),
        ops,
    )?;

    Ok(())
}
