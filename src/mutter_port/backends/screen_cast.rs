//! Screen Cast — ported from GNOME Mutter
//!
//! Core screen casting functionality for remote access, including cursor mode control
//! and recording flags for streaming audio/video frame capture.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// Cursor rendering mode for screen cast sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaScreenCastCursorMode {
    /// Cursor is not rendered in the cast stream.
    META_SCREEN_CAST_CURSOR_MODE_HIDDEN = 0,
    /// Cursor is embedded in the video frames.
    META_SCREEN_CAST_CURSOR_MODE_EMBEDDED = 1,
    /// Cursor position/shape is sent as metadata separately.
    META_SCREEN_CAST_CURSOR_MODE_METADATA = 2,
}

/// Flags controlling screen cast behavior.
pub const META_SCREEN_CAST_FLAG_NONE: u32 = 0;
pub const META_SCREEN_CAST_FLAG_IS_RECORDING: u32 = 1 << 0;
pub const META_SCREEN_CAST_FLAG_IS_PLATFORM: u32 = 1 << 1;

/// Main screen cast object managing sessions and stream permissions.
pub struct MetaScreenCast {
    /// Render device for screen cast operations (opaque).
    pub screen_cast_device: *mut c_void,
}

impl MetaScreenCast {
    /// Create a new MetaScreenCast instance.
    pub fn new() -> Self {
        MetaScreenCast {
            screen_cast_device: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaScreenCast {
    fn default() -> Self {
        Self::new()
    }
}