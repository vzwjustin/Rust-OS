//! Stream Source Virtual — ported from GNOME Mutter
//!
//! MetaStreamSourceVirtual provides the actual pixel data capture for a virtual
//! monitor stream, handling rendering and frame recording for software-defined
//! displays. Manages viewport layout, cursor position tracking, preferred scaling,
//! and frame clock driving.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-virtual.h

use alloc::vec::Vec;

pub struct MetaStreamSource {
    // Opaque base class
}

pub struct MetaStreamVirtual {
    // Opaque virtual stream type
}

pub struct ClutterStageView {
    // Opaque Clutter type
}

pub struct MetaLogicalMonitor {
    // Opaque logical monitor type
}

pub struct MetaVirtualMonitor;
pub struct MetaStageWatch;
pub struct MetaStreamFrameClockDriver;

/// Struct representing cursor metadata (position and validity flag).
#[derive(Debug, Clone, Copy)]
pub struct CursorMetadata {
    pub set: bool,
    pub x: i32,
    pub y: i32,
}

/// Pixel source for virtual monitor captures with frame synchronization.
pub struct MetaStreamSourceVirtual {
    pub parent: MetaStreamSource,
    pub virtual_monitor: *mut MetaVirtualMonitor,
    pub mode_infos: *mut core::ffi::c_void,
    pub has_preferred_scale: bool,
    pub preferred_scale: f32,
    pub cursor_bitmap_invalid: bool,
    pub last_cursor_metadata: CursorMetadata,
    pub paint_watch: *mut MetaStageWatch,
    pub skipped_watch: *mut MetaStageWatch,
    pub layout_binding: *mut core::ffi::c_void,
    pub position_invalidated_handler_id: u64,
    pub cursor_changed_handler_id: u64,
    pub monitors_changed_handler_id: u64,
    pub driver: *mut MetaStreamFrameClockDriver,
}

impl MetaStreamSourceVirtual {
    pub fn new() -> Self {
        MetaStreamSourceVirtual {
            parent: MetaStreamSource {},
            virtual_monitor: core::ptr::null_mut(),
            mode_infos: core::ptr::null_mut(),
            has_preferred_scale: false,
            preferred_scale: 1.0,
            cursor_bitmap_invalid: false,
            last_cursor_metadata: CursorMetadata {
                set: false,
                x: 0,
                y: 0,
            },
            paint_watch: core::ptr::null_mut(),
            skipped_watch: core::ptr::null_mut(),
            layout_binding: core::ptr::null_mut(),
            position_invalidated_handler_id: 0,
            cursor_changed_handler_id: 0,
            monitors_changed_handler_id: 0,
            driver: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaStreamSourceVirtual {
    fn default() -> Self {
        Self::new()
    }
}
