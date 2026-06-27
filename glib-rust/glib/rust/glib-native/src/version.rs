//! Version information matching `gversion.h` / `gversion.c`.
//!
//! Provides GLib version constants and compatibility checking.
//! Fully `no_std` compatible.

use alloc::string::String;

/// GLib major version.
pub const GLIB_MAJOR_VERSION: u32 = 2;

/// GLib minor version.
pub const GLIB_MINOR_VERSION: u32 = 80;

/// GLib micro version.
pub const GLIB_MICRO_VERSION: u32 = 0;

/// Interface age.
pub const GLIB_INTERFACE_AGE: u32 = 0;

/// Binary age.
pub const GLIB_BINARY_AGE: u32 = 8000;

/// Check if the GLib version is at least the required version
/// (`glib_check_version`).
///
/// Returns `None` if compatible, or `Some(String)` with an error message
/// if the version is too old.
#[allow(clippy::absurd_extreme_comparisons)]
pub fn check_version(
    required_major: u32,
    required_minor: u32,
    required_micro: u32,
) -> Option<String> {
    if GLIB_MAJOR_VERSION > required_major {
        return None;
    }
    if GLIB_MAJOR_VERSION < required_major {
        return Some(format!(
            "GLib version too old ({}.{}, required {}.{})",
            GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION, required_major, required_minor
        ));
    }
    // Major versions match
    if GLIB_MINOR_VERSION > required_minor {
        return None;
    }
    if GLIB_MINOR_VERSION < required_minor {
        return Some(format!(
            "GLib version too old ({}.{}, required {}.{})",
            GLIB_MAJOR_VERSION, GLIB_MINOR_VERSION, required_major, required_minor
        ));
    }
    // Minor versions match
    if GLIB_MICRO_VERSION >= required_micro {
        None
    } else {
        Some(format!(
            "GLib version too old ({}.{}.{}, required {}.{}.{})",
            GLIB_MAJOR_VERSION,
            GLIB_MINOR_VERSION,
            GLIB_MICRO_VERSION,
            required_major,
            required_minor,
            required_micro
        ))
    }
}

/// Check if the current version is at least the given version
/// (`GLIB_CHECK_VERSION` macro).
#[allow(clippy::absurd_extreme_comparisons)]
pub fn check_version_bool(major: u32, minor: u32, micro: u32) -> bool {
    GLIB_MAJOR_VERSION > major
        || (GLIB_MAJOR_VERSION == major && GLIB_MINOR_VERSION > minor)
        || (GLIB_MAJOR_VERSION == major
            && GLIB_MINOR_VERSION == minor
            && GLIB_MICRO_VERSION >= micro)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_constants() {
        assert_eq!(GLIB_MAJOR_VERSION, 2);
        assert!(GLIB_MINOR_VERSION >= 0);
    }

    #[test]
    fn check_compatible() {
        assert!(check_version(2, 0, 0).is_none());
        assert!(check_version(2, 80, 0).is_none());
    }

    #[test]
    fn check_too_old() {
        assert!(check_version(3, 0, 0).is_some());
        assert!(check_version(2, 81, 0).is_some());
    }

    #[test]
    fn test_check_version_bool() {
        assert!(check_version_bool(2, 0, 0));
        assert!(check_version_bool(2, 80, 0));
        assert!(!check_version_bool(3, 0, 0));
        assert!(!check_version_bool(2, 81, 0));
    }
}
