//! Renderer Display Egl Private — EGL renderer display from GNOME Mutter
//!
//! Wraps an EGL display for use with Cogl rendering. Tracks the EGLDisplay
//! handle, the chosen EGLConfig, and initialization state. Actual EGL
//! initialization (`eglInitialize`, `eglChooseConfig`) is documented in
//! `initialize` but not issued here since there is no EGL implementation
//! in `no_std`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-display-egl-private.h

use core::ffi::c_void;

/// EGLDisplay handle (opaque pointer, matches Khronos `EGLDisplay`).
pub type EglDisplayHandle = *mut c_void;
/// EGLConfig handle (opaque pointer, matches Khronos `EGLConfig`).
pub type EglConfigHandle = *mut c_void;

/// Renderer Display Egl Private: EGL-based renderer display.
/// Extends CoglDisplayEGL with Mutter-specific configuration.
/// Note: EGL/Cogl rendering I/O is not available in `no_std`; the EGL
/// handles are tracked as opaque pointers so the display lifecycle
/// (initialize / teardown) ports structurally.
#[derive(Debug, Clone)]
pub struct RendererDisplayEglPrivate {
    /// Parent CoglDisplayEGL (opaque pointer)
    pub cogl_display: *mut c_void,
    /// EGLDisplay handle obtained from `eglGetDisplay`. `null` until
    /// `initialize` is called.
    egl_display: EglDisplayHandle,
    /// EGLConfig chosen via `eglChooseConfig` for the desired framebuffer
    /// config. `null` until `initialize` is called.
    egl_config: EglConfigHandle,
    /// Whether `initialize` has been called successfully.
    is_initialized: bool,
}

impl RendererDisplayEglPrivate {
    pub fn new() -> Self {
        RendererDisplayEglPrivate {
            cogl_display: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            egl_config: core::ptr::null_mut(),
            is_initialized: false,
        }
    }

    /// Returns the EGLDisplay handle, or null if not initialized.
    pub fn get_egl_display(&self) -> EglDisplayHandle {
        self.egl_display
    }

    /// Sets the EGLDisplay handle. Called after `eglGetDisplay` succeeds.
    pub fn set_egl_display(&mut self, display: EglDisplayHandle) {
        self.egl_display = display;
    }

    /// Returns the EGLConfig handle, or null if not initialized.
    pub fn get_egl_config(&self) -> EglConfigHandle {
        self.egl_config
    }

    /// Sets the EGLConfig handle. Called after `eglChooseConfig` selects a
    /// matching framebuffer config.
    pub fn set_egl_config(&mut self, config: EglConfigHandle) {
        self.egl_config = config;
    }

    /// Returns whether the EGL display has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Marks the EGL display as initialized. A full implementation would
    /// call `eglInitialize(egl_display, ...)` and `eglChooseConfig(...)` to
    /// populate `egl_display` and `egl_config` before setting this flag.
    pub fn set_initialized(&mut self, initialized: bool) {
        self.is_initialized = initialized;
    }
}

impl Default for RendererDisplayEglPrivate {
    fn default() -> Self {
        Self::new()
    }
}
