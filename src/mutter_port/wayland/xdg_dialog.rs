//! Wayland XDG Dialog module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-dialog.h
//!
//! Provides xdg-wm-dialog protocol support for window manager interactions.
//! Tracks per-surface dialog state: whether the surface is a modal dialog
//! and its optional parent surface.

/// XDG dialog state for a single surface.
///
/// In the C original, `MetaWaylandXdgDialog` is a GObject that wraps a
/// `zxdg_dialog_v1` resource and tracks the parent toplevel. Here we
/// model the state directly: the modal flag and the parent surface pointer.
#[derive(Debug)]
pub struct MetaWaylandXdgDialog {
    /// Whether this dialog is modal (blocks input to the parent).
    pub modal: bool,
    /// Parent surface pointer (MetaWaylandSurface), or None if unset.
    pub parent: Option<*mut core::ffi::c_void>,
    /// wl_resource pointer for the zxdg_dialog_v1 object.
    pub resource: *mut core::ffi::c_void,
}

impl MetaWaylandXdgDialog {
    /// Create a new dialog state with no parent and modal disabled.
    pub fn new() -> Self {
        MetaWaylandXdgDialog {
            modal: false,
            parent: None,
            resource: core::ptr::null_mut(),
        }
    }

    /// Create a new dialog state bound to a wl_resource.
    pub fn new_with_resource(resource: *mut core::ffi::c_void) -> Self {
        MetaWaylandXdgDialog {
            modal: false,
            parent: None,
            resource,
        }
    }

    /// Set whether this dialog is modal.
    /// A full implementation would also update the window's modal state
    /// in the compositor window manager and potentially grab input.
    pub fn set_modal(&mut self, modal: bool) {
        self.modal = modal;
    }

    /// Check whether this dialog is modal.
    pub fn is_modal(&self) -> bool {
        self.modal
    }

    /// Set the parent surface for this dialog.
    /// A full implementation would also update the parent-child
    /// relationship in the compositor's window stack.
    pub fn set_parent(&mut self, parent: Option<*mut core::ffi::c_void>) {
        self.parent = parent.filter(|&p| !p.is_null());
    }

    /// Get the parent surface pointer, if any.
    pub fn get_parent(&self) -> Option<*mut core::ffi::c_void> {
        self.parent
    }

    /// Get the wl_resource pointer for this dialog.
    pub fn get_resource(&self) -> *mut core::ffi::c_void {
        self.resource
    }

    /// Set the wl_resource pointer for this dialog.
    pub fn set_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource = resource;
    }

    /// Initialize XDG wm-dialog protocol support for the compositor.
    /// A full implementation would call wl_global_create for
    /// zxdg_wm_dialog_v1.
    pub fn init(_compositor: *mut core::ffi::c_void) {
        // Protocol global registration requires libwayland-server.
    }
}

impl Default for MetaWaylandXdgDialog {
    fn default() -> Self {
        Self::new()
    }
}
