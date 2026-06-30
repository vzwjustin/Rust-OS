//! GFileEnumerator matching `gio/gfileenumerator.h`.
//!
//! Upstream `GFileEnumerator` enumerates files in a directory. We port
//! it as a struct with a `Mutex`-protected list of `FileInfo` entries.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gfile::{File, FileInfo};
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// A file enumerator (`GFileEnumerator`).
pub struct FileEnumerator {
    container: File,
    entries: Mutex<Vec<FileInfo>>,
    index: Mutex<usize>,
    closed: Mutex<bool>,
    pending: Mutex<bool>,
}

impl FileEnumerator {
    /// Creates a new file enumerator for a container file.
    pub fn new(container: File, entries: Vec<FileInfo>) -> Self {
        Self {
            container,
            entries: Mutex::new(entries),
            index: Mutex::new(0),
            closed: Mutex::new(false),
            pending: Mutex::new(false),
        }
    }

    /// Returns the next file info, or `None` if exhausted.
    ///
    /// Mirrors `g_file_enumerator_next_file`.
    pub fn next_file(
        &self,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Option<FileInfo>, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Enumerator is closed",
            ));
        }
        let mut idx = self.index.lock();
        let entries = self.entries.lock();
        if *idx >= entries.len() {
            return Ok(None);
        }
        let info = entries[*idx].clone();
        *idx += 1;
        Ok(Some(info))
    }

    /// Closes the enumerator.
    ///
    /// Mirrors `g_file_enumerator_close`.
    pub fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        *self.closed.lock() = true;
        Ok(())
    }

    /// Checks if the enumerator is closed.
    ///
    /// Mirrors `g_file_enumerator_is_closed`.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    /// Checks if the enumerator has a pending operation.
    ///
    /// Mirrors `g_file_enumerator_has_pending`.
    pub fn has_pending(&self) -> bool {
        *self.pending.lock()
    }

    /// Sets the pending flag.
    ///
    /// Mirrors `g_file_enumerator_set_pending`.
    pub fn set_pending(&self, pending: bool) {
        *self.pending.lock() = pending;
    }

    /// Gets the container file.
    ///
    /// Mirrors `g_file_enumerator_get_container`.
    pub fn get_container(&self) -> &File {
        &self.container
    }

    /// Gets a child file for the given info.
    ///
    /// Mirrors `g_file_enumerator_get_child`.
    pub fn get_child(&self, info: &FileInfo) -> File {
        let name = info.get_name();
        let container_path = self.container.get_path().unwrap_or_else(|| "/".to_string());
        let child_path = if container_path.ends_with('/') {
            format!("{}{}", container_path, name)
        } else {
            format!("{}/{}", container_path, name)
        };
        File::new_for_path(&child_path)
    }

    /// Iterates to the next file, returning `(info, child)`.
    ///
    /// Mirrors `g_file_enumerator_iterate`.
    pub fn iterate(
        &self,
        cancellable: Option<&GCancellable>,
    ) -> Result<(Option<FileInfo>, Option<File>), Error> {
        let info = self.next_file(cancellable)?;
        if let Some(ref i) = info {
            let child = self.get_child(i);
            Ok((Some(i.clone()), Some(child)))
        } else {
            Ok((None, None))
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfile::FileType;

    fn make_info(name: &str, size: u64) -> FileInfo {
        let mut info = FileInfo::new();
        info.set_name(name);
        info.set_size(size);
        info.set_file_type(FileType::Regular);
        info
    }

    #[test]
    fn test_enumerator_new() {
        let container = File::new_for_path("/test");
        let entries = vec![make_info("a.txt", 10), make_info("b.txt", 20)];
        let enumerator = FileEnumerator::new(container, entries);
        assert!(!enumerator.is_closed());
        assert!(!enumerator.has_pending());
    }

    #[test]
    fn test_next_file() {
        let container = File::new_for_path("/test");
        let entries = vec![make_info("a.txt", 10), make_info("b.txt", 20)];
        let enumerator = FileEnumerator::new(container, entries);
        let info1 = enumerator.next_file(None).unwrap().unwrap();
        assert_eq!(info1.get_name(), "a.txt");
        let info2 = enumerator.next_file(None).unwrap().unwrap();
        assert_eq!(info2.get_name(), "b.txt");
        assert!(enumerator.next_file(None).unwrap().is_none());
    }

    #[test]
    fn test_close() {
        let container = File::new_for_path("/test");
        let enumerator = FileEnumerator::new(container, vec![]);
        enumerator.close(None).unwrap();
        assert!(enumerator.is_closed());
        assert!(enumerator.next_file(None).is_err());
    }

    #[test]
    fn test_pending() {
        let container = File::new_for_path("/test");
        let enumerator = FileEnumerator::new(container, vec![]);
        assert!(!enumerator.has_pending());
        enumerator.set_pending(true);
        assert!(enumerator.has_pending());
    }

    #[test]
    fn test_get_container() {
        let container = File::new_for_path("/test");
        let enumerator = FileEnumerator::new(container.clone(), vec![]);
        assert_eq!(enumerator.get_container().get_path(), container.get_path());
    }

    #[test]
    fn test_get_child() {
        let container = File::new_for_path("/test");
        let entries = vec![make_info("a.txt", 10)];
        let enumerator = FileEnumerator::new(container, entries);
        let info = enumerator.next_file(None).unwrap().unwrap();
        let child = enumerator.get_child(&info);
        assert!(child.get_path().unwrap().contains("a.txt"));
    }

    #[test]
    fn test_iterate() {
        let container = File::new_for_path("/test");
        let entries = vec![make_info("a.txt", 10)];
        let enumerator = FileEnumerator::new(container, entries);
        let (info, child) = enumerator.iterate(None).unwrap();
        assert!(info.is_some());
        assert!(child.is_some());
        let (info2, child2) = enumerator.iterate(None).unwrap();
        assert!(info2.is_none());
        assert!(child2.is_none());
    }

    #[test]
    fn test_empty_enumerator() {
        let container = File::new_for_path("/test");
        let enumerator = FileEnumerator::new(container, vec![]);
        assert!(enumerator.next_file(None).unwrap().is_none());
    }
}
