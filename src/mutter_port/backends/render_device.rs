//! Render Device — abstract rendering hardware interface from GNOME Mutter
//!
//! Provides a common interface for render devices (GBM, surfaceless, etc.),
//! managing EGL display, device files, DMA-buf allocation, and format queries.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device.h

/// Render Device — abstract base for rendering backends.
/// Wraps EGL display, device file, and hardware acceleration state.
#[derive(Debug, Clone)]
pub struct RenderDevice {
    /// Backend reference (MetaBackend *)
    pub backend: *mut core::ffi::c_void,
    /// Device file (MetaDeviceFile *)
    pub device_file: *mut core::ffi::c_void,
    /// EGL display handle
    pub egl_display: *mut core::ffi::c_void,
    /// Hardware acceleration flag
    pub is_hardware_rendering: u32,
}

impl RenderDevice {
    pub fn new() -> Self {
        RenderDevice {
            backend: core::ptr::null_mut(),
            device_file: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            is_hardware_rendering: 0,
        }
    }
}

impl Default for RenderDevice {
    fn default() -> Self {
        Self::new()
    }
}
