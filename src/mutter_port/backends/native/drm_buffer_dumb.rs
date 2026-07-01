//! DRM dumb buffer allocation and management.
//!
//! Allocates simple DRM buffers using dumb buffer ioctl for CPU-writable
//! graphics memory. Supports mmap for direct CPU access and dmabuf export.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-drm-buffer-dumb.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// DRM dumb buffer (simple CPU-accessible allocation)
pub struct DrmBufferDumb {
    /// DRM handle for this buffer
    pub handle: u32,
    /// Mapped CPU-accessible memory pointer
    pub map: *mut core::ffi::c_void,
    /// Size of mapped region
    pub map_size: u64,
    /// Buffer width in pixels
    pub width: i32,
    /// Buffer height in pixels
    pub height: i32,
    /// Row stride in bytes
    pub stride_bytes: i32,
    /// DRM format code (e.g., DRM_FORMAT_XRGB8888)
    pub drm_format: u32,
    /// dmabuf file descriptor (-1 if not exported)
    pub dmabuf_fd: i32,
    /// Offset within buffer
    pub offset: i32,
}

impl DrmBufferDumb {
    pub fn new() -> Self {
        DrmBufferDumb {
            handle: 0,
            map: core::ptr::null_mut(),
            map_size: 0,
            width: 0,
            height: 0,
            stride_bytes: 0,
            drm_format: 0,
            dmabuf_fd: -1,
            offset: 0,
        }
    }
}

impl Default for DrmBufferDumb {
    fn default() -> Self {
        Self::new()
    }
}
