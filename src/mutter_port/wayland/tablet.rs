//! Wayland Tablet module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet.h
//!
//! Represents a tablet input device in Wayland, modeling physical stylus/pen hardware.
//! Manages resource binding and sprite state.

use alloc::string::String;
use alloc::vec::Vec;

/// A tablet input device (stylus, pen) connected to the seat.
pub struct MetaWaylandTablet {
    /// Parent tablet seat managing this tablet.
    pub tablet_seat: *mut core::ffi::c_void,
    /// Clutter input device backing this tablet.
    pub device: *mut core::ffi::c_void,
    /// Sprite (cursor image) for this tablet; may be null.
    pub sprite: *mut core::ffi::c_void,
    /// Linked list of Wayland resources.
    pub resource_list: Vec<*mut core::ffi::c_void>,
    /// Current surface receiving tablet events, if any.
    pub current: *mut core::ffi::c_void,
    /// Device vendor ID (e.g., "WAC").
    pub vendor_id: String,
    /// Device product ID.
    pub product_id: String,
    /// Number of axes on the device.
    pub n_axes: u32,
}

impl MetaWaylandTablet {
    /// Create a new tablet bound to a device and tablet seat. A full
    /// implementation would query the ClutterInputDevice for vendor ID,
    /// product ID, and axis count.
    pub fn new() -> Self {
        Self {
            tablet_seat: core::ptr::null_mut(),
            device: core::ptr::null_mut(),
            sprite: core::ptr::null_mut(),
            resource_list: Vec::new(),
            current: core::ptr::null_mut(),
            vendor_id: String::new(),
            product_id: String::new(),
            n_axes: 0,
        }
    }

    /// Set the device info from ClutterInputDevice introspection.
    pub fn set_device_info(&mut self, vendor: String, product: String, axes: u32) {
        self.vendor_id = vendor;
        self.product_id = product;
        self.n_axes = axes;
    }

    /// Get the vendor ID.
    pub fn get_vendor_id(&self) -> &str {
        &self.vendor_id
    }

    /// Get the product ID.
    pub fn get_product_id(&self) -> &str {
        &self.product_id
    }

    /// Get the number of axes.
    pub fn get_n_axes(&self) -> u32 {
        self.n_axes
    }
}

impl Default for MetaWaylandTablet {
    fn default() -> Self {
        Self::new()
    }
}
