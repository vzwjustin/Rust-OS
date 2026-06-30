//! Backlight subsystem
//!
//! Provides display backlight brightness control with platform driver
//! registration. Mirrors Linux's `drivers/video/backlight/backlight.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Backlight power state (Linux `enum backlight_power`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklightPower {
    On,
    Off,
    Suspend,
    Unblank,
    Blank,
}

/// Backlight operations (Linux `struct backlight_ops`).
pub struct BacklightOps {
    pub update_status: fn(brightness: u8, power: BacklightPower) -> Result<(), &'static str>,
    pub get_brightness: fn() -> u8,
    pub get_name: fn() -> &'static str,
    pub get_max_brightness: fn() -> u8,
}

struct BacklightDevice {
    id: u32,
    name: String,
    ops: &'static BacklightOps,
    brightness: u8,
    max_brightness: u8,
    power: BacklightPower,
}

// ── Software backlight (in-memory state) ────────────────────────────────

static mut SW_BRIGHTNESS: u8 = 128;
static mut SW_POWER: BacklightPower = BacklightPower::On;

fn sw_update(brightness: u8, power: BacklightPower) -> Result<(), &'static str> {
    unsafe {
        SW_BRIGHTNESS = brightness;
        SW_POWER = power;
    }
    Ok(())
}

fn sw_get_brightness() -> u8 {
    unsafe { SW_BRIGHTNESS }
}

fn sw_name() -> &'static str {
    "software-backlight"
}
fn sw_max() -> u8 {
    255
}

pub static SW_BACKLIGHT_OPS: BacklightOps = BacklightOps {
    update_status: sw_update,
    get_brightness: sw_get_brightness,
    get_name: sw_name,
    get_max_brightness: sw_max,
};

// ── Registry ────────────────────────────────────────────────────────────

static BACKLIGHTS: RwLock<BTreeMap<u32, BacklightDevice>> = RwLock::new(BTreeMap::new());
static NEXT_BL_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a backlight device (Linux `backlight_device_register`).
pub fn register_backlight(name: &str, ops: &'static BacklightOps) -> Result<u32, &'static str> {
    let max = (ops.get_max_brightness)();
    let current = (ops.get_brightness)();
    let id = NEXT_BL_ID.fetch_add(1, Ordering::SeqCst);
    BACKLIGHTS.write().insert(
        id,
        BacklightDevice {
            id,
            name: String::from(name),
            ops,
            brightness: current,
            max_brightness: max,
            power: BacklightPower::On,
        },
    );
    Ok(id)
}

/// Set backlight brightness (Linux `backlight_update_status`).
pub fn set_brightness(bl_id: u32, brightness: u8) -> Result<(), &'static str> {
    let (ops, max) = {
        let bls = BACKLIGHTS.read();
        let bl = bls.get(&bl_id).ok_or("Backlight not found")?;
        (bl.ops, bl.max_brightness)
    };
    let clamped = brightness.min(max);
    (ops.update_status)(clamped, BacklightPower::On)?;
    let mut bls = BACKLIGHTS.write();
    if let Some(bl) = bls.get_mut(&bl_id) {
        bl.brightness = clamped;
        bl.power = BacklightPower::On;
    }
    Ok(())
}

/// Get current brightness (Linux `backlight_get_brightness`).
pub fn get_brightness(bl_id: u32) -> Result<u8, &'static str> {
    let bls = BACKLIGHTS.read();
    let bl = bls.get(&bl_id).ok_or("Backlight not found")?;
    Ok(bl.brightness)
}

/// Set power state (Linux `backlight_power_off/on`).
pub fn set_power(bl_id: u32, power: BacklightPower) -> Result<(), &'static str> {
    let ops = {
        let bls = BACKLIGHTS.read();
        let bl = bls.get(&bl_id).ok_or("Backlight not found")?;
        bl.ops
    };
    let brightness = match power {
        BacklightPower::On | BacklightPower::Unblank => {
            let bls = BACKLIGHTS.read();
            bls.get(&bl_id).map_or(0, |bl| bl.brightness)
        }
        _ => 0,
    };
    (ops.update_status)(brightness, power)?;
    let mut bls = BACKLIGHTS.write();
    if let Some(bl) = bls.get_mut(&bl_id) {
        bl.power = power;
    }
    Ok(())
}

/// Get max brightness.
pub fn get_max_brightness(bl_id: u32) -> Result<u8, &'static str> {
    let bls = BACKLIGHTS.read();
    let bl = bls.get(&bl_id).ok_or("Backlight not found")?;
    Ok(bl.max_brightness)
}

/// Find backlight by name.
pub fn find_by_name(name: &str) -> Option<u32> {
    BACKLIGHTS
        .read()
        .iter()
        .find(|(_, bl)| bl.name == name)
        .map(|(id, _)| *id)
}

/// Number of registered backlights.
pub fn count() -> usize {
    BACKLIGHTS.read().len()
}

/// Initialize backlight subsystem with software device.
pub fn init() -> Result<(), &'static str> {
    if !BACKLIGHTS.read().is_empty() {
        return Ok(());
    }

    let bl_id = register_backlight("software-backlight", &SW_BACKLIGHT_OPS)?;
    crate::serial_println!(
        "backlight: software device registered (id={}, max=255)",
        bl_id
    );
    Ok(())
}
