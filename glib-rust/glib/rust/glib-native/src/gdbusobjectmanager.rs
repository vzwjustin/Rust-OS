//! GDBusObjectManager matching `gio/gdbusobjectmanager.h`.
//!
//! Base type for D-Bus object managers. In this no_std port we model
//! a registry of objects keyed by path with add/remove/lookup.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus object manager (`GDBusObjectManager`).
pub struct DBusObjectManager {
    object_path: Mutex<String>,
    objects: Mutex<BTreeMap<String, Vec<String>>>,
}

impl DBusObjectManager {
    /// Creates a new object manager with the given root path.
    pub fn new(object_path: &str) -> Self {
        Self {
            object_path: Mutex::new(object_path.to_string()),
            objects: Mutex::new(BTreeMap::new()),
        }
    }

    /// Gets the root object path.
    ///
    /// Mirrors `g_dbus_object_manager_get_object_path`.
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Gets all registered object paths.
    ///
    /// Mirrors `g_dbus_object_manager_get_objects`.
    pub fn get_objects(&self) -> Vec<String> {
        self.objects.lock().keys().cloned().collect()
    }

    /// Gets a specific object by path.
    ///
    /// Mirrors `g_dbus_object_manager_get_object`.
    pub fn get_object(&self, object_path: &str) -> Option<Vec<String>> {
        self.objects.lock().get(object_path).cloned()
    }

    /// Gets a specific interface on an object.
    ///
    /// Mirrors `g_dbus_object_manager_get_interface`.
    pub fn get_interface(&self, object_path: &str, interface_name: &str) -> bool {
        self.objects
            .lock()
            .get(object_path)
            .map(|ifaces| ifaces.iter().any(|i| i == interface_name))
            .unwrap_or(false)
    }

    /// Adds an object with its interfaces.
    pub fn add_object(&self, object_path: &str, interfaces: Vec<String>) {
        self.objects
            .lock()
            .insert(object_path.to_string(), interfaces);
    }

    /// Removes an object by path.
    pub fn remove_object(&self, object_path: &str) -> bool {
        self.objects.lock().remove(object_path).is_some()
    }

    /// Returns the number of managed objects.
    pub fn object_count(&self) -> usize {
        self.objects.lock().len()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let mgr = DBusObjectManager::new("/org/test");
        assert_eq!(mgr.get_object_path(), "/org/test");
        assert_eq!(mgr.object_count(), 0);
    }

    #[test]
    fn test_add_remove_object() {
        let mgr = DBusObjectManager::new("/org/test");
        mgr.add_object("/org/test/obj1", vec!["org.test.Iface".to_string()]);
        assert_eq!(mgr.object_count(), 1);
        assert!(mgr.get_object("/org/test/obj1").is_some());
        assert!(mgr.remove_object("/org/test/obj1"));
        assert_eq!(mgr.object_count(), 0);
    }

    #[test]
    fn test_get_interface() {
        let mgr = DBusObjectManager::new("/org/test");
        mgr.add_object(
            "/org/test/obj1",
            vec!["org.test.A".to_string(), "org.test.B".to_string()],
        );
        assert!(mgr.get_interface("/org/test/obj1", "org.test.A"));
        assert!(!mgr.get_interface("/org/test/obj1", "org.test.C"));
        assert!(!mgr.get_interface("/nonexistent", "org.test.A"));
    }

    #[test]
    fn test_get_objects() {
        let mgr = DBusObjectManager::new("/root");
        mgr.add_object("/root/a", vec![]);
        mgr.add_object("/root/b", vec![]);
        let objs = mgr.get_objects();
        assert_eq!(objs.len(), 2);
    }
}
