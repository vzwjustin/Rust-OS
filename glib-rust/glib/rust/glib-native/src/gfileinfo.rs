//! GFileInfo matching `gio/gfileinfo.h`.
//!
//! Stores file metadata as attribute key-value pairs. Upstream uses a
//! hash table with typed values; we use `BTreeMap<String, FileAttributeValue>`.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// File type (`GFileType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Unknown = 0,
    Regular = 1,
    Directory = 2,
    SymbolicLink = 3,
    Special = 4,
    Shortcut = 5,
    Mountable = 6,
}

/// File attribute value type (`GFileAttributeType`).
#[derive(Debug, Clone, PartialEq)]
pub enum FileAttributeValue {
    String(String),
    ByteString(Vec<u8>),
    Boolean(bool),
    Uint32(u32),
    Int32(i32),
    Uint64(u64),
    Int64(i64),
    Object(String),
    Stringv(Vec<String>),
}

impl FileAttributeValue {
    pub fn as_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::ByteString(b) => alloc::string::String::from_utf8_lossy(b).into_owned(),
            Self::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
            Self::Uint32(v) => v.to_string(),
            Self::Int32(v) => v.to_string(),
            Self::Uint64(v) => v.to_string(),
            Self::Int64(v) => v.to_string(),
            Self::Object(s) => s.clone(),
            Self::Stringv(v) => v.join(","),
        }
    }

    pub fn as_boolean(&self) -> bool {
        matches!(self, Self::Boolean(true))
    }

    pub fn as_uint32(&self) -> u32 {
        match self {
            Self::Uint32(v) => *v,
            Self::Int32(v) => *v as u32,
            _ => 0,
        }
    }

    pub fn as_uint64(&self) -> u64 {
        match self {
            Self::Uint64(v) => *v,
            Self::Int64(v) => *v as u64,
            Self::Uint32(v) => *v as u64,
            _ => 0,
        }
    }
}

/// Standard file attribute key constants.
pub const FILE_ATTRIBUTE_STANDARD_TYPE: &str = "standard::type";
pub const FILE_ATTRIBUTE_STANDARD_IS_HIDDEN: &str = "standard::is-hidden";
pub const FILE_ATTRIBUTE_STANDARD_IS_BACKUP: &str = "standard::is-backup";
pub const FILE_ATTRIBUTE_STANDARD_IS_SYMLINK: &str = "standard::is-symlink";
pub const FILE_ATTRIBUTE_STANDARD_NAME: &str = "standard::name";
pub const FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME: &str = "standard::display-name";
pub const FILE_ATTRIBUTE_STANDARD_SIZE: &str = "standard::size";
pub const FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE: &str = "standard::content-type";
pub const FILE_ATTRIBUTE_STANDARD_SYMLINK_TARGET: &str = "standard::symlink-target";
pub const FILE_ATTRIBUTE_STANDARD_SORT_ORDER: &str = "standard::sort-order";
pub const FILE_ATTRIBUTE_ETAG_VALUE: &str = "etag::value";
pub const FILE_ATTRIBUTE_TIME_MODIFIED: &str = "time::modified";

/// File information (`GFileInfo`).
pub struct FileInfo {
    attributes: Mutex<BTreeMap<String, FileAttributeValue>>,
}

impl FileInfo {
    /// Creates a new empty `GFileInfo`.
    ///
    /// Mirrors `g_file_info_new`.
    pub fn new() -> Self {
        Self {
            attributes: Mutex::new(BTreeMap::new()),
        }
    }

    /// Gets a file attribute as a string.
    ///
    /// Mirrors `g_file_info_get_attribute`.
    pub fn get_attribute(&self, attribute: &str) -> Option<FileAttributeValue> {
        self.attributes.lock().get(attribute).cloned()
    }

    /// Sets a file attribute.
    ///
    /// Mirrors `g_file_info_set_attribute`.
    pub fn set_attribute(&self, attribute: &str, value: FileAttributeValue) {
        self.attributes.lock().insert(attribute.to_string(), value);
    }

    /// Gets a string attribute.
    ///
    /// Mirrors `g_file_info_get_attribute_string`.
    pub fn get_attribute_string(&self, attribute: &str) -> Option<String> {
        self.attributes.lock().get(attribute).map(|v| v.as_string())
    }

    /// Sets a string attribute.
    pub fn set_attribute_string(&self, attribute: &str, value: &str) {
        self.set_attribute(attribute, FileAttributeValue::String(value.to_string()));
    }

    /// Gets a boolean attribute.
    pub fn get_attribute_boolean(&self, attribute: &str) -> bool {
        self.attributes
            .lock()
            .get(attribute)
            .map(|v| v.as_boolean())
            .unwrap_or(false)
    }

    /// Sets a boolean attribute.
    pub fn set_attribute_boolean(&self, attribute: &str, value: bool) {
        self.set_attribute(attribute, FileAttributeValue::Boolean(value));
    }

