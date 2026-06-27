//! GFileIcon matching `gio/gfileicon.h`.
//!
//! Upstream `GFileIcon` is a `GIcon` backed by a `GFile`. We port it
//! as a struct wrapping a file path string, implementing `LoadableIcon`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::{InputStream, MemoryInputStream};
use crate::gloadableicon::LoadableIcon;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A file-based icon (`GFileIcon`).
///
/// Wraps a file path and implements `LoadableIcon` by reading the
/// file contents into a `FileInputStream`.
pub struct FileIcon {
    file_path: String,
    cached_data: Mutex<Option<Vec<u8>>>,
}

impl FileIcon {
    /// Creates a new file icon from a file path.
    ///
    /// Mirrors `g_file_icon_new`.
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            cached_data: Mutex::new(None),
        }
    }

    /// Gets the file path backing this icon.
    ///
    /// Mirrors `g_file_icon_get_file`.
    pub fn get_file(&self) -> &str {
        &self.file_path
    }

    /// Sets cached icon data (simulates loading from filesystem).
    pub fn set_data(&self, data: &[u8]) {
        *self.cached_data.lock() = Some(data.to_vec());
    }
}

impl LoadableIcon for FileIcon {
    fn load(
        &self,
        _size: i32,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(InputStream, Option<String>), Error> {
        let data = self.cached_data.lock();
        match &*data {
            Some(d) => {
                let bytes = Bytes::new(&d[..]);
                let stream = InputStream::new(MemoryInputStream::new_from_bytes(bytes));
                Ok((stream, Some("image/png".to_string())))
            }
            None => Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::NotFound.to_code(),
                "No icon data available",
            )),
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_icon_new() {
        let icon = FileIcon::new("/usr/share/icons/test.png");
        assert_eq!(icon.get_file(), "/usr/share/icons/test.png");
    }

    #[test]
    fn test_file_icon_load_no_data() {
        let icon = FileIcon::new("/usr/share/icons/test.png");
        assert!(icon.load(48, None).is_err());
    }

    #[test]
    fn test_file_icon_load_with_data() {
        let icon = FileIcon::new("/usr/share/icons/test.png");
        icon.set_data(b"fake png data");
        let (stream, icon_type) = icon.load(48, None).unwrap();
        assert_eq!(icon_type.unwrap(), "image/png");
        let mut buf = [0u8; 13];
        let (n, _) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 13);
        assert_eq!(&buf, b"fake png data");
    }

    #[test]
    fn test_file_icon_load_partial() {
        let icon = FileIcon::new("/tmp/icon.svg");
        icon.set_data(b"hello world");
        let (stream, _) = icon.load(0, None).unwrap();
        let mut buf = [0u8; 5];
        let (n, _) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
    }
}
