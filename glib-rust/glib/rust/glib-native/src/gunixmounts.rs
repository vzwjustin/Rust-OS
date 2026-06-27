//! GUnixMounts matching `gio/gunixmounts.h`.
//! Unix mount point monitoring. In this no_std port we model it with
//! a list of mount entries and a changed flag.
//! Fully `no_std` compatible using `alloc`.

use crate::gunixmount::UnixMountEntry;
use alloc::vec::Vec;
use spin::Mutex;

/// Unix mount monitor (`GUnixMounts`).
pub struct UnixMounts {
    mounts: Mutex<Vec<UnixMountEntry>>,
    changed: Mutex<bool>,
}

impl UnixMounts {
    pub fn new() -> Self {
        Self {
            mounts: Mutex::new(Vec::new()),
            changed: Mutex::new(false),
        }
    }

    pub fn add(&self, entry: UnixMountEntry) {
        self.mounts.lock().push(entry);
        *self.changed.lock() = true;
    }

    pub fn get_mounts(&self) -> usize {
        self.mounts.lock().len()
    }

    pub fn has_changed(&self) -> bool {
        let c = *self.changed.lock();
        *self.changed.lock() = false;
        c
    }

    pub fn find_by_path(&self, path: &str) -> bool {
        self.mounts.lock().iter().any(|m| m.mount_path == path)
    }
}

impl Default for UnixMounts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_find() {
        let m = UnixMounts::new();
        m.add(UnixMountEntry::new("/mnt/usb", "vfat", "/dev/sdb1"));
        assert_eq!(m.get_mounts(), 1);
        assert!(m.has_changed());
        assert!(!m.has_changed()); // flag cleared
        assert!(m.find_by_path("/mnt/usb"));
        assert!(!m.find_by_path("/mnt/other"));
    }
}
