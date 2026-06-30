//! `inotify_path` matching `gio/inotify/inotify-path.h`.
//!
//! Inotify path tracking: manages watch descriptors per directory path
//! and dispatches events to subscriptions.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::inotify_kernel::{self};
use crate::inotify_sub::InotifySub;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

static PATH_WATCHES: Mutex<Vec<(i32, String, Vec<InotifySub>)>> = Mutex::new(Vec::new());

/// Starts up the path tracker (mirrors `_ip_startup`).
/// No-op in our no_std port.
pub fn startup() -> bool {
    inotify_kernel::startup()
}

/// Starts watching a subscription's directory (mirrors `_ip_start_watching`).
pub fn start_watching(sub: InotifySub) -> bool {
    let path = sub.dirname.clone();
    let mask = inotify_kernel::IN_MODIFY
        | inotify_kernel::IN_ATTRIB
        | inotify_kernel::IN_CLOSE_WRITE
        | inotify_kernel::IN_MOVED_FROM
        | inotify_kernel::IN_MOVED_TO
        | inotify_kernel::IN_CREATE
        | inotify_kernel::IN_DELETE
        | inotify_kernel::IN_DELETE_SELF
        | inotify_kernel::IN_MOVE_SELF;

    let wd = inotify_kernel::watch(&path, mask);
    if wd < 0 {
        return false;
    }

    let mut watches = PATH_WATCHES.lock();
    if let Some((_, _, subs)) = watches.iter_mut().find(|(w, _, _)| *w == wd) {
        subs.push(sub);
    } else {
        watches.push((wd, path, vec![sub]));
    }
    true
}

/// Stops watching a subscription's directory (mirrors `_ip_stop_watching`).
pub fn stop_watching(dirname: &str, filename: &str) -> bool {
    let mut watches = PATH_WATCHES.lock();
    let mut found = false;
    for (_, _, subs) in watches.iter_mut() {
        if let Some(idx) = subs
            .iter()
            .position(|s| s.filename == filename && s.dirname == dirname)
        {
            subs.remove(idx);
            found = true;
            break;
        }
    }
    watches.retain(|(_, _, subs)| !subs.is_empty());
    found
}

/// Gets the path for a watch descriptor (mirrors `_ip_get_path_for_wd`).
pub fn get_path_for_wd(wd: i32) -> Option<String> {
    PATH_WATCHES
        .lock()
        .iter()
        .find(|(w, _, _)| *w == wd)
        .map(|(_, p, _)| p.clone())
}

/// Returns the number of active path watches.
pub fn watch_count() -> usize {
    PATH_WATCHES.lock().len()
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    PATH_WATCHES.lock().clear();
    inotify_kernel::reset_for_test();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inotify_sub::InotifySub;

    #[test]
    fn test_start_and_stop_watching() {
        reset_for_test();
        startup();
        let sub = InotifySub::new("/tmp", "file.txt", "file.txt");
        assert!(start_watching(sub));
        assert_eq!(watch_count(), 1);
        assert!(stop_watching("/tmp", "file.txt"));
        assert_eq!(watch_count(), 0);
    }

    #[test]
    fn test_get_path_for_wd() {
        reset_for_test();
        startup();
        let sub = InotifySub::new("/var", "log", "log");
        assert!(start_watching(sub));
        let watches = PATH_WATCHES.lock();
        let wd = watches.first().map(|(w, _, _)| *w);
        drop(watches);
        if let Some(wd) = wd {
            assert_eq!(get_path_for_wd(wd), Some("/var".to_string()));
        }
        assert!(stop_watching("/var", "log"));
    }
}
