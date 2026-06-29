//! HID report parser and input event dispatch
//!
//! Parses USB HID boot protocol keyboard reports and dispatches key events
//! to the input manager. Supports device registration for multiple HID sources.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::drivers::input_manager;
use crate::keyboard::{KeyEvent, SpecialKey};

// ── Types ───────────────────────────────────────────────────────────────

/// Boot protocol keyboard report (8 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct HidBootKeyboardReport {
    pub modifiers: u8,
    pub reserved: u8,
    pub keys: [u8; 6],
}

impl HidBootKeyboardReport {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 8 {
            return Err("HID boot keyboard report must be at least 8 bytes");
        }
        Ok(Self {
            modifiers: data[0],
            reserved: data[1],
            keys: [data[2], data[3], data[4], data[5], data[6], data[7]],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceType {
    BootKeyboard,
    BootMouse,
    Generic,
}

pub struct HidDeviceOps {
    pub parse_report: fn(report: &[u8]) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct HidDevice {
    id: u32,
    name: String,
    device_type: HidDeviceType,
    ops: HidDeviceOps,
}

/// Tracks pressed keys for boot keyboard rollover handling.
#[derive(Clone, Copy)]
struct BootKeyboardState {
    modifiers: u8,
    pressed_keys: [u8; 6],
}

impl BootKeyboardState {
    const fn new() -> Self {
        Self {
            modifiers: 0,
            pressed_keys: [0; 6],
        }
    }
}

static mut BOOT_KBD_STATE: BootKeyboardState = BootKeyboardState::new();

// ── HID usage to KeyEvent mapping ───────────────────────────────────────

fn hid_usage_to_key_event(usage: u8, pressed: bool) -> Option<KeyEvent> {
    match usage {
        0x04..=0x1D => {
            let c = char::from(b'a' + (usage - 0x04));
            Some(if pressed {
                KeyEvent::CharacterPress(c)
            } else {
                KeyEvent::CharacterRelease(c)
            })
        }
        0x1E..=0x27 => {
            let c = match usage {
                0x1E => '1',
                0x1F => '2',
                0x20 => '3',
                0x21 => '4',
                0x22 => '5',
                0x23 => '6',
                0x24 => '7',
                0x25 => '8',
                0x26 => '9',
                0x27 => '0',
                _ => return None,
            };
            Some(if pressed {
                KeyEvent::CharacterPress(c)
            } else {
                KeyEvent::CharacterRelease(c)
            })
        }
        0x28 => special_key(SpecialKey::Enter, pressed),
        0x29 => special_key(SpecialKey::Escape, pressed),
        0x2A => special_key(SpecialKey::Backspace, pressed),
        0x2B => special_key(SpecialKey::Tab, pressed),
        0x2C => char_key(' ', pressed),
        0x2D => char_key('-', pressed),
        0x2E => char_key('=', pressed),
        0x2F => char_key('[', pressed),
        0x30 => char_key(']', pressed),
        0x31 => char_key('\\', pressed),
        0x33 => char_key(';', pressed),
        0x34 => char_key('\'', pressed),
        0x35 => char_key('`', pressed),
        0x36 => char_key(',', pressed),
        0x37 => char_key('.', pressed),
        0x38 => char_key('/', pressed),
        0x39 => special_key(SpecialKey::CapsLock, pressed),
        0x3A => special_key(SpecialKey::F1, pressed),
        0x3B => special_key(SpecialKey::F2, pressed),
        0x3C => special_key(SpecialKey::F3, pressed),
        0x3D => special_key(SpecialKey::F4, pressed),
        0x3E => special_key(SpecialKey::F5, pressed),
        0x3F => special_key(SpecialKey::F6, pressed),
        0x40 => special_key(SpecialKey::F7, pressed),
        0x41 => special_key(SpecialKey::F8, pressed),
        0x42 => special_key(SpecialKey::F9, pressed),
        0x43 => special_key(SpecialKey::F10, pressed),
        0x44 => special_key(SpecialKey::F11, pressed),
        0x45 => special_key(SpecialKey::F12, pressed),
        0x49 => special_key(SpecialKey::Insert, pressed),
        0x4A => special_key(SpecialKey::Home, pressed),
        0x4B => special_key(SpecialKey::PageUp, pressed),
        0x4C => special_key(SpecialKey::Delete, pressed),
        0x4D => special_key(SpecialKey::End, pressed),
        0x4E => special_key(SpecialKey::PageDown, pressed),
        0x4F => special_key(SpecialKey::ArrowRight, pressed),
        0x50 => special_key(SpecialKey::ArrowLeft, pressed),
        0x51 => special_key(SpecialKey::ArrowDown, pressed),
        0x52 => special_key(SpecialKey::ArrowUp, pressed),
        _ => Some(if pressed {
            KeyEvent::RawPress(usage)
        } else {
            KeyEvent::RawRelease(usage)
        }),
    }
}

fn char_key(c: char, pressed: bool) -> Option<KeyEvent> {
    Some(if pressed {
        KeyEvent::CharacterPress(c)
    } else {
        KeyEvent::CharacterRelease(c)
    })
}

fn special_key(key: SpecialKey, pressed: bool) -> Option<KeyEvent> {
    Some(if pressed {
        KeyEvent::SpecialPress(key)
    } else {
        KeyEvent::SpecialRelease(key)
    })
}

fn dispatch_key_event(event: KeyEvent) {
    input_manager::handle_keyboard_event(event, event.is_press());
}

fn process_modifier_changes(old_mod: u8, new_mod: u8) {
    const MODIFIERS: [(u8, SpecialKey); 4] = [
        (1 << 0, SpecialKey::LeftCtrl),
        (1 << 1, SpecialKey::LeftShift),
        (1 << 2, SpecialKey::LeftAlt),
        (1 << 4, SpecialKey::LeftCtrl), // Right Ctrl -> LeftCtrl for input layer
    ];

    for (mask, key) in MODIFIERS {
        let was = (old_mod & mask) != 0;
        let now = (new_mod & mask) != 0;
        if was != now {
            if let Some(ev) = special_key(key, now) {
                dispatch_key_event(ev);
            }
        }
    }

    // Right shift
    let was = (old_mod & (1 << 5)) != 0;
    let now = (new_mod & (1 << 5)) != 0;
    if was != now {
        if let Some(ev) = special_key(SpecialKey::RightShift, now) {
            dispatch_key_event(ev);
        }
    }
}

/// Parse a boot protocol keyboard report and dispatch events to input_manager.
pub fn parse_boot_keyboard_report(data: &[u8]) -> Result<(), &'static str> {
    let report = HidBootKeyboardReport::from_bytes(data)?;

    let state = unsafe { &mut BOOT_KBD_STATE };
    process_modifier_changes(state.modifiers, report.modifiers);
    state.modifiers = report.modifiers;

    // Release keys no longer present in the report.
    for &old_key in &state.pressed_keys {
        if old_key != 0 && !report.keys.contains(&old_key) {
            if let Some(ev) = hid_usage_to_key_event(old_key, false) {
                dispatch_key_event(ev);
            }
        }
    }

    // Press newly appeared keys.
    for &new_key in &report.keys {
        if new_key != 0 && !state.pressed_keys.contains(&new_key) {
            if let Some(ev) = hid_usage_to_key_event(new_key, true) {
                dispatch_key_event(ev);
            }
        }
    }

    state.pressed_keys = report.keys;
    Ok(())
}

fn boot_keyboard_parse(report: &[u8]) -> Result<(), &'static str> {
    parse_boot_keyboard_report(report)
}

fn boot_keyboard_name() -> &'static str {
    "boot-keyboard"
}

const BOOT_KEYBOARD_OPS: HidDeviceOps = HidDeviceOps {
    parse_report: boot_keyboard_parse,
    get_name: boot_keyboard_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static HID_DEVICES: RwLock<BTreeMap<u32, HidDevice>> = RwLock::new(BTreeMap::new());
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_device(
    name: &str,
    device_type: HidDeviceType,
    ops: HidDeviceOps,
) -> Result<u32, &'static str> {
    let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    HID_DEVICES.write().insert(
        id,
        HidDevice {
            id,
            name: String::from(name),
            device_type,
            ops,
        },
    );
    Ok(id)
}

pub fn handle_report(device_id: u32, report: &[u8]) -> Result<(), &'static str> {
    let devices = HID_DEVICES.read();
    let device = devices.get(&device_id).ok_or("HID device not found")?;
    (device.ops.parse_report)(report)
}

pub fn device_count() -> usize {
    HID_DEVICES.read().len()
}

/// Initialize HID subsystem and register boot protocol keyboard handler.
pub fn init() -> Result<(), &'static str> {
    if !HID_DEVICES.read().is_empty() {
        return Ok(());
    }

    if !input_manager::is_initialized() {
        input_manager::init();
    }

    register_device(
        "boot-keyboard",
        HidDeviceType::BootKeyboard,
        BOOT_KEYBOARD_OPS,
    )?;
    crate::serial_println!("hid: {} device(s) registered", device_count());
    Ok(())
}
