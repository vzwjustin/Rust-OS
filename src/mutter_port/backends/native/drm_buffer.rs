//! DRM buffer (framebuffer) abstraction.
//!
//! Represents graphics memory buffers managed by the DRM subsystem.
//! Ported from `meta-drm-buffer.c`.

/// DRM buffer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrmBufferType {
    /// Dumb buffer (simple linear memory)
    Dumb,
    /// GBM (Graphics Buffer Manager) allocated
    GBM,
    /// Imported from external source
    Imported,
}

/// DRM buffer object
#[derive(Debug, Clone)]
pub struct DrmBuffer {
    /// Buffer type
    pub buffer_type: DrmBufferType,
    /// DRM framebuffer ID
    pub fb_id: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel format (DRM fourcc code)
    pub format: u32,
    /// Pitch/stride in bytes
    pub stride: u32,
    /// Handle/pointer to buffer memory
    pub handle: u64,
}

impl DrmBuffer {
    /// Create a new DRM buffer
    pub fn new(
        buffer_type: DrmBufferType,
        fb_id: u32,
        width: u32,
        height: u32,
        format: u32,
        stride: u32,
        handle: u64,
    ) -> Self {
        DrmBuffer {
            buffer_type,
            fb_id,
            width,
            height,
            format,
            stride,
            handle,
        }
    }

    /// Get buffer dimensions
    pub fn get_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get pitch (stride)
    pub fn get_stride(&self) -> u32 {
        self.stride
    }

    /// Get buffer size in bytes
    pub fn get_byte_size(&self) -> u64 {
        (self.stride as u64) * (self.height as u64)
    }

    /// Check if this is a dumb buffer
    pub fn is_dumb(&self) -> bool {
        self.buffer_type == DrmBufferType::Dumb
    }

    /// Check if this is a GBM buffer
    pub fn is_gbm(&self) -> bool {
        self.buffer_type == DrmBufferType::GBM
    }

    /// Get buffer framebuffer ID
    pub fn get_fb_id(&self) -> u32 {
        self.fb_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_creation() {
        let buffer = DrmBuffer::new(DrmBufferType::Dumb, 1, 1920, 1080, 0x34325241, 7680, 0x1000);
        assert_eq!(buffer.fb_id, 1);
        assert_eq!(buffer.width, 1920);
        assert_eq!(buffer.height, 1080);
    }

    #[test]
    fn test_buffer_size() {
        let buffer = DrmBuffer::new(DrmBufferType::Dumb, 1, 1920, 1080, 0x34325241, 7680, 0x1000);
        let (w, h) = buffer.get_size();
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn test_byte_size() {
        let buffer = DrmBuffer::new(DrmBufferType::Dumb, 1, 1920, 1080, 0x34325241, 7680, 0x1000);
        // 7680 * 1080 = 8294400 bytes
        assert_eq!(buffer.get_byte_size(), 8294400);
    }

    #[test]
    fn test_buffer_type() {
        let dumb = DrmBuffer::new(DrmBufferType::Dumb, 1, 1920, 1080, 0x34325241, 7680, 0x1000);
        assert!(dumb.is_dumb());
        assert!(!dumb.is_gbm());

        let gbm = DrmBuffer::new(DrmBufferType::GBM, 1, 1920, 1080, 0x34325241, 7680, 0x1000);
        assert!(!gbm.is_dumb());
        assert!(gbm.is_gbm());
    }
}
