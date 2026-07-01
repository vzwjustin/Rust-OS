//! Eis Client — Per-client EIS connection handler from GNOME Mutter
//!
//! Wraps a libeis client connection and processes EIS events.
//! Manages virtual input devices, keymaps, and viewport synchronization.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-client.h

use alloc::collections::BTreeMap;
use core::ffi::c_void;

/// Opaque EIS manager reference.
pub struct MetaEis;

/// Opaque EIS device reference (libeis device wrapper).
pub struct MetaEisDevice;

/// Opaque EIS viewport reference.
pub struct MetaEisViewport;

/// MetaEisClient — Per-client EIS connection.
/// Processes inbound events from a libeis client socket, managing virtual devices,
/// keymaps, and viewport state per connected EIS client.
pub struct MetaEisClient {
    /// Reference to the parent EIS manager (opaque).
    pub eis: *mut MetaEis,
    /// Opaque libeis client connection.
    pub eis_client: *mut c_void,
    /// Opaque libeis seat for this client.
    pub eis_seat: *mut c_void,
    /// Hash table mapping eis_device pointers to MetaEisDevice wrappers.
    pub eis_devices: BTreeMap<*mut c_void, *mut MetaEisDevice>,
    /// Cached pointer (mouse) device for this client.
    pub pointer_device: *mut MetaEisDevice,
    /// Cached keyboard device for this client.
    pub keyboard_device: *mut MetaEisDevice,
    /// Handler ID for keymap change signal subscription.
    pub keymap_changed_handler_id: u64,
    /// Handler ID for keymap state change signal subscription.
    pub keymap_state_changed_handler_id: u64,
    /// Flag: whether client has absolute position pointer devices.
    pub have_abs_pointer_devices: bool,
    /// Flag: whether client has touch input devices.
    pub have_touch_devices: bool,
    /// Handler ID for viewport changes signal subscription.
    pub viewports_changed_handler_id: u64,
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
