//! Driver trait and registration (analogous to Linux `struct device_driver`).
//!
//! Pure-Rust, no_std. No bindings:: calls.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::base::device::{BusType, Device};

// ── Power management state ───────────────────────────────────────────────────

/// Power-management state passed to `Driver::suspend`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PmState(pub u32);

impl PmState {
    pub const ON: Self = Self(0);
    pub const STANDBY: Self = Self(1);
    pub const MEM: Self = Self(3);
    pub const DISK: Self = Self(4);
}

// ── Device ID ────────────────────────────────────────────────────────────────

/// Generic device identifier entry (analogous to `struct pci_device_id`).
#[derive(Clone, Debug)]
pub struct DeviceId {
    pub vendor: u32,
    pub device: u32,
    pub subvendor: u32,
    pub subdevice: u32,
    pub class: u32,
    pub class_mask: u32,
    /// Driver-private cookie, typically an index into a private table.
    pub driver_data: u64,
}

impl DeviceId {
    /// Wildcard sentinel value (matches any vendor/device).
    pub const ANY: u32 = !0u32;

    pub const fn new(vendor: u32, device: u32) -> Self {
        Self {
            vendor,
            device,
            subvendor: Self::ANY,
            subdevice: Self::ANY,
            class: 0,
            class_mask: 0,
            driver_data: 0,
        }
    }
}

// ── Driver trait ─────────────────────────────────────────────────────────────

/// Core driver interface (analogous to `struct device_driver` + ops).
pub trait Driver: Send + Sync {
    /// Driver name (used for binding and sysfs).
    fn name(&self) -> &str;

    /// Called when a matching device is found (`->probe`).
    ///
    /// Returns 0 on success, negative errno on failure.
    fn probe(&self, dev: &Arc<Device>) -> i32;

    /// Called when a device is removed (`->remove`).
    fn remove(&self, dev: &Arc<Device>);

    /// Called during system shutdown, before power-off.
    fn shutdown(&self, _dev: &Arc<Device>) {}

    /// Suspend the device to `state`.  Returns 0 on success.
    fn suspend(&self, _dev: &Arc<Device>, _state: PmState) -> i32 {
        0
    }

    /// Resume from the given power state.  Returns 0 on success.
    fn resume(&self, _dev: &Arc<Device>) -> i32 {
        0
    }

    /// Optional static ID table for bus-level matching.
    fn id_table(&self) -> Option<&[DeviceId]> {
        None
    }
}

// ── Driver registration ──────────────────────────────────────────────────────

/// Registered driver handle.  Dropping this handle does NOT automatically
/// unregister — call [`DriverRegistration::unregister`] explicitly.
pub struct DriverRegistration {
    drivers: Arc<Mutex<Vec<Arc<dyn Driver>>>>,
    bus: &'static BusType,
    driver: Arc<dyn Driver>,
}

/// Global driver registry.
static DRIVER_REGISTRY: Mutex<Vec<Arc<dyn Driver>>> = Mutex::new(Vec::new());

impl DriverRegistration {
    /// Register `driver` on `bus`.
    ///
    /// Returns `Ok(DriverRegistration)` on success, `Err(errno)` on failure.
    pub fn register(driver: Arc<dyn Driver>, bus: &'static BusType) -> Result<Self, i32> {
        let mut registry = DRIVER_REGISTRY.lock();
        // Prevent duplicate registration.
        if registry.iter().any(|d| d.name() == driver.name()) {
            return Err(-17); // EEXIST
        }
        registry.push(driver.clone());
        drop(registry);

        Ok(Self {
            drivers: Arc::new(Mutex::new(Vec::new())),
            bus,
            driver,
        })
    }

    /// Unregister this driver, removing it from the global registry.
    pub fn unregister(&self) {
        let mut registry = DRIVER_REGISTRY.lock();
        let name = self.driver.name();
        registry.retain(|d| d.name() != name);
    }

    /// Returns the driver name.
    pub fn name(&self) -> &str {
        self.driver.name()
    }
}

// ── Registry helpers ─────────────────────────────────────────────────────────

/// Look up a registered driver by name.
pub fn find_driver(name: &str) -> Option<Arc<dyn Driver>> {
    DRIVER_REGISTRY
        .lock()
        .iter()
        .find(|d| d.name() == name)
        .cloned()
}

/// Iterate all registered drivers and call `f` for each.
pub fn for_each_driver<F: FnMut(&Arc<dyn Driver>)>(mut f: F) {
    for d in DRIVER_REGISTRY.lock().iter() {
        f(d);
    }
}

/// Returns the number of currently registered drivers.
pub fn driver_count() -> usize {
    DRIVER_REGISTRY.lock().len()
}
