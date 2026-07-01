//! Filesystem operations
//!
//! This module implements Linux filesystem operations including
//! mount, umount, statfs, and filesystem-level operations.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
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
    crate::quota::register_mount(source, target);
}

fn remove_mount(target: &str) {
    MOUNT_TABLE.lock().retain(|e| e.target != target);
    crate::quota::unregister_mount(target);
}

/// `/proc/mounts` content for userspace mount checks.
pub fn mounts_proc_content() -> String {
    let mut out = String::from(
        "rootfs / rootfs rw 0 0\n\
         proc /proc proc rw 0 0\n\
         sysfs /sys sysfs rw 0 0\n\
         devtmpfs /dev devtmpfs rw 0 0\n",
    );
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
            let (lower, upper) =
                crate::vfs::overlayfs::parse_overlay_options(opts).ok_or(LinuxError::EINVAL)?;
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
            crate::vfs::legacy_mount::mount_block_device(source, target, fstype).map_err(|e| {
                match e {
                    VfsError::AlreadyExists => LinuxError::EBUSY,
                    VfsError::NotFound => LinuxError::ENODEV,
                    _ => LinuxError::EIO,
                }
            })?;
        }
    }
    record_mount(source, target, fstype, flags);
    Ok(0)
}

static NEXT_INOTIFY_ID: AtomicU32 = AtomicU32::new(1);

struct InotifyWatch {
    path: String,
    mask: u32,
}

/// A single inotify event waiting to be read by userspace.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InotifyEvent {
    pub wd: i32,
    pub mask: u32,
    pub cookie: u32,
    pub len: u32,
    // Followed by `len` bytes of name (NUL-terminated, padded to NUL boundary)
}

struct InotifyInstance {
    flags: i32,
    watches: BTreeMap<i32, InotifyWatch>,
    next_wd: AtomicI32,
    /// Pending events queue (event header + optional name).
    events: alloc::collections::VecDeque<(InotifyEvent, Option<alloc::string::String>)>,
}

impl InotifyInstance {
    fn new(flags: i32) -> Self {
        Self {
            flags,
            watches: BTreeMap::new(),
            next_wd: AtomicI32::new(1),
            events: alloc::collections::VecDeque::new(),
        }
    }

    /// Push an inotify event into the queue.
    fn push_event(&mut self, wd: i32, mask: u32, cookie: u32, name: Option<&str>) {
        let name_bytes = name
            .map(|n| {
                // Name field is NUL-terminated and padded to align to the
                // next sizeof(struct inotify_event) boundary (16 bytes).
                let raw_len = n.as_bytes().len() + 1; // +1 for NUL
                let padded_len = (raw_len + 15) & !15;
                padded_len as u32
            })
            .unwrap_or(0);

        self.events.push_back((
            InotifyEvent {
                wd,
                mask,
                cookie,
                len: name_bytes,
            },
            name.map(|n| alloc::string::String::from(n)),
        ));
    }
}

/// Fire an inotify event to all instances watching the given path.
/// Called by VFS operations (create, delete, modify, etc.).
pub fn fire_inotify_event(path: &str, mask: u32, cookie: u32, name: Option<&str>) {
    let mut instances = INOTIFY_INSTANCES.lock();
    for instance in instances.values_mut() {
        // Collect matching watch descriptors first to avoid borrow conflict.
        let matching_wds: alloc::vec::Vec<i32> = instance
            .watches
            .iter()
            .filter(|(_, watch)| {
                if watch.mask & mask == 0 {
                    return false;
                }
                path == watch.path || path.starts_with(&watch.path) || watch.path == "/"
            })
            .map(|(wd, _)| *wd)
            .collect();

        for wd in matching_wds {
            instance.push_event(wd, mask, cookie, name);
        }
    }
}

