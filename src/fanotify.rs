//! fanotify — advanced filesystem event monitoring
//!
//! Ported from Linux fs/notify/fanotify/ (fanotify_user.c, fanotify.c).
//! Provides:
//! - fanotify_init(): create a fanotify instance
//! - fanotify_mark(): add/remove marks on filesystem objects
//!
//! fanotify is more powerful than inotify: it supports mount/filesystem-wide
//! monitoring, permission events, and reports file descriptors or file IDs
//! in events.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── Event masks (from include/uapi/linux/fanotify.h) ────────────────────

pub const FAN_ACCESS: u64 = 0x00000001;
pub const FAN_MODIFY: u64 = 0x00000002;
pub const FAN_ATTRIB: u64 = 0x00000004;
pub const FAN_CLOSE_WRITE: u64 = 0x00000008;
pub const FAN_CLOSE_NOWRITE: u64 = 0x00000010;
pub const FAN_OPEN: u64 = 0x00000020;
pub const FAN_MOVED_FROM: u64 = 0x00000040;
pub const FAN_MOVED_TO: u64 = 0x00000080;
pub const FAN_CREATE: u64 = 0x00000100;
pub const FAN_DELETE: u64 = 0x00000200;
pub const FAN_DELETE_SELF: u64 = 0x00000400;
pub const FAN_MOVE_SELF: u64 = 0x00000800;
pub const FAN_OPEN_EXEC: u64 = 0x00001000;
pub const FAN_Q_OVERFLOW: u64 = 0x00004000;
pub const FAN_FS_ERROR: u64 = 0x00008000;
pub const FAN_OPEN_PERM: u64 = 0x00010000;
pub const FAN_ACCESS_PERM: u64 = 0x00020000;
pub const FAN_OPEN_EXEC_PERM: u64 = 0x00040000;
pub const FAN_PRE_ACCESS: u64 = 0x00100000;
pub const FAN_MNT_ATTACH: u64 = 0x01000000;
pub const FAN_MNT_DETACH: u64 = 0x02000000;
pub const FAN_EVENT_ON_CHILD: u64 = 0x08000000;
pub const FAN_RENAME: u64 = 0x10000000;
pub const FAN_ONDIR: u64 = 0x40000000;

pub const FAN_CLOSE: u64 = FAN_CLOSE_WRITE | FAN_CLOSE_NOWRITE;
pub const FAN_MOVE: u64 = FAN_MOVED_FROM | FAN_MOVED_TO;

// ── Init flags ──────────────────────────────────────────────────────────

pub const FAN_CLOEXEC: u32 = 0x00000001;
pub const FAN_NONBLOCK: u32 = 0x00000002;
pub const FAN_CLASS_NOTIF: u32 = 0x00000000;
pub const FAN_CLASS_CONTENT: u32 = 0x00000004;
pub const FAN_CLASS_PRE_CONTENT: u32 = 0x00000008;
pub const FAN_UNLIMITED_QUEUE: u32 = 0x00000010;
pub const FAN_UNLIMITED_MARKS: u32 = 0x00000020;
pub const FAN_ENABLE_AUDIT: u32 = 0x00000040;
pub const FAN_REPORT_PIDFD: u32 = 0x00000080;
pub const FAN_REPORT_TID: u32 = 0x00000100;
pub const FAN_REPORT_FID: u32 = 0x00000200;
pub const FAN_REPORT_DIR_FID: u32 = 0x00000400;
pub const FAN_REPORT_NAME: u32 = 0x00000800;
pub const FAN_REPORT_TARGET_FID: u32 = 0x00001000;
pub const FAN_REPORT_FD_ERROR: u32 = 0x00002000;
pub const FAN_REPORT_MNT: u32 = 0x00004000;

// ── Mark flags ──────────────────────────────────────────────────────────

