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
/// TODO: port logic from meta_wayland_surface_show_inhibit_shortcuts_dialog, UI integration
pub fn meta_wayland_surface_show_inhibit_shortcuts_dialog(
    _surface: *mut core::ffi::c_void,
    _seat: *mut core::ffi::c_void,
) {
    // TODO: implement
}

/// Cancel the inhibit shortcuts dialog for a surface.
///
/// Hides any pending dialog and reverts shortcut inhibition request.
///
/// TODO: port logic from meta_wayland_surface_cancel_inhibit_shortcuts_dialog
pub fn meta_wayland_surface_cancel_inhibit_shortcuts_dialog(_surface: *mut core::ffi::c_void) {
    // TODO: implement
}

/// Initialize inhibit shortcuts dialog support.
///
/// Registers protocol binding and handler for inhibit_shortcuts interface.
///
/// TODO: port logic from meta_wayland_surface_inhibit_shortcuts_dialog_init
pub fn meta_wayland_surface_inhibit_shortcuts_dialog_init() {
    // TODO: implement
}
