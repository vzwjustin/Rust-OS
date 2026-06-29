//! Inotify — Filesystem event monitoring
//!
//! Ported from Linux fs/notify/inotify/.
//! Provides:
//! - inotify_init1(): create inotify instance
//! - inotify_add_watch(): add a watch on a file/directory
//! - inotify_rm_watch(): remove a watch
//! - Event queue with overflow handling
//!
//! ## Events
//! Watches can monitor: access, modify, attrib, close, open, move,
//! create, delete, unmount, etc.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// ── Event masks (from include/uapi/linux/inotify.h) ─────────────────────

pub const IN_ACCESS: u32 = 0x00000001;
pub const IN_MODIFY: u32 = 0x00000002;
pub const IN_ATTRIB: u32 = 0x00000004;
pub const IN_CLOSE_WRITE: u32 = 0x00000008;
pub const IN_CLOSE_NOWRITE: u32 = 0x00000010;
pub const IN_OPEN: u32 = 0x00000020;
pub const IN_MOVED_FROM: u32 = 0x00000040;
pub const IN_MOVED_TO: u32 = 0x00000080;
pub const IN_CREATE: u32 = 0x00000100;
pub const IN_DELETE: u32 = 0x00000200;
pub const IN_DELETE_SELF: u32 = 0x00000400;
pub const IN_MOVE_SELF: u32 = 0x00000800;

pub const IN_UNMOUNT: u32 = 0x00002000;
pub const IN_Q_OVERFLOW: u32 = 0x00004000;
pub const IN_IGNORED: u32 = 0x00008000;

pub const IN_CLOSE: u32 = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;
pub const IN_MOVE: u32 = IN_MOVED_FROM | IN_MOVED_TO;
pub const IN_ALL_EVENTS: u32 = IN_ACCESS
    | IN_MODIFY
    | IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_CLOSE_NOWRITE
    | IN_OPEN
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_DELETE
    | IN_CREATE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;

// Special flags
pub const IN_ONLYDIR: u32 = 0x01000000;
pub const IN_DONT_FOLLOW: u32 = 0x02000000;
pub const IN_EXCL_UNLINK: u32 = 0x04000000;
pub const IN_MASK_CREATE: u32 = 0x10000000;
pub const IN_MASK_ADD: u32 = 0x20000000;
pub const IN_ISDIR: u32 = 0x40000000;
pub const IN_ONESHOT: u32 = 0x80000000;

// Init flags
pub const IN_CLOEXEC: i32 = 0o2000000;
pub const IN_NONBLOCK: i32 = 0o20000;

// ── Inotify event structure ─────────────────────────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InotifyEvent {
    pub wd: i32,
    pub mask: u32,
    pub cookie: u32,
    pub len: u32,
}

impl InotifyEvent {
    pub fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

// ── Watch descriptor ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Watch {
    pub wd: i32,
    pub path: String,
    pub mask: u32,
    pub is_dir: bool,
}

// ── Inotify instance ────────────────────────────────────────────────────

const MAX_EVENTS: usize = 256;
const MAX_WATCHES: usize = 128;

pub struct InotifyInstance {
    pub id: u32,
    pub watches: Vec<Watch>,
    pub event_queue: VecDeque<(InotifyEvent, Option<String>)>,
    pub overflow: bool,
}

impl InotifyInstance {
    fn new(id: u32) -> Self {
        Self {
            id,
            watches: Vec::new(),
            event_queue: VecDeque::new(),
            overflow: false,
        }
    }

    /// Add a watch. Returns the watch descriptor.
    fn add_watch(&mut self, path: &str, mask: u32) -> Result<i32, &'static str> {
        // Check if watch already exists for this path
        if mask & IN_MASK_ADD != 0 {
            if let Some(w) = self.watches.iter_mut().find(|w| w.path == path) {
                w.mask |= mask & !IN_MASK_ADD;
                return Ok(w.wd);
            }
        }

        if mask & IN_MASK_CREATE != 0 {
            if self.watches.iter().any(|w| w.path == path) {
                return Err("watch already exists");
            }
        }

        if self.watches.len() >= MAX_WATCHES {
            return Err("too many watches");
        }

        let wd = NEXT_WD.fetch_add(1, Ordering::SeqCst) as i32;
        let is_dir = path.ends_with('/');
        self.watches.push(Watch {
            wd,
            path: String::from(path),
            mask: mask & !(IN_MASK_ADD | IN_MASK_CREATE),
            is_dir,
        });

        crate::serial_println!(
            "[inotify] add_watch: wd={} path={} mask={:#x}",
            wd,
            path,
            mask
        );

