//! Thread primitives matching `gthread.h` / `gthread.c`.
//!
//! Uses `spin` crate for mutex, RW lock, and once initialization.
//! Thread creation/joining is abstracted via [`ThreadPlatform`].
//! Fully `no_std` compatible using `spin`.

use spin::{Mutex, Once as SpinOnce, RwLock};

/// Thread error (`GThreadError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadError {
    Again,
}

/// Opaque thread handle returned by [`ThreadPlatform::spawn`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreadHandle(u64);

impl ThreadHandle {
    /// Construct a handle from a kernel thread id (RustOS integration).
    pub const fn from_tid(tid: u32) -> Self {
        Self(tid as u64)
    }

    /// Extract the kernel thread id when the platform stores it in the handle.
    pub const fn tid(self) -> u32 {
        self.0 as u32
    }
}

/// Once status (`GOnceStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OnceStatus {
    NotCalled,
    Progress,
    Ready,
}

/// Platform trait for OS thread creation and joining.
pub trait ThreadPlatform: Sync {
    /// Spawn a new thread with the given name and entry function.
    fn spawn(&self, name: &str, func: fn()) -> Result<ThreadHandle, ThreadError>;

    /// Block until the thread identified by `handle` has finished.
    fn join(&self, handle: ThreadHandle) -> Result<(), ThreadError>;
}

/// A no-op thread platform: always returns [`ThreadError::Again`].
pub struct NoThreadPlatform;

impl ThreadPlatform for NoThreadPlatform {
    fn spawn(&self, _name: &str, _func: fn()) -> Result<ThreadHandle, ThreadError> {
        Err(ThreadError::Again)
    }

    fn join(&self, _handle: ThreadHandle) -> Result<(), ThreadError> {
        Err(ThreadError::Again)
    }
}

#[cfg(test)]
mod host_thread {
    use super::*;
    use core::sync::atomic::{AtomicU64, Ordering};

    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
    static HOST_THREADS: Mutex<BTreeMap<u64, std::thread::JoinHandle<()>>> =
        Mutex::new(BTreeMap::new());

    /// Host thread platform using `std::thread` (tests only).
    pub struct HostThreadPlatform;

    impl ThreadPlatform for HostThreadPlatform {
        fn spawn(&self, _name: &str, func: fn()) -> Result<ThreadHandle, ThreadError> {
            let handle_id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            let join_handle = std::thread::spawn(func);
            HOST_THREADS.lock().insert(handle_id, join_handle);
            Ok(ThreadHandle(handle_id))
        }

        fn join(&self, handle: ThreadHandle) -> Result<(), ThreadError> {
            let join_handle = HOST_THREADS
                .lock()
                .remove(&handle.0)
                .ok_or(ThreadError::Again)?;
            join_handle.join().map_err(|_| ThreadError::Again)
        }
    }

    pub fn reset_host_threads() {
        HOST_THREADS.lock().clear();
    }
}

#[cfg(test)]
pub use host_thread::{reset_host_threads, HostThreadPlatform};

static THREAD_PLATFORM: RwLock<&'static dyn ThreadPlatform> = RwLock::new(&NoThreadPlatform);

/// Installs the platform thread implementation.
pub fn register_thread_platform(platform: &'static dyn ThreadPlatform) {
    *THREAD_PLATFORM.write() = platform;
}

/// Thread creation wrapper (`GThread::spawn` / `g_thread_new`).
pub struct GThread;

impl GThread {
    /// Spawn a new thread with the given name and entry function.
    pub fn spawn(name: &str, func: fn()) -> Result<ThreadHandle, ThreadError> {
        THREAD_PLATFORM.read().spawn(name, func)
    }

    /// Join a thread, blocking until it completes.
    pub fn join(handle: ThreadHandle) -> Result<(), ThreadError> {
        THREAD_PLATFORM.read().join(handle)
    }
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
    pub fn lock(&self) -> spin::MutexGuard<'_, T> {
        self.inner.lock()
    }

    /// Try to lock the mutex (`g_mutex_trylock`).
    pub fn try_lock(&self) -> Option<spin::MutexGuard<'_, T>> {
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
    pub fn lock(&self) -> spin::MutexGuard<'_, ()> {
        self.inner.lock()
    }

    /// Try to lock (`g_rec_mutex_trylock`).
    pub fn try_lock(&self) -> Option<spin::MutexGuard<'_, ()>> {
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
    pub fn writer_lock(&self) -> spin::RwLockWriteGuard<'_, ()> {
        self.inner.write()
    }

    /// Try writer lock (`g_rw_lock_writer_trylock`).
    pub fn try_writer_lock(&self) -> Option<spin::RwLockWriteGuard<'_, ()>> {
        self.inner.try_write()
    }

    /// Acquire reader lock (`g_rw_lock_reader_lock`).
    pub fn reader_lock(&self) -> spin::RwLockReadGuard<'_, ()> {
        self.inner.read()
    }

    /// Try reader lock (`g_rw_lock_reader_trylock`).
    pub fn try_reader_lock(&self) -> Option<spin::RwLockReadGuard<'_, ()>> {
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
    use core::sync::atomic::{AtomicU32, Ordering};

    fn reset_thread_platform() {
        register_thread_platform(&NoThreadPlatform);
        reset_host_threads();
    }

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

    #[test]
    fn no_thread_platform_spawn_fails() {
        reset_thread_platform();
        assert_eq!(GThread::spawn("worker", || {}), Err(ThreadError::Again));
    }

    #[test]
    fn no_thread_platform_join_fails() {
        reset_thread_platform();
        assert_eq!(GThread::join(ThreadHandle(1)), Err(ThreadError::Again));
    }

    #[test]
    fn host_thread_platform_spawn_and_join() {
        reset_thread_platform();
        static RAN: AtomicU32 = AtomicU32::new(0);
        fn worker() {
            RAN.fetch_add(1, Ordering::Relaxed);
        }
        register_thread_platform(&HostThreadPlatform);
        let handle = GThread::spawn("test-worker", worker).unwrap();
        GThread::join(handle).unwrap();
        assert_eq!(RAN.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn register_thread_platform_switches_implementation() {
        reset_thread_platform();
        assert!(GThread::spawn("x", || {}).is_err());
        register_thread_platform(&HostThreadPlatform);
        let handle = GThread::spawn("y", || {}).unwrap();
        GThread::join(handle).unwrap();
    }
}
