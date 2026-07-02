//! Stream Window — ported from GNOME Mutter
//!
//! MetaStreamWindow represents a screen capture stream for a single application window.
//! It captures the contents of a specific window and any child windows with automatic
//! resize handling and damage tracking.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-window.h

use super::stream::MetaStream;
use core::ffi::c_void;

/// Opaque window object from meta/window.h
pub struct MetaWindow;

/// MetaStreamWindow: Captures a single application window.
///
/// Extends MetaStream with window-specific metadata, including window reference,
/// dimensions, and damage notification callbacks.
pub struct MetaStreamWindow {
    /// Base stream data (cursor mode, sources, configuration).
    pub base: MetaStream,
    /// Pointer to captured MetaWindow (opaque).
    pub window: *mut MetaWindow,
    /// Cached window width in pixels.
    pub width: i32,
    /// Cached window height in pixels.
    pub height: i32,
    /// Signal handler ID for window damage updates (opaque GSignalHandler).
    pub damage_handler_id: u64,
    /// Signal handler ID for window position/size changes.
    pub size_changed_handler_id: u64,
}

impl MetaStreamWindow {
    pub fn new() -> Self {
        MetaStreamWindow {
            base: MetaStream::new(),
            window: core::ptr::null_mut(),
            width: 0,
            height: 0,
            damage_handler_id: 0,
            size_changed_handler_id: 0,
        }
    }
}

impl Default for MetaStreamWindow {
    fn default() -> Self {
        Self::new()
    }
}
