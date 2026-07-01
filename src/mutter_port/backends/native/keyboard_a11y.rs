//! Keyboard accessibility features (a11y).
//!
//! Handles keyboard accessibility settings including slow keys, sticky keys,
//! bounce keys, and mouse keys functionality for improved keyboard interaction.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-keyboard-a11y.c

use alloc::{boxed::Box, collections::LinkedList, string::String, vec::Vec};
use core::ffi::c_void;

/// Keyboard accessibility settings flags
pub type MetaKeyboardA11yFlags = u32;

pub const META_A11Y_SLOWKEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 0;
pub const META_A11Y_BOUNCE_KEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 1;
pub const META_A11Y_TOGGLE_KEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 2;
pub const META_A11Y_STICKY_KEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 3;
pub const META_A11Y_MOUSE_KEYS_ENABLED: MetaKeyboardA11yFlags = 1 << 4;

/// Keyboard accessibility handler for slow keys, sticky keys, bounce keys, mouse keys.
pub struct KeyboardA11y {
    /// Parent GObject (opaque)
    pub parent_instance: *mut c_void,
    /// Associated seat implementation
    pub seat_impl: *mut c_void,
    /// Active accessibility flags
    pub a11y_flags: MetaKeyboardA11yFlags,
    /// Virtual input device for mouse keys
    pub mousekeys_pointer: *mut c_void,
    /// List of pending slow key events
    pub slow_keys_list: LinkedList<*mut c_void>,
    /// Debounce timer source
    pub debounce_timer: *mut c_void,
    /// Key code under debounce
    pub debounce_key: u16,
    /// XKB modifier mask for sticky keys (depressed)
    pub stickykeys_depressed_mask: u32,
    /// XKB modifier mask for sticky keys (latched)
    pub stickykeys_latched_mask: u32,
    /// XKB modifier mask for sticky keys (locked)
    pub stickykeys_locked_mask: u32,
    /// Timer for slow keys toggle
    pub toggle_slowkeys_timer: *mut c_void,
    /// Shift key press counter (for slow keys)
    pub shift_count: u16,
    /// Timestamp of last shift press (ms)
    pub last_shift_time: u32,
    /// Current mouse button being emulated (0-2: left/middle/right)
    pub mousekeys_btn: i32,
    /// Button press state for mouse keys (3 buttons)
    pub mousekeys_btn_states: [bool; 3],
    /// Initial mouse keys motion time (ms)
    pub mousekeys_first_motion_time: u32,
    /// Last mouse keys motion time (ms)
    pub mousekeys_last_motion_time: u32,
    /// Mouse keys initial delay (ms)
    pub mousekeys_init_delay: u32,
    /// Mouse keys acceleration time (ms)
    pub mousekeys_accel_time: u32,
    /// Mouse keys maximum speed (pixels/s)
    pub mousekeys_max_speed: u32,
    /// Mouse keys acceleration curve factor
    pub mousekeys_curve_factor: f64,
    /// Timer for mouse key motion
    pub move_mousekeys_timer: *mut c_void,
    /// Last key code triggering mouse keys
    pub last_mousekeys_key: u16,
}

impl KeyboardA11y {
    pub fn new() -> Self {
        KeyboardA11y {
            parent_instance: core::ptr::null_mut(),
            seat_impl: core::ptr::null_mut(),
            a11y_flags: 0,
            mousekeys_pointer: core::ptr::null_mut(),
            slow_keys_list: LinkedList::new(),
            debounce_timer: core::ptr::null_mut(),
            debounce_key: 0,
            stickykeys_depressed_mask: 0,
            stickykeys_latched_mask: 0,
            stickykeys_locked_mask: 0,
            toggle_slowkeys_timer: core::ptr::null_mut(),
            shift_count: 0,
            last_shift_time: 0,
            mousekeys_btn: -1,
            mousekeys_btn_states: [false; 3],
            mousekeys_first_motion_time: 0,
            mousekeys_last_motion_time: 0,
            mousekeys_init_delay: 0,
            mousekeys_accel_time: 0,
            mousekeys_max_speed: 0,
            mousekeys_curve_factor: 0.0,
            move_mousekeys_timer: core::ptr::null_mut(),
            last_mousekeys_key: 0,
        }
    }
}

impl Default for KeyboardA11y {
    fn default() -> Self {
        Self::new()
    }
}
