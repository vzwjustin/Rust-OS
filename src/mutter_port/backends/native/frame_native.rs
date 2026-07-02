//! Native frame management for DRM backends.
//!
//! Tracks frame buffers, KMS updates, and damage regions for scanout operations.
//! Associates DRM buffers, damage tracking, and sync file descriptors with
//! Clutter frames to coordinate rendering with display hardware.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-frame-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Opaque DRM buffer reference.
pub struct MetaDrmBuffer;

/// Opaque Cogl scanout reference.
pub struct CoglScanout;

/// Opaque KMS update reference.
pub struct MetaKmsUpdate;

/// Opaque Mtk region reference.
pub struct MtkRegion;

/// Poll FD structure for sync file tracking.
#[derive(Debug, Clone, Copy)]
pub struct GPollFD {
    pub fd: i32,
    pub events: u16,
    pub revents: u16,
}

impl GPollFD {
    pub fn new() -> Self {
        GPollFD {
            fd: -1,
            events: 0,
            revents: 0,
        }
    }
}

impl Default for GPollFD {
    fn default() -> Self {
        Self::new()
    }
}

/// Native frame state for a single scanout cycle.
pub struct FrameNative {
    /// Associated DRM buffer (opaque).
    pub buffer: *mut MetaDrmBuffer,
    /// Scanout configuration (opaque).
    pub scanout: *mut CoglScanout,
    /// KMS update for this frame (opaque).
    pub kms_update: *mut MetaKmsUpdate,
    /// Damage region for partial updates (opaque).
    pub damage: *mut MtkRegion,
    /// Poll FD for frame sync.
    pub sync: GPollFD,
}

impl FrameNative {
    /// Create a new native frame.
    pub fn new() -> Self {
        FrameNative {
            buffer: core::ptr::null_mut(),
            scanout: core::ptr::null_mut(),
            kms_update: core::ptr::null_mut(),
            damage: core::ptr::null_mut(),
            sync: GPollFD::new(),
        }
    }
}

impl Default for FrameNative {
    fn default() -> Self {
        Self::new()
    }
}
