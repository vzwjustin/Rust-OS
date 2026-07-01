//! Native stage (display surface) implementation for DRM backends.
//!
//! Extends the Clutter stage with native display backend integration,
//! managing screen composition and synchronization for DRM-driven displays.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-stage-native.h

use alloc::{boxed::Box, string::String, vec::Vec};

/// Native stage implementation for direct display control.
///
/// Inherits from MetaStageImpl and provides DRM-specific rendering
/// and display synchronization.
pub struct StageNative {
    /// Placeholder for platform-specific state (opaque).
    _state: *mut core::ffi::c_void,
}

impl StageNative {
    /// Create a new native stage.
    pub fn new() -> Self {
        StageNative {
            _state: core::ptr::null_mut(),
        }
    }
}

impl Default for StageNative {
    fn default() -> Self {
        Self::new()
    }
}
