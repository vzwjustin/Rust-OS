//! GDBusInterfaceSkeleton matching `gio/gdbusinterfaceskeleton.h`.
//! A server-side D-Bus interface skeleton. In this no_std port we model
//! it with object path, interface name, and export state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// Export flags for `GDBusInterfaceSkeleton`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DBusInterfaceSkeletonFlags(pub u32);

impl DBusInterfaceSkeletonFlags {
    pub const NONE: Self = Self(0);
    pub const EXPOSE_METHODS: Self = Self(1 << 0);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A D-Bus interface skeleton (`GDBusInterfaceSkeleton`).
pub struct DBusInterfaceSkeleton {
    interface_name: Mutex<String>,
    object_path: Mutex<Option<String>>,
    flags: Mutex<DBusInterfaceSkeletonFlags>,
    exported: Mutex<bool>,
}

impl DBusInterfaceSkeleton {
    pub fn new(interface_name: &str) -> Self {
        Self {
            interface_name: Mutex::new(interface_name.to_string()),
            object_path: Mutex::new(None),
            flags: Mutex::new(DBusInterfaceSkeletonFlags::NONE),
            exported: Mutex::new(false),
        }
    }

    pub fn get_interface_name(&self) -> String {
        self.interface_name.lock().clone()
    }

    pub fn get_object_path(&self) -> Option<String> {
        self.object_path.lock().clone()
    }

    pub fn set_object_path(&self, path: Option<String>) {
        *self.object_path.lock() = path;
    }

    pub fn get_flags(&self) -> DBusInterfaceSkeletonFlags {
        *self.flags.lock()
    }

    pub fn set_flags(&self, flags: DBusInterfaceSkeletonFlags) {
        *self.flags.lock() = flags;
    }

    pub fn export(&self, object_path: &str) {
        *self.object_path.lock() = Some(object_path.to_string());
        *self.exported.lock() = true;
    }

    pub fn unexport(&self) {
        *self.object_path.lock() = None;
        *self.exported.lock() = false;
    }

    pub fn is_exported(&self) -> bool {
        *self.exported.lock()
    }

    pub fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = DBusInterfaceSkeleton::new("org.test.Iface");
        assert_eq!(s.get_interface_name(), "org.test.Iface");
        assert!(!s.is_exported());
    }

    #[test]
    fn test_export_unexport() {
        let s = DBusInterfaceSkeleton::new("org.test.Iface");
        s.export("/org/test/obj");
        assert!(s.is_exported());
        assert_eq!(s.get_object_path(), Some("/org/test/obj".to_string()));
        s.unexport();
        assert!(!s.is_exported());
    }

    #[test]
    fn test_flags() {
        let s = DBusInterfaceSkeleton::new("org.test.Iface");
        s.set_flags(DBusInterfaceSkeletonFlags::EXPOSE_METHODS);
        assert!(s
            .get_flags()
            .contains(DBusInterfaceSkeletonFlags::EXPOSE_METHODS));
    }
}
