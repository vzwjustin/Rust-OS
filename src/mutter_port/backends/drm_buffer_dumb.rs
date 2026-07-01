//! DRM Buffer Dumb ported from GNOME Mutter's src/backends/
//!
//! Provides support for simple dumb framebuffer creation via DRM IOCTL.
//! The buffer metadata (handle, dimensions, bpp, pitch, size) is tracked
//! locally; the actual `DRM_IOCTL_MODE_CREATE_DUMB` and `drmModeMapDumb`
//! ioctls to allocate and map the buffer are documented in the methods but
//! not issued here since there is no DRM file descriptor in `no_std`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-dumb.h
//! Upstream header not found; minimal port.

/// GEM buffer handle returned by `DRM_IOCTL_MODE_CREATE_DUMB`.
pub type DrmGemHandle = u32;

/// Dumb DRM buffer created via the kernel dumb-buffer ioctl.
///
/// Tracks the GEM handle, dimensions, bits-per-pixel, pitch (row stride in
/// bytes), and total size. A full implementation would call
/// `DRM_IOCTL_MODE_CREATE_DUMB` to allocate the buffer and
/// `DRM_IOCTL_MODE_MAP_DUMB` to obtain an mmap offset for CPU access.
pub struct DrmBufferDumb {
    /// GEM handle (0 until the dumb buffer is created via ioctl).
    handle: DrmGemHandle,
    /// Buffer width in pixels.
    width: u32,
    /// Buffer height in pixels.
    height: u32,
    /// Bits per pixel.
    bpp: u32,
    /// Pitch (row stride in bytes) returned by the kernel.
    pitch: u32,
    /// Total buffer size in bytes.
    size: u64,
}

impl DrmBufferDumb {
    /// Create a new dumb buffer descriptor with the given dimensions and
    /// bits-per-pixel. The handle and pitch are populated after the
    /// `DRM_IOCTL_MODE_CREATE_DUMB` ioctl succeeds.
    pub fn new(width: u32, height: u32, bpp: u32) -> Self {
        DrmBufferDumb {
            handle: 0,
            width,
            height,
            bpp,
            pitch: 0,
            size: 0,
        }
    }

    /// Returns the GEM handle (0 if not yet created).
    pub fn get_handle(&self) -> DrmGemHandle {
        self.handle
    }

    /// Sets the GEM handle. Called after `DRM_IOCTL_MODE_CREATE_DUMB`.
    pub fn set_handle(&mut self, handle: DrmGemHandle) {
        self.handle = handle;
    }

    /// Returns the buffer width in pixels.
    pub fn get_width(&self) -> u32 {
        self.width
    }

    /// Sets the buffer width.
    pub fn set_width(&mut self, width: u32) {
        self.width = width;
    }

    /// Returns the buffer height in pixels.
    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Sets the buffer height.
    pub fn set_height(&mut self, height: u32) {
        self.height = height;
    }

    /// Returns the bits per pixel.
    pub fn get_bpp(&self) -> u32 {
        self.bpp
    }

    /// Sets the bits per pixel.
    pub fn set_bpp(&mut self, bpp: u32) {
        self.bpp = bpp;
    }

    /// Returns the pitch (row stride in bytes).
    pub fn get_pitch(&self) -> u32 {
        self.pitch
    }

    /// Sets the pitch. Called after `DRM_IOCTL_MODE_CREATE_DUMB` returns
    /// the kernel-computed row stride.
    pub fn set_pitch(&mut self, pitch: u32) {
        self.pitch = pitch;
    }

    /// Returns the total buffer size in bytes.
    pub fn get_size(&self) -> u64 {
        self.size
    }

    /// Sets the total buffer size. Typically `pitch * height`.
    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }
}

impl Default for DrmBufferDumb {
    fn default() -> Self {
        DrmBufferDumb {
            handle: 0,
            width: 0,
            height: 0,
            bpp: 32,
            pitch: 0,
            size: 0,
        }
    }
}
