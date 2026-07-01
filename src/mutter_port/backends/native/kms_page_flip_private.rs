//! Private page-flip event tracking and synchronization.
//!
//! Internal state for asynchronous page-flip completion,
//! including VBlank synchronization, feedback handling, and
//! frame timing coordination with display hardware.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-page-flip-private.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// Opaque KMS impl device reference.
pub struct MetaKmsImplDevice;

/// Opaque KMS CRTC reference.
pub struct MetaKmsCrtc;

/// Opaque listener vtable reference.
pub struct MetaKmsPageFlipListenerVtable;

/// Reference-counted page-flip data for KMS operations.
///
/// Tracks pending page-flip events, frame timing, and associated listeners.
pub struct MetaKmsPageFlipData {
    /// Atomic reference count for memory management.
    pub ref_count: u32,
    /// Associated KMS impl device (opaque).
    pub impl_device: *mut MetaKmsImplDevice,
    /// Target CRTC for this page flip (opaque).
    pub crtc: *mut MetaKmsCrtc,
    /// List of listener closures (opaque).
    pub closures: *mut c_void,
    /// Vblank sequence number.
    pub sequence: u32,
    /// Frame timestamp (seconds).
    pub sec: u32,
    /// Frame timestamp (microseconds).
    pub usec: u32,
    /// Flag: is this a symbolic flip (no actual hardware flip)?
    pub is_symbolic: bool,
    /// Associated error, if any (opaque).
    pub error: *mut c_void,
}

impl MetaKmsPageFlipData {
    /// Create a new page-flip data structure.
    pub fn new() -> Self {
        MetaKmsPageFlipData {
            ref_count: 1,
            impl_device: core::ptr::null_mut(),
            crtc: core::ptr::null_mut(),
            closures: core::ptr::null_mut(),
            sequence: 0,
            sec: 0,
            usec: 0,
            is_symbolic: false,
            error: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKmsPageFlipData {
    fn default() -> Self {
        Self::new()
    }
}