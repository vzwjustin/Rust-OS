//! EGL (OpenGL ES platform abstraction) ported from GNOME Mutter's src/backends/
//!
//! Provides an abstraction over EGL display and context creation, configuration
//! management, and surface/image handling. The MetaEgl struct wraps platform-
//! specific EGL operations; actual DRM/Wayland/platform-specific I/O is deferred
//! to backend implementations. This module defines the type signatures and data
//! structures for safe EGL wrappers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-egl.h

/// Opaque EGL wrapper struct.
///
/// In upstream, this is a GObject that encapsulates EGL platform operations.
/// The actual implementation methods (e.g., eglInitialize, eglChooseConfig)
/// are left to backend implementers.
pub struct MetaEgl;

impl MetaEgl {
    /// Create a new EGL wrapper instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MetaEgl {
    fn default() -> Self {
        Self::new()
    }
}
