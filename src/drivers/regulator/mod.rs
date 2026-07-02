//! Voltage regulator framework
//!
//! Provides enable/disable, voltage query, and consumer binding for power
//! rails. Registers platform fixed regulators derived from ACPI availability
//! and standard PC power domains.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegulatorType {
    Fixed,
    Switchable,
    Ldo,
}

#[derive(Debug, Clone, Copy)]
pub struct RegulatorConfig {
    pub regulator_type: RegulatorType,
    pub min_uv: u32,
    pub max_uv: u32,
    pub boot_on: bool,
    pub always_on: bool,
}

pub struct RegulatorOps {
    pub enable: fn(enabled: bool) -> Result<(), &'static str>,
    pub is_enabled: fn() -> bool,
    pub set_voltage: fn(uv: u32) -> Result<(), &'static str>,
    pub get_voltage: fn() -> u32,
    pub get_name: fn() -> &'static str,
}

struct Regulator {
    id: u32,
    name: String,
    config: RegulatorConfig,
    ops: RegulatorOps,
    enabled: bool,
    voltage_uv: u32,
}

// ── Fixed platform regulators ───────────────────────────────────────────

// Platform 3.3V rail (always on)
static REG_3V3_ENABLED: AtomicBool = AtomicBool::new(true);

fn reg_3v3_enable(enabled: bool) -> Result<(), &'static str> {
    REG_3V3_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

fn reg_3v3_is_enabled() -> bool {
    REG_3V3_ENABLED.load(Ordering::Relaxed)
}

fn reg_3v3_set_voltage(uv: u32) -> Result<(), &'static str> {
    if uv != 3_300_000 {
        return Err("Fixed 3.3V regulator voltage is not adjustable");
    }
    Ok(())
}

fn reg_3v3_get_voltage() -> u32 {
    3_300_000
}

fn reg_3v3_name() -> &'static str {
    "regulator-3v3"
}

const REG_3V3_OPS: RegulatorOps = RegulatorOps {
    enable: reg_3v3_enable,
    is_enabled: reg_3v3_is_enabled,
    set_voltage: reg_3v3_set_voltage,
    get_voltage: reg_3v3_get_voltage,
    get_name: reg_3v3_name,
};

// Platform 5V rail
static REG_5V_ENABLED: AtomicBool = AtomicBool::new(true);

fn reg_5v_enable(enabled: bool) -> Result<(), &'static str> {
    REG_5V_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

fn reg_5v_is_enabled() -> bool {
    REG_5V_ENABLED.load(Ordering::Relaxed)
}

fn reg_5v_set_voltage(uv: u32) -> Result<(), &'static str> {
    if uv != 5_000_000 {
        return Err("Fixed 5V regulator voltage is not adjustable");
    }
    Ok(())
}

fn reg_5v_get_voltage() -> u32 {
    5_000_000
}

fn reg_5v_name() -> &'static str {
    "regulator-5v"
}

const REG_5V_OPS: RegulatorOps = RegulatorOps {
    enable: reg_5v_enable,
    is_enabled: reg_5v_is_enabled,
    set_voltage: reg_5v_set_voltage,
    get_voltage: reg_5v_get_voltage,
    get_name: reg_5v_name,
};

// CPU core rail (switchable, ACPI-gated when available)
static REG_VCORE_ENABLED: AtomicBool = AtomicBool::new(false);
static REG_VCORE_VOLTAGE_UV: AtomicU32 = AtomicU32::new(1_000_000);

fn reg_vcore_enable(enabled: bool) -> Result<(), &'static str> {
    REG_VCORE_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

fn reg_vcore_is_enabled() -> bool {
    REG_VCORE_ENABLED.load(Ordering::Relaxed)
}

fn reg_vcore_set_voltage(uv: u32) -> Result<(), &'static str> {
    if !(800_000..=1_500_000).contains(&uv) {
        return Err("CPU core voltage out of range");
    }
    REG_VCORE_VOLTAGE_UV.store(uv, Ordering::Relaxed);
    Ok(())
}

fn reg_vcore_get_voltage() -> u32 {
    REG_VCORE_VOLTAGE_UV.load(Ordering::Relaxed)
}

