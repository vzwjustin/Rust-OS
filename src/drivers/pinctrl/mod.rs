//! Pin control (pinctrl) subsystem
//!
//! Provides pin multiplexing, configuration (pull-up/down, drive strength),
//! and GPIO integration similar to Linux's `drivers/pinctrl/core.c`.
//! Includes a software pin controller for platform use.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── Types ───────────────────────────────────────────────────────────────

/// Pin configuration parameters (Linux `enum pin_config_param`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinConfigParam {
    PullUp,
    PullDown,
    PullNone,
    DriveStrength(u32), // in mA
    InputEnable,
    OutputEnable,
    BiasDisable,
    SlewRateFast,
    SlewRateSlow,
}

/// Pin function descriptor (Linux `struct pinmux_ops` function).
#[derive(Debug, Clone)]
pub struct PinFunction {
    pub name: String,
    pub groups: Vec<String>,
}

/// Operations implemented by a pin controller driver (Linux `struct pinctrl_ops`).
pub struct PinctrlOps {
    pub get_groups_count: fn() -> u32,
    pub get_group_name: fn(selector: u32) -> &'static str,
    pub get_group_pins: fn(selector: u32) -> Vec<u32>,
    pub get_functions_count: fn() -> u32,
    pub get_function_name: fn(selector: u32) -> &'static str,
    pub get_function_groups: fn(selector: u32) -> Vec<u32>,
    pub pinmux_set: fn(function: u32, group: u32) -> Result<(), &'static str>,
    pub pin_config_set: fn(pin: u32, config: PinConfigParam) -> Result<(), &'static str>,
    pub pin_config_get: fn(pin: u32) -> Result<PinConfigParam, &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_npins: fn() -> u32,
}

struct PinctrlDev {
    id: u32,
    name: String,
    ops: &'static PinctrlOps,
    npins: u32,
}

/// Per-pin state tracking.
struct PinState {
    controller_id: u32,
    pin: u32,
    function: Option<u32>,
    config: PinConfigParam,
    owner: Option<String>,
}

// ── Software pin controller ─────────────────────────────────────────────

static SW_PIN_FUNCS: Mutex<Vec<Option<u32>>> = Mutex::new(Vec::new());
static SW_PIN_CONFIGS: Mutex<Vec<PinConfigParam>> = Mutex::new(Vec::new());

fn sw_groups_count() -> u32 {
    4
}
fn sw_group_name(sel: u32) -> &'static str {
    match sel {
        0 => "gpio-group",
        1 => "uart-group",
        2 => "spi-group",
        3 => "i2c-group",
        _ => "unknown",
    }
}
fn sw_group_pins(sel: u32) -> Vec<u32> {
    match sel {
        0 => (0..16).collect(),
        1 => {
            let mut v = Vec::new();
            v.push(16);
            v.push(17);
            v.push(18);
            v.push(19);
            v
        }
        2 => {
            let mut v = Vec::new();
            v.push(20);
            v.push(21);
            v.push(22);
            v.push(23);
            v
        }
        3 => {
            let mut v = Vec::new();
            v.push(24);
            v.push(25);
            v
        }
        _ => Vec::new(),
    }
}
fn sw_functions_count() -> u32 {
    4
}
fn sw_function_name(sel: u32) -> &'static str {
    match sel {
        0 => "gpio",
        1 => "uart",
        2 => "spi",
        3 => "i2c",
        _ => "unknown",
    }
}
fn sw_function_groups(sel: u32) -> Vec<u32> {
    match sel {
        0 => {
            let mut v = Vec::new();
            v.push(0);
            v
        }
        1 => {
            let mut v = Vec::new();
            v.push(1);
            v
        }
        2 => {
            let mut v = Vec::new();
            v.push(2);
            v
        }
        3 => {
            let mut v = Vec::new();
            v.push(3);
            v
        }
        _ => Vec::new(),
    }
}
fn sw_pinmux_set(function: u32, group: u32) -> Result<(), &'static str> {
    let pins = sw_group_pins(group);
    let mut funcs = SW_PIN_FUNCS.lock();
    for pin in pins {
        let idx = pin as usize;
        if idx < funcs.len() {
            funcs[idx] = Some(function);
        }
    }
    Ok(())
}
fn sw_pin_config_set(pin: u32, config: PinConfigParam) -> Result<(), &'static str> {
    let mut configs = SW_PIN_CONFIGS.lock();
    let idx = pin as usize;
    if idx >= configs.len() {
        return Err("Pin out of range");
    }
    configs[idx] = config;
    Ok(())
}
fn sw_pin_config_get(pin: u32) -> Result<PinConfigParam, &'static str> {
    let configs = SW_PIN_CONFIGS.lock();
    let idx = pin as usize;
    if idx >= configs.len() {
        return Err("Pin out of range");
    }
    Ok(configs[idx])
}
fn sw_name() -> &'static str {
    "software-pinctrl"
}
fn sw_npins() -> u32 {
    32
}

