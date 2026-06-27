//! GNativeVolumeMonitor matching `gio/gnativevolumemonitor.h`.
//! A native volume monitor. In this no_std port we model it as a
//! registry of mounted volumes and drives.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A native volume monitor (`GNativeVolumeMonitor`).
pub struct NativeVolumeMonitor {
    volumes: Mutex<Vec<String>>,
    mounts: Mutex<Vec<String>>,
}

impl NativeVolumeMonitor {
    pub fn new() -> Self {
        Self {
            volumes: Mutex::new(Vec::new()),
            mounts: Mutex::new(Vec::new()),
        }
    }

    pub fn add_volume(&self, name: &str) {
        self.volumes.lock().push(name.to_string());
    }
    pub fn remove_volume(&self, name: &str) -> bool {
        let mut v = self.volumes.lock();
        let before = v.len();
        v.retain(|n| n != name);
        v.len() != before
    }
    pub fn get_volumes(&self) -> Vec<String> {
        self.volumes.lock().clone()
    }
    pub fn volume_count(&self) -> usize {
        self.volumes.lock().len()
    }

    pub fn add_mount(&self, path: &str) {
        self.mounts.lock().push(path.to_string());
    }
    pub fn remove_mount(&self, path: &str) -> bool {
        let mut m = self.mounts.lock();
        let before = m.len();
        m.retain(|p| p != path);
        m.len() != before
    }
    pub fn get_mounts(&self) -> Vec<String> {
        self.mounts.lock().clone()
    }
    pub fn mount_count(&self) -> usize {
        self.mounts.lock().len()
    }
}

impl Default for NativeVolumeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volumes() {
        let m = NativeVolumeMonitor::new();
        m.add_volume("sda1");
        m.add_volume("sdb1");
        assert_eq!(m.volume_count(), 2);
        assert!(m.remove_volume("sda1"));
        assert_eq!(m.volume_count(), 1);
    }

    #[test]
    fn test_mounts() {
        let m = NativeVolumeMonitor::new();
        m.add_mount("/mnt/usb");
        assert_eq!(m.mount_count(), 1);
    }
}
