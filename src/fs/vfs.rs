//! Virtual File System utilities and helpers
//!
//! This module provides additional VFS utilities and helper functions
//! for path manipulation, file operations, and filesystem management.

use super::{DirectoryEntry, FilePermissions, FileType, FsError, FsResult};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

/// Path manipulation utilities
pub struct PathUtils;

impl PathUtils {
    /// Normalize a path by resolving . and .. components
    pub fn normalize(path: &str) -> String {
        if path.is_empty() {
            return "/".to_string();
        }

        let mut components = Vec::new();
        let is_absolute = path.starts_with('/');

        for component in path.split('/').filter(|c| !c.is_empty()) {
            match component {
                "." => continue,
                ".." => {
                    if !components.is_empty() && components.last() != Some(&"..".to_string()) {
                        components.pop();
                    } else if !is_absolute {
                        components.push("..".to_string());
                    }
                }
                _ => components.push(component.to_string()),
            }
        }

        if is_absolute {
            if components.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", components.join("/"))
            }
        } else if components.is_empty() {
            ".".to_string()
        } else {
            components.join("/")
        }
    }

    /// Join two paths.
    pub fn join(base: &str, path: &str) -> String {
        crate::glib::build_filename(&[base, path])
    }

    /// Get the parent directory of a path
    pub fn parent(path: &str) -> Option<String> {
        if path == "/" {
            return None;
        }

        let normalized = Self::normalize(path);
        if let Some(pos) = normalized.rfind('/') {
            if pos == 0 {
                Some("/".to_string())
            } else {
                Some(normalized[..pos].to_string())
            }
        } else {
            Some(".".to_string())
        }
    }

    /// Get the filename component of a path
    pub fn filename(path: &str) -> Option<String> {
        let normalized = Self::normalize(path);
        if normalized == "/" {
            return None;
        }

        if let Some(pos) = normalized.rfind('/') {
            Some(normalized[pos + 1..].to_string())
        } else {
            Some(normalized)
        }
    }

    /// Check if a path is absolute
    pub fn is_absolute(path: &str) -> bool {
        path.starts_with('/')
    }

    /// Check if a path is relative
    pub fn is_relative(path: &str) -> bool {
        !Self::is_absolute(path)
    }

    /// Get the extension of a filename
    pub fn extension(path: &str) -> Option<String> {
        if let Some(filename) = Self::filename(path) {
            if let Some(pos) = filename.rfind('.') {
                if pos > 0 && pos < filename.len() - 1 {
                    return Some(filename[pos + 1..].to_string());
                }
            }
        }
        None
    }

    /// Remove the extension from a filename
    pub fn stem(path: &str) -> Option<String> {
        if let Some(filename) = Self::filename(path) {
            if let Some(pos) = filename.rfind('.') {
                if pos > 0 {
                    return Some(filename[..pos].to_string());
                }
            }
            Some(filename)
        } else {
            None
        }
    }
}

/// File type detection utilities
pub struct FileTypeUtils;

impl FileTypeUtils {
    /// Detect file type from filename extension
    pub fn from_extension(extension: &str) -> FileType {
        match crate::glib::ascii_strdown(extension).as_str() {
            // Always return Regular for files with extensions
            // In a real implementation, this might distinguish between
            // different types of files, but for now we keep it simple
            _ => FileType::Regular,
        }
    }

    /// Check if a file type is executable
    pub fn is_executable(file_type: FileType) -> bool {
        match file_type {
            FileType::Regular => true, // Could be executable
            _ => false,
        }
    }

    /// Get a human-readable description of a file type
    pub fn description(file_type: FileType) -> &'static str {
        match file_type {
            FileType::Regular => "regular file",
            FileType::Directory => "directory",
            FileType::SymbolicLink => "symbolic link",
            FileType::CharacterDevice => "character device",
            FileType::BlockDevice => "block device",
            FileType::NamedPipe => "named pipe",
            FileType::Socket => "socket",
        }
    }
}

/// Permission utilities
pub struct PermissionUtils;

impl PermissionUtils {
    /// Check if permissions allow read access for owner
    pub fn can_read_owner(permissions: &FilePermissions) -> bool {
        permissions.owner_read
    }

    /// Check if permissions allow write access for owner
    pub fn can_write_owner(permissions: &FilePermissions) -> bool {
        permissions.owner_write
    }

    /// Check if permissions allow execute access for owner
    pub fn can_execute_owner(permissions: &FilePermissions) -> bool {
        permissions.owner_execute
    }

    /// Convert permissions to a Unix-style string (e.g., "rwxr-xr-x")
    pub fn to_string(permissions: &FilePermissions) -> String {
        let mut result = String::with_capacity(9);

        // Owner permissions
        result.push(if permissions.owner_read { 'r' } else { '-' });
        result.push(if permissions.owner_write { 'w' } else { '-' });
        result.push(if permissions.owner_execute { 'x' } else { '-' });

        // Group permissions
        result.push(if permissions.group_read { 'r' } else { '-' });
        result.push(if permissions.group_write { 'w' } else { '-' });
        result.push(if permissions.group_execute { 'x' } else { '-' });

        // Other permissions
        result.push(if permissions.other_read { 'r' } else { '-' });
        result.push(if permissions.other_write { 'w' } else { '-' });
        result.push(if permissions.other_execute { 'x' } else { '-' });

        result
    }

