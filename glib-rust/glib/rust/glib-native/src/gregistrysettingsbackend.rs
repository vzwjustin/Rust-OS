//! GRegistrySettingsBackend matching `gio/gregistrysettingsbackend.h`.
//! A Windows registry-backed settings backend. In this no_std port we
//! model it as a simple key-value store (registry not available).
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// A registry settings backend (`GRegistrySettingsBackend`).
pub struct RegistrySettingsBackend {
    values: Mutex<BTreeMap<String, String>>,
}

impl RegistrySettingsBackend {
    pub fn new() -> Self {
        Self {
            values: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn write(&self, key: &str, value: &str) -> bool {
        self.values
            .lock()
            .insert(key.to_string(), value.to_string());
        true
    }

    pub fn read(&self, key: &str) -> Option<String> {
        self.values.lock().get(key).cloned()
    }

    pub fn reset(&self, key: &str) -> bool {
        self.values.lock().remove(key).is_some()
    }

    pub fn key_count(&self) -> usize {
        self.values.lock().len()
    }
}

impl Default for RegistrySettingsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read() {
        let b = RegistrySettingsBackend::new();
        b.write("HKCU/Software/Test", "value");
        assert_eq!(b.read("HKCU/Software/Test"), Some("value".to_string()));
    }

    #[test]
    fn test_reset() {
        let b = RegistrySettingsBackend::new();
        b.write("key", "val");
        assert!(b.reset("key"));
        assert!(b.read("key").is_none());
    }
}
