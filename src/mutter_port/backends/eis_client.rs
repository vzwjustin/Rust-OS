//! Eis Client — Per-client EIS connection handler from GNOME Mutter
//!
//! Wraps a libeis client connection and processes EIS events.
//! Event parsing and device routing are left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-client.h

use alloc::collections::BTreeMap;

pub struct MetaEis {
    // Opaque parent type
}

pub struct MetaEisDevice {
    // Opaque device type
}

pub struct MetaEisViewport {
    // Opaque viewport type
}

/// MetaEisClient — Per-client EIS connection.
/// Processes inbound events from a libeis client socket.
pub struct MetaEisClient {
    pub eis: *mut MetaEis,
    pub eis_client: *mut core::ffi::c_void,
    pub eis_seat: *mut core::ffi::c_void,
    pub eis_devices: BTreeMap<usize, *mut MetaEisDevice>,
    pub pointer_device: *mut MetaEisDevice,
    pub keyboard_device: *mut MetaEisDevice,
    pub keymap_changed_handler_id: usize,
    pub keymap_state_changed_handler_id: usize,
    pub have_abs_pointer_devices: bool,
    pub have_touch_devices: bool,
    pub viewports_changed_handler_id: usize,
}

impl MetaEisClient {
    pub fn new() -> Self {
        MetaEisClient {
            eis: core::ptr::null_mut(),
            eis_client: core::ptr::null_mut(),
            eis_seat: core::ptr::null_mut(),
            eis_devices: BTreeMap::new(),
            pointer_device: core::ptr::null_mut(),
            keyboard_device: core::ptr::null_mut(),
            keymap_changed_handler_id: 0,
            keymap_state_changed_handler_id: 0,
            have_abs_pointer_devices: false,
            have_touch_devices: false,
            viewports_changed_handler_id: 0,
        }
    }
}

impl Default for MetaEisClient {
    fn default() -> Self {
        Self::new()
    }
}
