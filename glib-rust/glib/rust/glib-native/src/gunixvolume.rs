//! GUnixVolume matching `gio/gunixvolume.h`.
//! A Unix volume. In this no_std port we model it with name, device,
//! and mount state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A Unix volume (`GUnixVolume`).
pub struct UnixVolume {
    name: Mutex<String>,
    device: Mutex<String>,
    mounted: Mutex<bool>,
    mount_path: Mutex<Option<String>>,
}

impl UnixVolume {
    pub fn new(name: &str, device: &str) -> Self {
        Self {
            name: Mutex::new(name.to_string()),
            device: Mutex::new(device.to_string()),
            mounted: Mutex::new(false),
            mount_path: Mutex::new(None),
        }
    }

    pub fn get_name(&self) -> String {
        self.name.lock().clone()
    }
    pub fn get_device(&self) -> String {
        self.device.lock().clone()
    }
    pub fn is_mounted(&self) -> bool {
        *self.mounted.lock()
    }
    pub fn get_mount_path(&self) -> Option<String> {
        self.mount_path.lock().clone()
    }

    pub fn mount(&self, path: &str) {
        *self.mounted.lock() = true;
        *self.mount_path.lock() = Some(path.to_string());
    }

    pub fn unmount(&self) {
        *self.mounted.lock() = false;
        *self.mount_path.lock() = None;
    }

    pub fn can_mount(&self) -> bool {
        true
    }
    pub fn can_eject(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_unmount() {
        let v = UnixVolume::new("USB Drive", "/dev/sdb1");
        assert!(!v.is_mounted());
        v.mount("/mnt/usb");
        assert!(v.is_mounted());
        assert_eq!(v.get_mount_path(), Some("/mnt/usb".to_string()));
        v.unmount();
        assert!(!v.is_mounted());
    }
}
