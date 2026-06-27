//! GLocalFileInfo matching `gio/glocalfileinfo.h`.
//! File info for local files. In this no_std port we model it with
//! common file attributes.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// File type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFileType {
    Regular,
    Directory,
    Symlink,
    Special,
    Unknown,
}

/// Local file info (`GLocalFileInfo`).
pub struct LocalFileInfo {
    name: Mutex<String>,
    file_type: Mutex<LocalFileType>,
    size: Mutex<u64>,
    is_hidden: Mutex<bool>,
    is_backup: Mutex<bool>,
    is_symlink: Mutex<bool>,
}

impl LocalFileInfo {
    pub fn new(name: &str) -> Self {
        Self {
            name: Mutex::new(name.to_string()),
            file_type: Mutex::new(LocalFileType::Unknown),
            size: Mutex::new(0),
            is_hidden: Mutex::new(false),
            is_backup: Mutex::new(false),
            is_symlink: Mutex::new(false),
        }
    }

    pub fn get_name(&self) -> String {
        self.name.lock().clone()
    }
    pub fn get_file_type(&self) -> LocalFileType {
        *self.file_type.lock()
    }
    pub fn set_file_type(&self, t: LocalFileType) {
        *self.file_type.lock() = t;
    }
    pub fn get_size(&self) -> u64 {
        *self.size.lock()
    }
    pub fn set_size(&self, size: u64) {
        *self.size.lock() = size;
    }
    pub fn is_hidden(&self) -> bool {
        *self.is_hidden.lock()
    }
    pub fn set_hidden(&self, hidden: bool) {
        *self.is_hidden.lock() = hidden;
    }
    pub fn is_backup(&self) -> bool {
        *self.is_backup.lock()
    }
    pub fn set_backup(&self, backup: bool) {
        *self.is_backup.lock() = backup;
    }
    pub fn is_symlink(&self) -> bool {
        *self.is_symlink.lock()
    }
    pub fn set_symlink(&self, symlink: bool) {
        *self.is_symlink.lock() = symlink;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let info = LocalFileInfo::new("test.txt");
        assert_eq!(info.get_name(), "test.txt");
        assert_eq!(info.get_file_type(), LocalFileType::Unknown);
    }

    #[test]
    fn test_set_attrs() {
        let info = LocalFileInfo::new(".hidden");
        info.set_file_type(LocalFileType::Regular);
        info.set_hidden(true);
        info.set_size(1024);
        assert_eq!(info.get_file_type(), LocalFileType::Regular);
        assert!(info.is_hidden());
        assert_eq!(info.get_size(), 1024);
    }
}