/// Read pending inotify events for a given inotify fd.
/// Returns the number of bytes written to `buf`.
pub fn read_inotify_events(fd: i32, buf: &mut [u8]) -> LinuxResult<isize> {
    let id = inotify_id_for_fd(fd)?;
    let mut instances = INOTIFY_INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(LinuxError::EBADF)?;

    if instance.events.is_empty() {
        return Err(LinuxError::EAGAIN);
    }

    let mut offset = 0usize;
    while let Some((event, name)) = instance.events.front() {
        let name_bytes = name
            .as_ref()
            .map(|n| {
                let raw_len = n.as_bytes().len() + 1;
                let padded_len = (raw_len + 15) & !15;
                padded_len
            })
            .unwrap_or(0);
        let total = 16 + name_bytes;

        if offset + total > buf.len() {
            break;
        }

        // Write event header
        buf[offset..offset + 4].copy_from_slice(&event.wd.to_le_bytes());
        buf[offset + 4..offset + 8].copy_from_slice(&event.mask.to_le_bytes());
        buf[offset + 8..offset + 12].copy_from_slice(&event.cookie.to_le_bytes());
        buf[offset + 12..offset + 16].copy_from_slice(&event.len.to_le_bytes());

        // Write name if present
        if let Some(name_str) = name {
            let name_bytes = name_str.as_bytes();
            let raw_len = name_bytes.len() + 1;
            let padded_len = (raw_len + 15) & !15;
            buf[offset + 16..offset + 16 + name_bytes.len()].copy_from_slice(name_bytes);
            // NUL-pad the rest
            for i in (16 + name_bytes.len())..(16 + padded_len) {
                buf[offset + i] = 0;
            }
        }

        offset += total;
        instance.events.pop_front();
    }

    if offset == 0 {
        // Event too large for buffer
        return Err(LinuxError::EINVAL);
    }

    Ok(offset as isize)
}

/// Check if an inotify instance has pending events (for poll).
pub fn inotify_has_events(id: u32) -> bool {
    let instances = INOTIFY_INSTANCES.lock();
    instances
        .get(&id)
        .map(|inst| !inst.events.is_empty())
        .unwrap_or(false)
}

/// Drop inotify instance state when the special fd is closed.
pub fn close_inotify(id: u32) {
    INOTIFY_INSTANCES.lock().remove(&id);
    INOTIFY_FD_MAP
        .lock()
        .retain(|_, instance_id| *instance_id != id);
}

/// Initialize filesystem operations subsystem
pub fn init_fs_operations() {
    FS_OPS_COUNT.store(0, Ordering::Relaxed);
    crate::fs::cifs::init();
    crate::fs::nfsd::init();
    crate::fs::nfs_client::init();
}

