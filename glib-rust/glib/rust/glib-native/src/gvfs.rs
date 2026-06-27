//! GVfs matching `gio/gvfs.h`.
//!
//! Upstream `GVfs` is a virtual file system interface for registering
//! URI schemes and looking up files. We port it as a trait with a
//! simple default implementation.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfile::File;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Trait for VFS implementations (`GVfs`).
pub trait Vfs: Send + Sync {
    /// Checks if the VFS is active.
    fn is_active(&self) -> bool;

    /// Gets a `File` for a path.
    fn get_file_for_path(&self, path: &str) -> File;

    /// Gets a `File` for a URI.
    fn get_file_for_uri(&self, uri: &str) -> File;

    /// Gets supported URI schemes.
    fn get_supported_uri_schemes(&self) -> Vec<String>;

    /// Parses a name into a `File`.
    fn parse_name(&self, parse_name: &str) -> File;
}

/// A simple local VFS implementation (`GLocalVfs`).
pub struct LocalVfs {
    active: Mutex<bool>,
    schemes: Vec<String>,
}

impl LocalVfs {
    /// Creates a new local VFS.
    pub fn new() -> Self {
        Self {
            active: Mutex::new(true),
            schemes: vec!["file".to_string()],
        }
    }
}

impl Default for LocalVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs for LocalVfs {
    fn is_active(&self) -> bool {
        *self.active.lock()
    }

    fn get_file_for_path(&self, path: &str) -> File {
        File::new_for_path(path)
    }

    fn get_file_for_uri(&self, uri: &str) -> File {
        if let Some(path) = uri.strip_prefix("file://") {
            File::new_for_path(path)
        } else {
            File::new_for_uri(uri)
        }
    }

    fn get_supported_uri_schemes(&self) -> Vec<String> {
        self.schemes.clone()
    }

    fn parse_name(&self, parse_name: &str) -> File {
        if parse_name.starts_with('/') {
            File::new_for_path(parse_name)
        } else {
            File::new_for_uri(parse_name)
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_vfs_new() {
        let vfs = LocalVfs::new();
        assert!(vfs.is_active());
    }

    #[test]
    fn test_get_file_for_path() {
        let vfs = LocalVfs::new();
        let file = vfs.get_file_for_path("/home/user");
        assert_eq!(file.get_path(), Some("/home/user".to_string()));
    }

    #[test]
    fn test_get_file_for_uri() {
        let vfs = LocalVfs::new();
        let file = vfs.get_file_for_uri("file:///home/user");
        assert_eq!(file.get_path(), Some("/home/user".to_string()));
    }

    #[test]
    fn test_get_file_for_non_file_uri() {
        let vfs = LocalVfs::new();
        let file = vfs.get_file_for_uri("http://example.com");
        assert_eq!(file.get_uri(), "http://example.com".to_string());
    }

    #[test]
    fn test_get_supported_uri_schemes() {
        let vfs = LocalVfs::new();
        let schemes = vfs.get_supported_uri_schemes();
        assert!(schemes.contains(&"file".to_string()));
    }

    #[test]
    fn test_parse_name_path() {
        let vfs = LocalVfs::new();
        let file = vfs.parse_name("/etc/passwd");
        assert_eq!(file.get_path(), Some("/etc/passwd".to_string()));
    }

    #[test]
    fn test_parse_name_uri() {
        let vfs = LocalVfs::new();
        let file = vfs.parse_name("file:///tmp");
        assert_eq!(file.get_path().unwrap(), "/tmp".to_string());
    }
}
