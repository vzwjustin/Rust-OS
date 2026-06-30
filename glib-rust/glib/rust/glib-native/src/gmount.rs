//! GIO mount matching `gio/gmount.h` / `gio/gmount.c`.
//!
//! Upstream `GMount` is a `GInterface` representing a mounted storage
//! location. We port it as a plain `Mount` trait plus a `SimpleMount`
//! concrete struct suitable for use in no_std environments.
//!
//! Provides:
//! - `Mount` trait with name, UUID, capability, and location queries.
//! - `SimpleMount` struct implementing `Mount`.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;

// ──────────────────────────── Mount trait ─────────────────────────────────

/// Trait representing a mounted storage location (`GMount`).
///
/// A mount is the in-kernel attachment of a volume to a path in the
/// filesystem namespace. The trait provides read-only queries about the
/// mount's identity and capabilities; actual unmount/eject operations
/// would be performed through platform-specific code.
pub trait Mount {
    /// Returns the display name of the mount.
    ///
    /// Mirrors `g_mount_get_name`.
    fn get_name(&self) -> String;

    /// Returns the UUID of the mount, if known.
    ///
    /// Mirrors `g_mount_get_uuid`.
    fn get_uuid(&self) -> Option<String>;

    /// Returns whether the mount can be unmounted.
    ///
    /// Mirrors `g_mount_can_unmount`.
    fn can_unmount(&self) -> bool;

    /// Returns whether the mount can be ejected.
    ///
    /// Mirrors `g_mount_can_eject`.
    fn can_eject(&self) -> bool;

    /// Returns the default location for the mount as a path string.
    ///
    /// Mirrors `g_mount_get_default_location`.
    fn get_default_location(&self) -> String;
}

// ──────────────────────────── SimpleMount ─────────────────────────────────

/// A simple, in-memory `Mount` implementation.
///
/// Useful for testing and as a stub in environments without real VFS
/// mount enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleMount {
    name: String,
    uuid: Option<String>,
    can_unmount: bool,
    can_eject: bool,
    default_location: String,
}

impl SimpleMount {
    /// Creates a new `SimpleMount`.
    pub fn new(
        name: impl Into<String>,
        uuid: Option<String>,
        can_unmount: bool,
        can_eject: bool,
        default_location: impl Into<String>,
    ) -> Self {
        SimpleMount {
            name: name.into(),
            uuid,
            can_unmount,
            can_eject,
            default_location: default_location.into(),
        }
    }
}

impl Mount for SimpleMount {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_uuid(&self) -> Option<String> {
        self.uuid.clone()
    }

    fn can_unmount(&self) -> bool {
        self.can_unmount
    }

    fn can_eject(&self) -> bool {
        self.can_eject
    }

    fn get_default_location(&self) -> String {
        self.default_location.clone()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mount() -> SimpleMount {
        SimpleMount::new(
            "My Disk",
            Some("cafe-1234".into()),
            true,
            false,
            "/media/mydisk",
        )
    }

    #[test]
    fn test_get_name() {
        let m = make_mount();
        assert_eq!(m.get_name(), "My Disk");
    }

    #[test]
    fn test_get_uuid() {
        let m = make_mount();
        assert_eq!(m.get_uuid(), Some("cafe-1234".into()));
    }

    #[test]
    fn test_can_unmount_true() {
        let m = make_mount();
        assert!(m.can_unmount());
    }

    #[test]
    fn test_can_eject_false() {
        let m = make_mount();
        assert!(!m.can_eject());
    }

    #[test]
    fn test_get_default_location() {
        let m = make_mount();
        assert_eq!(m.get_default_location(), "/media/mydisk");
    }
}