fn parse_nfs_source(source: &str) -> LinuxResult<(String, String)> {
    let trimmed = source.trim();
    if let Some(rest) = trimmed.strip_prefix("//") {
        let mut parts = rest.splitn(2, '/');
        let server = parts.next().ok_or(LinuxError::EINVAL)?.to_string();
        let export = parts
            .next()
            .filter(|p| !p.is_empty())
            .unwrap_or("/")
            .to_string();
        return Ok((server, export));
    }
    let mut parts = trimmed.splitn(2, ':');
    let server = parts.next().ok_or(LinuxError::EINVAL)?.to_string();
    let export = parts
        .next()
        .filter(|p| !p.is_empty())
        .unwrap_or("/")
        .to_string();
    Ok((server, export))
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
        VfsError::DiskQuotaExceeded => LinuxError::EDQUOT,
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

fn record_tmpfs_mount(source: *const u8, target: &str, flags: u64) -> LinuxResult<()> {
    if source.is_null() {
        record_mount("tmpfs", target, "tmpfs", flags);
    } else {
        let src = c_str_to_string(source)?;
        record_mount(&src, target, "tmpfs", flags);
    }
    Ok(())
}

fn kernel_overlay_socket_ready(path: &str) -> bool {
    matches!(
        vfs::vfs_stat(path),
        Ok(stat) if stat.inode_type == InodeType::Socket
    )
}

fn should_preserve_kernel_runtime_mount(target: &str) -> bool {
    target == "/run"
        && crate::gnome_overlay::is_ready()
        && kernel_overlay_socket_ready(crate::gnome_overlay::DBUS_SESSION_SOCKET)
        && kernel_overlay_socket_ready(crate::gnome_overlay::WAYLAND_SOCKET)
}

fn should_preserve_virtual_runtime_mount(fstype: &str, target: &str) -> bool {
    matches!(
        (fstype, target),
        ("proc", "/proc") | ("sysfs", "/sys") | ("devtmpfs", "/dev")
    )
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

fn copy_struct_to_user<T>(dst: *mut T, value: &T) -> LinuxResult<()> {
    if dst.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let bytes = unsafe {
        core::slice::from_raw_parts((value as *const T).cast::<u8>(), core::mem::size_of::<T>())
    };
    UserSpaceMemory::copy_to_user(dst as u64, bytes).map_err(|_| LinuxError::EFAULT)
}

fn fill_statfs(buf: *mut StatFs, vfs_stat: vfs::StatFs) -> LinuxResult<()> {
    let mut stat = StatFs::zero();
    stat.f_type = vfs_stat.fs_type as i64;
    stat.f_bsize = vfs_stat.block_size as i64;
    stat.f_blocks = vfs_stat.total_blocks;
    stat.f_bfree = vfs_stat.free_blocks;
    stat.f_bavail = vfs_stat.avail_blocks;
    stat.f_files = vfs_stat.total_inodes;
    stat.f_ffree = vfs_stat.free_inodes;
    stat.f_namelen = vfs_stat.max_name_len as i64;
    stat.f_frsize = vfs_stat.block_size as i64;

    copy_struct_to_user(buf, &stat)
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

    if mountflags & mount_flags::MS_REMOUNT != 0 {
        // Update the flags of an existing mount. The target must already be
        // mounted; source and filesystemtype are ignored for remount.
        let mut table = MOUNT_TABLE.lock();
        if let Some(entry) = table.iter_mut().find(|e| e.target == target_str) {
            entry.flags = mountflags & !mount_flags::MS_REMOUNT;
            return Ok(0);
        }
        return Err(LinuxError::EINVAL);
    }

    if mountflags & mount_flags::MS_MOVE != 0 {
        // MS_MOVE: move an existing mount from source to target
        if source.is_null() {
            return Err(LinuxError::EINVAL);
        }
        let source_str = normalize_mount_path(&c_str_to_string(source)?);
        // Move mount: unmount from source, mount at target
        // Simplified — just record the move
        remove_mount(&source_str);
        record_mount(&source_str, &target_str, "move", mountflags);
        return Ok(0);
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
            if should_preserve_kernel_runtime_mount(&target_str) {
                record_tmpfs_mount(source, &target_str, mountflags)?;
                return Ok(0);
            }

            let sb = Arc::new(ramfs::RamFs::new());
            vfs::vfs_mount(&target_str, sb).map_err(|e| match e {
                VfsError::AlreadyExists => LinuxError::EBUSY,
                VfsError::NotFound => LinuxError::ENOENT,
                _ => LinuxError::ENOSYS,
            })?;
            record_tmpfs_mount(source, &target_str, mountflags)?;
            Ok(0)
        }
        "proc" | "sysfs" | "devtmpfs" | "devpts" => {
            if should_preserve_virtual_runtime_mount(&fstype, &target_str)
                && vfs::vfs_stat(&target_str).is_ok()
            {
                let src = if source.is_null() {
                    fstype.clone()
                } else {
                    c_str_to_string(source)?
                };
                record_mount(&src, &target_str, &fstype, mountflags);
                return Ok(0);
            }

            // Mount a real in-memory filesystem at the target. This is a
            // best-effort virtual filesystem implementation; userspace can read
            // and write entries in the mounted tree.
            let sb = Arc::new(ramfs::RamFs::new());
            match vfs::vfs_mount(&target_str, sb) {
                Ok(()) => {
                    let src = if source.is_null() {
                        fstype.clone()
                    } else {
                        c_str_to_string(source)?
                    };
                    record_mount(&src, &target_str, &fstype, mountflags);
                    Ok(0)
                }
                Err(VfsError::AlreadyExists) => Err(LinuxError::EBUSY),
                Err(VfsError::NotFound) => Err(LinuxError::ENOENT),
                Err(_) => Err(LinuxError::ENOSYS),
            }
        }
        "ext4" | "ext3" | "ext2" | "vfat" | "msdos" | "fat" | "squashfs" | "overlay" | "f2fs"
        | "btrfs" | "xfs" | "iso9660" => {
            if source.is_null() {
                return Err(LinuxError::ENODEV);
            }
            let source_str = c_str_to_string(source)?;
            cooperative_block_mount(
                &source_str,
                &target_str,
                &fstype,
                mountflags,
                mount_data_ref,
            )
        }
        "nfs" | "nfs4" => {
            if source.is_null() {
                return Err(LinuxError::ENODEV);
            }
            let source_str = c_str_to_string(source)?;
            let (server, export) = parse_nfs_source(&source_str)?;
            crate::fs::nfs_client::mount_read_only(
                &server,
                &export,
                &target_str,
                crate::fs::nfs_client::NfsFh { data: Vec::new() },
            )
            .map_err(|_| LinuxError::EIO)?;
            let sb = Arc::new(ramfs::RamFs::new());
            vfs::vfs_mount(&target_str, sb).map_err(|e| match e {
                VfsError::AlreadyExists => LinuxError::EBUSY,
                VfsError::NotFound => LinuxError::ENOENT,
                _ => LinuxError::ENOSYS,
            })?;
            record_mount(&source_str, &target_str, &fstype, mountflags);
            Ok(0)
        }
        "cifs" | "smb3" => {
            if source.is_null() {
                return Err(LinuxError::ENODEV);
            }
            let source_str = c_str_to_string(source)?;
            let read_only = mountflags & mount_flags::MS_RDONLY != 0;
            crate::fs::cifs::mount_from_options(
                &source_str,
                &target_str,
                mount_data_ref,
                read_only,
            )
            .map_err(|_| LinuxError::EIO)?;
            let sb = Arc::new(ramfs::RamFs::new());
            vfs::vfs_mount(&target_str, sb).map_err(|e| match e {
                VfsError::AlreadyExists => LinuxError::EBUSY,
                VfsError::NotFound => LinuxError::ENOENT,
                _ => LinuxError::ENOSYS,
            })?;
            record_mount(&source_str, &target_str, &fstype, mountflags);
            Ok(0)
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

    match vfs::vfs_umount(&target_str) {
        Ok(()) => {
            remove_mount(&target_str);
            Ok(0)
        }
        Err(VfsError::NotFound) => Err(LinuxError::ENOENT),
        Err(VfsError::InvalidArgument) => Err(LinuxError::EBUSY),
        Err(_) => Err(LinuxError::EINVAL),
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

    if flags & umount_flags::MNT_EXPIRE != 0 {
        // MNT_EXPIRE: mark mount as expirable — if busy, return EBUSY
        // Simplified: just proceed with normal umount
    }

    if flags & umount_flags::UMOUNT_NOFOLLOW != 0 {
        // UMOUNT_NOFOLLOW: don't follow symlinks — proceed normally
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

    fill_statfs(buf, vfs_stat)?;
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
    fill_statfs(buf, vfs_stat)?;
    Ok(0)
}

/// ustat - get filesystem statistics (obsolete, use statfs)
pub fn ustat(_dev: Dev, ubuf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if ubuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // struct ustat { char f_fname[6]; char f_fpack[6]; long f_tfree; ino_t f_tinode; }
    // 20 bytes on 64-bit — return zeroed
    let zeros = [0u8; 20];
    unsafe {
        core::ptr::copy_nonoverlapping(zeros.as_ptr(), ubuf, 20);
    }
    Ok(0)
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
pub fn quotactl(cmd: i32, special: *const u8, id: i32, addr: *mut u8) -> LinuxResult<i32> {
    inc_ops();
    crate::quota::quotactl(cmd, special, id, addr)
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

    if flags & (CLONE_FILES | CLONE_FS) != 0 {
        return Err(LinuxError::ENOTSUP);
    }

    let ret = crate::namespace::unshare(flags as u32);
    if ret < 0 {
        return Err(LinuxError::from_errno(-ret));
    }
    Ok(ret)
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

    let ret = crate::namespace::setns(fd, nstype as u32);
    if ret < 0 {
        Err(LinuxError::from_errno(-ret))
    } else {
        Ok(ret)
    }
}

// ============================================================================
// Swap Operations
// ============================================================================

/// Static table mapping swap area index → VFS file descriptor.
/// Needed because SwapArea uses `fn` pointers, not closures.
static SWAP_FDS: Mutex<BTreeMap<u8, i32>> = Mutex::new(BTreeMap::new());

fn swap_write_page_impl(area_index: u8, slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    let fds = SWAP_FDS.lock();
    let fd = *fds.get(&area_index).ok_or("swap area not found")?;
    drop(fds);
    let offset = slot.checked_mul(4096).ok_or("swap offset overflow")?;
    let vfs = crate::vfs::get_vfs();
    let written = vfs
        .pwrite(fd, data, offset)
        .map_err(|_| "swap write failed")?;
    if written != data.len() {
        return Err("short swap write");
    }
    Ok(())
}

fn swap_read_page_impl(
    area_index: u8,
    slot: u64,
    data: &mut [u8; 4096],
) -> Result<(), &'static str> {
    let fds = SWAP_FDS.lock();
    let fd = *fds.get(&area_index).ok_or("swap area not found")?;
    drop(fds);
    let offset = slot.checked_mul(4096).ok_or("swap offset overflow")?;
    let vfs = crate::vfs::get_vfs();
    let read = vfs
        .pread(fd, data, offset)
        .map_err(|_| "swap read failed")?;
    if read != data.len() {
        return Err("short swap read");
    }
    Ok(())
}

/// Generate trampoline fn pointers for swap area indices 0-7.
/// SwapArea takes `fn` pointers, not closures, so we need one
/// trampoline per possible area index.

fn swap_write_page_0(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(0, slot, data)
}
fn swap_read_page_0(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(0, slot, data)
}
fn swap_write_page_1(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(1, slot, data)
}
fn swap_read_page_1(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(1, slot, data)
}
fn swap_write_page_2(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(2, slot, data)
}
fn swap_read_page_2(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(2, slot, data)
}
fn swap_write_page_3(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(3, slot, data)
}
fn swap_read_page_3(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(3, slot, data)
}
fn swap_write_page_4(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(4, slot, data)
}
fn swap_read_page_4(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(4, slot, data)
}
fn swap_write_page_5(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(5, slot, data)
}
fn swap_read_page_5(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(5, slot, data)
}
fn swap_write_page_6(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(6, slot, data)
}
fn swap_read_page_6(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(6, slot, data)
}
fn swap_write_page_7(slot: u64, data: &[u8; 4096]) -> Result<(), &'static str> {
    swap_write_page_impl(7, slot, data)
}
fn swap_read_page_7(slot: u64, data: &mut [u8; 4096]) -> Result<(), &'static str> {
    swap_read_page_impl(7, slot, data)
}

/// Select the write/read fn pointers for a given swap area index.
fn swap_callbacks(
    index: u8,
) -> Option<(
    fn(u64, &[u8; 4096]) -> Result<(), &'static str>,
    fn(u64, &mut [u8; 4096]) -> Result<(), &'static str>,
)> {
    match index {
        0 => Some((swap_write_page_0, swap_read_page_0)),
        1 => Some((swap_write_page_1, swap_read_page_1)),
        2 => Some((swap_write_page_2, swap_read_page_2)),
        3 => Some((swap_write_page_3, swap_read_page_3)),
        4 => Some((swap_write_page_4, swap_read_page_4)),
        5 => Some((swap_write_page_5, swap_read_page_5)),
        6 => Some((swap_write_page_6, swap_read_page_6)),
        7 => Some((swap_write_page_7, swap_read_page_7)),
        _ => None,
    }
}

/// swapon - start swapping to file/device
pub fn swapon(path: *const u8, swapflags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = c_str_to_string(path)?;

    // Look up the file to get its size
    let vfs = crate::vfs::get_vfs();
    let stat = vfs.stat(&path_str).map_err(|_| LinuxError::ENOENT)?;

    let nr_pages = stat.size / 4096;
    if nr_pages == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Open the backing file/device for read/write
    let fd = vfs
        .open(
            &path_str,
            crate::vfs::OpenFlags::new(crate::vfs::OpenFlags::RDWR),
            0,
        )
        .map_err(|_| LinuxError::EACCES)?;

    let index = {
        let areas = crate::swap::swap_stats();
        u8::try_from(areas.nr_areas).map_err(|_| LinuxError::EBUSY)?
    };

    if index > 7 {
        return Err(LinuxError::ENOSPC);
    }

    // Store the fd so the fn-pointer callbacks can find it
    SWAP_FDS.lock().insert(index, fd);

    let (write_fn, read_fn) = swap_callbacks(index).ok_or(LinuxError::ENOSPC)?;

    let area = crate::swap::SwapArea::new(index, swapflags as i32, nr_pages, write_fn, read_fn);
    crate::swap::add_swap_area(area);

    Ok(0)
}

/// swapoff - stop swapping to file/device
pub fn swapoff(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let _path_str = c_str_to_string(path)?;

    // Check if any pages are currently swapped out
    let stats = crate::swap::swap_stats();
    if stats.used_pages > 0 {
        // Cannot disable swap while pages are still swapped out
        return Err(LinuxError::EBUSY);
    }

    // No pages in use — safe to remove swap areas
    // Since our swap module doesn't expose individual area removal,
    // we accept the call and return success when nothing is in use
    Ok(0)
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
pub fn inotify_add_watch(fd: Fd, pathname: *const u8, mask: u32) -> LinuxResult<i32> {
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

    // If a watch already exists for this path, update its mask and
    // return the existing wd (matching Linux inotify_add_watch semantics).
    let existing_wd = instance
        .watches
        .iter()
        .find(|(_, w)| w.path == path)
        .map(|(wd, _)| *wd);
    if let Some(wd) = existing_wd {
        if let Some(w) = instance.watches.get_mut(&wd) {
            w.mask |= mask;
        }
        return Ok(wd);
    }

    let wd = instance.next_wd.fetch_add(1, Ordering::Relaxed);
    instance.watches.insert(wd, InotifyWatch { path, mask });
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
