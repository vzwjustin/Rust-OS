//! Thread primitives matching `gthread.h` / `gthread.c`.
//!
//! Uses `spin` crate for mutex, RW lock, and once initialization.
//! Thread creation/joining requires OS support and is deferred.
//! Fully `no_std` compatible using `spin`.

use spin::{Mutex, RwLock, Once as SpinOnce};

/// Thread error (`GThreadError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadError {
    Again,
}

/// Once status (`GOnceStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OnceStatus {
    NotCalled,
    Progress,
    Ready,
}

/// A once-initialized value (`GOnce`).
pub struct Once<T> {
    inner: SpinOnce<T>,
}

impl<T> Once<T> {
    /// Create a new once-initialized value.
    pub const fn new() -> Self {
        Self {
            inner: SpinOnce::new(),
        }
    }

    /// Get or initialize the value (`g_once`).
    pub fn call_once<F>(&self, init: F) -> &T
    where
        F: FnOnce() -> T,
    {
        self.inner.call_once(init)
    }

    /// Check if the value has been initialized.
    pub fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }
}

/// A mutex (`GMutex`).
///
/// Wraps `spin::Mutex<()>` for lock/unlock semantics.
pub struct GMutex<T = ()> {
    inner: Mutex<T>,
}

impl<T> GMutex<T> {
    /// Create a new mutex (`g_mutex_init`).
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    /// Lock the mutex (`g_mutex_lock`).
    pub fn lock(&self) -> spin::MutexGuard<T> {
        self.inner.lock()
    }

    /// Try to lock the mutex (`g_mutex_trylock`).
    pub fn try_lock(&self) -> Option<spin::MutexGuard<T>> {
        self.inner.try_lock()
    }

}

impl GMutex<()> {
    /// Create a new unit mutex (legacy API).
    pub const fn new_unit() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }
}

impl Default for GMutex<()> {
    fn default() -> Self {
        Self::new_unit()
    }
}

/// A recursive mutex (`GRecMutex`).
///
/// Note: `spin` does not provide a recursive mutex. This implementation
/// tracks the owner conceptually but cannot enforce reentrancy in no_std.
/// Use with care - avoid recursive locking.
pub struct GRecMutex {
    inner: Mutex<()>,
}

impl GRecMutex {
    /// Create a new recursive mutex (`g_rec_mutex_init`).
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }

    /// Lock (`g_rec_mutex_lock`).
    pub fn lock(&self) -> spin::MutexGuard<()> {
        self.inner.lock()
    }

    /// Try to lock (`g_rec_mutex_trylock`).
    pub fn try_lock(&self) -> Option<spin::MutexGuard<()>> {
        self.inner.try_lock()
    }
}

impl Default for GRecMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// A read-write lock (`GRWLock`).
///
/// Wraps `spin::RwLock<()>`.
pub struct GRWLock {
    inner: RwLock<()>,
}

impl GRWLock {
    /// Create a new RW lock (`g_rw_lock_init`).
    pub const fn new() -> Self {
        Self {
            inner: RwLock::new(()),
        }
    }

    /// Acquire writer lock (`g_rw_lock_writer_lock`).
    pub fn writer_lock(&self) -> spin::RwLockWriteGuard<()> {
        self.inner.write()
    }

    /// Try writer lock (`g_rw_lock_writer_trylock`).
    pub fn try_writer_lock(&self) -> Option<spin::RwLockWriteGuard<()>> {
        self.inner.try_write()
    }

    /// Acquire reader lock (`g_rw_lock_reader_lock`).
    pub fn reader_lock(&self) -> spin::RwLockReadGuard<()> {
        self.inner.read()
    }

    /// Try reader lock (`g_rw_lock_reader_trylock`).
    pub fn try_reader_lock(&self) -> Option<spin::RwLockReadGuard<()>> {
        self.inner.try_read()
    }
}

impl Default for GRWLock {
    fn default() -> Self {
        Self::new()
    }
}

/// A condition variable (`GCond`).
///
/// In no_std, we cannot implement a true condition variable (requires
/// OS-level blocking/waking). This is a placeholder that provides
/// the type interface; signaling requires platform support.
pub struct GCond {
    inner: Mutex<bool>,
}

impl GCond {
    /// Create a new condition variable (`g_cond_init`).
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(false),
        }
    }

    /// Signal the condition (`g_cond_signal`).
    pub fn signal(&self) {
        *self.inner.lock() = true;
    }

    /// Broadcast (`g_cond_broadcast`).
    pub fn broadcast(&self) {
        *self.inner.lock() = true;
    }

    /// Wait (busy-wait, not efficient) (`g_cond_wait`).
    ///
    /// **Warning**: This is a busy-wait in no_std. Real implementation
    /// requires OS support.
    pub fn wait(&self) {
        loop {
            let mut guard = self.inner.lock();
            if *guard {
                *guard = false;
                return;
            }
            drop(guard);
            core::hint::spin_loop();
        }
    }
}

impl Default for GCond {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread error quark (`g_thread_error_quark`).
pub fn thread_error_quark() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutex_lock_unlock() {
        let m = GMutex::new(0u32);
        {
            let _guard = m.lock();
            // locked
        }
        // unlocked
        let _guard = m.lock();
    }

    #[test]
    fn mutex_try_lock() {
        let m = GMutex::new(0u32);
        let guard = m.try_lock();
        assert!(guard.is_some());
        drop(guard);
    }

    #[test]
    fn rw_lock() {
        let lock = GRWLock::new();
        {
            let _r = lock.reader_lock();
        }
        {
            let _w = lock.writer_lock();
        }
    }

    #[test]
    fn rw_lock_try() {
        let lock = GRWLock::new();
        assert!(lock.try_reader_lock().is_some());
        assert!(lock.try_writer_lock().is_some());
    }

    #[test]
    fn once_init() {
        static O: Once<i32> = Once::new();
        let v = O.call_once(|| 42);
        assert_eq!(*v, 42);
        assert!(O.is_completed());
    }

    #[test]
    fn rec_mutex() {
        let m = GRecMutex::new();
        let _g = m.lock();
    }

    #[test]
    fn cond_signal() {
        let c = GCond::new();
        c.signal();
    }
}
