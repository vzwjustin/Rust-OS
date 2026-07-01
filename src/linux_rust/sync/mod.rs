//! Synchronization primitives.
//!
//! Ported from Linux `rust/kernel/sync/`. C-binding-dependent parts are
//! reimplemented in pure Rust using `core::sync::atomic` and `alloc`.

pub mod arc;
pub mod aref;
pub mod atomic;
pub mod barrier;
pub mod completion;
pub mod condvar;
pub mod lock;
pub mod locked_by;
pub mod poll;
pub mod rcu;
pub mod refcount;
pub mod set_once;

pub use arc::Arc;
pub use aref::{ARef, AlwaysRefCounted};
pub use atomic::Atomic;
pub use barrier::{smp_mb, smp_rmb, smp_wmb};
pub use completion::Completion;
pub use condvar::CondVar;
pub use lock::{Guard, Lock, Mutex, MutexGuard, SpinLock, SpinLockGuard, Backend};
pub use locked_by::LockedBy;
pub use rcu::Guard as RcuGuard;
pub use refcount::Refcount;
pub use set_once::SetOnce;

/// A lock class key.
///
/// In the kernel this is used for lockdep. Here it's a no-op ZST.
#[repr(transparent)]
pub struct LockClassKey(core::marker::PhantomData<()>);

impl LockClassKey {
    pub const fn new() -> Self {
        Self(core::marker::PhantomData)
    }
}

// SAFETY: LockClassKey contains only PhantomData, which is zero-sized and
// has no thread affinity. It is a pure type-level marker for lockdep.
unsafe impl Send for LockClassKey {}
unsafe impl Sync for LockClassKey {}

/// Create a static lock class.
#[macro_export]
macro_rules! static_lock_class {
    () => {{
        static CLASS: $crate::linux_rust::sync::LockClassKey =
            $crate::linux_rust::sync::LockClassKey::new();
        ::core::pin::Pin::static_ref(&CLASS)
    }};
}

/// Optional name macro (returns a static CStr or empty).
#[macro_export]
macro_rules! optional_name {
    ($name:literal) => { $name };
    () => { "" };
}
