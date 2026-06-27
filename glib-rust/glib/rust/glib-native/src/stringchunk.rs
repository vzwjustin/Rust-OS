//! String chunk matching `gstringchunk.h` / `gstringchunk.c`.
//!
//! Provides a string pool that deduplicates strings and keeps them alive.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;

/// A string chunk (`GStringChunk`).
///
/// Stores strings in a pool, returning stable references. Inserted strings
/// are deduplicated via `insert_const`.
pub struct StringChunk {
    strings: BTreeMap<String, usize>,
    storage: Vec<String>,
}

impl StringChunk {
    /// Create a new string chunk (`g_string_chunk_new`).
    ///
    /// The `size` parameter is ignored in this implementation (GLib uses it
    /// for pre-allocation hints, but we use a BTreeMap).
    pub fn new(_size: usize) -> Self {
        Self {
            strings: BTreeMap::new(),
            storage: Vec::new(),
        }
    }

    /// Insert a string into the chunk (`g_string_chunk_insert`).
    ///
    /// Returns the index of the inserted string. Always inserts a new copy.
    pub fn insert(&mut self, string: &str) -> usize {
        let idx = self.storage.len();
        self.storage.push(string.to_owned());
        idx
    }

    /// Insert a string with explicit length (`g_string_chunk_insert_len`).
    pub fn insert_len(&mut self, string: &str, len: isize) -> usize {
        let actual_len = if len < 0 {
            string.len()
        } else {
            core::cmp::min(len as usize, string.len())
        };
        let idx = self.storage.len();
        self.storage.push(string[..actual_len].to_owned());
        idx
    }

    /// Insert a string, deduplicating (`g_string_chunk_insert_const`).
    ///
    /// Returns the index of the string. If the same string was inserted
    /// before, returns the existing index.
    pub fn insert_const(&mut self, string: &str) -> usize {
        if let Some(&idx) = self.strings.get(string) {
            return idx;
        }
        let idx = self.storage.len();
        let owned = string.to_owned();
        self.strings.insert(owned, idx);
        self.storage.push(string.to_owned());
        idx
    }

    /// Get a string by index.
    pub fn get(&self, idx: usize) -> Option<&str> {
        self.storage.get(idx).map(|s| s.as_str())
    }

    /// Clear all strings (`g_string_chunk_clear`).
    pub fn clear(&mut self) {
        self.strings.clear();
        self.storage.clear();
    }

    /// Number of stored strings.
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Returns `true` if the chunk is empty.
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }
}

impl Default for StringChunk {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut chunk = StringChunk::new(1024);
        let idx = chunk.insert("hello");
        assert_eq!(chunk.get(idx), Some("hello"));
    }

    #[test]
    fn insert_const_dedup() {
        let mut chunk = StringChunk::new(1024);
        let idx1 = chunk.insert_const("world");
        let idx2 = chunk.insert_const("world");
        assert_eq!(idx1, idx2);
        assert_eq!(chunk.len(), 1); // deduped: only one entry in storage
    }

    #[test]
    fn insert_len() {
        let mut chunk = StringChunk::new(1024);
        let idx = chunk.insert_len("hello world", 5);
        assert_eq!(chunk.get(idx), Some("hello"));
    }

    #[test]
    fn insert_len_negative() {
        let mut chunk = StringChunk::new(1024);
        let idx = chunk.insert_len("full string", -1);
        assert_eq!(chunk.get(idx), Some("full string"));
    }

    #[test]
    fn clear() {
        let mut chunk = StringChunk::new(1024);
        chunk.insert("a");
        chunk.insert("b");
        assert_eq!(chunk.len(), 2);
        chunk.clear();
        assert_eq!(chunk.len(), 0);
        assert!(chunk.is_empty());
    }
}
