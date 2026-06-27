//! GDelayedSettingsBackend matching `gio/gdelayedsettingsbackend.h`.
//!
//! Wraps [`SettingsBackend`] and buffers writes in a pending map until
//! [`DelayedSettingsBackend::flush`] applies them to the inner backend.
//! Reads always delegate to the inner backend immediately.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gsettingsbackend::SettingsBackend;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A settings backend that delays writes until flushed (`GDelayedSettingsBackend`).
pub struct DelayedSettingsBackend {
    inner: SettingsBackend,
    pending: Mutex<BTreeMap<String, String>>,
}

impl DelayedSettingsBackend {
    /// Creates a delayed backend wrapping `inner`.
    pub fn new(inner: SettingsBackend) -> Self {
        Self {
            inner,
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    /// Creates a delayed backend with a fresh in-memory inner backend.
    pub fn new_default() -> Self {
        Self::new(SettingsBackend::new())
    }

    /// Reads a key from the inner backend immediately.
    ///
    /// Mirrors `g_settings_backend_read` (delayed reads do not consult pending).
    pub fn read(&self, key: &str) -> Option<String> {
        self.inner.read(key)
    }

    /// Buffers a write in the pending map.
    ///
    /// Mirrors `g_settings_backend_write` on the delayed layer.
    pub fn write(&self, key: &str, value: &str) -> bool {
        self.pending
            .lock()
            .insert(key.to_string(), value.to_string());
        true
    }

    /// Applies all pending writes to the inner backend.
    ///
    /// Mirrors `g_delayed_settings_backend_revert` / flush semantics.
    pub fn flush(&self) {
        let entries: Vec<(String, String)> = self
            .pending
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, value) in entries {
            self.pending.lock().remove(&key);
            self.inner.write(&key, &value);
        }
    }

    /// Clears a pending entry and resets the key on the inner backend.
    ///
    /// Mirrors `g_settings_backend_reset`.
    pub fn reset(&self, key: &str) {
        self.pending.lock().remove(key);
        self.inner.reset(key);
    }

    /// Subscribes to changes on the inner backend.
    pub fn subscribe(&self, name: &str) {
        self.inner.subscribe(name);
    }

    /// Unsubscribes from changes on the inner backend.
    pub fn unsubscribe(&self, name: &str) {
        self.inner.unsubscribe(name);
    }

    /// Returns whether a key is writable on the inner backend.
    pub fn get_writable(&self, key: &str) -> bool {
        self.inner.get_writable(key)
    }

    /// Sets writability on the inner backend.
    pub fn set_writable(&self, key: &str, writable: bool) {
        self.inner.set_writable(key, writable);
    }

    /// Returns the number of buffered pending writes.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }

    /// Returns a snapshot of pending keys (tests / diagnostics).
    pub fn pending_keys(&self) -> Vec<String> {
        self.pending.lock().keys().cloned().collect()
    }

    /// Borrows the inner backend for inspection.
    pub fn inner(&self) -> &SettingsBackend {
        &self.inner
    }
}

impl Default for DelayedSettingsBackend {
    fn default() -> Self {
        Self::new_default()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_buffered_until_flush() {
        let backend = DelayedSettingsBackend::new_default();
        assert!(backend.write("font-name", "Sans 12"));
        assert_eq!(backend.pending_count(), 1);
        assert!(backend.read("font-name").is_none());

        backend.flush();
        assert_eq!(backend.pending_count(), 0);
        assert_eq!(backend.read("font-name").unwrap(), "Sans 12");
    }

    #[test]
    fn test_read_delegates_immediately() {
        let inner = SettingsBackend::new();
        inner.write("theme", "dark");
        let backend = DelayedSettingsBackend::new(inner);
        assert_eq!(backend.read("theme").unwrap(), "dark");
    }

    #[test]
    fn test_reset_clears_pending_and_inner() {
        let backend = DelayedSettingsBackend::new_default();
        backend.write("key", "pending");
        backend.flush();
        backend.write("key", "new-pending");
        backend.reset("key");
        assert_eq!(backend.pending_count(), 0);
        assert!(backend.read("key").is_none());
    }

    #[test]
    fn test_subscribe_delegates() {
        let backend = DelayedSettingsBackend::new_default();
        backend.subscribe("/app/");
        assert_eq!(backend.inner().get_subscribers().len(), 1);
        backend.unsubscribe("/app/");
        assert!(backend.inner().get_subscribers().is_empty());
    }

    #[test]
    fn test_multiple_pending_flush() {
        let backend = DelayedSettingsBackend::new_default();
        backend.write("a", "1");
        backend.write("b", "2");
        assert_eq!(backend.pending_keys().len(), 2);
        backend.flush();
        assert_eq!(backend.read("a").unwrap(), "1");
        assert_eq!(backend.read("b").unwrap(), "2");
    }
}
