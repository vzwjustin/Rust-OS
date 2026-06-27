//! GDBusObjectProxy matching `gio/gdbusobjectproxy.h`.
//! A client-side D-Bus object proxy. In this no_std port we model it
//! with bus name, object path, and interface set.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus object proxy (`GDBusObjectProxy`).
pub struct DBusObjectProxy {
    bus_name: Mutex<String>,
    object_path: Mutex<String>,
    interfaces: Mutex<Vec<String>>,
}

impl DBusObjectProxy {
    pub fn new(bus_name: &str, object_path: &str) -> Self {
        Self {
            bus_name: Mutex::new(bus_name.to_string()),
            object_path: Mutex::new(object_path.to_string()),
            interfaces: Mutex::new(Vec::new()),
        }
    }

    pub fn get_bus_name(&self) -> String {
        self.bus_name.lock().clone()
    }

    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    pub fn add_interface(&self, name: &str) {
        self.interfaces.lock().push(name.to_string());
    }

    pub fn get_interfaces(&self) -> Vec<String> {
        self.interfaces.lock().clone()
    }

    pub fn interface_count(&self) -> usize {
        self.interfaces.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let p = DBusObjectProxy::new("org.test.Bus", "/org/test/obj");
        assert_eq!(p.get_bus_name(), "org.test.Bus");
        assert_eq!(p.get_object_path(), "/org/test/obj");
        assert_eq!(p.interface_count(), 0);
    }

    #[test]
    fn test_add_interface() {
        let p = DBusObjectProxy::new("org.test", "/obj");
        p.add_interface("org.test.A");
        p.add_interface("org.test.B");
        assert_eq!(p.interface_count(), 2);
    }
}
