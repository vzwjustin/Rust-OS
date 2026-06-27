//! GResourceFile matching `gio/gresourcefile.h`.
//! A `GFile` backed by a `GResource`. In this no_std port we model it
//! with a resource path and contents.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A resource file (`GResourceFile`).
pub struct ResourceFile {
    path: Mutex<String>,
    contents: Mutex<Vec<u8>>,
}

impl ResourceFile {
    pub fn new(path: &str) -> Self {
        Self {
            path: Mutex::new(path.to_string()),
            contents: Mutex::new(Vec::new()),
        }
    }

    pub fn new_with_contents(path: &str, contents: Vec<u8>) -> Self {
        Self {
            path: Mutex::new(path.to_string()),
            contents: Mutex::new(contents),
        }
    }

    pub fn get_path(&self) -> String {
        self.path.lock().clone()
    }

    pub fn get_uri(&self) -> String {
        alloc::format!("resource://{}", self.path.lock())
    }

    pub fn get_contents(&self) -> Vec<u8> {
        self.contents.lock().clone()
    }

    pub fn set_contents(&self, data: &[u8]) {
        *self.contents.lock() = data.to_vec();
    }

    pub fn size(&self) -> usize {
        self.contents.lock().len()
    }

    pub fn query_exists(&self) -> bool {
        !self.path.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let f = ResourceFile::new("/org/test/data.txt");
        assert_eq!(f.get_path(), "/org/test/data.txt");
        assert_eq!(f.get_uri(), "resource:///org/test/data.txt");
        assert!(f.query_exists());
    }

    #[test]
    fn test_contents() {
        let f = ResourceFile::new_with_contents("/test", b"hello world".to_vec());
        assert_eq!(f.size(), 11);
        assert_eq!(f.get_contents(), b"hello world".to_vec());
    }
}
