//! `gsettingsbackendinternal` matching `gio/gsettingsbackendinternal.h`.
//!
//! Internal settings backend API: listener vtable, watch/unwatch,
//! read/write/reset operations, and backend type registration.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::variant::Variant;
use crate::varianttype::VariantType;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Listener callback table (mirrors `GSettingsListenerVTable`).
///
/// In Rust, we use function pointers instead of C-style vtable with `gpointer`.
pub struct SettingsListenerVTable {
    pub changed: fn(key: &str, origin_tag: u64),
    pub path_changed: fn(path: &str, origin_tag: u64),
    pub keys_changed: fn(prefix: &str, origin_tag: u64, names: &[&str]),
    pub writable_changed: fn(key: &str),
    pub path_writable_changed: fn(path: &str),
}

/// A registered listener watch.
#[derive(Debug)]
struct Watch {
    target_id: u64,
    context: Option<String>,
}

/// Internal settings backend state.
#[derive(Debug)]
pub struct SettingsBackendInternal {
    /// Key-value store (key → Variant).
    values: Mutex<BTreeMap<String, Variant>>,
    /// Writable keys set.
    writable: Mutex<BTreeMap<String, bool>>,
    /// Registered watches.
    watches: Mutex<Vec<Watch>>,
    /// Subscribed paths.
    subscriptions: Mutex<Vec<String>>,
}

impl SettingsBackendInternal {
    /// Creates a new empty settings backend.
    pub fn new() -> Self {
        Self {
            values: Mutex::new(BTreeMap::new()),
            writable: Mutex::new(BTreeMap::new()),
            watches: Mutex::new(Vec::new()),
            subscriptions: Mutex::new(Vec::new()),
        }
    }

    /// Registers a watch on the backend (mirrors `g_settings_backend_watch`).
    pub fn watch(&self, target_id: u64, context: Option<&str>) {
        self.watches.lock().push(Watch {
            target_id,
            context: context.map(|s| s.to_string()),
        });
    }

    /// Removes a watch (mirrors `g_settings_backend_unwatch`).
    pub fn unwatch(&self, target_id: u64) {
        self.watches.lock().retain(|w| w.target_id != target_id);
    }

    /// Reads a value (mirrors `g_settings_backend_read`).
    pub fn read(
        &self,
        key: &str,
        _expected_type: &VariantType,
        default_value: bool,
    ) -> Option<Variant> {
        if default_value {
            return None;
        }
        self.values.lock().get(key).cloned()
    }

    /// Reads a user value (mirrors `g_settings_backend_read_user_value`).
    pub fn read_user_value(&self, key: &str, _expected_type: &VariantType) -> Option<Variant> {
        self.values.lock().get(key).cloned()
    }

    /// Writes a value (mirrors `g_settings_backend_write`).
    pub fn write(&self, key: &str, value: Variant, _origin_tag: u64) -> bool {
        self.values.lock().insert(key.to_string(), value);
        true
    }

    /// Writes multiple keys from a tree (mirrors `g_settings_backend_write_tree`).
    pub fn write_tree(&self, tree: &BTreeMap<String, Variant>, _origin_tag: u64) -> bool {
        let mut values = self.values.lock();
        for (key, value) in tree {
            values.insert(key.clone(), value.clone());
        }
        true
    }

    /// Resets a key (mirrors `g_settings_backend_reset`).
    pub fn reset(&self, key: &str, _origin_tag: u64) {
        self.values.lock().remove(key);
    }

    /// Checks if a key is writable (mirrors `g_settings_backend_get_writable`).
    pub fn get_writable(&self, key: &str) -> bool {
        self.writable.lock().get(key).copied().unwrap_or(true)
    }

    /// Subscribes to a path (mirrors `g_settings_backend_subscribe`).
    pub fn subscribe(&self, name: &str) {
        self.subscriptions.lock().push(name.to_string());
    }

    /// Unsubscribes from a path (mirrors `g_settings_backend_unsubscribe`).
    pub fn unsubscribe(&self, name: &str) {
        self.subscriptions.lock().retain(|s| s != name);
    }

    /// Syncs the backend (mirrors `g_settings_backend_sync_default`).
    pub fn sync(&self) {
        // No-op for in-memory backend
    }

    /// Creates an empty tree (mirrors `g_settings_backend_create_tree`).
    pub fn create_tree() -> BTreeMap<String, Variant> {
        BTreeMap::new()
    }
}

impl Default for SettingsBackendInternal {
    fn default() -> Self {
        Self::new()
    }
}

/// Backend type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsBackendType {
    Null,
    Memory,
    Keyfile,
    Nextstep,
    Registry,
}

/// Returns the type name for a backend type.
pub fn backend_type_name(ty: SettingsBackendType) -> &'static str {
    match ty {
        SettingsBackendType::Null => "GNullSettingsBackend",
        SettingsBackendType::Memory => "GMemorySettingsBackend",
        SettingsBackendType::Keyfile => "GKeyfileSettingsBackend",
        SettingsBackendType::Nextstep => "GNextstepSettingsBackend",
        SettingsBackendType::Registry => "GRegistrySettingsBackend",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read() {
        let backend = SettingsBackendInternal::new();
        let key = "test.key";
        let value = Variant::new_string("hello");
        assert!(backend.write(key, value.clone(), 1));
        let read = backend.read(key, &VariantType::any(), false);
        assert!(read.is_some());
        assert_eq!(read.unwrap().get_string(), "hello");
    }

    #[test]
    fn test_reset() {
        let backend = SettingsBackendInternal::new();
        backend.write("key1", Variant::new_int32(42), 1);
        assert!(backend.read("key1", &VariantType::any(), false).is_some());
        backend.reset("key1", 1);
        assert!(backend.read("key1", &VariantType::any(), false).is_none());
    }

    #[test]
    fn test_writable() {
        let backend = SettingsBackendInternal::new();
        assert!(backend.get_writable("any.key"));
    }

    #[test]
    fn test_subscribe_unsubscribe() {
        let backend = SettingsBackendInternal::new();
        backend.subscribe("/test/");
        assert_eq!(backend.subscriptions.lock().len(), 1);
        backend.unsubscribe("/test/");
        assert_eq!(backend.subscriptions.lock().len(), 0);
    }

    #[test]
    fn test_watch_unwatch() {
        let backend = SettingsBackendInternal::new();
        backend.watch(1, None);
        backend.watch(2, Some("ctx"));
        assert_eq!(backend.watches.lock().len(), 2);
        backend.unwatch(1);
        assert_eq!(backend.watches.lock().len(), 1);
    }

    #[test]
    fn test_write_tree() {
        let backend = SettingsBackendInternal::new();
        let mut tree = SettingsBackendInternal::create_tree();
        tree.insert("a".to_string(), Variant::new_int32(1));
        tree.insert("b".to_string(), Variant::new_int32(2));
        assert!(backend.write_tree(&tree, 1));
        assert!(backend.read("a", &VariantType::any(), false).is_some());
        assert!(backend.read("b", &VariantType::any(), false).is_some());
    }

    #[test]
    fn test_backend_type_names() {
        assert_eq!(
            backend_type_name(SettingsBackendType::Null),
            "GNullSettingsBackend"
        );
        assert_eq!(
            backend_type_name(SettingsBackendType::Memory),
            "GMemorySettingsBackend"
        );
        assert_eq!(
            backend_type_name(SettingsBackendType::Keyfile),
            "GKeyfileSettingsBackend"
        );
    }
}
