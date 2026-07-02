//! Render Device Gbm — GBM-based rendering from GNOME Mutter
//!
//! A render device backed by GBM (Generic Buffer Management),
//! supporting DMA-buf allocation and import for hardware-accelerated rendering.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device-gbm.c

/// Render Device Gbm — wraps a GBM device for buffer management.
#[derive(Debug, Clone)]
pub struct RenderDeviceGbm {
    /// Opaque GBM device handle (from gbm.h: struct gbm_device)
    pub gbm_device: *mut core::ffi::c_void,
}

impl RenderDeviceGbm {
    pub fn new() -> Self {
        RenderDeviceGbm {
            gbm_device: core::ptr::null_mut(),
        }
    }
}

impl Default for RenderDeviceGbm {
    fn default() -> Self {
        Self::new()
    }
}
