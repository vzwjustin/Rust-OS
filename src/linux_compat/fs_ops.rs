//! Filesystem operations
//!
//! This module implements Linux filesystem operations including
//! mount, umount, statfs, and filesystem-level operations.

#![no_std]

extern crate alloc;

use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

/// Operation counter for statistics
static FS_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize filesystem operations subsystem
pub fn init_fs_operations() {
    FS_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of filesystem operations performed
pub fn get_operation_count() -> u64 {
    FS_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    FS_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Helper to convert null-terminated C string to Rust string
unsafe fn c_str_to_string(ptr: *const u8) -> Result<String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    String::from_utf8(slice.iter().copied().collect())
        .map_err(|_| LinuxError::EINVAL)
}

// ============================================================================
// Mount Flags
// ============================================================================

pub mod mount_flags {
    /// Mount read-only
    pub const MS_RDONLY: u64 = 1;
    /// Ignore suid and sgid bits
    pub const MS_NOSUID: u64 = 2;
    /// Disallow access to device special files
    pub const MS_NODEV: u64 = 4;
    /// Disallow program execution
    pub const MS_NOEXEC: u64 = 8;
    /// Writes are synced at once
    pub const MS_SYNCHRONOUS: u64 = 16;
    /// Alter flags of a mounted FS
    pub const MS_REMOUNT: u64 = 32;
    /// Allow mandatory locks on an FS
    pub const MS_MANDLOCK: u64 = 64;
    /// Directory modifications are synchronous
    pub const MS_DIRSYNC: u64 = 128;
    /// Do not update access times
    pub const MS_NOATIME: u64 = 1024;
    /// Do not update directory access times
    pub const MS_NODIRATIME: u64 = 2048;
    /// Bind directory at different place
    pub const MS_BIND: u64 = 4096;
    /// Move a subtree
    pub const MS_MOVE: u64 = 8192;
    /// Recursively apply flags
    pub const MS_REC: u64 = 16384;
    /// Update atime relative to mtime/ctime
    pub const MS_RELATIME: u64 = 1 << 21;
    /// Create a private mount
    pub const MS_PRIVATE: u64 = 1 << 18;
    /// Create a slave mount
    pub const MS_SLAVE: u64 = 1 << 19;
    /// Create a shared mount
    pub const MS_SHARED: u64 = 1 << 20;
}

// ============================================================================
// Umount Flags
// ============================================================================

pub mod umount_flags {
    /// Force unmount
    pub const MNT_FORCE: i32 = 1;
    /// Just detach from the tree
    pub const MNT_DETACH: i32 = 2;
    /// Mark for expiry
    pub const MNT_EXPIRE: i32 = 4;
    /// Don't follow symlink on umount
    pub const UMOUNT_NOFOLLOW: i32 = 8;
}

// ============================================================================
// Filesystem Information Structures
// ============================================================================

/// Filesystem statistics (statfs)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct StatFs {
    /// Type of filesystem
    pub f_type: i64,
    /// Optimal transfer block size
    pub f_bsize: i64,
    /// Total data blocks in filesystem
    pub f_blocks: u64,
    /// Free blocks in filesystem
    pub f_bfree: u64,
    /// Free blocks available to unprivileged user
    pub f_bavail: u64,
    /// Total file nodes in filesystem
    pub f_files: u64,
    /// Free file nodes in filesystem
    pub f_ffree: u64,
    /// Filesystem ID
    pub f_fsid: [i32; 2],
    /// Maximum length of filenames
    pub f_namelen: i64,
    /// Fragment size
    pub f_frsize: i64,
    /// Mount flags
    pub f_flags: i64,
    /// Padding
    pub f_spare: [i64; 4],
}

impl StatFs {
    pub fn zero() -> Self {
        StatFs {
            f_type: 0,
            f_bsize: 4096,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_fsid: [0; 2],
            f_namelen: 255,
            f_frsize: 4096,
            f_flags: 0,
            f_spare: [0; 4],
        }
    }
}

/// Filesystem types
pub mod fstype {
    /// ext2/ext3/ext4
    pub const EXT2_SUPER_MAGIC: i64 = 0xEF53;
    /// tmpfs
    pub const TMPFS_MAGIC: i64 = 0x01021994;
    /// proc
    pub const PROC_SUPER_MAGIC: i64 = 0x9fa0;
    /// NFS
    pub const NFS_SUPER_MAGIC: i64 = 0x6969;
    /// FAT
    pub const MSDOS_SUPER_MAGIC: i64 = 0x4d44;
    /// ISO 9660 CD-ROM
    pub const ISOFS_SUPER_MAGIC: i64 = 0x9660;
}

// ============================================================================
// Mount Operations
// ============================================================================

