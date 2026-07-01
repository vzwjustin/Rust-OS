//! DRM Buffer Import ported from GNOME Mutter's src/backends/
//!
//! Provides DRM buffer creation from external DMA-buf file descriptors.
//! Allows importing buffers from other subsystems (e.g., Wayland, GPU drivers).
//! The buffer metadata (fd, dimensions, format, modifier) is tracked locally;
//! the actual DRM `DRM_IOCTL_PRIME_FD_TO_HANDLE` ioctl to obtain a GEM handle
//! is documented in `import_fd` but not issued here since there is no DRM
//! file descriptor in `no_std`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-import.h
//! Upstream header not found; minimal port.

/// DRM framebuffer format fourcc (e.g. `DRM_FORMAT_XRGB8888`).
pub type DrmFormat = u32;
/// DRM buffer modifier (e.g. `DRM_FORMAT_MOD_LINEAR`); `0` means linear/none.
pub type DrmModifier = u64;
/// GEM buffer handle returned by `DRM_IOCTL_PRIME_FD_TO_HANDLE`.
pub type DrmGemHandle = u32;

/// Imported DRM buffer created from an external DMA-buf file descriptor.
///
/// Tracks the imported fd, buffer dimensions, pixel format, tiling modifier,
/// and the GEM handle assigned by the kernel. A full implementation would
/// call `DRM_IOCTL_PRIME_FD_TO_HANDLE` to convert the fd to a GEM handle and
/// `DRM_IOCTL_MODE_CREATE_DUMB` / `drmModeAddFB2WithModifiers` to register a
/// framebuffer.
pub struct DrmBufferImport {
    /// DMA-buf file descriptor (-1 if not yet imported).
    fd: i32,
    /// Buffer width in pixels.
    width: u32,
    /// Buffer height in pixels.
    height: u32,
    /// Pixel format fourcc.
    format: DrmFormat,
    /// Layout modifier (tiling/compression).
    modifier: DrmModifier,
    /// GEM handle (0 until the fd is imported via ioctl).
    handle: DrmGemHandle,
}

impl DrmBufferImport {
    /// Create a new imported DRM buffer with the given metadata. The fd is
    /// stored but not yet converted to a GEM handle; `set_handle` is called
    /// after the `DRM_IOCTL_PRIME_FD_TO_HANDLE` ioctl succeeds.
    pub fn new(fd: i32, width: u32, height: u32, format: DrmFormat, modifier: DrmModifier) -> Self {
        DrmBufferImport {
            fd,
            width,
            height,
            format,
            modifier,
            handle: 0,
        }
    }

    /// Returns the DMA-buf file descriptor.
    pub fn get_fd(&self) -> i32 {
        self.fd
    }

    /// Sets the DMA-buf file descriptor.
    pub fn set_fd(&mut self, fd: i32) {
        self.fd = fd;
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

    /// Returns the pixel format fourcc.
    pub fn get_format(&self) -> DrmFormat {
        self.format
    }

    /// Sets the pixel format fourcc.
    pub fn set_format(&mut self, format: DrmFormat) {
        self.format = format;
    }

    /// Returns the layout modifier.
    pub fn get_modifier(&self) -> DrmModifier {
        self.modifier
    }

    /// Sets the layout modifier.
    pub fn set_modifier(&mut self, modifier: DrmModifier) {
        self.modifier = modifier;
    }

    /// Returns the GEM handle (0 if not yet imported).
    pub fn get_handle(&self) -> DrmGemHandle {
        self.handle
    }

    /// Sets the GEM handle. Called after `DRM_IOCTL_PRIME_FD_TO_HANDLE`
    /// converts the fd to a kernel GEM handle.
    pub fn set_handle(&mut self, handle: DrmGemHandle) {
        self.handle = handle;
    }
}

impl Default for DrmBufferImport {
    fn default() -> Self {
        DrmBufferImport {
            fd: -1,
            width: 0,
            height: 0,
            format: 0,
            modifier: 0,
            handle: 0,
        }
    }
}
