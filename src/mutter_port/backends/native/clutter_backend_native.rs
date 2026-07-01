//! Clutter backend for native display server.
//!
//! MetaClutterBackendNative provides a #ClutterBackend implementation for
//! the native (non-X) backend, creating a stage with #MetaStageNative and
//! rendering using EGL/Cogl.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-clutter-backend-native.c

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use core::ffi::c_void;

/// Clutter backend for native display rendering via EGL.
pub struct ClutterBackendNative {
    /// Parent ClutterBackend (opaque C object)
    pub parent: *mut c_void,
    /// Associated MetaBackend instance
    pub backend: *mut c_void,
    /// Whether the backend has been disposed
    pub disposed: bool,
    /// Hash table of touch input sprites (keyed by event sequence)
    pub touch_sprites: BTreeMap<usize, *mut c_void>,
    /// Hash table of stylus input sprites (keyed by input device)
    pub stylus_sprites: BTreeMap<usize, *mut c_void>,
    /// Pointer sprite instance
    pub pointer_sprite: *mut c_void,
    /// Keyboard focus state
    pub key_focus: *mut c_void,
}

impl ClutterBackendNative {
    /// Create a new ClutterBackendNative instance
    pub fn new() -> Self {
        ClutterBackendNative {
            parent: core::ptr::null_mut(),
            backend: core::ptr::null_mut(),
            disposed: false,
            touch_sprites: BTreeMap::new(),
            stylus_sprites: BTreeMap::new(),
            pointer_sprite: core::ptr::null_mut(),
            key_focus: core::ptr::null_mut(),
        }
    }
}

impl Default for ClutterBackendNative {
    fn default() -> Self {
        Self::new()
    }
}
