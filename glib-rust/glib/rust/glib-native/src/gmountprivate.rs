//! `gmountprivate` matching `gio/gmountprivate.h`.
//!
//! Private mount API: get mount for a mount path.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A simple mount entry.
#[derive(Debug, Clone)]
pub struct MountEntry {
    pub mount_path: String,
    pub filesystem_type: String,
    pub device_path: Option<String>,
    pub is_read_only: bool,
}

/// Returns the mount entry for a given mount path
/// (mirrors `_g_mount_get_for_mount_path`).
pub fn get_for_mount_path(mount_path: &str) -> Option<MountEntry> {
    MOUNTS
        .lock()
        .iter()
        .find(|m| m.mount_path == mount_path)
        .cloned()
}

/// Returns all registered mounts.
pub fn get_all_mounts() -> Vec<MountEntry> {
    MOUNTS.lock().clone()
}

/// Registers a mount entry.
pub fn register_mount(entry: MountEntry) {
    MOUNTS.lock().push(entry);
}

/// Clears all mount entries (for testing).
pub fn clear_mounts() {
    MOUNTS.lock().clear();
}

static MOUNTS: Mutex<Vec<MountEntry>> = Mutex::new(Vec::new());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_for_mount_path() {
        clear_mounts();
        register_mount(MountEntry {
            mount_path: "/mnt/data".to_string(),
            filesystem_type: "ext4".to_string(),
            device_path: Some("/dev/sda1".to_string()),
            is_read_only: false,
        });
        let mount = get_for_mount_path("/mnt/data");
        assert!(mount.is_some());
        assert_eq!(mount.unwrap().filesystem_type, "ext4");
    }

    #[test]
    fn test_not_found() {
        clear_mounts();
        assert!(get_for_mount_path("/nonexistent").is_none());
    }
}