pub static SW_PINCTRL_OPS: PinctrlOps = PinctrlOps {
    get_groups_count: sw_groups_count,
    get_group_name: sw_group_name,
    get_group_pins: sw_group_pins,
    get_functions_count: sw_functions_count,
    get_function_name: sw_function_name,
    get_function_groups: sw_function_groups,
    pinmux_set: sw_pinmux_set,
    pin_config_set: sw_pin_config_set,
    pin_config_get: sw_pin_config_get,
    get_name: sw_name,
    get_npins: sw_npins,
};

// ── Registry ────────────────────────────────────────────────────────────

static PINCTRL_DEVS: RwLock<BTreeMap<u32, PinctrlDev>> = RwLock::new(BTreeMap::new());
static PIN_STATES: RwLock<BTreeMap<(u32, u32), PinState>> = RwLock::new(BTreeMap::new());
static NEXT_CTRL_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a pin controller (Linux `pinctrl_register`).
pub fn register_controller(name: &str, ops: &'static PinctrlOps) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("Pin controller name is empty");
    }
    let npins = (ops.get_npins)();
    if npins == 0 {
        return Err("Pin controller must have at least one pin");
    }
    if (ops.get_groups_count)() == 0 {
        return Err("Pin controller must have at least one group");
    }
    if (ops.get_functions_count)() == 0 {
        return Err("Pin controller must have at least one function");
    }
    let mut devs = PINCTRL_DEVS.write();
    if devs.values().any(|dev| dev.name == name) {
        return Err("Pin controller already registered");
    }

    let id = NEXT_CTRL_ID.fetch_add(1, Ordering::SeqCst);
    devs.insert(
        id,
        PinctrlDev {
            id,
            name: String::from(name),
            ops,
            npins,
        },
    );

    // Initialize pin states.
    let mut states = PIN_STATES.write();
    for pin in 0..npins {
        states.insert(
            (id, pin),
            PinState {
                controller_id: id,
                pin,
                function: None,
                config: PinConfigParam::PullNone,
                owner: None,
            },
        );
    }
    Ok(id)
}

/// Request exclusive use of a pin (Linux `pin_request`).
pub fn request_pin(ctrl_id: u32, pin: u32, owner: &str) -> Result<(), &'static str> {
    if owner.is_empty() {
        return Err("Pin owner is empty");
    }

    let mut states = PIN_STATES.write();
    let state = states.get_mut(&(ctrl_id, pin)).ok_or("Pin not found")?;
    if state.owner.is_some() {
        return Err("Pin already requested");
    }
    state.owner = Some(String::from(owner));
    Ok(())
}

/// Free a requested pin (Linux `pin_free`).
pub fn free_pin(ctrl_id: u32, pin: u32) -> Result<(), &'static str> {
    let mut states = PIN_STATES.write();
    let state = states.get_mut(&(ctrl_id, pin)).ok_or("Pin not found")?;
    state.owner = None;
    state.function = None;
    Ok(())
}

