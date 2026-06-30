//! GPIO (General Purpose Input/Output) framework
//!
//! Provides GPIO chip registration, line direction control, and value
//! read/write similar to Linux's gpiolib (`drivers/gpio/gpiolib.c`).
//! Includes a software GPIO chip with virtual lines for platform use.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioPull {
    None,
    PullUp,
    PullDown,
}

#[derive(Debug, Clone, Copy)]
pub struct GpioLineConfig {
    pub direction: GpioDirection,
    pub pull: GpioPull,
    pub default_value: bool,
    pub label: &'static str,
}

impl Default for GpioLineConfig {
    fn default() -> Self {
        Self {
            direction: GpioDirection::Input,
            pull: GpioPull::None,
            default_value: false,
            label: "",
        }
    }
}

/// IRQ trigger type for GPIO interrupts (Linux GPIO_V2_LINE_FLAG_EDGE_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioIrqTrigger {
    None,
    Rising,
    Falling,
    Both,
}

/// Operations implemented by a GPIO controller driver.
pub struct GpioChipOps {
    pub get_direction: fn(line: u32) -> Result<GpioDirection, &'static str>,
    pub set_direction_input: fn(line: u32) -> Result<(), &'static str>,
    pub set_direction_output: fn(line: u32, value: bool) -> Result<(), &'static str>,
    pub get_value: fn(line: u32) -> Result<bool, &'static str>,
    pub set_value: fn(line: u32, value: bool) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_ngpio: fn() -> u32,
}

struct GpioChip {
    id: u32,
    name: String,
    base: u32,
    ngpio: u32,
    ops: GpioChipOps,
}

/// Per-line state tracking.
struct GpioLineState {
    chip_id: u32,
    line: u32,
    direction: GpioDirection,
    value: bool,
    irq_trigger: GpioIrqTrigger,
    consumer: Option<String>,
}

// ── Software GPIO chip ──────────────────────────────────────────────────

/// Virtual GPIO lines backed by in-memory state.
struct SoftwareGpioChip {
    lines: Vec<bool>,
    directions: Vec<GpioDirection>,
}

static mut SOFTWARE_GPIO: SoftwareGpioChip = SoftwareGpioChip {
    lines: Vec::new(),
    directions: Vec::new(),
};

fn sw_get_direction(_line: u32) -> Result<GpioDirection, &'static str> {
    let chip = unsafe { &SOFTWARE_GPIO };
    let idx = _line as usize;
    if idx >= chip.directions.len() {
        return Err("GPIO line out of range");
    }
    Ok(chip.directions[idx])
}

fn sw_set_direction_input(line: u32) -> Result<(), &'static str> {
    let chip = unsafe { &mut SOFTWARE_GPIO };
    let idx = line as usize;
    if idx >= chip.directions.len() {
        return Err("GPIO line out of range");
    }
    chip.directions[idx] = GpioDirection::Input;
    Ok(())
}

fn sw_set_direction_output(line: u32, value: bool) -> Result<(), &'static str> {
    let chip = unsafe { &mut SOFTWARE_GPIO };
    let idx = line as usize;
    if idx >= chip.directions.len() {
        return Err("GPIO line out of range");
    }
    chip.directions[idx] = GpioDirection::Output;
    chip.lines[idx] = value;
    Ok(())
}

fn sw_get_value(line: u32) -> Result<bool, &'static str> {
    let chip = unsafe { &SOFTWARE_GPIO };
    let idx = line as usize;
    if idx >= chip.lines.len() {
        return Err("GPIO line out of range");
    }
    Ok(chip.lines[idx])
}

fn sw_set_value(line: u32, value: bool) -> Result<(), &'static str> {
    let chip = unsafe { &mut SOFTWARE_GPIO };
    let idx = line as usize;
    if idx >= chip.lines.len() {
        return Err("GPIO line out of range");
    }
    chip.lines[idx] = value;
    Ok(())
}

fn sw_name() -> &'static str {
    "software-gpio"
}

fn sw_ngpio() -> u32 {
    32
}

const SOFTWARE_GPIO_OPS: GpioChipOps = GpioChipOps {
    get_direction: sw_get_direction,
    set_direction_input: sw_set_direction_input,
    set_direction_output: sw_set_direction_output,
    get_value: sw_get_value,
    set_value: sw_set_value,
    get_name: sw_name,
    get_ngpio: sw_ngpio,
};

// ── Registry ────────────────────────────────────────────────────────────

static GPIO_CHIPS: RwLock<BTreeMap<u32, GpioChip>> = RwLock::new(BTreeMap::new());
static GPIO_LINES: RwLock<BTreeMap<(u32, u32), GpioLineState>> = RwLock::new(BTreeMap::new());
static NEXT_CHIP_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a GPIO controller chip.
pub fn register_chip(name: &str, base: u32, ops: GpioChipOps) -> Result<u32, &'static str> {
    let ngpio = (ops.get_ngpio)();
    if ngpio == 0 {
        return Err("GPIO chip must expose at least one line");
    }
    let id = NEXT_CHIP_ID.fetch_add(1, Ordering::SeqCst);
    GPIO_CHIPS.write().insert(
        id,
        GpioChip {
            id,
            name: String::from(name),
            base,
            ngpio,
            ops,
        },
    );

    // Initialize line state entries.
    let mut lines = GPIO_LINES.write();
    for line in 0..ngpio {
        lines.insert(
            (id, line),
            GpioLineState {
                chip_id: id,
                line,
                direction: GpioDirection::Input,
                value: false,
                irq_trigger: GpioIrqTrigger::None,
                consumer: None,
            },
        );
    }
    Ok(id)
}

