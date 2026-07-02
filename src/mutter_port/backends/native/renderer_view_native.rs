//! Native renderer view for a single output (monitor).
//!
//! Represents a single render target on a display output, handling
//! frame presentation, deadline evasion tuning, and output-specific
//! rendering state.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-view-native.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// Native output-specific renderer view
pub struct RendererViewNative {
    /// Parent renderer view (opaque)
    pub parent: *mut core::ffi::c_void,
    /// Associated CRTC (opaque)
    pub crtc: *mut core::ffi::c_void,
    /// Frame clock for scheduling (opaque)
    pub frame_clock: *mut core::ffi::c_void,
    /// Deadline evasion in microseconds
    pub deadline_evasion_us: i64,
}

impl RendererViewNative {
    pub fn new() -> Self {
        RendererViewNative {
            parent: core::ptr::null_mut(),
            crtc: core::ptr::null_mut(),
            frame_clock: core::ptr::null_mut(),
            deadline_evasion_us: 0,
        }
    }
}

impl Default for RendererViewNative {
    fn default() -> Self {
        Self::new()
    }
}
