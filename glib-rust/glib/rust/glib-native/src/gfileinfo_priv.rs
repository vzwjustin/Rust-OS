//! `gfileinfo-priv` matching `gio/gfileinfo-priv.h`.
//!
//! Private file info API: attribute ID constants and set-by-id helpers.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gfileinfo::FileAttributeValue;
use crate::gfileinfo::FileInfo;
use crate::prelude::*;

// ── Attribute ID constants (mirrors `G_FILE_ATTRIBUTE_ID_*`) ──────────────

pub const G_FILE_ATTRIBUTE_ID_STANDARD_TYPE: u32 = 1048576 + 1;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_IS_HIDDEN: u32 = 1048576 + 2;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_IS_BACKUP: u32 = 1048576 + 3;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_IS_SYMLINK: u32 = 1048576 + 4;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_IS_VIRTUAL: u32 = 1048576 + 5;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_NAME: u32 = 1048576 + 6;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_DISPLAY_NAME: u32 = 1048576 + 7;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_EDIT_NAME: u32 = 1048576 + 8;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_COPY_NAME: u32 = 1048576 + 9;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_DESCRIPTION: u32 = 1048576 + 10;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_ICON: u32 = 1048576 + 11;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_CONTENT_TYPE: u32 = 1048576 + 12;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_FAST_CONTENT_TYPE: u32 = 1048576 + 13;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_SIZE: u32 = 1048576 + 14;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_ALLOCATED_SIZE: u32 = 1048576 + 15;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_SYMLINK_TARGET: u32 = 1048576 + 16;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_TARGET_URI: u32 = 1048576 + 17;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_SORT_ORDER: u32 = 1048576 + 18;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_SYMBOLIC_ICON: u32 = 1048576 + 19;
pub const G_FILE_ATTRIBUTE_ID_STANDARD_IS_VOLATILE: u32 = 1048576 + 20;
pub const G_FILE_ATTRIBUTE_ID_ETAG_VALUE: u32 = 2097152 + 1;
pub const G_FILE_ATTRIBUTE_ID_ID_FILE: u32 = 3145728 + 1;
pub const G_FILE_ATTRIBUTE_ID_ID_FILESYSTEM: u32 = 3145728 + 2;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_READ: u32 = 4194304 + 1;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_WRITE: u32 = 4194304 + 2;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_EXECUTE: u32 = 4194304 + 3;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_DELETE: u32 = 4194304 + 4;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_TRASH: u32 = 4194304 + 5;
pub const G_FILE_ATTRIBUTE_ID_ACCESS_CAN_RENAME: u32 = 4194304 + 6;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_MOUNT: u32 = 5242880 + 1;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_UNMOUNT: u32 = 5242880 + 2;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_EJECT: u32 = 5242880 + 3;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_UNIX_DEVICE: u32 = 5242880 + 4;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_UNIX_DEVICE_FILE: u32 = 5242880 + 5;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_HAL_UDI: u32 = 5242880 + 6;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_START: u32 = 5242880 + 7;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_START_DEGRADED: u32 = 5242880 + 8;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_STOP: u32 = 5242880 + 9;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_START_STOP_TYPE: u32 = 5242880 + 10;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_CAN_POLL: u32 = 5242880 + 11;
pub const G_FILE_ATTRIBUTE_ID_MOUNTABLE_IS_MEDIA_CHECK_AUTOMATIC: u32 = 5242880 + 12;
pub const G_FILE_ATTRIBUTE_ID_TIME_MODIFIED: u32 = 6291456 + 1;
pub const G_FILE_ATTRIBUTE_ID_TIME_MODIFIED_USEC: u32 = 6291456 + 2;
pub const G_FILE_ATTRIBUTE_ID_TIME_ACCESS: u32 = 6291456 + 3;
pub const G_FILE_ATTRIBUTE_ID_TIME_ACCESS_USEC: u32 = 6291456 + 4;
pub const G_FILE_ATTRIBUTE_ID_TIME_CHANGED: u32 = 6291456 + 5;
pub const G_FILE_ATTRIBUTE_ID_TIME_CHANGED_USEC: u32 = 6291456 + 6;
pub const G_FILE_ATTRIBUTE_ID_TIME_CREATED: u32 = 6291456 + 7;
pub const G_FILE_ATTRIBUTE_ID_TIME_CREATED_USEC: u32 = 6291456 + 8;
pub const G_FILE_ATTRIBUTE_ID_TIME_MODIFIED_NSEC: u32 = 6291456 + 9;
pub const G_FILE_ATTRIBUTE_ID_TIME_ACCESS_NSEC: u32 = 6291456 + 10;
pub const G_FILE_ATTRIBUTE_ID_TIME_CREATED_NSEC: u32 = 6291456 + 11;
pub const G_FILE_ATTRIBUTE_ID_TIME_CHANGED_NSEC: u32 = 6291456 + 12;
pub const G_FILE_ATTRIBUTE_ID_UNIX_DEVICE: u32 = 7340032 + 1;
pub const G_FILE_ATTRIBUTE_ID_UNIX_INODE: u32 = 7340032 + 2;
pub const G_FILE_ATTRIBUTE_ID_UNIX_MODE: u32 = 7340032 + 3;
pub const G_FILE_ATTRIBUTE_ID_UNIX_NLINK: u32 = 7340032 + 4;
pub const G_FILE_ATTRIBUTE_ID_UNIX_UID: u32 = 7340032 + 5;
pub const G_FILE_ATTRIBUTE_ID_UNIX_GID: u32 = 7340032 + 6;
pub const G_FILE_ATTRIBUTE_ID_UNIX_RDEV: u32 = 7340032 + 7;
pub const G_FILE_ATTRIBUTE_ID_UNIX_BLOCK_SIZE: u32 = 7340032 + 8;
pub const G_FILE_ATTRIBUTE_ID_UNIX_BLOCKS: u32 = 7340032 + 9;
pub const G_FILE_ATTRIBUTE_ID_UNIX_IS_MOUNTPOINT: u32 = 7340032 + 10;
pub const G_FILE_ATTRIBUTE_ID_DOS_IS_ARCHIVE: u32 = 8388608 + 1;
pub const G_FILE_ATTRIBUTE_ID_DOS_IS_SYSTEM: u32 = 8388608 + 2;
pub const G_FILE_ATTRIBUTE_ID_DOS_IS_MOUNTPOINT: u32 = 8388608 + 3;
pub const G_FILE_ATTRIBUTE_ID_DOS_REPARSE_POINT_TAG: u32 = 8388608 + 4;
pub const G_FILE_ATTRIBUTE_ID_OWNER_USER: u32 = 9437184 + 1;
pub const G_FILE_ATTRIBUTE_ID_OWNER_USER_REAL: u32 = 9437184 + 2;
pub const G_FILE_ATTRIBUTE_ID_OWNER_GROUP: u32 = 9437184 + 3;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_PATH: u32 = 10485760 + 1;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAILING_FAILED: u32 = 10485760 + 2;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_IS_VALID: u32 = 10485760 + 3;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_PATH_NORMAL: u32 = 10485760 + 4;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAILING_FAILED_NORMAL: u32 = 10485760 + 5;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_IS_VALID_NORMAL: u32 = 10485760 + 6;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_PATH_LARGE: u32 = 10485760 + 7;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAILING_FAILED_LARGE: u32 = 10485760 + 8;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_IS_VALID_LARGE: u32 = 10485760 + 9;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_PATH_XLARGE: u32 = 10485760 + 10;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAILING_FAILED_XLARGE: u32 = 10485760 + 11;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_IS_VALID_XLARGE: u32 = 10485760 + 12;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_PATH_XXLARGE: u32 = 10485760 + 13;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAILING_FAILED_XXLARGE: u32 = 10485760 + 14;
pub const G_FILE_ATTRIBUTE_ID_THUMBNAIL_IS_VALID_XXLARGE: u32 = 10485760 + 15;
pub const G_FILE_ATTRIBUTE_ID_PREVIEW_ICON: u32 = 11534336 + 1;
pub const G_FILE_ATTRIBUTE_ID_FILESYSTEM_SIZE: u32 = 12582912 + 1;
pub const G_FILE_ATTRIBUTE_ID_FILESYSTEM_FREE: u32 = 12582912 + 2;
pub const G_FILE_ATTRIBUTE_ID_FILESYSTEM_TYPE: u32 = 12582912 + 3;
pub const G_FILE_ATTRIBUTE_ID_FILESYSTEM_READONLY: u32 = 12582912 + 4;
pub const G_FILE_ATTRIBUTE_ID_FILESYSTEM_USE_PREVIEW: u32 = 12582912 + 5;
pub const G_FILE_ATTRIBUTE_ID_GVFS_BACKEND: u32 = 13631488 + 1;
pub const G_FILE_ATTRIBUTE_ID_SELINUX_CONTEXT: u32 = 14680064 + 1;
pub const G_FILE_ATTRIBUTE_ID_TRASH_ITEM_COUNT: u32 = 15728640 + 1;
pub const G_FILE_ATTRIBUTE_ID_TRASH_ORIG_PATH: u32 = 15728640 + 2;
pub const G_FILE_ATTRIBUTE_ID_TRASH_DELETION_DATE: u32 = 15728640 + 3;

