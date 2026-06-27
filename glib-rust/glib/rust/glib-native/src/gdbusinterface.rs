//! GDBusInterface matching `gio/gdbusinterface.h`.
//!
//! Base type for D-Bus interfaces. In this no_std port we model it as
//! a trait with a concrete implementation.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// Trait for D-Bus interface objects (`GDBusInterface`).
pub trait DBusInterface {
    /// Returns interface info (name).
    fn get_info(&self) -> String;

    /// Gets the enclosing object path, if any.
    fn get_object(&self) -> Option<String>;

    /// Sets the enclosing object path.
    fn set_object(&self, object_path: Option<String>);
}

/// A simple D-Bus interface implementation.
pub struct SimpleDBusInterface {
    name: String,
    object_path: Mutex<Option<String>>,
}

impl SimpleDBusInterface {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            object_path: Mutex::new(None),
        }
    }
}

impl DBusInterface for SimpleDBusInterface {
    fn get_info(&self) -> String {
        self.name.clone()
    }

    fn get_object(&self) -> Option<String> {
        self.object_path.lock().clone()
    }

    fn set_object(&self, object_path: Option<String>) {
        *self.object_path.lock() = object_path;
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let iface = SimpleDBusInterface::new("org.test.Iface");
        assert_eq!(iface.get_info(), "org.test.Iface");
        assert!(iface.get_object().is_none());
    }

    #[test]
    fn test_set_get_object() {
        let iface = SimpleDBusInterface::new("org.test.Iface");
        iface.set_object(Some("/org/test/object".to_string()));
        assert_eq!(iface.get_object(), Some("/org/test/object".to_string()));
    }

    #[test]
    fn test_clear_object() {
        let iface = SimpleDBusInterface::new("org.test.Iface");
        iface.set_object(Some("/path".to_string()));
        iface.set_object(None);
        assert!(iface.get_object().is_none());
    }
}