pub const FAN_MARK_ADD: u32 = 0x00000001;
pub const FAN_MARK_REMOVE: u32 = 0x00000002;
pub const FAN_MARK_DONT_FOLLOW: u32 = 0x00000004;
pub const FAN_MARK_ONLYDIR: u32 = 0x00000008;
pub const FAN_MARK_MOUNT: u32 = 0x00000010;
pub const FAN_MARK_IGNORED_MASK: u32 = 0x00000020;
pub const FAN_MARK_IGNORED_SURV_MODIFY: u32 = 0x00000040;
pub const FAN_MARK_FLUSH: u32 = 0x00000080;
pub const FAN_MARK_FILESYSTEM: u32 = 0x00000100;
pub const FAN_MARK_EVICTABLE: u32 = 0x00000200;
pub const FAN_MARK_IGNORE: u32 = 0x00000400;
pub const FAN_MARK_MNTNS: u32 = 0x00000110;

// ── Event metadata (from include/uapi/linux/fanotify.h) ─────────────────

pub const FANOTIFY_METADATA_VERSION: u8 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FanotifyEventMetadata {
    pub event_len: u32,
    pub vers: u8,
    pub reserved: u8,
    pub metadata_len: u16,
    pub mask: u64,
    pub fd: i32,
    pub pid: i32,
}

// ── Fanotify instance state ─────────────────────────────────────────────

/// A mark on a filesystem object
#[derive(Clone, Debug)]
pub struct FanotifyMark {
    pub mask: u64,
    pub mark_type: u32, // FAN_MARK_INODE, FAN_MARK_MOUNT, FAN_MARK_FILESYSTEM
    pub path: String,
    pub ignored_mask: u64,
}

/// A fanotify instance
pub struct FanotifyInstance {
    pub id: u32,
    pub flags: u32,
    pub event_flags: u32,
    pub marks: Vec<FanotifyMark>,
    pub event_queue: VecDeque<FanotifyEventMetadata>,
    pub max_events: usize,
    pub overflow: bool,
}

impl FanotifyInstance {
    fn new(id: u32, flags: u32, event_flags: u32) -> Self {
        let max_events = if flags & FAN_UNLIMITED_QUEUE != 0 {
            65536
        } else {
            16384
        };
        Self {
            id,
            flags,
            event_flags,
            marks: Vec::new(),
            event_queue: VecDeque::with_capacity(max_events),
            max_events,
            overflow: false,
        }
    }

    fn add_mark(&mut self, mask: u64, mark_type: u32, path: &str) {
        // Check if mark already exists for this path
        if let Some(existing) = self
            .marks
            .iter_mut()
            .find(|m| m.path == path && m.mark_type == mark_type)
        {
            existing.mask |= mask;
        } else {
            self.marks.push(FanotifyMark {
                mask,
                mark_type,
                path: String::from(path),
                ignored_mask: 0,
            });
        }
    }

    fn remove_mark(&mut self, mask: u64, mark_type: u32, path: &str) {
        if let Some(existing) = self
            .marks
            .iter_mut()
            .find(|m| m.path == path && m.mark_type == mark_type)
        {
            existing.mask &= !mask;
            if existing.mask == 0 {
                self.marks
                    .retain(|m| m.path != path || m.mark_type != mark_type);
            }
        }
    }

    fn flush_marks(&mut self) {
        self.marks.clear();
    }

    fn push_event(&mut self, event: FanotifyEventMetadata) {
        if self.event_queue.len() >= self.max_events {
            self.overflow = true;
            // Push overflow event
            self.event_queue.push_back(FanotifyEventMetadata {
                event_len: core::mem::size_of::<FanotifyEventMetadata>() as u32,
                vers: FANOTIFY_METADATA_VERSION,
                reserved: 0,
                metadata_len: core::mem::size_of::<FanotifyEventMetadata>() as u16,
                mask: FAN_Q_OVERFLOW,
                fd: -1,
                pid: 0,
            });
            return;
        }
        self.event_queue.push_back(event);
    }
}

// ── Global state ────────────────────────────────────────────────────────

static INSTANCES: RwLock<BTreeMap<u32, Mutex<FanotifyInstance>>> = RwLock::new(BTreeMap::new());
static NEXT_INSTANCE_ID: AtomicU32 = AtomicU32::new(1);

