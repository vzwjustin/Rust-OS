//! Thread — Kernel vs user thread abstraction from GNOME Mutter
//!
//! Manages threading for input and rendering tasks.
//! Encapsulates kernel thread or user-level thread scheduling.
//!
//! Upstream header not found; minimal stub based on thread architecture patterns.

use core::ffi::c_void;

/// Thread type selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaThreadType {
    /// Kernel-level thread (OS thread)
    META_THREAD_TYPE_KERNEL = 0,
    /// User-level thread (library-managed)
    META_THREAD_TYPE_USER = 1,
}

/// Thread abstraction for kernel/user-level threading.
/// Provides a unified interface for task scheduling across thread types.
#[derive(Debug, Clone)]
pub struct Thread {
    /// Thread type (kernel or user)
    pub thread_type: MetaThreadType,
    /// Thread implementation (opaque pointer)
    pub impl_ptr: *mut c_void,
}

impl Thread {
    pub fn new(thread_type: MetaThreadType) -> Self {
        Thread {
            thread_type,
            impl_ptr: core::ptr::null_mut(),
        }
    }
}

impl Default for Thread {
    fn default() -> Self {
        Self::new(MetaThreadType::META_THREAD_TYPE_KERNEL)
    }
}
