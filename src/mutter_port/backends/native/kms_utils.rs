//! KMS utility functions for display mode calculations.
//!
//! Provides common calculations for DRM mode information such as refresh rate
//! and vblank duration. Ported from `meta-kms-utils.c`.

/// DRM mode information for calculations
#[derive(Debug, Clone, Copy)]
pub struct DrmModeInfo {
    /// Horizontal clock in kHz
    pub clock: u32,
    /// Horizontal active pixels
    pub hdisplay: u32,
    /// Horizontal blanking start
    pub hsync_start: u32,
    /// Horizontal sync end
    pub hsync_end: u32,
    /// Horizontal total
    pub htotal: u32,
    /// Vertical active lines
    pub vdisplay: u32,
    /// Vertical blanking start
    pub vsync_start: u32,
    /// Vertical sync end
    pub vsync_end: u32,
    /// Vertical total
    pub vtotal: u32,
    /// Vertical scan (interlace multiplier)
    pub vscan: u32,
    /// Mode flags
    pub flags: u32,
}

/// DRM mode flags
pub mod flags {
    pub const DBLSCAN: u32 = 1 << 0;
    pub const INTERLACE: u32 = 1 << 1;
}

/// Calculate refresh rate from DRM mode information (in Hz)
pub fn calculate_drm_mode_refresh_rate(mode: &DrmModeInfo) -> f32 {
    if mode.htotal == 0 || mode.vtotal == 0 {
        return 0.0;
    }

    let numerator = (mode.clock as f64) * 1000.0;
    let mut denominator = (mode.vtotal as f64) * (mode.htotal as f64);

    if mode.vscan > 1 {
        denominator *= mode.vscan as f64;
    }

    (numerator / denominator) as f32
}

/// Calculate vertical blanking duration from DRM mode (in microseconds)
pub fn calculate_drm_mode_vblank_duration_us(mode: &DrmModeInfo) -> i64 {
    if mode.htotal == 0 || mode.vtotal == 0 {
        return 0;
    }

    // Convert to i64 early
    let mut value = (mode.vtotal - mode.vdisplay) as i64;
    value *= mode.htotal as i64;

    // Account for double-scan modes
    if (mode.flags & flags::DBLSCAN) != 0 {
        value *= 2;
    }

    // Convert from kHz-based timing to microseconds
    // Round up as this is used for buffer swap deadline computation
    value = (value * 1000 + (mode.clock as i64) - 1) / (mode.clock as i64);

    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_refresh_rate_1920x1080_60hz() {
        let mode = DrmModeInfo {
            clock: 148500, // 148.5 MHz
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

        let refresh_rate = calculate_drm_mode_refresh_rate(&mode);
        // Should be approximately 60 Hz
        assert!((refresh_rate - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_refresh_rate_invalid() {
        let mode = DrmModeInfo {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 0, // Invalid
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 1,
            flags: 0,
        };

        let refresh_rate = calculate_drm_mode_refresh_rate(&mode);
        assert_eq!(refresh_rate, 0.0);
    }

    #[test]
    fn test_vblank_duration() {
        let mode = DrmModeInfo {
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

        let duration_us = calculate_drm_mode_vblank_duration_us(&mode);
        // Vblank is (1125-1080)*2200 / (148500 kHz) = ~16.6ms
        assert!(duration_us > 16000 && duration_us < 17000);
    }
}
