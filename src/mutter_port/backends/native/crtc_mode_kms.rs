//! KMS-based CRTC mode for hardware display configurations.
//!
//! Represents a display mode that's directly supported by DRM/KMS hardware.
//! Ported from `meta-crtc-mode-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// KMS mode refresh rate configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRateMode {
    /// Use only the mode's exact refresh rate
    Fixed,
    /// Allow variable refresh rate (adaptive sync)
    Variable,
}

/// Information about a KMS display mode
#[derive(Debug, Clone)]
pub struct ModeInfo {
    /// Mode name (e.g., "1920x1080")
    pub name: String,
    /// Display width in pixels
    pub width: u32,
    /// Display height in pixels
    pub height: u32,
    /// Refresh rate in mHz (millihertz)
    pub refresh_rate: u32,
    /// Display flags (interlaced, doublescan, etc.)
    pub flags: u32,
}

/// KMS CRTC mode (direct hardware mode from DRM)
#[derive(Debug, Clone)]
pub struct CrtcModeKms {
    /// Unique mode ID
    pub id: u64,
    /// Mode information
    pub info: ModeInfo,
    /// Refresh rate configuration
    pub refresh_rate_mode: RefreshRateMode,
    /// Reference to underlying KMS mode
    pub kms_mode_id: Option<u64>,
}

impl CrtcModeKms {
    /// Create a new KMS CRTC mode
    pub fn new(
        id: u64,
        name: String,
        width: u32,
        height: u32,
        refresh_rate: u32,
        refresh_rate_mode: RefreshRateMode,
    ) -> Self {
        CrtcModeKms {
            id,
            info: ModeInfo {
                name,
                width,
                height,
                refresh_rate,
                flags: 0,
            },
            refresh_rate_mode,
            kms_mode_id: None,
        }
    }

    /// Set the underlying KMS mode ID
    pub fn set_kms_mode_id(&mut self, kms_mode_id: u64) {
        self.kms_mode_id = Some(kms_mode_id);
    }

    /// Get the KMS mode ID if available
    pub fn get_kms_mode_id(&self) -> Option<u64> {
        self.kms_mode_id
    }

    /// Format mode information string
    pub fn mode_name(&self) -> String {
        let hz = self.info.refresh_rate / 1000;
        let mut name = format!("{}x{}@{}Hz", self.info.width, self.info.height, hz);
        if self.refresh_rate_mode == RefreshRateMode::Variable {
            name.push_str(" (variable)");
        }
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kms_mode_creation() {
        let mode = CrtcModeKms::new(
            1,
            "1920x1080".to_string(),
            1920,
            1080,
            60000,
            RefreshRateMode::Fixed,
        );
        assert_eq!(mode.info.width, 1920);
        assert_eq!(mode.refresh_rate_mode, RefreshRateMode::Fixed);
    }

    #[test]
    fn test_kms_mode_id() {
        let mut mode = CrtcModeKms::new(
            1,
            "1920x1080".to_string(),
            1920,
            1080,
            60000,
            RefreshRateMode::Fixed,
        );
        assert_eq!(mode.get_kms_mode_id(), None);
        mode.set_kms_mode_id(42);
        assert_eq!(mode.get_kms_mode_id(), Some(42));
    }

    #[test]
    fn test_mode_name() {
        let mode = CrtcModeKms::new(
            1,
            "1920x1080".to_string(),
            1920,
            1080,
            60000,
            RefreshRateMode::Fixed,
        );
        assert_eq!(mode.mode_name(), "1920x1080@60Hz");
    }
}
