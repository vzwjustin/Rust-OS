//! Wayland Inhibit Shortcuts Dialog module
//!
//! Manages keyboard shortcut inhibition for Wayland surfaces, allowing clients
//! to request that keyboard shortcuts (Alt+Tab, Super, etc.) be inhibited and passed
//! to the application instead of the window manager.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-inhibit-shortcuts-dialog.h

/// Show the inhibit shortcuts dialog for a surface.
///
/// Presents a dialog asking the user to permit or deny keyboard shortcut inhibition.
///
/// ponytail: real impl requires UI dialog integration and user confirmation
pub fn meta_wayland_surface_show_inhibit_shortcuts_dialog(
    _surface: *mut core::ffi::c_void,
    _seat: *mut core::ffi::c_void,
) {
}

/// Cancel the inhibit shortcuts dialog for a surface.
///
/// Hides any pending dialog and reverts shortcut inhibition request.
///
/// ponytail: real impl cleans up dialog state and reverts inhibition
pub fn meta_wayland_surface_cancel_inhibit_shortcuts_dialog(_surface: *mut core::ffi::c_void) {}

/// Initialize inhibit shortcuts dialog support.
///
/// Registers protocol binding and handler for inhibit_shortcuts interface.
///
/// ponytail: register inhibit_shortcuts protocol; real impl binds protocol
pub fn meta_wayland_surface_inhibit_shortcuts_dialog_init() {}
