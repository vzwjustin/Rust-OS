//! Thread Implementation — GLib-based background thread.
//!
//! Manages task queuing, event dispatch, and scheduling for background operations
//! (KMS updates, input processing, etc.). Opaque in upstream; implements GInitableIface.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-thread-impl.h

use core::ffi::c_void;

pub struct ThreadImpl {
    /// Underlying MetaThread object (opaque pointer)
    pub thread: *mut c_void,
    /// GLib event loop (opaque pointer to GMainLoop)
    pub loop_handle: *mut c_void,
    /// Whether currently in impl task
    pub in_impl_task: bool,
    /// Thread's GMainContext (opaque pointer to GMainContext)
    pub thread_context: *mut c_void,
    /// Implementation event source (opaque pointer to GSource)
    pub impl_source: *mut c_void,
    /// Task queue (opaque pointer to GAsyncQueue)
    pub task_queue: *mut c_void,
    /// Scheduling priority (u32)
    pub scheduling_priority: u32,
}

impl ThreadImpl {
    pub fn new() -> Self {
        ThreadImpl {
            thread: core::ptr::null_mut(),
            loop_handle: core::ptr::null_mut(),
            in_impl_task: false,
            thread_context: core::ptr::null_mut(),
            impl_source: core::ptr::null_mut(),
            task_queue: core::ptr::null_mut(),
            scheduling_priority: 0,
        }
    }
}

impl Default for ThreadImpl {
    fn default() -> Self {
        Self::new()
    }
}