/// Maps an attribute ID to its string name (inverse of the ID constants).
pub fn attribute_id_to_name(id: u32) -> Option<&'static str> {
    match id {
        G_FILE_ATTRIBUTE_ID_STANDARD_TYPE => Some("standard::type"),
        G_FILE_ATTRIBUTE_ID_STANDARD_IS_HIDDEN => Some("standard::is-hidden"),
        G_FILE_ATTRIBUTE_ID_STANDARD_IS_BACKUP => Some("standard::is-backup"),
        G_FILE_ATTRIBUTE_ID_STANDARD_IS_SYMLINK => Some("standard::is-symlink"),
        G_FILE_ATTRIBUTE_ID_STANDARD_NAME => Some("standard::name"),
        G_FILE_ATTRIBUTE_ID_STANDARD_DISPLAY_NAME => Some("standard::display-name"),
        G_FILE_ATTRIBUTE_ID_STANDARD_CONTENT_TYPE => Some("standard::content-type"),
        G_FILE_ATTRIBUTE_ID_STANDARD_SIZE => Some("standard::size"),
        G_FILE_ATTRIBUTE_ID_STANDARD_SYMLINK_TARGET => Some("standard::symlink-target"),
        G_FILE_ATTRIBUTE_ID_STANDARD_SORT_ORDER => Some("standard::sort-order"),
        G_FILE_ATTRIBUTE_ID_ETAG_VALUE => Some("etag::value"),
        G_FILE_ATTRIBUTE_ID_TIME_MODIFIED => Some("time::modified"),
        G_FILE_ATTRIBUTE_ID_TIME_ACCESS => Some("time::access"),
        G_FILE_ATTRIBUTE_ID_TIME_CHANGED => Some("time::changed"),
        G_FILE_ATTRIBUTE_ID_TIME_CREATED => Some("time::created"),
        G_FILE_ATTRIBUTE_ID_UNIX_DEVICE => Some("unix::device"),
        G_FILE_ATTRIBUTE_ID_UNIX_INODE => Some("unix::inode"),
        G_FILE_ATTRIBUTE_ID_UNIX_MODE => Some("unix::mode"),
        G_FILE_ATTRIBUTE_ID_UNIX_UID => Some("unix::uid"),
        G_FILE_ATTRIBUTE_ID_UNIX_GID => Some("unix::gid"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_READ => Some("access::can-read"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_WRITE => Some("access::can-write"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_EXECUTE => Some("access::can-execute"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_DELETE => Some("access::can-delete"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_TRASH => Some("access::can-trash"),
        G_FILE_ATTRIBUTE_ID_ACCESS_CAN_RENAME => Some("access::can-rename"),
        G_FILE_ATTRIBUTE_ID_OWNER_USER => Some("owner::user"),
        G_FILE_ATTRIBUTE_ID_OWNER_GROUP => Some("owner::group"),
        _ => None,
    }
}

/// Sets a string attribute by ID (mirrors `_g_file_info_set_attribute_string_by_id`).
pub fn set_attribute_string_by_id(info: &FileInfo, id: u32, value: &str) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::String(value.to_string()));
    }
}

/// Sets a boolean attribute by ID (mirrors `_g_file_info_set_attribute_boolean_by_id`).
pub fn set_attribute_boolean_by_id(info: &FileInfo, id: u32, value: bool) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::Boolean(value));
    }
}

