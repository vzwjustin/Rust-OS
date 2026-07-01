//! Wayland buffer management ported from GNOME Mutter's src/wayland/meta-wayland-buffer.c
//!
//! Implements MetaWaylandBuffer which represents a wl_buffer resource, handling different
//! buffer types (SHM, EGL image, DMA-BUF, single-pixel) and their associated textures.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-buffer.c

use alloc::string::String;
use alloc::vec::Vec;
use core::option::Option;

/// Buffer type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaWaylandBufferType {
    /// Unknown or uninitialized buffer type.
    Unknown,
    /// Shared memory (wl_shm) buffer.
    Shm,
    /// EGL image buffer.
    EglImage,
    /// Linux DMA-BUF buffer.
    DmaBuf,
    /// Single-pixel solid color buffer.
    SinglePixel,
}

/// Represents a wl_buffer resource from a Wayland client.
#[derive(Debug)]
pub struct MetaWaylandBuffer {
    /// Unique resource ID (from Wayland protocol).
    resource_id: u32,

    /// Buffer type classification.
    buffer_type: MetaWaylandBufferType,

    /// Width in pixels.
    width: u32,

    /// Height in pixels.
    height: u32,

    /// Stride in bytes (for SHM buffers).
    stride: Option<u32>,

    /// Format code (DRM fourcc for DMA-BUF, SHM format enum, etc).
    format: u32,

    /// Reference count for lifecycle management.
    ref_count: u32,

    /// Whether the buffer has been used (for tracking stale buffers).
    used: bool,

    /// Associated DMA-BUF metadata (if applicable).
    dma_buf_buffer: Option<*mut ()>,

    /// Associated single-pixel buffer data (if applicable).
    single_pixel_buffer: Option<*mut ()>,
}

impl MetaWaylandBuffer {
    /// Create a new Wayland buffer resource.
    pub fn new(
        resource_id: u32,
        buffer_type: MetaWaylandBufferType,
        width: u32,
        height: u32,
        format: u32,
    ) -> Self {
        MetaWaylandBuffer {
            resource_id,
            buffer_type,
            width,
            height,
            stride: None,
            format,
            ref_count: 1,
            used: false,
            dma_buf_buffer: None,
            single_pixel_buffer: None,
        }
    }

    /// Get the resource ID.
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Get the buffer type.
    pub fn buffer_type(&self) -> MetaWaylandBufferType {
        self.buffer_type
    }

    /// Get buffer width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get buffer height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get buffer stride (for SHM buffers).
    pub fn stride(&self) -> Option<u32> {
        self.stride
    }

    /// Set buffer stride.
    pub fn set_stride(&mut self, stride: u32) {
        self.stride = Some(stride);
    }

    /// Get buffer format code.
    pub fn format(&self) -> u32 {
        self.format
    }

    /// Increment reference count.
    pub fn ref_acquire(&mut self) {
        self.ref_count = self.ref_count.saturating_add(1);
    }

    /// Decrement reference count and return true if this was the last reference.
    pub fn ref_release(&mut self) -> bool {
        if self.ref_count > 0 {
            self.ref_count -= 1;
        }
        self.ref_count == 0
    }

    /// Get current reference count.
    pub fn ref_count(&self) -> u32 {
        self.ref_count
    }

    /// Mark buffer as used (has been rendered).
    pub fn mark_used(&mut self) {
        self.used = true;
    }

    /// Check if buffer has been used.
    pub fn is_used(&self) -> bool {
        self.used
    }

    /// STUB: Get texture representation of buffer.
    /// In the C version, this handles EGL texture creation, DMA-BUF import, etc.
    pub fn get_texture(&self) -> Option<*mut ()> {
        // STUB: texture creation from buffer data requires GPU integration
        // Should handle:
        // - Cogl texture creation
        // - DMA-BUF format mapping
        // - Single-pixel buffer color extraction
        // - EGL image -> texture binding
        None
    }

    /// STUB: Get DRM format representation (for scanout).
    /// Used for direct display/scanout optimization.
    pub fn get_drm_format(&self) -> Option<u32> {
        // STUB: format conversion and scanout capability checking
        None
    }

    /// STUB: Perform scanout on a specific CRTC/view.
    /// Optimizes display by directly scanning out buffer without composition.
    pub fn can_scanout(&self) -> bool {
        // STUB: scanout eligibility checking (contiguous memory, format support, etc)
        matches!(self.buffer_type, MetaWaylandBufferType::DmaBuf)
    }
}

/// Error type for buffer operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferError {
    /// Invalid buffer resource.
    InvalidResource,
    /// Unsupported buffer format.
    UnsupportedFormat,
    /// Failed to import buffer (e.g., DMA-BUF import).
    ImportFailed,
    /// Buffer is no longer available.
    Destroyed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_creation() {
        let buffer = MetaWaylandBuffer::new(1, MetaWaylandBufferType::Shm, 1920, 1080, 0);
        assert_eq!(buffer.resource_id(), 1);
        assert_eq!(buffer.width(), 1920);
        assert_eq!(buffer.height(), 1080);
        assert_eq!(buffer.buffer_type(), MetaWaylandBufferType::Shm);
    }

    #[test]
    fn buffer_ref_counting() {
        let mut buffer = MetaWaylandBuffer::new(1, MetaWaylandBufferType::Shm, 1920, 1080, 0);
        assert_eq!(buffer.ref_count(), 1);

        buffer.ref_acquire();
        assert_eq!(buffer.ref_count(), 2);

        assert!(!buffer.ref_release());
        assert_eq!(buffer.ref_count(), 1);

        assert!(buffer.ref_release());
        assert_eq!(buffer.ref_count(), 0);
    }

    #[test]
    fn buffer_usage_tracking() {
        let mut buffer = MetaWaylandBuffer::new(1, MetaWaylandBufferType::Shm, 1920, 1080, 0);
        assert!(!buffer.is_used());

        buffer.mark_used();
        assert!(buffer.is_used());
    }
}
