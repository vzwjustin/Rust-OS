//! GDBusObjectSkeleton matching `gio/gdbusobjectskeleton.h`.
//!
//! A server-side D-Bus object skeleton. In this no_std port we model
//! the object path and interface set with add/remove operations.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus object skeleton (`GDBusObjectSkeleton`).
pub struct DBusObjectSkeleton {
    object_path: Mutex<String>,
    interfaces: Mutex<Vec<String>>,
}

impl DBusObjectSkeleton {
    /// Creates a new object skeleton with the given path.
    ///
    /// Mirrors `g_dbus_object_skeleton_new`.
    pub fn new(object_path: &str) -> Self {
        Self {
            object_path: Mutex::new(object_path.to_string()),
            interfaces: Mutex::new(Vec::new()),
        }
    }

    /// Gets the object path.
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Sets the object path.
    ///
    /// Mirrors `g_dbus_object_skeleton_set_object_path`.
    pub fn set_object_path(&self, object_path: &str) {
        *self.object_path.lock() = object_path.to_string();
    }

    /// Adds an interface by name.
    ///
    /// Mirrors `g_dbus_object_skeleton_add_interface`.
    pub fn add_interface(&self, interface_name: &str) {
        self.interfaces.lock().push(interface_name.to_string());
    }

    /// Removes an interface by name.
    ///
    /// Mirrors `g_dbus_object_skeleton_remove_interface_by_name`.
    pub fn remove_interface(&self, interface_name: &str) {
        self.interfaces.lock().retain(|i| i != interface_name);
    }

    /// Returns all interface names.
    pub fn get_interfaces(&self) -> Vec<String> {
        self.interfaces.lock().clone()
    }

    /// Flushes pending changes (no-op in this stub).
    ///
    /// Mirrors `g_dbus_object_skeleton_flush`.
    pub fn flush(&self) {}

    /// Returns the number of interfaces.
    pub fn interface_count(&self) -> usize {
        self.interfaces.lock().len()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let obj = DBusObjectSkeleton::new("/org/test/object");
        assert_eq!(obj.get_object_path(), "/org/test/object");
        assert_eq!(obj.interface_count(), 0);
    }

    #[test]
    fn test_set_object_path() {
        let obj = DBusObjectSkeleton::new("/old");
        obj.set_object_path("/new");
        assert_eq!(obj.get_object_path(), "/new");
    }

    #[test]
    fn test_add_remove_interface() {
        let obj = DBusObjectSkeleton::new("/test");
        obj.add_interface("org.test.A");
        obj.add_interface("org.test.B");
        assert_eq!(obj.interface_count(), 2);
        obj.remove_interface("org.test.A");
        assert_eq!(obj.interface_count(), 1);
        assert_eq!(obj.get_interfaces()[0], "org.test.B");
    }

    #[test]
    fn test_flush() {
        let obj = DBusObjectSkeleton::new("/test");
        obj.flush();
    }
}