// ── Syscall implementations ─────────────────────────────────────────────

/// fanotify_init — create a fanotify instance
///
/// `flags` controls instance behavior (FAN_CLOEXEC, FAN_NONBLOCK, FAN_CLASS_*).
/// `eventflags` is currently unused (must be 0 in most kernels).
///
/// Returns a file descriptor on success, negative errno on failure.
pub fn fanotify_init(flags: u32, event_flags: u32) -> i32 {
    // Only root can create fanotify instances (CAP_SYS_ADMIN)
    let pid = crate::process::current_pid();
    let pm = crate::process::get_process_manager();
    if let Some(pcb) = pm.get_process(pid) {
        if pcb.euid != 0 {
            return -1; // EPERM
        }
    } else {
        return -3; // ESRCH
    }

    let supported_flags = FAN_CLOEXEC
        | FAN_NONBLOCK
        | FAN_CLASS_NOTIF
        | FAN_CLASS_CONTENT
        | FAN_CLASS_PRE_CONTENT
        | FAN_UNLIMITED_QUEUE
        | FAN_UNLIMITED_MARKS
        | FAN_ENABLE_AUDIT
        | FAN_REPORT_PIDFD
        | FAN_REPORT_TID
        | FAN_REPORT_FID
        | FAN_REPORT_DIR_FID
        | FAN_REPORT_NAME
        | FAN_REPORT_TARGET_FID
        | FAN_REPORT_FD_ERROR
        | FAN_REPORT_MNT;

    if flags & !supported_flags != 0 {
        return -22; // EINVAL
    }

    // Only one class bit allowed
    let class_bits = flags & (FAN_CLASS_NOTIF | FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT);
    if class_bits != FAN_CLASS_NOTIF
        && class_bits != FAN_CLASS_CONTENT
        && class_bits != FAN_CLASS_PRE_CONTENT
    {
        // Multiple class bits set (NOTIF is 0, so check only CONTENT|PRE_CONTENT)
        if (flags & (FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT))
            == (FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT)
        {
            return -22;
        }
    }

    let id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
    let instance = FanotifyInstance::new(id, flags, event_flags);
    INSTANCES.write().insert(id, Mutex::new(instance));

    let mut fd_flags: u32 = crate::vfs::OpenFlags::RDWR;
    if flags & FAN_CLOEXEC != 0 {
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }
    if flags & FAN_NONBLOCK != 0 {
        fd_flags |= crate::vfs::OpenFlags::NONBLOCK;
    }

    let fd = crate::linux_compat::special_fd::register_fanotify(id, fd_flags);
    if fd < 0 {
        INSTANCES.write().remove(&id);
        return -23; // ENFILE
    }

    crate::serial_println!("[fanotify] init: flags={:#x} fd={}", flags, fd);
    fd
}