    /// Gets a uint32 attribute.
    pub fn get_attribute_uint32(&self, attribute: &str) -> u32 {
        self.attributes
            .lock()
            .get(attribute)
            .map(|v| v.as_uint32())
            .unwrap_or(0)
    }

    /// Sets a uint32 attribute.
    pub fn set_attribute_uint32(&self, attribute: &str, value: u32) {
        self.set_attribute(attribute, FileAttributeValue::Uint32(value));
    }

    /// Gets a uint64 attribute.
    pub fn get_attribute_uint64(&self, attribute: &str) -> u64 {
        self.attributes
            .lock()
            .get(attribute)
            .map(|v| v.as_uint64())
            .unwrap_or(0)
    }

    /// Sets a uint64 attribute.
    pub fn set_attribute_uint64(&self, attribute: &str, value: u64) {
        self.set_attribute(attribute, FileAttributeValue::Uint64(value));
    }

    /// Checks if an attribute exists.
    ///
    /// Mirrors `g_file_info_has_attribute`.
    pub fn has_attribute(&self, attribute: &str) -> bool {
        self.attributes.lock().contains_key(attribute)
    }

    /// Lists all attribute names.
    ///
    /// Mirrors `g_file_info_list_attributes`.
    pub fn list_attributes(&self) -> Vec<String> {
        self.attributes.lock().keys().cloned().collect()
    }

    /// Removes an attribute.
    ///
    /// Mirrors `g_file_info_remove_attribute`.
    pub fn remove_attribute(&self, attribute: &str) {
        self.attributes.lock().remove(attribute);
    }

    /// Clears all attributes.
    ///
    /// Mirrors `g_file_info_clear_status`.
    pub fn clear_status(&self) {
        self.attributes.lock().clear();
    }

    // ── Helper getters ──────────────────────────────────────────

    /// Gets the file type.
    pub fn get_file_type(&self) -> FileType {
        match self.get_attribute_uint32(FILE_ATTRIBUTE_STANDARD_TYPE) {
            1 => FileType::Regular,
            2 => FileType::Directory,
            3 => FileType::SymbolicLink,
            4 => FileType::Special,
            5 => FileType::Shortcut,
            6 => FileType::Mountable,
            _ => FileType::Unknown,
        }
    }

    /// Sets the file type.
    pub fn set_file_type(&self, ftype: FileType) {
        self.set_attribute_uint32(FILE_ATTRIBUTE_STANDARD_TYPE, ftype as u32);
    }

    /// Gets whether the file is hidden.
    pub fn get_is_hidden(&self) -> bool {
        self.get_attribute_boolean(FILE_ATTRIBUTE_STANDARD_IS_HIDDEN)
    }

    /// Sets whether the file is hidden.
    pub fn set_is_hidden(&self, is_hidden: bool) {
        self.set_attribute_boolean(FILE_ATTRIBUTE_STANDARD_IS_HIDDEN, is_hidden);
    }

    /// Gets whether the file is a symlink.
    pub fn get_is_symlink(&self) -> bool {
        self.get_attribute_boolean(FILE_ATTRIBUTE_STANDARD_IS_SYMLINK)
    }

    /// Sets whether the file is a symlink.
    pub fn set_is_symlink(&self, is_symlink: bool) {
        self.set_attribute_boolean(FILE_ATTRIBUTE_STANDARD_IS_SYMLINK, is_symlink);
    }

    /// Gets the file name.
    pub fn get_name(&self) -> Option<String> {
        self.get_attribute_string(FILE_ATTRIBUTE_STANDARD_NAME)
    }

    /// Sets the file name.
    pub fn set_name(&self, name: &str) {
        self.set_attribute_string(FILE_ATTRIBUTE_STANDARD_NAME, name);
    }

    /// Gets the display name.
    pub fn get_display_name(&self) -> Option<String> {
        self.get_attribute_string(FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME)
    }

    /// Sets the display name.
    pub fn set_display_name(&self, name: &str) {
        self.set_attribute_string(FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME, name);
    }

    /// Gets the file size.
    pub fn get_size(&self) -> u64 {
        self.get_attribute_uint64(FILE_ATTRIBUTE_STANDARD_SIZE)
    }

    /// Sets the file size.
    pub fn set_size(&self, size: u64) {
        self.set_attribute_uint64(FILE_ATTRIBUTE_STANDARD_SIZE, size);
    }

    /// Gets the content type.
    pub fn get_content_type(&self) -> Option<String> {
        self.get_attribute_string(FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE)
    }

    /// Sets the content type.
    pub fn set_content_type(&self, ctype: &str) {
        self.set_attribute_string(FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE, ctype);
    }

    /// Gets the symlink target.
    pub fn get_symlink_target(&self) -> Option<String> {
        self.get_attribute_string(FILE_ATTRIBUTE_STANDARD_SYMLINK_TARGET)
    }

