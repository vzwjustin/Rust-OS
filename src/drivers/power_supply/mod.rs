//! Power supply class
//!
//! Provides battery, AC adapter, and USB power supply registration with
//! property reporting (capacity, status, voltage, current, temperature).
//! Mirrors Linux's `drivers/power/supply/power_supply_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Power supply type (Linux `enum power_supply_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSupplyType {
    Battery,
    Ups,
    Mains,
    Usb,
    UsbType,
    UsbPd,
    Wireless,
}

/// Power supply status (Linux `enum power_supply_property` / status).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSupplyStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

/// Power supply health (Linux `enum power_supply_health`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSupplyHealth {
    Good,
    Dead,
    Overheat,
    OverVoltage,
    Cold,
    WatchdogTimer,
    SafetyTimerExpire,
    Unknown,
}

/// Power supply properties (Linux `union power_supply_propval`).
#[derive(Debug, Clone)]
pub struct PowerSupplyProperties {
    pub status: PowerSupplyStatus,
    pub health: PowerSupplyHealth,
    pub capacity: u8,        // 0-100 percent
    pub voltage_now_uv: u32, // microvolts
    pub current_now_ua: i32, // microamps (negative = discharging)
    pub temp_celsius: i32,
    pub charge_full_uah: u32,
    pub charge_now_uah: u32,
    pub cycle_count: u32,
    pub manufacturer: String,
    pub model_name: String,
    pub serial_number: String,
    pub technology: &'static str,
}

impl Default for PowerSupplyProperties {
    fn default() -> Self {
        Self {
            status: PowerSupplyStatus::Unknown,
            health: PowerSupplyHealth::Good,
            capacity: 0,
            voltage_now_uv: 0,
            current_now_ua: 0,
            temp_celsius: 25,
            charge_full_uah: 0,
            charge_now_uah: 0,
            cycle_count: 0,
            manufacturer: String::new(),
            model_name: String::new(),
            serial_number: String::new(),
            technology: "unknown",
        }
    }
}

/// Operations for reading power supply properties (Linux `struct power_supply_ops`).
pub struct PowerSupplyOps {
    pub get_properties: fn() -> PowerSupplyProperties,
    pub get_name: fn() -> &'static str,
    pub get_type: fn() -> PowerSupplyType,
    pub is_online: fn() -> bool,
}

struct PowerSupply {
    id: u32,
    name: String,
    supply_type: PowerSupplyType,
    ops: &'static PowerSupplyOps,
}

// ── Mains (AC adapter) power supply ─────────────────────────────────────

fn mains_get_props() -> PowerSupplyProperties {
    let mut props = PowerSupplyProperties::default();
    props.status = PowerSupplyStatus::Charging;
    props.voltage_now_uv = 120_000_000; // 120V in µV (mains)
    props.current_now_ua = 1_500_000; // 1.5A
    props.temp_celsius = 25;
    props.manufacturer = String::from("ACPI");
    props.model_name = String::from("AC Adapter");
    props.technology = "mains";
    props
}

fn mains_name() -> &'static str {
    "mains"
}
fn mains_type() -> PowerSupplyType {
    PowerSupplyType::Mains
}
fn mains_online() -> bool {
    true
}

pub static MAINS_OPS: PowerSupplyOps = PowerSupplyOps {
    get_properties: mains_get_props,
    get_name: mains_name,
    get_type: mains_type,
    is_online: mains_online,
};

// ── Battery power supply ────────────────────────────────────────────────

static mut BATT_CAPACITY: u8 = 75;
static mut BATT_STATUS: PowerSupplyStatus = PowerSupplyStatus::Discharging;

fn battery_get_props() -> PowerSupplyProperties {
    let mut props = PowerSupplyProperties::default();
    props.status = unsafe { BATT_STATUS };
    props.health = PowerSupplyHealth::Good;
    props.capacity = unsafe { BATT_CAPACITY };
    props.voltage_now_uv = 12_000_000; // 12V
    props.current_now_ua = if unsafe { BATT_STATUS } == PowerSupplyStatus::Discharging {
        -500_000 // -0.5A discharging
    } else {
        1_000_000 // 1A charging
    };
    props.temp_celsius = 30;
    props.charge_full_uah = 50_000_000; // 50Wh
    props.charge_now_uah = (props.charge_full_uah as u64 * props.capacity as u64 / 100) as u32;
    props.cycle_count = 42;
    props.manufacturer = String::from("RustOS");
    props.model_name = String::from("Virtual Battery");
    props.serial_number = String::from("ROS-BAT-001");
    props.technology = "Li-ion";
    props
}

fn battery_name() -> &'static str {
    "BAT0"
}
fn battery_type() -> PowerSupplyType {
    PowerSupplyType::Battery
}
fn battery_online() -> bool {
    true
}

