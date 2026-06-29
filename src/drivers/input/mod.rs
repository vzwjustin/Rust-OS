//! Input event framework (evdev)
//!
//! Provides Linux input event codes, device registration, and event
//! dispatch for keyboards, mice, touchscreens, and other input devices.
//! Mirrors Linux's `drivers/input/input.c` and `drivers/input/evdev.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Linux input event types (Linux `input.h` EV_*) ──────────────────────

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;
pub const EV_SW: u16 = 0x05;
pub const EV_LED: u16 = 0x11;
pub const EV_SND: u16 = 0x12;
pub const EV_REP: u16 = 0x14;
pub const EV_FF: u16 = 0x15;
pub const EV_PWR: u16 = 0x16;
pub const EV_FF_STATUS: u16 = 0x17;

// ── Common key codes (Linux `input-event-codes.h` KEY_*) ────────────────

pub const KEY_RESERVED: u16 = 0;
pub const KEY_ESC: u16 = 1;
pub const KEY_1: u16 = 2;
pub const KEY_2: u16 = 3;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_SPACE: u16 = 57;
pub const KEY_CAPSLOCK: u16 = 58;
pub const KEY_F1: u16 = 59;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_TAB: u16 = 15;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_UP: u16 = 103;
pub const KEY_DOWN: u16 = 108;
pub const KEY_LEFT: u16 = 105;
pub const KEY_RIGHT: u16 = 106;

// ── Relative axis codes ─────────────────────────────────────────────────

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_HWHEEL: u16 = 0x06;

// ── Mouse button codes ──────────────────────────────────────────────────

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;

// ── Input event structure (Linux `struct input_event`) ──────────────────

#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
    pub timestamp_ms: u64,
}

// ── Input device types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    Keyboard,
    Mouse,
    Touchscreen,
    Joystick,
    Tablet,
    Other,
}

/// Device capabilities (which event types it supports).
#[derive(Debug, Clone)]
pub struct InputDeviceCaps {
    pub ev_types: Vec<u16>,
    pub key_codes: Vec<u16>,
    pub rel_codes: Vec<u16>,
    pub abs_codes: Vec<u16>,
}

/// Operations for an input device.
pub struct InputDeviceOps {
    pub get_events: fn() -> Vec<InputEvent>,
    pub get_name: fn() -> &'static str,
    pub get_device_type: fn() -> InputDeviceType,
}

struct InputDevice {
    id: u32,
    name: String,
    device_type: InputDeviceType,
    ops: &'static InputDeviceOps,
    event_queue: Vec<InputEvent>,
    enabled: bool,
}

// ── Keyboard input device ───────────────────────────────────────────────

fn kbd_get_events() -> Vec<InputEvent> {
    // Events are dispatched directly to input_manager by the keyboard
    // interrupt handler. This returns an empty queue for polling.
    Vec::new()
}

fn kbd_name() -> &'static str {
    "RustOS Keyboard"
}
fn kbd_type() -> InputDeviceType {
    InputDeviceType::Keyboard
}

pub static KBD_INPUT_OPS: InputDeviceOps = InputDeviceOps {
    get_events: kbd_get_events,
    get_name: kbd_name,
    get_device_type: kbd_type,
};

// ── Mouse input device ──────────────────────────────────────────────────

fn mouse_get_events() -> Vec<InputEvent> {
    Vec::new()
}

fn mouse_name() -> &'static str {
    "RustOS Mouse"
}
fn mouse_type() -> InputDeviceType {
    InputDeviceType::Mouse
}

pub static MOUSE_INPUT_OPS: InputDeviceOps = InputDeviceOps {
    get_events: mouse_get_events,
    get_name: mouse_name,
    get_device_type: mouse_type,
};

// ── Registry ────────────────────────────────────────────────────────────

