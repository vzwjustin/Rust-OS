//! GMemorySettingsBackend matching `gio/gmemorysettingsbackend.h` /
//! `gio/gmemorysettingsbackend.c`.
//!
//! An in-memory settings backend. Reads and writes work against a
//! `BTreeMap` in memory. Changes are not persisted to any backing
//! storage, so the next run starts with default values again.
//! All keys are writable.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gsimplepermission::SimplePermission;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A memory-backed settings backend (`GMemorySettingsBackend`).
///
/// Allows changes to settings but does not write them to any backing
/// storage. The next time the application runs, the memory backend
/// will start with default values again.
pub struct MemorySettingsBackend {
    table: Mutex<BTreeMap<String, String>>,
}

impl MemorySettingsBackend {
    /// Creates a new memory-backed settings backend.
    ///
    /// Mirrors `g_memory_settings_backend_new`.
    pub fn new() -> Self {
        Self {
            table: Mutex::new(BTreeMap::new()),
        }
    }

    /// Reads a key's value from memory.
    ///
    /// Mirrors `g_memory_settings_backend_read`.
    pub fn read(&self, key: &str) -> Option<String> {
        self.table.lock().get(key).cloned()
    }

    /// Writes a key's value to memory. Returns `true` on success.
    ///
    /// Mirrors `g_memory_settings_backend_write`. If the value differs
    /// from the existing value (or the key is new), it is inserted.
    pub fn write(&self, key: &str, value: &str) -> bool {
        let mut table = self.table.lock();
        let old = table.get(key);
        if old.map_or(true, |v| v != value) {
            table.insert(key.to_string(), value.to_string());
        }
        true
    }

    /// Writes multiple key/value pairs at once.
    ///
    /// Mirrors `g_memory_settings_backend_write_tree`.
    pub fn write_tree(&self, entries: &[(String, String)]) -> bool {
        let mut table = self.table.lock();
        for (key, value) in entries {
            table.insert(key.clone(), value.clone());
        }
        true
    }

    /// Resets a key (removes it from memory).
    ///
    /// Mirrors `g_memory_settings_backend_reset`.
    pub fn reset(&self, key: &str) {
        self.table.lock().remove(key);
    }

    /// Returns whether a key is writable — always `true`.
    ///
    /// Mirrors `g_memory_settings_backend_get_writable`.
    pub fn get_writable(&self, _name: &str) -> bool {
        true
    }

    /// Returns a permission that is always allowed.
    ///
    /// Mirrors `g_memory_settings_backend_get_permission`.
    pub fn get_permission(&self) -> SimplePermission {
        SimplePermission::new(true)
    }

    /// Returns the number of stored keys.
    pub fn n_keys(&self) -> usize {
        self.table.lock().len()
    }

    /// Returns all stored keys.
    pub fn keys(&self) -> Vec<String> {
        self.table.lock().keys().cloned().collect()
    }
}

impl Default for MemorySettingsBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let b = MemorySettingsBackend::new();
        assert_eq!(b.n_keys(), 0);
    }

    #[test]
    fn test_write_and_read() {
        let b = MemorySettingsBackend::new();
        assert!(b.write("key1", "value1"));
        assert_eq!(b.read("key1").unwrap(), "value1");
    }

    #[test]
    fn test_read_missing() {
        let b = MemorySettingsBackend::new();
        assert!(b.read("missing").is_none());
    }

    #[test]
    fn test_reset() {
        let b = MemorySettingsBackend::new();
        b.write("key1", "value1");
        b.reset("key1");
        assert!(b.read("key1").is_none());
    }

    #[test]
    fn test_get_writable_always_true() {
        let b = MemorySettingsBackend::new();
        assert!(b.get_writable("any-key"));
    }

    #[test]
    fn test_permission_allowed() {
        let b = MemorySettingsBackend::new();
        let perm = b.get_permission();
        assert!(perm.get_allowed());
    }

    #[test]
    fn test_write_tree() {
        let b = MemorySettingsBackend::new();
        let entries = vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ];
        assert!(b.write_tree(&entries));
        assert_eq!(b.n_keys(), 2);
        assert_eq!(b.read("a").unwrap(), "1");
        assert_eq!(b.read("b").unwrap(), "2");
    }

    #[test]
    fn test_write_same_value() {
        let b = MemorySettingsBackend::new();
        b.write("key", "val");
        b.write("key", "val");
        assert_eq!(b.n_keys(), 1);
    }
}
