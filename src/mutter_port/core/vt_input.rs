//! MetaVtInput ported from GNOME Mutter's src/core/meta-vt-input.c
//!
//! MetaVtInput reads input from a Linux VT (virtual terminal) when the
//! compositor is running on a text VT. It reads key events from
//! /dev/tty0 (or the active VT) and translates them into Clutter key
//! events for the compositor.
//!
//! In the kernel, we have direct access to the keyboard driver. This module
//! provides the VT input abstraction: it tracks the current VT, reads
//! raw keycodes from the keyboard interrupt handler, and translates them
//! into Mutter-compatible key events using the Linux keycode → keysym mapping.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-vt-input.c

use alloc::string::String;
use alloc::vec::Vec;

/// A VT input event. Mirrors the ClutterKeyEvent that MetaVtInput produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VtKeyEvent {
    /// Linux keycode (linux/input.h).
    pub keycode: u32,
    /// Whether the key was pressed (true) or released (false).
    pub pressed: bool,
    /// Modifier state at the time of the event.
    pub modifier_state: u32,
    /// Translated keysym (X11 keysym, matching libxkbcommon output).
    pub keysym: u32,
    /// Unicode code point, if the key produces a character.
    pub unicode: Option<u32>,
}

/// VT input reader. Mirrors MetaVtInput.
///
/// In Mutter this opens /dev/tty* and reads key events using read().
/// In the kernel, the keyboard driver pushes events directly; this struct
/// provides the translation and buffering layer.
#[derive(Debug)]
pub struct MetaVtInput {
    /// Current VT number.
    vt: u32,
    /// Whether VT input is active (the compositor owns the VT).
    active: bool,
    /// Pending key events to dispatch.
    pending_events: Vec<VtKeyEvent>,
    /// Current modifier state.
    modifier_state: u32,
    /// Current keyboard layout index.
    layout_index: u32,
    /// Whether Num Lock is active.
    num_lock: bool,
    /// Whether Caps Lock is active.
    caps_lock: bool,
}

/// Modifier bitmask values (same as seat_impl::modifiers).
pub mod vt_modifiers {
    pub const SHIFT: u32 = 1 << 0;
    pub const CAPS_LOCK: u32 = 1 << 1;
    pub const CTRL: u32 = 1 << 2;
    pub const ALT: u32 = 1 << 3;
    pub const NUM_LOCK: u32 = 1 << 4;
    pub const META: u32 = 1 << 5;
}

