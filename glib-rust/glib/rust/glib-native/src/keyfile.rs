//! Key file (INI/desktop entry) parser matching `gkeyfile.h` / `gkeyfile.c`.
//!
//! Supports loading from data strings, querying and setting values by group/key,
//! and serializing back to text. File I/O variants (`load_from_file`,
//! `save_to_file`) are omitted; use `load_from_data` / `to_data` with a
//! platform file abstraction.

use crate::prelude::*;
use crate::quark::{quark_from_static_string, Quark};
use alloc::collections::BTreeMap;

/// Key file error codes (`GKeyFileError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum KeyFileError {
    /// Unknown encoding.
    UnknownEncoding = 0,
    /// Parse error.
    Parse,
    /// Key not found.
    KeyNotFound,
    /// Group not found.
    GroupNotFound,
    /// Invalid value.
    InvalidValue,
}

/// Returns the quark for `G_KEY_FILE_ERROR`.
pub fn key_file_error_quark() -> Quark {
    quark_from_static_string(Some("g-key-file-error-quark"))
}

/// Flags for loading key files (`GKeyFileFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyFileFlags(u32);

impl KeyFileFlags {
    /// No flags.
    pub const NONE: Self = Self(0);
    /// Keep comments.
    pub const KEEP_COMMENTS: Self = Self(1 << 0);
    /// Keep translations.
    pub const KEEP_TRANSLATIONS: Self = Self(1 << 1);
}

impl core::ops::BitOr for KeyFileFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for KeyFileFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Key file parser (`GKeyFile`).
///
/// Stores groups as a `BTreeMap` of group name → (`BTreeMap` of key → value).
#[derive(Clone, Debug)]
pub struct KeyFile {
    groups: BTreeMap<String, BTreeMap<String, String>>,
    start_group: Option<String>,
    list_separator: char,
}

impl KeyFile {
    /// Create a new empty key file (`g_key_file_new`).
    pub fn new() -> Self {
        Self {
            groups: BTreeMap::new(),
            start_group: None,
            list_separator: ';',
        }
    }

    /// Set the list separator character (`g_key_file_set_list_separator`).
    pub fn set_list_separator(&mut self, separator: char) {
        self.list_separator = separator;
    }

    /// Load key file from a data string (`g_key_file_load_from_data`).
    pub fn load_from_data(&mut self, data: &str, _flags: KeyFileFlags) -> Result<(), KeyFileError> {
        let mut current_group: Option<String> = None;

        for line in data.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            // Comment line
            if trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            // Group header [group]
            if trimmed.starts_with('[') {
                if let Some(end) = trimmed.find(']') {
                    let group = trimmed[1..end].trim().to_owned();
                    if group.is_empty() {
                        return Err(KeyFileError::Parse);
                    }
                    if !self.groups.contains_key(&group) {
                        self.groups.insert(group.clone(), BTreeMap::new());
                    }
                    if self.start_group.is_none() {
                        self.start_group = Some(group.clone());
                    }
                    current_group = Some(group);
                } else {
                    return Err(KeyFileError::Parse);
                }
                continue;
            }

            // Key=value
            if let Some(eq) = trimmed.find('=') {
                let key = trimmed[..eq].trim().to_owned();
                let value = trimmed[eq + 1..].trim().to_owned();

                if key.is_empty() {
                    return Err(KeyFileError::Parse);
                }

                let group = current_group.as_ref().ok_or(KeyFileError::Parse)?;
                self.groups
                    .entry(group.clone())
                    .or_default()
                    .insert(key, value);
            } else {
                return Err(KeyFileError::Parse);
            }
        }

