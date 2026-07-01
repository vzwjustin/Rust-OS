//! Native seat implementation for GNOME Mutter.
//!
//! Manages input devices (keyboards, mice, touchscreens) and their event handling.
//! Maintains keyboard maps, virtual device slots, cursor renderers, and device lists.
//! Core abstraction for input subsystem in native backends.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-seat-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Flag type for seat configuration.
pub type MetaSeatNativeFlag = u32;

/// Native implementation of input seat.
pub struct SeatNative {
    /// Reference to backend (opaque C handle).
    pub backend: *mut c_void,
    /// Seat implementation (opaque C handle).
    pub impl_seat: *mut c_void,
    /// Seat identifier string.
    pub seat_id: Option<String>,
    /// Configuration flags.
    pub flags: MetaSeatNativeFlag,
    /// List of input devices (opaque C handle).
    pub devices: *mut c_void,
    /// XKB keymap (opaque C handle).
    pub xkb_keymap: *mut c_void,
    /// Current XKB layout index.
    pub xkb_layout_index: u32,
    /// Keymap description (opaque C handle).
    pub keymap_description: *mut c_void,
    /// Virtual touch slot base.
    pub virtual_touch_slot_base: u32,
    /// Reserved virtual slots (opaque C handle).
    pub reserved_virtual_slots: *mut c_void,
    /// Keymap (opaque C handle).
    pub keymap: *mut c_void,
    /// Cursor renderer (opaque C handle).
    pub cursor_renderer: *mut c_void,
    /// Secondary cursor renderers (opaque C handle).
    pub secondary_cursor_renderers: *mut c_void,
    /// Whether seat has been released.
    pub released: bool,
    /// Touch mode enabled.
    pub touch_mode: bool,
}

impl SeatNative {
    /// Create a new native seat.
    pub fn new() -> Self {
        SeatNative {
            backend: core::ptr::null_mut(),
            impl_seat: core::ptr::null_mut(),
            seat_id: None,
            flags: 0,
            devices: core::ptr::null_mut(),
            xkb_keymap: core::ptr::null_mut(),
            xkb_layout_index: 0,
            keymap_description: core::ptr::null_mut(),
            virtual_touch_slot_base: 0,
            reserved_virtual_slots: core::ptr::null_mut(),
            keymap: core::ptr::null_mut(),
            cursor_renderer: core::ptr::null_mut(),
            secondary_cursor_renderers: core::ptr::null_mut(),
            released: false,
            touch_mode: false,
        }
    }
}

impl Default for SeatNative {
    fn default() -> Self {
        Self::new()
    }
}
