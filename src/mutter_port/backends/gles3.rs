//! Gles3 — OpenGL ES 3.0 renderer wrapper from GNOME Mutter
//!
//! Provides a GObject wrapper around EGL/GLES3, managing function pointers,
//! extension detection, and error state. GL/EGL I/O is left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-gles3.h

use alloc::boxed::Box;

/// Opaque EGL context type.
pub struct MetaEgl;

/// Opaque GLES3 function table type.
pub struct MetaGles3Table;

/// MetaGles3 — Wrapper for EGL context and GLES3 function pointers.
/// Tracks extension availability and GL error state.
pub struct MetaGles3 {
    pub egl: *mut MetaEgl,
    pub table: *mut MetaGles3Table,
}

impl MetaGles3 {
    /// Create a new GLES3 wrapper with a given EGL context.
    pub fn new(egl: *mut MetaEgl) -> Self {
        MetaGles3 {
            egl,
            table: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaGles3 {
    fn default() -> Self {
        MetaGles3 {
            egl: core::ptr::null_mut(),
            table: core::ptr::null_mut(),
        }
    }
}
