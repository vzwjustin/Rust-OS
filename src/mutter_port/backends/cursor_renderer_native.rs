//! Cursor Renderer Native ported from GNOME Mutter's src/backends/
//!
//! Native backend cursor renderer using hardware cursors via DRM/KMS.
//! Manages hardware cursor planes and animation timers for native display outputs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-renderer-native.c

use alloc::boxed::Box;
use core::ffi::c_void;

/// Opaque backend reference.
pub struct MetaBackend;

/// Opaque Clutter cursor reference.
pub struct ClutterCursor;

/// Native DRM/KMS hardware cursor renderer.
pub struct MetaCursorRendererNative {
    /// Reference to the backend (opaque).
    pub backend: *mut MetaBackend,
    /// Current Clutter cursor object.
    pub current_cursor: *mut ClutterCursor,
    /// Signal handler ID for texture changes.
    pub texture_changed_handler_id: u64,
    /// Animation timeout ID for cursor updates.
    pub animation_timeout_id: u32,
    /// Signal handler ID for pointer position changes.
    pub pointer_position_changed_handler_id: u64,
    /// Flag indicating input thread is disconnected.
    pub input_disconnected: bool,
}

impl MetaCursorRendererNative {
    /// Create a new native cursor renderer.
    pub fn new() -> Self {
        MetaCursorRendererNative {
            backend: core::ptr::null_mut(),
            current_cursor: core::ptr::null_mut(),
            texture_changed_handler_id: 0,
            animation_timeout_id: 0,
            pointer_position_changed_handler_id: 0,
            input_disconnected: false,
        }
    }

    /// Prepare cursor frame for renderer view.
    pub fn prepare_frame(&mut self) {
        // TODO: Render cursor sprite to hardware buffer
    }
}

impl Default for MetaCursorRendererNative {
    fn default() -> Self {
        Self::new()
    }
}
