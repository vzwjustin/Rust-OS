//! `kqueue_helper` matching `gio/kqueue/kqueue-helper.h`.
//!
//! Kqueue helper: manages kqueue subscriptions and directory diffs.
//! Stubbed in no_std since kqueue requires BSD syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::dep_list::DepList;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Kqueue subscription (mirrors `kqueue_sub`).
#[derive(Debug)]
pub struct KqueueSub {
    pub filename: String,
    pub basename: String,
    pub fd: i32,
    pub is_dir: bool,
    pub deps: Option<DepList>,
}

impl KqueueSub {
    /// Creates a new kqueue subscription.
    pub fn new(filename: &str, is_dir: bool) -> Self {
        let basename = filename.rsplit('/').next().unwrap_or(filename).to_string();
        Self {
            filename: filename.into(),
            basename,
            fd: -1,
            is_dir,
            deps: None,
        }
    }
}

static SUBS: Mutex<Vec<KqueueSub>> = Mutex::new(Vec::new());

/// Starts watching a subscription (mirrors `_kqsub_start_watching`).
/// No-op in our no_std port.
pub fn start_watching(sub: KqueueSub) -> bool {
    SUBS.lock().push(sub);
    true
}

/// Performs a directory diff (mirrors `_kh_dir_diff`).
/// No-op in our no_std port.
pub fn dir_diff(_sub: &KqueueSub, _handle_deleted: bool) {}

/// Adds a missing subscription (mirrors `_km_add_missing`).
pub fn add_missing(sub: KqueueSub) {
    SUBS.lock().push(sub);
}

/// Scans missing subscriptions (mirrors `_km_scan_missing`).
/// No-op in our no_std port. Returns false.
pub fn scan_missing() -> bool {
    false
}

/// Removes a subscription (mirrors `_km_remove`).
pub fn remove(filename: &str) {
    SUBS.lock().retain(|s| s.filename != filename);
}

/// Returns the number of active subscriptions.
pub fn sub_count() -> usize {
    SUBS.lock().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sub() {
        let sub = KqueueSub::new("/tmp/test", true);
        assert_eq!(sub.filename, "/tmp/test");
        assert_eq!(sub.basename, "test");
        assert!(sub.is_dir);
        assert_eq!(sub.fd, -1);
    }

    #[test]
    fn test_start_and_remove() {
        let sub = KqueueSub::new("/tmp/file", false);
        assert!(start_watching(sub));
        assert_eq!(sub_count(), 1);
        remove("/tmp/file");
        assert_eq!(sub_count(), 0);
    }

    #[test]
    fn test_scan_missing() {
        assert!(!scan_missing());
    }
}
