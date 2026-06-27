//! GIoEnums matching `gio/gioenums.h`.
//! GIO enumeration types. In this no_std port we re-export key enums
//! and define additional ones not covered elsewhere.
//! Fully `no_std` compatible.

/// File monitor event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMonitorEvent {
    Changed,
    ChangedDone,
    Deleted,
    Created,
    AttributeChanged,
    PreUnmount,
    Unmounted,
    Moved,
}

/// File query info flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileQueryInfoFlags(pub u32);

impl FileQueryInfoFlags {
    pub const NONE: Self = Self(0);
    pub const NOFOLLOW_SYMLINKS: Self = Self(1);
}

/// File copy flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileCopyFlags(pub u32);

impl FileCopyFlags {
    pub const NONE: Self = Self(0);
    pub const OVERWRITE: Self = Self(1);
    pub const NOFOLLOW_SYMLINKS: Self = Self(1 << 1);
    pub const ALL_METADATA: Self = Self(1 << 2);
}

/// Mount mount/unmount flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountUnmountFlags(pub u32);

impl MountUnmountFlags {
    pub const NONE: Self = Self(0);
    pub const FORCE: Self = Self(1);
}

/// AppInfo create flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppInfoCreateFlags(pub u32);

impl AppInfoCreateFlags {
    pub const NONE: Self = Self(0);
    pub const NEEDS_TERMINAL: Self = Self(1);
    pub const SUPPORTS_URIS: Self = Self(1 << 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_monitor_event() {
        let e = FileMonitorEvent::Created;
        assert_eq!(e, FileMonitorEvent::Created);
    }

    #[test]
    fn test_flags() {
        let f = FileCopyFlags::OVERWRITE;
        assert_eq!(f.0, 1);
        let f2 = FileCopyFlags::NOFOLLOW_SYMLINKS;
        assert_eq!(f2.0, 2);
    }
}
