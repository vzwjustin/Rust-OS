//! GIO emblemed icon matching `gio/gemblemedicon.h` /
//! `gio/gemblemedicon.c`.
//!
//! Upstream `GEmblemedIcon` is a `GObject` subclass implementing `GIcon`.
//! It wraps a base icon plus a sorted list of emblems. We port it as
//! a plain struct.
//!
//! Provides:
//! - `EmblemedIcon` struct (icon + emblems list).
//! - `new(icon, emblem)`, `get_icon()`, `get_emblems()`,
//!   `add_emblem()`, `clear_emblems()`.
//! - `hash()`, `equal()`, `to_string()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gemblem::Emblem;
use crate::gicon::Icon;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// An emblemed icon (`GEmblemedIcon`).
///
/// An icon with a list of emblems attached. Emblems are sorted by
/// their hash value when added (matching upstream's
/// `g_list_insert_sorted` with `g_emblem_comp`).
///
/// Plain struct port of the upstream GObject+GIcon subclass.
#[derive(Clone, Debug)]
pub struct EmblemedIcon {
    icon: Box<Icon>,
    emblems: Vec<Emblem>,
}

impl EmblemedIcon {
    /// Creates a new emblemed icon for the given icon, optionally with
    /// an initial emblem.
    ///
    /// Mirrors `g_emblemed_icon_new`.
    pub fn new(icon: Icon, emblem: Option<Emblem>) -> Self {
        let mut result = EmblemedIcon {
            icon: Box::new(icon),
            emblems: Vec::new(),
        };
        if let Some(e) = emblem {
            result.add_emblem(e);
        }
        result
    }

    /// Gets the main icon.
    ///
    /// Mirrors `g_emblemed_icon_get_icon`.
    pub fn get_icon(&self) -> &Icon {
        &self.icon
    }

    /// Gets the list of emblems.
    ///
    /// Mirrors `g_emblemed_icon_get_emblems`.
    pub fn get_emblems(&self) -> &[Emblem] {
        &self.emblems
    }

    /// Adds an emblem to the list (sorted by hash).
    ///
    /// Mirrors `g_emblemed_icon_add_emblem`.
    pub fn add_emblem(&mut self, emblem: Emblem) {
        let hash = emblem.hash();
        let pos = self.emblems.partition_point(|e| e.hash() <= hash);
        self.emblems.insert(pos, emblem);
    }

    /// Removes all emblems.
    ///
    /// Mirrors `g_emblemed_icon_clear_emblems`.
    pub fn clear_emblems(&mut self) {
        self.emblems.clear();
    }

    /// Computes a hash (icon hash XOR all emblem hashes).
    ///
    /// Mirrors `g_emblemed_icon_hash`.
    pub fn hash(&self) -> u32 {
        let mut h = self.icon.hash();
        for emblem in &self.emblems {
            h ^= emblem.hash();
        }
        h
    }

    /// Checks if two emblemed icons are equal (same icon + same emblems).
    ///
    /// Mirrors `g_emblemed_icon_equal`.
    pub fn equal(&self, other: &Self) -> bool {
        if !self.icon.equal(&other.icon) {
            return false;
        }
        if self.emblems.len() != other.emblems.len() {
            return false;
        }
        for (a, b) in self.emblems.iter().zip(other.emblems.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        true
    }

    /// Serializes to a string representation.
    ///
    /// Format: `"<icon_string> [<emblem_string>]*"`.
    pub fn to_string(&self) -> String {
        let mut s = self.icon.to_string();
        for emblem in &self.emblems {
            s.push(' ');
            s.push_str(&emblem.to_string());
        }
        s
    }
}

impl PartialEq for EmblemedIcon {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for EmblemedIcon {}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemblem::EmblemOrigin;
    use crate::gthemedicon::ThemedIcon;

    fn make_icon() -> Icon {
        Icon::Themed(ThemedIcon::new("folder"))
    }

    fn make_emblem(origin: EmblemOrigin) -> Emblem {
        Emblem::new_with_origin(Icon::Themed(ThemedIcon::new("emblem-default")), origin)
    }

    #[test]
    fn test_new_no_emblem() {
        let ei = EmblemedIcon::new(make_icon(), None);
        assert!(ei.get_emblems().is_empty());
    }

    #[test]
    fn test_new_with_emblem() {
        let emblem = make_emblem(EmblemOrigin::Device);
        let ei = EmblemedIcon::new(make_icon(), Some(emblem));
        assert_eq!(ei.get_emblems().len(), 1);
    }

    #[test]
    fn test_get_icon() {
        let ei = EmblemedIcon::new(make_icon(), None);
        match ei.get_icon() {
            Icon::Themed(ti) => assert!(ti.names().iter().any(|n| n == "folder")),
            _ => panic!("expected Themed icon"),
        }
    }

    #[test]
    fn test_add_emblem() {
        let mut ei = EmblemedIcon::new(make_icon(), None);
        ei.add_emblem(make_emblem(EmblemOrigin::Device));
        ei.add_emblem(make_emblem(EmblemOrigin::Tag));
        assert_eq!(ei.get_emblems().len(), 2);
    }

    #[test]
    fn test_clear_emblems() {
        let mut ei = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        ei.add_emblem(make_emblem(EmblemOrigin::Tag));
        assert_eq!(ei.get_emblems().len(), 2);
        ei.clear_emblems();
        assert!(ei.get_emblems().is_empty());
    }

    #[test]
    fn test_equal_same() {
        let a = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        let b = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        assert!(a.equal(&b));
    }

    #[test]
    fn test_equal_different_icon() {
        let a = EmblemedIcon::new(Icon::Themed(ThemedIcon::new("folder")), None);
        let b = EmblemedIcon::new(Icon::Themed(ThemedIcon::new("file")), None);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_equal_different_emblems() {
        let a = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        let b = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Tag)));
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_equal_different_emblem_count() {
        let a = EmblemedIcon::new(make_icon(), None);
        let b = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_hash_consistency() {
        let a = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        let b = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn test_to_string_no_emblems() {
        let ei = EmblemedIcon::new(make_icon(), None);
        let s = ei.to_string();
        assert!(s.contains("folder"));
    }

    #[test]
    fn test_to_string_with_emblems() {
        let ei = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        let s = ei.to_string();
        assert!(s.contains("folder"));
        assert!(s.contains("emblem-default"));
    }

    #[test]
    fn test_clone() {
        let a = EmblemedIcon::new(make_icon(), Some(make_emblem(EmblemOrigin::Device)));
        let b = a.clone();
        assert!(a.equal(&b));
    }

    #[test]
    fn test_emblems_sorted_by_hash() {
        let mut ei = EmblemedIcon::new(make_icon(), None);
        let e1 = make_emblem(EmblemOrigin::Tag);
        let e2 = make_emblem(EmblemOrigin::Device);
        ei.add_emblem(e1.clone());
        ei.add_emblem(e2.clone());
        // Emblems should be sorted by hash (non-decreasing).
        let hashes: Vec<u32> = ei.get_emblems().iter().map(|e| e.hash()).collect();
        assert!(hashes.windows(2).all(|w| w[0] <= w[1]));
    }
}
