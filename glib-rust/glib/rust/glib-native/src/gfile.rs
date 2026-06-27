//! GIO file operations matching `gio/gfile.h` / `gio/gfile.c`.
//!
//! Provides the base `File` and `FileInfo` structs, file type enums, flags,
//! and a platform registration mechanism for filesystem integration.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use crate::gioerror::IOErrorEnum;
use crate::goutputstream::OutputStream;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::RwLock;

/// File type classifications matching `GFileType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FileType {
    /// Unknown file type (`G_FILE_TYPE_UNKNOWN`).
    Unknown = 0,
    /// Regular file (`G_FILE_TYPE_REGULAR`).
    Regular,
    /// Directory (`G_FILE_TYPE_DIRECTORY`).
    Directory,
    /// Symbolic link (`G_FILE_TYPE_SYMBOLIC_LINK`).
    SymbolicLink,
    /// Special file e.g. socket, FIFO (`G_FILE_TYPE_SPECIAL`).
    Special,
    /// Shortcut file (`G_FILE_TYPE_SHORTCUT`).
    ShortCut,
    /// Mountable location (`G_FILE_TYPE_MOUNTABLE`).
    Mountable,
}

/// Metadata attributes and information about a file (`GFileInfo`).
#[derive(Clone, Debug)]
pub struct FileInfo {
    size: u64,
    file_type: FileType,
    name: String,
    attributes: BTreeMap<String, String>,
}

impl FileInfo {
    /// Create a new empty `FileInfo`.
    ///
    /// Mirrors `g_file_info_new`.
    pub fn new() -> Self {
        Self {
            size: 0,
            file_type: FileType::Unknown,
            name: String::new(),
            attributes: BTreeMap::new(),
        }
    }

    /// Gets the file size in bytes.
    ///
    /// Mirrors `g_file_info_get_size`.
    pub fn get_size(&self) -> u64 {
        self.size
    }

    /// Sets the file size.
    ///
    /// Mirrors `g_file_info_set_size`.
    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }

    /// Gets the file type.
    ///
    /// Mirrors `g_file_info_get_file_type`.
    pub fn get_file_type(&self) -> FileType {
        self.file_type
    }

    /// Sets the file type.
    ///
    /// Mirrors `g_file_info_set_file_type`.
    pub fn set_file_type(&mut self, file_type: FileType) {
        self.file_type = file_type;
    }

    /// Gets the display name of the file.
    ///
    /// Mirrors `g_file_info_get_name`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Sets the name of the file.
    ///
    /// Mirrors `g_file_info_set_name`.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.into();
    }

    /// Gets a string attribute value.
    ///
    /// Mirrors `g_file_info_get_attribute_string`.
    pub fn get_attribute_string(&self, attribute: &str) -> Option<&str> {
        self.attributes.get(attribute).map(|s| s.as_str())
    }

    /// Sets a string attribute value.
    ///
    /// Mirrors `g_file_info_set_attribute_string`.
    pub fn set_attribute_string(&mut self, attribute: &str, value: &str) {
        self.attributes.insert(attribute.into(), value.into());
    }
}

impl Default for FileInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Creation flags matching `GFileCreateFlags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FileCreateFlags {
    /// No flags.
    None = 0,
    /// Overwrite destination if it exists.
    ReplaceDestination = 1 << 0,
    /// Create file with private permissions.
    Private = 1 << 1,
}

/// Query info flags matching `GFileQueryInfoFlags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FileQueryInfoFlags {
    /// No flags.
    None = 0,
    /// Do not follow symbolic links.
    NofollowSymlinks = 1 << 0,
}

/// A handle to a file or directory resource (`GFile`).
#[derive(Clone, Debug)]
pub struct File {
    path: Option<String>,
    uri: String,
}

impl File {
    /// Create a new `File` from a local path.
    ///
    /// Mirrors `g_file_new_for_path`.
    pub fn new_for_path(path: &str) -> Self {
        let canonical_path = crate::fileutils::canonicalize_filename(path, None);
        let uri = crate::convert::filename_to_uri(&canonical_path, None)
            .unwrap_or_else(|_| format!("file://{}", path));
        Self {
            path: Some(canonical_path),
            uri,
        }
    }

