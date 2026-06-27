//! GNullSettingsBackend matching `gio/gnullsettingsbackend.h` /
//! `gio/gnullsettingsbackend.c`.
//!
//! A read-only settings backend. All reads return `None`, all writes
//! return `false`, and `get_writable` always returns `false`.
//! This matches the upstream behavior where settings always have
//! their default values.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gsettingsbackend::SettingsBackend;
use crate::gsimplepermission::SimplePermission;
use alloc::string::String;

/// A read-only settings backend (`GNullSettingsBackend`).
///
/// All reads return `None`, all writes fail, nothing is writable.
/// Settings using this backend will always have their default values.
pub struct NullSettingsBackend {
    inner: SettingsBackend,
}

impl NullSettingsBackend {
    /// Creates a new read-only settings backend.
    ///
    /// Mirrors `g_null_settings_backend_new`.
    pub fn new() -> Self {
        let backend = Self {
            inner: SettingsBackend::new(),
        };
        // Mark all keys as non-writable by locking the writable map.
        // In practice, get_writable always returns false regardless.
        backend
    }

    /// Reads a key — always returns `None`.
    ///
    /// Mirrors `g_null_settings_backend_read`.
    pub fn read(&self, _key: &str) -> Option<String> {
        None
    }

    /// Writes a key — always returns `false` (not writable).
    ///
    /// Mirrors `g_null_settings_backend_write`.
    pub fn write(&self, _key: &str, _value: &str) -> bool {
        false
    }

    /// Resets a key — no-op.
    ///
    /// Mirrors `g_null_settings_backend_reset`.
    pub fn reset(&self, _key: &str) {}

    /// Returns whether a key is writable — always `false`.
    ///
    /// Mirrors `g_null_settings_backend_get_writable`.
    pub fn get_writable(&self, _name: &str) -> bool {
        false
    }

    /// Returns a permission that is never allowed.
    ///
    /// Mirrors `g_null_settings_backend_get_permission`.
    pub fn get_permission(&self) -> SimplePermission {
        SimplePermission::new(false)
    }

    /// Returns the inner backend (for subscribe/unsubscribe delegation).
    pub fn inner(&self) -> &SettingsBackend {
        &self.inner
    }
}

impl Default for NullSettingsBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_always_none() {
        let b = NullSettingsBackend::new();
        assert!(b.read("any-key").is_none());
    }

    #[test]
    fn test_write_always_false() {
        let b = NullSettingsBackend::new();
        assert!(!b.write("key", "value"));
    }

    #[test]
    fn test_get_writable_always_false() {
        let b = NullSettingsBackend::new();
        assert!(!b.get_writable("key"));
    }

    #[test]
    fn test_permission_not_allowed() {
        let b = NullSettingsBackend::new();
        let perm = b.get_permission();
        assert!(!perm.get_allowed());
    }

    #[test]
    fn test_reset_noop() {
        let b = NullSettingsBackend::new();
        b.reset("key");
        assert!(b.read("key").is_none());
    }
}
