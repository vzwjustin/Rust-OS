//! Wayland GTK Shell module
//!
//! Implements GTK-specific shell extensions for window management and theming.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-gtk-shell.h

use core::ffi::c_void;

/// GTK shell protocol support for GTK client integrations.
pub struct MetaWaylandGtkShell {
    /// Associated Wayland compositor
    pub compositor: Option<*mut c_void>,
}

impl MetaWaylandGtkShell {
    /// Create a new GTK shell instance
    pub fn new() -> Self {
        Self { compositor: None }
    }

    /// Initialize GTK shell support for the compositor
    /// ponytail: register gtk_shell1 protocol globally if needed, but no-op in this kernel stub
    pub fn init(_compositor: *mut c_void) {}
}

impl Default for MetaWaylandGtkShell {
    fn default() -> Self {
        Self::new()
    }
}
