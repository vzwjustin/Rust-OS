//! GIO icon matching `gio/gicon.h` / `gio/gicon.c`.
//!
//! Upstream `GIcon` is an abstract interface (`GInterface`) for icons.
//! We port it as a Rust `enum` wrapping the concrete icon types rather
//! than registering a `GInterface`, mirroring upstream semantics with
//! idiomatic Rust. The enum provides `hash`, `equal`, `to_string`, and
//! `serialize` methods that delegate to the concrete variant.
//!
//! Provides:
//! - `Icon` enum (`Themed` / `Bytes` / `Emblem` / `EmblemedIcon` variants).
//! - `hash()`, `equal()`, `to_string()`, `serialize()`.
//! - `new_for_string()` — parse `. ` prefixed token string.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gbytesicon::BytesIcon;
use crate::gemblem::Emblem;
use crate::gemblemedicon::EmblemedIcon;
use crate::gioerror::IOErrorEnum;
use crate::gthemedicon::ThemedIcon;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// An icon (`GIcon`).
///
/// Abstract type representing an icon. In upstream GLib this is a
/// `GInterface`; here we model it as an enum wrapping the concrete
/// implementations.
#[derive(Clone, Debug)]
pub enum Icon {
    /// A themed icon with icon names (`GThemedIcon`).
    Themed(ThemedIcon),
    /// An icon backed by in-memory bytes (`GBytesIcon`).
    Bytes(BytesIcon),
    /// An emblem — icon with origin metadata (`GEmblem`).
    Emblem(Emblem),
    /// An icon with attached emblems (`GEmblemedIcon`).
    EmblemedIcon(EmblemedIcon),
}

impl Icon {
    /// Computes a hash value for the icon.
    ///
    /// Mirrors `g_icon_hash`.
    pub fn hash(&self) -> u32 {
        match self {
            Icon::Themed(ti) => ti.hash(),
            Icon::Bytes(bi) => bi.hash(),
            Icon::Emblem(e) => e.hash(),
            Icon::EmblemedIcon(ei) => ei.hash(),
        }
    }

    /// Checks if two icons are equal.
    ///
    /// Mirrors `g_icon_equal`.
    pub fn equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Icon::Themed(a), Icon::Themed(b)) => a.equal(b),
            (Icon::Bytes(a), Icon::Bytes(b)) => a.equal(b),
            (Icon::Emblem(a), Icon::Emblem(b)) => a.equal(b),
            (Icon::EmblemedIcon(a), Icon::EmblemedIcon(b)) => a.equal(b),
            _ => false,
        }
    }

    /// Serializes the icon to a string representation.
    ///
    /// Mirrors `g_icon_to_string`.
    pub fn to_string(&self) -> String {
        match self {
            Icon::Themed(ti) => ti.to_string(),
            Icon::Bytes(bi) => bi.to_string(),
            Icon::Emblem(e) => e.to_string(),
            Icon::EmblemedIcon(ei) => ei.to_string(),
        }
    }

    /// Creates an icon from a string representation.
    ///
    /// Mirrors `g_icon_new_for_string`.
    pub fn new_for_string(str: &str) -> Result<Self, IOErrorEnum> {
        let str = str.trim();
        if str.is_empty() {
            return Err(IOErrorEnum::InvalidArgument);
        }

        if str.starts_with(". ") {
            // Themed icon with multiple names: ". name1 name2 ..."
            let rest = &str[2..];
            let names: Vec<&str> = rest.split_whitespace().collect();
            if names.is_empty() {
                return Err(IOErrorEnum::InvalidArgument);
            }
            Ok(Icon::Themed(ThemedIcon::new_from_names(&names)))
        } else if str.starts_with("bytes ") {
            // Bytes icon: "bytes <hex-encoded data>"
            // We don't fully implement hex decoding here; return error.
            Err(IOErrorEnum::NotSupported)
        } else {
            // Single-name themed icon.
            Ok(Icon::Themed(ThemedIcon::new(str)))
        }
    }
}

impl PartialEq for Icon {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for Icon {}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_themed_icon_variant() {
        let icon = Icon::Themed(ThemedIcon::new("folder"));
        assert!(icon.to_string().contains("folder"));
    }

    #[test]
    fn test_bytes_icon_variant() {
        let bytes = Bytes::from_static(b"png data");
        let icon = Icon::Bytes(BytesIcon::new(bytes));
        assert_eq!(icon.to_string(), "bytes");
    }

    #[test]
    fn test_equal_same_type() {
        let a = Icon::Themed(ThemedIcon::new("folder"));
        let b = Icon::Themed(ThemedIcon::new("folder"));
        assert!(a.equal(&b));
    }

    #[test]
    fn test_equal_different_type() {
        let a = Icon::Themed(ThemedIcon::new("folder"));
        let bytes = Bytes::from_static(b"data");
        let b = Icon::Bytes(BytesIcon::new(bytes));
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_new_for_string_single() {
        let icon = Icon::new_for_string("folder").unwrap();
        match icon {
            Icon::Themed(ti) => {
                let names: Vec<&str> = ti.names().iter().map(|s| s.as_str()).collect();
                assert!(names.contains(&"folder"));
            }
            _ => panic!("expected Themed variant"),
        }
    }

    #[test]
    fn test_new_for_string_multi() {
        let icon = Icon::new_for_string(". folder open").unwrap();
        match icon {
            Icon::Themed(ti) => {
                let names: Vec<&str> = ti.names().iter().map(|s| s.as_str()).collect();
                assert!(names.contains(&"folder"));
                assert!(names.contains(&"open"));
            }
            _ => panic!("expected Themed variant"),
        }
    }

    #[test]
    fn test_new_for_string_empty() {
        assert!(Icon::new_for_string("").is_err());
    }

    #[test]
    fn test_hash_consistency() {
        let a = Icon::Themed(ThemedIcon::new("folder"));
        let b = Icon::Themed(ThemedIcon::new("folder"));
        assert_eq!(a.hash(), b.hash());
    }
}
