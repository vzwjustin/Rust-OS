//! Trait-based platform driver framework.
//!
//! Extends the existing platform bus with a Linux-kernel-compatible
//! `PlatformDriver` trait and `PlatformDeviceId` table, mirroring
//! `include/linux/platform_device.h`.
//!
//! Pure-Rust, no_std. No bindings:: calls.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use super::{PlatformDevice as LegacyPlatformDevice, PlatformResource, PlatformResourceType};
use crate::drivers::base::device::Device;

// ── Resource flags (bitflag shim) ────────────────────────────────────────────

/// Flags describing the type of a platform resource (mirrors Linux `IORESOURCE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformResourceFlags(pub u32);

impl PlatformResourceFlags {
    pub const MEM: Self = Self(0x0000_0200);
    pub const IO: Self = Self(0x0000_0100);
    pub const IRQ: Self = Self(0x0000_0400);
    pub const DMA: Self = Self(0x0000_0800);

    pub fn contains(&self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

// ── Platform device (enhanced public view) ───────────────────────────────────

/// An enhanced platform device that wraps a [`Device`] and carries typed
/// resources.  Created by [`platform_device_register`].
pub struct TraitPlatformDevice {
    pub base: Arc<Device>,
    pub id: i32,
    pub name: String,
    pub resources: Vec<TraitPlatformResource>,
}

/// A single platform resource with bitflag-style type.
pub struct TraitPlatformResource {
    pub start: u64,
    pub end: u64,
    pub name: Option<String>,
    pub flags: PlatformResourceFlags,
}

impl TraitPlatformResource {
    /// Byte size of the resource window.
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }
}

// ── Platform device ID ───────────────────────────────────────────────────────

/// Entry in a platform driver's match table.
#[derive(Clone, Debug)]
pub struct PlatformDeviceId {
    pub name: &'static str,
    pub driver_data: u64,
}

// ── Platform driver trait ────────────────────────────────────────────────────

/// Trait implemented by platform drivers (analogous to `struct platform_driver`).
pub trait PlatformDriver: Send + Sync {
    /// Driver name.
    fn name(&self) -> &'static str;

    /// Called when a matching device is found.  Returns 0 on success.
    fn probe(&self, dev: &TraitPlatformDevice) -> i32;

    /// Called when a device is removed.
    fn remove(&self, dev: &TraitPlatformDevice);

    /// Optional static ID table.  Used by the bus for matching.
    fn id_table(&self) -> Option<&'static [PlatformDeviceId]> {
        None
    }
}

// ── Global registries ────────────────────────────────────────────────────────

static TRAIT_PLATFORM_DRIVERS: Mutex<Vec<Arc<dyn PlatformDriver>>> = Mutex::new(Vec::new());
static TRAIT_PLATFORM_DEVICES: Mutex<Vec<Arc<TraitPlatformDevice>>> = Mutex::new(Vec::new());

// ── Registration API ─────────────────────────────────────────────────────────

/// Register a platform driver and attempt to probe any matching devices.
///
/// Returns 0 on success, `-17` (EEXIST) if already registered.
pub fn platform_driver_register(drv: Arc<dyn PlatformDriver>) -> i32 {
    let mut drivers = TRAIT_PLATFORM_DRIVERS.lock();
    if drivers.iter().any(|d| d.name() == drv.name()) {
        return -17; // EEXIST
    }
    drivers.push(drv.clone());
    drop(drivers);

    // Try to probe existing devices.
    let devices = TRAIT_PLATFORM_DEVICES.lock();
    for dev in devices.iter() {
        if device_matches_driver(dev, &*drv) {
            let _ = drv.probe(dev);
        }
    }
    0
}

/// Unregister a platform driver.
pub fn platform_driver_unregister(drv: &Arc<dyn PlatformDriver>) {
    let mut drivers = TRAIT_PLATFORM_DRIVERS.lock();
    let name = drv.name();
    drivers.retain(|d| d.name() != name);
}

/// Register a new platform device.
///
/// Returns 0 on success.
pub fn platform_device_register(dev: TraitPlatformDevice) -> i32 {
    let dev = Arc::new(dev);
    let mut devices = TRAIT_PLATFORM_DEVICES.lock();
    devices.push(dev.clone());
    drop(devices);

    // Try existing drivers.
    let drivers = TRAIT_PLATFORM_DRIVERS.lock();
    for drv in drivers.iter() {
        if device_matches_driver(&dev, &**drv) {
            let _ = drv.probe(&dev);
            break;
        }
    }
    0
}

/// Unregister a platform device by name.
pub fn platform_device_unregister(dev: &TraitPlatformDevice) {
    let name = dev.name.as_str();
    let mut devices = TRAIT_PLATFORM_DEVICES.lock();
    devices.retain(|d| d.name.as_str() != name);
}

/// Get the `num`-th resource of the given type flags from a device.
pub fn platform_get_resource(
    dev: &TraitPlatformDevice,
    rtype: PlatformResourceFlags,
    num: u32,
) -> Option<&TraitPlatformResource> {
    dev.resources
        .iter()
        .filter(|r| r.flags.contains(rtype))
        .nth(num as usize)
}

/// Get the `num`-th IRQ for a device.
///
/// Returns the IRQ start address or a negative errno.
pub fn platform_get_irq(dev: &TraitPlatformDevice, num: u32) -> i32 {
    match platform_get_resource(dev, PlatformResourceFlags::IRQ, num) {
        Some(r) => r.start as i32,
        None => -22, // EINVAL
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn device_matches_driver(dev: &TraitPlatformDevice, drv: &dyn PlatformDriver) -> bool {
    // 1. Match via ID table if present.
    if let Some(table) = drv.id_table() {
        for entry in table {
            if dev.name == entry.name {
                return true;
            }
        }
        return false;
    }
    // 2. Fallback: name equality.
    dev.name == drv.name()
}
