//! GDebugControllerDBus matching `gio/gdebugcontrollerdbus.h`.
//! D-Bus-based debug controller. In this no_std port we model it
//! extending DebugController with a D-Bus export path.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A D-Bus debug controller (`GDebugControllerDBus`).
pub struct DebugControllerDBus {
    debug_enabled: Mutex<bool>,
    object_path: Mutex<String>,
    exported: Mutex<bool>,
}

impl DebugControllerDBus {
    pub fn new(object_path: &str) -> Self {
        Self {
            debug_enabled: Mutex::new(false),
            object_path: Mutex::new(object_path.to_string()),
            exported: Mutex::new(false),
        }
    }

    pub fn get_debug_enabled(&self) -> bool {
        *self.debug_enabled.lock()
    }
    pub fn set_debug_enabled(&self, enabled: bool) {
        *self.debug_enabled.lock() = enabled;
    }
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }
    pub fn export(&self) {
        *self.exported.lock() = true;
    }
    pub fn unexport(&self) {
        *self.exported.lock() = false;
    }
    pub fn is_exported(&self) -> bool {
        *self.exported.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let c = DebugControllerDBus::new("/org/test/Debug");
        assert_eq!(c.get_object_path(), "/org/test/Debug");
        assert!(!c.is_exported());
    }

    #[test]
    fn test_enable_export() {
        let c = DebugControllerDBus::new("/debug");
        c.set_debug_enabled(true);
        c.export();
        assert!(c.get_debug_enabled());
        assert!(c.is_exported());
    }
}
