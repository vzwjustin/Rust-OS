//! Unified device model (driver core)
//!
//! Provides the keystone driver core that other subsystems hang off of:
//! buses, classes, devices, and drivers, plus the match/bind/probe machinery
//! that ties them together. Mirrors Linux's `drivers/base/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// A device in the unified device model (Linux `struct device`).
pub struct Device {
    pub id: u32,
    pub name: String,
    /// Parent device id, forming the device hierarchy (root devices have None).
    pub parent: Option<u32>,
    /// Name of the bus this device sits on (matches a registered `Bus`).
    pub bus: String,
    /// Name of the class this device belongs to (matches a registered `Class`).
    pub class: String,
    /// Compatible / modalias string used to match against drivers.
    pub compatible: String,
    /// Id of the driver currently bound to this device, if any.
    pub bound_driver: Option<u32>,
    /// Key/value attributes standing in for sysfs entries.
    pub properties: BTreeMap<String, String>,
}

/// Probe/remove callbacks for a driver (Linux `struct device_driver` ops).
pub struct DeviceDriverOps {
    pub probe: fn(dev_id: u32) -> Result<(), &'static str>,
    pub remove: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// A device driver (Linux `struct device_driver`).
pub struct DeviceDriver {
    pub id: u32,
    pub name: String,
    /// Name of the bus this driver registers on.
    pub bus: String,
    /// Predicate matching a device's compatible/modalias string.
    pub matches: fn(compatible: &str) -> bool,
    pub ops: DeviceDriverOps,
}

/// A bus type (Linux `struct bus_type`).
pub struct Bus {
    pub id: u32,
    pub name: String,
    pub device_ids: Vec<u32>,
    pub driver_ids: Vec<u32>,
}

/// A device class (Linux `struct class`).
pub struct Class {
    pub id: u32,
    pub name: String,
    pub device_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUSES: RwLock<BTreeMap<u32, Bus>> = RwLock::new(BTreeMap::new());
static CLASSES: RwLock<BTreeMap<u32, Class>> = RwLock::new(BTreeMap::new());
static DEVICES: RwLock<BTreeMap<u32, Device>> = RwLock::new(BTreeMap::new());
static DRIVERS: RwLock<BTreeMap<u32, DeviceDriver>> = RwLock::new(BTreeMap::new());

static NEXT_BUS_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_CLASS_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_DRIVER_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API: registration ──────────────────────────────────────────────

/// Register a bus by name. Returns the existing id if a bus with that name
/// already exists, keeping registration idempotent.
pub fn register_bus(name: &str) -> Result<u32, &'static str> {
    if let Some(id) = bus_id_by_name(name) {
        return Ok(id);
    }
    let id = NEXT_BUS_ID.fetch_add(1, Ordering::SeqCst);
    BUSES.write().insert(
        id,
        Bus {
            id,
            name: String::from(name),
            device_ids: Vec::new(),
            driver_ids: Vec::new(),
        },
    );
    Ok(id)
}

/// Register a device class by name. Returns the existing id if present.
pub fn register_class(name: &str) -> Result<u32, &'static str> {
    if let Some(id) = class_id_by_name(name) {
        return Ok(id);
    }
    let id = NEXT_CLASS_ID.fetch_add(1, Ordering::SeqCst);
    CLASSES.write().insert(
        id,
        Class {
            id,
            name: String::from(name),
            device_ids: Vec::new(),
        },
    );
    Ok(id)
}

/// Register a device. The device is attached to its bus and class, then a bind
/// is attempted against every unbound driver already on its bus.
pub fn register_device(
    name: &str,
    parent: Option<u32>,
    bus: &str,
    class: &str,
    compatible: &str,
) -> Result<u32, &'static str> {
    if let Some(p) = parent {
        if !DEVICES.read().contains_key(&p) {
            return Err("base: parent device not found");
        }
    }

    let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    DEVICES.write().insert(
        id,
        Device {
            id,
            name: String::from(name),
            parent,
            bus: String::from(bus),
            class: String::from(class),
            compatible: String::from(compatible),
            bound_driver: None,
            properties: BTreeMap::new(),
        },
    );

    // Link into bus and class membership lists.
    if let Some(b) = BUSES.write().values_mut().find(|b| b.name == bus) {
        b.device_ids.push(id);
    }
    if let Some(c) = CLASSES.write().values_mut().find(|c| c.name == class) {
        c.device_ids.push(id);
    }

    try_bind_device(id);
    Ok(id)
}

