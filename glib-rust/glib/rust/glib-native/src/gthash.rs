//! `gthash` matching `girepository/gthash.c`.
//!
//! Hash table for typelib string deduplication.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Simple hash function for strings (mirrors `gthash` internal hash).
pub fn hash_string(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    hash
}

/// String hash table for deduplication (mirrors `GThtab`).
#[derive(Debug, Default)]
pub struct StringHash {
    entries: Vec<String>,
}

impl StringHash {
    /// Creates a new string hash.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a string and returns its index.
    /// If the string already exists, returns the existing index.
    pub fn insert(&mut self, s: &str) -> usize {
        if let Some(idx) = self.entries.iter().position(|e| e == s) {
            idx
        } else {
            self.entries.push(s.into());
            self.entries.len() - 1
        }
    }

    /// Looks up a string by index.
    pub fn get(&self, idx: usize) -> Option<&str> {
        self.entries.get(idx).map(|s| s.as_str())
    }

    /// Returns the number of unique strings.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_string() {
        assert_eq!(hash_string("hello"), hash_string("hello"));
        assert_ne!(hash_string("hello"), hash_string("world"));
    }

    #[test]
    fn test_insert_dedup() {
        let mut h = StringHash::new();
        let i1 = h.insert("foo");
        let i2 = h.insert("bar");
        let i3 = h.insert("foo");
        assert_eq!(i1, i3);
        assert_ne!(i1, i2);
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn test_get() {
        let mut h = StringHash::new();
        let idx = h.insert("test");
        assert_eq!(h.get(idx), Some("test"));
        assert_eq!(h.get(999), None);
    }
}
