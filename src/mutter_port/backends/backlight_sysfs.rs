//! Backlight Sysfs ported from GNOME Mutter's src/backends/
//!
//! A backlight implementation that reads and writes brightness via sysfs,
//! using either D-Bus/logind or a privileged helper for privilege escalation.
//! Monitors udev events for sysfs brightness changes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-sysfs.c

use alloc::string::String;

/// A backlight that controls display brightness via sysfs brightness file.
/// Communicates with udev for hot-plug detection and D-Bus/logind for privileged access.
pub struct BacklightSysfs {
    /// udev device for this backlight (opaque, hardware-specific)
    pub device: usize,
    /// D-Bus session proxy for setting brightness (opaque)
    pub session_proxy: usize,
    /// Device name (e.g., "intel_backlight")
    pub device_name: String,
    /// Full sysfs path to the device
    pub device_path: String,
    /// Path to the brightness file within sysfs
    pub brightness_path: String,
}

impl BacklightSysfs {
    /// Create a new sysfs backlight instance with default values.
    pub fn new() -> Self {
        BacklightSysfs {
            device: 0,
            session_proxy: 0,
            device_name: String::new(),
            device_path: String::new(),
            brightness_path: String::new(),
        }
    }
}

impl Default for BacklightSysfs {
    fn default() -> Self {
        Self::new()
    }
}
