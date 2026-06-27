//! GDBusObject matching `gio/gdbusobject.h`.
//!
//! Base type for D-Bus objects. In this no_std port we model it as
//! a trait with a concrete implementation.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Trait for D-Bus objects (`GDBusObject`).
pub trait DBusObject {
    /// Returns the object path.
    fn get_object_path(&self) -> String;

    /// Returns all interface names on this object.
    fn get_interfaces(&self) -> Vec<String>;

    /// Returns a specific interface by name.
    fn get_interface(&self, interface_name: &str) -> Option<String>;
}

/// A simple D-Bus object implementation.
pub struct SimpleDBusObject {
    object_path: String,
    interfaces: Mutex<Vec<String>>,
}

impl SimpleDBusObject {
    pub fn new(object_path: &str) -> Self {
        Self {
            object_path: object_path.to_string(),
            interfaces: Mutex::new(Vec::new()),
        }
    }

    pub fn add_interface(&self, interface_name: &str) {
        self.interfaces.lock().push(interface_name.to_string());
    }

    pub fn remove_interface(&self, interface_name: &str) {
        self.interfaces.lock().retain(|i| i != interface_name);
    }
}

impl DBusObject for SimpleDBusObject {
    fn get_object_path(&self) -> String {
        self.object_path.clone()
    }

    fn get_interfaces(&self) -> Vec<String> {
        self.interfaces.lock().clone()
    }

    fn get_interface(&self, interface_name: &str) -> Option<String> {
        self.interfaces
            .lock()
            .iter()
            .find(|i| i.as_str() == interface_name)
            .cloned()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let obj = SimpleDBusObject::new("/org/test/object");
        assert_eq!(obj.get_object_path(), "/org/test/object");
        assert!(obj.get_interfaces().is_empty());
    }

    #[test]
    fn test_add_remove_interface() {
        let obj = SimpleDBusObject::new("/test");
        obj.add_interface("org.test.A");
        obj.add_interface("org.test.B");
        assert_eq!(obj.get_interfaces().len(), 2);
        obj.remove_interface("org.test.A");
        assert_eq!(obj.get_interfaces().len(), 1);
    }

    #[test]
    fn test_get_interface() {
        let obj = SimpleDBusObject::new("/test");
        obj.add_interface("org.test.A");
        assert!(obj.get_interface("org.test.A").is_some());
        assert!(obj.get_interface("org.test.Missing").is_none());
    }
}
