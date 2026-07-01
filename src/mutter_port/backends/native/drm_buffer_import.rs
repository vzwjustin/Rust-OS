//! DRM Buffer Import ported from GNOME Mutter.
//!
//! Represents a DRM buffer that has been imported from another device via dma_buf.
//! Keeps a reference to the originating GBM buffer to prevent premature cleanup.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-drm-buffer-import.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Imported DRM buffer with reference to originating GBM buffer.
pub struct DrmBufferImport {
    /// Reference to the originating GBM buffer (opaque C handle).
    pub importee: *mut c_void,
}

impl DrmBufferImport {
    /// Create a new imported DRM buffer.
    pub fn new() -> Self {
        DrmBufferImport {
            importee: core::ptr::null_mut(),
        }
    }
}

impl Default for DrmBufferImport {
    fn default() -> Self {
        Self::new()
    }
}
