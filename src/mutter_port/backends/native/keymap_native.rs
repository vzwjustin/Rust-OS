//! Native keymap management for input devices.
//!
//! Extends Clutter's keymap with native input device keymap handling via XKB.
//! Manages keyboard layout state and keycode translation for the native backend.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-keymap-native.h

use alloc::{boxed::Box, string::String, vec::Vec};

/// Native keymap state handler.
///
/// Wraps XKB keymap state and provides keycode translation for native input.
pub struct KeymapNative {
    // XKB state managed internally; minimal exposed fields per GObject pattern
    /// Placeholder for keymap state (opaque).
    _state: *mut core::ffi::c_void,
}

impl KeymapNative {
    /// Create a new native keymap.
    pub fn new() -> Self {
        KeymapNative {
            _state: core::ptr::null_mut(),
        }
    }
}

impl Default for KeymapNative {
    fn default() -> Self {
        Self::new()
    }
}
