//! Platform device and bus framework
//!
//! Provides platform bus registration for non-PCI/USB devices (ACPI-
//! enumerated, device-tree, or statically declared). Mirrors Linux's
//! `drivers/base/platform.c` with driver matching, probe/remove lifecycle,
//! and resource (IRQ, memory) management.

// Trait-based platform driver API (mirrors `include/linux/platform_device.h`).
pub mod traits;
pub use traits::{
    platform_device_register, platform_device_unregister, platform_driver_register,
    platform_driver_unregister, platform_get_irq, platform_get_resource, PlatformDeviceId,
    PlatformDriver, PlatformResourceFlags, TraitPlatformDevice, TraitPlatformResource,
};

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Resource type for platform devices (Linux `struct resource`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformResourceType {
    Memory,
    Irq,
    Dma,
    Io,
}

/// A platform device resource entry.
#[derive(Debug, Clone, Copy)]
pub struct PlatformResource {
    pub resource_type: PlatformResourceType,
    pub start: u64,
    pub end: u64,
    pub name: &'static str,
    pub flags: u32,
}

/// Operations implemented by a platform driver.
pub struct PlatformDriverOps {
    pub probe: fn(device_id: u32) -> Result<(), &'static str>,
    pub remove: fn(device_id: u32) -> Result<(), &'static str>,
    pub suspend: Option<fn(device_id: u32) -> Result<(), &'static str>>,
    pub resume: Option<fn(device_id: u32) -> Result<(), &'static str>>,
    pub get_name: fn() -> &'static str,
    pub match_device: fn(name: &str) -> bool,
}

struct PlatformDriverEntry {
    id: u32,
    name: String,
    ops: PlatformDriverOps,
    bound_device: Option<u32>,
}

struct PlatformDevice {
    id: u32,
    name: String,
    resources: Vec<PlatformResource>,
    driver_id: Option<u32>,
    probed: bool,
    platform_data: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static PLATFORM_DEVICES: RwLock<BTreeMap<u32, PlatformDevice>> = RwLock::new(BTreeMap::new());
static PLATFORM_DRIVERS: RwLock<BTreeMap<u32, PlatformDriverEntry>> = RwLock::new(BTreeMap::new());
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_DRIVER_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a platform device (Linux platform_device_register).
pub fn register_device(
    name: &str,
    resources: &[PlatformResource],
    platform_data: u64,
) -> Result<u32, &'static str> {
    let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    PLATFORM_DEVICES.write().insert(
        id,
        PlatformDevice {
            id,
            name: String::from(name),
            resources: resources.to_vec(),
            driver_id: None,
            probed: false,
            platform_data,
        },
    );
    Ok(id)
}

/// Register a platform driver (Linux platform_driver_register).
pub fn register_driver(name: &str, ops: PlatformDriverOps) -> Result<u32, &'static str> {
    let id = NEXT_DRIVER_ID.fetch_add(1, Ordering::SeqCst);
    PLATFORM_DRIVERS.write().insert(
        id,
        PlatformDriverEntry {
            id,
            name: String::from(name),
            ops,
            bound_device: None,
        },
    );
    Ok(id)
}

/// Attempt to bind a driver to a device (Linux driver_probe_device).
pub fn probe_device(device_id: u32) -> Result<(), &'static str> {
    let device_name = {
        let devices = PLATFORM_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Platform device not found")?;
        if dev.probed {
            return Ok(()); // Already probed
        }
        dev.name.clone()
    };

    // Find a matching driver.
    let driver_ids: Vec<u32> = PLATFORM_DRIVERS.read().keys().copied().collect();
    for driver_id in driver_ids {
        let matches = {
            let drivers = PLATFORM_DRIVERS.read();
            let driver = match drivers.get(&driver_id) {
                Some(d) => d,
                None => continue,
            };
            (driver.ops.match_device)(&device_name)
        };

        if matches {
            let probe_fn = {
                let drivers = PLATFORM_DRIVERS.read();
                let driver = drivers.get(&driver_id).ok_or("Driver vanished")?;
                driver.ops.probe
            };

            (probe_fn)(device_id)?;

            let mut devices = PLATFORM_DEVICES.write();
            if let Some(dev) = devices.get_mut(&device_id) {
                dev.driver_id = Some(driver_id);
                dev.probed = true;
            }
            let mut drivers = PLATFORM_DRIVERS.write();
            if let Some(drv) = drivers.get_mut(&driver_id) {
                drv.bound_device = Some(device_id);
            }
            return Ok(());
        }
    }

    Err("No matching platform driver found")
}

