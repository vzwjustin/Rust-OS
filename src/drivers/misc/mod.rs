//! Misc device framework
//!
//! Provides simplified character device registration for miscellaneous
//! devices that don't warrant their own major number. Mirrors Linux's
//! `drivers/misc/` and `drivers/char/misc.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::drivers::char::{self, CharDeviceOps};

// ── Types ───────────────────────────────────────────────────────────────

/// Misc device registration info (Linux `struct miscdevice`).
pub struct MiscDevice {
    pub name: String,
    pub minor: u32,
    pub ops: &'static CharDeviceOps,
}

// ── Dynamic minor allocation ────────────────────────────────────────────

/// Misc device minor range (Linux: 0-255 dynamic, reserved below 64).
const MISC_DYNAMIC_MINOR_START: u32 = 64;
const MISC_DYNAMIC_MINOR_END: u32 = 255;

static MISC_DEVICES: RwLock<BTreeMap<u32, MiscDevice>> = RwLock::new(BTreeMap::new());
static NEXT_DYNAMIC_MINOR: AtomicU32 = AtomicU32::new(MISC_DYNAMIC_MINOR_START);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a misc device (Linux `misc_register`).
/// Uses the misc major (10) with either a specified or dynamically allocated minor.
pub fn register_device(
    name: &str,
    minor: Option<u32>,
    ops: &'static CharDeviceOps,
) -> Result<u32, &'static str> {
    let allocated_minor = match minor {
        Some(m) => {
            if MISC_DEVICES.read().contains_key(&m) {
                return Err("Misc minor number already in use");
            }
            m
        }
        None => {
            // Find next available dynamic minor.
            loop {
                let m = NEXT_DYNAMIC_MINOR.fetch_add(1, Ordering::SeqCst);
                if m > MISC_DYNAMIC_MINOR_END {
                    return Err("No available dynamic misc minor numbers");
                }
                if !MISC_DEVICES.read().contains_key(&m) {
                    break m;
                }
            }
        }
    };

    // Register with char device framework under misc major.
    char::register_device(char::MISC_MAJOR, name, 256, ops)?;

    MISC_DEVICES.write().insert(
        allocated_minor,
        MiscDevice {
            name: String::from(name),
            minor: allocated_minor,
            ops,
        },
    );

    Ok(allocated_minor)
}

/// Unregister a misc device (Linux `misc_deregister`).
pub fn unregister_device(minor: u32) -> Result<(), &'static str> {
    MISC_DEVICES
        .write()
        .remove(&minor)
        .ok_or("Misc device not found")?;
    Ok(())
}

/// Find a misc device by minor number.
pub fn get_device(minor: u32) -> Option<String> {
    MISC_DEVICES.read().get(&minor).map(|d| d.name.clone())
}

/// Get all registered misc devices.
pub fn get_all_devices() -> Vec<(u32, String)> {
    MISC_DEVICES
        .read()
        .iter()
        .map(|(minor, dev)| (*minor, dev.name.clone()))
        .collect()
}

/// Number of registered misc devices.
pub fn count() -> usize {
    MISC_DEVICES.read().len()
}

/// Initialize misc device framework.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("misc: framework ready ({} devices)", count());
    Ok(())
}