/// Select a function for a group of pins (Linux `pinmux_select`).
pub fn select_function(ctrl_id: u32, function: u32, group: u32) -> Result<(), &'static str> {
    let (ops, npins) = {
        let devs = PINCTRL_DEVS.read();
        let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
        let group_count = (dev.ops.get_groups_count)();
        if group >= group_count {
            return Err("Pin group selector out of range");
        }
        let function_count = (dev.ops.get_functions_count)();
        if function >= function_count {
            return Err("Pin function selector out of range");
        }
        let function_groups = (dev.ops.get_function_groups)(function);
        if !function_groups.iter().any(|candidate| *candidate == group) {
            return Err("Pin function does not support group");
        }
        (dev.ops, dev.npins)
    };

    let pins = (ops.get_group_pins)(group);
    if pins.is_empty() {
        return Err("Pin group has no pins");
    }
    if pins.iter().any(|pin| *pin >= npins) {
        return Err("Pin group contains invalid pin");
    }

    (ops.pinmux_set)(function, group)?;

    // Update pin states.
    let mut states = PIN_STATES.write();
    for pin in pins {
        if let Some(state) = states.get_mut(&(ctrl_id, pin)) {
            state.function = Some(function);
        }
    }
    Ok(())
}

/// Configure a pin (Linux `pin_config_set`).
pub fn config_pin(ctrl_id: u32, pin: u32, config: PinConfigParam) -> Result<(), &'static str> {
    let ops = {
        let devs = PINCTRL_DEVS.read();
        let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
        if pin >= dev.npins {
            return Err("Pin selector out of range");
        }
        dev.ops
    };

    (ops.pin_config_set)(pin, config)?;

    let mut states = PIN_STATES.write();
    if let Some(state) = states.get_mut(&(ctrl_id, pin)) {
        state.config = config;
    }
    Ok(())
}

/// Get pin configuration (Linux `pin_config_get`).
pub fn get_pin_config(ctrl_id: u32, pin: u32) -> Result<PinConfigParam, &'static str> {
    let ops = {
        let devs = PINCTRL_DEVS.read();
        let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
        if pin >= dev.npins {
            return Err("Pin selector out of range");
        }
        dev.ops
    };
    (ops.pin_config_get)(pin)
}

/// Get number of pin groups (Linux `pinctrl_get_groups_count`).
pub fn get_groups_count(ctrl_id: u32) -> Result<u32, &'static str> {
    let devs = PINCTRL_DEVS.read();
    let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
    Ok((dev.ops.get_groups_count)())
}

/// Get group name by selector.
pub fn get_group_name(ctrl_id: u32, selector: u32) -> Result<&'static str, &'static str> {
    let devs = PINCTRL_DEVS.read();
    let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
    if selector >= (dev.ops.get_groups_count)() {
        return Err("Pin group selector out of range");
    }
    Ok((dev.ops.get_group_name)(selector))
}

/// Get number of functions.
pub fn get_functions_count(ctrl_id: u32) -> Result<u32, &'static str> {
    let devs = PINCTRL_DEVS.read();
    let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
    Ok((dev.ops.get_functions_count)())
}

/// Get function name by selector.
pub fn get_function_name(ctrl_id: u32, selector: u32) -> Result<&'static str, &'static str> {
    let devs = PINCTRL_DEVS.read();
    let dev = devs.get(&ctrl_id).ok_or("Pin controller not found")?;
    if selector >= (dev.ops.get_functions_count)() {
        return Err("Pin function selector out of range");
    }
    Ok((dev.ops.get_function_name)(selector))
}

/// Number of registered pin controllers.
pub fn controller_count() -> usize {
    PINCTRL_DEVS.read().len()
}

/// Total number of pins across all controllers.
pub fn total_pins() -> usize {
    PIN_STATES.read().len()
}

/// Initialize pinctrl subsystem with software controller.
pub fn init() -> Result<(), &'static str> {
    if !PINCTRL_DEVS.read().is_empty() {
        return Ok(());
    }

    let npins = (SW_PINCTRL_OPS.get_npins)();
    *SW_PIN_FUNCS.lock() = alloc::vec![None; npins as usize];
    *SW_PIN_CONFIGS.lock() = alloc::vec![PinConfigParam::PullNone; npins as usize];

    register_controller("software-pinctrl", &SW_PINCTRL_OPS)?;
    crate::serial_println!("pinctrl: software controller registered ({} pins)", npins);
    Ok(())
}
