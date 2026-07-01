//! Native sprite implementation for Clutter input handling.
//!
//! Provides a MetaSprite subclass for managing cursor/input device rendering
//! in the native backend, integrating with input device tracking.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-sprite-native.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Native sprite for input device rendering (cursor, touch point, tablet).
pub struct SpriteNative {
    /// Parent MetaSprite (opaque C object)
    pub parent_instance: *mut c_void,
}

impl SpriteNative {
    pub fn new() -> Self {
        SpriteNative {
            parent_instance: core::ptr::null_mut(),
        }
    }
}

impl Default for SpriteNative {
    fn default() -> Self {
        Self::new()
    }
}
