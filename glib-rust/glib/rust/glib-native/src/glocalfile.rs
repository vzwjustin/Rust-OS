//! GLocalFile matching `gio/glocalfile.h`.
//! A `GFile` backed by the local filesystem. In this no_std port we
//! model it with a path and file metadata.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A local file (`GLocalFile`).
pub struct LocalFile {
    path: Mutex<String>,
    exists: Mutex<bool>,
    is_dir: Mutex<bool>,
    size: Mutex<u64>,
}

impl LocalFile {
    pub fn new(path: &str) -> Self {
        Self {
            path: Mutex::new(path.to_string()),
            exists: Mutex::new(false),
            is_dir: Mutex::new(false),
            size: Mutex::new(0),
        }
    }

    pub fn get_path(&self) -> String {
        self.path.lock().clone()
    }
    pub fn get_uri(&self) -> String {
        alloc::format!("file://{}", self.path.lock())
    }
    pub fn get_basename(&self) -> String {
        self.path
            .lock()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string()
    }

    pub fn query_exists(&self) -> bool {
        *self.exists.lock()
    }
    pub fn set_exists(&self, exists: bool) {
        *self.exists.lock() = exists;
    }
    pub fn is_directory(&self) -> bool {
        *self.is_dir.lock()
    }
    pub fn set_is_directory(&self, is_dir: bool) {
        *self.is_dir.lock() = is_dir;
    }
    pub fn get_size(&self) -> u64 {
        *self.size.lock()
    }
    pub fn set_size(&self, size: u64) {
        *self.size.lock() = size;
    }

    pub fn delete(&self) -> bool {
        *self.exists.lock() = false;
        true
    }

    pub fn create(&self) -> bool {
        *self.exists.lock() = true;
        *self.is_dir.lock() = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let f = LocalFile::new("/tmp/test.txt");
        assert_eq!(f.get_path(), "/tmp/test.txt");
        assert_eq!(f.get_uri(), "file:///tmp/test.txt");
        assert_eq!(f.get_basename(), "test.txt");
    }

    #[test]
    fn test_create_delete() {
        let f = LocalFile::new("/tmp/test.txt");
        f.create();
        assert!(f.query_exists());
        f.delete();
        assert!(!f.query_exists());
    }
}