    /// Create a new `File` from a local path.
    ///
    /// Compatibility alias for callers that use the shorter Rust-style
    /// constructor name.
    pub fn for_path(path: &str) -> Self {
        Self::new_for_path(path)
    }

    /// Create a new `File` from a URI string.
    ///
    /// Mirrors `g_file_new_for_uri`.
    pub fn new_for_uri(uri: &str) -> Self {
        let path = crate::convert::filename_from_uri(uri).ok().map(|(p, _)| p);
        Self {
            path,
            uri: uri.into(),
        }
    }

    /// Create a new `File` from a command line argument.
    ///
    /// Mirrors `g_file_new_for_commandline_arg`.
    pub fn new_for_commandline_arg(arg: &str) -> Self {
        if crate::fileutils::path_is_absolute(arg) {
            Self::new_for_path(arg)
        } else {
            Self::new_for_path(arg)
        }
    }

    /// Gets the local path, if available.
    ///
    /// Mirrors `g_file_get_path`.
    pub fn get_path(&self) -> Option<String> {
        self.path.clone()
    }

    /// Gets the URI representing the file.
    ///
    /// Mirrors `g_file_get_uri`.
    pub fn get_uri(&self) -> String {
        self.uri.clone()
    }

    /// Gets the basename of the file.
    ///
    /// Mirrors `g_file_get_basename`.
    pub fn get_basename(&self) -> Option<String> {
        self.path
            .as_ref()
            .map(|p| crate::fileutils::path_get_basename(p))
    }

    /// Gets the parent directory of the file.
    ///
    /// Mirrors `g_file_get_parent`.
    pub fn get_parent(&self) -> Option<File> {
        self.path.as_ref().and_then(|p| {
            let parent_dir = crate::fileutils::path_get_dirname(p);
            if parent_dir == *p {
                None
            } else {
                Some(File::new_for_path(&parent_dir))
            }
        })
    }

    /// Checks if the file exists.
    ///
    /// Mirrors `g_file_query_exists`.
    pub fn query_exists(&self, cancellable: Option<&GCancellable>) -> bool {
        if let Some(c) = cancellable {
            if c.is_cancelled() {
                return false;
            }
        }
        if let Some(p) = &self.path {
            FilePlatformWrapper::query_exists(p)
        } else {
            false
        }
    }

    /// Opens an input stream for reading the file.
    ///
    /// Mirrors `g_file_read`.
    pub fn read(&self, cancellable: Option<&GCancellable>) -> Result<InputStream, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::read(path)
    }

    /// Opens an output stream for creating/overwriting the file.
    ///
    /// Mirrors `g_file_create`.
    pub fn create(
        &self,
        flags: FileCreateFlags,
        cancellable: Option<&GCancellable>,
    ) -> Result<OutputStream, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::create(path, flags)
    }

    /// Replaces the file with new content.
    ///
    /// Mirrors `g_file_replace`.
    pub fn replace(
        &self,
        etag: Option<&str>,
        make_backup: bool,
        flags: FileCreateFlags,
        cancellable: Option<&GCancellable>,
    ) -> Result<OutputStream, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::replace(path, etag, make_backup, flags)
    }

    /// Queries info metadata for the file.
    ///
    /// Mirrors `g_file_query_info`.
    pub fn query_info(
        &self,
        attributes: &str,
        flags: FileQueryInfoFlags,
        cancellable: Option<&GCancellable>,
    ) -> Result<FileInfo, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::query_info(path, attributes, flags)
    }

    /// Deletes the file.
    ///
    /// Mirrors `g_file_delete`.
    pub fn delete(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::delete(path)
    }

    /// Moves the file to trash when the platform supports it.
    ///
    /// Mirrors `g_file_trash`.
    pub fn trash(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let path = self.path.as_ref().ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "File has no path",
            )
        })?;
        FilePlatformWrapper::trash(path)
    }
}

