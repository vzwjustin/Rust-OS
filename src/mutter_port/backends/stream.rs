//! Stream — ported from GNOME Mutter
//!
//! MetaStream is the base class for different types of screen capture streams
//! (window, monitor, area). It provides the interface for creating sources and
//! transforming cursor coordinates.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream.h

use alloc::string::String;
use core::ffi::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStreamCursorMode {
    META_STREAM_CURSOR_MODE_HIDDEN = 0,
    META_STREAM_CURSOR_MODE_EMBEDDED = 1,
    META_STREAM_CURSOR_MODE_METADATA = 2,
}

/// MetaStream: Base class for screen capture streams.
/// Manages cursor modes, configuration state, and stream sources.
#[derive(Debug, Clone)]
pub struct MetaStream {
    /// Backend reference (opaque pointer to MetaBackend)
    pub backend: *mut c_void,
    /// EIS (Embedded Input Server) context (opaque pointer to MetaEis)
    pub eis: *mut c_void,
    /// Stream mapping identifier
    pub mapping_id: String,
    /// Cursor visibility and embedding mode
    pub cursor_mode: MetaStreamCursorMode,
    /// Whether the stream is configured and ready
    pub is_configured: bool,
    /// Stream source (window/monitor/area) - opaque pointer to MetaStreamSource
    pub source: *mut c_void,
}

impl MetaStream {
    pub fn new() -> Self {
        MetaStream {
            backend: core::ptr::null_mut(),
            eis: core::ptr::null_mut(),
            mapping_id: String::new(),
            cursor_mode: MetaStreamCursorMode::META_STREAM_CURSOR_MODE_HIDDEN,
            is_configured: false,
            source: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaStream {
    fn default() -> Self {
        Self::new()
    }
}
