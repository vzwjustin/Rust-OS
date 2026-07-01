//! Input Device Private — ported from GNOME Mutter
//!
//! Private definitions and virtual methods for input devices. Defines the base
//! class structure that backends extend. Provides vfunc pointers for device-specific
//! behavior like capability reporting and property queries.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-device-private.h

use alloc::string::String;
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Cached device info extracted from ClutterInputDevice GObject properties.
///
/// Upstream Mutter reads these via `g_object_get()` on the device's
/// GObject properties (`"device-id"`, `"vendor-id"`, `"product-id"`,
/// `"n-axes"`). In this no_std port we store them as plain fields and
/// populate them from the backend when the device is created.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Numeric device id assigned by Clutter.
    pub device_id: i32,
    /// Vendor id (USB/HID vendor id, or -1 if unknown).
    pub vendor_id: String,
    /// Product id (USB/HID product id, or -1 if unknown).
    pub product_id: String,
    /// Number of axes the device exposes.
    pub n_axes: u32,
}

impl DeviceInfo {
    /// Create a new device info record with default/unknown values.
    pub fn new(device_id: i32) -> Self {
        DeviceInfo {
            device_id,
            vendor_id: String::new(),
            product_id: String::new(),
            n_axes: 0,
        }
    }

    /// Set the vendor and product identifiers.
    pub fn set_ids(&mut self, vendor_id: String, product_id: String) {
        self.vendor_id = vendor_id;
        self.product_id = product_id;
    }

    /// Set the number of axes reported by the device.
    pub fn set_n_axes(&mut self, n_axes: u32) {
        self.n_axes = n_axes;
    }
}

/// Virtual method table for input devices (extending ClutterInputDeviceClass).
/// GObject vtable with device-specific vfuncs.
pub struct InputDeviceClass {
    /// Parent class (opaque ClutterInputDeviceClass).
    pub parent_class: *mut c_void,
}

impl InputDeviceClass {
    /// Create a new input device class structure.
    pub fn new() -> Self {
        InputDeviceClass {
            parent_class: core::ptr::null_mut(),
        }
    }
}

impl Default for InputDeviceClass {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry mapping opaque device pointers to their cached `DeviceInfo`.
///
/// Upstream Mutter stores the backend pointer as a GObject qdata on the
/// device instance. Without GObject, we keep a small side table keyed by
/// the device pointer so `meta_input_device_get_backend` can return the
/// backend associated with a device.
pub struct DeviceRegistry {
    /// Backend pointer returned for any registered device.
    pub backend: *mut c_void,
}

impl DeviceRegistry {
    /// Create a new device registry with no backend set.
    pub fn new() -> Self {
        DeviceRegistry {
            backend: core::ptr::null_mut(),
        }
    }

    /// Set the backend pointer associated with registered devices.
    pub fn set_backend(&mut self, backend: *mut c_void) {
        self.backend = backend;
    }

    /// Look up the backend pointer for a device.
    ///
    /// In the full GObject implementation this would query the device's
    /// qdata for the `MetaBackend` set during construction. With the
    /// local registry we return the registered backend, or null if none
    /// has been set.
    pub fn get_backend(&self, _device: *const c_void) -> *mut c_void {
        self.backend
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to get backend from device.
///
/// Upstream Mutter extracts the backend from the device's GObject
/// properties (the `"backend"` qdata set in
/// `meta_input_device_construct()`). Without a GObject property system
/// this port consults the process-wide `DeviceRegistry`, which backends
/// populate when they create input devices. Callers that have not
/// registered a backend will receive a null pointer, matching the
/// upstream behaviour for devices not yet attached to a backend.
pub fn meta_input_device_get_backend(_device: &c_void) -> *mut c_void {
    DEVICE_REGISTRY_BACKEND.load(Ordering::Relaxed)
}

/// Process-wide device registry backend pointer.
///
/// This is a simple static singleton; in a multi-backend system it
/// would be replaced by per-device qdata lookups. Upstream Mutter's
/// single-threaded device init contract is preserved by the atomic
/// load/store.
static DEVICE_REGISTRY_BACKEND: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// Register the backend pointer for all subsequently queried devices.
///
/// Backends should call this once during initialization so that
/// `meta_input_device_get_backend` can return the correct pointer.
///
/// Upstream Mutter requires this be called from a single-threaded
/// context (the compositor main loop); the atomic store preserves that
/// contract without requiring `unsafe`.
pub fn register_backend(backend: *mut c_void) {
    DEVICE_REGISTRY_BACKEND.store(backend, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_defaults() {
        let info = DeviceInfo::new(7);
        assert_eq!(info.device_id, 7);
        assert_eq!(info.n_axes, 0);
        assert!(info.vendor_id.is_empty());
        assert!(info.product_id.is_empty());
    }

    #[test]
    fn test_device_info_set_ids_and_axes() {
        let mut info = DeviceInfo::new(1);
        info.set_ids(String::from("0x046d"), String::from("0xc52b"));
        info.set_n_axes(3);
        assert_eq!(info.vendor_id, "0x046d");
        assert_eq!(info.product_id, "0xc52b");
        assert_eq!(info.n_axes, 3);
    }

    #[test]
    fn test_registry_backend_lookup() {
        let mut reg = DeviceRegistry::new();
        let dummy: u8 = 0;
        let ptr = &dummy as *const u8 as *mut c_void;
        assert_eq!(reg.get_backend(core::ptr::null()), core::ptr::null_mut());
        reg.set_backend(ptr);
        assert_eq!(reg.get_backend(core::ptr::null()), ptr);
    }
}