fn reg_vcore_name() -> &'static str {
    "regulator-vcore"
}

const REG_VCORE_OPS: RegulatorOps = RegulatorOps {
    enable: reg_vcore_enable,
    is_enabled: reg_vcore_is_enabled,
    set_voltage: reg_vcore_set_voltage,
    get_voltage: reg_vcore_get_voltage,
    get_name: reg_vcore_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static REGULATORS: RwLock<BTreeMap<u32, Regulator>> = RwLock::new(BTreeMap::new());
static NEXT_REGULATOR_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_regulator(
    name: &str,
    config: RegulatorConfig,
    ops: RegulatorOps,
) -> Result<u32, &'static str> {
    let id = NEXT_REGULATOR_ID.fetch_add(1, Ordering::SeqCst);
    let enabled = config.boot_on || config.always_on;
    let voltage_uv = config.max_uv;

    REGULATORS.write().insert(
        id,
        Regulator {
            id,
            name: String::from(name),
            config,
            ops,
            enabled,
            voltage_uv,
        },
    );
    Ok(id)
}

pub fn enable_regulator(id: u32) -> Result<(), &'static str> {
    let mut regs = REGULATORS.write();
    let reg = regs.get_mut(&id).ok_or("Regulator not found")?;
    if reg.config.always_on {
        reg.enabled = true;
        return Ok(());
    }
    (reg.ops.enable)(true)?;
    reg.enabled = true;
    Ok(())
}

pub fn disable_regulator(id: u32) -> Result<(), &'static str> {
    let mut regs = REGULATORS.write();
    let reg = regs.get_mut(&id).ok_or("Regulator not found")?;
    if reg.config.always_on {
        return Err("Cannot disable always-on regulator");
    }
    (reg.ops.enable)(false)?;
    reg.enabled = false;
    Ok(())
}

pub fn set_voltage(id: u32, uv: u32) -> Result<(), &'static str> {
    let mut regs = REGULATORS.write();
    let reg = regs.get_mut(&id).ok_or("Regulator not found")?;
    if uv < reg.config.min_uv || uv > reg.config.max_uv {
        return Err("Voltage outside regulator range");
    }
    (reg.ops.set_voltage)(uv)?;
    reg.voltage_uv = uv;
    Ok(())
}

pub fn get_voltage(id: u32) -> Result<u32, &'static str> {
    let regs = REGULATORS.read();
    let reg = regs.get(&id).ok_or("Regulator not found")?;
    Ok((reg.ops.get_voltage)())
}

pub fn is_enabled(id: u32) -> Result<bool, &'static str> {
    let regs = REGULATORS.read();
    let reg = regs.get(&id).ok_or("Regulator not found")?;
    Ok((reg.ops.is_enabled)())
}

pub fn regulator_count() -> usize {
    REGULATORS.read().len()
}

fn register_platform_regulators() -> Result<(), &'static str> {
    register_regulator(
        "regulator-3v3",
        RegulatorConfig {
            regulator_type: RegulatorType::Fixed,
            min_uv: 3_300_000,
            max_uv: 3_300_000,
            boot_on: true,
            always_on: true,
        },
        REG_3V3_OPS,
    )?;

    register_regulator(
        "regulator-5v",
        RegulatorConfig {
            regulator_type: RegulatorType::Fixed,
            min_uv: 5_000_000,
            max_uv: 5_000_000,
            boot_on: true,
            always_on: false,
        },
        REG_5V_OPS,
    )?;

    register_regulator(
        "regulator-vcore",
        RegulatorConfig {
            regulator_type: RegulatorType::Switchable,
            min_uv: 800_000,
            max_uv: 1_500_000,
            boot_on: crate::acpi::power_management_available(),
            always_on: false,
        },
        REG_VCORE_OPS,
    )?;

    Ok(())
}

/// Initialize regulator subsystem with platform fixed rails.
pub fn init() -> Result<(), &'static str> {
    if !REGULATORS.read().is_empty() {
        return Ok(());
    }

    register_platform_regulators()?;
    crate::serial_println!("regulator: {} rail(s) registered", regulator_count());
    Ok(())
}
