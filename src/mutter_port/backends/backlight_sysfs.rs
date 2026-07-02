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
///
/// Tracks the sysfs device path, max/current brightness, and a brightness
/// step used for incremental adjustments. A full implementation would read
/// `max_brightness` and write `brightness` under the sysfs device path
/// (e.g. `/sys/class/backlight/intel_backlight/`); those file I/O
/// operations are documented in the private module but not issued here
/// since there is no sysfs filesystem in `no_std`.
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
    /// Maximum brightness value supported by the device.
    max_brightness: u32,
    /// Current brightness level.
    current_brightness: u32,
    /// Step size for incremental brightness changes.
    brightness_step: u32,
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
            max_brightness: 0,
            current_brightness: 0,
            brightness_step: 0,
        }
    }

    /// Returns the maximum brightness value.
    pub fn get_max_brightness(&self) -> u32 {
        self.max_brightness
    }

    /// Sets the maximum brightness value. A full implementation would read
    /// `/sys/class/backlight/<device>/max_brightness` to obtain this.
    pub fn set_max_brightness(&mut self, max: u32) {
        self.max_brightness = max;
        // Recompute the step as 1% of max (minimum 1) when max changes.
        self.brightness_step = if max >= 100 { max / 100 } else { 1 };
    }

    /// Returns the current brightness level.
    pub fn get_current_brightness(&self) -> u32 {
        self.current_brightness
    }

    /// Sets the current brightness level. A full implementation would write
    /// this value to `/sys/class/backlight/<device>/brightness` via sysfs
    /// (or through the D-Bus/logind privileged helper).
    pub fn set_current_brightness(&mut self, brightness: u32) {
        self.current_brightness = brightness.min(self.max_brightness);
    }

    /// Returns the brightness step size for incremental adjustments.
    pub fn get_brightness_step(&self) -> u32 {
        self.brightness_step
    }

    /// Sets the brightness step size.
    pub fn set_brightness_step(&mut self, step: u32) {
        self.brightness_step = step;
    }
}

impl Default for BacklightSysfs {
    fn default() -> Self {
        Self::new()
    }
}
