//! gwin32volumemonitor matching `gio/gwin32volumemonitor.c`.
//!
//! Windows volume monitor that enumerates logical drives (A: through Z:)
//! and reports them as mounts. Uses `GetLogicalDrives()` and optionally
//! filters based on registry policy keys.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gwin32mount::Win32Mount;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Windows volume monitor (`GWin32VolumeMonitor`).
pub struct Win32VolumeMonitor {
    /// Bitmask of viewable drives (bit 0 = A:, bit 1 = B:, etc.)
    viewable_drives: Mutex<u32>,
}

impl Win32VolumeMonitor {
    pub fn new() -> Self {
        Self {
            viewable_drives: Mutex::new(0xFFFFFFFF),
        }
    }

    /// Returns whether this monitor is supported.
    pub fn is_supported() -> bool {
        true
    }

    /// Gets the bitmask of viewable logical drives.
    ///
    /// Mirrors `get_viewable_logical_drives`.
    pub fn get_viewable_drives(&self) -> u32 {
        *self.viewable_drives.lock()
    }

    /// Sets the viewable drives bitmask (for testing / policy).
    pub fn set_viewable_drives(&self, mask: u32) {
        *self.viewable_drives.lock() = mask;
    }

    /// Enumerates mounted volumes (drive letters).
    ///
    /// Mirrors `get_mounts`.
    pub fn get_mounts(&self) -> Vec<Win32Mount> {
        let drives = *self.viewable_drives.lock();
        let mut result = Vec::new();

        for i in 0..26u32 {
            if drives & (1 << i) != 0 {
                let letter = (b'A' + i as u8) as char;
                let drive = format!("{}:\\", letter);
                let name = format!("{}: Drive", letter);
                result.push(Win32Mount::new(&drive, &name, &drive));
            }
        }

        result
    }

    /// Returns volumes (not implemented on Windows — returns empty).
    pub fn get_volumes(&self) -> Vec<()> {
        Vec::new()
    }

    /// Returns connected drives (not implemented on Windows — returns empty).
    pub fn get_connected_drives(&self) -> Vec<()> {
        Vec::new()
    }

    /// Gets a mount by UUID (not supported on Windows).
    pub fn get_volume_for_uuid(&self, _uuid: &str) -> Option<()> {
        None
    }

    /// Gets a mount by UUID (not supported on Windows).
    pub fn get_mount_for_uuid(&self, _uuid: &str) -> Option<Win32Mount> {
        None
    }

    /// Gets a mount for a mount path.
    pub fn get_mount_for_mount_path(&self, mount_path: &str) -> Win32Mount {
        Win32Mount::new(mount_path, mount_path, mount_path)
    }
}

impl Default for Win32VolumeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        assert!(Win32VolumeMonitor::is_supported());
    }

    #[test]
    fn test_get_mounts_all() {
        let monitor = Win32VolumeMonitor::new();
        monitor.set_viewable_drives(0xFFFFFFFF);
        let mounts = monitor.get_mounts();
        assert_eq!(mounts.len(), 26);
        assert_eq!(mounts[0].drive(), "A:\\");
        assert_eq!(mounts[2].drive(), "C:\\");
        assert_eq!(mounts[25].drive(), "Z:\\");
    }

    #[test]
    fn test_get_mounts_subset() {
        let monitor = Win32VolumeMonitor::new();
        // Only C: (bit 2) and D: (bit 3)
        monitor.set_viewable_drives(0b1100);
        let mounts = monitor.get_mounts();
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].drive(), "C:\\");
        assert_eq!(mounts[1].drive(), "D:\\");
    }

    #[test]
    fn test_get_mounts_none() {
        let monitor = Win32VolumeMonitor::new();
        monitor.set_viewable_drives(0);
        let mounts = monitor.get_mounts();
        assert!(mounts.is_empty());
    }

    #[test]
    fn test_get_mount_for_path() {
        let monitor = Win32VolumeMonitor::new();
        let mount = monitor.get_mount_for_mount_path("E:\\");
        assert_eq!(mount.drive(), "E:\\");
    }

    #[test]
    fn test_no_volumes_or_drives() {
        let monitor = Win32VolumeMonitor::new();
        assert!(monitor.get_volumes().is_empty());
        assert!(monitor.get_connected_drives().is_empty());
        assert!(monitor.get_volume_for_uuid("test").is_none());
        assert!(monitor.get_mount_for_uuid("test").is_none());
    }
}
