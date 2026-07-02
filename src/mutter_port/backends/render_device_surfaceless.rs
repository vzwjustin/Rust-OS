//! Render Device Surfaceless — EGL surfaceless rendering from GNOME Mutter
//!
//! A render device that uses EGL with the surfaceless platform extension,
//! avoiding the need for a GBM device or direct framebuffer access.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device-surfaceless.c

/// Render Device Surfaceless — uses EGL_MESA_platform_surfaceless.
/// Inherits EGLDisplay and device file from parent MetaRenderDevice.
#[derive(Debug, Clone)]
pub struct RenderDeviceSurfaceless;

impl RenderDeviceSurfaceless {
    pub fn new() -> Self {
        RenderDeviceSurfaceless
    }
}

impl Default for RenderDeviceSurfaceless {
    fn default() -> Self {
        Self::new()
    }
}
