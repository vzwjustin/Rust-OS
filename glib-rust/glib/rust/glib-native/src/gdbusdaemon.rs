//! GDBusDaemon matching `gio/gdbusdaemon.h`.
//! An in-process D-Bus daemon. In this no_std port we model it with
//! a list of registered names and running state.
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// An in-process D-Bus daemon (`GDBusDaemon`).
pub struct DBusDaemon {
    names: Mutex<BTreeMap<String, String>>,
    running: Mutex<bool>,
    address: Mutex<String>,
}

impl DBusDaemon {
    pub fn new() -> Self {
        Self {
            names: Mutex::new(BTreeMap::new()),
            running: Mutex::new(false),
            address: Mutex::new(String::new()),
        }
    }

    pub fn start(&self, address: &str) {
        *self.running.lock() = true;
        *self.address.lock() = address.to_string();
    }

    pub fn stop(&self) {
        *self.running.lock() = false;
        self.names.lock().clear();
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock()
    }
    pub fn get_address(&self) -> String {
        self.address.lock().clone()
    }

    pub fn register_name(&self, name: &str, owner: &str) -> bool {
        if !*self.running.lock() {
            return false;
        }
        self.names
            .lock()
            .insert(name.to_string(), owner.to_string());
        true
    }

    pub fn unregister_name(&self, name: &str) -> bool {
        self.names.lock().remove(name).is_some()
    }

    pub fn lookup_name(&self, name: &str) -> Option<String> {
        self.names.lock().get(name).cloned()
    }

    pub fn name_count(&self) -> usize {
        self.names.lock().len()
    }
}

impl Default for DBusDaemon {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_register() {
        let d = DBusDaemon::new();
        d.start("unix:abstract=test");
        assert!(d.is_running());
        d.register_name("org.test", ":1.1");
        assert_eq!(d.lookup_name("org.test"), Some(":1.1".to_string()));
    }

    #[test]
    fn test_not_running() {
        let d = DBusDaemon::new();
        assert!(!d.register_name("org.test", ":1.1"));
    }
}