static INPUT_DEVICES: RwLock<BTreeMap<u32, InputDevice>> = RwLock::new(BTreeMap::new());
static NEXT_INPUT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an input device (Linux `input_register_device`).
pub fn register_device(name: &str, ops: &'static InputDeviceOps) -> Result<u32, &'static str> {
    let device_type = (ops.get_device_type)();
    let id = NEXT_INPUT_ID.fetch_add(1, Ordering::SeqCst);
    INPUT_DEVICES.write().insert(
        id,
        InputDevice {
            id,
            name: String::from(name),
            device_type,
            ops,
            event_queue: Vec::new(),
            enabled: true,
        },
    );
    Ok(id)
}

/// Queue an event for a device (Linux `input_event`).
pub fn queue_event(device_id: u32, event: InputEvent) -> Result<(), &'static str> {
    let mut devices = INPUT_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Input device not found")?;
    if !dev.enabled {
        return Err("Input device is disabled");
    }
    dev.event_queue.push(event);
    // Keep queue bounded.
    if dev.event_queue.len() > 256 {
        dev.event_queue.remove(0);
    }
    Ok(())
}

/// Drain queued events from a device (Linux evdev read).
pub fn read_events(device_id: u32) -> Result<Vec<InputEvent>, &'static str> {
    let mut devices = INPUT_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Input device not found")?;
    let events = dev.event_queue.clone();
    dev.event_queue.clear();
    Ok(events)
}

/// Poll for events from a device (non-blocking).
pub fn poll_events(device_id: u32) -> Vec<InputEvent> {
    let mut devices = INPUT_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        if dev.enabled {
            let events = dev.event_queue.clone();
            dev.event_queue.clear();
            return events;
        }
    }
    Vec::new()
}

/// Enable/disable a device (Linux `input_enable_device`).
pub fn set_enabled(device_id: u32, enabled: bool) -> Result<(), &'static str> {
    let mut devices = INPUT_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Input device not found")?;
    dev.enabled = enabled;
    Ok(())
}

/// Get device type.
pub fn get_device_type(device_id: u32) -> Result<InputDeviceType, &'static str> {
    let devices = INPUT_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Input device not found")?;
    Ok(dev.device_type)
}

/// Get device name.
pub fn get_device_name(device_id: u32) -> Result<String, &'static str> {
    let devices = INPUT_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Input device not found")?;
    Ok(dev.name.clone())
}

/// Find device by type.
pub fn find_by_type(device_type: InputDeviceType) -> Option<u32> {
    INPUT_DEVICES
        .read()
        .iter()
        .find(|(_, d)| d.device_type == device_type)
        .map(|(id, _)| *id)
}

/// Number of registered input devices.
pub fn device_count() -> usize {
    INPUT_DEVICES.read().len()
}

/// Get all device IDs with their types.
pub fn get_all_devices() -> Vec<(u32, String, InputDeviceType)> {
    INPUT_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.device_type))
        .collect()
}

/// Create a key event helper.
pub fn key_event(code: u16, pressed: bool, timestamp_ms: u64) -> InputEvent {
    InputEvent {
        event_type: EV_KEY,
        code,
        value: if pressed { 1 } else { 0 },
        timestamp_ms,
    }
}

/// Create a relative movement event helper.
pub fn rel_event(code: u16, value: i32, timestamp_ms: u64) -> InputEvent {
    InputEvent {
        event_type: EV_REL,
        code,
        value,
        timestamp_ms,
    }
}

/// Create a sync event helper.
pub fn sync_event(timestamp_ms: u64) -> InputEvent {
    InputEvent {
        event_type: EV_SYN,
        code: 0,
        value: 0,
        timestamp_ms,
    }
}

/// Initialize input subsystem with keyboard and mouse.
pub fn init() -> Result<(), &'static str> {
    if !INPUT_DEVICES.read().is_empty() {
        return Ok(());
    }

    register_device("RustOS Keyboard", &KBD_INPUT_OPS)?;
    register_device("RustOS Mouse", &MOUSE_INPUT_OPS)?;

    crate::serial_println!("input: {} device(s) registered", device_count());
    Ok(())
}
