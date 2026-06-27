//! GLocalVfs matching `gio/glocalvfs.h`.
//! A local VFS implementation. In this no_std port we model it with
//! a file resolution map.
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// A local VFS (`GLocalVfs`).
pub struct LocalVfs {
    files: Mutex<BTreeMap<String, String>>,
}

impl LocalVfs {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn get_file(&self, path: &str) -> String {
        self.files
            .lock()
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_string())
    }

    pub fn get_file_for_uri(&self, uri: &str) -> String {
        if uri.starts_with("file://") {
            uri[7..].to_string()
        } else {
            uri.to_string()
        }
    }

    pub fn register_file(&self, path: &str, target: &str) {
        self.files
            .lock()
            .insert(path.to_string(), target.to_string());
    }

    pub fn is_local(&self, uri: &str) -> bool {
        uri.starts_with("file://") || !uri.contains("://")
    }
}

impl Default for LocalVfs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_to_path() {
        let v = LocalVfs::new();
        assert_eq!(v.get_file_for_uri("file:///tmp/test.txt"), "/tmp/test.txt");
    }

    #[test]
    fn test_is_local() {
        let v = LocalVfs::new();
        assert!(v.is_local("file:///tmp/test.txt"));
        assert!(v.is_local("/tmp/test.txt"));
        assert!(!v.is_local("http://example.com/"));
    }
}
