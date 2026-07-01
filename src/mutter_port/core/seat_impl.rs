//! MetaSeatImpl ported from GNOME Mutter's src/core/meta-seat-impl.c
//!
//! MetaSeatImpl is the kernel-level seat implementation: it manages the
//! set of input devices (keyboard, pointer, touch, tablet) belonging to
//! one seat, processes raw input events from evdev/libinput, and dispatches
//! them to the Clutter/Mutter event handling stack.
//!
//! In Mutter this is a GObject that wraps libinput and creates
//! ClutterInputDevice objects for each physical device. In the kernel,
//! libinput is not available; input events come from the kernel's own
//! keyboard/mouse drivers. The seat is modeled as a plain struct that
//! tracks devices and provides event dispatch helpers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-seat-impl.c

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Seat ID source.
static SEAT_ID: AtomicU32 = AtomicU32::new(0);

fn next_seat_id() -> u32 {
    SEAT_ID.fetch_add(1, Ordering::Relaxed) + 1
}

/// Input device class, mirrors ClutterInputDeviceType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Physical keyboard.
    Keyboard,
    /// Physical mouse / pointer.
    Pointer,
    /// Touchpad.
    Touchpad,
    /// Touchscreen.
    Touchscreen,
    /// Pen / drawing tablet.
    Pen,
    /// Eraser (tablet eraser tip).
    Eraser,
    /// Tablet pad (button strip).
    TabletPad,
    /// Trackball.
    Trackball,
    /// Joystick.
    Joystick,
    /// Other / unspecified.
    Other,
}

impl DeviceType {
    /// Whether this device type provides pointer events.
    pub fn is_pointer(&self) -> bool {
        matches!(
            self,
            DeviceType::Pointer
                | DeviceType::Touchpad
                | DeviceType::Trackball
                | DeviceType::Pen
                | DeviceType::Eraser
        )
    }

    /// Whether this device type provides keyboard events.
    pub fn is_keyboard(&self) -> bool {
        matches!(self, DeviceType::Keyboard)
    }

    /// Whether this device type provides touch events.
    pub fn is_touch(&self) -> bool {
        matches!(self, DeviceType::Touchscreen)
    }
}

/// An input device tracked by the seat. Mirrors ClutterInputDevice
/// (the subset managed by MetaSeatImpl).
#[derive(Debug, Clone)]
pub struct InputDevice {
    /// Unique device id.
    pub id: u32,
    /// Device type.
    pub device_type: DeviceType,
    /// Device name (from evdev).
    pub name: String,
    /// Vendor ID (from input_id).
    pub vendor_id: u32,
    /// Product ID (from input_id).
    pub product_id: u32,
    /// Whether this is a logical device (synthesized, not physical).
    pub is_logical: bool,
    /// Dimensions of the device's native coordinate space (for tablets/touch).
    pub width_mm: f32,
    pub height_mm: f32,
    /// Number of axes (for tablets).
    pub n_axes: u32,
    /// Number of buttons (for pointers).
    pub n_buttons: u32,
}

impl InputDevice {
    pub fn new(id: u32, device_type: DeviceType, name: &str) -> Self {
        InputDevice {
            id,
            device_type,
            name: String::from(name),
            vendor_id: 0,
            product_id: 0,
            is_logical: false,
            width_mm: 0.0,
            height_mm: 0.0,
            n_axes: 0,
            n_buttons: 0,
        }
    }
}

/// A raw input event from the kernel input subsystem. Mirrors the
/// libinput_event → ClutterEvent translation path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// Key press/release. (device_id, keycode, pressed)
    Key {
        device_id: u32,
        keycode: u32,
        pressed: bool,
    },
    /// Pointer motion. (device_id, dx, dy)
    Motion { device_id: u32, dx: f32, dy: f32 },
    /// Absolute pointer position. (device_id, x, y)
    AbsoluteMotion { device_id: u32, x: f32, y: f32 },
    /// Button press/release. (device_id, button, pressed)
    Button {
        device_id: u32,
        button: u32,
        pressed: bool,
    },
    /// Touch down. (device_id, slot, x, y)
    TouchDown {
        device_id: u32,
        slot: u32,
        x: f32,
        y: f32,
    },
    /// Touch motion. (device_id, slot, x, y)
    TouchMotion {
        device_id: u32,
        slot: u32,
        x: f32,
        y: f32,
    },
    /// Touch up. (device_id, slot)
    TouchUp { device_id: u32, slot: u32 },
    /// Proximity in/out (tablet). (device_id, in)
    Proximity { device_id: u32, proximity_in: bool },
    /// Scroll wheel. (device_id, dx, dy)
    Scroll { device_id: u32, dx: f32, dy: f32 },
}

