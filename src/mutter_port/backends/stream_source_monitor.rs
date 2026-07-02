//! Stream Source Monitor — ported from GNOME Mutter
//!
//! MetaStreamSourceMonitor provides the actual pixel data capture for a monitor-based
//! stream, handling rendering and frame recording for display outputs.
//! Inherits from MetaStreamSource and manages pixel buffer recording.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-monitor.h

use core::ffi::c_void;

/// Opaque stream source base class (GObject-based).
pub struct MetaStreamSource;

/// Opaque stream monitor type for capturing a specific monitor.
pub struct MetaStreamMonitor;

/// Enum for stream record results (flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStreamRecordResult {
    META_STREAM_RECORD_RESULT_RECORDED_NOTHING = 0,
    META_STREAM_RECORD_RESULT_RECORDED_FRAME = 1 << 0,
    META_STREAM_RECORD_RESULT_RECORDED_CURSOR = 1 << 1,
}

/// Enum for stream record flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStreamRecordFlag {
    META_STREAM_RECORD_FLAG_NONE = 0,
    META_STREAM_RECORD_FLAG_CURSOR_ONLY = 1 << 0,
}

/// Enum for stream paint phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStreamPaintPhase {
    META_STREAM_PAINT_PHASE_DETACHED = 0,
    META_STREAM_PAINT_PHASE_PRE_PAINT = 1,
    META_STREAM_PAINT_PHASE_PRE_SWAP_BUFFER = 2,
}

/// MetaStreamSourceMonitor: Pixel source for monitor captures.
///
/// Extends MetaStreamSource to provide pixel-data capture for a specific
/// monitor output. Handles frame recording and cursor overlay.
pub struct MetaStreamSourceMonitor {
    /// Pointer to parent MetaStreamSource (inherited).
    pub base: *mut MetaStreamSource,
    /// Reference to the stream monitor being captured (opaque).
    pub stream_monitor: *mut MetaStreamMonitor,
    /// Last recorded frame result flags.
    pub last_result: MetaStreamRecordResult,
    /// Current paint phase for buffer synchronization.
    pub paint_phase: MetaStreamPaintPhase,
}

impl MetaStreamSourceMonitor {
    /// Create a new monitor stream source.
    pub fn new() -> Self {
        MetaStreamSourceMonitor {
            base: core::ptr::null_mut(),
            stream_monitor: core::ptr::null_mut(),
            last_result: MetaStreamRecordResult::META_STREAM_RECORD_RESULT_RECORDED_NOTHING,
            paint_phase: MetaStreamPaintPhase::META_STREAM_PAINT_PHASE_DETACHED,
        }
    }
}

impl Default for MetaStreamSourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}
