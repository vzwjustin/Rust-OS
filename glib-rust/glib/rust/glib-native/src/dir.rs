//! Directory operations matching `gdir.h` / `gdir.c`.
//!
//! Defines the `Dir` type and a platform trait for directory iteration.
//! Actual directory access requires OS support and is deferred to a
//! platform abstraction layer. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use spin::RwLock;

/// `g_dir_open` flags: names are case-sensitive.
pub const DIR_CASE_SENSITIVE: u32 = 0;
/// `g_dir_open` flags: skip `.` and `..`.
pub const DIR_NO_DOT_AND_DOTDOT: u32 = 1 << 0;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirError {
    NotFound,
    NotDirectory,
    PermissionDenied,
    InvalidUtf8,
    Other,
}

/// A directory iterator (`GDir`).
///
/// In no_std, directory entries are provided by a platform-specific
/// implementation that collects them into a `Vec<String>`.
pub struct Dir {
    entries: Vec<String>,
    index: usize,
}

impl Dir {
    /// Create a new `Dir` from a list of entry names.
    ///
    /// On a real system, this would be called by a platform implementation
    /// that reads the directory using OS syscalls.
    pub fn from_entries(entries: Vec<String>) -> Self {
        Self { entries, index: 0 }
    }

    /// Read the next entry name (`g_dir_read_name`).
    ///
    /// Returns `None` when all entries have been read.
    pub fn read_name(&mut self) -> Option<&str> {
        if self.index >= self.entries.len() {
            return None;
        }
        let entry = &self.entries[self.index];
        self.index += 1;
        Some(entry)
    }

    /// Rewind to the beginning (`g_dir_rewind`).
    pub fn rewind(&mut self) {
        self.index = 0;
    }

    /// Close the directory (`g_dir_close`).
    ///
    /// In Rust, this is equivalent to dropping the `Dir`.
    pub fn close(self) {
        // Drop is automatic
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the directory has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Platform trait for opening directories.
pub trait DirPlatform: Sync {
    /// Open a directory and return its entries.
    fn open(&self, path: &str, flags: u32) -> Result<Vec<String>, DirError>;
}

/// A no-op platform implementation that always returns an error.
pub struct NoDirPlatform;

impl DirPlatform for NoDirPlatform {
    fn open(&self, _path: &str, _flags: u32) -> Result<Vec<String>, DirError> {
        Err(DirError::Other)
    }
}

static DIR_PLATFORM: RwLock<&'static dyn DirPlatform> = RwLock::new(&NoDirPlatform);

/// Installs the platform directory implementation.
pub fn register_dir_platform(platform: &'static dyn DirPlatform) {
    *DIR_PLATFORM.write() = platform;
}

/// Opens a directory (`g_dir_open`).
pub fn dir_open(path: &str, flags: u32) -> Result<Dir, DirError> {
    let entries = DIR_PLATFORM.read().open(path, flags)?;
    Ok(Dir::from_entries(entries))
}

#[cfg(test)]
/// In-memory directory platform for unit tests.
pub struct TestDirPlatform {
    dirs: alloc::collections::BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
impl TestDirPlatform {
    pub fn new() -> Self {
        Self {
            dirs: alloc::collections::BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: &str, entries: Vec<String>) {
        self.dirs.insert(path.to_owned(), entries);
    }
}

#[cfg(test)]
impl DirPlatform for TestDirPlatform {
    fn open(&self, path: &str, flags: u32) -> Result<Vec<String>, DirError> {
        let mut entries = self.dirs.get(path).cloned().ok_or(DirError::NotFound)?;
        if flags & DIR_NO_DOT_AND_DOTDOT != 0 {
            entries.retain(|e| e != "." && e != "..");
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_iteration() {
        let mut dir = Dir::from_entries(vec![
            "file1.txt".to_owned(),
            "file2.txt".to_owned(),
            "dir1".to_owned(),
        ]);
        assert_eq!(dir.read_name(), Some("file1.txt"));
        assert_eq!(dir.read_name(), Some("file2.txt"));
        assert_eq!(dir.read_name(), Some("dir1"));
        assert_eq!(dir.read_name(), None);
    }

    #[test]
    fn dir_rewind() {
        let mut dir = Dir::from_entries(vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(dir.read_name(), Some("a"));
        dir.rewind();
        assert_eq!(dir.read_name(), Some("a"));
        assert_eq!(dir.read_name(), Some("b"));
        assert_eq!(dir.read_name(), None);
    }

    #[test]
    fn dir_empty() {
        let mut dir = Dir::from_entries(Vec::new());
        assert!(dir.is_empty());
        assert_eq!(dir.read_name(), None);
    }

    #[test]
    fn dir_len() {
        let dir = Dir::from_entries(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        assert_eq!(dir.len(), 3);
    }

    struct MockDirPlatform;
    impl DirPlatform for MockDirPlatform {
        fn open(&self, path: &str, _flags: u32) -> Result<Vec<String>, DirError> {
            if path == "/mock" {
                Ok(vec!["one".into(), "two".into()])
            } else {
                Err(DirError::NotFound)
            }
        }
    }

    #[test]
    fn test_dir_platform_in_memory() {
        let mut built = TestDirPlatform::new();
        built.insert(
            "/test",
            vec![".".into(), "..".into(), "alpha".into(), "beta".into()],
        );
        let platform: &'static TestDirPlatform = Box::leak(Box::new(built));
        register_dir_platform(platform);
        let mut dir = dir_open("/test", DIR_NO_DOT_AND_DOTDOT).unwrap();
        assert_eq!(dir.read_name(), Some("alpha"));
        assert_eq!(dir.read_name(), Some("beta"));
        assert_eq!(dir.read_name(), None);
        register_dir_platform(&NoDirPlatform);
    }

    #[test]
    fn dir_open_via_platform() {
        register_dir_platform(&MockDirPlatform);
        let mut dir = dir_open("/mock", DIR_CASE_SENSITIVE).unwrap();
        assert_eq!(dir.read_name(), Some("one"));
        register_dir_platform(&NoDirPlatform);
    }
}
