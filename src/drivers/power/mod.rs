//! Power (power management) driver subsystem
//!
//! Provides power management framework for suspend, resume, hibernate.
//! Mirrors Linux's `drivers/power/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Power state (Linux `enum power_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Running,
    SuspendToRam,
    SuspendToDisk,
    Hibernate,
    PowerOff,
    Reboot,
}

/// Power supply type (Linux `enum power_supply_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSupplyType {
    Battery,
    Ups,
    Mains,
    Usb,
    Wireless,
}

/// Power supply (Linux `struct power_supply`).
pub struct PowerSupply {
    pub id: u32,
    pub name: String,
    pub supply_type: PowerSupplyType,
    pub online: bool,
    pub capacity: u32,
    pub voltage_uv: u32,
    pub current_ua: i32,
    pub temp: u32,
    pub ops: PowerSupplyOps,
}

/// Power supply operations (Linux `struct power_supply_ops`).
pub struct PowerSupplyOps {
    pub get_property: fn(supply_id: u32, prop: PowerProperty) -> Result<u32, &'static str>,
    pub set_property:
        Option<fn(supply_id: u32, prop: PowerProperty, value: u32) -> Result<(), &'static str>>,
    pub external_power_changed: Option<fn(supply_id: u32)>,
}

/// Power supply property (Linux `enum power_supply_property`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProperty {
    Status,
    ChargeType,
    Health,
    Present,
    Online,
    Technology,
    Capacity,
    VoltageNow,
    CurrentNow,
    Temp,
    ModelName,
    Manufacturer,
}

/// Power management ops (Linux `struct dev_pm_ops`).
pub struct PmOps {
    pub suspend: fn() -> Result<(), &'static str>,
    pub resume: fn() -> Result<(), &'static str>,
    pub freeze: Option<fn() -> Result<(), &'static str>>,
    pub thaw: Option<fn() -> Result<(), &'static str>>,
    pub poweroff: Option<fn() -> Result<(), &'static str>>,
    pub restore: Option<fn() -> Result<(), &'static str>>,
}

// ── Registry ────────────────────────────────────────────────────────────

static SUPPLY_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static POWER_SUPPLIES: RwLock<BTreeMap<u32, PowerSupply>> = RwLock::new(BTreeMap::new());
static PM_OPS: RwLock<Option<PmOps>> = RwLock::new(None);
static CURRENT_POWER_STATE: RwLock<PowerState> = RwLock::new(PowerState::Running);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a power supply (Linux `power_supply_register`).
pub fn register_supply(
    name: &str,
    supply_type: PowerSupplyType,
    ops: PowerSupplyOps,
    online: bool,
    capacity: u32,
    voltage_uv: u32,
) -> Result<u32, &'static str> {
    let id = SUPPLY_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let supply = PowerSupply {
        id,
        name: String::from(name),
        supply_type,
        online,
        capacity,
        voltage_uv,
        current_ua: 0,
        temp: 250,
        ops,
    };
    POWER_SUPPLIES.write().insert(id, supply);
    Ok(id)
}

/// Get a power supply property.
pub fn get_property(supply_id: u32, prop: PowerProperty) -> Result<u32, &'static str> {
    let supplies = POWER_SUPPLIES.read();
    let supply = supplies.get(&supply_id).ok_or("Power supply not found")?;
    (supply.ops.get_property)(supply_id, prop)
}

/// Register global power management operations.
pub fn register_pm_ops(ops: PmOps) {
    *PM_OPS.write() = Some(ops);
}

/// Enter a power state (Linux `pm_suspend`).
pub fn enter_state(state: PowerState) -> Result<(), &'static str> {
    let ops = PM_OPS.read();
    if let Some(ref ops) = *ops {
        match state {
            PowerState::SuspendToRam => {
                (ops.suspend)()?;
                *CURRENT_POWER_STATE.write() = PowerState::SuspendToRam;
            }
            PowerState::Hibernate | PowerState::SuspendToDisk => {
                if let Some(freeze) = ops.freeze {
                    (freeze)()?;
                } else {
                    (ops.suspend)()?;
                }
                *CURRENT_POWER_STATE.write() = state;
            }
            PowerState::PowerOff | PowerState::Reboot => {
                if let Some(poweroff) = ops.poweroff {
                    (poweroff)()?;
                }
                *CURRENT_POWER_STATE.write() = state;
            }
            PowerState::Running => {
                (ops.resume)()?;
                *CURRENT_POWER_STATE.write() = PowerState::Running;
            }
        }
    } else {
        return Err("No PM ops registered");
    }
    Ok(())
}

/// Resume from suspend.
pub fn resume() -> Result<(), &'static str> {
    let ops = PM_OPS.read();
    if let Some(ref ops) = *ops {
        (ops.resume)()?;
        *CURRENT_POWER_STATE.write() = PowerState::Running;
        Ok(())
    } else {
        Err("No PM ops registered")
    }
}

/// Get current power state.
pub fn current_state() -> PowerState {
    *CURRENT_POWER_STATE.read()
}

/// List all power supplies.
pub fn list_supplies() -> Vec<(u32, String, PowerSupplyType, bool, u32)> {
    POWER_SUPPLIES
        .read()
        .iter()
        .map(|(id, s)| (*id, s.name.clone(), s.supply_type, s.online, s.capacity))
        .collect()
}

/// Count supplies.
pub fn supply_count() -> usize {
    POWER_SUPPLIES.read().len()
}

// ── Software power supply ───────────────────────────────────────────────

fn sw_get_property(_id: u32, prop: PowerProperty) -> Result<u32, &'static str> {
    match prop {
        PowerProperty::Online => Ok(1),
        PowerProperty::Capacity => Ok(100),
        PowerProperty::VoltageNow => Ok(12_000_000),
        PowerProperty::CurrentNow => Ok(0),
        PowerProperty::Temp => Ok(250),
        PowerProperty::Status => Ok(4),
        PowerProperty::Present => Ok(1),
        _ => Ok(0),
    }
}

/// Software power supply ops.
pub fn software_supply_ops() -> PowerSupplyOps {
    PowerSupplyOps {
        get_property: sw_get_property,
        set_property: None,
        external_power_changed: None,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !POWER_SUPPLIES.read().is_empty() {
        return Ok(());
    }

    let ops = software_supply_ops();
    let supply_id = register_supply("mains", PowerSupplyType::Mains, ops, true, 100, 12_000_000)?;

    crate::serial_println!(
        "power: mains power supply registered (id={}, online, 100%)",
        supply_id
    );
    Ok(())
}
