//! GDummyFile matching `gio/gdummyfile.h`.
//! A dummy `GFile` implementation. In this no_std port we model it
//! with a URI string.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A dummy file (`GDummyFile`).
pub struct DummyFile {
    uri: Mutex<String>,
}

impl DummyFile {
    pub fn new(uri: &str) -> Self {
        Self {
            uri: Mutex::new(uri.to_string()),
        }
    }

    pub fn get_uri(&self) -> String {
        self.uri.lock().clone()
    }
    pub fn get_path(&self) -> Option<String> {
        let uri = self.uri.lock();
        if uri.starts_with("file://") {
            Some(uri[7..].to_string())
        } else {
            None
        }
    }
    pub fn get_basename(&self) -> String {
        let uri = self.uri.lock();
        uri.rsplit('/').next().unwrap_or(&uri).to_string()
    }
    pub fn query_exists(&self) -> bool {
        !self.uri.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri() {
        let f = DummyFile::new("http://example.com/file.txt");
        assert_eq!(f.get_uri(), "http://example.com/file.txt");
        assert_eq!(f.get_basename(), "file.txt");
    }

    #[test]
    fn test_file_path() {
        let f = DummyFile::new("file:///tmp/test.txt");
        assert_eq!(f.get_path(), Some("/tmp/test.txt".to_string()));
    }
}
