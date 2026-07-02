//! Cursor Renderer for native (DRM) backends.
//!
//! Manages hardware cursor rendering and synchronization for DRM displays.
//! Inherits from MetaCursorRenderer and provides platform-specific cursor
//! rendering via hardware cursor planes or software blitting.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-cursor-renderer-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Opaque backend reference.
pub struct MetaBackend;

/// Opaque renderer view reference.
pub struct MetaRendererView;

/// Opaque Clutter frame reference.
pub struct ClutterFrame;

/// Native cursor renderer state for DRM displays.
///
/// Extends MetaCursorRenderer with hardware-specific cursor plane management.
pub struct CursorRendererNative {
    /// Reference to the backend (opaque).
    pub backend: *mut MetaBackend,
    /// Current Clutter cursor (opaque).
    pub current_cursor: *mut c_void,
    /// Handle ID for texture change callbacks.
    pub texture_changed_handler_id: u64,
    /// Animation timeout ID for cursor updates.
    pub animation_timeout_id: u32,
    /// Handler ID for pointer position changes.
    pub pointer_position_changed_handler_id: u64,
    /// Flag indicating input is disconnected.
    pub input_disconnected: bool,
}

impl CursorRendererNative {
    /// Create a new cursor renderer for the native backend.
    pub fn new() -> Self {
        CursorRendererNative {
            backend: core::ptr::null_mut(),
            current_cursor: core::ptr::null_mut(),
            texture_changed_handler_id: 0,
            animation_timeout_id: 0,
            pointer_position_changed_handler_id: 0,
            input_disconnected: false,
        }
    }
}

impl Default for CursorRendererNative {
    fn default() -> Self {
        Self::new()
    }
}
