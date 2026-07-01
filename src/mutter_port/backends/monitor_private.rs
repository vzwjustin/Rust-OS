//! Monitor Private — ported from GNOME Mutter
//!
//! Private monitor definitions including monitor specs (EDID identity), mode specs
//! (resolution/refresh/flags), and CRTC mode assignments. Supports both tiled and
//! normal monitors, with constraint flags for scale calculations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-private.h






use crate::mutter_port::backends::common_types::*;
use alloc::string::String;

/// Monitor identity from EDID: connector, vendor, product, serial.
/// Used to uniquely identify physical monitors across hotplug/redetect cycles.
#[derive(Debug, Clone)]
pub struct MetaMonitorSpec {
    pub connector: String,
    pub vendor: String,
    pub product: String,
    pub serial: String,
}

impl MetaMonitorSpec {
    pub fn new() -> Self {
        MetaMonitorSpec {
            connector: String::new(),
            vendor: String::new(),
            product: String::new(),
            serial: String::new(),
        }
    }
}

impl Default for MetaMonitorSpec {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitor mode specification: resolution, refresh rate, and flags.
#[derive(Debug, Clone)]
pub struct MetaMonitorModeSpec {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub refresh_rate_mode: MetaCrtcRefreshRateMode,
    pub flags: MetaCrtcModeFlag,
}

impl MetaMonitorModeSpec {
    pub fn new(width: i32, height: i32, refresh_rate: f32) -> Self {
        MetaMonitorModeSpec {
            width,
            height,
            refresh_rate,
            refresh_rate_mode: MetaCrtcRefreshRateMode::Exact,
            flags: MetaCrtcModeFlag(0),
        }
    }
}

impl Default for MetaMonitorModeSpec {
    fn default() -> Self {
        Self::new(0, 0, 0.0)
    }
}

/// Opaque MetaOutput reference.
pub struct MetaOutput;

/// Opaque MetaCrtcMode reference.
pub struct MetaCrtcMode;

/// Monitor-to-CRTC mode assignment: which output uses which CRTC mode.
#[derive(Debug, Clone)]
pub struct MetaMonitorCrtcMode {
    pub output: *mut MetaOutput,
    pub crtc_mode: *mut MetaCrtcMode,
}

impl MetaMonitorCrtcMode {
    pub fn new() -> Self {
        MetaMonitorCrtcMode {
            output: core::ptr::null_mut(),
            crtc_mode: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaMonitorCrtcMode {
    fn default() -> Self {
        Self::new()
    }
}

/// Constraint flags for monitor scale calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaMonitorScalesConstraint {
    META_MONITOR_SCALES_CONSTRAINT_NONE = 0,
    META_MONITOR_SCALES_CONSTRAINT_NO_FRAC = 1,
}
