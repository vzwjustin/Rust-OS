//! Backlight Sysfs Private ported from GNOME Mutter's src/backends/
//!
//! Private interface for the sysfs backlight implementation.
//! Provides the constructor and helper for finding backlight devices.
//! Sysfs file I/O (reading `max_brightness`, writing `brightness`) is
//! documented in the functions but not issued here since there is no
//! sysfs filesystem in `no_std`; the brightness state is tracked locally
//! on the `BacklightSysfs` struct.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-sysfs-private.h

use super::backlight_sysfs::BacklightSysfs;

/// Type alias for the base Backlight class (opaque, used by private implementation).
pub type Backlight = ();

/// Constructor for creating a new BacklightSysfs instance.
///
/// In the upstream C code, this takes a MetaBackend pointer, MetaOutputInfo,
/// and an error. Device discovery and D-Bus communication are
/// hardware-specific. A full implementation would:
/// 1. Use udev to enumerate `/sys/class/backlight/` devices matching the
///    output's connector (e.g. `intel_backlight`, `amdgpu_bl0`).
/// 2. Read `max_brightness` from `<device_path>/max_brightness` via sysfs.
/// 3. Read the current `brightness` from `<device_path>/brightness`.
/// 4. Set up a D-Bus/logind session proxy for privileged brightness writes.
/// Here the device path and brightness state are left at defaults; callers
/// populate them via the `set_*` methods on the returned instance.
pub fn meta_backlight_sysfs_new() -> BacklightSysfs {
    BacklightSysfs::new()
}

/// Helper to determine backlight brightness constraints from a udev device.
///
/// Upstream reads `max_brightness` from sysfs and computes `min` based on
/// `max / 100` or `0`. Returns `(min, max)` brightness values. In this port
/// the values are derived from the `BacklightSysfs` state: `max` is the
/// device's `max_brightness` (defaulting to 100 if unset) and `min` is
/// `max / 100` (matching upstream's heuristic) or `0` when max is 0.
///
/// A full implementation would open `<device_path>/max_brightness`, parse
/// the integer, and check the device type to decide whether a non-zero
/// minimum is required.
pub fn get_backlight_info(backlight: &BacklightSysfs) -> (i32, i32) {
    let max = backlight.get_max_brightness();
    if max == 0 {
        // No device probed yet; return a typical 0-100 range as a default.
        return (0, 100);
    }
    let min = (max / 100) as i32;
    (min, max as i32)
}
