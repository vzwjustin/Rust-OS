//! New mount API — fsopen, fsconfig, fsmount, fspick, move_mount, open_tree
//!
//! Ported from Linux fs/namespace.c (new mount API).
//!
//! The new mount API uses file descriptors to represent mount contexts:
//! 1. fsopen() creates a filesystem context fd
//! 2. fsconfig() configures the context (set source, flags, etc.)
//! 3. fsmount() creates a mount object from the configured context
//! 4. move_mount() attaches the mount to the filesystem tree
//! 5. fspick() creates a context from an existing mount for reconfiguration

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── fsconfig commands ───────────────────────────────────────────────────

pub const FSCONFIG_SET_FLAG: u32 = 0;
pub const FSCONFIG_SET_STRING: u32 = 1;
pub const FSCONFIG_SET_BINARY: u32 = 2;
pub const FSCONFIG_SET_PATH: u32 = 3;
pub const FSCONFIG_SET_PATH_EMPTY: u32 = 4;
pub const FSCONFIG_SET_FD: u32 = 5;
pub const FSCONFIG_CREATE: u32 = 6;
pub const FSCONFIG_RECONFIGURE: u32 = 7;

// ── fsmount flags ───────────────────────────────────────────────────────

pub const FSMOUNT_CLOEXEC: u32 = 0x00000001;

// ── move_mount flags ────────────────────────────────────────────────────

pub const MOVE_MOUNT_F_SYMLINKS: u32 = 0x00000001;
pub const MOVE_MOUNT_F_AUTOMOUNTS: u32 = 0x00000002;
pub const MOVE_MOUNT_F_EMPTY_PATH: u32 = 0x00000004;
pub const MOVE_MOUNT_T_SYMLINKS: u32 = 0x00000010;
pub const MOVE_MOUNT_T_AUTOMOUNTS: u32 = 0x00000020;
pub const MOVE_MOUNT_T_EMPTY_PATH: u32 = 0x00000040;

// ── open_tree flags ─────────────────────────────────────────────────────

pub const OPEN_TREE_CLONE: u32 = 1;
pub const OPEN_TREE_CLOEXEC: u32 = 0x80000; // O_CLOEXEC

// ── mount setattr flags ─────────────────────────────────────────────────

pub const MOUNT_ATTR_RDONLY: u64 = 0x00000001;
pub const MOUNT_ATTR_NOSUID: u64 = 0x00000002;
pub const MOUNT_ATTR_NODEV: u64 = 0x00000004;
pub const MOUNT_ATTR_NOEXEC: u64 = 0x00000008;
pub const MOUNT_ATTR_NOATIME: u64 = 0x00000010;
pub const MOUNT_ATTR_NODIRATIME: u64 = 0x00000020;
pub const MOUNT_ATTR_RELATIME: u64 = 0x00000040;
pub const MOUNT_ATTR_NOSYMFOLLOW: u64 = 0x00000080;

// ── Mount context state ─────────────────────────────────────────────────

/// A filesystem context (created by fsopen or fspick)
pub struct FsContext {
    pub id: u32,
    pub fs_type: String,
    pub source: String,
    pub flags: u32,
    pub options: Vec<(String, String)>,
    pub mounted: bool,
    pub mount_path: Option<String>,
}

impl FsContext {
    fn new(id: u32, fs_type: &str) -> Self {
        Self {
            id,
            fs_type: String::from(fs_type),
            source: String::new(),
            flags: 0,
            options: Vec::new(),
            mounted: false,
            mount_path: None,
        }
    }

    fn set_string(&mut self, key: &str, value: &str) {
        match key {
            "source" => self.source = String::from(value),
            _ => {
                // Update or add option
                if let Some(opt) = self.options.iter_mut().find(|(k, _)| k == key) {
                    opt.1 = String::from(value);
                } else {
                    self.options.push((String::from(key), String::from(value)));
                }
            }
        }
    }

    fn set_flag(&mut self, key: &str) {
        match key {
            "ro" => self.flags |= MOUNT_ATTR_RDONLY as u32,
            "nosuid" => self.flags |= MOUNT_ATTR_NOSUID as u32,
            "nodev" => self.flags |= MOUNT_ATTR_NODEV as u32,
            "noexec" => self.flags |= MOUNT_ATTR_NOEXEC as u32,
            "noatime" => self.flags |= MOUNT_ATTR_NOATIME as u32,
            "nodiratime" => self.flags |= MOUNT_ATTR_NODIRATIME as u32,
            "relatime" => self.flags |= MOUNT_ATTR_RELATIME as u32,
            "nosymfollow" => self.flags |= MOUNT_ATTR_NOSYMFOLLOW as u32,
            _ => {
                self.options.push((String::from(key), String::from("1")));
            }
        }
    }
}

