//! Stream Source Window — ported from GNOME Mutter
//!
//! MetaStreamSourceWindow provides the actual pixel data capture for a window-based
//! stream, handling rendering and frame recording for individual application windows.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-window.h

use core::ffi::c_void;

/// Base stream source type. Provides interface for pixel data capture.
///
/// Opaque GObject derived type providing frame recording functionality.
pub struct MetaStreamSource {
    // GObject derived base; kept opaque as vtable interface
}

impl MetaStreamSource {
    /// Create a new stream source.
    pub fn new() -> Self {
        MetaStreamSource {}
    }
}

impl Default for MetaStreamSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream window for capturing application window pixels.
///
/// Represents a window-based pixel source for screen recording.
pub struct MetaStreamWindow {
    /// Reference to backend (opaque).
    pub backend: *mut c_void,
    /// Target window pointer (opaque MetaWindow).
    pub window: *mut c_void,
    /// Cursor mode for this stream.
    pub cursor_mode: u32,
    /// Window width in pixels.
    pub width: i32,
    /// Window height in pixels.
    pub height: i32,
}

impl MetaStreamWindow {
    /// Create a new stream window.
    pub fn new() -> Self {
        MetaStreamWindow {
            backend: core::ptr::null_mut(),
            window: core::ptr::null_mut(),
            cursor_mode: 0,
            width: 0,
            height: 0,
        }
    }
}

impl Default for MetaStreamWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// MetaStreamSourceWindow: Pixel source for window captures.
///
/// Extends MetaStreamSource to capture pixels from a specific application window.
pub struct MetaStreamSourceWindow {
    /// Base stream source (opaque parent).
    pub base: *mut c_void,
    /// Pointer to the stream window (opaque MetaStreamWindow).
    pub stream_window: *mut MetaStreamWindow,
}

impl MetaStreamSourceWindow {
    /// Create a new stream source window.
    pub fn new() -> Self {
        MetaStreamSourceWindow {
            base: core::ptr::null_mut(),
            stream_window: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaStreamSourceWindow {
    fn default() -> Self {
        Self::new()
    }
}
