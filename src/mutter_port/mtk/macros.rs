//! Port of mutter's `mtk/mtk/mtk-macros.h` to idiomatic Rust.
//!
//! `mtk-macros.h` is a small header of C preprocessor macros used across
//! the Mtk library.
//!
//! # What's ported
//!
//! - `MTK_DEFINE_AUTOPTR_CLEANUP_FUNC` → `AutoPtr<T, F>` wrapper (scope-guard).
//! - `MTK_AVAILABLE_IN_ALL` and version macros → `pub const` no-ops.
//! - `MTK_EXPORT` / `MTK_INTERNAL` → documentation constants.
//!
//! # What's skipped
//!
//! - `G_DEFINE_AUTOPTR_CLEANUP_FUNC` — Rust's `Drop` trait is the equivalent.
//! - `G_DECLARE_FINAL_TYPE` / `G_DEFINE_TYPE` — GObject macros, N/A.

#![allow(dead_code)]

use core::ops::{Deref, DerefMut};

pub const MTK_EXPORT: &str = "pub";
pub const MTK_INTERNAL: &str = "pub(crate)";
pub const MTK_AVAILABLE_IN_ALL: u32 = 0;
pub const fn mtk_available_in(_major: u32, _minor: u32) -> u32 { 0 }
pub const MTK_DEPRECATED: u32 = 0;
pub const fn mtk_deprecated_for(_replacement: &str) -> u32 { 0 }

/// A scope-guard wrapper that calls a cleanup function when dropped.
///
/// This is the Rust equivalent of GLib's `g_autoptr(TypeName)` combined with
/// `MTK_DEFINE_AUTOPTR_CLEANUP_FUNC(TypeName, cleanup_func)`.
#[derive(Debug)]
pub struct AutoPtr<T, F: FnMut(*mut T)> {
    value: *mut T,
    cleanup: Option<F>,
}

impl<T, F: FnMut(*mut T)> AutoPtr<T, F> {
    /// Creates a new `AutoPtr` that owns `value` and will call `cleanup` when dropped.
    ///
    /// # Safety
    ///
    /// The caller must ensure `value` is valid and `cleanup` safely handles it.
    pub unsafe fn new(value: *mut T, cleanup: F) -> Self {
        AutoPtr { value, cleanup: Some(cleanup) }
    }

    /// Releases ownership, returning the raw pointer and preventing cleanup on drop.
    pub fn steal(&mut self) -> *mut T {
        self.cleanup = None;
        let result = self.value;
        self.value = core::ptr::null_mut();
        result
    }

    pub fn as_ptr(&self) -> *mut T { self.value }
    pub fn is_null(&self) -> bool { self.value.is_null() }
}

impl<T, F: FnMut(*mut T)> Deref for AutoPtr<T, F> {
    type Target = *mut T;
    fn deref(&self) -> &Self::Target { &self.value }
}

impl<T, F: FnMut(*mut T)> DerefMut for AutoPtr<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.value }
}

impl<T, F: FnMut(*mut T)> Drop for AutoPtr<T, F> {
    fn drop(&mut self) {
        if let Some(mut cleanup) = self.cleanup.take() {
            // SAFETY: caller of `new` guaranteed cleanup safely handles value.
            unsafe { cleanup(self.value); }
        }
    }
}

/// Documents the Rust equivalent of `MTK_DEFINE_AUTOPTR_CLEANUP_FUNC`. No-op.
pub const fn define_autoptr_cleanup() {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn test_autoptr_calls_cleanup_on_drop() {
        static CALLED: Cell<bool> = Cell::new(false);
        extern "C" fn cleanup(_ptr: *mut i32) { CALLED.set(true); }
        let mut value: i32 = 42;
        // SAFETY: value is valid; cleanup only sets a flag.
        let autoptr = unsafe { AutoPtr::new(&mut value as *mut i32, cleanup) };
        drop(autoptr);
        assert!(CALLED.get());
    }

    #[test]
    fn test_autoptr_steal_prevents_cleanup() {
        static CALLED: Cell<bool> = Cell::new(false);
        extern "C" fn cleanup(_ptr: *mut i32) { CALLED.set(true); }
        let mut value: i32 = 42;
        // SAFETY: value is valid; cleanup only sets a flag.
        let mut autoptr = unsafe { AutoPtr::new(&mut value as *mut i32, cleanup) };
        let stolen = autoptr.steal();
        assert_eq!(unsafe { *stolen }, 42);
        drop(autoptr);
        assert!(!CALLED.get());
    }

    #[test]
    fn test_version_macros_are_noops() {
        assert_eq!(MTK_AVAILABLE_IN_ALL, 0);
        assert_eq!(mtk_available_in(1, 2), 0);
        assert_eq!(MTK_DEPRECATED, 0);
    }
}