        Ok(wd)
    }

    /// Remove a watch by watch descriptor.
    fn rm_watch(&mut self, wd: i32) -> Result<(), &'static str> {
        let idx = self
            .watches
            .iter()
            .position(|w| w.wd == wd)
            .ok_or("invalid watch descriptor")?;

        self.watches.remove(idx);

        // Emit IN_IGNORED event
        let event = InotifyEvent {
            wd,
            mask: IN_IGNORED,
            cookie: 0,
            len: 0,
        };
        self.push_event(event, None);

        Ok(())
    }

    /// Push an event into the queue.
    fn push_event(&mut self, event: InotifyEvent, name: Option<String>) {
        if self.event_queue.len() >= MAX_EVENTS {
            self.overflow = true;
            // Push an overflow event
            let overflow = InotifyEvent {
                wd: -1,
                mask: IN_Q_OVERFLOW,
                cookie: 0,
                len: 0,
            };
            self.event_queue.push_back((overflow, None));
            return;
        }

        self.event_queue.push_back((event, name));
    }

    /// Read events from the queue into a buffer.
    /// Returns the number of bytes written.
    fn read_events(&mut self, buf: &mut [u8]) -> usize {
        let mut offset = 0;

        while let Some((event, name)) = self.event_queue.pop_front() {
            let name_bytes = name.as_ref().map(|n| n.as_bytes()).unwrap_or(&[]);
            let name_len = name_bytes.len();
            // Align to 4 bytes
            let padded_len = (name_len + 3) & !3;
            let event_size = InotifyEvent::size() + padded_len;

            if offset + event_size > buf.len() {
                // Put the event back
                self.event_queue.push_front((event, name));
                break;
            }

            // Write the event struct
            unsafe {
                let ptr = buf.as_mut_ptr().add(offset) as *mut InotifyEvent;
                *ptr = InotifyEvent {
                    wd: event.wd,
                    mask: event.mask,
                    cookie: event.cookie,
                    len: padded_len as u32,
                };
            }

            // Write the name (if any)
            if name_len > 0 {
                unsafe {
                    let name_ptr = buf.as_mut_ptr().add(offset + InotifyEvent::size());
                    core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), name_ptr, name_len);
                    // Zero-pad
                    for i in name_len..padded_len {
                        *name_ptr.add(i) = 0;
                    }
                }
            }

            offset += event_size;
        }

        offset
    }

    /// Check if there are events available.
    fn has_events(&self) -> bool {
        !self.event_queue.is_empty()
    }
}

// ── Global state ────────────────────────────────────────────────────────

static INOTIFY_INSTANCES: RwLock<BTreeMap<u32, Mutex<InotifyInstance>>> =
    RwLock::new(BTreeMap::new());
static NEXT_INSTANCE_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_WD: AtomicU32 = AtomicU32::new(1);
static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// inotify_init1 — create a new inotify instance.
/// Returns instance ID (used as fd) on success, negative errno on failure.
pub fn inotify_init1(flags: i32) -> i32 {
    let id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
    let instance = InotifyInstance::new(id);

    INOTIFY_INSTANCES.write().insert(id, Mutex::new(instance));

    // Register as a special fd
    let mut fd_flags: u32 = crate::vfs::OpenFlags::RDWR;
    if flags & IN_CLOEXEC != 0 {
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }
    if flags & IN_NONBLOCK != 0 {
        fd_flags |= crate::vfs::OpenFlags::NONBLOCK;
    }

    // Use the special fd registration from linux_compat
    let fd = crate::linux_compat::special_fd::register_inotify(id, fd_flags);

    crate::serial_println!("[inotify] init1: instance id={} fd={}", id, fd);
    fd
}

