//! Stdio wrappers matching `gstdio.h` / `gstdio.c`.
//!
//! On Unix, gstdio functions are just aliases for the C library functions.
//! In no_std, we define types and a platform trait for file operations.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use spin::RwLock;

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
pub trait StdioPlatform: Sync {
    /// Check file access permissions (`g_access`).
    fn access(&self, path: &str, mode: i32) -> i32;

    /// Change directory (`g_chdir`).
    fn chdir(&self, path: &str) -> i32;

    /// Create a directory (`g_mkdir`).
    fn mkdir(&self, path: &str, mode: u32) -> i32;

    /// Remove a directory (`g_rmdir`).
    fn rmdir(&self, path: &str) -> i32;

    /// Unlink a file (`g_unlink`).
    fn unlink(&self, path: &str) -> i32;

    /// Rename a file (`g_rename`).
    fn rename(&self, oldpath: &str, newpath: &str) -> i32;

    /// Change file permissions (`g_chmod`).
    fn chmod(&self, path: &str, mode: u32) -> i32;

    /// Open a file (`g_open`).
    fn open(&self, path: &str, flags: OpenFlags, mode: u32) -> i32;

    /// Create a file (`g_creat`).
    fn creat(&self, path: &str, mode: u32) -> i32;

    /// Read from an open file descriptor.
    fn read(&self, fd: i32, buf: &mut [u8]) -> isize;

    /// Close an open file descriptor.
    fn close(&self, fd: i32) -> i32;

    /// Stat a file (`g_stat`).
    fn stat(&self, path: &str) -> Option<StatBuf>;

    /// Lstat a file (`g_lstat`).
    fn lstat(&self, path: &str) -> Option<StatBuf>;

    /// Remove a file (`g_remove`).
    fn remove(&self, path: &str) -> i32;

    /// Synchronize a file (`g_fsync`).
    fn fsync(&self, fd: i32) -> i32;
}

/// A no-op platform implementation.
pub struct NoStdioPlatform;

impl StdioPlatform for NoStdioPlatform {
    fn access(&self, _path: &str, _mode: i32) -> i32 {
        -1
    }
    fn chdir(&self, _path: &str) -> i32 {
        -1
    }
    fn mkdir(&self, _path: &str, _mode: u32) -> i32 {
        -1
    }
    fn rmdir(&self, _path: &str) -> i32 {
        -1
    }
    fn unlink(&self, _path: &str) -> i32 {
        -1
    }
    fn rename(&self, _oldpath: &str, _newpath: &str) -> i32 {
        -1
    }
    fn chmod(&self, _path: &str, _mode: u32) -> i32 {
        -1
    }
    fn open(&self, _path: &str, _flags: OpenFlags, _mode: u32) -> i32 {
        -1
    }
    fn read(&self, _fd: i32, _buf: &mut [u8]) -> isize {
        -1
    }
    fn close(&self, _fd: i32) -> i32 {
        -1
    }
    fn creat(&self, _path: &str, _mode: u32) -> i32 {
        -1
    }
    fn stat(&self, _path: &str) -> Option<StatBuf> {
        None
    }
    fn lstat(&self, _path: &str) -> Option<StatBuf> {
        None
    }
    fn remove(&self, _path: &str) -> i32 {
        -1
    }
    fn fsync(&self, _fd: i32) -> i32 {
        -1
    }
}

static STDIO_PLATFORM: RwLock<&'static dyn StdioPlatform> = RwLock::new(&NoStdioPlatform);

/// Installs the platform stdio implementation.
pub fn register_stdio_platform(platform: &'static dyn StdioPlatform) {
    *STDIO_PLATFORM.write() = platform;
}

/// Checks file access (`g_access`).
pub fn access(path: &str, mode: i32) -> i32 {
    STDIO_PLATFORM.read().access(path, mode)
}

/// Stats a path (`g_stat`).
pub fn stat(path: &str) -> Option<StatBuf> {
    STDIO_PLATFORM.read().stat(path)
}

/// Creates a directory (`g_mkdir`).
pub fn mkdir(path: &str, mode: u32) -> i32 {
    STDIO_PLATFORM.read().mkdir(path, mode)
}

/// Open a path (`g_open`).
pub fn open(path: &str, flags: OpenFlags, mode: u32) -> i32 {
    STDIO_PLATFORM.read().open(path, flags, mode)
}

/// Create a path (`g_creat`).
pub fn creat(path: &str, mode: u32) -> i32 {
    STDIO_PLATFORM.read().creat(path, mode)
}

/// Read from a file descriptor.
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    STDIO_PLATFORM.read().read(fd, buf)
}

/// Close a file descriptor.
pub fn close(fd: i32) -> i32 {
    STDIO_PLATFORM.read().close(fd)
}

/// Lstat a path (`g_lstat`).
pub fn lstat(path: &str) -> Option<StatBuf> {
    STDIO_PLATFORM.read().lstat(path)
}

/// Remove a path (`g_remove`).
pub fn remove(path: &str) -> i32 {
    STDIO_PLATFORM.read().remove(path)
}

/// Unlink a path (`g_unlink`).
pub fn unlink(path: &str) -> i32 {
    STDIO_PLATFORM.read().unlink(path)
}

/// Rename a path (`g_rename`).
pub fn rename(oldpath: &str, newpath: &str) -> i32 {
    STDIO_PLATFORM.read().rename(oldpath, newpath)
}

/// Change mode (`g_chmod`).
pub fn chmod(path: &str, mode: u32) -> i32 {
    STDIO_PLATFORM.read().chmod(path, mode)
}

/// Synchronize a file descriptor (`g_fsync`).
pub fn fsync(fd: i32) -> i32 {
    STDIO_PLATFORM.read().fsync(fd)
}

/// Read the full contents of a file into memory.
///
/// Uses `std::fs` when running unit tests on the host; otherwise delegates to
/// the registered [`StdioPlatform`].
pub fn read_file_bytes(path: &str) -> Option<Vec<u8>> {
    #[cfg(test)]
    {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }

    let platform = STDIO_PLATFORM.read();
    let fd = platform.open(path, OpenFlags::O_RDONLY, 0);
    if fd < 0 {
        return None;
    }

    let size = platform.stat(path)?.st_size as usize;
    let mut buf = alloc::vec![0u8; size];
    let mut filled = 0usize;
    while filled < size {
        let n = platform.read(fd, &mut buf[filled..]);
        if n <= 0 {
            let _ = platform.close(fd);
            return None;
        }
        filled += n as usize;
    }
    let _ = platform.close(fd);
    Some(buf)
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
        let platform = NoStdioPlatform;
        assert_eq!(platform.access("/tmp", F_OK), -1);
        assert_eq!(platform.mkdir("/tmp/test", 0o755), -1);
        assert!(platform.stat("/tmp").is_none());
    }
}
