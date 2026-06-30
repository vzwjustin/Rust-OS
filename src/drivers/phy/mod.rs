//! PHY (Physical layer) subsystem
//!
//! Provides physical layer abstraction for USB, PCIe, SATA, Ethernet, and
//! MIPI interfaces with power on/off, reset, and calibration operations.
//! Mirrors Linux's `drivers/phy/phy-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PHY type (Linux `enum phy_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyType {
    Usb2,
    Usb3,
    Pcie,
    Sata,
    Ethernet,
    MipiDphy,
    MipiCphy,
    MipiDsi,
    Hdmi,
    DisplayPort,
    Sdio,
    Pcm,
    Ufs,
}

/// PHY mode (Linux `enum phy_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyMode {
    Invalid,
    Host,
    Device,
    Otp,
    HostUsb,
    DeviceUsb,
    Drp,
    Ota,
    Ufp,
    Dfp,
}

/// PHY operations (Linux `struct phy_ops`).
pub struct PhyOps {
    pub init: fn() -> Result<(), &'static str>,
    pub exit: fn() -> Result<(), &'static str>,
    pub power_on: fn() -> Result<(), &'static str>,
    pub power_off: fn() -> Result<(), &'static str>,
    pub set_mode: fn(mode: PhyMode) -> Result<(), &'static str>,
    pub reset: fn() -> Result<(), &'static str>,
    pub calibrate: fn() -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_type: fn() -> PhyType,
}

struct PhyProvider {
    id: u32,
    name: String,
    ops: &'static PhyOps,
    phy_type: PhyType,
    powered_on: bool,
    initialized: bool,
    mode: PhyMode,
    consumers: u32,
}

// ── USB2 PHY ────────────────────────────────────────────────────────────

static mut USB2_PHY_POWER: bool = false;
static mut USB2_PHY_MODE: PhyMode = PhyMode::Host;

fn usb2_init() -> Result<(), &'static str> {
    Ok(())
}
fn usb2_exit() -> Result<(), &'static str> {
    Ok(())
}

fn usb2_power_on() -> Result<(), &'static str> {
    unsafe {
        USB2_PHY_POWER = true;
    }
    Ok(())
}

fn usb2_power_off() -> Result<(), &'static str> {
    unsafe {
        USB2_PHY_POWER = false;
    }
    Ok(())
}

fn usb2_set_mode(mode: PhyMode) -> Result<(), &'static str> {
    unsafe {
        USB2_PHY_MODE = mode;
    }
    Ok(())
}

fn usb2_reset() -> Result<(), &'static str> {
    unsafe {
        USB2_PHY_POWER = false;
    }
    let mut i = 0u32;
    while i < 1000 {
        core::hint::spin_loop();
        i += 1;
    }
    unsafe {
        USB2_PHY_POWER = true;
    }
    Ok(())
}

fn usb2_calibrate() -> Result<(), &'static str> {
    Ok(())
}
fn usb2_name() -> &'static str {
    "usb2-phy"
}
fn usb2_type() -> PhyType {
    PhyType::Usb2
}

pub static USB2_PHY_OPS: PhyOps = PhyOps {
    init: usb2_init,
    exit: usb2_exit,
    power_on: usb2_power_on,
    power_off: usb2_power_off,
    set_mode: usb2_set_mode,
    reset: usb2_reset,
    calibrate: usb2_calibrate,
    get_name: usb2_name,
    get_type: usb2_type,
};

// ── PCIe PHY ────────────────────────────────────────────────────────────

static mut PCIE_PHY_POWER: bool = false;

fn pcie_init() -> Result<(), &'static str> {
    Ok(())
}
fn pcie_exit() -> Result<(), &'static str> {
    Ok(())
}
fn pcie_power_on() -> Result<(), &'static str> {
    unsafe {
        PCIE_PHY_POWER = true;
    }
    Ok(())
}
fn pcie_power_off() -> Result<(), &'static str> {
    unsafe {
        PCIE_PHY_POWER = false;
    }
    Ok(())
}
fn pcie_set_mode(_m: PhyMode) -> Result<(), &'static str> {
    Ok(())
}
fn pcie_reset() -> Result<(), &'static str> {
    unsafe {
        PCIE_PHY_POWER = false;
    }
    let mut i = 0u32;
    while i < 2000 {
        core::hint::spin_loop();
        i += 1;
    }
    unsafe {
        PCIE_PHY_POWER = true;
    }
    Ok(())
}
fn pcie_calibrate() -> Result<(), &'static str> {
    Ok(())
}
fn pcie_name() -> &'static str {
    "pcie-phy"
}
fn pcie_type() -> PhyType {
    PhyType::Pcie
}

pub static PCIE_PHY_OPS: PhyOps = PhyOps {
    init: pcie_init,
    exit: pcie_exit,
    power_on: pcie_power_on,
    power_off: pcie_power_off,
    set_mode: pcie_set_mode,
    reset: pcie_reset,
    calibrate: pcie_calibrate,
    get_name: pcie_name,
    get_type: pcie_type,
};

// ── Registry ────────────────────────────────────────────────────────────

