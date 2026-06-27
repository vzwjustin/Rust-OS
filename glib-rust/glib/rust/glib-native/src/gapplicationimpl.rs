//! GApplicationImpl matching `gio/gapplicationimpl.h`.
//! Application implementation backend. In this no_std port we model it
//! with application ID, bus name, and registration state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// An application implementation (`GApplicationImpl`).
pub struct ApplicationImpl {
    app_id: Mutex<String>,
    registered: Mutex<bool>,
    remote: Mutex<bool>,
    bus_address: Mutex<String>,
}

impl ApplicationImpl {
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: Mutex::new(app_id.to_string()),
            registered: Mutex::new(false),
            remote: Mutex::new(false),
            bus_address: Mutex::new(String::new()),
        }
    }

    pub fn get_app_id(&self) -> String {
        self.app_id.lock().clone()
    }
    pub fn is_registered(&self) -> bool {
        *self.registered.lock()
    }
    pub fn register(&self) {
        *self.registered.lock() = true;
    }
    pub fn unregister(&self) {
        *self.registered.lock() = false;
    }
    pub fn is_remote(&self) -> bool {
        *self.remote.lock()
    }
    pub fn set_remote(&self, remote: bool) {
        *self.remote.lock() = remote;
    }
    pub fn get_bus_address(&self) -> String {
        self.bus_address.lock().clone()
    }
    pub fn set_bus_address(&self, addr: &str) {
        *self.bus_address.lock() = addr.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register() {
        let a = ApplicationImpl::new("org.test.App");
        assert_eq!(a.get_app_id(), "org.test.App");
        assert!(!a.is_registered());
        a.register();
        assert!(a.is_registered());
    }
}
