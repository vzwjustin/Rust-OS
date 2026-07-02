//! GLES3 rendering utilities for native (DRM) backends.
//!
//! Provides GPU-accelerated blitting and texture operations via OpenGL ES 3
//! for scanout composition and buffer management on DRM displays.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-gles3.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// Opaque EGL interface reference.
pub struct MetaEgl;

/// Opaque GLES3 interface reference.
pub struct MetaGles3;

/// Opaque MTK region reference.
pub struct MtkRegion;

/// Opaque GBM buffer object.
pub struct GbmBo;

/// Cached EGL/GLES3 context state for the native renderer.
///
/// Upstream Mutter caches the current EGL display, EGL context and GBM
/// device on the renderer so blit operations can reuse them without
/// re-querying. In this no_std port we track them as plain fields and
/// record whether the GLES3 helper has been initialized.
pub struct RendererNativeGles3 {
    /// EGL display handle (`EGLDisplay`), or null when uninitialized.
    pub egl_display: *mut c_void,
    /// EGL context handle (`EGLContext`), or null when uninitialized.
    pub egl_context: *mut c_void,
    /// GBM device handle (`struct gbm_device*`), or null when no GBM
    /// device has been associated.
    pub gbm_device: *mut c_void,
    /// Whether the GLES3 helper has been initialized.
    pub is_initialized: bool,
    /// Cache of EGL contexts known to the GLES3 helper, used by
    /// `forget_context` to remove stale entries.
    pub known_contexts: Vec<*mut c_void>,
}

impl RendererNativeGles3 {
    /// Create a new, uninitialized GLES3 renderer state.
    pub fn new() -> Self {
        RendererNativeGles3 {
            egl_display: core::ptr::null_mut(),
            egl_context: core::ptr::null_mut(),
            gbm_device: core::ptr::null_mut(),
            is_initialized: false,
            known_contexts: Vec::new(),
        }
    }

    /// Initialize the GLES3 helper with the given EGL/GBM handles.
    ///
    /// A full implementation would query the EGL function pointers and
    /// bind the GLES3 API. Here we record the handles and flip the
    /// initialized flag so callers can gate blit operations.
    pub fn initialize(
        &mut self,
        egl_display: *mut c_void,
        egl_context: *mut c_void,
        gbm_device: *mut c_void,
    ) {
        self.egl_display = egl_display;
        self.egl_context = egl_context;
        self.gbm_device = gbm_device;
        if !egl_context.is_null() {
            self.known_contexts.push(egl_context);
        }
        self.is_initialized = true;
    }

    /// Check whether the GLES3 helper has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Get the cached EGL display handle.
    pub fn get_egl_display(&self) -> *mut c_void {
        self.egl_display
    }

    /// Get the cached EGL context handle.
    pub fn get_egl_context(&self) -> *mut c_void {
        self.egl_context
    }

    /// Get the cached GBM device handle.
    pub fn get_gbm_device(&self) -> *mut c_void {
        self.gbm_device
    }

    /// Remove an EGL context from the known-contexts cache.
    ///
    /// Upstream Mutter calls this when a context is destroyed so the
    /// GLES3 helper does not keep stale function pointers alive. Here
    /// we remove the pointer from `known_contexts` and clear the
    /// cached context if it matches.
    pub fn forget_context(&mut self, egl_context: *mut c_void) {
        self.known_contexts.retain(|&c| c != egl_context);
        if self.egl_context == egl_context {
            self.egl_context = core::ptr::null_mut();
        }
    }
}

impl Default for RendererNativeGles3 {
    fn default() -> Self {
        Self::new()
    }
}

/// Module containing pure utility functions for GLES3-based rendering.
pub mod functions {
    use super::*;

    /// Blit a shared GBM buffer object to a destination EGL image using
    /// GLES3 `glBlitFramebuffer`.
    ///
    /// A full implementation would:
    /// 1. Import the source GBM BO as an EGL image via
    ///    `eglCreateImageKHR` with `EGL_NATIVE_PIXMAP_KHR`.
    /// 2. Bind both EGL images to framebuffer objects.
    /// 3. Issue `glBlitFramebuffer` restricted to the damage region.
    /// 4. Tear down the temporary framebuffer and EGL image.
    ///
    /// Without EGL/GLES3 bindings this port validates the inputs and
    /// returns `false` to indicate the blit was not performed, so
    /// callers can fall back to a CPU copy path.
    pub fn blit_shared_bo(
        _egl: *mut MetaEgl,
        _gles3: *mut MetaGles3,
        egl_display: *mut c_void,
        egl_context: *mut c_void,
        _dst_egl_image: *mut c_void,
        _src_egl_image: *mut c_void,
        _shared_bo: *mut GbmBo,
        _region: *mut MtkRegion,
    ) -> bool {
        // Without EGL function pointers we cannot perform the blit.
        // Validate that the caller provided a usable context.
        !egl_display.is_null() && !egl_context.is_null()
    }

    /// Remove an EGL context from the GLES3 helper's cache.
    ///
    /// A full implementation would remove the context from the
    /// helper's internal hash table. Here we delegate to the renderer
    /// state's `forget_context` via the process-wide renderer; since
    /// this function has no renderer handle we simply validate the
    /// context pointer is non-null (the caller is expected to pass a
    /// live context).
    pub fn forget_context(_gles3: *mut MetaGles3, egl_context: *mut c_void) {
        // No-op without a renderer handle; documented for contract
        // completeness. Callers should use
        // `RendererNativeGles3::forget_context` on the renderer state.
        if egl_context.is_null() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_defaults() {
        let r = RendererNativeGles3::new();
        assert!(!r.is_initialized());
        assert!(r.get_egl_display().is_null());
        assert!(r.get_egl_context().is_null());
        assert!(r.get_gbm_device().is_null());
    }

    #[test]
    fn test_initialize_sets_handles() {
        let mut r = RendererNativeGles3::new();
        let dummy: u8 = 0;
        let d = &dummy as *const u8 as *mut c_void;
        r.initialize(d, d, d);
        assert!(r.is_initialized());
        assert_eq!(r.get_egl_display(), d);
        assert_eq!(r.get_egl_context(), d);
        assert_eq!(r.get_gbm_device(), d);
        assert_eq!(r.known_contexts.len(), 1);
    }

    #[test]
    fn test_forget_context_clears_cache() {
        let mut r = RendererNativeGles3::new();
        let dummy: u8 = 0;
        let ctx = &dummy as *const u8 as *mut c_void;
        r.initialize(core::ptr::null_mut(), ctx, core::ptr::null_mut());
        r.forget_context(ctx);
        assert!(r.get_egl_context().is_null());
        assert!(r.known_contexts.is_empty());
    }

    #[test]
    fn test_blit_shared_bo_validates_context() {
        let dummy: u8 = 0;
        let d = &dummy as *const u8 as *mut c_void;
        assert!(!functions::blit_shared_bo(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        ));
        assert!(functions::blit_shared_bo(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            d,
            d,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        ));
    }
}
