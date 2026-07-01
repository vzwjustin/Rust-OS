//! Private KMS display mode representation.
//!
//! Internal structures and utilities for managing display modes
//! (resolution, refresh rate, timings) within the KMS subsystem.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-mode-private.h

use core::ffi::c_void;

/// DRM mode timing information structure.
/// Mirrors Linux DRM `drm_mode_modeinfo` for video timings.
#[derive(Debug, Clone)]
pub struct DrmModeModeInfo {
    /// Pixel clock in kHz
    pub clock: u32,
    /// Horizontal display size
    pub hdisplay: u16,
    /// Horizontal sync start
    pub hsync_start: u16,
    /// Horizontal sync end
    pub hsync_end: u16,
    /// Horizontal total
    pub htotal: u16,
    /// Vertical display size
    pub vdisplay: u16,
    /// Vertical sync start
    pub vsync_start: u16,
    /// Vertical sync end
    pub vsync_end: u16,
    /// Vertical total
    pub vtotal: u16,
    /// Flags (INTERLACE, DBLSCAN, etc.)
    pub flags: u32,
    /// Mode type enum
    pub mode_type: u32,
    /// Refresh rate in Hz * 100
    pub vrefresh: u32,
}

impl DrmModeModeInfo {
    pub fn new() -> Self {
        DrmModeModeInfo {
            clock: 0,
            hdisplay: 0,
            hsync_start: 0,
            hsync_end: 0,
            htotal: 0,
            vdisplay: 0,
            vsync_start: 0,
            vsync_end: 0,
            vtotal: 0,
            flags: 0,
            mode_type: 0,
            vrefresh: 0,
        }
    }
}

impl Default for DrmModeModeInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Private display mode data for KMS
pub struct MetaKmsModePrivate {
    /// Underlying DRM mode structure with timings
    pub drm_mode: DrmModeModeInfo,
    /// DRM format (FOURCC code, e.g. DRM_FORMAT_RGB888)
    pub drm_format: u32,
    /// Opaque reference to parent KMS mode or device
    pub device_context: *mut c_void,
}

impl MetaKmsModePrivate {
    /// Create private mode data
    pub fn new() -> Self {
        MetaKmsModePrivate {
            drm_mode: DrmModeModeInfo::new(),
            drm_format: 0,
            device_context: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKmsModePrivate {
    fn default() -> Self {
        Self::new()
    }
}