//! Filesystem operations
//!
//! This module implements Linux filesystem operations including
//! mount, umount, statfs, and filesystem-level operations.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use lazy_static::lazy_static;
use spin::Mutex;

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::vfs::{self, ramfs, FdKind, InodeType, VfsError};

/// Operation counter for statistics
static FS_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct MountEntry {
    source: String,
    target: String,
    fstype: String,
    flags: u64,
}

lazy_static! {
    static ref INOTIFY_INSTANCES: Mutex<BTreeMap<u32, InotifyInstance>> =
        Mutex::new(BTreeMap::new());
    static ref INOTIFY_FD_MAP: Mutex<BTreeMap<Fd, u32>> = Mutex::new(BTreeMap::new());
    static ref MOUNT_TABLE: Mutex<Vec<MountEntry>> = Mutex::new(Vec::new());
}

fn record_mount(source: &str, target: &str, fstype: &str, flags: u64) {
    let mut table = MOUNT_TABLE.lock();
    table.retain(|e| e.target != target);
    table.push(MountEntry {
        source: String::from(source),
        target: String::from(target),
        fstype: String::from(fstype),
        flags,
    });
}

fn remove_mount(target: &str) {
    MOUNT_TABLE.lock().retain(|e| e.target != target);
}

/// `/proc/mounts` content for userspace mount checks.
pub fn mounts_proc_content() -> String {
    let mut out = String::from("rootfs / rootfs rw 0 0\n");
    for entry in MOUNT_TABLE.lock().iter() {
        let ro = if entry.flags & mount_flags::MS_RDONLY != 0 {
            "ro"
        } else {
            "rw"
        };
        out.push_str(&format!(
            "{} {} {} {} 0 0\n",
            entry.source, entry.target, entry.fstype, ro
        ));
    }
    out
}

fn ensure_mount_target(path: &str) -> LinuxResult<()> {
    if vfs::vfs_stat(path).is_ok() {
        return Ok(());
    }
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let mut current = String::new();
    while let Some(part) = parts.next() {
        if current.is_empty() {
            current.push('/');
        } else {
            current.push('/');
        }
        current.push_str(part);
        if vfs::vfs_stat(&current).is_err() {
            let _ = vfs::vfs_mkdir(&current, 0o755);
        }
    }
    Ok(())
}

fn parse_block_device_path(path: &str) -> Option<(u32, Option<u8>)> {
    let path = path.trim();
    if !path.starts_with("/dev/sd") || path.len() < 7 {
        return None;
    }
    let letter = path.as_bytes()[6];
    if !letter.is_ascii_lowercase() {
        return None;
    }
    let device_id = (letter - b'a') as u32;
    let suffix = &path[7..];
    if suffix.is_empty() {
        return Some((device_id, None));
    }
    if !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut part: u8 = 0;
    for b in suffix.bytes() {
        part = part.saturating_mul(10).saturating_add(b - b'0');
    }
    Some((device_id, Some(part)))
}

fn cooperative_block_mount(
    source: &str,
    target: &str,
    fstype: &str,
    flags: u64,
    mount_data: Option<&str>,
) -> LinuxResult<i32> {
    ensure_mount_target(target)?;
    match fstype {
        "overlay" => {
            let opts = mount_data.unwrap_or("");
            let (lower, upper) = crate::vfs::overlayfs::parse_overlay_options(opts)
                .ok_or(LinuxError::EINVAL)?;
            crate::vfs::overlayfs::mount_overlay(&lower, target, &upper, "/run/overlay/work")
                .map_err(|_| LinuxError::EIO)?;
        }
        "squashfs" => {
            crate::vfs::live_mount::mount_squashfs(source, target).map_err(|_| LinuxError::EIO)?;
        }
        _ => {
            if vfs::vfs_stat(source).is_err() {
                return Err(LinuxError::ENODEV);
            }
            if let Some((device_id, part)) = parse_block_device_path(source) {
                if let Some(fsi) =
                    crate::drivers::storage::filesystem_interface::get_filesystem_interface()
                {
                    let _ = fsi.mount_filesystem(device_id, part, String::from(target), None);
                }
            }
            crate::vfs::legacy_mount::mount_block_device(source, target, fstype)
                .map_err(|e| match e {
                    VfsError::AlreadyExists => LinuxError::EBUSY,
                    VfsError::NotFound => LinuxError::ENODEV,
                    _ => LinuxError::EIO,
                })?;
        }
    }
    record_mount(source, target, fstype, flags);
    Ok(0)
}