/// Remove a driver from a device (Linux device_release_driver).
pub fn remove_device(device_id: u32) -> Result<(), &'static str> {
    let driver_id = {
        let mut devices = PLATFORM_DEVICES.write();
        let dev = devices
            .get_mut(&device_id)
            .ok_or("Platform device not found")?;
        if !dev.probed {
            return Ok(());
        }
        dev.probed = false;
        dev.driver_id.take()
    };

    if let Some(did) = driver_id {
        let remove_fn = {
            let drivers = PLATFORM_DRIVERS.read();
            let driver = drivers.get(&did).ok_or("Driver not found")?;
            driver.ops.remove
        };
        (remove_fn)(device_id)?;

        let mut drivers = PLATFORM_DRIVERS.write();
        if let Some(drv) = drivers.get_mut(&did) {
            drv.bound_device = None;
        }
    }
    Ok(())
}

/// Get resources for a platform device (Linux platform_get_resource).
pub fn get_resource(
    device_id: u32,
    resource_type: PlatformResourceType,
    index: u32,
) -> Result<PlatformResource, &'static str> {
    let devices = PLATFORM_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Platform device not found")?;
    let mut count = 0u32;
    for res in &dev.resources {
        if res.resource_type == resource_type {
            if count == index {
                return Ok(*res);
            }
            count += 1;
        }
    }
    Err("Resource not found")
}

/// Get an IRQ resource for a device (Linux platform_get_irq).
pub fn get_irq(device_id: u32, index: u32) -> Result<u32, &'static str> {
    let res = get_resource(device_id, PlatformResourceType::Irq, index)?;
    Ok(res.start as u32)
}

/// Get platform data for a device.
pub fn get_platform_data(device_id: u32) -> Result<u64, &'static str> {
    let devices = PLATFORM_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Platform device not found")?;
    Ok(dev.platform_data)
}

/// Number of registered platform devices.
pub fn device_count() -> usize {
    PLATFORM_DEVICES.read().len()
}

/// Number of registered platform drivers.
pub fn driver_count() -> usize {
    PLATFORM_DRIVERS.read().len()
}

/// Number of probed (bound) devices.
pub fn probed_count() -> usize {
    PLATFORM_DEVICES
        .read()
        .values()
        .filter(|d| d.probed)
        .count()
}

/// Probe all unbound devices (Linux bus_type.probe).
pub fn probe_all() {
    let device_ids: Vec<u32> = PLATFORM_DEVICES
        .read()
        .iter()
        .filter(|(_, d)| !d.probed)
        .map(|(id, _)| *id)
        .collect();

    for dev_id in device_ids {
        if let Err(e) = probe_device(dev_id) {
            // Not all devices will have drivers; that's normal.
            let _ = e;
        }
    }
}

/// Initialize platform bus with ACPI-derived devices.
pub fn init() -> Result<(), &'static str> {
    if !PLATFORM_DEVICES.read().is_empty() {
        return Ok(());
    }

    // Register platform devices derived from ACPI availability.
    // On a real system these would come from ACPI DSDT/FADT enumeration.
    let rtc_resources = [PlatformResource {
        resource_type: PlatformResourceType::Io,
        start: 0x70,
        end: 0x71,
        name: "rtc-ports",
        flags: 0,
    }];
    register_device("rtc-cmos", &rtc_resources, 0)?;

    let pit_resources = [PlatformResource {
        resource_type: PlatformResourceType::Io,
        start: 0x40,
        end: 0x43,
        name: "pit-ports",
        flags: 0,
    }];
    register_device("pit-i8253", &pit_resources, 0)?;

    let kbd_resources = [PlatformResource {
        resource_type: PlatformResourceType::Io,
        start: 0x60,
        end: 0x64,
        name: "kbd-ports",
        flags: 0,
    }];
    register_device("i8042-kbd", &kbd_resources, 0)?;

    let com1_resources = [PlatformResource {
        resource_type: PlatformResourceType::Io,
        start: 0x3F8,
        end: 0x3FF,
        name: "com1-ports",
        flags: 0,
    }];
    register_device("serial-8250", &com1_resources, 0)?;

    crate::serial_println!("platform: {} device(s) registered", device_count());

    // Attempt to probe all devices.
    probe_all();

    crate::serial_println!("platform: {} device(s) probed", probed_count());

    Ok(())
}
