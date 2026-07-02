//! Render Device Private — internal state for rendering devices
//!
//! Holds the private data for a MetaRenderDevice: backend reference,
//! device file, EGL display, and hardware acceleration detection flag.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device.c

/// Render Device Private — opaque private data for MetaRenderDevice.
#[derive(Debug, Clone)]
pub struct RenderDevicePrivate {
    /// Backend reference (MetaBackend *)
    pub backend: *mut core::ffi::c_void,
    /// Device file handle (MetaDeviceFile *)
    pub device_file: *mut core::ffi::c_void,
    /// EGL display handle
    pub egl_display: *mut core::ffi::c_void,
    /// Whether GPU rendering is hardware-accelerated (vs. software)
    pub is_hardware_rendering: u32,
}

impl RenderDevicePrivate {
    pub fn new() -> Self {
        RenderDevicePrivate {
            backend: core::ptr::null_mut(),
            device_file: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            is_hardware_rendering: 0,
        }
    }
}

impl Default for RenderDevicePrivate {
    fn default() -> Self {
        Self::new()
    }
}