static NEXT_INOTIFY_ID: AtomicU32 = AtomicU32::new(1);

struct InotifyInstance {
    flags: i32,
    watches: BTreeMap<i32, String>,
    next_wd: AtomicI32,
}

impl InotifyInstance {
    fn new(flags: i32) -> Self {
        Self {
            flags,
            watches: BTreeMap::new(),
            next_wd: AtomicI32::new(1),
        }
    }
}

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
fn c_str_to_string(ptr: *const u8) -> Result<String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path =
        UserSpaceMemory::copy_string_from_user(ptr as u64, 4096).map_err(|_| LinuxError::EFAULT)?;
    if path.len() >= 4096 {
        return Err(LinuxError::ENAMETOOLONG);
    }

    Ok(path)
}

fn vfs_error_to_linux(err: VfsError) -> LinuxError {
    match err {
        VfsError::NotFound => LinuxError::ENOENT,
        VfsError::PermissionDenied => LinuxError::EACCES,
        VfsError::AlreadyExists => LinuxError::EEXIST,
        VfsError::NotDirectory => LinuxError::ENOTDIR,
        VfsError::IsDirectory => LinuxError::EISDIR,
        VfsError::InvalidArgument => LinuxError::EINVAL,
        VfsError::IoError => LinuxError::EIO,
        VfsError::NoSpace => LinuxError::ENOSPC,
        VfsError::TooManyFiles => LinuxError::EMFILE,
        VfsError::BadFileDescriptor => LinuxError::EBADF,
        VfsError::InvalidSeek => LinuxError::EINVAL,
        VfsError::NameTooLong => LinuxError::ENAMETOOLONG,
        VfsError::CrossDevice => LinuxError::EXDEV,
        VfsError::ReadOnly => LinuxError::EROFS,
        VfsError::NotSupported => LinuxError::ENOSYS,
        VfsError::DirectoryNotEmpty => LinuxError::ENOTEMPTY,
    }
}

