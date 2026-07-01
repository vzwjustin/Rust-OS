//! GNOME src/wayland/meta-wayland-keyboard.c
//!
//! MetaWaylandKeyboard implements the wl_keyboard half of a seat. It tracks the
//! focused surface, the set of currently-pressed keys (so a fresh focus can be
//! sent the correct wl_keyboard.enter key array), per-key press serials, and
//! the xkb modifier/group state. The C file also owns the compiled xkb keymap
//! and the anonymous-file plumbing to hand it to clients; that is stubbed here.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-keyboard.c

use super::input_device::{next_serial, MetaWaylandInputDevice};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// wl_keyboard key state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Released,
    Pressed,
}

/// Xkb modifier/group state broadcast via wl_keyboard.modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub depressed: u32,
    pub latched: u32,
    pub locked: u32,
    pub group: u32,
}

/// MetaWaylandKeyboard
pub struct MetaWaylandKeyboard {
    parent: MetaWaylandInputDevice,

    /// Surface with keyboard focus (wl_keyboard.enter target).
    focus_surface: Option<u32>,
    /// Serial of the last enter sent to `focus_surface`.
    focus_serial: u32,

    /// Evdev keycodes currently held down, in press order (mirrors the
    /// `pressed_keys` wl_array). Sent verbatim on focus enter.
    pressed_keys: Vec<u32>,
    /// keycode -> serial of the press event, so the matching release can be
    /// correlated (mirrors `key_down_serials`).
    key_down_serials: BTreeMap<u32, u32>,
    last_key_up_serial: u32,
    last_key_up: u32,

    /// Current xkb modifier state.
    modifiers: ModifierState,
}

impl MetaWaylandKeyboard {
    pub fn new(seat: u32) -> Self {
        MetaWaylandKeyboard {
            parent: MetaWaylandInputDevice::new(seat),
            focus_surface: None,
            focus_serial: 0,
            pressed_keys: Vec::new(),
            key_down_serials: BTreeMap::new(),
            last_key_up_serial: 0,
            last_key_up: 0,
            modifiers: ModifierState::default(),
        }
    }

    pub fn seat(&self) -> u32 {
        self.parent.get_seat()
    }

    pub fn focus_surface(&self) -> Option<u32> {
        self.focus_surface
    }

    pub fn focus_serial(&self) -> u32 {
        self.focus_serial
    }

    pub fn pressed_keys(&self) -> &[u32] {
        &self.pressed_keys
    }

    pub fn modifiers(&self) -> ModifierState {
        self.modifiers
    }

    /// meta_wayland_keyboard_set_focus(): emit leave on old focus, enter (with
    /// the current pressed-key array + modifiers) on the new one. Returns the
    /// enter serial (0 when clearing focus).
    pub fn set_focus(&mut self, surface: Option<u32>) -> u32 {
        if self.focus_surface == surface {
            return self.focus_serial;
        }
        // STUB: wl_keyboard.leave on old focus resources.
        self.focus_surface = surface;
        if surface.is_some() {
            self.focus_serial = next_serial();
            // STUB: wl_keyboard.enter (pressed_keys) + modifiers to new focus.
        } else {
            self.focus_serial = 0;
        }
        self.focus_serial
    }

    /// meta_wayland_keyboard_broadcast_key(): update pressed-key tracking and
    /// return the serial for this key event. Duplicate presses/releases are
    /// filtered like the real code (a key already down is not re-added).
    pub fn key(&mut self, keycode: u32, state: KeyState) -> u32 {
        let serial = next_serial();
        match state {
            KeyState::Pressed => {
                if !self.pressed_keys.contains(&keycode) {
                    self.pressed_keys.push(keycode);
                    self.key_down_serials.insert(keycode, serial);
                }
            }
            KeyState::Released => {
                self.pressed_keys.retain(|&k| k != keycode);
                self.key_down_serials.remove(&keycode);
                self.last_key_up = keycode;
                self.last_key_up_serial = serial;
            }
        }
        // STUB: wl_keyboard.key to focus resources.
        serial
    }

    /// Serial recorded for a currently-held key, if any.
    pub fn key_down_serial(&self, keycode: u32) -> Option<u32> {
        self.key_down_serials.get(&keycode).copied()
    }

    /// notify_modifiers(): update and broadcast xkb modifier state. Returns the
    /// serial if the state changed (a modifiers event is only sent on change).
    pub fn set_modifiers(&mut self, mods: ModifierState) -> Option<u32> {
        if self.modifiers == mods {
            return None;
        }
        self.modifiers = mods;
        let serial = next_serial();
        // STUB: wl_keyboard.modifiers to focus resources.
        Some(serial)
    }

    /// STUB: compiling the xkb keymap and exposing it via an anonymous file
    /// (wl_keyboard.keymap). Kernel side has no xkb/mmap; returns a placeholder.
    pub fn keymap_fd(&self) -> Option<i32> {
        None
    }

    /// Called when the focused surface is destroyed.
    pub fn surface_destroyed(&mut self, surface: u32) {
        if self.focus_surface == Some(surface) {
            self.focus_surface = None;
            self.focus_serial = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_serial() {
        let mut k = MetaWaylandKeyboard::new(1);
        let s = k.set_focus(Some(11));
        assert_eq!(k.focus_surface(), Some(11));
        assert!(s > 0);
        assert_eq!(k.set_focus(None), 0);
        assert_eq!(k.focus_surface(), None);
    }

    #[test]
    fn test_pressed_key_tracking() {
        let mut k = MetaWaylandKeyboard::new(1);
        let sa = k.key(30, KeyState::Pressed);
        k.key(31, KeyState::Pressed);
        // Duplicate press is ignored.
        k.key(30, KeyState::Pressed);
        assert_eq!(k.pressed_keys(), &[30, 31]);
        assert_eq!(k.key_down_serial(30), Some(sa));

        k.key(30, KeyState::Released);
        assert_eq!(k.pressed_keys(), &[31]);
        assert_eq!(k.key_down_serial(30), None);
    }

    #[test]
    fn test_modifiers_only_change() {
        let mut k = MetaWaylandKeyboard::new(1);
        let m = ModifierState {
            depressed: 4,
            latched: 0,
            locked: 0,
            group: 0,
        };
        assert!(k.set_modifiers(m).is_some());
        // No change -> no event.
        assert!(k.set_modifiers(m).is_none());
        assert_eq!(k.modifiers(), m);
    }

    #[test]
    fn test_surface_destroyed() {
        let mut k = MetaWaylandKeyboard::new(1);
        k.set_focus(Some(8));
        k.surface_destroyed(8);
        assert_eq!(k.focus_surface(), None);
    }
}
