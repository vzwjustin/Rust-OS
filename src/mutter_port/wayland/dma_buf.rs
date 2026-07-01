//! Wayland DMA Buf module
//!
//! Manages DMA buffers (DMA-BUF protocol) for efficient zero-copy buffer sharing
//! between Wayland clients and the compositor. Handles buffer attachment, texture mapping,
//! and scanout acquisition for hardware-accelerated display.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-dma-buf.h

use alloc::boxed::Box;

/// Callback for DMA buf source operations.
///
/// Called with a buffer and user data when DMA buf operation completes.
pub type MetaWaylandDmaBufSourceDispatch =
    Option<unsafe extern "C" fn(*mut core::ffi::c_void, *mut core::ffi::c_void)>;

/// DMA buffer with associated texture and metadata.
///
/// Represents a DMA-backed buffer shared by a Wayland client.
#[derive(Debug)]
pub struct MetaWaylandDmaBufBuffer {
    pub buffer: Option<*mut core::ffi::c_void>, // MetaWaylandBuffer pointer
    pub texture: Option<*mut core::ffi::c_void>, // MetaMultiTexture pointer
}

impl MetaWaylandDmaBufBuffer {
    pub fn new() -> Self {
        MetaWaylandDmaBufBuffer {
            buffer: None,
            texture: None,
        }
    }
}

impl Default for MetaWaylandDmaBufBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for DMA buffer protocol and buffers on a compositor.
///
/// Creates and manages MetaWaylandDmaBufBuffer instances, handles DRM device discovery,
/// and coordinates format/modifier negotiation with clients.
#[derive(Debug)]
pub struct MetaWaylandDmaBufManager {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
}

impl MetaWaylandDmaBufManager {
    /// Create a new DMA buf manager for the compositor.
    ///
    /// ponytail: real impl enumerates DRM devices and registers buffer handler
    pub fn new(compositor: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        let manager = Box::new(MetaWaylandDmaBufManager {
            compositor: Some(compositor),
        });
        Some(Box::into_raw(manager) as *mut core::ffi::c_void)
    }
}

impl Default for MetaWaylandDmaBufManager {
    fn default() -> Self {
        MetaWaylandDmaBufManager { compositor: None }
    }
}

/// Attach a DMA buf buffer to a surface, returning a texture.
///
/// ponytail: real impl creates texture from FDs; stub returns false
pub fn meta_wayland_dma_buf_buffer_attach(
    _buffer: *mut core::ffi::c_void,
    _texture: *mut *mut core::ffi::c_void,
) -> bool {
    false
}

/// Get DMA buf FDs for a wayland buffer.
///
/// ponytail: real impl retrieves FDs from buffer; stub returns None
pub fn meta_wayland_dma_buf_fds_for_wayland_buffer(
    _buffer: *mut core::ffi::c_void,
) -> Option<*mut MetaWaylandDmaBufBuffer> {
    None
}

/// Get DMA buf from a buffer.
///
/// ponytail: real impl unwraps DMA buffer from Wayland buffer; stub returns None
pub fn meta_wayland_dma_buf_from_buffer(
    _buffer: *mut core::ffi::c_void,
) -> Option<*mut MetaWaylandDmaBufBuffer> {
    None
}

/// Create a GSource for DMA buf operations.
///
/// ponytail: real impl creates event loop source for buffer readiness; stub returns None
pub fn meta_wayland_dma_buf_create_source(
    _buffer: *mut core::ffi::c_void,
    _dispatch: MetaWaylandDmaBufSourceDispatch,
    _user_data: *mut core::ffi::c_void,
) -> Option<*mut core::ffi::c_void> {
    None
}

/// Try to acquire scanout for a buffer (direct display without composition).
///
/// ponytail: real impl acquires scanout for direct display; stub returns None
pub fn meta_wayland_dma_buf_try_acquire_scanout(
    _buffer: *mut core::ffi::c_void,
    _onscreen: *mut core::ffi::c_void,
    _stage_view: *mut core::ffi::c_void,
) -> Option<*mut core::ffi::c_void> {
    None
}
