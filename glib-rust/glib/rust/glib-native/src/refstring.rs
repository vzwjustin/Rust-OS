//! Reference-counted strings matching `grefstring.h` / `grefstring.c`.
//!
//! Uses `alloc::sync::Arc` for reference counting.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::sync::Arc;

/// A reference-counted string (`GRefString`).
///
/// Wraps an `Arc<str>` so the string can be shared and reference-counted.
#[derive(Clone, Debug)]
pub struct RefString {
    inner: Arc<str>,
}

impl RefString {
    /// Create a new reference-counted string (`g_ref_string_new`).
    pub fn new(s: &str) -> Self {
        Self {
            inner: Arc::from(s),
        }
    }

    /// Create a new reference-counted string with length (`g_ref_string_new_len`).
    pub fn new_len(s: &str, len: usize) -> Self {
        let truncated = if len <= s.len() { &s[..len] } else { s };
        Self {
            inner: Arc::from(truncated),
        }
    }

    /// Create a new interned reference-counted string (`g_ref_string_new_intern`).
    ///
    /// In this implementation, interning is handled by the quark system
    /// if available. For now, this is equivalent to `new`.
    pub fn new_intern(s: &str) -> Self {
        Self::new(s)
    }

    /// Acquire a reference (`g_ref_string_acquire`).
    ///
    /// This is equivalent to `Clone` in Rust.
    pub fn acquire(&self) -> Self {
        self.clone()
    }

    /// Release a reference (`g_ref_string_release`).
    ///
    /// In Rust, this is automatic when the `Arc` refcount drops to zero.
    /// This method is a no-op but provided for API compatibility.
    pub fn release(self) {
        // Drop is automatic
    }

    /// Get the length of the string (`g_ref_string_length`).
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Check equality (`g_ref_string_equal`).
    pub fn equal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner) || *self.inner == *other.inner
    }

    /// Get the string slice.
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl PartialEq for RefString {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for RefString {}

impl core::fmt::Display for RefString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.inner)
    }
}

impl From<&str> for RefString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for RefString {
    fn from(s: String) -> Self {
        Self {
            inner: Arc::from(s.as_str()),
        }
    }
}

impl AsRef<str> for RefString {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl core::borrow::Borrow<str> for RefString {
    fn borrow(&self) -> &str {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_as_str() {
        let s = RefString::new("hello");
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn new_len() {
        let s = RefString::new_len("hello world", 5);
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn acquire_release() {
        let s = RefString::new("test");
        let s2 = s.acquire();
        assert_eq!(s.as_str(), s2.as_str());
        s2.release();
    }

    #[test]
    fn equal() {
        let a = RefString::new("foo");
        let b = RefString::new("foo");
        assert!(a.equal(&b));
    }

    #[test]
    fn not_equal() {
        let a = RefString::new("foo");
        let b = RefString::new("bar");
        assert!(!a.equal(&b));
    }

    #[test]
    fn is_empty() {
        assert!(RefString::new("").is_empty());
        assert!(!RefString::new("x").is_empty());
    }

    #[test]
    fn display() {
        let s = RefString::new("world");
        assert_eq!(format!("{}", s), "world");
    }

    #[test]
    fn from_string() {
        let s = RefString::from(String::from("from string"));
        assert_eq!(s.as_str(), "from string");
    }
}
