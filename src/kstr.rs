// SPDX-License-Identifier: GPL-2.0
//! Kernel string types — ported from Linux `rust/kernel/str.rs`.
//!
//! Named `kstr` to avoid clashing with Rust's built-in `str` type.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use alloc::vec::Vec;
use core::{
    fmt,
    ops::Deref,
    ptr, slice, str,
};

// ---------------------------------------------------------------------------
// BStr — byte string slice (not necessarily UTF-8)
// ---------------------------------------------------------------------------

/// A byte string slice without any UTF-8 validity guarantee.
///
/// `BStr` is to `[u8]` what `str` is to valid UTF-8.  It carries no guarantee
/// about encoding; it just records that we intend to treat the bytes as text.
#[repr(transparent)]
pub struct BStr([u8]);

impl BStr {
    /// Returns the number of bytes in this string.
    #[inline]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if this string contains no bytes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Creates a `&BStr` from a byte slice.
    ///
    /// # Safety
    ///
    /// The resulting reference has the same lifetime as `bytes`.
    #[inline]
    pub const fn from_bytes(bytes: &[u8]) -> &Self {
        // SAFETY: `BStr` is `#[repr(transparent)]` over `[u8]`.
        unsafe { &*(bytes as *const [u8] as *const BStr) }
    }

    /// Returns the underlying byte slice.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Strip a prefix if present, analogous to `[u8]::strip_prefix`.
    pub fn strip_prefix(&self, prefix: &BStr) -> Option<&BStr> {
        self.0
            .strip_prefix(prefix.as_bytes())
            .map(BStr::from_bytes)
    }

    /// Strip a suffix if present.
    pub fn strip_suffix(&self, suffix: &BStr) -> Option<&BStr> {
        self.0
            .strip_suffix(suffix.as_bytes())
            .map(BStr::from_bytes)
    }

    /// Try to interpret this `BStr` as a UTF-8 `str`.
    pub fn as_str(&self) -> Result<&str, str::Utf8Error> {
        str::from_utf8(&self.0)
    }
}

impl Deref for BStr {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for BStr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl PartialEq for BStr {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for BStr {}

impl fmt::Display for BStr {
    /// Print printable ASCII characters; escape the rest as `\xNN`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &b in &self.0 {
            match b {
                b'\t' => f.write_str("\\t")?,
                b'\n' => f.write_str("\\n")?,
                b'\r' => f.write_str("\\r")?,
                0x20..=0x7e => f.write_char(b as char)?,
                _ => write!(f, "\\x{b:02x}")?,
            }
        }
        Ok(())
    }
}

impl fmt::Debug for BStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        for &b in &self.0 {
            match b {
                b'\t' => f.write_str("\\t")?,
                b'\n' => f.write_str("\\n")?,
                b'\r' => f.write_str("\\r")?,
                b'"'  => f.write_str("\\\"")?,
                b'\\' => f.write_str("\\\\")?,
                0x20..=0x7e => f.write_char(b as char)?,
                _ => write!(f, "\\x{b:02x}")?,
            }
        }
        f.write_char('"')
    }
}

/// Construct a `&BStr` from a byte-string literal.
///
/// ```rust,ignore
/// let s: &BStr = b_str!("hello");
/// ```
#[macro_export]
macro_rules! b_str {
    ($str:literal) => {{
        const BYTES: &[u8] = $str.as_bytes();
        $crate::kstr::BStr::from_bytes(BYTES)
    }};
}

// ---------------------------------------------------------------------------
// BString — owned byte string
// ---------------------------------------------------------------------------

/// An owned, heap-allocated byte string.
pub struct BString(Vec<u8>);

impl BString {
    /// Creates an empty `BString`.
    pub fn new() -> Self {
        BString(Vec::new())
    }

    /// Creates a `BString` from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        BString(bytes)
    }

    /// Extracts the inner byte vector.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl Deref for BString {
    type Target = BStr;
    fn deref(&self) -> &BStr {
        BStr::from_bytes(&self.0)
    }
}

impl AsRef<BStr> for BString {
    fn as_ref(&self) -> &BStr {
        self
    }
}

impl Default for BString {
    fn default() -> Self {
        BString::new()
    }
}

impl fmt::Display for BString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.deref(), f)
    }
}

impl fmt::Debug for BString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.deref(), f)
    }
}

// ---------------------------------------------------------------------------
// CStr — null-terminated byte string
// ---------------------------------------------------------------------------

/// A null-terminated byte string, equivalent to C's `const char *`.
///
/// The invariant is that the byte sequence contains exactly one null byte and
/// it is located at the end.
#[repr(transparent)]
pub struct CStr([u8]);

impl CStr {
    /// Creates a `&CStr` from a byte slice that ends with a null byte.
    ///
    /// Returns `None` if `bytes` does not end with `\0` or contains an
    /// interior null byte.
    pub fn from_bytes_with_nul(bytes: &[u8]) -> Option<&Self> {
        if bytes.last() != Some(&b'\0') {
            return None;
        }
        if bytes[..bytes.len() - 1].contains(&b'\0') {
            return None;
        }
        // SAFETY: we checked the invariant.
        Some(unsafe { Self::from_bytes_with_nul_unchecked(bytes) })
    }

    /// Creates a `&CStr` from a byte slice without checking the invariant.
    ///
    /// # Safety
    ///
    /// `bytes` must end with exactly one null byte and contain no interior
    /// null bytes.
    #[inline]
    pub const unsafe fn from_bytes_with_nul_unchecked(bytes: &[u8]) -> &Self {
        // SAFETY: `CStr` is `#[repr(transparent)]` over `[u8]`.
        unsafe { &*(bytes as *const [u8] as *const CStr) }
    }

    /// Creates a `&CStr` from a raw C string pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid null-terminated C string that lives at
    /// least as long as `'a`.
    pub unsafe fn from_char_ptr<'a>(ptr: *const u8) -> &'a Self {
        let mut len = 0usize;
        // SAFETY: caller guarantees `ptr` is a valid C string.
        while unsafe { *ptr.add(len) } != b'\0' {
            len += 1;
        }
        // Include the null terminator.
        let bytes = unsafe { slice::from_raw_parts(ptr, len + 1) };
        // SAFETY: we just verified the null-terminator invariant.
        unsafe { Self::from_bytes_with_nul_unchecked(bytes) }
    }

    /// Returns the bytes of this C string, including the trailing null.
    #[inline]
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        &self.0
    }

    /// Returns the bytes of this C string, excluding the trailing null.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let bytes = &self.0;
        &bytes[..bytes.len() - 1]
    }

    /// Returns a raw pointer to the start of this C string.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Returns the length of this C string, not counting the trailing null.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len() - 1
    }

    /// Returns `true` if this C string is empty (has length 0).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for CStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &b in self.as_bytes() {
            match b {
                0x20..=0x7e => f.write_char(b as char)?,
                _ => write!(f, "\\x{b:02x}")?,
            }
        }
        Ok(())
    }
}

impl fmt::Debug for CStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(BStr::from_bytes(self.as_bytes()), f)
    }
}

/// Construct a `&CStr` from a string literal, adding a null terminator.
///
/// ```rust,ignore
/// let s: &CStr = c_str!("hello");
/// assert_eq!(s.as_bytes(), b"hello");
/// ```
#[macro_export]
macro_rules! c_str {
    ($str:expr) => {{
        const BYTES: &[u8] = concat!($str, "\0").as_bytes();
        // SAFETY: we just appended exactly one null byte.
        unsafe { $crate::kstr::CStr::from_bytes_with_nul_unchecked(BYTES) }
    }};
}
