//! Wayland XDG Dialog module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-dialog.h
//!
//! Provides xdg-wm-dialog protocol support for window manager interactions.
//! Protocol binding and dialog event handling are TODO.

/// Placeholder unit type for XDG dialog (wm-dialog) support in the compositor.
pub struct MetaWaylandXdgDialog;

impl MetaWaylandXdgDialog {
    /// Initialize XDG wm-dialog protocol support for the compositor.
    /// TODO: protocol binding and resource creation.
    pub fn init(_compositor: *mut core::ffi::c_void) {
        // Protocol binding deferred to backend implementation.
    }
}
