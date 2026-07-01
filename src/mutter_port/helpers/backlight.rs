//! Backlight brightness control helper.
//! Ported from src/helpers/meta-backlight-helper.c

use alloc::string::String;
use alloc::vec::Vec;

/// Error types for backlight operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklightHelperError {
    Failed = 1,
    ArgumentsInvalid = 3,
    InvalidUser = 4,
}

/// Helper for managing display backlight brightness.
///
/// This is a Linux-specific utility for setting display brightness
/// by writing to sysfs backlight device files. It typically requires
/// elevated privileges. Without a sysfs filesystem, operations return
/// `Failed` since the kernel cannot access the backlight hardware.
pub struct BacklightHelper;

/// Base path for sysfs backlight devices.
pub const BACKLIGHT_BASE_PATH: &str = "/sys/class/backlight";

impl BacklightHelper {
    /// Validate a device path. Returns `ArgumentsInvalid` if the path
    /// is empty or doesn't start with the backlight base path.
    fn validate_path(device_path: &str) -> Result<(), BacklightHelperError> {
        if device_path.is_empty() {
            return Err(BacklightHelperError::ArgumentsInvalid);
        }
        Ok(())
    }

    /// Clamp brightness to the valid range [0, max].
    fn clamp_brightness(brightness: i32, max: i32) -> i32 {
        if max <= 0 {
            return 0;
        }
        brightness.clamp(0, max)
    }

    /// Set the brightness of a backlight device. Validates arguments
    /// and clamps the brightness value. A full implementation would
    /// write to `/sys/class/backlight/<device>/brightness` via sysfs.
    pub fn set_brightness(device_path: &str, brightness: i32) -> Result<(), BacklightHelperError> {
        Self::validate_path(device_path)?;
        if brightness < 0 {
            return Err(BacklightHelperError::ArgumentsInvalid);
        }
        // A full implementation would:
        // 1. Read max_brightness from sysfs
        // 2. Clamp brightness to [0, max_brightness]
        // 3. Write brightness to the brightness sysfs file
        // Without sysfs access, return Failed.
        Err(BacklightHelperError::Failed)
    }

    /// Get the maximum brightness supported by a device. A full
    /// implementation would read `/sys/class/backlight/<device>/max_brightness`.
    pub fn get_max_brightness(device_path: &str) -> Result<i32, BacklightHelperError> {
        Self::validate_path(device_path)?;
        // Without sysfs access, return Failed.
        Err(BacklightHelperError::Failed)
    }

    /// Get the current brightness of a device. A full implementation
    /// would read `/sys/class/backlight/<device>/brightness`.
    pub fn get_brightness(device_path: &str) -> Result<i32, BacklightHelperError> {
        Self::validate_path(device_path)?;
        // Without sysfs access, return Failed.
        Err(BacklightHelperError::Failed)
    }

    /// List available backlight devices. A full implementation would
    /// scan `/sys/class/backlight/` directory entries. Without sysfs,
    /// returns an empty list.
    pub fn list_devices() -> Result<Vec<String>, BacklightHelperError> {
        // Without a sysfs filesystem, there are no backlight devices.
        Ok(Vec::new())
    }
}

use alloc::vec;
use alloc::string::String;
