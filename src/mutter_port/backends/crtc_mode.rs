//! CRTC mode ported from GNOME Mutter's src/backends/meta-crtc-mode.c
//!
//! Represents a display mode (resolution, refresh rate, timing flags) as
//! advertised by a CRTC. `MetaCrtcModeInfo` carries the immutable mode
//! description; `MetaCrtcMode` binds an id and name to that description.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-crtc-mode.c

use alloc::string::String;

/// Timing flags for a CRTC mode (mirrors `MetaCrtcModeFlag`, DRM values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaCrtcModeFlag(pub u32);

impl MetaCrtcModeFlag {
    pub const NONE: u32 = 0;
    pub const PHSYNC: u32 = 1 << 0;
    pub const NHSYNC: u32 = 1 << 1;
    pub const PVSYNC: u32 = 1 << 2;
    pub const NVSYNC: u32 = 1 << 3;
    pub const INTERLACE: u32 = 1 << 4;
    pub const DBLSCAN: u32 = 1 << 5;
    pub const CSYNC: u32 = 1 << 6;
    pub const PCSYNC: u32 = 1 << 7;
    pub const NCSYNC: u32 = 1 << 8;
    pub const HSKEW: u32 = 1 << 9;
    pub const BCAST: u32 = 1 << 10;
    pub const PIXMUX: u32 = 1 << 11;
    pub const DBLCLK: u32 = 1 << 12;
    pub const CLKDIV2: u32 = 1 << 13;
    pub const MASK: u32 = 0x3fff;
}

/// Whether the refresh rate is fixed or variable (mirrors `MetaCrtcRefreshRateMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaCrtcRefreshRateMode {
    Fixed,
    Variable,
}

/// Immutable description of a CRTC mode.
///
/// In C this is a boxed, reference-counted type; here it is a plain value
/// type that callers can clone as needed.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaCrtcModeInfo {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub refresh_rate_mode: MetaCrtcRefreshRateMode,
    pub vblank_duration_us: i64,
    pub pixel_clock_khz: u32,
    pub flags: MetaCrtcModeFlag,

    pub has_preferred_scale: bool,
    pub preferred_scale: f32,
}

impl MetaCrtcModeInfo {
    pub fn new() -> Self {
        MetaCrtcModeInfo {
            width: 0,
            height: 0,
            refresh_rate: 0.0,
            refresh_rate_mode: MetaCrtcRefreshRateMode::Fixed,
            vblank_duration_us: 0,
            pixel_clock_khz: 0,
            flags: MetaCrtcModeFlag(MetaCrtcModeFlag::NONE),
            has_preferred_scale: false,
            preferred_scale: 0.0,
        }
    }
}

impl Default for MetaCrtcModeInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// A CRTC mode: an id and name bound to a mode description.
#[derive(Debug, Clone)]
pub struct MetaCrtcMode {
    id: u64,
    name: String,
    info: MetaCrtcModeInfo,
}

impl MetaCrtcMode {
    pub fn new(id: u64, name: String, info: MetaCrtcModeInfo) -> Self {
        MetaCrtcMode { id, name, info }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_info(&self) -> &MetaCrtcModeInfo {
        &self.info
    }
}
