//! Reference-counted boxes matching `grcbox.h` / `grcbox.c`.
//!
//! Provides reference-counted heap allocations using `alloc::sync::Arc`.
//! Fully `no_std` compatible using `alloc`.

use alloc::sync::Arc;
use alloc::boxed::Box;

/// A reference-counted box (`GRcBox`).
///
/// Wraps `Arc<T>` for shared ownership of heap-allocated data.
pub struct RcBox<T> {
    inner: Arc<T>,
}

impl<T> RcBox<T> {
    /// Allocate a new reference-counted box (`g_rc_box_alloc`).
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(value),
        }
    }

    /// Acquire a reference (`g_rc_box_acquire`).
    pub fn acquire(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Get the number of strong references.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Get a reference to the inner value.
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Get the size of the allocation (`g_rc_box_get_size`).
    pub fn size(&self) -> usize {
        core::mem::size_of::<T>()
    }
}

impl<T: Clone> RcBox<T> {
    /// Duplicate from an existing block (`g_rc_box_dup`).
    pub fn dup(data: &T) -> Self {
        Self::new(data.clone())
    }
}

impl<T> Clone for RcBox<T> {
    fn clone(&self) -> Self {
        self.acquire()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for RcBox<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RcBox")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<T> core::ops::Deref for RcBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

/// An atomically reference-counted box (`GAtomicRcBox`).
///
/// Same as `RcBox` since we use `Arc` (which is always atomic).
pub type AtomicRcBox<T> = RcBox<T>;

/// Allocate a zero-initialized reference-counted box (`g_rc_box_alloc0`).
pub fn rc_box_alloc0<T: Default>() -> RcBox<T> {
    RcBox::new(T::default())
}

/// Allocate an atomically reference-counted box (`g_atomic_rc_box_alloc`).
pub fn atomic_rc_box_alloc<T>() -> RcBox<T> {
    RcBox::new(unsafe { core::mem::zeroed() })
}

/// Allocate a zero-initialized atomically reference-counted box (`g_atomic_rc_box_alloc0`).
pub fn atomic_rc_box_alloc0<T: Default>() -> RcBox<T> {
    RcBox::new(T::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let a = RcBox::new(42i32);
        assert_eq!(*a, 42);
        assert_eq!(a.ref_count(), 1);
    }

    #[test]
    fn acquire() {
        let a = RcBox::new(String::from("hello"));
        let b = a.acquire();
        assert_eq!(a.ref_count(), 2);
        assert_eq!(b.get(), "hello");
    }

    #[test]
    fn clone() {
        let a = RcBox::new(10i32);
        let b = a.clone();
        assert_eq!(a.ref_count(), 2);
        assert_eq!(*a, *b);
    }

    #[test]
    fn deref() {
        let a = RcBox::new(vec![1i32, 2, 3]);
        assert_eq!(a.len(), 3);
    }

    #[test]
    fn dup() {
        let original = RcBox::new(99i32);
        let copy = RcBox::dup(&original);
        assert_eq!(*copy, 99);
    }

    #[test]
    fn size() {
        let a = RcBox::new(0u64);
        assert_eq!(a.size(), 8);
    }

    #[test]
    fn alloc0_default() {
        let a: RcBox<i32> = rc_box_alloc0();
        assert_eq!(*a, 0);
    }

    #[test]
    fn atomic_alloc() {
        let a: AtomicRcBox<i32> = atomic_rc_box_alloc();
        let b = a.acquire();
        assert_eq!(a.ref_count(), 2);
    }
}
