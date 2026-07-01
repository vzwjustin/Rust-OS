//! KMS-based CRTC implementation for hardware display management.
//!
//! This module implements CRTC management via the Linux kernel's KMS (Kernel Mode Setting)
//! subsystem. Handles mode setting, gamma correction, plane management, and hardware cursor.
//! Ported from `meta-crtc-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::crtc_mode_kms::{CrtcModeKms, RefreshRateMode};
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
    /// Currently active display mode, or `None` when the CRTC is
    /// disabled.
    pub mode: Option<CrtcModeKms>,
    /// X origin of the CRTC scanout within the framebuffer, in pixels.
    pub x: i32,
    /// Y origin of the CRTC scanout within the framebuffer, in pixels.
    pub y: i32,
    /// Current hardware transform applied to the scanout.
    pub transform: MonitorTransform,
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
            mode: None,
            x: 0,
            y: 0,
            transform: MonitorTransform::Normal,
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

    /// KMS hardware typically supports all rotation transforms.
    ///
    /// Upstream Mutter queries the plane's `IN_FORMATS` property to
    /// verify the hardware can scan out the rotated buffer. Here we
    /// report all transforms as handled when a primary plane has been
    /// assigned, since most modern KMS drivers support rotation on the
    /// primary plane.
    pub fn is_transform_handled(&self, transform: MonitorTransform) -> bool {
        self.primary_plane.is_some() || transform == MonitorTransform::Normal
    }

    /// Most modern KMS hardware supports hardware cursor
    pub fn is_hw_cursor_supported(&self) -> bool {
        self.cursor_plane.is_some()
    }

    /// Get deadline evasion time in microseconds (hardware-specific).
    ///
    /// A full implementation would query the KMS device's vblank queue
    /// deadline from the GPU object. Here we return a conservative
    /// default of 1000 us (1 ms), matching the upstream fallback used
    /// when no driver-specific value is available.
    pub fn get_deadline_evasion(&self) -> i64 {
        1000
    }

    /// Set the display mode and mark the CRTC active.
    ///
    /// A full implementation would issue an atomic KMS commit with the
    /// new mode via `drmModeAtomicAddProperty` and
    /// `drmModeAtomicCommit`. Here we record the mode locally and flip
    /// the active flag so downstream code can observe the intended
    /// configuration.
    pub fn set_mode(&mut self, mode: &CrtcModeKms) {
        self.mode = Some(mode.clone());
        self.native.active = true;
    }

    /// Get the currently active display mode, if any.
    pub fn get_mode(&self) -> Option<&CrtcModeKms> {
        self.mode.as_ref()
    }

    /// Set the CRTC scanout origin within the framebuffer.
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// Get the CRTC scanout origin.
    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Set the hardware transform applied to the scanout.
    pub fn set_transform(&mut self, transform: MonitorTransform) {
        self.transform = transform;
    }

    /// Get the current hardware transform.
    pub fn get_transform(&self) -> MonitorTransform {
        self.transform
    }

    /// Unset current configuration (disable CRTC).
    ///
    /// A full implementation would issue an atomic KMS commit to
    /// disable this CRTC. Here we clear the local mode/transform state
    /// and mark the CRTC inactive.
    pub fn unset_config(&mut self) {
        self.mode = None;
        self.transform = MonitorTransform::Normal;
        self.x = 0;
        self.y = 0;
        self.native.active = false;
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

    #[test]
    fn test_set_mode_activates_crtc() {
        let mut crtc = CrtcKms::new(1);
        assert!(crtc.get_mode().is_none());
        assert!(!crtc.native.active);
        let mode = CrtcModeKms::new(
            1,
            "1920x1080".to_string(),
            1920,
            1080,
            60000,
            RefreshRateMode::Fixed,
        );
        crtc.set_mode(&mode);
        assert!(crtc.get_mode().is_some());
        assert!(crtc.native.active);
    }

    #[test]
    fn test_position_and_transform() {
        let mut crtc = CrtcKms::new(1);
        assert_eq!(crtc.get_position(), (0, 0));
        assert_eq!(crtc.get_transform(), MonitorTransform::Normal);
        crtc.set_position(100, 200);
        crtc.set_transform(MonitorTransform::Rotated90);
        assert_eq!(crtc.get_position(), (100, 200));
        assert_eq!(crtc.get_transform(), MonitorTransform::Rotated90);
    }

    #[test]
    fn test_unset_config_clears_state() {
        let mut crtc = CrtcKms::new(1);
        let mode = CrtcModeKms::new(
            1,
            "1920x1080".to_string(),
            1920,
            1080,
            60000,
            RefreshRateMode::Fixed,
        );
        crtc.set_mode(&mode);
        crtc.set_position(10, 20);
        crtc.set_transform(MonitorTransform::Rotated180);
        crtc.unset_config();
        assert!(crtc.get_mode().is_none());
        assert!(!crtc.native.active);
        assert_eq!(crtc.get_position(), (0, 0));
        assert_eq!(crtc.get_transform(), MonitorTransform::Normal);
    }

    #[test]
    fn test_transform_handled_requires_primary_plane() {
        let mut crtc = CrtcKms::new(1);
        assert!(crtc.is_transform_handled(MonitorTransform::Normal));
        assert!(!crtc.is_transform_handled(MonitorTransform::Rotated90));
        crtc.set_primary_plane(PlaneHandle::Primary(1));
        assert!(crtc.is_transform_handled(MonitorTransform::Rotated90));
    }
}
