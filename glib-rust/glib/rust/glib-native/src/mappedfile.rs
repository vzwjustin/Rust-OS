//! Mapped file matching `gmappedfile.h` / `gmappedfile.c`.
//!
//! Defines types for memory-mapped files. Actual file mapping
//! requires OS support (mmap) and is deferred to a platform layer.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::bytes::Bytes;

/// A memory-mapped file (`GMappedFile`).
///
/// In no_std, the file contents are loaded into a `Vec<u8>` by a
/// platform implementation. On a real OS, this would use `mmap`.
pub struct MappedFile {
    contents: Vec<u8>,
    writable: bool,
}

impl MappedFile {
    /// Create a new mapped file from contents.
    ///
    /// On a real system, a platform implementation would use `mmap`
    /// to map the file and then call this with the mapped bytes.
    pub fn from_contents(contents: Vec<u8>, writable: bool) -> Self {
        Self { contents, writable }
    }

    /// Get the length of the mapped file (`g_mapped_file_get_length`).
    pub fn get_length(&self) -> usize {
        self.contents.len()
    }

    /// Get the contents of the mapped file (`g_mapped_file_get_contents`).
    pub fn get_contents(&self) -> &[u8] {
        &self.contents
    }

    /// Get the contents as `Bytes` (`g_mapped_file_get_bytes`).
    pub fn get_bytes(&self) -> Bytes {
        Bytes::new(&self.contents[..])
    }

    /// Check if the file is writable.
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    /// Get mutable contents (if writable).
    pub fn get_contents_mut(&mut self) -> Option<&mut [u8]> {
        if self.writable {
            Some(&mut self.contents)
        } else {
            None
        }
    }
}

/// Platform trait for mapping files.
pub trait MappedFilePlatform {
    /// Open and map a file (`g_mapped_file_new`).
    fn open(path: &str, writable: bool) -> Result<MappedFile, MappedFileError>;

    /// Map from a file descriptor (`g_mapped_file_new_from_fd`).
    fn open_from_fd(fd: i32, writable: bool) -> Result<MappedFile, MappedFileError>;
}

/// Mapped file errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MappedFileError {
    NotFound,
    PermissionDenied,
    InvalidFd,
    MapFailed,
    TooLarge,
    Other,
}

/// A no-op platform implementation.
pub struct NoMappedFilePlatform;

impl MappedFilePlatform for NoMappedFilePlatform {
    fn open(_path: &str, _writable: bool) -> Result<MappedFile, MappedFileError> {
        Err(MappedFileError::Other)
    }

    fn open_from_fd(_fd: i32, _writable: bool) -> Result<MappedFile, MappedFileError> {
        Err(MappedFileError::InvalidFd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_file_from_contents() {
        let data = vec![1u8, 2, 3, 4, 5];
        let mf = MappedFile::from_contents(data.clone(), false);
        assert_eq!(mf.get_length(), 5);
        assert_eq!(mf.get_contents(), &data[..]);
        assert!(!mf.is_writable());
    }

    #[test]
    fn mapped_file_writable() {
        let data = vec![0u8; 16];
        let mut mf = MappedFile::from_contents(data, true);
        assert!(mf.is_writable());
        let contents = mf.get_contents_mut().unwrap();
        contents[0] = 42;
        assert_eq!(mf.get_contents()[0], 42);
    }

    #[test]
    fn mapped_file_not_writable() {
        let data = vec![0u8; 8];
        let mut mf = MappedFile::from_contents(data, false);
        assert!(mf.get_contents_mut().is_none());
    }

    #[test]
    fn mapped_file_bytes() {
        let data = vec![1u8, 2, 3];
        let mf = MappedFile::from_contents(data, false);
        let bytes = mf.get_bytes();
        assert_eq!(bytes.len(), 3);
    }

    #[test]
    fn no_platform_fails() {
        assert!(NoMappedFilePlatform::open("/tmp/test", false).is_err());
        assert!(NoMappedFilePlatform::open_from_fd(0, false).is_err());
    }
}
