//! `kqueue_missing` matching `gio/kqueue/kqueue-missing.h`.
//!
//! Kqueue missing list: tracks subscriptions for missing paths,
//! retrying them periodically.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::kqueue_helper::KqueueSub;
use crate::prelude::*;
use alloc::vec::Vec;
use spin::Mutex;

static MISSING: Mutex<Vec<KqueueSub>> = Mutex::new(Vec::new());

/// Adds a subscription to the missing list (mirrors `_km_add_missing`).
pub fn add_missing(sub: KqueueSub) {
    MISSING.lock().push(sub);
}

/// Scans missing subscriptions (mirrors `_km_scan_missing`).
/// No-op in our no_std port. Returns false.
pub fn scan_missing() -> bool {
    false
}

/// Removes a subscription from the missing list (mirrors `_km_remove`).
pub fn remove(filename: &str) {
    MISSING.lock().retain(|s| s.filename != filename);
}

/// Returns the number of missing subscriptions.
pub fn count() -> usize {
    MISSING.lock().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kqueue_helper::KqueueSub;

    #[test]
    fn test_add_and_remove() {
        add_missing(KqueueSub::new("/missing/path", false));
        assert_eq!(count(), 1);
        remove("/missing/path");
        assert_eq!(count(), 0);
    }

    #[test]
    fn test_scan_missing() {
        assert!(!scan_missing());
    }
}
