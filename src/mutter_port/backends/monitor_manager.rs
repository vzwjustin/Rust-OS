//! Monitor Manager — ported from GNOME Mutter
//!
//! Manages monitor configurations, layout modes, and capability reporting.
//! Coordinates CRTC and output assignments for multi-monitor setups.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-manager-private.h

use alloc::string::String;

/// Capability flags for the monitor manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaMonitorManagerCapability {
    /// No special capabilities.
    NONE = 0,
    /// Supports adjustable layout mode (logical vs physical).
    LAYOUT_MODE = (1 << 0),
    /// Global scale factor is required.
    GLOBAL_SCALE_REQUIRED = (1 << 1),
}

/// Configuration method for applying monitor settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaMonitorsConfigMethod {
    /// Verify the configuration without applying it.
    VERIFY = 0,
    /// Apply temporarily (will revert on timeout or explicit revert).
    TEMPORARY = 1,
    /// Apply permanently to persistent storage.
    PERSISTENT = 2,
}

/// Logical monitor layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaLogicalMonitorLayoutMode {
    /// Logical coordinates (scaled by monitor DPI).
    LOGICAL = 1,
    /// Physical coordinates (unscaled pixel coordinates).
    PHYSICAL = 2,
}

/// Privacy screen state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaPrivacyScreenChangeState {
    NONE = 0,
    INIT = 1,
    PENDING_HOTKEY = 2,
    PENDING_SETTING = 3,
}

/// Placeholder types for opaque C structures (no direct port needed for kernel use).
#[derive(Debug)]
pub struct MetaCrtc;

#[derive(Debug)]
pub struct MetaCrtcMode;

#[derive(Debug)]
pub struct MetaOutput;

#[derive(Debug)]
pub struct MetaColorMode;

/// Constants for screen dimensions.
pub const META_MONITOR_MANAGER_MIN_SCREEN_WIDTH: u32 = 640;
pub const META_MONITOR_MANAGER_MIN_SCREEN_HEIGHT: u32 = 480;