        Ok(())
    }

    /// Serialize the key file to a string (`g_key_file_to_data`).
    pub fn to_data(&self) -> String {
        let mut result = String::new();
        for (group_name, keys) in &self.groups {
            result.push('[');
            result.push_str(group_name);
            result.push_str("]\n");
            for (key, value) in keys {
                result.push_str(key);
                result.push('=');
                result.push_str(value);
                result.push('\n');
            }
            result.push('\n');
        }
        result
    }

    /// Returns the start group name (`g_key_file_get_start_group`).
    pub fn start_group(&self) -> Option<&str> {
        self.start_group.as_deref()
    }

    /// Returns all group names (`g_key_file_get_groups`).
    pub fn groups(&self) -> Vec<String> {
        self.groups.keys().cloned().collect()
    }

    /// Returns `true` if `group_name` exists (`g_key_file_has_group`).
    pub fn has_group(&self, group_name: &str) -> bool {
        self.groups.contains_key(group_name)
    }

    /// Returns all keys in `group_name` (`g_key_file_get_keys`).
    pub fn keys(&self, group_name: &str) -> Result<Vec<String>, KeyFileError> {
        let group = self
            .groups
            .get(group_name)
            .ok_or(KeyFileError::GroupNotFound)?;
        Ok(group.keys().cloned().collect())
    }

    /// Returns `true` if `key` exists in `group_name` (`g_key_file_has_key`).
    pub fn has_key(&self, group_name: &str, key: &str) -> Result<bool, KeyFileError> {
        let group = self
            .groups
            .get(group_name)
            .ok_or(KeyFileError::GroupNotFound)?;
        Ok(group.contains_key(key))
    }

    /// Get the raw value of `key` in `group_name` (`g_key_file_get_value`).
    pub fn get_value(&self, group_name: &str, key: &str) -> Result<String, KeyFileError> {
        let group = self
            .groups
            .get(group_name)
            .ok_or(KeyFileError::GroupNotFound)?;
        group.get(key).ok_or(KeyFileError::KeyNotFound).cloned()
    }

    /// Set the raw value of `key` in `group_name` (`g_key_file_set_value`).
    pub fn set_value(&mut self, group_name: &str, key: &str, value: &str) {
        self.groups
            .entry(group_name.to_owned())
            .or_default()
            .insert(key.to_owned(), value.to_owned());
    }

    /// Get the string value of `key` in `group_name` (`g_key_file_get_string`).
    pub fn get_string(&self, group_name: &str, key: &str) -> Result<String, KeyFileError> {
        self.get_value(group_name, key)
    }

    /// Set the string value of `key` in `group_name` (`g_key_file_set_string`).
    pub fn set_string(&mut self, group_name: &str, key: &str, value: &str) {
        self.set_value(group_name, key, value);
    }

    /// Get the boolean value of `key` in `group_name` (`g_key_file_get_boolean`).
    pub fn get_boolean(&self, group_name: &str, key: &str) -> Result<bool, KeyFileError> {
        let value = self.get_value(group_name, key)?;
        match value.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(true),
            "false" | "no" | "0" => Ok(false),
            _ => Err(KeyFileError::InvalidValue),
        }
    }

    /// Set the boolean value of `key` in `group_name` (`g_key_file_set_boolean`).
    pub fn set_boolean(&mut self, group_name: &str, key: &str, value: bool) {
        self.set_value(group_name, key, if value { "true" } else { "false" });
    }

    /// Get the integer value of `key` in `group_name` (`g_key_file_get_integer`).
    pub fn get_integer(&self, group_name: &str, key: &str) -> Result<i64, KeyFileError> {
        let value = self.get_value(group_name, key)?;
        value.parse::<i64>().map_err(|_| KeyFileError::InvalidValue)
    }

    /// Set the integer value of `key` in `group_name` (`g_key_file_set_integer`).
    pub fn set_integer(&mut self, group_name: &str, key: &str, value: i64) {
        self.set_value(group_name, key, &value.to_string());
    }

    /// Get the double value of `key` in `group_name` (`g_key_file_get_double`).
    pub fn get_double(&self, group_name: &str, key: &str) -> Result<f64, KeyFileError> {
        let value = self.get_value(group_name, key)?;
        value.parse::<f64>().map_err(|_| KeyFileError::InvalidValue)
    }

    /// Set the double value of `key` in `group_name` (`g_key_file_set_double`).
    pub fn set_double(&mut self, group_name: &str, key: &str, value: f64) {
        self.set_value(group_name, key, &value.to_string());
    }

    /// Get a list of string values for `key` in `group_name` (`g_key_file_get_string_list`).
    pub fn get_string_list(
        &self,
        group_name: &str,
        key: &str,
    ) -> Result<Vec<String>, KeyFileError> {
        let value = self.get_value(group_name, key)?;
        Ok(value
            .split(self.list_separator)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Set a list of string values for `key` in `group_name` (`g_key_file_set_string_list`).
    pub fn set_string_list(&mut self, group_name: &str, key: &str, list: &[&str]) {
        let value = list.join(self.list_separator.to_string().as_str());
        self.set_value(group_name, key, &value);
    }

    /// Remove `group_name` from the key file (`g_key_file_remove_group`).
    pub fn remove_group(&mut self, group_name: &str) -> Result<(), KeyFileError> {
        if self.groups.remove(group_name).is_none() {
            return Err(KeyFileError::GroupNotFound);
        }
        if self.start_group.as_deref() == Some(group_name) {
            self.start_group = self.groups.keys().next().cloned();
        }
        Ok(())
    }

    /// Remove `key` from `group_name` (`g_key_file_remove_key`).
    pub fn remove_key(&mut self, group_name: &str, key: &str) -> Result<(), KeyFileError> {
        let group = self
            .groups
            .get_mut(group_name)
            .ok_or(KeyFileError::GroupNotFound)?;
        if group.remove(key).is_none() {
            return Err(KeyFileError::KeyNotFound);
        }
        Ok(())
    }
}

impl Default for KeyFile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let data = "[Group1]\nkey1=value1\nkey2=value2\n\n[Group2]\nfoo=bar\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();

        assert_eq!(kf.start_group(), Some("Group1"));
        assert!(kf.has_group("Group1"));
        assert!(kf.has_group("Group2"));
        assert!(!kf.has_group("Group3"));

        assert_eq!(kf.get_string("Group1", "key1").unwrap(), "value1");
        assert_eq!(kf.get_string("Group1", "key2").unwrap(), "value2");
        assert_eq!(kf.get_string("Group2", "foo").unwrap(), "bar");
    }

    #[test]
    fn parse_with_comments() {
        let data = "# Comment\n[Group]\n; Another comment\nkey=val\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();
        assert_eq!(kf.get_string("Group", "key").unwrap(), "val");
    }

    #[test]
    fn get_boolean() {
        let data = "[Flags]\nenabled=true\ndisabled=false\nyes=yes\nno=no\none=1\nzero=0\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();

        assert!(kf.get_boolean("Flags", "enabled").unwrap());
        assert!(!kf.get_boolean("Flags", "disabled").unwrap());
        assert!(kf.get_boolean("Flags", "yes").unwrap());
        assert!(!kf.get_boolean("Flags", "no").unwrap());
        assert!(kf.get_boolean("Flags", "one").unwrap());
        assert!(!kf.get_boolean("Flags", "zero").unwrap());
    }

    #[test]
    fn get_integer() {
        let data = "[Numbers]\npositive=42\nnegative=-17\nzero=0\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();

        assert_eq!(kf.get_integer("Numbers", "positive").unwrap(), 42);
        assert_eq!(kf.get_integer("Numbers", "negative").unwrap(), -17);
        assert_eq!(kf.get_integer("Numbers", "zero").unwrap(), 0);
    }

    #[test]
    fn set_and_serialize() {
        let mut kf = KeyFile::new();
        kf.set_string("Group1", "key1", "value1");
        kf.set_integer("Group1", "count", 42);
        kf.set_boolean("Group1", "flag", true);

        let data = kf.to_data();
        assert!(data.contains("[Group1]"));
        assert!(data.contains("key1=value1"));
        assert!(data.contains("count=42"));
        assert!(data.contains("flag=true"));
    }

    #[test]
    fn string_list() {
        let mut kf = KeyFile::new();
        kf.set_string_list("G", "items", &["a", "b", "c"]);
        let list = kf.get_string_list("G", "items").unwrap();
        assert_eq!(list, vec!["a", "b", "c"]);
    }

    #[test]
    fn remove_group_and_key() {
        let data = "[G1]\nk1=v1\nk2=v2\n[G2]\nk3=v3\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();

        kf.remove_key("G1", "k1").unwrap();
        assert!(!kf.has_key("G1", "k1").unwrap());
        assert!(kf.has_key("G1", "k2").unwrap());

        kf.remove_group("G2").unwrap();
        assert!(!kf.has_group("G2"));
    }

    #[test]
    fn errors() {
        let mut kf = KeyFile::new();
        kf.load_from_data("[G]\nk=v\n", KeyFileFlags::NONE).unwrap();

        assert_eq!(
            kf.get_string("Nonexistent", "key"),
            Err(KeyFileError::GroupNotFound)
        );
        assert_eq!(
            kf.get_string("G", "nonexistent"),
            Err(KeyFileError::KeyNotFound)
        );
        assert_eq!(kf.get_integer("G", "k"), Err(KeyFileError::InvalidValue));
    }

    #[test]
    fn roundtrip() {
        let data = "[Section]\nname=test\nvalue=123\n";
        let mut kf = KeyFile::new();
        kf.load_from_data(data, KeyFileFlags::NONE).unwrap();
        let out = kf.to_data();
        let mut kf2 = KeyFile::new();
        kf2.load_from_data(&out, KeyFileFlags::NONE).unwrap();
        assert_eq!(
            kf.get_string("Section", "name"),
            kf2.get_string("Section", "name")
        );
        assert_eq!(
            kf.get_string("Section", "value"),
            kf2.get_string("Section", "value")
        );
    }

    #[test]
    fn key_file_error_quark_is_nonzero() {
        assert_ne!(key_file_error_quark(), 0);
    }
}