/// The seat implementation. Mirrors MetaSeatImpl.
#[derive(Debug)]
pub struct MetaSeatImpl {
    /// Unique seat id.
    pub id: u32,
    /// Seat name (e.g. "seat0").
    pub name: String,
    /// All input devices on this seat.
    devices: Vec<InputDevice>,
    /// The keyboard device id, if any.
    keyboard_device: Option<u32>,
    /// The pointer device id, if any.
    pointer_device: Option<u32>,
    /// Current keyboard modifier state (bitmask).
    modifier_state: u32,
    /// Current pointer position.
    pointer_pos: (f32, f32),
    /// Current button state (bitmask: button 1 = bit 0, etc.).
    button_state: u32,
    /// Number of touch points currently down.
    touch_count: u32,
    /// Whether the seat is paused (VT switch away).
    paused: bool,
    /// Pending events to dispatch.
    pending_events: Vec<InputEvent>,
    /// Next device id.
    next_device_id: u32,
}

/// Modifier key bitmask values, matching Linux input.h modifier constants.
pub mod modifiers {
    pub const SHIFT: u32 = 1 << 0;
    pub const CAPS_LOCK: u32 = 1 << 1;
    pub const CTRL: u32 = 1 << 2;
    pub const ALT: u32 = 1 << 3;
    pub const NUM_LOCK: u32 = 1 << 4;
    pub const META: u32 = 1 << 5; // Super/Windows key
}

impl MetaSeatImpl {
    /// Create a new seat. Mirrors meta_seat_impl_new().
    pub fn new(name: &str) -> Self {
        MetaSeatImpl {
            id: next_seat_id(),
            name: String::from(name),
            devices: Vec::new(),
            keyboard_device: None,
            pointer_device: None,
            modifier_state: 0,
            pointer_pos: (0.0, 0.0),
            button_state: 0,
            touch_count: 0,
            paused: false,
            pending_events: Vec::new(),
            next_device_id: 1,
        }
    }

    // ── Device management ─────────────────────────────────────────────

    /// Add a device to the seat. Mirrors meta_seat_impl_add_device().
    pub fn add_device(&mut self, device_type: DeviceType, name: &str) -> u32 {
        let id = self.next_device_id;
        self.next_device_id += 1;
        let device = InputDevice::new(id, device_type, name);
        self.devices.push(device);

        // Auto-assign primary keyboard/pointer.
        if device_type == DeviceType::Keyboard && self.keyboard_device.is_none() {
            self.keyboard_device = Some(id);
        }
        if device_type.is_pointer() && self.pointer_device.is_none() {
            self.pointer_device = Some(id);
        }

        id
    }

