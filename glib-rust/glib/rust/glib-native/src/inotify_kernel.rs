//! `inotify_kernel` matching `gio/inotify/inotify-kernel.h`.
//!
//! Inotify kernel interface: wraps Linux inotify syscalls.
//! Stubbed in no_std since inotify requires Linux syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Inotify event (mirrors `ik_event_t`).
#[derive(Debug, Clone)]
pub struct IkEvent {
    pub wd: i32,
    pub mask: u32,
    pub original_mask: u32,
    pub cookie: u32,
    pub len: u32,
    pub name: String,
    pub is_second_in_pair: bool,
    pub timestamp: i64,
}

impl IkEvent {
    /// Creates a new dummy event (mirrors `_ik_event_new_dummy`).
    pub fn new_dummy(name: &str, wd: i32, mask: u32) -> Self {
        Self {
            wd,
            mask,
            original_mask: mask,
            cookie: 0,
            len: name.len() as u32,
            name: name.into(),
            is_second_in_pair: false,
            timestamp: 0,
        }
    }
}

/// Inotify mask flags.
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
pub const IN_ISDIR: u32 = 0x40000000;

static WATCH_ID: Mutex<i32> = Mutex::new(0);
static WATCHES: Mutex<Vec<(i32, String, u32)>> = Mutex::new(Vec::new());
static MOVE_MATCHES: Mutex<u32> = Mutex::new(0);
static MOVE_MISSES: Mutex<u32> = Mutex::new(0);

/// Starts up the inotify kernel interface (mirrors `_ik_startup`).
/// No-op in our no_std port.
pub fn startup() -> bool {
    true
}

/// Adds a watch (mirrors `_ik_watch`).
/// Returns a watch descriptor, or -1 on error.
pub fn watch(path: &str, mask: u32) -> i32 {
    let mut id = WATCH_ID.lock();
    *id += 1;
    let wd = *id;
    WATCHES.lock().push((wd, path.into(), mask));
    wd
}

/// Removes a watch (mirrors `_ik_ignore`).
pub fn ignore(path: &str, wd: i32) -> bool {
    let mut watches = WATCHES.lock();
    let len_before = watches.len();
    watches.retain(|(w, p, _)| !(*w == wd || p == path));
    watches.len() < len_before
}

/// Returns move statistics (mirrors `_ik_move_stats`).
pub fn move_stats() -> (u32, u32) {
    (*MOVE_MATCHES.lock(), *MOVE_MISSES.lock())
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    *WATCH_ID.lock() = 0;
    WATCHES.lock().clear();
    *MOVE_MATCHES.lock() = 0;
    *MOVE_MISSES.lock() = 0;
}

/// Converts a mask to a string representation (mirrors `_ik_mask_to_string`).
pub fn mask_to_string(mask: u32) -> String {
    let mut parts = Vec::new();
    if mask & IN_ACCESS != 0 {
        parts.push("ACCESS");
    }
    if mask & IN_MODIFY != 0 {
        parts.push("MODIFY");
    }
    if mask & IN_ATTRIB != 0 {
        parts.push("ATTRIB");
    }
    if mask & IN_CLOSE_WRITE != 0 {
        parts.push("CLOSE_WRITE");
    }
    if mask & IN_CLOSE_NOWRITE != 0 {
        parts.push("CLOSE_NOWRITE");
    }
    if mask & IN_OPEN != 0 {
        parts.push("OPEN");
    }
    if mask & IN_MOVED_FROM != 0 {
        parts.push("MOVED_FROM");
    }
    if mask & IN_MOVED_TO != 0 {
        parts.push("MOVED_TO");
    }
    if mask & IN_CREATE != 0 {
        parts.push("CREATE");
    }
    if mask & IN_DELETE != 0 {
        parts.push("DELETE");
    }
    if mask & IN_DELETE_SELF != 0 {
        parts.push("DELETE_SELF");
    }
    if mask & IN_MOVE_SELF != 0 {
        parts.push("MOVE_SELF");
    }
    if mask & IN_ISDIR != 0 {
        parts.push("ISDIR");
    }
    parts.join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_dummy_event() {
        let e = IkEvent::new_dummy("file.txt", 1, IN_MODIFY);
        assert_eq!(e.wd, 1);
        assert_eq!(e.name, "file.txt");
        assert_eq!(e.mask, IN_MODIFY);
    }

    #[test]
    fn test_watch_and_ignore() {
        startup();
        let wd = watch("/tmp", IN_MODIFY);
        assert!(wd > 0);
        assert!(ignore("/tmp", wd));
    }

    #[test]
    fn test_mask_to_string() {
        assert_eq!(mask_to_string(IN_MODIFY), "MODIFY");
        assert_eq!(mask_to_string(IN_MODIFY | IN_ISDIR), "MODIFY|ISDIR");
        assert_eq!(mask_to_string(0), "");
    }

    #[test]
    fn test_move_stats() {
        let (m, n) = move_stats();
        let _ = (m, n);
    }
}