fn c_string_bytes(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len() + 1);
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    bytes
}

fn mount_attr_to_legacy_flags(attr: u32) -> u64 {
    let mut flags = 0u64;
    if attr & MOUNT_ATTR_RDONLY as u32 != 0 {
        flags |= 1;
    }
    if attr & MOUNT_ATTR_NOSUID as u32 != 0 {
        flags |= 2;
    }
    if attr & MOUNT_ATTR_NODEV as u32 != 0 {
        flags |= 4;
    }
    if attr & MOUNT_ATTR_NOEXEC as u32 != 0 {
        flags |= 8;
    }
    flags
}

// ── Global state ────────────────────────────────────────────────────────

static FS_CONTEXTS: RwLock<BTreeMap<u32, Mutex<FsContext>>> = RwLock::new(BTreeMap::new());
static NEXT_CONTEXT_ID: AtomicU32 = AtomicU32::new(1);

// ── Syscall implementations ─────────────────────────────────────────────

/// fsopen — open a filesystem context
///
/// `fs_type` is the filesystem type (e.g., "ramfs", "proc", "sysfs").
/// `flags` controls behavior (FSOPEN_CLOEXEC).
///
/// Returns a file descriptor on success, negative errno on failure.
pub fn fsopen(fs_type: *const u8, flags: u32) -> i32 {
    if fs_type.is_null() {
        return -14; // EFAULT
    }

    let mut len = 0;
    while unsafe { *fs_type.add(len) } != 0 {
        len += 1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(fs_type, len) };
    let fs_type_str = String::from_utf8_lossy(bytes).into_owned();

    // Validate filesystem type
    let known_types = [
        "ramfs", "proc", "sysfs", "tmpfs", "devtmpfs", "devpts", "overlay", "9p", "ext4", "ext3",
        "ext2", "vfat", "msdos", "fat", "squashfs", "f2fs", "btrfs", "xfs", "iso9660", "nfs",
        "nfs4", "cifs", "smb3",
    ];
    if !known_types.contains(&fs_type_str.as_str()) {
        return -19; // ENODEV
    }

    let id = NEXT_CONTEXT_ID.fetch_add(1, Ordering::SeqCst);
    let ctx = FsContext::new(id, &fs_type_str);
    FS_CONTEXTS.write().insert(id, Mutex::new(ctx));

    let mut fd_flags: u32 = crate::vfs::OpenFlags::RDWR;
    if flags & FSMOUNT_CLOEXEC != 0 {
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }

    let fd = crate::linux_compat::special_fd::register_fs_context(id, fd_flags);
    if fd < 0 {
        FS_CONTEXTS.write().remove(&id);
        return -23; // ENFILE
    }

    crate::serial_println!("[mount_api] fsopen: type={} fd={}", fs_type_str, fd);
    fd
}

/// fsconfig — configure a filesystem context
///
/// `fd` is the fs context fd.
/// `cmd` is the configuration command (FSCONFIG_SET_STRING, etc.).
/// `key` is the parameter name.
/// `value` is the parameter value (or auxiliary data).
/// `aux` is auxiliary data (fd number for FSCONFIG_SET_FD).
///
/// Returns 0 on success, negative errno on failure.
pub fn fsconfig(fd: i32, cmd: u32, key: *const u8, value: *const u8, _aux: i32) -> i32 {
    let id = match crate::linux_compat::special_fd::get_fs_context_id(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let contexts = FS_CONTEXTS.read();
    let ctx_mutex = match contexts.get(&id) {
        Some(c) => c,
        None => return -9,
    };
    let mut ctx = ctx_mutex.lock();

    let key_str = if key.is_null() {
        String::new()
    } else {
        let mut len = 0;
        while unsafe { *key.add(len) } != 0 {
            len += 1;
        }
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(key, len) }).into_owned()
    };

    let value_str = if value.is_null() {
        String::new()
    } else {
        let mut len = 0;
        while unsafe { *value.add(len) } != 0 {
            len += 1;
        }
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(value, len) }).into_owned()
    };

    match cmd {
        FSCONFIG_SET_FLAG => {
            ctx.set_flag(&key_str);
        }
        FSCONFIG_SET_STRING => {
            ctx.set_string(&key_str, &value_str);
        }
        FSCONFIG_SET_BINARY => {
            // Binary parameter — store as hex string
            ctx.set_string(&key_str, "binary");
        }
        FSCONFIG_SET_PATH | FSCONFIG_SET_PATH_EMPTY => {
            ctx.set_string(&key_str, &value_str);
        }
        FSCONFIG_SET_FD => {
            ctx.set_string(&key_str, "fd");
        }
        FSCONFIG_CREATE => {
            // Mark context as ready for mounting
            // Nothing to do — fsmount will use the configured context
        }
        FSCONFIG_RECONFIGURE => {
            // Reconfigure an existing mount
        }
        _ => return -22, // EINVAL
    }

    crate::serial_println!(
        "[mount_api] fsconfig: fd={} cmd={} key={}",
        fd,
        cmd,
        key_str
    );
    0
}

