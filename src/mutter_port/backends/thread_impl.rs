//! Thread Impl — Thread implementation data from GNOME Mutter
//!
//! Contains implementation-specific thread state and worker task queue.
//!
//! Upstream header not found; minimal stub based on threading architecture.

use alloc::vec::Vec;
use core::ffi::c_void;

/// Thread implementation: worker pool and task management.
/// Manages queued tasks and worker thread lifecycle.
#[derive(Debug, Clone)]
pub struct ThreadImpl {
    /// Worker threads (opaque pointers to GThread)
    pub workers: Vec<*mut c_void>,
    /// Task queue (opaque pointer to GAsyncQueue)
    pub task_queue: *mut c_void,
    /// Whether the implementation is running
    pub running: bool,
}

impl ThreadImpl {
    pub fn new() -> Self {
        ThreadImpl {
            workers: Vec::new(),
            task_queue: core::ptr::null_mut(),
            running: false,
        }
    }
}

impl Default for ThreadImpl {
    fn default() -> Self {
        Self::new()
    }
}