static PHY_PROVIDERS: RwLock<BTreeMap<u32, PhyProvider>> = RwLock::new(BTreeMap::new());
static NEXT_PHY_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PHY provider (Linux `phy_create`).
pub fn register_provider(name: &str, ops: &'static PhyOps) -> Result<u32, &'static str> {
    let phy_type = (ops.get_type)();
    let id = NEXT_PHY_ID.fetch_add(1, Ordering::SeqCst);
    PHY_PROVIDERS.write().insert(
        id,
        PhyProvider {
            id,
            name: String::from(name),
            ops,
            phy_type,
            powered_on: false,
            initialized: false,
            mode: PhyMode::Invalid,
            consumers: 0,
        },
    );
    Ok(id)
}

/// Get a reference to a PHY (Linux `phy_get`).
pub fn get_phy(phy_id: u32) -> Result<(), &'static str> {
    let mut providers = PHY_PROVIDERS.write();
    let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
    phy.consumers += 1;
    Ok(())
}

/// Put (release) a PHY reference (Linux `phy_put`).
pub fn put_phy(phy_id: u32) -> Result<(), &'static str> {
    let mut providers = PHY_PROVIDERS.write();
    let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
    if phy.consumers > 0 {
        phy.consumers -= 1;
    }
    if phy.consumers == 0 && phy.powered_on {
        let _ = (phy.ops.power_off)();
        phy.powered_on = false;
    }
    Ok(())
}

/// Initialize a PHY (Linux `phy_init`).
pub fn init_phy(phy_id: u32) -> Result<(), &'static str> {
    let (ops, already_init) = {
        let mut providers = PHY_PROVIDERS.write();
        let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
        if phy.initialized {
            return Ok(());
        }
        (phy.ops, phy.initialized)
    };
    let _ = already_init;
    (ops.init)()?;
    let mut providers = PHY_PROVIDERS.write();
    if let Some(phy) = providers.get_mut(&phy_id) {
        phy.initialized = true;
    }
    Ok(())
}

/// Power on a PHY (Linux `phy_power_on`).
pub fn power_on(phy_id: u32) -> Result<(), &'static str> {
    let ops = {
        let mut providers = PHY_PROVIDERS.write();
        let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
        if !phy.initialized {
            return Err("PHY must be initialized before power on");
        }
        if phy.powered_on {
            return Ok(());
        }
        phy.ops
    };
    (ops.power_on)()?;
    let mut providers = PHY_PROVIDERS.write();
    if let Some(phy) = providers.get_mut(&phy_id) {
        phy.powered_on = true;
    }
    Ok(())
}

/// Power off a PHY (Linux `phy_power_off`).
pub fn power_off(phy_id: u32) -> Result<(), &'static str> {
    let ops = {
        let mut providers = PHY_PROVIDERS.write();
        let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
        if !phy.powered_on {
            return Ok(());
        }
        phy.ops
    };
    (ops.power_off)()?;
    let mut providers = PHY_PROVIDERS.write();
    if let Some(phy) = providers.get_mut(&phy_id) {
        phy.powered_on = false;
    }
    Ok(())
}

/// Set PHY mode (Linux `phy_set_mode`).
pub fn set_mode(phy_id: u32, mode: PhyMode) -> Result<(), &'static str> {
    let ops = {
        let mut providers = PHY_PROVIDERS.write();
        let phy = providers.get_mut(&phy_id).ok_or("PHY not found")?;
        phy.mode = mode;
        phy.ops
    };
    (ops.set_mode)(mode)
}

/// Reset a PHY (Linux `phy_reset`).
pub fn reset_phy(phy_id: u32) -> Result<(), &'static str> {
    let ops = {
        let providers = PHY_PROVIDERS.read();
        let phy = providers.get(&phy_id).ok_or("PHY not found")?;
        phy.ops
    };
    (ops.reset)()
}

/// Calibrate a PHY (Linux `phy_calibrate`).
pub fn calibrate(phy_id: u32) -> Result<(), &'static str> {
    let ops = {
        let providers = PHY_PROVIDERS.read();
        let phy = providers.get(&phy_id).ok_or("PHY not found")?;
        phy.ops
    };
    (ops.calibrate)()
}

/// Get PHY type.
pub fn get_type(phy_id: u32) -> Result<PhyType, &'static str> {
    let providers = PHY_PROVIDERS.read();
    let phy = providers.get(&phy_id).ok_or("PHY not found")?;
    Ok(phy.phy_type)
}

/// Number of registered PHY providers.
pub fn provider_count() -> usize {
    PHY_PROVIDERS.read().len()
}

/// Initialize PHY subsystem with USB2 and PCIe PHYs.
pub fn init() -> Result<(), &'static str> {
    if !PHY_PROVIDERS.read().is_empty() {
        return Ok(());
    }

    register_provider("usb2-phy", &USB2_PHY_OPS)?;
    register_provider("pcie-phy", &PCIE_PHY_OPS)?;

    crate::serial_println!("phy: {} provider(s) registered", provider_count());
    Ok(())
}
