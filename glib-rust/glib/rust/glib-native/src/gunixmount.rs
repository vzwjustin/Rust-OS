//! GUnixMount matching `gio/gunixmount.h`.
//! A Unix mount entry. In this no_std port we model it with path,
//! filesystem type, and device.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};

/// A Unix mount entry (`GUnixMountEntry`).
pub struct UnixMountEntry {
    pub mount_path: String,
    pub filesystem_type: String,
    pub device_path: String,
    pub is_read_only: bool,
    pub is_user_mountable: bool,
}

impl UnixMountEntry {
    pub fn new(mount_path: &str, filesystem_type: &str, device_path: &str) -> Self {
        Self {
            mount_path: mount_path.to_string(),
            filesystem_type: filesystem_type.to_string(),
            device_path: device_path.to_string(),
            is_read_only: false,
            is_user_mountable: false,
        }
    }

    pub fn get_mount_path(&self) -> &str {
        &self.mount_path
    }
    pub fn get_filesystem_type(&self) -> &str {
        &self.filesystem_type
    }
    pub fn get_device_path(&self) -> &str {
        &self.device_path
    }
    pub fn is_read_only(&self) -> bool {
        self.is_read_only
    }
    pub fn is_user_mountable(&self) -> bool {
        self.is_user_mountable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = UnixMountEntry::new("/mnt/usb", "vfat", "/dev/sdb1");
        assert_eq!(m.get_mount_path(), "/mnt/usb");
        assert_eq!(m.get_filesystem_type(), "vfat");
        assert_eq!(m.get_device_path(), "/dev/sdb1");
    }
}
