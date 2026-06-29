//! External connector (extcon) subsystem
//!
//! Provides external connector state monitoring for USB, charger, HDMI,
//! and other cable types. Mirrors Linux's `drivers/extcon/extcon.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// External connector types (Linux `enum extcon_cable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExtconType {
    Usb,
    Charger,
    ChargerFast,
    ChargerSlow,
    Hdmi,
    Dvi,
    Vga,
    AudioJack,
    DisplayPort,
    UsbHost,
    UsbGadget,
    TypeC,
    TypeCUsb,
    TypeCDevice,
    TypeCPower,
}

impl ExtconType {
    pub fn name(self) -> &'static str {
        match self {
            ExtconType::Usb => "USB",
            ExtconType::Charger => "CHARGER",
            ExtconType::ChargerFast => "FAST-CHARGER",
            ExtconType::ChargerSlow => "SLOW-CHARGER",
            ExtconType::Hdmi => "HDMI",
            ExtconType::Dvi => "DVI",
            ExtconType::Vga => "VGA",
            ExtconType::AudioJack => "AUDIO-JACK",
            ExtconType::DisplayPort => "DP",
            ExtconType::UsbHost => "USB-HOST",
            ExtconType::UsbGadget => "USB-GADGET",
            ExtconType::TypeC => "TYPE-C",
            ExtconType::TypeCUsb => "TYPE-C-USB",
            ExtconType::TypeCDevice => "TYPE-C-DEVICE",
            ExtconType::TypeCPower => "TYPE-C-POWER",
        }
    }
}

/// Cable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableState {
    Disconnected,
    Connected,
}

/// Extcon device operations (Linux `struct extcon_dev`).
pub struct ExtconOps {
    pub get_state: fn(cable: ExtconType) -> CableState,
    pub set_state: fn(cable: ExtconType, state: CableState) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_supported: fn() -> Vec<ExtconType>,
}

struct ExtconDevice {
    id: u32,
    name: String,
    ops: &'static ExtconOps,
    supported: Vec<ExtconType>,
    states: BTreeMap<ExtconType, CableState>,
}

// ── Platform extcon (USB + charger) ─────────────────────────────────────

static mut PLAT_USB_STATE: CableState = CableState::Disconnected;
static mut PLAT_CHARGER_STATE: CableState = CableState::Connected;

fn plat_get_state(cable: ExtconType) -> CableState {
    match cable {
        ExtconType::Usb => unsafe { PLAT_USB_STATE },
        ExtconType::Charger => unsafe { PLAT_CHARGER_STATE },
        _ => CableState::Disconnected,
    }
}

fn plat_set_state(cable: ExtconType, state: CableState) -> Result<(), &'static str> {
    match cable {
        ExtconType::Usb => unsafe {
            PLAT_USB_STATE = state;
        },
        ExtconType::Charger => unsafe {
            PLAT_CHARGER_STATE = state;
        },
        _ => return Err("Unsupported cable type"),
    }
    Ok(())
}

fn plat_name() -> &'static str {
    "platform-extcon"
}

fn plat_supported() -> Vec<ExtconType> {
    let mut v = Vec::new();
    v.push(ExtconType::Usb);
    v.push(ExtconType::Charger);
    v.push(ExtconType::ChargerFast);
    v
}

pub static PLAT_EXTCON_OPS: ExtconOps = ExtconOps {
    get_state: plat_get_state,
    set_state: plat_set_state,
    get_name: plat_name,
    get_supported: plat_supported,
};

// ── Registry ────────────────────────────────────────────────────────────

static EXTCON_DEVICES: RwLock<BTreeMap<u32, ExtconDevice>> = RwLock::new(BTreeMap::new());
static NEXT_EXTCON_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an extcon device (Linux `extcon_dev_register`).
pub fn register_device(name: &str, ops: &'static ExtconOps) -> Result<u32, &'static str> {
    let supported = (ops.get_supported)();
    if supported.is_empty() {
        return Err("Extcon device must support at least one cable type");
    }

    // Initialize states.
    let mut states = BTreeMap::new();
    for cable in &supported {
        states.insert(*cable, (ops.get_state)(*cable));
    }

    let id = NEXT_EXTCON_ID.fetch_add(1, Ordering::SeqCst);
    EXTCON_DEVICES.write().insert(
        id,
        ExtconDevice {
            id,
            name: String::from(name),
            ops,
            supported,
            states,
        },
    );
    Ok(id)
}

/// Get cable state (Linux `extcon_get_state`).
pub fn get_state(device_id: u32, cable: ExtconType) -> Result<CableState, &'static str> {
    let devices = EXTCON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Extcon device not found")?;
    if !dev.supported.contains(&cable) {
        return Err("Cable type not supported by this device");
    }
    Ok((dev.ops.get_state)(cable))
}

/// Set cable state (Linux `extcon_set_state`).
pub fn set_state(device_id: u32, cable: ExtconType, state: CableState) -> Result<(), &'static str> {
    let ops = {
        let mut devices = EXTCON_DEVICES.write();
        let dev = devices
            .get_mut(&device_id)
            .ok_or("Extcon device not found")?;
        if !dev.supported.contains(&cable) {
            return Err("Cable type not supported by this device");
        }
        dev.states.insert(cable, state);
        dev.ops
    };
    (ops.set_state)(cable, state)
}

/// Get all supported cable types for a device.
pub fn get_supported(device_id: u32) -> Result<Vec<ExtconType>, &'static str> {
    let devices = EXTCON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Extcon device not found")?;
    Ok(dev.supported.clone())
}

/// Get all cable states for a device.
pub fn get_all_states(device_id: u32) -> Result<Vec<(ExtconType, CableState)>, &'static str> {
    let devices = EXTCON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Extcon device not found")?;
    Ok(dev.states.iter().map(|(t, s)| (*t, *s)).collect())
}

/// Check if any cable is connected.
pub fn is_any_connected(device_id: u32) -> Result<bool, &'static str> {
    let devices = EXTCON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Extcon device not found")?;
    Ok(dev.states.values().any(|s| *s == CableState::Connected))
}

/// Number of registered extcon devices.
pub fn device_count() -> usize {
    EXTCON_DEVICES.read().len()
}

/// Initialize extcon subsystem with platform device.
pub fn init() -> Result<(), &'static str> {
    if !EXTCON_DEVICES.read().is_empty() {
        return Ok(());
    }

    register_device("platform-extcon", &PLAT_EXTCON_OPS)?;

    crate::serial_println!(
        "extcon: platform device registered ({} cable types)",
        plat_supported().len()
    );
    Ok(())
}