    /// Parse permissions from a Unix-style string
    pub fn from_string(perm_str: &str) -> FsResult<FilePermissions> {
        if perm_str.len() != 9 {
            return Err(FsError::InvalidArgument);
        }

        let chars: Vec<char> = perm_str.chars().collect();

        Ok(FilePermissions {
            owner_read: chars[0] == 'r',
            owner_write: chars[1] == 'w',
            owner_execute: chars[2] == 'x',
            group_read: chars[3] == 'r',
            group_write: chars[4] == 'w',
            group_execute: chars[5] == 'x',
            other_read: chars[6] == 'r',
            other_write: chars[7] == 'w',
            other_execute: chars[8] == 'x',
        })
    }
}

/// Directory listing utilities
pub struct DirectoryUtils;

impl DirectoryUtils {
    /// Sort directory entries by name
    pub fn sort_by_name(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Sort directory entries by type (directories first)
    pub fn sort_by_type(entries: &mut Vec<DirectoryEntry>) {
        entries.sort_by(|a, b| match (a.file_type, b.file_type) {
            (FileType::Directory, FileType::Directory) => a.name.cmp(&b.name),
            (FileType::Directory, _) => core::cmp::Ordering::Less,
            (_, FileType::Directory) => core::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
    }

    /// Filter directory entries by file type
    pub fn filter_by_type(entries: &[DirectoryEntry], file_type: FileType) -> Vec<DirectoryEntry> {
        entries
            .iter()
            .filter(|entry| entry.file_type == file_type)
            .cloned()
            .collect()
    }

    /// Filter directory entries by name pattern (simple glob matching)
    pub fn filter_by_pattern(entries: &[DirectoryEntry], pattern: &str) -> Vec<DirectoryEntry> {
        if pattern == "*" {
            return entries.to_vec();
        }

        entries
            .iter()
            .filter(|entry| Self::matches_pattern(&entry.name, pattern))
            .cloned()
            .collect()
    }

    /// Simple glob pattern matching via glib-native.
    fn matches_pattern(name: &str, pattern: &str) -> bool {
        crate::glib::pattern_match_simple(pattern, name)
    }
}

/// File size formatting utilities
pub struct SizeUtils;

impl SizeUtils {
    /// Format file size in human-readable format
    pub fn format_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        const THRESHOLD: u64 = 1024;

        if size == 0 {
            return "0 B".to_string();
        }

        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= THRESHOLD as f64 && unit_index < UNITS.len() - 1 {
            size /= THRESHOLD as f64;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    /// Parse human-readable size to bytes
    pub fn parse_size(size_str: &str) -> FsResult<u64> {
        let size_str = crate::glib::ascii_strup(size_str.trim());

        if size_str.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        // Extract number and unit
        let (number_part, unit_part) = if size_str.ends_with('B') {
            if size_str.len() < 2 {
                (size_str.as_str(), "")
            } else {
                let without_b = &size_str[..size_str.len() - 1];
                if without_b.ends_with('K')
                    || without_b.ends_with('M')
                    || without_b.ends_with('G')
                    || without_b.ends_with('T')
                {
                    (
                        &without_b[..without_b.len() - 1],
                        &without_b[without_b.len() - 1..],
                    )
                } else {
                    (without_b, "")
                }
            }
        } else if size_str.ends_with('K')
            || size_str.ends_with('M')
            || size_str.ends_with('G')
            || size_str.ends_with('T')
        {
            (
                &size_str[..size_str.len() - 1],
                &size_str[size_str.len() - 1..],
            )
        } else {
            (size_str.as_str(), "")
        };

        let number: f64 = number_part.parse().map_err(|_| FsError::InvalidArgument)?;

        let multiplier = match unit_part {
            "" | "B" => 1,
            "K" => 1024,
            "M" => 1024 * 1024,
            "G" => 1024 * 1024 * 1024,
            "T" => 1024u64 * 1024 * 1024 * 1024,
            _ => return Err(FsError::InvalidArgument),
        };

        Ok((number * multiplier as f64) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_path_normalize() {
        assert_eq!(PathUtils::normalize("/"), "/");
        assert_eq!(PathUtils::normalize("/foo/bar"), "/foo/bar");
        assert_eq!(PathUtils::normalize("/foo/../bar"), "/bar");
        assert_eq!(PathUtils::normalize("/foo/./bar"), "/foo/bar");
        assert_eq!(PathUtils::normalize("foo/bar"), "foo/bar");
        assert_eq!(PathUtils::normalize("foo/../bar"), "bar");
        assert_eq!(PathUtils::normalize("../foo"), "../foo");
    }

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_path_join() {
        assert_eq!(PathUtils::join("/foo", "bar"), "/foo/bar");
        assert_eq!(PathUtils::join("/foo/", "bar"), "/foo/bar");
        assert_eq!(PathUtils::join("/foo", "/bar"), "/bar");
    }

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_size_format() {
        assert_eq!(SizeUtils::format_size(0), "0 B");
        assert_eq!(SizeUtils::format_size(512), "512 B");
        assert_eq!(SizeUtils::format_size(1024), "1.0 KB");
        assert_eq!(SizeUtils::format_size(1536), "1.5 KB");
        assert_eq!(SizeUtils::format_size(1048576), "1.0 MB");
    }

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_size_parse() {
        assert_eq!(SizeUtils::parse_size("512").unwrap(), 512);
        assert_eq!(SizeUtils::parse_size("1K").unwrap(), 1024);
        assert_eq!(SizeUtils::parse_size("1KB").unwrap(), 1024);
        assert_eq!(SizeUtils::parse_size("1M").unwrap(), 1048576);
        assert_eq!(SizeUtils::parse_size("1.5K").unwrap(), 1536);
    }
}