/// Register a driver. The driver is attached to its bus, then a bind is
/// attempted against every existing unbound device on that bus.
pub fn register_driver(
    name: &str,
    bus: &str,
    matches: fn(&str) -> bool,
    ops: DeviceDriverOps,
) -> Result<u32, &'static str> {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    DRIVERS.write().insert(
        id,
        DeviceDriver {
            id,
            name: String::from(name),
            bus: String::from(bus),
            matches,
            ops,
        },
    );

    if let Some(b) = BUSES.write().values_mut().find(|b| b.name == bus) {
        b.driver_ids.push(id);
    }

    try_bind_driver(id);
    Ok(id)
}

// ── Public API: binding ─────────────────────────────────────────────────

/// Bind a device to a driver, calling the driver's probe op. The two must
/// share a bus and the driver's match predicate must accept the device.
pub fn bind(dev_id: u32, drv_id: u32) -> Result<(), &'static str> {
    let (dev_bus, dev_compat, already_bound) = {
        let devices = DEVICES.read();
        let dev = devices.get(&dev_id).ok_or("base: device not found")?;
        (dev.bus.clone(), dev.compatible.clone(), dev.bound_driver)
    };
    if already_bound.is_some() {
        return Err("base: device already bound");
    }

    let (drv_bus, matches, probe) = {
        let drivers = DRIVERS.read();
        let drv = drivers.get(&drv_id).ok_or("base: driver not found")?;
        (drv.bus.clone(), drv.matches, drv.ops.probe)
    };

    if dev_bus != drv_bus {
        return Err("base: device and driver on different buses");
    }
    if !(matches)(&dev_compat) {
        return Err("base: driver does not match device");
    }

    (probe)(dev_id)?;

    if let Some(dev) = DEVICES.write().get_mut(&dev_id) {
        dev.bound_driver = Some(drv_id);
    }
    Ok(())
}

/// Unbind a device from its driver, calling the driver's remove op.
pub fn unbind(dev_id: u32) -> Result<(), &'static str> {
    let drv_id = {
        let devices = DEVICES.read();
        let dev = devices.get(&dev_id).ok_or("base: device not found")?;
        dev.bound_driver.ok_or("base: device not bound")?
    };

    let remove = {
        let drivers = DRIVERS.read();
        let drv = drivers.get(&drv_id).ok_or("base: driver not found")?;
        drv.ops.remove
    };

    (remove)(dev_id)?;

    if let Some(dev) = DEVICES.write().get_mut(&dev_id) {
        dev.bound_driver = None;
    }
    Ok(())
}

/// Attempt to bind an unbound device against every driver on its bus.
fn try_bind_device(dev_id: u32) {
    let drv_ids: Vec<u32> = DRIVERS.read().keys().copied().collect();
    for drv_id in drv_ids {
        if DEVICES
            .read()
            .get(&dev_id)
            .map(|d| d.bound_driver)
            .unwrap_or(None)
            .is_some()
        {
            break;
        }
        let _ = bind(dev_id, drv_id);
    }
}

/// Attempt to bind a driver against every unbound device on its bus.
fn try_bind_driver(drv_id: u32) {
    let dev_ids: Vec<u32> = DEVICES.read().keys().copied().collect();
    for dev_id in dev_ids {
        let bound = DEVICES
            .read()
            .get(&dev_id)
            .map(|d| d.bound_driver)
            .unwrap_or(None);
        if bound.is_none() {
            let _ = bind(dev_id, drv_id);
        }
    }
}

// ── Public API: introspection ─────────────────────────────────────────────

/// Set a sysfs-style property on a device.
pub fn set_property(dev_id: u32, key: &str, value: &str) -> Result<(), &'static str> {
    let mut devices = DEVICES.write();
    let dev = devices.get_mut(&dev_id).ok_or("base: device not found")?;
    dev.properties
        .insert(String::from(key), String::from(value));
    Ok(())
}

/// Read a sysfs-style property from a device.
pub fn get_property(dev_id: u32, key: &str) -> Option<String> {
    DEVICES
        .read()
        .get(&dev_id)
        .and_then(|d| d.properties.get(key).cloned())
}

/// Build a "/sys/devices/.../name" style path by walking the parent chain.
pub fn device_path(dev_id: u32) -> Result<String, &'static str> {
    let devices = DEVICES.read();
    if !devices.contains_key(&dev_id) {
        return Err("base: device not found");
    }

    // Collect names from the device up to the root.
    let mut names: Vec<String> = Vec::new();
    let mut current = Some(dev_id);
    while let Some(id) = current {
        let dev = devices.get(&id).ok_or("base: dangling parent reference")?;
        names.push(dev.name.clone());
        current = dev.parent;
    }

    let mut path = String::from("/sys/devices");
    for name in names.iter().rev() {
        path.push('/');
        path.push_str(name);
    }
    Ok(path)
}

