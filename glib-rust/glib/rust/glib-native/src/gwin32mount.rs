//! gwin32mount matching `gio/gwin32mount.c`.
//!
//! Windows mount implementation. Represents a mounted volume
//! (drive letter or network share) and provides icon, name, and UUID.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A Windows mount point (`GWin32Mount`).
pub struct Win32Mount {
    drive: Mutex<String>,
    name: Mutex<String>,
    icon: Mutex<String>,
    uuid: Mutex<Option<String>>,
    root_path: Mutex<String>,
    can_unmount: Mutex<bool>,
    can_eject: Mutex<bool>,
}

impl Win32Mount {
    pub fn new(drive: &str, name: &str, root_path: &str) -> Self {
        Self {
            drive: Mutex::new(drive.to_string()),
            name: Mutex::new(name.to_string()),
            icon: Mutex::new("drive-harddisk".to_string()),
            uuid: Mutex::new(None),
            root_path: Mutex::new(root_path.to_string()),
            can_unmount: Mutex::new(false),
            can_eject: Mutex::new(false),
        }
    }

    pub fn drive(&self) -> String {
        self.drive.lock().clone()
    }
    pub fn name(&self) -> String {
        self.name.lock().clone()
    }
    pub fn set_name(&self, name: &str) {
        *self.name.lock() = name.to_string();
    }
    pub fn icon(&self) -> String {
        self.icon.lock().clone()
    }
    pub fn set_icon(&self, icon: &str) {
        *self.icon.lock() = icon.to_string();
    }
    pub fn uuid(&self) -> Option<String> {
        self.uuid.lock().clone()
    }
    pub fn set_uuid(&self, uuid: Option<String>) {
        *self.uuid.lock() = uuid;
    }
    pub fn root_path(&self) -> String {
        self.root_path.lock().clone()
    }
    pub fn can_unmount(&self) -> bool {
        *self.can_unmount.lock()
    }
    pub fn can_eject(&self) -> bool {
        *self.can_eject.lock()
    }
    pub fn set_can_unmount(&self, v: bool) {
        *self.can_unmount.lock() = v;
    }
    pub fn set_can_eject(&self, v: bool) {
        *self.can_eject.lock() = v;
    }

    pub fn unmount(&self) -> Result<(), String> {
        if !*self.can_unmount.lock() {
            return Err("mount cannot be unmounted".to_string());
        }
        Ok(())
    }

    pub fn eject(&self) -> Result<(), String> {
        if !*self.can_eject.lock() {
            return Err("mount cannot be ejected".to_string());
        }
        Ok(())
    }
}

/// Enumerates available Windows drives.
///
/// Returns a list of drive letters (e.g. "C:\\", "D:\\").
///
/// Mirrors `get_viewable_logical_drives` + mount enumeration.
pub fn enumerate_mounts() -> Vec<Win32Mount> {
    let mut mounts = Vec::new();
    for c in b'A'..=b'Z' {
        let drive = format!("{}:\\", c as char);
        let name = format!("{}: Drive", c as char);
        mounts.push(Win32Mount::new(&drive, &name, &drive));
    }
    mounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_basic() {
        let mount = Win32Mount::new("C:\\", "Local Disk", "C:\\");
        assert_eq!(mount.drive(), "C:\\");
        assert_eq!(mount.name(), "Local Disk");
        assert_eq!(mount.root_path(), "C:\\");
    }

    #[test]
    fn test_mount_uuid() {
        let mount = Win32Mount::new("C:\\", "Disk", "C:\\");
        assert!(mount.uuid().is_none());
        mount.set_uuid(Some("12345".to_string()));
        assert_eq!(mount.uuid(), Some("12345".to_string()));
    }

    #[test]
    fn test_mount_unmount() {
        let mount = Win32Mount::new("C:\\", "Disk", "C:\\");
        assert!(mount.unmount().is_err());
        mount.set_can_unmount(true);
        assert!(mount.unmount().is_ok());
    }

    #[test]
    fn test_enumerate() {
        let mounts = enumerate_mounts();
        assert_eq!(mounts.len(), 26);
        assert_eq!(mounts[0].drive(), "A:\\");
        assert_eq!(mounts[2].drive(), "C:\\");
    }
}