pub static BATTERY_OPS: PowerSupplyOps = PowerSupplyOps {
    get_properties: battery_get_props,
    get_name: battery_name,
    get_type: battery_type,
    is_online: battery_online,
};

// ── USB power supply ────────────────────────────────────────────────────

fn usb_get_props() -> PowerSupplyProperties {
    let mut props = PowerSupplyProperties::default();
    props.status = PowerSupplyStatus::Charging;
    props.voltage_now_uv = 5_000_000; // 5V
    props.current_now_ua = 900_000; // 900mA (USB 2.0)
    props.temp_celsius = 25;
    props.manufacturer = String::from("USB");
    props.model_name = String::from("USB Port");
    props.technology = "usb";
    props
}

fn usb_name() -> &'static str {
    "usb"
}
fn usb_type() -> PowerSupplyType {
    PowerSupplyType::Usb
}
fn usb_online() -> bool {
    false
}

pub static USB_OPS: PowerSupplyOps = PowerSupplyOps {
    get_properties: usb_get_props,
    get_name: usb_name,
    get_type: usb_type,
    is_online: usb_online,
};

// ── Registry ────────────────────────────────────────────────────────────

static POWER_SUPPLIES: RwLock<BTreeMap<u32, PowerSupply>> = RwLock::new(BTreeMap::new());
static NEXT_PS_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a power supply (Linux `power_supply_register`).
pub fn register_supply(name: &str, ops: &'static PowerSupplyOps) -> Result<u32, &'static str> {
    let supply_type = (ops.get_type)();
    let id = NEXT_PS_ID.fetch_add(1, Ordering::SeqCst);
    POWER_SUPPLIES.write().insert(
        id,
        PowerSupply {
            id,
            name: String::from(name),
            supply_type,
            ops,
        },
    );
    Ok(id)
}

/// Get power supply properties (Linux `power_supply_get_property`).
pub fn get_properties(supply_id: u32) -> Result<PowerSupplyProperties, &'static str> {
    let ops = {
        let supplies = POWER_SUPPLIES.read();
        let supply = supplies.get(&supply_id).ok_or("Power supply not found")?;
        supply.ops
    };
    Ok((ops.get_properties)())
}

/// Get supply type.
pub fn get_type(supply_id: u32) -> Result<PowerSupplyType, &'static str> {
    let supplies = POWER_SUPPLIES.read();
    let supply = supplies.get(&supply_id).ok_or("Power supply not found")?;
    Ok(supply.supply_type)
}

/// Check if supply is online (Linux `power_supply_is_online`).
pub fn is_online(supply_id: u32) -> Result<bool, &'static str> {
    let ops = {
        let supplies = POWER_SUPPLIES.read();
        let supply = supplies.get(&supply_id).ok_or("Power supply not found")?;
        supply.ops
    };
    Ok((ops.is_online)())
}

/// Find a supply by name (Linux `power_supply_get_by_name`).
pub fn find_by_name(name: &str) -> Option<u32> {
    POWER_SUPPLIES
        .read()
        .iter()
        .find(|(_, s)| s.name == name)
        .map(|(id, _)| *id)
}

/// Get all power supply IDs.
pub fn get_all_supplies() -> Vec<(u32, String, PowerSupplyType)> {
    POWER_SUPPLIES
        .read()
        .iter()
        .map(|(id, s)| (*id, s.name.clone(), s.supply_type))
        .collect()
}

/// Number of registered power supplies.
pub fn supply_count() -> usize {
    POWER_SUPPLIES.read().len()
}

/// Get combined battery status across all battery supplies.
pub fn get_battery_summary() -> Option<(u8, PowerSupplyStatus)> {
    let supplies = POWER_SUPPLIES.read();
    for supply in supplies.values() {
        if supply.supply_type == PowerSupplyType::Battery {
            let props = (supply.ops.get_properties)();
            return Some((props.capacity, props.status));
        }
    }
    None
}

/// Check if AC mains is online.
pub fn is_mains_online() -> bool {
    let supplies = POWER_SUPPLIES.read();
    for supply in supplies.values() {
        if supply.supply_type == PowerSupplyType::Mains {
            return (supply.ops.is_online)();
        }
    }
    false
}

/// Initialize power supply subsystem with mains and battery.
pub fn init() -> Result<(), &'static str> {
    if !POWER_SUPPLIES.read().is_empty() {
        return Ok(());
    }

    register_supply("mains", &MAINS_OPS)?;
    register_supply("BAT0", &BATTERY_OPS)?;
    register_supply("usb", &USB_OPS)?;

    crate::serial_println!("power_supply: {} supply(s) registered", supply_count());
    Ok(())
}
