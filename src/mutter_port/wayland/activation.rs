//! Wayland Activation module
//!
//! Handles xdg-activation protocol for application startup tokens.
//! Clients can request tokens to launch windows and get visual feedback.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-activation.h

use alloc::{string::String, vec::Vec};

/// Represents an XDG activation token for application startup.
/// Associates a token string with a surface, seat, and optional startup sequence.
pub struct MetaXdgActivationToken {
    pub surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub seat: Option<*mut core::ffi::c_void>,    // MetaWaylandSeat pointer
    pub activation: Option<*mut core::ffi::c_void>, // MetaWaylandActivation pointer
    pub sequence: Option<*mut core::ffi::c_void>,   // MetaStartupSequence pointer
    pub app_id: Option<String>,
    pub token: Option<String>,
    pub serial: u32,
    pub committed: bool,
}

/// Manages XDG activation tokens for the Wayland compositor.
/// Tracks token lifecycle, pending activations, and resource allocation.
pub struct MetaWaylandActivation {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub token_list: Vec<*mut core::ffi::c_void>,
    pub tokens: Option<*mut core::ffi::c_void>, // GHashTable
    pub pending_activations: Option<*mut core::ffi::c_void>, // GHashTable
}

impl MetaWaylandActivation {
    /// Create a new activation handler (stub).
    pub fn new() -> Self {
        MetaWaylandActivation {
            compositor: None,
            resource_list: Vec::new(),
            token_list: Vec::new(),
            tokens: None,
            pending_activations: None,
        }
    }

    /// Initialize wayland activation support for the compositor.
    /// TODO: wire up token protocol handlers
    pub fn init(_compositor: *mut core::ffi::c_void) {
    }

    /// Finalize wayland activation support for the compositor.
    /// TODO: clean up token resources
    pub fn finalize(_compositor: *mut core::ffi::c_void) {
    }
}

impl Default for MetaWaylandActivation {
    fn default() -> Self {
        Self::new()
    }
}
