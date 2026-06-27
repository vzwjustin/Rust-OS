//! `inotify_helper` matching `gio/inotify/inotify-helper.h`.
//!
//! Inotify helper: bridges inotify events to GIO file monitor callbacks.
//! Stubbed in no_std since inotify requires Linux syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::inotify_sub::InotifySub;
use crate::prelude::*;
use alloc::vec::Vec;
use spin::Mutex;

static STARTED: Mutex<bool> = Mutex::new(false);
static SUBS: Mutex<Vec<InotifySub>> = Mutex::new(Vec::new());

/// Starts up the inotify helper (mirrors `_ih_startup`).
/// No-op in our no_std port.
pub fn startup() -> bool {
    *STARTED.lock() = true;
    true
}

/// Adds a subscription (mirrors `_ih_sub_add`).
pub fn sub_add(sub: InotifySub) -> bool {
    if !*STARTED.lock() {
        return false;
    }
    SUBS.lock().push(sub);
    true
}

/// Cancels a subscription (mirrors `_ih_sub_cancel`).
pub fn sub_cancel(dirname: &str, filename: &str) -> bool {
    let mut subs = SUBS.lock();
    if let Some(sub) = subs
        .iter_mut()
        .find(|s| s.dirname == dirname && s.filename == filename)
    {
        sub.cancel();
        true
    } else {
        false
    }
}

/// Returns whether the helper has been started.
pub fn is_started() -> bool {
    *STARTED.lock()
}

/// Returns the number of active subscriptions.
pub fn sub_count() -> usize {
    SUBS.lock().iter().filter(|s| !s.cancelled).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inotify_sub::InotifySub;

    #[test]
    fn test_startup() {
        assert!(startup());
        assert!(is_started());
    }

    #[test]
    fn test_sub_add_and_cancel() {
        startup();
        let sub = InotifySub::new("/tmp", "file.txt", "file.txt");
        assert!(sub_add(sub));
        assert_eq!(sub_count(), 1);
        assert!(sub_cancel("/tmp", "file.txt"));
        assert_eq!(sub_count(), 0);
    }
}
