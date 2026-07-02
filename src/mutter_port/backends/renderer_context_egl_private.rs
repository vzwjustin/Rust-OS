//! Renderer Context Egl Private — internal EGL rendering context state
//!
//! Holds the private data for an EGL rendering context: the EGL context handle,
//! associated display, config, and API bindings.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-private.h

/// Renderer Context Egl Private — opaque private EGL context data.
#[derive(Debug, Clone)]
pub struct RendererContextEglPrivate {
    /// EGL context handle (EGLContext)
    pub egl_context: *mut core::ffi::c_void,
    /// EGL display handle (EGLDisplay)
    pub egl_display: *mut core::ffi::c_void,
    /// EGL config (EGLConfig)
    pub egl_config: *mut core::ffi::c_void,
    /// API state / capabilities (MetaGles3 *)
    pub gles3: *mut core::ffi::c_void,
}

impl RendererContextEglPrivate {
    pub fn new() -> Self {
        RendererContextEglPrivate {
            egl_context: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            egl_config: core::ptr::null_mut(),
            gles3: core::ptr::null_mut(),
        }
    }
}

impl Default for RendererContextEglPrivate {
    fn default() -> Self {
        Self::new()
    }
}
