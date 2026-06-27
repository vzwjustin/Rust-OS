//! gwin32registrykey matching `gio/gwin32registrykey.c`.
//!
//! Windows registry key abstraction. Provides read/write access to
//! registry keys, value enumeration, and change notification.
//!
//! In this no_std port, we model the registry as an in-memory tree.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Registry value types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryValueType {
    None,
    Sz(String),
    ExpandSz(String),
    Binary(Vec<u8>),
    Dword(u32),
    DwordBigEndian(u32),
    MultiSz(Vec<String>),
    Qword(u64),
}

/// A registry key.
pub struct RegistryKey {
    path: String,
    values: Mutex<BTreeMap<String, RegistryValueType>>,
    subkeys: Mutex<BTreeMap<String, RegistryKey>>,
}

impl RegistryKey {
    /// Creates a new registry key at the given path.
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            values: Mutex::new(BTreeMap::new()),
            subkeys: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the path of this key.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Gets a value from this key.
    pub fn get_value(&self, name: &str) -> Option<RegistryValueType> {
        self.values.lock().get(name).cloned()
    }

    /// Sets a value in this key.
    pub fn set_value(&self, name: &str, value: RegistryValueType) {
        self.values.lock().insert(name.to_string(), value);
    }

    /// Enumerates the values in this key.
    pub fn enum_values(&self) -> Vec<(String, RegistryValueType)> {
        self.values
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Gets a subkey.
    pub fn get_subkey(&self, name: &str) -> Option<RegistryKey> {
        self.subkeys.lock().get(name).map(|k| RegistryKey {
            path: k.path.clone(),
            values: Mutex::new(k.values.lock().clone()),
            subkeys: Mutex::new(BTreeMap::new()),
        })
    }

    /// Creates or opens a subkey.
    pub fn create_subkey(&self, name: &str) -> RegistryKey {
        let path = format!("{}\\{}", self.path, name);
        let key = RegistryKey::new(&path);
        self.subkeys.lock().insert(
            name.to_string(),
            RegistryKey {
                path: path.clone(),
                values: Mutex::new(BTreeMap::new()),
                subkeys: Mutex::new(BTreeMap::new()),
            },
        );
        key
    }

    /// Enumerates subkey names.
    pub fn enum_subkeys(&self) -> Vec<String> {
        self.subkeys.lock().keys().cloned().collect()
    }

    /// Deletes a value.
    pub fn delete_value(&self, name: &str) -> bool {
        self.values.lock().remove(name).is_some()
    }

    /// Deletes a subkey.
    pub fn delete_subkey(&self, name: &str) -> bool {
        self.subkeys.lock().remove(name).is_some()
    }

    /// Checks if a value exists.
    pub fn has_value(&self, name: &str) -> bool {
        self.values.lock().contains_key(name)
    }

    /// Checks if a subkey exists.
    pub fn has_subkey(&self, name: &str) -> bool {
        self.subkeys.lock().contains_key(name)
    }
}

/// Root registry keys (HKEY_*).
pub struct Registry {
    classes_root: RegistryKey,
    current_user: RegistryKey,
    local_machine: RegistryKey,
}

impl Registry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            classes_root: RegistryKey::new("HKEY_CLASSES_ROOT"),
            current_user: RegistryKey::new("HKEY_CURRENT_USER"),
            local_machine: RegistryKey::new("HKEY_LOCAL_MACHINE"),
        }
    }

    /// Returns HKEY_CLASSES_ROOT.
    pub fn classes_root(&self) -> &RegistryKey {
        &self.classes_root
    }

    /// Returns HKEY_CURRENT_USER.
    pub fn current_user(&self) -> &RegistryKey {
        &self.current_user
    }

    /// Returns HKEY_LOCAL_MACHINE.
    pub fn local_machine(&self) -> &RegistryKey {
        &self.local_machine
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_values() {
        let key = RegistryKey::new("HKEY_CLASSES_ROOT\\txtfile");
        key.set_value("", RegistryValueType::Sz("Text Document".to_string()));
        key.set_value("EditFlags", RegistryValueType::Dword(0x00010000));

        assert!(key.has_value(""));
        assert!(key.has_value("EditFlags"));
        assert!(!key.has_value("NonExistent"));

        match key.get_value("") {
            Some(RegistryValueType::Sz(s)) => assert_eq!(s, "Text Document"),
            _ => panic!("expected string value"),
        }
        match key.get_value("EditFlags") {
            Some(RegistryValueType::Dword(v)) => assert_eq!(v, 0x00010000),
            _ => panic!("expected dword value"),
        }
    }

    #[test]
    fn test_key_subkeys() {
        let key = RegistryKey::new("HKEY_CLASSES_ROOT");
        let sub = key.create_subkey(".txt");
        sub.set_value("", RegistryValueType::Sz("txtfile".to_string()));

        assert!(key.has_subkey(".txt"));
        let sub2 = key.get_subkey(".txt").unwrap();
        assert_eq!(sub2.path(), "HKEY_CLASSES_ROOT\\.txt");
    }

    #[test]
    fn test_delete() {
        let key = RegistryKey::new("test");
        key.set_value("foo", RegistryValueType::Dword(42));
        assert!(key.delete_value("foo"));
        assert!(!key.has_value("foo"));
        assert!(!key.delete_value("foo"));
    }

    #[test]
    fn test_enum() {
        let key = RegistryKey::new("test");
        key.set_value("a", RegistryValueType::Dword(1));
        key.set_value("b", RegistryValueType::Dword(2));
        let values = key.enum_values();
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_registry_roots() {
        let reg = Registry::new();
        assert_eq!(reg.classes_root().path(), "HKEY_CLASSES_ROOT");
        assert_eq!(reg.current_user().path(), "HKEY_CURRENT_USER");
        assert_eq!(reg.local_machine().path(), "HKEY_LOCAL_MACHINE");
    }
}
