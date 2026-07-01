//! GBM-based render device for hardware-accelerated rendering.
//!
//! Wraps GBM (Generic Buffer Management) device handles for DRM-based graphics
//! rendering. Manages GPU memory and off-screen buffer allocation for
//! display composition.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device-gbm.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Opaque backend reference.
pub struct MetaBackend;

/// Opaque GBM device (struct gbm_device).
pub struct GbmDevice;

/// GBM render device for GPU-accelerated composition.
///
/// Inherits from MetaRenderDevice and provides GPU buffer management
/// via the GBM interface.
pub struct RenderDeviceGbm {
    /// Associated backend (opaque).
    pub backend: *mut MetaBackend,
    /// Underlying GBM device handle (opaque).
    pub gbm_device: *mut GbmDevice,
}

impl RenderDeviceGbm {
    /// Create a new GBM render device.
    pub fn new() -> Self {
        RenderDeviceGbm {
            backend: core::ptr::null_mut(),
            gbm_device: core::ptr::null_mut(),
        }
    }
}

impl Default for RenderDeviceGbm {
    fn default() -> Self {
        Self::new()
    }
}
