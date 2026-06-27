//! GTestDBus matching `gio/gtestdbus.h`.
//! A D-Bus daemon for testing. In this no_std port we model it with
//! a configuration and running state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A test D-Bus daemon (`GTestDBus`).
pub struct TestDBus {
    address: Mutex<String>,
    running: Mutex<bool>,
    config_dir: Mutex<Option<String>>,
}

impl TestDBus {
    pub fn new() -> Self {
        Self {
            address: Mutex::new(String::new()),
            running: Mutex::new(false),
            config_dir: Mutex::new(None),
        }
    }

    pub fn get_bus_address(&self) -> String {
        self.address.lock().clone()
    }
    pub fn set_bus_address(&self, addr: &str) {
        *self.address.lock() = addr.to_string();
    }
    pub fn is_running(&self) -> bool {
        *self.running.lock()
    }
    pub fn start(&self) {
        *self.running.lock() = true;
    }
    pub fn stop(&self) {
        *self.running.lock() = false;
    }
    pub fn set_config_dir(&self, dir: Option<String>) {
        *self.config_dir.lock() = dir;
    }
    pub fn get_config_dir(&self) -> Option<String> {
        self.config_dir.lock().clone()
    }
}

impl Default for TestDBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_stop() {
        let d = TestDBus::new();
        d.start();
        assert!(d.is_running());
        d.stop();
        assert!(!d.is_running());
    }

    #[test]
    fn test_address() {
        let d = TestDBus::new();
        d.set_bus_address("unix:abstract=test");
        assert_eq!(d.get_bus_address(), "unix:abstract=test");
    }
}
