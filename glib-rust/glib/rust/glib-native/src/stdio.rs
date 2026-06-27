//! Stdio wrappers matching `gstdio.h` / `gstdio.c`.
//!
//! On Unix, gstdio functions are just aliases for the C library functions.
//! In no_std, we define types and a platform trait for file operations.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// File access modes for `g_access`.
pub const F_OK: i32 = 0;
pub const R_OK: i32 = 4;
pub const W_OK: i32 = 2;
pub const X_OK: i32 = 1;

/// File open flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct OpenFlags(pub i32);

impl OpenFlags {
    pub const O_RDONLY: OpenFlags = OpenFlags(0);
    pub const O_WRONLY: OpenFlags = OpenFlags(1);
    pub const O_RDWR: OpenFlags = OpenFlags(2);
    pub const O_CREAT: OpenFlags = OpenFlags(0o100);
    pub const O_EXCL: OpenFlags = OpenFlags(0o200);
    pub const O_TRUNC: OpenFlags = OpenFlags(0o1000);
    pub const O_APPEND: OpenFlags = OpenFlags(0o2000);
    pub const O_NONBLOCK: OpenFlags = OpenFlags(0o4000);
}

/// File mode bits.
pub const S_IRWXU: u32 = 0o0700;
pub const S_IRUSR: u32 = 0o0400;
pub const S_IWUSR: u32 = 0o0200;
pub const S_IXUSR: u32 = 0o0100;
pub const S_IRWXG: u32 = 0o0070;
pub const S_IRGRP: u32 = 0o0040;
pub const S_IWGRP: u32 = 0o0020;
pub const S_IXGRP: u32 = 0o0010;
pub const S_IRWXO: u32 = 0o0007;
pub const S_IROTH: u32 = 0o0004;
pub const S_IWOTH: u32 = 0o0002;
pub const S_IXOTH: u32 = 0o0001;

/// File stat information (`GStatBuf`).
#[derive(Clone, Debug, Default)]
pub struct StatBuf {
    pub st_size: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub gid: u32,
    pub st_atime: i64,
    pub st_mtime: i64,
    pub st_ctime: i64,
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_blocks: u64,
    pub st_blksize: u64,
}

impl StatBuf {
    /// Check if the file is a regular file.
    pub fn is_file(&self) -> bool {
        (self.st_mode & 0o170000) == 0o100000
    }

    /// Check if the file is a directory.
    pub fn is_dir(&self) -> bool {
        (self.st_mode & 0o170000) == 0o040000
    }

    /// Check if the file is a symlink.
    pub fn is_symlink(&self) -> bool {
        (self.st_mode & 0o170000) == 0o120000
    }

    /// Check if the file is executable.
    pub fn is_executable(&self) -> bool {
        (self.st_mode & 0o111) != 0
    }
}

/// Platform trait for stdio operations.
pub trait StdioPlatform {
    /// Check file access permissions (`g_access`).
    fn access(path: &str, mode: i32) -> i32;

    /// Change directory (`g_chdir`).
    fn chdir(path: &str) -> i32;

    /// Create a directory (`g_mkdir`).
    fn mkdir(path: &str, mode: u32) -> i32;

    /// Remove a directory (`g_rmdir`).
    fn rmdir(path: &str) -> i32;

    /// Unlink a file (`g_unlink`).
    fn unlink(path: &str) -> i32;

    /// Rename a file (`g_rename`).
    fn rename(oldpath: &str, newpath: &str) -> i32;

    /// Change file permissions (`g_chmod`).
    fn chmod(path: &str, mode: u32) -> i32;

    /// Open a file (`g_open`).
    fn open(path: &str, flags: OpenFlags, mode: u32) -> i32;

    /// Create a file (`g_creat`).
    fn creat(path: &str, mode: u32) -> i32;

    /// Stat a file (`g_stat`).
    fn stat(path: &str) -> Option<StatBuf>;

    /// Lstat a file (`g_lstat`).
    fn lstat(path: &str) -> Option<StatBuf>;

    /// Remove a file (`g_remove`).
    fn remove(path: &str) -> i32;

    /// Synchronize a file (`g_fsync`).
    fn fsync(fd: i32) -> i32;
}

/// A no-op platform implementation.
pub struct NoStdioPlatform;

impl StdioPlatform for NoStdioPlatform {
    fn access(_path: &str, _mode: i32) -> i32 { -1 }
    fn chdir(_path: &str) -> i32 { -1 }
    fn mkdir(_path: &str, _mode: u32) -> i32 { -1 }
    fn rmdir(_path: &str) -> i32 { -1 }
    fn unlink(_path: &str) -> i32 { -1 }
    fn rename(_oldpath: &str, _newpath: &str) -> i32 { -1 }
    fn chmod(_path: &str, _mode: u32) -> i32 { -1 }
    fn open(_path: &str, _flags: OpenFlags, _mode: u32) -> i32 { -1 }
    fn creat(_path: &str, _mode: u32) -> i32 { -1 }
    fn stat(_path: &str) -> Option<StatBuf> { None }
    fn lstat(_path: &str) -> Option<StatBuf> { None }
    fn remove(_path: &str) -> i32 { -1 }
    fn fsync(_fd: i32) -> i32 { -1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_buf_default() {
        let sb = StatBuf::default();
        assert_eq!(sb.st_size, 0);
        assert!(!sb.is_file());
        assert!(!sb.is_dir());
    }

    #[test]
    fn stat_buf_is_file() {
        let sb = StatBuf {
            st_mode: 0o100644,
            ..Default::default()
        };
        assert!(sb.is_file());
        assert!(!sb.is_dir());
    }

    #[test]
    fn stat_buf_is_dir() {
        let sb = StatBuf {
            st_mode: 0o040755,
            ..Default::default()
        };
        assert!(sb.is_dir());
        assert!(!sb.is_file());
    }

    #[test]
    fn stat_buf_is_symlink() {
        let sb = StatBuf {
            st_mode: 0o120755,
            ..Default::default()
        };
        assert!(sb.is_symlink());
    }

    #[test]
    fn stat_buf_is_executable() {
        let sb = StatBuf {
            st_mode: 0o100755,
            ..Default::default()
        };
        assert!(sb.is_executable());
    }

    #[test]
    fn open_flags() {
        assert_eq!(OpenFlags::O_RDONLY.0, 0);
        assert_eq!(OpenFlags::O_WRONLY.0, 1);
        assert_eq!(OpenFlags::O_RDWR.0, 2);
    }

    #[test]
    fn no_stdio_platform() {
        assert_eq!(NoStdioPlatform::access("/tmp", F_OK), -1);
        assert_eq!(NoStdioPlatform::mkdir("/tmp/test", 0o755), -1);
        assert!(NoStdioPlatform::stat("/tmp").is_none());
    }
}
