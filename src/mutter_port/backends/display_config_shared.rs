//! Display Config Shared ported from GNOME Mutter's src/backends/
//!
//! Shared display configuration enums for DPMS (Display Power Management Signaling).
//! Defines power save states used across mutter and gnome-desktop.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-display-config-shared.h

/// MetaPowerSave — DPMS power save state enum.
/// Defines display power modes: unsupported, on, standby, suspend, or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MetaPowerSave {
    /// Power save not supported by this output.
    UNSUPPORTED = -1,
    /// Display powered on (normal operation).
    ON = 0,
    /// Display in standby mode (low power).
    STANDBY = 1,
    /// Display in suspend mode (very low power).
    SUSPEND = 2,
    /// Display powered off.
    OFF = 3,
}