/// inotify_add_watch — add a watch to an inotify instance.
pub fn inotify_add_watch(fd: i32, path: &str, mask: u32) -> i32 {
    // Get instance ID from fd
    let instance_id = match crate::linux_compat::special_fd::get_inotify_id(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let instances = INOTIFY_INSTANCES.read();
    let Some(instance_mutex) = instances.get(&instance_id) else {
        return -9;
    };

    let mut instance = instance_mutex.lock();
    match instance.add_watch(path, mask) {
        Ok(wd) => wd,
        Err(e) => {
            crate::serial_println!("[inotify] add_watch error: {}", e);
            -22 // EINVAL
        }
    }
}

/// inotify_rm_watch — remove a watch from an inotify instance.
pub fn inotify_rm_watch(fd: i32, wd: i32) -> i32 {
    let instance_id = match crate::linux_compat::special_fd::get_inotify_id(fd) {
        Some(id) => id,
        None => return -9,
    };

    let instances = INOTIFY_INSTANCES.read();
    let Some(instance_mutex) = instances.get(&instance_id) else {
        return -9;
    };

    let mut instance = instance_mutex.lock();
    match instance.rm_watch(wd) {
        Ok(()) => 0,
        Err(e) => {
            crate::serial_println!("[inotify] rm_watch error: {}", e);
            -22
        }
    }
}

/// Read events from an inotify instance.
pub fn read_events(fd: i32, buf: &mut [u8]) -> i32 {
    let instance_id = match crate::linux_compat::special_fd::get_inotify_id(fd) {
        Some(id) => id,
        None => return -9,
    };

    let instances = INOTIFY_INSTANCES.read();
    let Some(instance_mutex) = instances.get(&instance_id) else {
        return -9;
    };

    let mut instance = instance_mutex.lock();
    if !instance.has_events() {
        return 0; // No events available
    }

    instance.read_events(buf) as i32
}

/// Check if an inotify instance has events pending.
pub fn has_events(fd: i32) -> bool {
    let instance_id = match crate::linux_compat::special_fd::get_inotify_id(fd) {
        Some(id) => id,
        None => return false,
    };

    let instances = INOTIFY_INSTANCES.read();
    instances
        .get(&instance_id)
        .map(|m| m.lock().has_events())
        .unwrap_or(false)
}

// ── Event generation (called by VFS layer) ──────────────────────────────

/// Notify all inotify instances watching a path about an event.
/// Called by the VFS when a file operation occurs.
pub fn notify_path(path: &str, mask: u32, name: Option<&str>) {
    EVENT_COUNT.fetch_add(1, Ordering::Relaxed);

    let instances = INOTIFY_INSTANCES.read();
    for (_, instance_mutex) in instances.iter() {
        let mut instance = instance_mutex.lock();

        // Find watches that match this path and mask
        let matching_watches: Vec<(i32, u32)> = instance
            .watches
            .iter()
            .filter(|w| {
                // Check if the event path is under the watched path
                let watched = &w.path;
                let event_path = path;

                // Exact match or the watched path is a parent directory
                event_path == watched
                    || event_path.starts_with(watched)
                    || (watched.ends_with('/') && event_path.starts_with(watched))
            })
            .filter(|w| w.mask & mask != 0)
            .map(|w| (w.wd, w.mask))
            .collect();

        for (wd, watch_mask) in matching_watches {
            let event = InotifyEvent {
                wd,
                mask: mask & watch_mask,
                cookie: 0,
                len: 0,
            };
            instance.push_event(event, name.map(|s| String::from(s)));

            // If IN_ONESHOT is set, remove the watch after the event
            if watch_mask & IN_ONESHOT != 0 {
                instance.watches.retain(|w| w.wd != wd);
            }
        }
    }
}

/// Convenience function to notify a file access event.
pub fn notify_access(path: &str) {
    notify_path(path, IN_ACCESS, None);
}

/// Convenience function to notify a file modification event.
pub fn notify_modify(path: &str) {
    notify_path(path, IN_MODIFY, None);
}

/// Convenience function to notify a file creation event in a directory.
pub fn notify_create(dir: &str, filename: &str) {
    notify_path(dir, IN_CREATE, Some(filename));
}

/// Convenience function to notify a file deletion event in a directory.
pub fn notify_delete(dir: &str, filename: &str) {
    notify_path(dir, IN_DELETE, Some(filename));
}

/// Convenience function to notify a file open event.
pub fn notify_open(path: &str) {
    notify_path(path, IN_OPEN, None);
}

/// Convenience function to notify a file close event.
pub fn notify_close(path: &str, writable: bool) {
    let mask = if writable {
        IN_CLOSE_WRITE
    } else {
        IN_CLOSE_NOWRITE
    };
    notify_path(path, mask, None);
}

/// Convenience function to notify a file attribute change.
pub fn notify_attrib(path: &str) {
    notify_path(path, IN_ATTRIB, None);
}

/// Convenience function to notify a file move.
pub fn notify_move(from_dir: &str, to_dir: &str, filename: &str, cookie: u32) {
    notify_path(from_dir, IN_MOVED_FROM, Some(filename));
    notify_path(to_dir, IN_MOVED_TO, Some(filename));
}

// ── Cleanup ─────────────────────────────────────────────────────────────

/// Destroy an inotify instance.
pub fn destroy_instance(id: u32) {
    INOTIFY_INSTANCES.write().remove(&id);
    crate::serial_println!("[inotify] destroyed instance id={}", id);
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[inotify] inotify subsystem initialized");
}

// ── Statistics ──────────────────────────────────────────────────────────

pub fn event_count() -> u64 {
    EVENT_COUNT.load(Ordering::Relaxed)
}

pub fn instance_count() -> usize {
    INOTIFY_INSTANCES.read().len()
}