fn validate_mount_target(path: &str) -> LinuxResult<()> {
    match vfs::vfs_stat(path) {
        Ok(stat) => {
            if stat.inode_type != InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }
            Ok(())
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

fn normalize_mount_path(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }
    String::from(path.trim_end_matches('/'))
}

fn root_inode() -> Arc<dyn vfs::InodeOps> {
    vfs::get_vfs().lookup("/").expect("root mount")
}

fn alloc_inotify_fd(flags: i32) -> LinuxResult<Fd> {
    let id = NEXT_INOTIFY_ID.fetch_add(1, Ordering::Relaxed);
    INOTIFY_INSTANCES
        .lock()
        .insert(id, InotifyInstance::new(flags));

    let vfs_flags = if flags & 0x800 != 0 { 0x800 } else { 0 };

    let fd =
        vfs::vfs_open_special(root_inode(), vfs_flags, FdKind::Inotify(id)).map_err(
            |e| match e {
                VfsError::TooManyFiles => LinuxError::EMFILE,
                _ => LinuxError::EMFILE,
            },
        )?;

    INOTIFY_FD_MAP.lock().insert(fd, id);
    Ok(fd)
}

fn inotify_id_for_fd(fd: Fd) -> LinuxResult<u32> {
    INOTIFY_FD_MAP
        .lock()
        .get(&fd)
        .copied()
        .ok_or(LinuxError::EBADF)
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

fn fill_statfs(buf: *mut StatFs, vfs_stat: vfs::StatFs) {
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

    let target_str = normalize_mount_path(&c_str_to_string(target)?);
    ensure_mount_target(&target_str)?;
    validate_mount_target(&target_str)?;

    if mountflags & (mount_flags::MS_MOVE | mount_flags::MS_REMOUNT) != 0 {
        return Err(LinuxError::ENOSYS);
    }

    if mountflags & mount_flags::MS_BIND != 0 {
        if source.is_null() {
            return Err(LinuxError::ENODEV);
        }
        let source_str = normalize_mount_path(&c_str_to_string(source)?);
        if vfs::vfs_stat(&source_str).is_err() {
            return Err(LinuxError::ENOENT);
        }
        ensure_mount_target(&target_str)?;
        record_mount(&source_str, &target_str, "bind", mountflags);
        return Ok(0);
    }

    if filesystemtype.is_null() {
        return Err(LinuxError::EINVAL);
    }

    let mount_data = if data.is_null() {
        None
    } else {
        Some(c_str_to_string(data)?)
    };
    let mount_data_ref = mount_data.as_deref();

    let fstype = c_str_to_string(filesystemtype)?;
    match fstype.as_str() {
        "tmpfs" => {
            let sb = Arc::new(ramfs::RamFs::new());
            vfs::vfs_mount(&target_str, sb).map_err(|e| match e {
                VfsError::AlreadyExists => LinuxError::EBUSY,
                VfsError::NotFound => LinuxError::ENOENT,
                _ => LinuxError::ENOSYS,
            })?;
            if source.is_null() {
                record_mount("tmpfs", &target_str, "tmpfs", mountflags);
            } else {
                let src = c_str_to_string(source)?;
                record_mount(&src, &target_str, "tmpfs", mountflags);
            }
            Ok(0)
        }
        "proc" | "sysfs" | "devtmpfs" | "devpts" => {
            let src = if source.is_null() {
                fstype.clone()
            } else {
                c_str_to_string(source)?
            };
            record_mount(&src, &target_str, &fstype, mountflags);
            Ok(0)
        }
        "ext4" | "ext3" | "ext2" | "vfat" | "msdos" | "fat" | "squashfs" | "overlay" => {
            if source.is_null() {
                return Err(LinuxError::ENODEV);
            }
            let source_str = c_str_to_string(source)?;
            cooperative_block_mount(&source_str, &target_str, &fstype, mountflags, mount_data_ref)
        }
        _ => {
            if source.is_null() {
                return Err(LinuxError::ENODEV);
            }
            let source_str = c_str_to_string(source)?;
            if vfs::vfs_stat(&source_str).is_err() {
                return Err(LinuxError::ENODEV);
            }
            Err(LinuxError::ENOSYS)
        }
    }
}

/// umount - unmount filesystem
pub fn umount(target: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target_str = normalize_mount_path(&c_str_to_string(target)?);
    if target_str == "/" {
        return Err(LinuxError::EBUSY);
    }

    remove_mount(&target_str);
    match vfs::vfs_umount(&target_str) {
        Ok(()) => Ok(0),
        Err(VfsError::NotFound) => Ok(0),
        Err(VfsError::InvalidArgument) => Err(LinuxError::EBUSY),
        Err(_) => Ok(0),
    }
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

    if flags & (umount_flags::MNT_EXPIRE | umount_flags::UMOUNT_NOFOLLOW) != 0 {
        return Err(LinuxError::ENOSYS);
    }

    umount(target)
}

/// pivot_root - change root filesystem
pub fn pivot_root(new_root: *const u8, put_old: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if new_root.is_null() || put_old.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let new_root_str = c_str_to_string(new_root)?;
    let put_old_str = c_str_to_string(put_old)?;

    ensure_mount_target(&new_root_str)?;
    ensure_mount_target(&put_old_str)?;
    validate_mount_target(&new_root_str)?;
    validate_mount_target(&put_old_str)?;
    record_mount(&new_root_str, "/", "root", mount_flags::MS_MOVE);
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

    let path = c_str_to_string(path)?;
    let vfs_stat = vfs::vfs_statfs(&path).map_err(|e| match e {
        VfsError::NotFound => LinuxError::ENOENT,
        _ => LinuxError::ENOSYS,
    })?;

    fill_statfs(buf, vfs_stat);
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

    let path = match vfs::vfs_fd_directory_path(fd) {
        Ok(p) => p,
        Err(_) => String::from("/"),
    };
    let vfs_stat = vfs::vfs_statfs(&path).map_err(|_| LinuxError::ENOSYS)?;
    fill_statfs(buf, vfs_stat);
    Ok(0)
}

/// ustat - get filesystem statistics (obsolete, use statfs)
pub fn ustat(_dev: Dev, ubuf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if ubuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    Err(LinuxError::ENOSYS)
}

// ============================================================================
// Filesystem Sync Operations
// ============================================================================

/// sync - commit filesystem caches to disk
pub fn sync() {
    inc_ops();

    let _ = vfs::get_vfs().sync_all();
}

/// syncfs - sync filesystem containing file
pub fn syncfs(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let _ = vfs::get_vfs().sync_all();
    Ok(0)
}

// ============================================================================
// Quota Operations
// ============================================================================

/// quotactl - manipulate disk quotas
pub fn quotactl(cmd: i32, _special: *const u8, _id: i32, _addr: *mut u8) -> LinuxResult<i32> {
    inc_ops();

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
        | Q_SYNC => Err(LinuxError::ENOSYS),
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// Namespace Operations
// ============================================================================

/// unshare - disassociate parts of execution context
pub fn unshare(flags: i32) -> LinuxResult<i32> {
    inc_ops();

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

    if flags == 0 {
        return Err(LinuxError::EINVAL);
    }

    Err(LinuxError::ENOSYS)
}

/// setns - reassociate thread with a namespace
pub fn setns(fd: Fd, nstype: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

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

    Err(LinuxError::ENOSYS)
}

// ============================================================================
// Swap Operations
// ============================================================================

/// swapon - start swapping to file/device
pub fn swapon(path: *const u8, _swapflags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let _ = c_str_to_string(path)?;
    Err(LinuxError::ENOSYS)
}

/// swapoff - stop swapping to file/device
pub fn swapoff(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let _ = c_str_to_string(path)?;
    Err(LinuxError::ENOSYS)
}

// ============================================================================
// Inotify (File Monitoring)
// ============================================================================

/// inotify_init - initialize inotify instance
pub fn inotify_init() -> LinuxResult<Fd> {
    inc_ops();
    alloc_inotify_fd(0)
}

/// inotify_init1 - initialize inotify instance with flags
pub fn inotify_init1(flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    const IN_CLOEXEC: i32 = 0x80000;
    const IN_NONBLOCK: i32 = 0x800;

    if flags & !(IN_CLOEXEC | IN_NONBLOCK) != 0 {
        return Err(LinuxError::EINVAL);
    }

    alloc_inotify_fd(flags)
}

/// inotify_add_watch - add watch to inotify instance
pub fn inotify_add_watch(fd: Fd, pathname: *const u8, _mask: u32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = c_str_to_string(pathname)?;
    if vfs::vfs_stat(&path).is_err() {
        return Err(LinuxError::ENOENT);
    }

    let id = inotify_id_for_fd(fd)?;
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(LinuxError::EBADF)?;
    let wd = instance.next_wd.fetch_add(1, Ordering::Relaxed);
    instance.watches.insert(wd, path);
    Ok(wd)
}

/// inotify_rm_watch - remove watch from inotify instance
pub fn inotify_rm_watch(fd: Fd, wd: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let id = inotify_id_for_fd(fd)?;
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(LinuxError::EBADF)?;
    if instance.watches.remove(&wd).is_none() {
        return Err(LinuxError::EINVAL);
    }
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
        assert_eq!(mount_flags::MS_RDONLY, 1);
        assert_eq!(mount_flags::MS_NOSUID, 2);
    }

    #[test_case]
    fn test_sync() {
        sync();
    }

    #[test_case]
    fn test_inotify() {
        assert!(inotify_init().is_ok());
        assert!(inotify_init1(0).is_ok());
    }
}
