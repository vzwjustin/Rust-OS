//! GIO bytes icon matching `gio/gbytesicon.h` / `gio/gbytesicon.c`.
//!
//! Upstream `GBytesIcon` is a `GObject` subclass implementing `GIcon`
//! and `GLoadableIcon`. It wraps a `GBytes` containing image data
//! (usually PNG). We port it as a plain struct wrapping `Bytes`.
//!
//! Provides:
//! - `BytesIcon` struct (wrapping `Bytes`).
//! - `new(bytes)`, `bytes()`.
//! - `hash()`, `equal()`, `to_string()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::prelude::*;
use alloc::string::String;

/// A bytes icon (`GBytesIcon`).
///
/// Specifies an image held in memory in a common format (usually PNG)
/// to be used as an icon.
///
/// Plain struct port of the upstream GObject+GIcon+GLoadableIcon subclass.
#[derive(Clone, Debug)]
pub struct BytesIcon {
    bytes: Bytes,
}

impl BytesIcon {
    /// Creates a new bytes icon for the given `Bytes`.
    ///
    /// Mirrors `g_bytes_icon_new`.
    pub fn new(bytes: Bytes) -> Self {
        BytesIcon { bytes }
    }

    /// Gets the `Bytes` associated with the icon.
    ///
    /// Mirrors `g_bytes_icon_get_bytes`.
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Computes a hash for the icon (delegates to `Bytes::hash`).
    ///
    /// Mirrors `g_bytes_icon_hash`.
    pub fn hash(&self) -> u32 {
        self.bytes.hash()
    }

    /// Checks if two bytes icons are equal (same bytes content).
    ///
    /// Mirrors `g_bytes_icon_equal`.
    pub fn equal(&self, other: &Self) -> bool {
        self.bytes.equal(&other.bytes)
    }

    /// Serializes to a string representation.
    ///
    /// Returns `"bytes"` as a placeholder (full hex encoding deferred).
    pub fn to_string(&self) -> String {
        String::from("bytes")
    }
}

impl PartialEq for BytesIcon {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for BytesIcon {}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let bytes = Bytes::from_static(b"png data here");
        let icon = BytesIcon::new(bytes);
        assert_eq!(icon.bytes().as_ref(), b"png data here");
    }

    #[test]
    fn test_equal_same() {
        let bytes = Bytes::from_static(b"icon data");
        let a = BytesIcon::new(bytes.clone());
        let b = BytesIcon::new(bytes);
        assert!(a.equal(&b));
    }

    #[test]
    fn test_equal_different() {
        let a = BytesIcon::new(Bytes::from_static(b"icon1"));
        let b = BytesIcon::new(Bytes::from_static(b"icon2"));
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_hash_consistency() {
        let bytes = Bytes::from_static(b"icon data");
        let a = BytesIcon::new(bytes.clone());
        let b = BytesIcon::new(bytes);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn test_hash_different() {
        let a = BytesIcon::new(Bytes::from_static(b"icon1"));
        let b = BytesIcon::new(Bytes::from_static(b"icon2"));
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn test_to_string() {
        let icon = BytesIcon::new(Bytes::from_static(b"data"));
        assert_eq!(icon.to_string(), "bytes");
    }

    #[test]
    fn test_clone() {
        let a = BytesIcon::new(Bytes::from_static(b"data"));
        let b = a.clone();
        assert!(a.equal(&b));
    }
}