/// Return the id of the driver bound to a device, if any.
pub fn bound_driver(dev_id: u32) -> Option<u32> {
    DEVICES.read().get(&dev_id).and_then(|d| d.bound_driver)
}

fn bus_id_by_name(name: &str) -> Option<u32> {
    BUSES.read().values().find(|b| b.name == name).map(|b| b.id)
}

fn class_id_by_name(name: &str) -> Option<u32> {
    CLASSES
        .read()
        .values()
        .find(|c| c.name == name)
        .map(|c| c.id)
}

/// Count registered buses.
pub fn bus_count() -> usize {
    BUSES.read().len()
}

/// Count registered classes.
pub fn class_count() -> usize {
    CLASSES.read().len()
}

/// Count registered devices.
pub fn device_count() -> usize {
    DEVICES.read().len()
}

/// Count registered drivers.
pub fn driver_count() -> usize {
    DRIVERS.read().len()
}

// ── Public API: integration helpers ───────────────────────────────────────

/// Convenience for subsystems publishing a device into the unified model:
/// ensures `bus` and a generic "device" class exist (create-or-ignore), then
/// registers a parentless device with the given `compatible`/modalias string.
///
/// Safe to call before [`init`]: because [`register_bus`]/[`register_class`]
/// are idempotent, a subsystem whose own init runs ahead of [`init`] still
/// links its device into a real bus. Returns the new device id.
pub fn register_device_simple(
    bus: &str,
    name: &str,
    compatible: &str,
) -> Result<u32, &'static str> {
    register_bus(bus)?;
    register_class("device")?;
    register_device(name, None, bus, "device", compatible)
}

/// List the names of every device currently registered on `bus`.
pub fn list_devices_on_bus(bus: &str) -> Vec<String> {
    DEVICES
        .read()
        .values()
        .filter(|d| d.bus == bus)
        .map(|d| d.name.clone())
        .collect()
}

/// Whether a device with the given name is registered.
pub fn device_exists(name: &str) -> bool {
    DEVICES.read().values().any(|d| d.name == name)
}

// ── Sample virtual driver ─────────────────────────────────────────────────

fn virtual_driver_matches(compatible: &str) -> bool {
    compatible == "virtual,sample-device"
}

fn virtual_driver_probe(dev_id: u32) -> Result<(), &'static str> {
    set_property(dev_id, "driver", "virtual-sample")?;
    set_property(dev_id, "state", "online")?;
    Ok(())
}

fn virtual_driver_remove(dev_id: u32) -> Result<(), &'static str> {
    set_property(dev_id, "state", "offline")?;
    Ok(())
}

const VIRTUAL_DRIVER_OPS: DeviceDriverOps = DeviceDriverOps {
    probe: virtual_driver_probe,
    remove: virtual_driver_remove,
};

// ── Init ────────────────────────────────────────────────────────────────

/// Initialize the unified device model with a platform bus, a virtual class,
/// and a sample device/driver pair bound together.
pub fn init() -> Result<(), &'static str> {
    // Standard buses that subsystems publish their devices onto. register_bus
    // is idempotent, so this is safe to run repeatedly and regardless of
    // whether a subsystem created one of these buses earlier in boot.
    for bus in ["pci", "platform", "input", "block", "scsi", "nvme", "net"] {
        register_bus(bus)?;
    }
    register_class("virtual")?;

    // Wire up the sample device/driver pair exactly once. Guarding on the
    // device (rather than an "any bus exists" check) keeps init idempotent even
    // when another subsystem created one of the standard buses first.
    if !device_exists("sample0") {
        let dev_id = register_device(
            "sample0",
            None,
            "platform",
            "virtual",
            "virtual,sample-device",
        )?;

        let drv_id = register_driver(
            "virtual-sample",
            "platform",
            virtual_driver_matches,
            VIRTUAL_DRIVER_OPS,
        )?;

        // register_driver already auto-binds; bind explicitly if it did not
        // (e.g. ordering), so the sample pair is always wired up.
        if bound_driver(dev_id).is_none() {
            bind(dev_id, drv_id)?;
        }
    }

    crate::serial_println!(
        "base: device model ready, {} buses, {} devices, {} drivers",
        bus_count(),
        device_count(),
        driver_count()
    );
    Ok(())
}
