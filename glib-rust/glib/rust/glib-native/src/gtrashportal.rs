//! GTrashPortal matching `gio/gtrashportal.h`.
//! Portal-based trash operations. In this no_std port we model it with
//! a list of trashed file paths.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A trash portal (`GTrashPortal`).
pub struct TrashPortal {
    trashed: Mutex<Vec<String>>,
    available: Mutex<bool>,
}

impl TrashPortal {
    pub fn new() -> Self {
        Self {
            trashed: Mutex::new(Vec::new()),
            available: Mutex::new(false),
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn trash_file(&self, path: &str) -> bool {
        if !*self.available.lock() {
            return false;
        }
        self.trashed.lock().push(path.to_string());
        true
    }

    pub fn get_trashed(&self) -> Vec<String> {
        self.trashed.lock().clone()
    }
    pub fn trashed_count(&self) -> usize {
        self.trashed.lock().len()
    }
}

impl Default for TrashPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trash_unavailable() {
        let p = TrashPortal::new();
        assert!(!p.trash_file("/tmp/test.txt"));
    }

    #[test]
    fn test_trash_file() {
        let p = TrashPortal::new();
        p.set_available(true);
        assert!(p.trash_file("/tmp/test.txt"));
        assert_eq!(p.trashed_count(), 1);
    }
}