/// fsmount — create a mount object from a configured context
///
/// `fd` is the fs context fd.
/// `flags` controls behavior (FSMOUNT_CLOEXEC).
/// `attr_flags` are mount attributes (MOUNT_ATTR_*).
///
/// Returns a mount fd on success, negative errno on failure.
pub fn fsmount(fd: i32, _flags: u32, attr_flags: u32) -> i32 {
    let id = match crate::linux_compat::special_fd::get_fs_context_id(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let contexts = FS_CONTEXTS.read();
    let ctx_mutex = match contexts.get(&id) {
        Some(c) => c,
        None => return -9,
    };
    let mut ctx = ctx_mutex.lock();

    ctx.flags |= attr_flags;
    ctx.mounted = true;

    // Create a mount fd — this represents a detached mount object
    // It will be attached to the tree via move_mount
    let mount_fd =
        crate::linux_compat::special_fd::register_mount_object(id, crate::vfs::OpenFlags::RDWR);
    if mount_fd < 0 {
        return -23; // ENFILE
    }

    crate::serial_println!("[mount_api] fsmount: fd={} mount_fd={}", fd, mount_fd);
    mount_fd
}

/// fspick — create a context from an existing mount
///
/// `path` is the mount point path.
/// `flags` controls behavior.
///
/// Returns a fs context fd on success, negative errno on failure.
pub fn fspick(path: *const u8, flags: u32) -> i32 {
    if path.is_null() {
        return -14;
    }

    let mut len = 0;
    while unsafe { *path.add(len) } != 0 {
        len += 1;
    }
    let path_str =
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(path, len) }).into_owned();

    // Verify the path is a mount point
    let vfs = crate::vfs::get_vfs();
    match vfs.lookup(&path_str) {
        Ok(_) => {}
        Err(_) => return -2, // ENOENT
    }

    let id = NEXT_CONTEXT_ID.fetch_add(1, Ordering::SeqCst);
    let mut ctx = FsContext::new(id, "unknown");
    ctx.mount_path = Some(path_str.clone());
    ctx.mounted = true;
    FS_CONTEXTS.write().insert(id, Mutex::new(ctx));

    let mut fd_flags: u32 = crate::vfs::OpenFlags::RDWR;
    if flags & 0x80000 != 0 {
        // O_CLOEXEC
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }

    let fd = crate::linux_compat::special_fd::register_fs_context(id, fd_flags);
    if fd < 0 {
        FS_CONTEXTS.write().remove(&id);
        return -23;
    }

    crate::serial_println!("[mount_api] fspick: path={} fd={}", path_str, fd);
    fd
}

