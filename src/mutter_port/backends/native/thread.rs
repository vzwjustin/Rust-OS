//! Thread management for kernel and user-facing subsystems.
//!
//! Manages separate execution threads for display (kernel-mode) and
//! input (user-mode) with callback coordination, task queuing, and
//! optional realtime scheduling via D-Bus RTKit.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-thread.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// Thread type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ThreadType {
    /// Kernel-mode display thread
    KERNEL = 0,
    /// User-mode input thread
    USER = 1,
}

/// Scheduling priority enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SchedulingPriority {
    /// Normal OS priority
    NORMAL = 0,
    /// Realtime priority (via RTKit)
    REALTIME = 1,
    /// High priority (elevated but not realtime)
    HIGH_PRIORITY = 2,
}

/// Thread management and scheduling
pub struct Thread {
    /// Parent GObject (opaque)
    pub parent: *mut core::ffi::c_void,
    /// Thread name
    pub name: String,
    /// Main context for this thread (opaque)
    pub main_context: *mut core::ffi::c_void,
    /// Thread implementation (opaque)
    pub impl_thread: *mut core::ffi::c_void,
    /// Preferred scheduling priority
    pub preferred_priority: SchedulingPriority,
    /// Thread type (kernel or user)
    pub thread_type: ThreadType,
    /// Glib thread handle (opaque)
    pub main_thread: *mut core::ffi::c_void,
    /// RTKit proxy (opaque)
    pub rtkit_proxy: *mut core::ffi::c_void,
    /// Realtime scheduling inhibit counter
    pub realtime_inhibit_count: i32,
    /// Callback sources map (opaque)
    pub callback_sources: *mut core::ffi::c_void,
}

impl Thread {
    pub fn new() -> Self {
        Thread {
            parent: core::ptr::null_mut(),
            name: String::new(),
            main_context: core::ptr::null_mut(),
            impl_thread: core::ptr::null_mut(),
            preferred_priority: SchedulingPriority::NORMAL,
            thread_type: ThreadType::USER,
            main_thread: core::ptr::null_mut(),
            rtkit_proxy: core::ptr::null_mut(),
            realtime_inhibit_count: 0,
            callback_sources: core::ptr::null_mut(),
        }
    }
}

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}
