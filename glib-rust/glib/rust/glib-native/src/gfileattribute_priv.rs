//! `gfileattribute-priv` matching `gio/gfileattribute-priv.h`.
//!
//! Private file attribute value API: create, free, clear, set, dup,
//! peek, as_string, and typed getters/setters.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfileattribute::FileAttributeType;
use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Attribute status (mirrors `GFileAttributeStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileAttributeStatus {
    #[default]
    Unset,
    Set,
    Copy,
}

/// A file attribute value (mirrors `GFileAttributeValue`).
///
/// In the C code this is a tagged union. In Rust we use an enum.
#[derive(Debug, Clone)]
pub enum FileAttributeValueData {
    Boolean(bool),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    String(String),
    ByteString(String),
    Stringv(Vec<String>),
    Object(String),
    None,
}

impl Default for FileAttributeValueData {
    fn default() -> Self {
        Self::None
    }
}

/// A file attribute value with type and status (mirrors `GFileAttributeValue`).
#[derive(Debug, Clone, Default)]
pub struct FileAttributeValuePriv {
    pub attr_type: FileAttributeType,
    pub status: FileAttributeStatus,
    pub data: FileAttributeValueData,
}

impl FileAttributeValuePriv {
    /// Creates a new empty value (mirrors `_g_file_attribute_value_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the value (mirrors `_g_file_attribute_value_clear`).
    pub fn clear(&mut self) {
        self.attr_type = FileAttributeType::Invalid;
        self.status = FileAttributeStatus::Unset;
        self.data = FileAttributeValueData::None;
    }

    /// Sets from another value (mirrors `_g_file_attribute_value_set`).
    pub fn set(&mut self, other: &FileAttributeValuePriv) {
        self.attr_type = other.attr_type;
        self.status = other.status;
        self.data = other.data.clone();
    }

    /// Duplicates (mirrors `_g_file_attribute_value_dup`).
    pub fn dup(&self) -> FileAttributeValuePriv {
        self.clone()
    }

    /// Returns the value as a string (mirrors `_g_file_attribute_value_as_string`).
    pub fn as_string(&self) -> String {
        match &self.data {
            FileAttributeValueData::Boolean(b) => b.to_string(),
            FileAttributeValueData::Int32(i) => i.to_string(),
            FileAttributeValueData::UInt32(u) => u.to_string(),
            FileAttributeValueData::Int64(i) => i.to_string(),
            FileAttributeValueData::UInt64(u) => u.to_string(),
            FileAttributeValueData::String(s) => s.clone(),
            FileAttributeValueData::ByteString(s) => s.clone(),
            FileAttributeValueData::Stringv(v) => v.join(","),
            FileAttributeValueData::Object(s) => s.clone(),
            FileAttributeValueData::None => String::new(),
        }
    }

    // Typed getters

    pub fn get_string(&self) -> &str {
        match &self.data {
            FileAttributeValueData::String(s) | FileAttributeValueData::ByteString(s) => s,
            _ => "",
        }
    }

    pub fn get_boolean(&self) -> bool {
        matches!(self.data, FileAttributeValueData::Boolean(true))
    }

    pub fn get_uint32(&self) -> u32 {
        match self.data {
            FileAttributeValueData::UInt32(u) => u,
            _ => 0,
        }
    }

    pub fn get_int32(&self) -> i32 {
        match self.data {
            FileAttributeValueData::Int32(i) => i,
            _ => 0,
        }
    }

    pub fn get_uint64(&self) -> u64 {
        match self.data {
            FileAttributeValueData::UInt64(u) => u,
            _ => 0,
        }
    }

    pub fn get_int64(&self) -> i64 {
        match self.data {
            FileAttributeValueData::Int64(i) => i,
            _ => 0,
        }
    }

    pub fn get_stringv(&self) -> Vec<String> {
        match &self.data {
            FileAttributeValueData::Stringv(v) => v.clone(),
            _ => Vec::new(),
        }
    }

    // Typed setters

    pub fn set_string(&mut self, s: &str) {
        self.attr_type = FileAttributeType::String;
        self.data = FileAttributeValueData::String(s.to_string());
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_byte_string(&mut self, s: &str) {
        self.attr_type = FileAttributeType::ByteString;
        self.data = FileAttributeValueData::ByteString(s.to_string());
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_boolean(&mut self, b: bool) {
        self.attr_type = FileAttributeType::Boolean;
        self.data = FileAttributeValueData::Boolean(b);
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_uint32(&mut self, v: u32) {
        self.attr_type = FileAttributeType::Uint32;
        self.data = FileAttributeValueData::UInt32(v);
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_int32(&mut self, v: i32) {
        self.attr_type = FileAttributeType::Int32;
        self.data = FileAttributeValueData::Int32(v);
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_uint64(&mut self, v: u64) {
        self.attr_type = FileAttributeType::Uint64;
        self.data = FileAttributeValueData::UInt64(v);
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_int64(&mut self, v: i64) {
        self.attr_type = FileAttributeType::Int64;
        self.data = FileAttributeValueData::Int64(v);
        self.status = FileAttributeStatus::Set;
    }

    pub fn set_stringv(&mut self, v: Vec<String>) {
        self.attr_type = FileAttributeType::Stringv;
        self.data = FileAttributeValueData::Stringv(v);
        self.status = FileAttributeStatus::Set;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let v = FileAttributeValuePriv::new();
        assert_eq!(v.status, FileAttributeStatus::Unset);
    }

    #[test]
    fn test_set_and_get_string() {
        let mut v = FileAttributeValuePriv::new();
        v.set_string("hello");
        assert_eq!(v.get_string(), "hello");
        assert_eq!(v.as_string(), "hello");
        assert_eq!(v.status, FileAttributeStatus::Set);
    }

    #[test]
    fn test_set_and_get_int32() {
        let mut v = FileAttributeValuePriv::new();
        v.set_int32(-42);
        assert_eq!(v.get_int32(), -42);
        assert_eq!(v.as_string(), "-42");
    }

    #[test]
    fn test_set_and_get_boolean() {
        let mut v = FileAttributeValuePriv::new();
        v.set_boolean(true);
        assert!(v.get_boolean());
        assert_eq!(v.as_string(), "true");
    }

    #[test]
    fn test_dup() {
        let mut v = FileAttributeValuePriv::new();
        v.set_uint64(123456);
        let d = v.dup();
        assert_eq!(d.get_uint64(), 123456);
    }

    #[test]
    fn test_clear() {
        let mut v = FileAttributeValuePriv::new();
        v.set_string("test");
        v.clear();
        assert_eq!(v.status, FileAttributeStatus::Unset);
    }

    #[test]
    fn test_stringv() {
        let mut v = FileAttributeValuePriv::new();
        v.set_stringv(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(v.get_stringv(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(v.as_string(), "a,b");
    }
}