// ────────────────────────── Platform Trait ─────────────────────────────────

/// Platform trait for backend filesystem operations.
pub trait FilePlatform: Sync {
    /// Open input stream for reading file.
    fn read(&self, path: &str) -> Result<InputStream, Error>;

    /// Open output stream for creating file.
    fn create(&self, path: &str, flags: FileCreateFlags) -> Result<OutputStream, Error>;

    /// Open output stream for replacing file.
    fn replace(
        &self,
        path: &str,
        etag: Option<&str>,
        make_backup: bool,
        flags: FileCreateFlags,
    ) -> Result<OutputStream, Error>;

    /// Checks if path exists.
    fn query_exists(&self, path: &str) -> bool;

    /// Queries metadata info.
    fn query_info(
        &self,
        path: &str,
        attributes: &str,
        flags: FileQueryInfoFlags,
    ) -> Result<FileInfo, Error>;

    /// Deletes path.
    fn delete(&self, path: &str) -> Result<(), Error>;

    /// Moves path to trash.
    fn trash(&self, path: &str) -> Result<(), Error>;
}

/// A stub no-op filesystem platform.
pub struct NoFilePlatform;

impl FilePlatform for NoFilePlatform {
    fn read(&self, _path: &str) -> Result<InputStream, Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File read not supported",
        ))
    }

    fn create(&self, _path: &str, _flags: FileCreateFlags) -> Result<OutputStream, Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File create not supported",
        ))
    }

    fn replace(
        &self,
        _path: &str,
        _etag: Option<&str>,
        _make_backup: bool,
        _flags: FileCreateFlags,
    ) -> Result<OutputStream, Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File replace not supported",
        ))
    }

    fn query_exists(&self, _path: &str) -> bool {
        false
    }

    fn query_info(
        &self,
        _path: &str,
        _attributes: &str,
        _flags: FileQueryInfoFlags,
    ) -> Result<FileInfo, Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File query info not supported",
        ))
    }

    fn delete(&self, _path: &str) -> Result<(), Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File delete not supported",
        ))
    }

    fn trash(&self, _path: &str) -> Result<(), Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "File trash not supported",
        ))
    }
}

static FILE_PLATFORM: RwLock<&'static dyn FilePlatform> = RwLock::new(&NoFilePlatform);

/// Register the active file platform driver.
pub fn register_file_platform(platform: &'static dyn FilePlatform) {
    *FILE_PLATFORM.write() = platform;
}

struct FilePlatformWrapper;

impl FilePlatformWrapper {
    fn read(path: &str) -> Result<InputStream, Error> {
        FILE_PLATFORM.read().read(path)
    }

    fn create(path: &str, flags: FileCreateFlags) -> Result<OutputStream, Error> {
        FILE_PLATFORM.read().create(path, flags)
    }

    fn replace(
        path: &str,
        etag: Option<&str>,
        make_backup: bool,
        flags: FileCreateFlags,
    ) -> Result<OutputStream, Error> {
        FILE_PLATFORM.read().replace(path, etag, make_backup, flags)
    }

    fn query_exists(path: &str) -> bool {
        FILE_PLATFORM.read().query_exists(path)
    }

    fn query_info(
        path: &str,
        attributes: &str,
        flags: FileQueryInfoFlags,
    ) -> Result<FileInfo, Error> {
        FILE_PLATFORM.read().query_info(path, attributes, flags)
    }

    fn delete(path: &str) -> Result<(), Error> {
        FILE_PLATFORM.read().delete(path)
    }