/// mount - mount filesystem
pub fn mount(
    source: *const u8,
    target: *const u8,
    filesystemtype: *const u8,
    mountflags: u64,
    data: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // source can be NULL for some filesystem types (e.g., tmpfs, proc)
    // filesystemtype can be NULL for MS_BIND, MS_MOVE, MS_REMOUNT

    // Validate flags
    let valid_flags = mount_flags::MS_RDONLY
        | mount_flags::MS_NOSUID
        | mount_flags::MS_NODEV
        | mount_flags::MS_NOEXEC
        | mount_flags::MS_SYNCHRONOUS
        | mount_flags::MS_REMOUNT
        | mount_flags::MS_MANDLOCK
        | mount_flags::MS_DIRSYNC
        | mount_flags::MS_NOATIME
        | mount_flags::MS_NODIRATIME
        | mount_flags::MS_BIND
        | mount_flags::MS_MOVE
        | mount_flags::MS_REC
        | mount_flags::MS_RELATIME
        | mount_flags::MS_PRIVATE
        | mount_flags::MS_SLAVE
        | mount_flags::MS_SHARED;

    if mountflags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Implement actual mounting
    // 1. Parse filesystem type
    // 2. Locate source device/path
    // 3. Load filesystem driver
    // 4. Mount at target path
    // 5. Apply mount flags
    // 6. Handle bind/move/remount specially

    Ok(0)
}

/// umount - unmount filesystem
pub fn umount(target: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Unmount filesystem at target
    // Check if not busy
    // Flush buffers
    // Remove from mount tree
    Ok(0)
}

/// umount2 - unmount filesystem with flags
pub fn umount2(target: *const u8, flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let valid_flags = umount_flags::MNT_FORCE
        | umount_flags::MNT_DETACH
        | umount_flags::MNT_EXPIRE
        | umount_flags::UMOUNT_NOFOLLOW;

    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Unmount with specific behavior
    // MNT_FORCE: force unmount even if busy
    // MNT_DETACH: lazy unmount
    // MNT_EXPIRE: mark for expiration
    Ok(0)
}

/// pivot_root - change root filesystem
pub fn pivot_root(new_root: *const u8, put_old: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if new_root.is_null() || put_old.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Change root filesystem
    // Move current root to put_old
    // Make new_root the new root
    // Requires CAP_SYS_ADMIN
    Ok(0)
}

// ============================================================================
// Filesystem Information
// ============================================================================

/// statfs - get filesystem statistics
pub fn statfs(path: *const u8, buf: *mut StatFs) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() || buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    let vfs_stat = crate::vfs::vfs_statfs(&path).map_err(|_| LinuxError::ENOSYS)?;

    unsafe {
        *buf = StatFs::zero();
        (*buf).f_type = vfs_stat.fs_type as i64;
        (*buf).f_bsize = vfs_stat.block_size as i64;
        (*buf).f_blocks = vfs_stat.total_blocks;
        (*buf).f_bfree = vfs_stat.free_blocks;
        (*buf).f_bavail = vfs_stat.avail_blocks;
        (*buf).f_files = vfs_stat.total_inodes;
        (*buf).f_ffree = vfs_stat.free_inodes;
        (*buf).f_namelen = vfs_stat.max_name_len as i64;
        (*buf).f_frsize = vfs_stat.block_size as i64;
    }

    Ok(0)
}

/// fstatfs - get filesystem statistics by file descriptor
pub fn fstatfs(fd: Fd, buf: *mut StatFs) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Get filesystem statistics for fd by looking up the root mount
    let vfs_stat = crate::vfs::vfs_statfs("/").map_err(|_| LinuxError::ENOSYS)?;
    unsafe {
        *buf = StatFs::zero();
        (*buf).f_type = vfs_stat.fs_type as i64;
        (*buf).f_bsize = vfs_stat.block_size as i64;
        (*buf).f_blocks = vfs_stat.total_blocks;
        (*buf).f_bfree = vfs_stat.free_blocks;
        (*buf).f_bavail = vfs_stat.avail_blocks;
        (*buf).f_files = vfs_stat.total_inodes;
        (*buf).f_ffree = vfs_stat.free_inodes;
        (*buf).f_namelen = vfs_stat.max_name_len as i64;
        (*buf).f_frsize = vfs_stat.block_size as i64;
    }

    Ok(0)
}

/// ustat - get filesystem statistics (obsolete, use statfs)
pub fn ustat(dev: Dev, ubuf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if ubuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Get filesystem statistics for device
    // This is obsolete, redirect to statfs
    Ok(0)
}

// ============================================================================
// Filesystem Sync Operations
// ============================================================================

/// sync - commit filesystem caches to disk
pub fn sync() {
    inc_ops();

    let _ = crate::vfs::get_vfs().sync_all();
}

/// syncfs - sync filesystem containing file
pub fn syncfs(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Sync all filesystems (syncfs syncs the filesystem containing the fd)
    let _ = crate::vfs::get_vfs().sync_all();
    Ok(0)
}

// ============================================================================
// Quota Operations
// ============================================================================

