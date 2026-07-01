//! Thread Private — Private thread state from GNOME Mutter
//!
//! Manages thread task scheduling and synchronization state.
//!
//! Upstream header not found; minimal stub based on threading patterns.

use core::ffi::c_void;

/// Thread private state: synchronization and task tracking.
/// Used internally by Thread to manage execution state.
#[derive(Debug, Clone)]
pub struct ThreadPrivate {
    /// Mutex for thread state (opaque pointer to GMutex)
    pub mutex: *mut c_void,
    /// Condition variable for task wakeup (opaque pointer to GCond)
    pub cond: *mut c_void,
    /// Current task context (opaque pointer)
    pub current_task: *mut c_void,
}

impl ThreadPrivate {
    pub fn new() -> Self {
        ThreadPrivate {
            mutex: core::ptr::null_mut(),
            cond: core::ptr::null_mut(),
            current_task: core::ptr::null_mut(),
        }
    }
}

impl Default for ThreadPrivate {
    fn default() -> Self {
        Self::new()
    }
}
