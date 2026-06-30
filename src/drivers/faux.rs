//! Faux (virtual / software-only) device bus.
//!
//! Mirrors Linux's `drivers/base/faux.c` (introduced in 6.10).  Provides a
//! lightweight virtual bus for software-only devices that have no real
//! hardware counterpart.
//!
//! Pure-Rust, no_std. No bindings:: calls.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::base::device::{BusType, Device};
use crate::drivers::base::driver::Driver;

// ── Faux bus type ─────────────────────────────────────────────────────────────

/// Static faux bus descriptor.
pub static FAUX_BUS: BusType = BusType {
    name: "faux",
    match_fn: None,
    probe: None,
    remove: None,
};

// ── Faux device ───────────────────────────────────────────────────────────────

/// A faux (virtual) device.
///
/// Wraps a base [`Device`] and registers it on the faux bus.
pub struct FauxDevice {
    pub base: Arc<Device>,
}

// ── Global faux device registry ───────────────────────────────────────────────

static FAUX_DEVICES: Mutex<Vec<Arc<FauxDevice>>> = Mutex::new(Vec::new());
static FAUX_DRIVERS: Mutex<Vec<Arc<dyn Driver>>> = Mutex::new(Vec::new());

impl FauxDevice {
    /// Create and register a new faux device with the given name.
    ///
    /// Returns `Ok(Arc<FauxDevice>)` on success or `Err(-12)` (ENOMEM).
    pub fn create(name: &str, parent: Option<Arc<Device>>) -> Result<Arc<Self>, i32> {
        let base = match parent {
            Some(p) => Device::new_with_parent(name, p, &FAUX_BUS),
            None => Device::new(name),
        };

        let faux = Arc::new(FauxDevice { base });
        FAUX_DEVICES.lock().push(faux.clone());

        // Try to probe any already-registered faux drivers.
        let drivers = FAUX_DRIVERS.lock();
        for drv in drivers.iter() {
            let _ = drv.probe(&faux.base);
        }

        Ok(faux)
    }

    /// Destroy a faux device, removing it from the registry.
    pub fn destroy(dev: Arc<FauxDevice>) {
        let ptr = Arc::as_ptr(&dev);
        FAUX_DEVICES.lock().retain(|d| Arc::as_ptr(d) != ptr);
        dev.base.unbind_driver();
    }
}

// ── Faux driver ───────────────────────────────────────────────────────────────

/// A registered faux driver handle.
pub struct FauxDriver {
    pub driver: Arc<dyn Driver>,
}

impl FauxDriver {
    /// Register a driver on the faux bus and probe any existing faux devices.
    pub fn register(driver: Arc<dyn Driver>) -> Result<Self, i32> {
        let mut drivers = FAUX_DRIVERS.lock();
        if drivers.iter().any(|d| d.name() == driver.name()) {
            return Err(-17); // EEXIST
        }
        drivers.push(driver.clone());
        drop(drivers);

        // Probe all existing faux devices.
        let devices = FAUX_DEVICES.lock();
        for dev in devices.iter() {
            let _ = driver.probe(&dev.base);
        }

        Ok(FauxDriver { driver })
    }

    /// Unregister this faux driver.
    pub fn unregister(self) {
        let name = self.driver.name();
        FAUX_DRIVERS.lock().retain(|d| d.name() != name);
    }
}
