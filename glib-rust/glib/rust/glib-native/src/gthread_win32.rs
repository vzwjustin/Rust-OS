//! Win32 thread compatibility (`gthread-win32.c`).

use core::hint::spin_loop;
use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Win32ThreadId(pub u64);

#[must_use]
pub fn current_thread_id() -> Win32ThreadId {
    Win32ThreadId(1)
}

#[must_use]
pub fn allocate_thread_id() -> Win32ThreadId {
    Win32ThreadId(NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn thread_yield() {
    spin_loop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_distinct_ids() {
        assert_eq!(current_thread_id(), Win32ThreadId(1));
        assert_ne!(allocate_thread_id(), allocate_thread_id());
    }
}