    /// Sets the symlink target.
    pub fn set_symlink_target(&self, target: &str) {
        self.set_attribute_string(FILE_ATTRIBUTE_STANDARD_SYMLINK_TARGET, target);
    }

    /// Copies all attributes from another `FileInfo`.
    ///
    /// Mirrors `g_file_info_copy_into`.
    pub fn copy_into(&self, dest: &FileInfo) {
        let src = self.attributes.lock();
        let mut dst = dest.attributes.lock();
        for (k, v) in src.iter() {
            dst.insert(k.clone(), v.clone());
        }
    }

    /// Duplicates the `FileInfo`.
    ///
    /// Mirrors `g_file_info_dup`.
    pub fn dup(&self) -> FileInfo {
        let copy = FileInfo::new();
        self.copy_into(&copy);
        copy
    }
}

impl Default for FileInfo {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let info = FileInfo::new();
        assert!(info.list_attributes().is_empty());
    }

    #[test]
    fn test_set_and_get_string() {
        let info = FileInfo::new();
        info.set_attribute_string("standard::name", "test.txt");
        assert_eq!(
            info.get_attribute_string("standard::name").unwrap(),
            "test.txt"
        );
    }

    #[test]
    fn test_set_and_get_boolean() {
        let info = FileInfo::new();
        info.set_attribute_boolean("standard::is-hidden", true);
        assert!(info.get_attribute_boolean("standard::is-hidden"));
        assert!(!info.get_attribute_boolean("standard::is-backup"));
    }

    #[test]
    fn test_set_and_get_uint32() {
        let info = FileInfo::new();
        info.set_attribute_uint32("custom::count", 42);
        assert_eq!(info.get_attribute_uint32("custom::count"), 42);
    }

    #[test]
    fn test_set_and_get_uint64() {
        let info = FileInfo::new();
        info.set_attribute_uint64("standard::size", 1048576);
        assert_eq!(info.get_size(), 1048576);
    }

    #[test]
    fn test_file_type() {
        let info = FileInfo::new();
        info.set_file_type(FileType::Directory);
        assert_eq!(info.get_file_type(), FileType::Directory);
    }

    #[test]
    fn test_file_type_regular() {
        let info = FileInfo::new();
        info.set_file_type(FileType::Regular);
        assert_eq!(info.get_file_type(), FileType::Regular);
    }

    #[test]
    fn test_is_hidden() {
        let info = FileInfo::new();
        info.set_is_hidden(true);
        assert!(info.get_is_hidden());
    }

    #[test]
    fn test_is_symlink() {
        let info = FileInfo::new();
        info.set_is_symlink(true);
        assert!(info.get_is_symlink());
    }

    #[test]
    fn test_name_and_display_name() {
        let info = FileInfo::new();
        info.set_name("file.txt");
        info.set_display_name("File.txt");
        assert_eq!(info.get_name().unwrap(), "file.txt");
        assert_eq!(info.get_display_name().unwrap(), "File.txt");
    }

    #[test]
    fn test_content_type() {
        let info = FileInfo::new();
        info.set_content_type("text/plain");
        assert_eq!(info.get_content_type().unwrap(), "text/plain");
    }

    #[test]
    fn test_symlink_target() {
        let info = FileInfo::new();
        info.set_symlink_target("/target/path");
        assert_eq!(info.get_symlink_target().unwrap(), "/target/path");
    }

    #[test]
    fn test_has_attribute() {
        let info = FileInfo::new();
        info.set_name("test");
        assert!(info.has_attribute("standard::name"));
        assert!(!info.has_attribute("standard::size"));
    }

    #[test]
    fn test_remove_attribute() {
        let info = FileInfo::new();
        info.set_name("test");
        info.remove_attribute("standard::name");
        assert!(!info.has_attribute("standard::name"));
    }

    #[test]
    fn test_list_attributes() {
        let info = FileInfo::new();
        info.set_name("a");
        info.set_size(100);
        info.set_is_hidden(true);
        let attrs = info.list_attributes();
        assert_eq!(attrs.len(), 3);
    }

    #[test]
    fn test_dup() {
        let info = FileInfo::new();
        info.set_name("original");
        info.set_size(2048);
        let copy = info.dup();
        assert_eq!(copy.get_name().unwrap(), "original");
        assert_eq!(copy.get_size(), 2048);
    }

    #[test]
    fn test_copy_into() {
        let src = FileInfo::new();
        src.set_name("source");
        src.set_file_type(FileType::Regular);
        let dst = FileInfo::new();
        src.copy_into(&dst);
        assert_eq!(dst.get_name().unwrap(), "source");
        assert_eq!(dst.get_file_type(), FileType::Regular);
    }

    #[test]
    fn test_clear_status() {
        let info = FileInfo::new();
        info.set_name("test");
        info.clear_status();
        assert!(info.list_attributes().is_empty());
    }
}