impl MetaVtInput {
    /// Create a new VT input reader for the given VT. Mirrors
    /// meta_vt_input_new().
    pub fn new(vt: u32) -> Self {
        MetaVtInput {
            vt,
            active: false,
            pending_events: Vec::new(),
            modifier_state: 0,
            layout_index: 0,
            num_lock: false,
            caps_lock: false,
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────

    /// Activate VT input (the compositor has taken the VT). Mirrors
    /// the VT_ACTIVATE signal handler.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate VT input (VT switch away). Mirrors the VT_DEACTIVATE
    /// signal handler.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.pending_events.clear();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn vt(&self) -> u32 {
        self.vt
    }

    pub fn set_vt(&mut self, vt: u32) {
        self.vt = vt;
    }

    // ── Event processing ──────────────────────────────────────────────

    /// Process a raw keycode from the keyboard driver. Translates it
    /// into a VtKeyEvent and queues it. Mirrors the read() → key event
    /// translation in meta_vt_input_process().
    pub fn process_keycode(&mut self, keycode: u32, pressed: bool) {
        if !self.active {
            return;
        }

        // Update modifier state.
        self.update_modifiers(keycode, pressed);

        // Translate keycode to keysym.
        let keysym = keycode_to_keysym(keycode, self.modifier_state, self.layout_index);
        let unicode = keysym_to_unicode(keysym, self.modifier_state);

        let event = VtKeyEvent {
            keycode,
            pressed,
            modifier_state: self.modifier_state,
            keysym,
            unicode,
        };
        self.pending_events.push(event);
    }

    /// Drain pending events.
    pub fn take_pending_events(&mut self) -> Vec<VtKeyEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Number of pending events.
    pub fn pending_event_count(&self) -> usize {
        self.pending_events.len()
    }

    // ── State ─────────────────────────────────────────────────────────

    pub fn modifier_state(&self) -> u32 {
        self.modifier_state
    }

    pub fn layout_index(&self) -> u32 {
        self.layout_index
    }

    pub fn set_layout_index(&mut self, index: u32) {
        self.layout_index = index;
    }

    pub fn num_lock(&self) -> bool {
        self.num_lock
    }

    pub fn caps_lock(&self) -> bool {
        self.caps_lock
    }

    // ── Internal ──────────────────────────────────────────────────────

    fn update_modifiers(&mut self, keycode: u32, pressed: bool) {
        let mod_bit = match keycode {
            42 | 54 => Some(vt_modifiers::SHIFT),
            29 | 97 => Some(vt_modifiers::CTRL),
            56 | 100 => Some(vt_modifiers::ALT),
            125 | 126 => Some(vt_modifiers::META),
            58 => Some(vt_modifiers::CAPS_LOCK),
            69 => Some(vt_modifiers::NUM_LOCK),
            _ => None,
        };

        if let Some(bit) = mod_bit {
            if bit == vt_modifiers::CAPS_LOCK {
                if pressed {
                    self.caps_lock = !self.caps_lock;
                    self.modifier_state ^= vt_modifiers::CAPS_LOCK;
                }
            } else if bit == vt_modifiers::NUM_LOCK {
                if pressed {
                    self.num_lock = !self.num_lock;
                    self.modifier_state ^= vt_modifiers::NUM_LOCK;
                }
            } else if pressed {
                self.modifier_state |= bit;
            } else {
                self.modifier_state &= !bit;
            }
        }
    }
}

/// Translate a Linux keycode to an X11 keysym. This is a simplified
/// version of the xkbcommon keycode → keysym mapping.
///
/// In Mutter, libxkbcommon handles this. Here we provide a basic mapping
/// for common keys; the full xkbcommon tables would be ported separately.
pub fn keycode_to_keysym(keycode: u32, modifier_state: u32, _layout: u32) -> u32 {
    let shift = modifier_state & vt_modifiers::SHIFT != 0;
    let caps = modifier_state & vt_modifiers::CAPS_LOCK != 0;

    // Linux keycodes to X11 keysyms (simplified, US layout).
    // Keycodes from linux/input.h.
    match keycode {
        // Letters a-z (keycodes 30-44, 46-57)
        30 => {
            if shift || caps {
                0x0041
            } else {
                0x0061
            }
        } // A/a
        48 => {
            if shift || caps {
                0x0042
            } else {
                0x0062
            }
        } // B/b
        46 => {
            if shift || caps {
                0x0043
            } else {
                0x0063
            }
        } // C/c
        32 => {
            if shift || caps {
                0x0044
            } else {
                0x0064
            }
        } // D/d
        18 => {
            if shift || caps {
                0x0045
            } else {
                0x0065
            }
        } // E/e
        33 => {
            if shift || caps {
                0x0046
            } else {
                0x0066
            }
        } // F/f
        34 => {
            if shift || caps {
                0x0047
            } else {
                0x0067
            }
        } // G/g
        35 => {
            if shift || caps {
                0x0048
            } else {
                0x0068
            }
        } // H/h
        23 => {
            if shift || caps {
                0x0049
            } else {
                0x0069
            }
        } // I/i
        36 => {
            if shift || caps {
                0x004A
            } else {
                0x006A
            }
        } // J/j
        37 => {
            if shift || caps {
                0x004B
            } else {
                0x006B
            }
        } // K/k
        38 => {
            if shift || caps {
                0x004C
            } else {
                0x006C
            }
        } // L/l
        50 => {
            if shift || caps {
                0x004D
            } else {
                0x006D
            }
        } // M/m
        49 => {
            if shift || caps {
                0x004E
            } else {
                0x006E
            }
        } // N/n
        24 => {
            if shift || caps {
                0x004F
            } else {
                0x006F
            }
        } // O/o
        25 => {
            if shift || caps {
                0x0050
            } else {
                0x0070
            }
        } // P/p
        16 => {
            if shift || caps {
                0x0051
            } else {
                0x0071
            }
        } // Q/q
        19 => {
            if shift || caps {
                0x0052
            } else {
                0x0072
            }
        } // R/r
        31 => {
            if shift || caps {
                0x0053
            } else {
                0x0073
            }
        } // S/s
        20 => {
            if shift || caps {
                0x0054
            } else {
                0x0074
            }
        } // T/t
        22 => {
            if shift || caps {
                0x0055
            } else {
                0x0075
            }
        } // U/u
        47 => {
            if shift || caps {
                0x0056
            } else {
                0x0076
            }
        } // V/v
        17 => {
            if shift || caps {
                0x0057
            } else {
                0x0077
            }
        } // W/w
        45 => {
            if shift || caps {
                0x0058
            } else {
                0x0078
            }
        } // X/x
        21 => {
            if shift || caps {
                0x0059
            } else {
                0x0079
            }
        } // Y/y
        44 => {
            if shift || caps {
                0x005A
            } else {
                0x007A
            }
        } // Z/z

        // Numbers 1-9, 0 (keycodes 2-11)
        2 => {
            if shift {
                0x0021
            } else {
                0x0031
            }
        } // 1/!
        3 => {
            if shift {
                0x0040
            } else {
                0x0032
            }
        } // 2/@
        4 => {
            if shift {
                0x0023
            } else {
                0x0033
            }
        } // 3/#
        5 => {
            if shift {
                0x0024
            } else {
                0x0034
            }
        } // 4/$
        6 => {
            if shift {
                0x0025
            } else {
                0x0035
            }
        } // 5/%
        7 => {
            if shift {
                0x005E
            } else {
                0x0036
            }
        } // 6/^
        8 => {
            if shift {
                0x0026
            } else {
                0x0037
            }
        } // 7/&
        9 => {
            if shift {
                0x002A
            } else {
                0x0038
            }
        } // 8/*
        10 => {
            if shift {
                0x0028
            } else {
                0x0039
            }
        } // 9/(
        11 => {
            if shift {
                0x0029
            } else {
                0x0030
            }
        } // 0/)

        // Special keys
        1 => 0xFF1B,  // Escape
        14 => 0xFF08, // Backspace
        15 => 0xFF09, // Tab
        28 => 0xFF0D, // Enter
        41 => 0x0060, // ` (backtick)
        43 => 0x005C, // \ (backslash)

        // Space
        57 => 0x0020,

        // Arrow keys
        103 => 0xFF52, // Up
        108 => 0xFF54, // Down
        105 => 0xFF51, // Left
        106 => 0xFF53, // Right

        // Function keys F1-F12
        59 => 0xFFBE, // F1
        60 => 0xFFBF, // F2
        61 => 0xFFC0, // F3
        62 => 0xFFC1, // F4
        63 => 0xFFC2, // F5
        64 => 0xFFC3, // F6
        65 => 0xFFC4, // F7
        66 => 0xFFC5, // F8
        67 => 0xFFC6, // F9
        68 => 0xFFC7, // F10
        87 => 0xFFC8, // F11
        88 => 0xFFC9, // F12

        // Modifier keys (return keysym for completeness)
        42 | 54 => 0xFFE1,   // Shift
        29 | 97 => 0xFFE3,   // Ctrl
        56 | 100 => 0xFFE9,  // Alt
        125 | 126 => 0xFFEB, // Meta/Super

        // Delete/Insert/Home/End/PageUp/PageDown
        110 => 0xFF50, // Home
        102 => 0xFF52, // Up (duplicate)
        104 => 0xFF55, // PageUp
        109 => 0xFF51, // Left (duplicate)
        107 => 0xFF53, // Right (duplicate)
        111 => 0xFF57, // End
        116 => 0xFF54, // Down (duplicate)
        117 => 0xFF56, // PageDown
        119 => 0xFFFF, // Delete

        _ => 0x0, // Unknown
    }
}

/// Translate an X11 keysym to a Unicode code point, if it produces a
/// printable character. Simplified version of xkbcommon keysym → UTF-32.
pub fn keysym_to_unicode(keysym: u32, modifier_state: u32) -> Option<u32> {
    let ctrl = modifier_state & vt_modifiers::CTRL != 0;

    if ctrl {
        return None; // Control characters are not printable.
    }

    // ASCII printable range (0x20-0x7E).
    if keysym >= 0x20 && keysym <= 0x7E {
        return Some(keysym);
    }

    // Latin-1 supplement (0xA0-0xFF).
    if keysym >= 0xA0 && keysym <= 0xFF {
        return Some(keysym);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let vt = MetaVtInput::new(1);
        assert_eq!(vt.vt(), 1);
        assert!(!vt.is_active());
    }

    #[test]
    fn test_activate_deactivate() {
        let mut vt = MetaVtInput::new(1);
        vt.activate();
        assert!(vt.is_active());
        vt.deactivate();
        assert!(!vt.is_active());
    }

    #[test]
    fn test_events_when_inactive() {
        let mut vt = MetaVtInput::new(1);
        // Not active — events should be dropped.
        vt.process_keycode(30, true); // 'a'
        assert_eq!(vt.pending_event_count(), 0);
    }

    #[test]
    fn test_keycode_to_keysym_letters() {
        // 'a' keycode 30, no modifiers → lowercase.
        assert_eq!(keycode_to_keysym(30, 0, 0), 0x0061);
        // 'a' with Shift → uppercase.
        assert_eq!(keycode_to_keysym(30, vt_modifiers::SHIFT, 0), 0x0041);
        // 'a' with Caps Lock → uppercase.
        assert_eq!(keycode_to_keysym(30, vt_modifiers::CAPS_LOCK, 0), 0x0041);
    }

    #[test]
    fn test_keycode_to_keysym_numbers() {
        // '1' keycode 2, no modifiers → '1'.
        assert_eq!(keycode_to_keysym(2, 0, 0), 0x0031);
        // '1' with Shift → '!'.
        assert_eq!(keycode_to_keysym(2, vt_modifiers::SHIFT, 0), 0x0021);
    }

    #[test]
    fn test_keycode_to_keysym_special() {
        assert_eq!(keycode_to_keysym(1, 0, 0), 0xFF1B); // Escape
        assert_eq!(keycode_to_keysym(14, 0, 0), 0xFF08); // Backspace
        assert_eq!(keycode_to_keysym(15, 0, 0), 0xFF09); // Tab
        assert_eq!(keycode_to_keysym(28, 0, 0), 0xFF0D); // Enter
        assert_eq!(keycode_to_keysym(57, 0, 0), 0x0020); // Space
    }

    #[test]
    fn test_keycode_to_keysym_arrows() {
        assert_eq!(keycode_to_keysym(103, 0, 0), 0xFF52); // Up
        assert_eq!(keycode_to_keysym(108, 0, 0), 0xFF54); // Down
        assert_eq!(keycode_to_keysym(105, 0, 0), 0xFF51); // Left
        assert_eq!(keycode_to_keysym(106, 0, 0), 0xFF53); // Right
    }

    #[test]
    fn test_keycode_to_keysym_function_keys() {
        assert_eq!(keycode_to_keysym(59, 0, 0), 0xFFBE); // F1
        assert_eq!(keycode_to_keysym(88, 0, 0), 0xFFC9); // F12
    }

    #[test]
    fn test_keysym_to_unicode() {
        assert_eq!(keysym_to_unicode(0x0061, 0), Some(0x0061)); // 'a'
        assert_eq!(keysym_to_unicode(0x0041, 0), Some(0x0041)); // 'A'
        assert_eq!(keysym_to_unicode(0x0020, 0), Some(0x0020)); // Space
        assert_eq!(keysym_to_unicode(0x0031, 0), Some(0x0031)); // '1'
    }

    #[test]
    fn test_keysym_to_unicode_with_ctrl() {
        // Ctrl+a → no unicode (control character).
        assert_eq!(keysym_to_unicode(0x0061, vt_modifiers::CTRL), None);
    }

    #[test]
    fn test_keysym_to_unicode_non_printable() {
        assert_eq!(keysym_to_unicode(0xFF1B, 0), None); // Escape
        assert_eq!(keysym_to_unicode(0xFF0D, 0), None); // Enter
        assert_eq!(keysym_to_unicode(0xFF08, 0), None); // Backspace
    }

    #[test]
    fn test_modifier_tracking() {
        let mut vt = MetaVtInput::new(1);
        vt.activate();

        // Press Shift (keycode 42).
        vt.process_keycode(42, true);
        assert!(vt.modifier_state() & vt_modifiers::SHIFT != 0);

        // Release Shift.
        vt.process_keycode(42, false);
        assert!(vt.modifier_state() & vt_modifiers::SHIFT == 0);
    }

    #[test]
    fn test_caps_lock_toggle() {
        let mut vt = MetaVtInput::new(1);
        vt.activate();

        // Press CapsLock → toggle on.
        vt.process_keycode(58, true);
        assert!(vt.caps_lock());

        // Release CapsLock → stays on.
        vt.process_keycode(58, false);
        assert!(vt.caps_lock());

        // Press again → toggle off.
        vt.process_keycode(58, true);
        assert!(!vt.caps_lock());
    }

    #[test]
    fn test_pending_events_drain() {
        let mut vt = MetaVtInput::new(1);
        vt.activate();

        vt.process_keycode(30, true); // 'a' press
        vt.process_keycode(30, false); // 'a' release
        assert_eq!(vt.pending_event_count(), 2);

        let events = vt.take_pending_events();
        assert_eq!(events.len(), 2);
        assert_eq!(vt.pending_event_count(), 0);

        // First event should be key press.
        assert!(events[0].pressed);
        assert!(!events[1].pressed);
    }

    #[test]
    fn test_deactivate_clears_events() {
        let mut vt = MetaVtInput::new(1);
        vt.activate();
        vt.process_keycode(30, true);
        assert_eq!(vt.pending_event_count(), 1);

        vt.deactivate();
        assert_eq!(vt.pending_event_count(), 0);
    }

    #[test]
    fn test_layout_index() {
        let mut vt = MetaVtInput::new(1);
        assert_eq!(vt.layout_index(), 0);
        vt.set_layout_index(1);
        assert_eq!(vt.layout_index(), 1);
    }
}
