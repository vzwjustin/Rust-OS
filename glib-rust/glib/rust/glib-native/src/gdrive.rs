//! GIO drive matching `gio/gdrive.h` / `gio/gdrive.c`.
//!
//! Upstream `GDrive` is a `GInterface` representing a physical or virtual
//! storage drive. We port it as a plain `Drive` trait plus a `SimpleDrive`
//! concrete struct suitable for use in no_std environments.
//!
//! Provides:
//! - `DriveStartFlags` enum.
//! - `Drive` trait with name, identifier, volume, and capability queries.
//! - `SimpleDrive` struct implementing `Drive` with stub defaults.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

// ──────────────────────────── DriveStartFlags ─────────────────────────────

/// Flags for starting a drive (`GDriveStartFlags`).
///
/// Mirrors `GDriveStartFlags` from `gio/gdrive.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DriveStartFlags {
    /// No special flags.
    None = 0,
}

// ──────────────────────────── Drive trait ─────────────────────────────────

/// Trait representing a physical or virtual storage drive (`GDrive`).
///
/// A drive corresponds to a hardware device or virtual block device and
/// may contain zero or more `Volume`s.
pub trait Drive {
    /// Returns the display name of the drive.
    ///
    /// Mirrors `g_drive_get_name`.
    fn get_name(&self) -> String;

    /// Returns an identifier of the given `kind` for the drive, or `None`.
    ///
    /// Common kind strings: `"unix-device"`, `"label"`, `"uuid"`.
    /// Mirrors `g_drive_get_identifier`.
    fn get_identifier(&self, kind: &str) -> Option<String>;

    /// Returns all identifier kinds supported by this drive.
    ///
    /// Mirrors `g_drive_enumerate_identifiers`.
    fn enumerate_identifiers(&self) -> Vec<String>;

    /// Returns whether this drive has any volumes.
    ///
    /// Mirrors `g_drive_has_volumes`.
    fn has_volumes(&self) -> bool;

    /// Returns whether the drive can be ejected.
    ///
    /// Mirrors `g_drive_can_eject`.
    fn can_eject(&self) -> bool;

    /// Returns whether the drive can be polled to check for media changes.
    ///
    /// Mirrors `g_drive_can_poll_for_media`.
    fn can_poll_for_media(&self) -> bool;

    /// Returns whether the media in the drive is removable.
    ///
    /// Mirrors `g_drive_is_media_removable`.
    fn is_media_removable(&self) -> bool;

    /// Returns whether the drive automatically checks for media changes.
    ///
    /// Mirrors `g_drive_is_media_check_automatic`.
    fn is_media_check_automatic(&self) -> bool;
}

// ──────────────────────────── SimpleDrive ─────────────────────────────────

/// A simple, in-memory `Drive` implementation.
///
/// Useful for testing and as a stub in environments without real block
/// device enumeration. Identifier queries always return `None`; volume,
/// poll, and automatic-check stubs return `false`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleDrive {
    name: String,
    can_eject: bool,
    is_media_removable: bool,
}

impl SimpleDrive {
    /// Creates a new `SimpleDrive`.
    pub fn new(name: impl Into<String>, can_eject: bool, is_media_removable: bool) -> Self {
        SimpleDrive {
            name: name.into(),
            can_eject,
            is_media_removable,
        }
    }
}

impl Drive for SimpleDrive {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_identifier(&self, _kind: &str) -> Option<String> {
        None
    }

    fn enumerate_identifiers(&self) -> Vec<String> {
        Vec::new()
    }

    fn has_volumes(&self) -> bool {
        false
    }

    fn can_eject(&self) -> bool {
        self.can_eject
    }

    fn can_poll_for_media(&self) -> bool {
        false
    }

    fn is_media_removable(&self) -> bool {
        self.is_media_removable
    }

    fn is_media_check_automatic(&self) -> bool {
        false
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_drive() -> SimpleDrive {
        SimpleDrive::new("USB Drive", true, true)
    }

    #[test]
    fn test_get_name() {
        let d = make_drive();
        assert_eq!(d.get_name(), "USB Drive");
    }

    #[test]
    fn test_can_eject_true() {
        let d = make_drive();
        assert!(d.can_eject());
    }

    #[test]
    fn test_is_media_removable_true() {
        let d = make_drive();
        assert!(d.is_media_removable());
    }

    #[test]
    fn test_enumerate_identifiers_empty() {
        let d = make_drive();
        assert!(d.enumerate_identifiers().is_empty());
    }

    #[test]
    fn test_stub_defaults() {
        let d = make_drive();
        assert!(!d.has_volumes());
        assert!(!d.can_poll_for_media());
        assert!(!d.is_media_check_automatic());
        assert_eq!(d.get_identifier("unix-device"), None);
    }
}