    fn trash(path: &str) -> Result<(), Error> {
        FILE_PLATFORM.read().trash(path)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::MemoryInputStream;
    use crate::goutputstream::MemoryOutputStream;

    #[test]
    fn test_file_info_setters_and_getters() {
        let mut info = FileInfo::new();
        info.set_size(1024);
        info.set_file_type(FileType::Regular);
        info.set_name("document.txt");
        info.set_attribute_string("standard::content-type", "text/plain");

        assert_eq!(info.get_size(), 1024);
        assert_eq!(info.get_file_type(), FileType::Regular);
        assert_eq!(info.get_name(), "document.txt");
        assert_eq!(
            info.get_attribute_string("standard::content-type"),
            Some("text/plain")
        );
    }

    #[test]
    fn test_file_paths_and_uris() {
        let file = File::new_for_path("/usr/bin/git");
        assert_eq!(file.get_path(), Some("/usr/bin/git".to_owned()));
        assert_eq!(file.get_basename(), Some("git".to_owned()));

        let alias_file = File::for_path("/usr/bin/git");
        assert_eq!(alias_file.get_path(), file.get_path());
        assert_eq!(alias_file.get_uri(), file.get_uri());

        let parent = file.get_parent().unwrap();
        assert_eq!(parent.get_path(), Some("/usr/bin".to_owned()));

        let root_parent = File::new_for_path("/").get_parent();
        assert!(root_parent.is_none());
    }

    #[test]
    fn test_file_from_uri() {
        let file = File::new_for_uri("file:///home/user/test.txt");
        assert_eq!(file.get_path(), Some("/home/user/test.txt".to_owned()));
        assert_eq!(file.get_basename(), Some("test.txt".to_owned()));
    }

    struct MockFilePlatform;
    impl FilePlatform for MockFilePlatform {
        fn read(&self, path: &str) -> Result<InputStream, Error> {
            if path == "/mock.txt" {
                Ok(InputStream::from(MemoryInputStream::new_from_bytes(
                    Bytes::from_static(b"mock bytes"),
                )))
            } else {
                Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::NotFound.to_code(),
                    "File not found",
                ))
            }
        }
        fn create(&self, path: &str, _flags: FileCreateFlags) -> Result<OutputStream, Error> {
            if path == "/new.txt" {
                Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
            } else {
                Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::PermissionDenied.to_code(),
                    "Denied",
                ))
            }
        }
        fn replace(
            &self,
            _path: &str,
            _etag: Option<&str>,
            _make_backup: bool,
            _flags: FileCreateFlags,
        ) -> Result<OutputStream, Error> {
            Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
        }

        fn delete(&self, path: &str) -> Result<(), Error> {
            if path == "/mock.txt" {
                Ok(())
            } else {
                Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::NotFound.to_code(),
                    "File not found",
                ))
            }
        }

        fn trash(&self, path: &str) -> Result<(), Error> {
            self.delete(path)
        }

        fn query_exists(&self, path: &str) -> bool {
            path == "/mock.txt"
        }
        fn query_info(
            &self,
            path: &str,
            _attributes: &str,
            _flags: FileQueryInfoFlags,
        ) -> Result<FileInfo, Error> {
            if path == "/mock.txt" {
                let mut info = FileInfo::new();
                info.set_size(10);
                info.set_file_type(FileType::Regular);
                Ok(info)
            } else {
                Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::NotFound.to_code(),
                    "Not found",
                ))
            }
        }
    }

    static MOCK_PLATFORM: MockFilePlatform = MockFilePlatform;

    #[test]
    fn test_file_operations_with_registered_platform() {
        register_file_platform(&MOCK_PLATFORM);

        let mock_file = File::new_for_path("/mock.txt");
        assert!(mock_file.query_exists(None));

        let stream = mock_file.read(None).unwrap();
        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 10);
        assert_eq!(&buf, b"mock bytes");

        let info = mock_file
            .query_info("*", FileQueryInfoFlags::None, None)
            .unwrap();
        assert_eq!(info.get_size(), 10);
        assert_eq!(info.get_file_type(), FileType::Regular);
        mock_file.delete(None).unwrap();
        mock_file.trash(None).unwrap();

        let new_file = File::new_for_path("/new.txt");
        let write_stream = new_file.create(FileCreateFlags::None, None).unwrap();
        let written = write_stream.write(b"content", None).unwrap();
        assert_eq!(written, 7);

        // Reset to default
        static DEFAULT_PLATFORM: NoFilePlatform = NoFilePlatform;
        register_file_platform(&DEFAULT_PLATFORM);
    }
}
