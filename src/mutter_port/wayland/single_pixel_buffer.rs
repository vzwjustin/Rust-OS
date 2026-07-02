//! Wayland Single Pixel Buffer module
//!
//! Handles single-pixel buffers for efficient solid-color surfaces via the
//! wp_single_pixel_buffer protocol. Optimizes rendering for solid backgrounds
//! and UI elements by encoding color directly without allocating full buffers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-single-pixel-buffer.h

/// Single-pixel buffer with RGBA color representation.
///
/// Encodes a solid color directly without allocating full pixel buffers.
/// Color values are stored as normalized 32-bit unsigned integers.
#[derive(Debug, Clone, Copy)]
pub struct MetaWaylandSinglePixelBuffer {
    /// Red channel (0..=u32::MAX normalized to 0.0..=1.0).
    pub r: u32,
    /// Green channel (0..=u32::MAX normalized to 0.0..=1.0).
    pub g: u32,
    /// Blue channel (0..=u32::MAX normalized to 0.0..=1.0).
    pub b: u32,
    /// Alpha channel (0..=u32::MAX normalized to 0.0..=1.0).
    pub a: u32,
}

impl MetaWaylandSinglePixelBuffer {
    /// Create a new single-pixel buffer with the given RGBA values.
    pub fn new(r: u32, g: u32, b: u32, a: u32) -> Self {
        MetaWaylandSinglePixelBuffer { r, g, b, a }
    }
}

impl Default for MetaWaylandSinglePixelBuffer {
    fn default() -> Self {
        MetaWaylandSinglePixelBuffer::new(0, 0, 0, u32::MAX)
    }
}

/// Attach a single pixel buffer to a surface, returning a texture.
///
/// Converts a single-pixel buffer to a texture for rendering.
///
/// ponytail: real impl creates texture from color; stub returns false
pub fn meta_wayland_single_pixel_buffer_attach(
    _buffer: *mut core::ffi::c_void,
    _texture: *mut *mut core::ffi::c_void,
) -> bool {
    false
}

/// Get single pixel buffer from a buffer.
///
/// Returns the opaque buffer handle if the buffer is a single-pixel buffer.
///
/// ponytail: real impl unwraps single-pixel buffer; stub returns None
pub fn meta_wayland_single_pixel_buffer_from_buffer(
    _buffer: *mut core::ffi::c_void,
) -> Option<*mut MetaWaylandSinglePixelBuffer> {
    None
}

/// Initialize single pixel buffer manager.
///
/// Registers the wp_single_pixel_buffer protocol and manager with the compositor.
///
/// ponytail: register protocol; real impl binds protocol handler
pub fn meta_wayland_init_single_pixel_buffer_manager(_compositor: *mut core::ffi::c_void) {}

/// Free a single pixel buffer.
///
/// Deallocates the buffer and its associated color data.
///
/// ponytail: cleanup deallocates buffer; stub is no-op (stack-allocated)
pub fn meta_wayland_single_pixel_buffer_free(_buffer: *mut MetaWaylandSinglePixelBuffer) {}

/// Check if buffer is opaque black.
///
/// Returns true if the buffer represents a fully opaque black color (0xFF000000).
///
/// ponytail: real impl checks RGBA == (0, 0, 0, MAX); stub conservatively returns false
pub fn meta_wayland_single_pixel_buffer_is_opaque_black(
    buffer: *mut MetaWaylandSinglePixelBuffer,
) -> bool {
    if buffer.is_null() {
        return false;
    }
    unsafe {
        let buf = &*buffer;
        buf.r == 0 && buf.g == 0 && buf.b == 0 && buf.a == u32::MAX
    }
}
