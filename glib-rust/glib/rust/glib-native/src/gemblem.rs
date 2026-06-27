//! GIO emblem matching `gio/gemblem.h` / `gio/gemblem.c`.
//!
//! Upstream `GEmblem` is a `GObject` subclass implementing `GIcon`.
//! It wraps an icon plus an origin enum. We port it as a plain struct.
//!
//! Provides:
//! - `EmblemOrigin` enum (Unknown/Device/LiveMetadata/Tag).
//! - `Emblem` struct (icon + origin).
//! - `new(icon)`, `new_with_origin(icon, origin)`.
//! - `icon()`, `origin()`.
//! - `hash()`, `equal()`, `to_string()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gicon::Icon;
use crate::prelude::*;
use alloc::string::String;
use alloc::string::ToString;

/// Origin of an emblem (`GEmblemOrigin`).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmblemOrigin {
    /// Emblem of unknown origin.
    Unknown = 0,
    /// Emblem adds device-specific information.
    Device = 1,
    /// Emblem depicts live metadata, such as "readonly".
    LiveMetadata = 2,
    /// Emblem comes from a user-defined tag.
    Tag = 3,
}

impl EmblemOrigin {
    /// Parse from an integer value.
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(EmblemOrigin::Unknown),
            1 => Some(EmblemOrigin::Device),
            2 => Some(EmblemOrigin::LiveMetadata),
            3 => Some(EmblemOrigin::Tag),
            _ => None,
        }
    }

    /// Convert to the nick name used in serialization.
    pub fn nick(&self) -> &'static str {
        match self {
            EmblemOrigin::Unknown => "unknown",
            EmblemOrigin::Device => "device",
            EmblemOrigin::LiveMetadata => "livemetadata",
            EmblemOrigin::Tag => "tag",
        }
    }
}

/// An emblem (`GEmblem`).
///
/// An icon with additional metadata about its origin. Can be added
/// to an `EmblemedIcon`.
///
/// Plain struct port of the upstream GObject+GIcon subclass.
#[derive(Clone, Debug)]
pub struct Emblem {
    icon: Box<Icon>,
    origin: EmblemOrigin,
}

impl Emblem {
    /// Creates a new emblem for the given icon with `Unknown` origin.
    ///
    /// Mirrors `g_emblem_new`.
    pub fn new(icon: Icon) -> Self {
        Emblem {
            icon: Box::new(icon),
            origin: EmblemOrigin::Unknown,
        }
    }

    /// Creates a new emblem for the given icon with a specific origin.
    ///
    /// Mirrors `g_emblem_new_with_origin`.
    pub fn new_with_origin(icon: Icon, origin: EmblemOrigin) -> Self {
        Emblem {
            icon: Box::new(icon),
            origin,
        }
    }

    /// Gets the icon from the emblem.
    ///
    /// Mirrors `g_emblem_get_icon`.
    pub fn icon(&self) -> &Icon {
        &self.icon
    }

    /// Gets the origin of the emblem.
    ///
    /// Mirrors `g_emblem_get_origin`.
    pub fn origin(&self) -> EmblemOrigin {
        self.origin
    }

    /// Computes a hash for the emblem (icon hash XOR origin).
    ///
    /// Mirrors `g_emblem_hash`.
    pub fn hash(&self) -> u32 {
        self.icon.hash() ^ (self.origin as u32)
    }

    /// Checks if two emblems are equal (same origin and icon).
    ///
    /// Mirrors `g_emblem_equal`.
    pub fn equal(&self, other: &Self) -> bool {
        self.origin == other.origin && self.icon.equal(&other.icon)
    }

    /// Serializes to a string representation.
    ///
    /// Format: `"<icon_string> <origin_int>"`.
    pub fn to_string(&self) -> String {
        format!("{} {}", self.icon.to_string(), self.origin as i32)
    }
}

impl PartialEq for Emblem {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for Emblem {}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gthemedicon::ThemedIcon;

    fn make_icon() -> Icon {
        Icon::Themed(ThemedIcon::new("folder"))
    }

    #[test]
    fn test_new() {
        let emblem = Emblem::new(make_icon());
        assert_eq!(emblem.origin(), EmblemOrigin::Unknown);
    }

    #[test]
    fn test_new_with_origin() {
        let emblem = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        assert_eq!(emblem.origin(), EmblemOrigin::Device);
    }

    #[test]
    fn test_icon() {
        let emblem = Emblem::new(make_icon());
        match emblem.icon() {
            Icon::Themed(ti) => assert!(ti.names().iter().any(|n| n == "folder")),
            _ => panic!("expected Themed icon"),
        }
    }

    #[test]
    fn test_equal_same() {
        let a = Emblem::new_with_origin(make_icon(), EmblemOrigin::Tag);
        let b = Emblem::new_with_origin(make_icon(), EmblemOrigin::Tag);
        assert!(a.equal(&b));
    }

    #[test]
    fn test_equal_different_origin() {
        let a = Emblem::new_with_origin(make_icon(), EmblemOrigin::Tag);
        let b = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_equal_different_icon() {
        let a = Emblem::new(Icon::Themed(ThemedIcon::new("folder")));
        let b = Emblem::new(Icon::Themed(ThemedIcon::new("file")));
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_hash_consistency() {
        let a = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        let b = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn test_hash_different_origin() {
        let a = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        let b = Emblem::new_with_origin(make_icon(), EmblemOrigin::Tag);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn test_to_string() {
        let emblem = Emblem::new_with_origin(make_icon(), EmblemOrigin::Device);
        let s = emblem.to_string();
        assert!(s.contains("folder"));
        assert!(s.contains("1")); // Device = 1
    }

    #[test]
    fn test_origin_from_i32() {
        assert_eq!(EmblemOrigin::from_i32(0), Some(EmblemOrigin::Unknown));
        assert_eq!(EmblemOrigin::from_i32(1), Some(EmblemOrigin::Device));
        assert_eq!(EmblemOrigin::from_i32(2), Some(EmblemOrigin::LiveMetadata));
        assert_eq!(EmblemOrigin::from_i32(3), Some(EmblemOrigin::Tag));
        assert_eq!(EmblemOrigin::from_i32(4), None);
    }

    #[test]
    fn test_origin_nick() {
        assert_eq!(EmblemOrigin::Unknown.nick(), "unknown");
        assert_eq!(EmblemOrigin::Device.nick(), "device");
        assert_eq!(EmblemOrigin::LiveMetadata.nick(), "livemetadata");
        assert_eq!(EmblemOrigin::Tag.nick(), "tag");
    }

    #[test]
    fn test_clone() {
        let a = Emblem::new_with_origin(make_icon(), EmblemOrigin::Tag);
        let b = a.clone();
        assert!(a.equal(&b));
    }
}
