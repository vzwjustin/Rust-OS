//! Renderer Display Egl — EGL display management from GNOME Mutter
//!
//! Wraps an EGL display (initialized from a render device), managing
//! surface creation, swapping, and version capabilities.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-private.h

/// Renderer Display Egl — an EGL display instance.
#[derive(Debug, Clone)]
pub struct RendererDisplayEgl {
    /// EGL display handle (EGLDisplay)
    pub egl_display: *mut core::ffi::c_void,
    /// Associated render device (MetaRenderDevice *)
    pub render_device: *mut core::ffi::c_void,
    /// EGL version supported (major * 100 + minor, e.g., 115 for EGL 1.15)
    pub egl_version: u32,
}

impl RendererDisplayEgl {
    pub fn new() -> Self {
        RendererDisplayEgl {
            egl_display: core::ptr::null_mut(),
            render_device: core::ptr::null_mut(),
            egl_version: 0,
        }
    }
}

impl Default for RendererDisplayEgl {
    fn default() -> Self {
        Self::new()
    }
}
