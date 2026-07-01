//! KMS-based CRTC implementation for hardware display management.
//!
//! This module implements CRTC management via the Linux kernel's KMS (Kernel Mode Setting)
//! subsystem. Handles mode setting, gamma correction, plane management, and hardware cursor.
//! Ported from `meta-crtc-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::crtc_mode_kms::CrtcModeKms;
use super::crtc_native::{CrtcNative, MonitorTransform};

/// Gamma LUT (Look-Up Table) for color correction
#[derive(Debug, Clone)]
pub struct GammaLut {
    /// Red channel values
    pub red: Vec<u16>,
    /// Green channel values
    pub green: Vec<u16>,
    /// Blue channel values
    pub blue: Vec<u16>,
}

/// Handle to underlying KMS CRTC (opaque in this port)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmsCrtcHandle(u64);

/// Handle to KMS plane (primary, cursor, overlay)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneHandle {
    Primary(u64),
    Cursor(u64),
    Overlay(u64),
}

/// KMS CRTC with hardware-specific capabilities
#[derive(Debug)]
pub struct CrtcKms {
    /// Base native CRTC
    pub native: CrtcNative,
    /// Reference to underlying KMS CRTC
    pub kms_crtc: Option<KmsCrtcHandle>,
    /// Primary plane for scanout
    pub primary_plane: Option<PlaneHandle>,
    /// Cursor plane for hardware cursor
    pub cursor_plane: Option<PlaneHandle>,
    /// Current gamma LUT if set
    pub gamma_lut: Option<GammaLut>,
    /// Is this CRTC leased out (DRM lease)
    pub is_leased: bool,
}

impl CrtcKms {
    /// Create a new KMS CRTC
    pub fn new(id: u64) -> Self {
        CrtcKms {
            native: CrtcNative::new(id),
            kms_crtc: None,
            primary_plane: None,
            cursor_plane: None,
            gamma_lut: None,
            is_leased: false,
        }
    }

    /// Set the underlying KMS CRTC handle
    pub fn set_kms_crtc(&mut self, handle: KmsCrtcHandle) {
        self.kms_crtc = Some(handle);
    }

    /// Get the underlying KMS CRTC handle
    pub fn get_kms_crtc(&self) -> Option<KmsCrtcHandle> {
        self.kms_crtc
    }

    /// Set the primary plane
    pub fn set_primary_plane(&mut self, plane: PlaneHandle) {
        self.primary_plane = Some(plane);
    }

    /// Get the primary plane
    pub fn get_primary_plane(&self) -> Option<PlaneHandle> {
        self.primary_plane
    }

    /// Set the cursor plane
    pub fn set_cursor_plane(&mut self, plane: PlaneHandle) {
        self.cursor_plane = Some(plane);
    }

    /// Get the cursor plane
    pub fn get_cursor_plane(&self) -> Option<PlaneHandle> {
        self.cursor_plane
    }

    /// Set gamma LUT for color correction
    pub fn set_gamma_lut(&mut self, lut: GammaLut) {
        self.gamma_lut = Some(lut);
    }

    /// Get gamma LUT size (number of entries per channel)
    pub fn get_gamma_lut_size(&self) -> usize {
        if let Some(lut) = &self.gamma_lut {
            lut.red.len()
        } else {
            0
        }
    }

    /// Get current gamma LUT
    pub fn get_gamma_lut(&self) -> Option<&GammaLut> {
        self.gamma_lut.as_ref()
    }

    /// Check if this CRTC is currently leased
    pub fn is_leased_out(&self) -> bool {
        self.is_leased
    }

    /// Set lease status
    pub fn set_leased(&mut self, leased: bool) {
        self.is_leased = leased;
    }

    /// KMS hardware typically supports all rotation transforms
    pub fn is_transform_handled(&self, transform: MonitorTransform) -> bool {
        // Most KMS hardware supports rotation; some may not support all transforms
        // For now, report all transforms as handled; subclasses can override
        transform != MonitorTransform::Normal || true // All transforms handled
    }

    /// Most modern KMS hardware supports hardware cursor
    pub fn is_hw_cursor_supported(&self) -> bool {
        self.cursor_plane.is_some()
    }

    /// Get deadline evasion time in microseconds (hardware-specific)
    /// Requires knowledge of the specific hardware (read from GPU object typically)
    pub fn get_deadline_evasion(&self) -> i64 {
        // TODO: Query from KMS device properties
        0
    }

    /// Set the display mode (requires KMS atomic operations)
    pub fn set_mode(&mut self, mode: &CrtcModeKms) {
        // TODO: Issue atomic KMS commit with new mode
        // This requires drmModeAtomicAddProperty() and drmModeAtomicCommit()
    }

    /// Unset current configuration (disable CRTC)
    pub fn unset_config(&mut self) {
        // TODO: Issue atomic KMS commit to disable this CRTC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kms_crtc_creation() {
        let crtc = CrtcKms::new(42);
        assert_eq!(crtc.native.id, 42);
        assert_eq!(crtc.get_kms_crtc(), None);
        assert!(!crtc.is_hw_cursor_supported());
    }

    #[test]
    fn test_kms_crtc_handle() {
        let mut crtc = CrtcKms::new(42);
        let handle = KmsCrtcHandle(1);
        crtc.set_kms_crtc(handle);
        assert_eq!(crtc.get_kms_crtc(), Some(handle));
    }

    #[test]
    fn test_plane_management() {
        let mut crtc = CrtcKms::new(42);
        let primary = PlaneHandle::Primary(1);
        let cursor = PlaneHandle::Cursor(2);

        crtc.set_primary_plane(primary);
        crtc.set_cursor_plane(cursor);

        assert_eq!(crtc.get_primary_plane(), Some(primary));
        assert_eq!(crtc.get_cursor_plane(), Some(cursor));
        assert!(crtc.is_hw_cursor_supported());
    }

    #[test]
    fn test_gamma_lut() {
        let mut crtc = CrtcKms::new(42);
        assert_eq!(crtc.get_gamma_lut_size(), 0);

        let lut = GammaLut {
            red: vec![0; 256],
            green: vec![0; 256],
            blue: vec![0; 256],
        };
        crtc.set_gamma_lut(lut);
        assert_eq!(crtc.get_gamma_lut_size(), 256);
    }
}
