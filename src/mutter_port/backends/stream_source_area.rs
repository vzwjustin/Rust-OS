//! Stream Source Area — ported from GNOME Mutter
//!
//! MetaStreamSourceArea provides the actual pixel data capture for an area-based
//! stream, handling rendering and frame recording for a rectangular region.
//! Tracks cursor state, damage regions, and frame scheduling.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-area.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// Opaque stream source parent class (base class for all stream pixel sources).
pub struct MetaStreamSource;

/// Opaque stream area reference.
pub struct MetaStreamArea;

/// Opaque Clutter or graphics renderer reference.
pub struct MetaRenderer;

/// MetaStreamSourceArea: Pixel source for area-based stream captures.
///
/// Inherits from MetaStreamSource and provides pixel data capture for a
/// rectangular region, with cursor rendering, damage tracking, and frame scheduling.
pub struct MetaStreamSourceArea {
    /// Base class fields (inherited from MetaStreamSource, opaque).
    pub parent: *mut MetaStreamSource,
    /// Flag: whether cursor bitmap cache is invalid and needs refresh.
    pub cursor_bitmap_invalid: bool,
    /// Flag: whether hardware cursor rendering is inhibited (use software fallback).
    pub hw_cursor_inhibited: bool,
    /// Last recorded cursor metadata (position set flag, x, y coordinates).
    pub last_cursor_metadata_set: bool,
    /// Last cursor position x coordinate.
    pub last_cursor_x: i32,
    /// Last cursor position y coordinate.
    pub last_cursor_y: i32,
    /// List of active watches or subscribers (opaque list).
    pub watches: Vec<*mut c_void>,
    /// Handler ID for position-invalidated signal.
    pub position_invalidated_handler_id: u64,
    /// Handler ID for cursor-changed signal.
    pub cursor_changed_handler_id: u64,
    /// Handler ID for prepare-frame signal.
    pub prepare_frame_handler_id: u64,
    /// Idle callback ID for frame recording retry/follow-up.
    pub maybe_record_idle_id: u32,
}

impl MetaStreamSourceArea {
    pub fn new() -> Self {
        MetaStreamSourceArea {
            parent: core::ptr::null_mut(),
            cursor_bitmap_invalid: false,
            hw_cursor_inhibited: false,
            last_cursor_metadata_set: false,
            last_cursor_x: 0,
            last_cursor_y: 0,
            watches: Vec::new(),
            position_invalidated_handler_id: 0,
            cursor_changed_handler_id: 0,
            prepare_frame_handler_id: 0,
            maybe_record_idle_id: 0,
        }
    }
}

impl Default for MetaStreamSourceArea {
    fn default() -> Self {
        Self::new()
    }
}
