//! Core device abstraction mirroring Linux's `struct device`.
//!
//! Pure-Rust, no_std implementation. No bindings:: dependencies.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::any::Any;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use spin::Mutex;

// ── Sysfs attribute ─────────────────────────────────────────────────────────

/// A single sysfs-style attribute (analogous to `struct device_attribute`).
pub struct SysfsAttr {
    pub name: &'static str,
    /// Unix permission bits (e.g. 0o444 read-only, 0o644 rw).
    pub mode: u16,
    pub show: Option<fn(&Device, &mut [u8]) -> usize>,
    pub store: Option<fn(&Device, &[u8]) -> usize>,
}

// ── Kobj type ───────────────────────────────────────────────────────────────

/// Object type metadata (analogous to `struct kobj_type`).
pub struct KobjType {
    pub release: Option<fn(&Device)>,
    pub sysfs_attrs: &'static [&'static SysfsAttr],
}

// ── Bus type ────────────────────────────────────────────────────────────────

/// Bus abstraction (analogous to `struct bus_type`).
pub struct BusType {
    pub name: &'static str,
    pub match_fn: Option<fn(&Device, &dyn crate::drivers::base::driver::Driver) -> bool>,
    pub probe: Option<fn(&Device) -> i32>,
    pub remove: Option<fn(&Device)>,
}

// ── Device power state ──────────────────────────────────────────────────────

/// Runtime power management state (analogous to `struct dev_pm_info`).
pub struct DevicePower {
    /// RPM_ACTIVE=0, RPM_IDLE=1, RPM_SUSPENDED=2, RPM_RESUMING=3, RPM_SUSPENDING=4
    pub runtime_status: AtomicU32,
    pub usage_count: AtomicI32,
    pub disable_depth: AtomicI32,
}

impl DevicePower {
    pub const RPM_ACTIVE: u32 = 0;
    pub const RPM_IDLE: u32 = 1;
    pub const RPM_SUSPENDED: u32 = 2;
    pub const RPM_RESUMING: u32 = 3;
    pub const RPM_SUSPENDING: u32 = 4;

    pub fn new() -> Self {
        Self {
            runtime_status: AtomicU32::new(Self::RPM_ACTIVE),
            usage_count: AtomicI32::new(0),
            disable_depth: AtomicI32::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.runtime_status.load(Ordering::Relaxed) == Self::RPM_ACTIVE
    }

    pub fn is_suspended(&self) -> bool {
        self.runtime_status.load(Ordering::Relaxed) == Self::RPM_SUSPENDED
    }
}

impl Default for DevicePower {
    fn default() -> Self {
        Self::new()
    }
}

// ── Device inner state ───────────────────────────────────────────────────────

struct DeviceInner {
    /// Type-erased driver private data (`dev_set_drvdata` / `dev_get_drvdata`).
    drvdata: Option<Box<dyn Any + Send>>,
}

// ── Device ───────────────────────────────────────────────────────────────────

/// Core device structure (analogous to Linux `struct device`).
///
/// Devices are reference-counted via `Arc<Device>`.  The `private` field holds
/// any driver-specific data attached with [`Device::set_drvdata`].
pub struct Device {
    /// Device name (e.g. "pci0000:00:02.0").
    name: Mutex<String>,
    /// Bus this device is on, if any.
    pub bus_type: Option<&'static BusType>,
    /// Currently bound driver.
    pub driver: Mutex<Option<Arc<dyn crate::drivers::base::driver::Driver>>>,
    /// Parent device in the device tree.
    pub parent: Option<Arc<Device>>,
    /// Object-type metadata for sysfs.
    pub kobj_type: Option<&'static KobjType>,
    /// Power management state.
    pub power: DevicePower,
    /// Reference count (tracked separately from Arc for Linux API compat).
    pub ref_count: AtomicU32,
    /// Mutable private state (driver data, devres list, etc.).
    inner: Mutex<DeviceInner>,
}

impl Device {
    /// Create a new device with the given name.
    pub fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: Mutex::new(name.to_string()),
            bus_type: None,
            driver: Mutex::new(None),
            parent: None,
            kobj_type: None,
            power: DevicePower::new(),
            ref_count: AtomicU32::new(1),
            inner: Mutex::new(DeviceInner { drvdata: None }),
        })
    }

    /// Create a device with parent and bus.
    pub fn new_with_parent(name: &str, parent: Arc<Device>, bus: &'static BusType) -> Arc<Self> {
        Arc::new(Self {
            name: Mutex::new(name.to_string()),
            bus_type: Some(bus),
            driver: Mutex::new(None),
            parent: Some(parent),
            kobj_type: None,
            power: DevicePower::new(),
            ref_count: AtomicU32::new(1),
            inner: Mutex::new(DeviceInner { drvdata: None }),
        })
    }

    /// Returns the device name.
    pub fn name(&self) -> String {
        self.name.lock().clone()
    }

    /// Sets the device name.
    pub fn set_name(&self, name: &str) {
        *self.name.lock() = name.to_string();
    }

    /// Returns a reference to the parent device.
    pub fn parent(&self) -> Option<&Arc<Device>> {
        self.parent.as_ref()
    }

    /// Attach typed driver data (`dev_set_drvdata`).
    pub fn set_drvdata<T: Any + Send + 'static>(&self, data: T) {
        self.inner.lock().drvdata = Some(Box::new(data));
    }

    /// Retrieve typed driver data (`dev_get_drvdata`).
    ///
    /// Returns `None` if no data is set or the type doesn't match.
    pub fn get_drvdata<T: Any + Send + 'static>(&self) -> Option<&T> {
        // SAFETY: We hold the Mutex for the duration of the downcast.
        // We return a raw pointer and re-borrow to extend lifetime to &self.
        let inner = self.inner.lock();
        inner
            .drvdata
            .as_ref()
            .and_then(|b| b.downcast_ref::<T>())
            // SAFETY: The Box is owned by self which has lifetime 'self,
            // so the pointer is valid as long as &self is valid.
            .map(|r| unsafe { &*(r as *const T) })
    }

    /// Remove and return driver data, leaving the slot empty.
    pub fn take_drvdata<T: Any + Send + 'static>(&self) -> Option<T> {
        let mut inner = self.inner.lock();
        let b = inner.drvdata.take()?;
        b.downcast::<T>().ok().map(|b| *b)
    }

    /// Bind a driver to this device.
    pub fn bind_driver(&self, driver: Arc<dyn crate::drivers::base::driver::Driver>) {
        *self.driver.lock() = Some(driver);
    }

    /// Unbind the current driver.
    pub fn unbind_driver(&self) {
        *self.driver.lock() = None;
    }

    /// Returns `true` if a driver is currently bound.
    pub fn has_driver(&self) -> bool {
        self.driver.lock().is_some()
    }

    /// Increment reference count (mirrors `get_device`).
    pub fn get(&self) {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement reference count (mirrors `put_device`).
    /// Returns `true` if the count reached zero.
    pub fn put(&self) -> bool {
        let prev = self.ref_count.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            // Trigger kobj_type release callback if set.
            if let Some(kt) = self.kobj_type {
                if let Some(release) = kt.release {
                    release(self);
                }
            }
            true
        } else {
            false
        }
    }
}
