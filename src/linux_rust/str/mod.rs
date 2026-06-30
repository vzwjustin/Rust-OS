//! String types.
//!
//! Ported from Linux `rust/kernel/str.rs` and `rust/kernel/str/parse_int.rs`.

pub mod parse_int;

pub use parse_int::ParseInt;

use alloc::string::String;
use alloc::vec::Vec;

/// Byte string without UTF-8 validity guarantee.
///
/// Ported from Linux `rust/kernel/str.rs` `BStr`.
#[repr(transparent)]
pub struct BStr([u8]);

impl BStr {
    /// Create a `BStr` from a byte slice.
    pub const fn from_bytes(bytes: &[u8]) -> &Self {
        // SAFETY: `BStr` is `repr(transparent)` over `[u8]`.
        unsafe { &*(bytes as *const [u8] as *const BStr) }
    }

    /// Returns the length.
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if empty.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the underlying bytes.
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl core::ops::Deref for BStr {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl core::fmt::Debug for BStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.0, f)
    }
}

impl core::fmt::Display for BStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Best-effort UTF-8 display
        core::fmt::Display::fmt(
            core::str::from_utf8(&self.0).unwrap_or("<invalid utf-8>"),
            f,
        )
    }
}

impl AsRef<[u8]> for BStr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Create a `BStr` from a string literal.
#[macro_export]
macro_rules! b_str {
    ($s:literal) => {
        $crate::linux_rust::str::BStr::from_bytes($s.as_bytes())
    };
}

/// C string wrapper for no_std.
///
/// Ported from Linux `rust/kernel/str.rs` `CStr`.
#[repr(transparent)]
pub struct CStr([u8]);

impl CStr {
    /// Create from a nul-terminated byte slice.
    pub const fn from_bytes_with_nul(bytes: &[u8]) -> Option<&Self> {
        if bytes.is_empty() || bytes[bytes.len() - 1] != 0 {
            return None;
        }
        // Check for embedded nuls
        let mut i = 0;
        while i < bytes.len() - 1 {
            if bytes[i] == 0 {
                return None;
            }
            i += 1;
        }
        // SAFETY: `CStr` is `repr(transparent)` over `[u8]`.
        Some(unsafe { &*(bytes as *const [u8] as *const CStr) })
    }

    /// Create from a string literal (must not contain interior nuls).
    pub const fn from_str(s: &str) -> &Self {
        let bytes = s.as_bytes();
        // SAFETY: String literals are nul-terminated in the binary.
        // We rely on the caller to ensure no interior nuls.
        unsafe { &*(bytes as *const [u8] as *const CStr) }
    }

    /// Returns the bytes without the trailing nul.
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0[..self.0.len() - 1]
    }

    /// Returns the bytes including the trailing nul.
    pub const fn as_bytes_with_nul(&self) -> &[u8] {
        &self.0
    }

    /// Length without nul.
    pub const fn len(&self) -> usize {
        self.0.len() - 1
    }

    /// Returns `true` if empty (just a nul).
    pub const fn is_empty(&self) -> bool {
        self.0.len() <= 1
    }
}

impl core::fmt::Debug for CStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(
            core::str::from_utf8(self.as_bytes()).unwrap_or("<invalid utf-8>"),
            f,
        )
    }
}

impl core::fmt::Display for CStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(
            core::str::from_utf8(self.as_bytes()).unwrap_or("<invalid utf-8>"),
            f,
        )
    }
}