/// move_mount — attach a detached mount to the filesystem tree
///
/// `from_dfd` is the mount fd (from fsmount) or AT_FDCWD.
/// `from_path` is the path relative to from_dfd.
/// `to_dfd` is the target directory fd or AT_FDCWD.
/// `to_path` is the target mount point path.
/// `flags` controls behavior.
///
/// Returns 0 on success, negative errno on failure.
pub fn move_mount(
    from_dfd: i32,
    _from_path: *const u8,
    _to_dfd: i32,
    to_path: *const u8,
    _flags: u32,
) -> i32 {
    if to_path.is_null() {
        return -14;
    }

    let mut to_len = 0;
    while unsafe { *to_path.add(to_len) } != 0 {
        to_len += 1;
    }
    let to_path_str =
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(to_path, to_len) })
            .into_owned();

    // Get the fs context from the from_dfd
    let ctx_id = crate::linux_compat::special_fd::get_fs_context_id(from_dfd)
        .or_else(|| crate::linux_compat::special_fd::get_mount_object_id(from_dfd));

    let Some(id) = ctx_id else {
        return -9; // EBADF
    };

    let (fs_type, source, mount_flags) = {
        let contexts = FS_CONTEXTS.read();
        let Some(ctx_mutex) = contexts.get(&id) else {
            return -9;
        };
        let ctx = ctx_mutex.lock();
        (
            ctx.fs_type.clone(),
            ctx.source.clone(),
            mount_attr_to_legacy_flags(ctx.flags),
        )
    };

    let fs_type_bytes = c_string_bytes(&fs_type);
    let target_bytes = c_string_bytes(&to_path_str);
    let source_bytes = if source.is_empty() {
        None
    } else {
        Some(c_string_bytes(&source))
    };
    let source_ptr = source_bytes
        .as_ref()
        .map(|bytes| bytes.as_ptr())
        .unwrap_or(core::ptr::null());

    match crate::linux_compat::fs_ops::mount(
        source_ptr,
        target_bytes.as_ptr(),
        fs_type_bytes.as_ptr(),
        mount_flags,
        core::ptr::null(),
    ) {
        Ok(_) => {
            let contexts = FS_CONTEXTS.read();
            if let Some(ctx_mutex) = contexts.get(&id) {
                let mut ctx = ctx_mutex.lock();
                ctx.mount_path = Some(to_path_str.clone());
                ctx.mounted = true;
            }
            crate::serial_println!("[mount_api] move_mount: ctx={} -> path={}", id, to_path_str);
            0
        }
        Err(e) => -(e as i32),
    }
}

/// open_tree — open a handle to a mount tree
///
/// `dfd` is the directory fd or AT_FDCWD.
/// `path` is the path to the mount point.
/// `flags` controls behavior (OPEN_TREE_CLONE, OPEN_TREE_CLOEXEC).
///
/// Returns a mount fd on success, negative errno on failure.
pub fn open_tree(_dfd: i32, path: *const u8, flags: u32) -> i32 {
    if path.is_null() {
        return -14;
    }

    let mut len = 0;
    while unsafe { *path.add(len) } != 0 {
        len += 1;
    }
    let path_str =
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(path, len) }).into_owned();

    // Verify path exists
    let vfs = crate::vfs::get_vfs();
    match vfs.lookup(&path_str) {
        Ok(_) => {}
        Err(_) => return -2, // ENOENT
    }

    // Create a mount object fd
    let mount_fd =
        crate::linux_compat::special_fd::register_mount_object(0, crate::vfs::OpenFlags::RDWR);
    if mount_fd < 0 {
        return -23;
    }

    crate::serial_println!(
        "[mount_api] open_tree: path={} fd={} clone={}",
        path_str,
        mount_fd,
        flags & OPEN_TREE_CLONE != 0
    );
    mount_fd
}

/// mount_setattr — change mount attributes
///
/// `dfd` is the directory fd or AT_FDCWD.
/// `path` is the mount point path.
/// `flags` controls behavior.
/// `attr` is a pointer to mount_attr struct.
/// `size` is the size of the attr struct.
///
/// Returns 0 on success, negative errno on failure.
pub fn mount_setattr(_dfd: i32, path: *const u8, _flags: u32, attr: u64, size: u64) -> i32 {
    if path.is_null() {
        return -14;
    }

    let mut len = 0;
    while unsafe { *path.add(len) } != 0 {
        len += 1;
    }
    let path_str =
        String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(path, len) }).into_owned();

    if attr == 0 || size == 0 {
        return -22;
    }

    // struct mount_attr { u64 attr_set; u64 attr_clr; u64 propagation; }
    #[repr(C)]
    struct MountAttr {
        attr_set: u64,
        attr_clr: u64,
        propagation: u64,
    }

    let mount_attr = unsafe { &*(attr as *const MountAttr) };

    // Apply the attribute changes to the VFS mount point.
    match crate::fs::vfs().set_mount_flags(&path_str, mount_attr.attr_set, mount_attr.attr_clr) {
        Ok(()) => {
            crate::serial_println!(
                "[mount_api] mount_setattr: path={} set={:#x} clr={:#x}",
                path_str,
                mount_attr.attr_set,
                mount_attr.attr_clr
            );
            0
        }
        Err(e) => {
            crate::serial_println!("[mount_api] mount_setattr: path={} error={:?}", path_str, e);
            -2 // ENOENT
        }
    }
}

/// Close a fs context (called when the fd is closed).
pub fn close_context(id: u32) {
    FS_CONTEXTS.write().remove(&id);
}

/// Initialize the new mount API subsystem.
pub fn init() {
    crate::serial_println!("[mount_api] new mount API subsystem initialized");
}