    /// Remove a device from the seat. Mirrors meta_seat_impl_remove_device().
    pub fn remove_device(&mut self, device_id: u32) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.id != device_id);

        if self.keyboard_device == Some(device_id) {
            self.keyboard_device = self
                .devices
                .iter()
                .find(|d| d.device_type == DeviceType::Keyboard)
                .map(|d| d.id);
        }
        if self.pointer_device == Some(device_id) {
            self.pointer_device = self
                .devices
                .iter()
                .find(|d| d.device_type.is_pointer())
                .map(|d| d.id);
        }

        self.devices.len() != before
    }

    /// Get all devices.
    pub fn devices(&self) -> &[InputDevice] {
        &self.devices
    }

    /// Get a device by id.
    pub fn get_device(&self, device_id: u32) -> Option<&InputDevice> {
        self.devices.iter().find(|d| d.id == device_id)
    }

    /// Get the primary keyboard device id.
    pub fn keyboard_device(&self) -> Option<u32> {
        self.keyboard_device
    }

    /// Get the primary pointer device id.
    pub fn pointer_device(&self) -> Option<u32> {
        self.pointer_device
    }

    /// Number of devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    // ── Event processing ──────────────────────────────────────────────

    /// Queue a raw input event. The compositor will drain these via
    /// `take_pending_events()`.
    pub fn queue_event(&mut self, event: InputEvent) {
        if self.paused {
            return;
        }
        self.update_state(&event);
        self.pending_events.push(event);
    }

    /// Drain pending events. Mirrors the dispatch path in
    /// meta_seat_impl_dispatch_event().
    pub fn take_pending_events(&mut self) -> Vec<InputEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Number of pending events.
    pub fn pending_event_count(&self) -> usize {
        self.pending_events.len()
    }

    /// Update internal state from an event. This mirrors the state tracking
    /// in meta_seat_impl_process_event() (modifier state, pointer position,
    /// button state, touch count).
    fn update_state(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::Key {
                keycode, pressed, ..
            } => {
                self.update_modifiers(keycode, pressed);
            }
            InputEvent::Motion { dx, dy, .. } => {
                self.pointer_pos.0 += dx;
                self.pointer_pos.1 += dy;
                if self.pointer_pos.0 < 0.0 {
                    self.pointer_pos.0 = 0.0;
                }
                if self.pointer_pos.1 < 0.0 {
                    self.pointer_pos.1 = 0.0;
                }
            }
            InputEvent::AbsoluteMotion { x, y, .. } => {
                self.pointer_pos = (x, y);
            }
            InputEvent::Button {
                button, pressed, ..
            } => {
                if button < 32 {
                    if pressed {
                        self.button_state |= 1 << button;
                    } else {
                        self.button_state &= !(1 << button);
                    }
                }
            }
            InputEvent::TouchDown { .. } => {
                self.touch_count += 1;
            }
            InputEvent::TouchUp { .. } => {
                if self.touch_count > 0 {
                    self.touch_count -= 1;
                }
            }
            _ => {}
        }
    }

    /// Update modifier state from a key event. Maps Linux keycodes to
    /// modifier bits.
    fn update_modifiers(&mut self, keycode: u32, pressed: bool) {
        // Linux keycodes (linux/input.h):
        // KEY_LEFTSHIFT=42, KEY_RIGHTSHIFT=54, KEY_LEFTCTRL=29, KEY_RIGHTCTRL=97,
        // KEY_LEFTALT=56, KEY_RIGHTALT=100, KEY_LEFTMETA=125, KEY_RIGHTMETA=126,
        // KEY_CAPSLOCK=58, KEY_NUMLOCK=69
        let mod_bit = match keycode {
            42 | 54 => Some(modifiers::SHIFT),
            29 | 97 => Some(modifiers::CTRL),
            56 | 100 => Some(modifiers::ALT),
            125 | 126 => Some(modifiers::META),
            58 => Some(modifiers::CAPS_LOCK),
            69 => Some(modifiers::NUM_LOCK),
            _ => None,
        };

        if let Some(bit) = mod_bit {
            if pressed {
                self.modifier_state |= bit;
            } else {
                // Caps Lock and Num Lock are toggles, not held.
                if bit == modifiers::CAPS_LOCK || bit == modifiers::NUM_LOCK {
                    self.modifier_state ^= bit;
                } else {
                    self.modifier_state &= !bit;
                }
            }
        }
    }

    // ── State queries ─────────────────────────────────────────────────

    /// Current modifier state bitmask.
    pub fn modifier_state(&self) -> u32 {
        self.modifier_state
    }

    /// Whether a modifier is active.
    pub fn has_modifier(&self, mod_bit: u32) -> bool {
        self.modifier_state & mod_bit != 0
    }

    /// Current pointer position.
    pub fn pointer_pos(&self) -> (f32, f32) {
        self.pointer_pos
    }

    /// Current button state bitmask.
    pub fn button_state(&self) -> u32 {
        self.button_state
    }

    /// Whether a button is currently pressed.
    pub fn is_button_pressed(&self, button: u32) -> bool {
        if button < 32 {
            self.button_state & (1 << button) != 0
        } else {
            false
        }
    }

    /// Number of active touch points.
    pub fn touch_count(&self) -> u32 {
        self.touch_count
    }

    // ── Pause / resume (VT switch) ────────────────────────────────────

    /// Pause the seat (VT switch away). Mirrors meta_seat_impl_pause().
    pub fn pause(&mut self) {
        self.paused = true;
        self.pending_events.clear();
    }

    /// Resume the seat (VT switch back). Mirrors meta_seat_impl_resume().
    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_seat() {
        let seat = MetaSeatImpl::new("seat0");
        assert_eq!(seat.name, "seat0");
        assert_eq!(seat.device_count(), 0);
        assert!(seat.keyboard_device().is_none());
        assert!(seat.pointer_device().is_none());
    }

    #[test]
    fn test_add_devices() {
        let mut seat = MetaSeatImpl::new("seat0");
        let kid = seat.add_device(DeviceType::Keyboard, "AT Keyboard");
        let pid = seat.add_device(DeviceType::Pointer, "USB Mouse");

        assert_eq!(seat.device_count(), 2);
        assert_eq!(seat.keyboard_device(), Some(kid));
        assert_eq!(seat.pointer_device(), Some(pid));
    }

    #[test]
    fn test_remove_device() {
        let mut seat = MetaSeatImpl::new("seat0");
        let kid = seat.add_device(DeviceType::Keyboard, "Keyboard");
        let pid = seat.add_device(DeviceType::Pointer, "Mouse");

        assert!(seat.remove_device(pid));
        assert_eq!(seat.pointer_device(), None);
        assert_eq!(seat.keyboard_device(), Some(kid));

        assert!(seat.remove_device(kid));
        assert_eq!(seat.keyboard_device(), None);
    }

    #[test]
    fn test_remove_falls_back_to_next() {
        let mut seat = MetaSeatImpl::new("seat0");
        let k1 = seat.add_device(DeviceType::Keyboard, "K1");
        let _k2 = seat.add_device(DeviceType::Keyboard, "K2");

        assert_eq!(seat.keyboard_device(), Some(k1));
        seat.remove_device(k1);
        // Should fall back to the remaining keyboard.
        assert!(seat.keyboard_device().is_some());
    }

    #[test]
    fn test_modifier_state() {
        let mut seat = MetaSeatImpl::new("seat0");
        let kid = seat.add_device(DeviceType::Keyboard, "Kbd");

        // Press Left Shift (keycode 42).
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 42,
            pressed: true,
        });
        assert!(seat.has_modifier(modifiers::SHIFT));

        // Press Left Ctrl (keycode 29).
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 29,
            pressed: true,
        });
        assert!(seat.has_modifier(modifiers::SHIFT));
        assert!(seat.has_modifier(modifiers::CTRL));

        // Release Left Shift.
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 42,
            pressed: false,
        });
        assert!(!seat.has_modifier(modifiers::SHIFT));
        assert!(seat.has_modifier(modifiers::CTRL));
    }

    #[test]
    fn test_caps_lock_toggle() {
        let mut seat = MetaSeatImpl::new("seat0");
        let kid = seat.add_device(DeviceType::Keyboard, "Kbd");

        // Press CapsLock (keycode 58) → toggle on.
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 58,
            pressed: true,
        });
        assert!(seat.has_modifier(modifiers::CAPS_LOCK));

        // Release CapsLock → should stay on (toggle).
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 58,
            pressed: false,
        });
        assert!(seat.has_modifier(modifiers::CAPS_LOCK));

        // Press again → toggle off.
        seat.queue_event(InputEvent::Key {
            device_id: kid,
            keycode: 58,
            pressed: true,
        });
        assert!(!seat.has_modifier(modifiers::CAPS_LOCK));
    }

    #[test]
    fn test_pointer_motion() {
        let mut seat = MetaSeatImpl::new("seat0");
        let pid = seat.add_device(DeviceType::Pointer, "Mouse");

        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 10.0,
            dy: 20.0,
        });
        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 5.0,
            dy: 5.0,
        });

        let (x, y) = seat.pointer_pos();
        assert_eq!(x, 15.0);
        assert_eq!(y, 25.0);
    }

    #[test]
    fn test_absolute_motion() {
        let mut seat = MetaSeatImpl::new("seat0");
        let pid = seat.add_device(DeviceType::Touchscreen, "Touch");

        seat.queue_event(InputEvent::AbsoluteMotion {
            device_id: pid,
            x: 100.0,
            y: 200.0,
        });
        assert_eq!(seat.pointer_pos(), (100.0, 200.0));
    }

    #[test]
    fn test_button_state() {
        let mut seat = MetaSeatImpl::new("seat0");
        let pid = seat.add_device(DeviceType::Pointer, "Mouse");

        seat.queue_event(InputEvent::Button {
            device_id: pid,
            button: 0,
            pressed: true,
        });
        assert!(seat.is_button_pressed(0));

        seat.queue_event(InputEvent::Button {
            device_id: pid,
            button: 1,
            pressed: true,
        });
        assert!(seat.is_button_pressed(0));
        assert!(seat.is_button_pressed(1));

        seat.queue_event(InputEvent::Button {
            device_id: pid,
            button: 0,
            pressed: false,
        });
        assert!(!seat.is_button_pressed(0));
        assert!(seat.is_button_pressed(1));
    }

    #[test]
    fn test_touch_count() {
        let mut seat = MetaSeatImpl::new("seat0");
        let tid = seat.add_device(DeviceType::Touchscreen, "Touch");

        seat.queue_event(InputEvent::TouchDown {
            device_id: tid,
            slot: 0,
            x: 10.0,
            y: 10.0,
        });
        seat.queue_event(InputEvent::TouchDown {
            device_id: tid,
            slot: 1,
            x: 20.0,
            y: 20.0,
        });
        assert_eq!(seat.touch_count(), 2);

        seat.queue_event(InputEvent::TouchUp {
            device_id: tid,
            slot: 0,
        });
        assert_eq!(seat.touch_count(), 1);
    }

    #[test]
    fn test_pending_events() {
        let mut seat = MetaSeatImpl::new("seat0");
        let pid = seat.add_device(DeviceType::Pointer, "Mouse");

        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 1.0,
            dy: 1.0,
        });
        seat.queue_event(InputEvent::Button {
            device_id: pid,
            button: 0,
            pressed: true,
        });
        assert_eq!(seat.pending_event_count(), 2);

        let events = seat.take_pending_events();
        assert_eq!(events.len(), 2);
        assert_eq!(seat.pending_event_count(), 0);
    }

    #[test]
    fn test_pause_resume() {
        let mut seat = MetaSeatImpl::new("seat0");
        let pid = seat.add_device(DeviceType::Pointer, "Mouse");

        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 1.0,
            dy: 1.0,
        });
        assert_eq!(seat.pending_event_count(), 1);

        seat.pause();
        // Events queued while paused are dropped.
        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 5.0,
            dy: 5.0,
        });
        assert_eq!(seat.pending_event_count(), 1);

        seat.resume();
        seat.queue_event(InputEvent::Motion {
            device_id: pid,
            dx: 2.0,
            dy: 2.0,
        });
        assert_eq!(seat.pending_event_count(), 2);
    }

    #[test]
    fn test_device_type_helpers() {
        assert!(DeviceType::Pointer.is_pointer());
        assert!(DeviceType::Touchpad.is_pointer());
        assert!(DeviceType::Pen.is_pointer());
        assert!(!DeviceType::Keyboard.is_pointer());

        assert!(DeviceType::Keyboard.is_keyboard());
        assert!(!DeviceType::Pointer.is_keyboard());

        assert!(DeviceType::Touchscreen.is_touch());
        assert!(!DeviceType::Pointer.is_touch());
    }
}
