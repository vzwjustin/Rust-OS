//! `inotify_missing` matching `gio/inotify/inotify-missing.h`.
//!
//! Inotify missing list: tracks subscriptions for missing paths,
//! retrying them periodically.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::inotify_sub::InotifySub;
use alloc::vec::Vec;
use spin::Mutex;

static MISSING_LIST: Mutex<Vec<InotifySub>> = Mutex::new(Vec::new());

/// Starts up the missing list (mirrors `_im_startup`).
/// No-op in our no_std port.
pub fn startup() {}

/// Adds a subscription to the missing list (mirrors `_im_add`).
pub fn add(sub: InotifySub) {
    MISSING_LIST.lock().push(sub);
}

/// Removes a subscription from the missing list (mirrors `_im_rm`).
pub fn rm(dirname: &str, filename: &str) {
    MISSING_LIST
        .lock()
        .retain(|s| !(s.dirname == dirname && s.filename == filename));
}

/// Returns the number of missing subscriptions.
pub fn count() -> usize {
    MISSING_LIST.lock().len()
}

/// Dumps diagnostic info (mirrors `_im_diag_dump`).
/// No-op in our no_std port.
pub fn diag_dump() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inotify_sub::InotifySub;

    #[test]
    fn test_add_and_rm() {
        startup();
        add(InotifySub::new("/missing", "file", "file"));
        assert_eq!(count(), 1);
        rm("/missing", "file");
        assert_eq!(count(), 0);
    }
}
