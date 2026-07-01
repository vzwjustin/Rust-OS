//! Wayland Data Device Primary module
//!
//! Manages primary selection (middle-click paste) for Wayland clients.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-device-primary.h

use alloc::{string::String, vec::Vec};
use core::ffi::c_void;

/// Primary selection device manages primary clipboard data and offers for Wayland clients.
pub struct MetaWaylandDataDevicePrimary {
    /// Associated Wayland seat
    pub seat: Option<*mut c_void>,
    /// Serial number for selection updates
    pub serial: u32,
    /// Current primary selection data source
    pub data_source: Option<*mut c_void>,
    /// List of all bound resource handles
    pub resource_list: Vec<*mut c_void>,
    /// List of focused client resource handles
    pub focus_resource_list: Vec<*mut c_void>,
    /// Currently focused Wayland client
    pub focus_client: Option<*mut c_void>,
    /// Selection owner source
    pub owner: Option<*mut c_void>,
}

impl MetaWaylandDataDevicePrimary {
    /// Create a new primary data device instance
    pub fn new() -> Self {
        Self {
            seat: None,
            serial: 0,
            data_source: None,
            resource_list: Vec::new(),
            focus_resource_list: Vec::new(),
            focus_client: None,
            owner: None,
        }
    }

    /// Initialize the data device primary manager for the compositor
    /// TODO: Register primary selection protocol and bind resources
    pub fn manager_init(_compositor: *mut c_void) {
        // TODO: implement
    }

    /// Set selection for a focused surface
    /// TODO: Update primary selection and notify clients
    pub fn set_focus(&mut self, _surface: *mut c_void) {
        // TODO: implement
    }
}

impl Default for MetaWaylandDataDevicePrimary {
    fn default() -> Self {
        Self::new()
    }
}