/// fanotify_mark — add, remove, or modify marks on filesystem objects
///
/// `fanotify_fd` is the fanotify instance fd.
/// `flags` controls the operation (FAN_MARK_ADD, FAN_MARK_REMOVE, FAN_MARK_FLUSH).
/// `mask` is the event mask to watch.
/// `dirfd` is AT_FDCWD or a directory fd (for relative paths).
/// `path` is the filesystem path to mark.
///
/// Returns 0 on success, negative errno on failure.
pub fn fanotify_mark(fanotify_fd: i32, flags: u32, mask: u64, dirfd: i32, path: *const u8) -> i32 {
    let id = match crate::linux_compat::special_fd::get_fanotify_id(fanotify_fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let instances = INSTANCES.read();
    let instance_mutex = match instances.get(&id) {
        Some(i) => i,
        None => return -9,
    };
    let mut inst = instance_mutex.lock();

    // Handle FAN_MARK_FLUSH
    if flags & FAN_MARK_FLUSH != 0 {
        inst.flush_marks();
        return 0;
    }

    // Validate flags
    let mark_flags = flags & (FAN_MARK_ADD | FAN_MARK_REMOVE);
    if mark_flags != FAN_MARK_ADD && mark_flags != FAN_MARK_REMOVE {
        return -22; // EINVAL
    }

    // Determine mark type
    let mark_type = if flags & FAN_MARK_FILESYSTEM != 0 {
        FAN_MARK_FILESYSTEM
    } else if flags & FAN_MARK_MOUNT != 0 {
        FAN_MARK_MOUNT
    } else {
        0 // FAN_MARK_INODE
    };

    // Get path string
    let path_str = if path.is_null() {
        return -14; // EFAULT
    } else {
        let mut len = 0;
        while unsafe { *path.add(len) } != 0 {
            len += 1;
        }
        let bytes = unsafe { core::slice::from_raw_parts(path, len) };
        String::from_utf8_lossy(bytes).into_owned()
    };

    // Handle dirfd for relative paths
    let full_path = if path_str.starts_with('/') {
        path_str
    } else if dirfd == -100 {
        // AT_FDCWD
        String::from("/") + &path_str
    } else {
        // Would need to resolve dirfd — simplified
        String::from("/") + &path_str
    };

    if flags & FAN_MARK_ADD != 0 {
        inst.add_mark(mask, mark_type, &full_path);
    } else if flags & FAN_MARK_REMOVE != 0 {
        inst.remove_mark(mask, mark_type, &full_path);
    }

    crate::serial_println!(
        "[fanotify] mark: fd={} flags={:#x} mask={:#x} path={}",
        fanotify_fd,
        flags,
        mask,
        full_path
    );
    0
}

/// Generate a fanotify event for a path (called by VFS layer).
pub fn notify_path(path: &str, event_mask: u64, fd: i32, pid: u32) {
    let instances = INSTANCES.read();
    for (_id, instance_mutex) in instances.iter() {
        let mut inst = instance_mutex.lock();
        // Check if any mark matches this path
        let matching = inst
            .marks
            .iter()
            .any(|m| (m.mask & event_mask) != 0 && (path.starts_with(&m.path) || path == &m.path));
        if matching {
            let event = FanotifyEventMetadata {
                event_len: core::mem::size_of::<FanotifyEventMetadata>() as u32,
                vers: FANOTIFY_METADATA_VERSION,
                reserved: 0,
                metadata_len: core::mem::size_of::<FanotifyEventMetadata>() as u16,
                mask: event_mask,
                fd,
                pid: pid as i32,
            };
            inst.push_event(event);
        }
    }
}

/// Read events from a fanotify fd.
pub fn read_events(fd: i32, buf: &mut [u8]) -> isize {
    let id = match crate::linux_compat::special_fd::get_fanotify_id(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let instances = INSTANCES.read();
    let instance_mutex = match instances.get(&id) {
        Some(i) => i,
        None => return -9,
    };
    let mut inst = instance_mutex.lock();

    let event_size = core::mem::size_of::<FanotifyEventMetadata>();
    let max_events = buf.len() / event_size;
    let mut offset = 0;
    let mut count = 0;

    while count < max_events {
        match inst.event_queue.pop_front() {
            Some(event) => {
                let bytes = unsafe {
                    core::slice::from_raw_parts(&event as *const _ as *const u8, event_size)
                };
                buf[offset..offset + event_size].copy_from_slice(bytes);
                offset += event_size;
                count += 1;
            }
            None => break,
        }
    }

    if count == 0 {
        return -11; // EAGAIN
    }

    offset as isize
}

/// Close a fanotify instance.
pub fn close_instance(id: u32) {
    INSTANCES.write().remove(&id);
}

/// Return whether a fanotify instance has events ready to read.
pub fn has_events(id: u32) -> bool {
    let instances = INSTANCES.read();
    let Some(instance_mutex) = instances.get(&id) else {
        return false;
    };
    let has_events = !instance_mutex.lock().event_queue.is_empty();
    has_events
}

/// Initialize the fanotify subsystem.
pub fn init() {
    crate::serial_println!("[fanotify] fanotify subsystem initialized");
}
