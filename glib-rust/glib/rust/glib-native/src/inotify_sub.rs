//! `inotify_sub` matching `gio/inotify/inotify-sub.h`.
//!
//! Inotify subscription: represents a file/directory watch subscription.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;

/// Inotify subscription (mirrors `inotify_sub`).
#[derive(Debug)]
pub struct InotifySub {
    pub dirname: String,
    pub filename: String,
    pub cancelled: bool,
    pub pair_moves: bool,
    pub hardlinks: bool,
}

impl InotifySub {
    /// Creates a new inotify subscription
    /// (mirrors `_ih_sub_new`).
    pub fn new(dirname: &str, basename: &str, filename: &str) -> Self {
        let _ = basename;
        Self {
            dirname: dirname.into(),
            filename: filename.into(),
            cancelled: false,
            pair_moves: false,
            hardlinks: false,
        }
    }

    /// Cancels the subscription.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Returns whether the subscription is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let sub = InotifySub::new("/tmp", "file.txt", "file.txt");
        assert_eq!(sub.dirname, "/tmp");
        assert_eq!(sub.filename, "file.txt");
        assert!(!sub.is_cancelled());
    }

    #[test]
    fn test_cancel() {
        let mut sub = InotifySub::new("/tmp", "f", "f");
        sub.cancel();
        assert!(sub.is_cancelled());
    }
}
