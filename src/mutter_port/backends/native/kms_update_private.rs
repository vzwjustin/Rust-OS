//! KMS Update Private — kernel modesetting configuration update structures.
//!
//! Internal structures for atomic KMS updates: plane assignments, CRTC color,
//! connector properties, mode sets, and feedback.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-update-private.h






use crate::mutter_port::backends::common_types::*;
use alloc::{boxed::Box, vec::Vec};
use core::ffi::c_void;

/// MetaKmsFeedback — result of an atomic KMS update
#[derive(Debug, Clone)]
pub struct MetaKmsFeedback {
    /// Atomic reference count
    pub ref_count: u32,
    /// Update result status
    pub result: MetaKmsFeedbackResult,
    /// Ready time (microseconds, -1 if not ready)
    pub ready_time_us: i64,
    /// List of failed planes (opaque pointer to GList)
    pub failed_planes: *mut c_void,
    /// Error message if failed (opaque pointer to GError)
    pub error: *mut c_void,
}

impl MetaKmsFeedback {
    pub fn new() -> Self {
        MetaKmsFeedback {
            ref_count: 1,
            result: MetaKmsFeedbackResult::Ok,
            ready_time_us: -1,
            failed_planes: core::ptr::null_mut(),
            error: core::ptr::null_mut(),
        }
    }

    /// TODO: port logic from meta_kms_feedback_unref
    pub fn unref(&self) {
        todo!()
    }
}

impl Default for MetaKmsFeedback {
    fn default() -> Self {
        Self::new()
    }
}

/// MetaKmsFbDamage — framebuffer damage region for KMS updates
#[derive(Debug, Clone)]
pub struct MetaKmsFbDamage {
    /// Array of damage rectangles (opaque pointer to drm_mode_rect array)
    pub rects: *mut c_void,
    /// Number of rectangles
    pub n_rects: i32,
}

impl MetaKmsFbDamage {
    pub fn new() -> Self {
        MetaKmsFbDamage {
            rects: core::ptr::null_mut(),
            n_rects: 0,
        }
    }
}

impl Default for MetaKmsFbDamage {
    fn default() -> Self {
        Self::new()
    }
}

/// MetaKmsModeSet — CRTC mode configuration
#[derive(Debug, Clone)]
pub struct MetaKmsModeSet {
    /// CRTC being configured (opaque pointer to MetaKmsCrtc)
    pub crtc: *mut c_void,
    /// List of connectors to enable on this CRTC (opaque pointer to GList)
    pub connectors: *mut c_void,
    /// Display mode (opaque pointer to MetaKmsMode)
    pub mode: *mut c_void,
}

impl MetaKmsModeSet {
    pub fn new() -> Self {
        MetaKmsModeSet {
            crtc: core::ptr::null_mut(),
            connectors: core::ptr::null_mut(),
            mode: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKmsModeSet {
    fn default() -> Self {
        Self::new()
    }
}
