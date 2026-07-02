//! Renderer Context Egl — EGL rendering context from GNOME Mutter
//!
//! Represents an OpenGL ES rendering context managed via EGL (OpenGL ES Initialization).
//! Encapsulates context lifecycle, surface binding, and draw/read framebuffer state.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-private.h

/// Renderer Context Egl — an EGL/GLES rendering context.
#[derive(Debug, Clone)]
pub struct RendererContextEgl {
    /// EGL context handle (EGLContext)
    pub egl_context: *mut core::ffi::c_void,
    /// EGL display (EGLDisplay)
    pub egl_display: *mut core::ffi::c_void,
    /// EGL config (EGLConfig)
    pub egl_config: *mut core::ffi::c_void,
    /// GLES3 API capabilities (MetaGles3 *)
    pub gles3: *mut core::ffi::c_void,
    /// Current draw surface (EGLSurface), or NO_SURFACE if unbound
    pub draw_surface: *mut core::ffi::c_void,
    /// Current read surface (EGLSurface), or NO_SURFACE if unbound
    pub read_surface: *mut core::ffi::c_void,
}

impl RendererContextEgl {
    pub fn new() -> Self {
        RendererContextEgl {
            egl_context: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            egl_config: core::ptr::null_mut(),
            gles3: core::ptr::null_mut(),
            draw_surface: core::ptr::null_mut(),
            read_surface: core::ptr::null_mut(),
        }
    }
}

impl Default for RendererContextEgl {
    fn default() -> Self {
        Self::new()
    }
}