/// Sets a uint32 attribute by ID (mirrors `_g_file_info_set_attribute_uint32_by_id`).
pub fn set_attribute_uint32_by_id(info: &FileInfo, id: u32, value: u32) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::Uint32(value));
    }
}

/// Sets an int32 attribute by ID (mirrors `_g_file_info_set_attribute_int32_by_id`).
pub fn set_attribute_int32_by_id(info: &FileInfo, id: u32, value: i32) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::Int32(value));
    }
}

/// Sets a uint64 attribute by ID (mirrors `_g_file_info_set_attribute_uint64_by_id`).
pub fn set_attribute_uint64_by_id(info: &FileInfo, id: u32, value: u64) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::Uint64(value));
    }
}

/// Sets an int64 attribute by ID (mirrors `_g_file_info_set_attribute_int64_by_id`).
pub fn set_attribute_int64_by_id(info: &FileInfo, id: u32, value: i64) {
    if let Some(name) = attribute_id_to_name(id) {
        info.set_attribute(name, FileAttributeValue::Int64(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_constants() {
        assert_eq!(G_FILE_ATTRIBUTE_ID_STANDARD_TYPE, 1048577);
        assert_eq!(G_FILE_ATTRIBUTE_ID_STANDARD_SIZE, 1048590);
        assert_eq!(G_FILE_ATTRIBUTE_ID_TIME_MODIFIED, 6291457);
        assert_eq!(G_FILE_ATTRIBUTE_ID_UNIX_MODE, 7340035);
    }

    #[test]
    fn test_id_to_name() {
        assert_eq!(
            attribute_id_to_name(G_FILE_ATTRIBUTE_ID_STANDARD_TYPE),
            Some("standard::type")
        );
        assert_eq!(
            attribute_id_to_name(G_FILE_ATTRIBUTE_ID_STANDARD_SIZE),
            Some("standard::size")
        );
        assert_eq!(attribute_id_to_name(999999), None);
    }

    #[test]
    fn test_set_string_by_id() {
        let info = FileInfo::new();
        set_attribute_string_by_id(&info, G_FILE_ATTRIBUTE_ID_STANDARD_DISPLAY_NAME, "test.txt");
        assert_eq!(
            info.get_attribute_string("standard::display-name"),
            Some("test.txt".to_string())
        );
    }

    #[test]
    fn test_set_boolean_by_id() {
        let info = FileInfo::new();
        set_attribute_boolean_by_id(&info, G_FILE_ATTRIBUTE_ID_STANDARD_IS_HIDDEN, true);
        assert!(info.get_attribute_boolean("standard::is-hidden"));
    }

    #[test]
    fn test_set_uint32_by_id() {
        let info = FileInfo::new();
        set_attribute_uint32_by_id(&info, G_FILE_ATTRIBUTE_ID_STANDARD_SIZE, 4096);
        assert_eq!(info.get_attribute_uint32("standard::size"), 4096);
    }
}
