//! GIO volume matching `gio/gvolume.h` / `gio/gvolume.c`.
//!
//! Upstream `GVolume` is a `GInterface` representing a storage volume that
//! may or may not be mounted. We port it as a plain `Volume` trait plus a
//! `SimpleVolume` concrete struct suitable for use in no_std environments.
//!
//! Provides:
//! - `MountUnmountFlags` enum.
//! - `Volume` trait with mount/eject/automount queries.
//! - `SimpleVolume` struct implementing `Volume`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::prelude::*;
use alloc::string::String;

// ──────────────────────────── MountUnmountFlags ───────────────────────────

/// Flags controlling mount/unmount operations (`GMountUnmountFlags`).
///
/// Mirrors `GMountUnmountFlags` from `gio/gvolume.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MountUnmountFlags {
    /// No special flags.
    None = 0,
    /// Force an unmount even if busy.
    Force = 1,
}

// ──────────────────────────── Volume trait ────────────────────────────────

/// Trait representing a storage volume (`GVolume`).
///
/// A volume is a potentially-mountable piece of storage (e.g. a USB stick,
/// optical disc, or network share). Whether it is currently mounted is tracked
/// separately by `Mount`.
pub trait Volume {
    /// Returns the display name of the volume.
    ///
    /// Mirrors `g_volume_get_name`.
    fn get_name(&self) -> String;

    /// Returns the UUID of the volume, if known.
    ///
    /// Mirrors `g_volume_get_uuid`.
    fn get_uuid(&self) -> Option<String>;

    /// Returns whether the volume can currently be mounted.
    ///
    /// Mirrors `g_volume_can_mount`.
    fn can_mount(&self) -> bool;

    /// Returns whether the volume can currently be ejected.
    ///
    /// Mirrors `g_volume_can_eject`.
    fn can_eject(&self) -> bool;

    /// Returns whether the volume should be automatically mounted.
    ///
    /// Mirrors `g_volume_should_automount`.
    fn should_automount(&self) -> bool;

    /// Returns an optional sort key for displaying volumes in a consistent
    /// order in a user interface.
    ///
    /// Mirrors `g_volume_get_sort_key`. Default: `None`.
    fn get_sort_key(&self) -> Option<String> {
        None
    }
}

// ──────────────────────────── SimpleVolume ────────────────────────────────

/// A simple, in-memory `Volume` implementation.
///
/// Useful for testing and as a stub in environments without real block
/// device enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleVolume {
    name: String,
    uuid: Option<String>,
    can_mount: bool,
    can_eject: bool,
    should_automount: bool,
}

impl SimpleVolume {
    /// Creates a new `SimpleVolume`.
    pub fn new(
        name: impl Into<String>,
        uuid: Option<String>,
        can_mount: bool,
        can_eject: bool,
        should_automount: bool,
    ) -> Self {
        SimpleVolume {
            name: name.into(),
            uuid,
            can_mount,
            can_eject,
            should_automount,
        }
    }
}

impl Volume for SimpleVolume {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_uuid(&self) -> Option<String> {
        self.uuid.clone()
    }

    fn can_mount(&self) -> bool {
        self.can_mount
    }

    fn can_eject(&self) -> bool {
        self.can_eject
    }

    fn should_automount(&self) -> bool {
        self.should_automount
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_volume() -> SimpleVolume {
        SimpleVolume::new(
            "USB Stick",
            Some("dead-beef-1234".into()),
            true,
            true,
            false,
        )
    }

    #[test]
    fn test_get_name() {
        let v = make_volume();
        assert_eq!(v.get_name(), "USB Stick");
    }

    #[test]
    fn test_get_uuid() {
        let v = make_volume();
        assert_eq!(v.get_uuid(), Some("dead-beef-1234".into()));
    }

    #[test]
    fn test_can_mount_and_eject() {
        let v = make_volume();
        assert!(v.can_mount());
        assert!(v.can_eject());
    }

    #[test]
    fn test_should_automount_false() {
        let v = make_volume();
        assert!(!v.should_automount());
    }

    #[test]
    fn test_sort_key_default_none() {
        let v = make_volume();
        assert_eq!(v.get_sort_key(), None);
    }
}
