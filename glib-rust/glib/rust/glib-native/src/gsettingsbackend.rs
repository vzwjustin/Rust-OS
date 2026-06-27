//! GSettingsBackend matching `gio/gsettingsbackend.h`.
//!
//! Abstract settings storage backend. In this no_std port we implement
//! an in-memory key-value store with subscribe/unsubscribe notifications.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A settings storage backend (`GSettingsBackend`).
pub struct SettingsBackend {
    values: Mutex<BTreeMap<String, String>>,
    writable: Mutex<BTreeMap<String, bool>>,
    subscribers: Mutex<Vec<String>>,
}

impl SettingsBackend {
    /// Creates a new empty settings backend.
    pub fn new() -> Self {
        Self {
            values: Mutex::new(BTreeMap::new()),
            writable: Mutex::new(BTreeMap::new()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Reads a key's value.
    ///
    /// Mirrors `g_settings_backend_read`.
    pub fn read(&self, key: &str) -> Option<String> {
        self.values.lock().get(key).cloned()
    }

    /// Writes a key's value.
    ///
    /// Mirrors `g_settings_backend_write`.
    pub fn write(&self, key: &str, value: &str) -> bool {
        let mut writable = self.writable.lock();
        let is_writable = writable.get(key).copied().unwrap_or(true);
        if !is_writable {
            return false;
        }
        drop(writable);
        self.values
            .lock()
            .insert(key.to_string(), value.to_string());
        true
    }

    /// Resets a key (removes it from storage).
    ///
    /// Mirrors `g_settings_backend_reset`.
    pub fn reset(&self, key: &str) {
        self.values.lock().remove(key);
        self.writable.lock().remove(key);
    }

    /// Gets whether a key is writable.
    ///
    /// Mirrors `g_settings_backend_get_writable`.
    pub fn get_writable(&self, key: &str) -> bool {
        self.writable.lock().get(key).copied().unwrap_or(true)
    }

    /// Sets whether a key is writable.
    pub fn set_writable(&self, key: &str, writable: bool) {
        self.writable.lock().insert(key.to_string(), writable);
    }

    /// Subscribes to changes for a name prefix.
    ///
    /// Mirrors `g_settings_backend_subscribe`.
    pub fn subscribe(&self, name: &str) {
        self.subscribers.lock().push(name.to_string());
    }

    /// Unsubscribes from changes for a name prefix.
    ///
    /// Mirrors `g_settings_backend_unsubscribe`.
    pub fn unsubscribe(&self, name: &str) {
        self.subscribers.lock().retain(|s| s != name);
    }

    /// Returns the list of subscribers.
    pub fn get_subscribers(&self) -> Vec<String> {
        self.subscribers.lock().clone()
    }

    /// Syncs state (no-op in memory backend).
    ///
    /// Mirrors `g_settings_backend_sync`.
    pub fn sync(&self) {}

    /// Reads the user value for a key.
    ///
    /// Mirrors `g_settings_backend_read_user_value`.
    pub fn read_user_value(&self, key: &str) -> Option<String> {
        self.read(key)
    }

    /// Returns the number of stored keys.
    pub fn n_keys(&self) -> usize {
        self.values.lock().len()
    }
}

impl Default for SettingsBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let backend = SettingsBackend::new();
        assert_eq!(backend.n_keys(), 0);
    }

    #[test]
    fn test_write_and_read() {
        let backend = SettingsBackend::new();
        assert!(backend.write("key1", "value1"));
        assert_eq!(backend.read("key1").unwrap(), "value1");
    }

    #[test]
    fn test_read_missing() {
        let backend = SettingsBackend::new();
        assert!(backend.read("missing").is_none());
    }

    #[test]
    fn test_reset() {
        let backend = SettingsBackend::new();
        backend.write("key1", "value1");
        backend.reset("key1");
        assert!(backend.read("key1").is_none());
    }

    #[test]
    fn test_writable() {
        let backend = SettingsBackend::new();
        assert!(backend.get_writable("key1"));
        backend.set_writable("key1", false);
        assert!(!backend.get_writable("key1"));
        assert!(!backend.write("key1", "value"));
    }

    #[test]
    fn test_subscribe_unsubscribe() {
        let backend = SettingsBackend::new();
        backend.subscribe("/app/settings");
        backend.subscribe("/app/other");
        assert_eq!(backend.get_subscribers().len(), 2);
        backend.unsubscribe("/app/settings");
        assert_eq!(backend.get_subscribers().len(), 1);
        assert_eq!(backend.get_subscribers()[0], "/app/other");
    }

    #[test]
    fn test_sync_noop() {
        let backend = SettingsBackend::new();
        backend.sync();
    }

    #[test]
    fn test_read_user_value() {
        let backend = SettingsBackend::new();
        backend.write("key1", "val");
        assert_eq!(backend.read_user_value("key1").unwrap(), "val");
    }

    #[test]
    fn test_n_keys() {
        let backend = SettingsBackend::new();
        backend.write("a", "1");
        backend.write("b", "2");
        assert_eq!(backend.n_keys(), 2);
    }
}