/// quotactl - manipulate disk quotas
pub fn quotactl(cmd: i32, special: *const u8, id: i32, addr: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Quota commands
    const Q_QUOTAON: i32 = 0x0100;
    const Q_QUOTAOFF: i32 = 0x0200;
    const Q_GETQUOTA: i32 = 0x0300;
    const Q_SETQUOTA: i32 = 0x0400;
    const Q_GETINFO: i32 = 0x0500;
    const Q_SETINFO: i32 = 0x0600;
    const Q_GETFMT: i32 = 0x0700;
    const Q_SYNC: i32 = 0x0800;

    let cmd_type = cmd & 0xFF00;
    match cmd_type {
        Q_QUOTAON | Q_QUOTAOFF | Q_GETQUOTA | Q_SETQUOTA | Q_GETINFO | Q_SETINFO | Q_GETFMT
        | Q_SYNC => {
            // TODO: Implement quota operations
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// Namespace Operations
// ============================================================================

/// unshare - disassociate parts of execution context
pub fn unshare(flags: i32) -> LinuxResult<i32> {
    inc_ops();

    // Unshare flags (from clone_flags)
    const CLONE_FILES: i32 = 0x00000400;
    const CLONE_FS: i32 = 0x00000200;
    const CLONE_NEWNS: i32 = 0x00020000;
    const CLONE_NEWUTS: i32 = 0x04000000;
    const CLONE_NEWIPC: i32 = 0x08000000;
    const CLONE_NEWNET: i32 = 0x40000000;
    const CLONE_NEWPID: i32 = 0x20000000;
    const CLONE_NEWUSER: i32 = 0x10000000;
    const CLONE_NEWCGROUP: i32 = 0x02000000;

    let valid_flags = CLONE_FILES
        | CLONE_FS
        | CLONE_NEWNS
        | CLONE_NEWUTS
        | CLONE_NEWIPC
        | CLONE_NEWNET
        | CLONE_NEWPID
        | CLONE_NEWUSER
        | CLONE_NEWCGROUP;

    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Unshare namespaces
    // Create new namespace(s) and move current process to them
    Ok(0)
}

/// setns - reassociate thread with a namespace
pub fn setns(fd: Fd, nstype: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Namespace types
    const CLONE_NEWNS: i32 = 0x00020000;
    const CLONE_NEWUTS: i32 = 0x04000000;
    const CLONE_NEWIPC: i32 = 0x08000000;
    const CLONE_NEWNET: i32 = 0x40000000;
    const CLONE_NEWPID: i32 = 0x20000000;
    const CLONE_NEWUSER: i32 = 0x10000000;
    const CLONE_NEWCGROUP: i32 = 0x02000000;

    if nstype != 0 {
        let valid_types = CLONE_NEWNS
            | CLONE_NEWUTS
            | CLONE_NEWIPC
            | CLONE_NEWNET
            | CLONE_NEWPID
            | CLONE_NEWUSER
            | CLONE_NEWCGROUP;

        if nstype & !valid_types != 0 {
            return Err(LinuxError::EINVAL);
        }
    }

    // TODO: Join namespace referred to by fd
    Ok(0)
}

// ============================================================================
// Swap Operations
// ============================================================================

/// swapon - start swapping to file/device
pub fn swapon(path: *const u8, swapflags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Swap flags
    const SWAP_FLAG_PREFER: i32 = 0x8000;
    const SWAP_FLAG_DISCARD: i32 = 0x10000;

    // TODO: Enable swapping
    // Requires CAP_SYS_ADMIN
    Ok(0)
}

/// swapoff - stop swapping to file/device
pub fn swapoff(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Disable swapping
    // Requires CAP_SYS_ADMIN
    Ok(0)
}

// ============================================================================
// Inotify (File Monitoring)
// ============================================================================

/// inotify_init - initialize inotify instance
pub fn inotify_init() -> LinuxResult<Fd> {
    inc_ops();

    // TODO: Create inotify instance
    Ok(200)
}

/// inotify_init1 - initialize inotify instance with flags
pub fn inotify_init1(flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    const IN_CLOEXEC: i32 = 0x80000;
    const IN_NONBLOCK: i32 = 0x800;

    if flags & !(IN_CLOEXEC | IN_NONBLOCK) != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Create inotify instance with flags
    Ok(200)
}

/// inotify_add_watch - add watch to inotify instance
pub fn inotify_add_watch(fd: Fd, pathname: *const u8, mask: u32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Add watch for pathname
    // Return watch descriptor
    Ok(1)
}

/// inotify_rm_watch - remove watch from inotify instance
pub fn inotify_rm_watch(fd: Fd, wd: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // TODO: Remove watch
    Ok(0)
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_statfs() {
        let mut buf = StatFs::zero();
        let path = b"/\0".as_ptr();
        assert!(statfs(path, &mut buf).is_ok());
    }

    #[test_case]
    fn test_mount_flags() {
        // Test that flags are properly defined
        assert_eq!(mount_flags::MS_RDONLY, 1);
        assert_eq!(mount_flags::MS_NOSUID, 2);
    }

    #[test_case]
    fn test_sync() {
        sync(); // Should not panic
    }

    #[test_case]
    fn test_inotify() {
        assert!(inotify_init().is_ok());
        assert!(inotify_init1(0).is_ok());
    }
}
