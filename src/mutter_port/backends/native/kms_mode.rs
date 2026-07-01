//! KMS display mode representation.
//!
//! Represents a display mode from the Linux DRM/KMS subsystem.
//! Ported from `meta-kms-mode.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::kms_utils::{calculate_drm_mode_refresh_rate, DrmModeInfo};

/// Flags for KMS mode properties
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KmsModeFlag {
    /// Preferred mode
    Preferred,
    /// Mode is currently active
    Current,
    /// Driver-defined flag 1
    DriverDef1,
    /// Driver-defined flag 2
    DriverDef2,
}

/// KMS display mode with underlying DRM mode data
#[derive(Debug, Clone)]
pub struct KmsMode {
    /// Underlying DRM mode information
    pub drm_mode: DrmModeInfo,
    /// Mode flags
    pub flags: Vec<KmsModeFlag>,
    /// Blob ID for this mode (if already created in kernel)
    pub blob_id: Option<u32>,
}

impl KmsMode {
    /// Create a new KMS mode
    pub fn new(drm_mode: DrmModeInfo) -> Self {
        KmsMode {
            drm_mode,
            flags: Vec::new(),
            blob_id: None,
        }
    }

    /// Get mode width
    pub fn get_width(&self) -> u32 {
        self.drm_mode.hdisplay
    }

    /// Get mode height
    pub fn get_height(&self) -> u32 {
        self.drm_mode.vdisplay
    }

    /// Get mode horizontal total
    pub fn get_htotal(&self) -> u32 {
        self.drm_mode.htotal
    }

    /// Get mode vertical total
    pub fn get_vtotal(&self) -> u32 {
        self.drm_mode.vtotal
    }

    /// Get mode clock in kHz
    pub fn get_clock(&self) -> u32 {
        self.drm_mode.clock
    }

    /// Calculate refresh rate from mode
    pub fn get_refresh_rate(&self) -> f32 {
        calculate_drm_mode_refresh_rate(&self.drm_mode)
    }

    /// Get mode name
    pub fn get_name(&self) -> String {
        // DRM mode names are up to 32 characters, terminated
        // For simplicity, generate a name from dimensions and refresh
        let refresh = self.get_refresh_rate() as u32;
        format!("{}x{}@{}Hz", self.get_width(), self.get_height(), refresh)
    }

    /// Set the blob ID for this mode
    pub fn set_blob_id(&mut self, id: u32) {
        self.blob_id = Some(id);
    }

    /// Get the blob ID if set
    pub fn get_blob_id(&self) -> Option<u32> {
        self.blob_id
    }

    /// Add a flag to this mode
    pub fn add_flag(&mut self, flag: KmsModeFlag) {
        if !self.flags.contains(&flag) {
            self.flags.push(flag);
        }
    }

    /// Check if mode has a specific flag
    pub fn has_flag(&self, flag: KmsModeFlag) -> bool {
        self.flags.contains(&flag)
    }

    /// Check if this is the preferred mode
    pub fn is_preferred(&self) -> bool {
        self.has_flag(KmsModeFlag::Preferred)
    }

    /// Check if this is the current mode
    pub fn is_current(&self) -> bool {
        self.has_flag(KmsModeFlag::Current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kms_mode_creation() {
        let drm_mode = DrmModeInfo {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 1,
            flags: 0,
        };

        let mode = KmsMode::new(drm_mode);
        assert_eq!(mode.get_width(), 1920);
        assert_eq!(mode.get_height(), 1080);
        assert_eq!(mode.get_clock(), 148500);
    }

    #[test]
    fn test_refresh_rate() {
        let drm_mode = DrmModeInfo {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 1,
            flags: 0,
        };

        let mode = KmsMode::new(drm_mode);
        let refresh = mode.get_refresh_rate();
        assert!((refresh - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_mode_flags() {
        let drm_mode = DrmModeInfo {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 1,
            flags: 0,
        };

        let mut mode = KmsMode::new(drm_mode);
        assert!(!mode.is_preferred());
        mode.add_flag(KmsModeFlag::Preferred);
        assert!(mode.is_preferred());
    }

    #[test]
    fn test_blob_id() {
        let drm_mode = DrmModeInfo {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 1,
            flags: 0,
        };

        let mut mode = KmsMode::new(drm_mode);
        assert_eq!(mode.get_blob_id(), None);
        mode.set_blob_id(42);
        assert_eq!(mode.get_blob_id(), Some(42));
    }
}
