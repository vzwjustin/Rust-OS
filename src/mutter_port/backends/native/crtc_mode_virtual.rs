//! Virtual CRTC mode representing a display resolution/refresh rate.
//!
//! Modes describe possible display configurations (resolution, refresh rate, etc.)
//! for virtual CRTCs. Ported from `meta-crtc-mode-virtual.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Information about a virtual display mode
#[derive(Debug, Clone, Copy)]
pub struct VirtualModeInfo {
    /// Display width in pixels
    pub width: u32,
    /// Display height in pixels
    pub height: u32,
    /// Refresh rate in mHz (millihertz, so 60000 = 60 Hz)
    pub refresh_rate: u32,
}

/// Virtual CRTC mode (display resolution/refresh combination)
#[derive(Debug, Clone)]
pub struct CrtcModeVirtual {
    /// Unique mode ID
    pub id: u64,
    /// Virtual mode information
    pub info: VirtualModeInfo,
}

impl CrtcModeVirtual {
    /// Virtual mode ID bit flag (MSB set)
    const ID_BIT: u64 = 1u64 << 63;

    /// Create a new virtual mode
    pub fn new(width: u32, height: u32, refresh_rate: u32) -> Self {
        // Generate a simple ID from dimensions and refresh rate
        let base_id = ((width as u64) << 32) | (height as u64);
        let id = Self::ID_BIT | (base_id & !Self::ID_BIT);

        CrtcModeVirtual {
            id,
            info: VirtualModeInfo {
                width,
                height,
                refresh_rate,
            },
        }
    }

    /// Create a new virtual mode with explicit ID
    pub fn with_id(id: u64, width: u32, height: u32, refresh_rate: u32) -> Self {
        let virtual_id = Self::ID_BIT | (id & !Self::ID_BIT);
        CrtcModeVirtual {
            id: virtual_id,
            info: VirtualModeInfo {
                width,
                height,
                refresh_rate,
            },
        }
    }

    /// Check if this is a virtual mode
    pub fn is_virtual(&self) -> bool {
        (self.id & Self::ID_BIT) != 0
    }

    /// Get the virtual ID without the ID_BIT marker
    pub fn virtual_id(&self) -> u64 {
        self.id & !Self::ID_BIT
    }

    /// Get mode name as string (e.g., "1920x1080 @ 60Hz")
    pub fn name(&self) -> String {
        let hz = self.info.refresh_rate / 1000;
        format!("{}x{}@{}Hz", self.info.width, self.info.height, hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_creation() {
        let mode = CrtcModeVirtual::new(1920, 1080, 60000);
        assert_eq!(mode.info.width, 1920);
        assert_eq!(mode.info.height, 1080);
        assert_eq!(mode.info.refresh_rate, 60000);
    }

    #[test]
    fn test_is_virtual() {
        let mode = CrtcModeVirtual::new(1920, 1080, 60000);
        assert!(mode.is_virtual());
    }

    #[test]
    fn test_mode_name() {
        let mode = CrtcModeVirtual::new(1920, 1080, 60000);
        assert_eq!(mode.name(), "1920x1080@60Hz");
    }
}