/// Request exclusive use of a GPIO line (Linux gpio_request).
pub fn request_line(chip_id: u32, line: u32, consumer: &str) -> Result<(), &'static str> {
    let mut lines = GPIO_LINES.write();
    let state = lines
        .get_mut(&(chip_id, line))
        .ok_or("GPIO line not found")?;
    if state.consumer.is_some() {
        return Err("GPIO line already requested");
    }
    state.consumer = Some(String::from(consumer));
    Ok(())
}

/// Free a previously requested GPIO line (Linux gpio_free).
pub fn free_line(chip_id: u32, line: u32) -> Result<(), &'static str> {
    let mut lines = GPIO_LINES.write();
    let state = lines
        .get_mut(&(chip_id, line))
        .ok_or("GPIO line not found")?;
    state.consumer = None;
    Ok(())
}

/// Set line direction to input (Linux gpio_direction_input).
pub fn direction_input(chip_id: u32, line: u32) -> Result<(), &'static str> {
    let chips = GPIO_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("GPIO chip not found")?;
    (chip.ops.set_direction_input)(line)?;
    if let Some(state) = GPIO_LINES.write().get_mut(&(chip_id, line)) {
        state.direction = GpioDirection::Input;
    }
    Ok(())
}

/// Set line direction to output with initial value (Linux gpio_direction_output).
pub fn direction_output(chip_id: u32, line: u32, value: bool) -> Result<(), &'static str> {
    let chips = GPIO_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("GPIO chip not found")?;
    (chip.ops.set_direction_output)(line, value)?;
    if let Some(state) = GPIO_LINES.write().get_mut(&(chip_id, line)) {
        state.direction = GpioDirection::Output;
        state.value = value;
    }
    Ok(())
}

/// Read the value of a GPIO line (Linux gpio_get_value).
pub fn get_value(chip_id: u32, line: u32) -> Result<bool, &'static str> {
    let chips = GPIO_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("GPIO chip not found")?;
    let value = (chip.ops.get_value)(line)?;
    if let Some(state) = GPIO_LINES.write().get_mut(&(chip_id, line)) {
        state.value = value;
    }
    Ok(value)
}

/// Set the value of a GPIO line (Linux gpio_set_value).
pub fn set_value(chip_id: u32, line: u32, value: bool) -> Result<(), &'static str> {
    let chips = GPIO_CHIPS.read();
    let chip = chips.get(&chip_id).ok_or("GPIO chip not found")?;
    (chip.ops.set_value)(line, value)?;
    if let Some(state) = GPIO_LINES.write().get_mut(&(chip_id, line)) {
        state.value = value;
    }
    Ok(())
}

/// Configure IRQ trigger type for a GPIO line (Linux gpio_set_irq_type).
pub fn set_irq_trigger(
    chip_id: u32,
    line: u32,
    trigger: GpioIrqTrigger,
) -> Result<(), &'static str> {
    let mut lines = GPIO_LINES.write();
    let state = lines
        .get_mut(&(chip_id, line))
        .ok_or("GPIO line not found")?;
    state.irq_trigger = trigger;
    Ok(())
}

/// Get the IRQ trigger type for a GPIO line.
pub fn get_irq_trigger(chip_id: u32, line: u32) -> Result<GpioIrqTrigger, &'static str> {
    let lines = GPIO_LINES.read();
    let state = lines.get(&(chip_id, line)).ok_or("GPIO line not found")?;
    Ok(state.irq_trigger)
}

/// Convert a global GPIO number to (chip_id, line).
pub fn gpio_to_chip_line(global: u32) -> Option<(u32, u32)> {
    let chips = GPIO_CHIPS.read();
    for chip in chips.values() {
        if global >= chip.base && global < chip.base + chip.ngpio {
            return Some((chip.id, global - chip.base));
        }
    }
    None
}

/// Number of registered GPIO chips.
pub fn chip_count() -> usize {
    GPIO_CHIPS.read().len()
}

/// Total number of GPIO lines across all chips.
pub fn total_lines() -> usize {
    GPIO_LINES.read().len()
}

/// Initialize GPIO subsystem with software chip.
pub fn init() -> Result<(), &'static str> {
    if !GPIO_CHIPS.read().is_empty() {
        return Ok(());
    }

    // Initialize software GPIO chip backing store.
    let ngpio = sw_ngpio() as usize;
    unsafe {
        SOFTWARE_GPIO.lines = alloc::vec![false; ngpio];
        SOFTWARE_GPIO.directions = alloc::vec![GpioDirection::Input; ngpio];
    }

    register_chip("software-gpio", 0, SOFTWARE_GPIO_OPS)?;
    crate::serial_println!("gpio: software chip registered ({} lines)", ngpio);
    crate::serial_println!("gpio: subsystem ready");
    Ok(())
}
