// SPDX-License-Identifier: MIT
//! Kernel thread (kthread) subsystem — Rust port of Linux kernel/kthread.c
//!
//! Provides the kthread API for creating, stopping, parking, and managing
//! kernel threads, plus a kthread_worker abstraction for deferred work.
//!
//! Relationship to Linux:
//!   - `KThread`           ↔ `struct kthread`   (per-thread private data)
//!   - `KThreadCreateInfo` ↔ `struct kthread_create_info`
//!   - `KThreadWorker`     ↔ `struct kthread_worker`
//!   - `KThreadWork`       ↔ `struct kthread_work`
//!   - `kthreadd()`        ↔ `kthreadd()` daemon (Linux PID 2 equivalent)

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use spin::Mutex;

// ---------------------------------------------------------------------------
// Constants — mirror Linux KTHREAD_BITS / flag values
// ---------------------------------------------------------------------------

pub const KTHREAD_SHOULD_STOP: u32 = 1 << 0;
pub const KTHREAD_SHOULD_PARK: u32 = 1 << 1;
pub const KTHREAD_IS_PARKED: u32 = 1 << 2;
pub const KTHREAD_IS_PER_CPU: u32 = 1 << 3;

/// Maximum length for a task/thread name, including NUL terminator.
pub const TASK_COMM_LEN: usize = 16;

// ---------------------------------------------------------------------------
// errno-style error codes (subset)
// ---------------------------------------------------------------------------

const ENOMEM: i32 = -12;
const EINVAL: i32 = -22;
const EINTR: i32 = -4;
const ENOENT: i32 = -2;

// ---------------------------------------------------------------------------
// KThread — per-thread private data (stored in the TCB's kthread slot)
// ---------------------------------------------------------------------------

/// Private data attached to every kthread.
///
/// Equivalent to Linux `struct kthread`.  Held behind an `Arc` so callers
/// and the thread itself can share it safely.
pub struct KThread {
    /// Bitfield of KTHREAD_* flags.
    pub flags: AtomicU32,
    /// Parking reference count (incremented by `kthread_park`, decremented
    /// by `kthread_unpark`).
    pub park_count: AtomicU32,
    /// The thread function.
    pub threadfn: Option<fn(*mut u8) -> i32>,
    /// Opaque data pointer passed to `threadfn`.
    pub data: *mut u8,
    /// Null-terminated thread name (ASCII).
    pub name: [u8; TASK_COMM_LEN],
}

// SAFETY: `data` is an opaque pointer whose lifetime and aliasing are managed
// by the caller.  KThread is only accessed via Arc which provides the
// necessary synchronisation.
unsafe impl Send for KThread {}
unsafe impl Sync for KThread {}

impl KThread {
    /// Construct a new `KThread` with all flags cleared.
    pub fn new(threadfn: fn(*mut u8) -> i32, data: *mut u8, name: &str) -> Self {
        let mut name_buf = [0u8; TASK_COMM_LEN];
        let bytes = name.as_bytes();
        let copy_len = bytes.len().min(TASK_COMM_LEN - 1);
        name_buf[..copy_len].copy_from_slice(&bytes[..copy_len]);

        KThread {
            flags: AtomicU32::new(0),
            park_count: AtomicU32::new(0),
            threadfn: Some(threadfn),
            data,
            name: name_buf,
        }
    }

    /// Return the name as a `&str`, stripping any trailing NUL bytes.
    pub fn name_str(&self) -> &str {
        let end = self
            .name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(TASK_COMM_LEN);
        core::str::from_utf8(&self.name[..end]).unwrap_or("<invalid>")
    }

    /// Set one or more flag bits atomically.
    #[inline]
    fn set_flags(&self, bits: u32) {
        self.flags.fetch_or(bits, Ordering::SeqCst);
    }

    /// Clear one or more flag bits atomically.
    #[inline]
    fn clear_flags(&self, bits: u32) {
        self.flags.fetch_and(!bits, Ordering::SeqCst);
    }

    /// Test whether *all* of the given bits are set.
    #[inline]
    fn test_flags(&self, bits: u32) -> bool {
        self.flags.load(Ordering::SeqCst) & bits == bits
    }
}

// ---------------------------------------------------------------------------
// KThreadCreateInfo — queued by kthread_create, consumed by kthreadd
// ---------------------------------------------------------------------------

/// Pending thread-creation request.
///
/// Equivalent to Linux `struct kthread_create_info`.  One of these is pushed
/// onto `KTHREAD_CREATE_QUEUE` by `kthread_create` and drained by the
/// `kthreadd` daemon.
pub struct KThreadCreateInfo {
    /// The thread body.
    pub threadfn: fn(*mut u8) -> i32,
    /// Opaque argument for `threadfn`.
    pub data: *mut u8,
    /// CPU affinity (`-1` = no affinity / any CPU).
    pub cpu: i32,
    /// Null-terminated name to assign to the new thread.
    pub name: [u8; TASK_COMM_LEN],
}

// SAFETY: see KThread above.
unsafe impl Send for KThreadCreateInfo {}
unsafe impl Sync for KThreadCreateInfo {}

impl KThreadCreateInfo {
    fn new(threadfn: fn(*mut u8) -> i32, data: *mut u8, cpu: i32, name: &str) -> Self {
        let mut name_buf = [0u8; TASK_COMM_LEN];
        let bytes = name.as_bytes();
        let copy_len = bytes.len().min(TASK_COMM_LEN - 1);
        name_buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
        KThreadCreateInfo {
            threadfn,
            data,
            cpu,
            name: name_buf,
        }
    }
}

// ---------------------------------------------------------------------------
// Global kthreadd state
// ---------------------------------------------------------------------------

/// The pending-create queue consumed by kthreadd.
static KTHREAD_CREATE_QUEUE: Mutex<VecDeque<KThreadCreateInfo>> = Mutex::new(VecDeque::new());

/// Set to `true` once kthreadd has been fully initialised.
static KTHREADD_READY: AtomicBool = AtomicBool::new(false);

/// TID of the kthreadd daemon thread, set by `kthreadd_init()`.
static KTHREADD_TID: AtomicU32 = AtomicU32::new(0);

/// Registry of live kthreads so that `kthread_stop` / `kthread_park` can
/// locate them by TID.
static KTHREAD_REGISTRY: Mutex<VecDeque<(u32, Arc<KThread>)>> = Mutex::new(VecDeque::new());

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Register a KThread so it can be found by TID later.
fn register_kthread(tid: u32, kt: Arc<KThread>) {
    KTHREAD_REGISTRY.lock().push_back((tid, kt));
}

/// Look up a registered KThread by TID.
fn find_kthread(tid: u32) -> Option<Arc<KThread>> {
    KTHREAD_REGISTRY
        .lock()
        .iter()
        .find(|(t, _)| *t == tid)
        .map(|(_, kt)| Arc::clone(kt))
}

/// Remove a KThread from the registry.
fn unregister_kthread(tid: u32) {
    let mut reg = KTHREAD_REGISTRY.lock();
    reg.retain(|(t, _)| *t != tid);
}

/// Spawn a kernel thread via the RustOS thread manager.
///
/// Creates a real kernel thread through `process::thread::create_kernel_thread`,
/// wrapping the kthread's `fn(*mut u8) -> i32` entry point in a closure.
/// The returned TID is the thread manager's TID, used for all subsequent
/// wake/stop operations.
fn spawn_kthread(info: KThreadCreateInfo) -> Result<u32, i32> {
    let name_bytes = &info.name;
    let end = name_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(TASK_COMM_LEN);
    let name_str = core::str::from_utf8(&name_bytes[..end]).unwrap_or("kthread");

    let threadfn = info.threadfn;
    let data = info.data;
    let data_addr = data as usize;
    let kt = Arc::new(KThread::new(threadfn, data, name_str));

    let kt_for_closure = Arc::clone(&kt);
    let closure = move || {
        let data = data_addr as *mut u8;
        let _result = threadfn(data);
        // Arc drop: when the closure finishes, kt_for_closure is dropped,
        // releasing the KThread reference.  The registry entry is cleaned
        // up by kthread_stop or the caller.
        drop(kt_for_closure);
    };

    let tid = crate::process::thread::create_kernel_thread(
        name_str,
        crate::process::Priority::Normal,
        0x2000, // 8 KB kernel stack
        closure,
    )
    .map_err(|_| ENOMEM)?;

    register_kthread(tid, kt);
    Ok(tid)
}

// ---------------------------------------------------------------------------
// Public API — thread creation
// ---------------------------------------------------------------------------

/// Request kthreadd to create a new kernel thread (does NOT wake it).
///
/// Analogous to `kthread_create_on_node()` in Linux.  The thread is created
/// in a stopped state; call `wake_up_process()` (or use `kthread_run`) to
/// start it.
///
/// # Returns
/// `Ok(tid)` on success, `Err(errno)` on failure.
pub fn kthread_create(threadfn: fn(*mut u8) -> i32, data: *mut u8, name: &str) -> Result<u32, i32> {
    kthread_create_on_cpu(threadfn, data, u32::MAX, name)
}

/// Create and immediately wake (run) a new kernel thread.
///
/// Equivalent to the `kthread_run()` macro in Linux.
pub fn kthread_run(threadfn: fn(*mut u8) -> i32, data: *mut u8, name: &str) -> Result<u32, i32> {
    let tid = kthread_create(threadfn, data, name)?;
    kthread_wake(tid);
    Ok(tid)
}

/// Create a kernel thread bound to a specific CPU.
///
/// Pass `cpu = u32::MAX` (or call `kthread_create`) for no affinity.
pub fn kthread_create_on_cpu(
    threadfn: fn(*mut u8) -> i32,
    data: *mut u8,
    cpu: u32,
    name: &str,
) -> Result<u32, i32> {
    if !KTHREADD_READY.load(Ordering::SeqCst) {
        // kthreadd not yet initialised — spawn directly (early boot path).
        let info = KThreadCreateInfo::new(
            threadfn,
            data,
            if cpu == u32::MAX { -1 } else { cpu as i32 },
            name,
        );
        return spawn_kthread(info);
    }

    let info = KThreadCreateInfo::new(
        threadfn,
        data,
        if cpu == u32::MAX { -1 } else { cpu as i32 },
        name,
    );
    KTHREAD_CREATE_QUEUE.lock().push_back(info);

    // Wake kthreadd so it processes the queue.
    let kthreadd_tid = KTHREADD_TID.load(Ordering::SeqCst);
    if kthreadd_tid != 0 {
        kthread_wake(kthreadd_tid);
    } else {
        // kthreadd not yet running — drain synchronously.
        return drain_create_queue();
    }

    // Return the TID of the last enqueued thread by draining the queue
    // here too (kthreadd may not have run yet on this CPU).
    drain_create_queue()
}

/// Drain the pending-create queue and return the TID of the last-created
/// thread (the one just enqueued by the caller).
fn drain_create_queue() -> Result<u32, i32> {
    let mut last_tid = Err(EINVAL);
    loop {
        let maybe_info = KTHREAD_CREATE_QUEUE.lock().pop_front();
        match maybe_info {
            Some(info) => {
                last_tid = spawn_kthread(info);
            }
            None => break,
        }
    }
    last_tid
}

/// Wake (unblock) a kernel thread by TID.
///
/// Calls the thread manager's `unblock_thread` to move the thread from
/// the blocked state back to the ready queue.
pub fn kthread_wake(tid: u32) {
    let _ = crate::process::thread::get_thread_manager().unblock_thread(tid);
}

// ---------------------------------------------------------------------------
// Public API — thread lifecycle
// ---------------------------------------------------------------------------

/// Ask a kthread to stop.
///
/// Sets `KTHREAD_SHOULD_STOP` on the thread's KThread data and wakes it.
/// The thread should check `kthread_should_stop()` in its main loop and
/// return when the flag is set.  After waking, terminates the thread in
/// the thread manager and removes it from the kthread registry.
///
/// Analogous to `kthread_stop()` in Linux.
pub fn kthread_stop(tid: u32) -> i32 {
    let kt = match find_kthread(tid) {
        Some(k) => k,
        None => return ENOENT,
    };

    kt.set_flags(KTHREAD_SHOULD_STOP);
    kthread_wake(tid);

    // Terminate the thread in the thread manager.
    let _ = crate::process::thread::get_thread_manager().terminate_thread(tid, 0);
    unregister_kthread(tid);
    0
}

/// Called by a kthread itself to test whether it should exit its main loop.
///
/// ```rust,no_run
/// while !kthread::kthread_should_stop(&my_kthread) {
///     // do work …
/// }
/// ```
#[inline]
pub fn kthread_should_stop(kthread: &KThread) -> bool {
    kthread.test_flags(KTHREAD_SHOULD_STOP)
}

/// Called by a kthread itself to test whether it has been asked to park.
#[inline]
pub fn kthread_should_park(kthread: &KThread) -> bool {
    kthread.test_flags(KTHREAD_SHOULD_PARK)
}

/// Ask a running kthread to park (suspend at a safe point).
///
/// Increments the park reference count and sets `KTHREAD_SHOULD_PARK`.
/// Returns `0` on success, `ENOENT` if the TID is unknown.
pub fn kthread_park(tid: u32) -> i32 {
    let kt = match find_kthread(tid) {
        Some(k) => k,
        None => return ENOENT,
    };

    kt.park_count.fetch_add(1, Ordering::SeqCst);
    kt.set_flags(KTHREAD_SHOULD_PARK);
    kthread_wake(tid);

    // Wait for the thread to actually park (set IS_PARKED).
    while !kt.test_flags(KTHREAD_IS_PARKED) {
        crate::process::thread::yield_thread();
    }
    0
}

/// Unpark a previously parked kthread.
///
/// Decrements the park reference count; clears `KTHREAD_SHOULD_PARK` once
/// it reaches zero, then wakes the thread.
pub fn kthread_unpark(tid: u32) {
    let kt = match find_kthread(tid) {
        Some(k) => k,
        None => return,
    };

    let prev = kt.park_count.fetch_sub(1, Ordering::SeqCst);
    if prev <= 1 {
        // Count reached zero.
        kt.clear_flags(KTHREAD_SHOULD_PARK | KTHREAD_IS_PARKED);
        kthread_wake(tid);
    }
}

/// Called by a kthread itself when it notices `KTHREAD_SHOULD_PARK`.
///
/// Sets `IS_PARKED` and yields the CPU until `SHOULD_PARK` is cleared
/// by `kthread_unpark`.
pub fn kthread_parkme(kthread: &KThread) {
    kthread.set_flags(KTHREAD_IS_PARKED);

    // Yield until the park flag is cleared.
    while kthread.test_flags(KTHREAD_SHOULD_PARK) {
        crate::process::thread::yield_thread();
    }

    kthread.clear_flags(KTHREAD_IS_PARKED);
}

// ---------------------------------------------------------------------------
// kthreadd daemon — Linux PID 2 equivalent
// ---------------------------------------------------------------------------

/// The kthreadd daemon body.
///
/// Continuously drains `KTHREAD_CREATE_QUEUE` and spawns the requested
/// threads.  Yields the CPU when idle between iterations.
///
/// This function is intended to be the `threadfn` of the kthreadd thread.
pub fn kthreadd(_data: *mut u8) -> i32 {
    KTHREADD_READY.store(true, Ordering::SeqCst);

    let kthread_data = KThread::new(kthreadd as fn(*mut u8) -> i32, _data, "kthreadd");

    loop {
        // Check for stop request.
        if kthread_data.test_flags(KTHREAD_SHOULD_STOP) {
            break;
        }

        // Drain the pending-create queue.
        loop {
            let maybe_info = KTHREAD_CREATE_QUEUE.lock().pop_front();
            match maybe_info {
                Some(info) => {
                    let _ = spawn_kthread(info);
                }
                None => break,
            }
        }

        // Check for park request.
        if kthread_data.test_flags(KTHREAD_SHOULD_PARK) {
            kthread_parkme(&kthread_data);
        }

        crate::process::thread::yield_thread();
    }

    0
}

/// Initialise the kthreadd daemon at kernel boot.
///
/// Call once from the early-init path (after the scheduler is ready).
/// Returns the TID of the kthreadd thread.
pub fn kthreadd_init() -> Result<u32, i32> {
    // Spawn kthreadd as a kernel thread.  We use the early-boot path here
    // (KTHREADD_READY is still false) so we bypass the queue.
    let info = KThreadCreateInfo::new(kthreadd, core::ptr::null_mut(), -1, "kthreadd");
    let tid = spawn_kthread(info)?;

    // Mark kthreadd as ready so subsequent kthread_create calls use the queue.
    KTHREADD_READY.store(true, Ordering::SeqCst);
    KTHREADD_TID.store(tid, Ordering::SeqCst);

    kthread_wake(tid);
    Ok(tid)
}

// ---------------------------------------------------------------------------
// KThreadWork / KThreadWorker / KThreadDelayedWork
// ---------------------------------------------------------------------------

/// A unit of work queued into a `KThreadWorker`.
///
/// Equivalent to Linux `struct kthread_work`.
pub struct KThreadWork {
    /// The function to execute.
    pub func: fn(&KThreadWork),
    /// Set to `true` once the work has been executed (or cancelled).
    pub done: AtomicBool,
    /// Set to `true` while the work is queued.
    queued: AtomicBool,
}

impl KThreadWork {
    /// Create a new `KThreadWork` item.
    pub fn new(func: fn(&KThreadWork)) -> Self {
        KThreadWork {
            func,
            done: AtomicBool::new(false),
            queued: AtomicBool::new(false),
        }
    }
}

/// A delayed work item (work + scheduling deadline in jiffies).
///
/// Equivalent to Linux `struct kthread_delayed_work`.
pub struct KThreadDelayedWork {
    /// The inner work item.
    pub work: KThreadWork,
    /// Absolute deadline in kernel jiffies.
    pub delay_jiffies: u64,
}

impl KThreadDelayedWork {
    /// Create a new delayed work item.
    pub fn new(func: fn(&KThreadWork), delay_jiffies: u64) -> Self {
        KThreadDelayedWork {
            work: KThreadWork::new(func),
            delay_jiffies,
        }
    }
}

/// Worker-thread flags.
pub const KTHREAD_WORKER_UNBOUND: u32 = 1 << 0;

/// A kthread worker with an associated queue of work items.
///
/// Equivalent to Linux `struct kthread_worker`.
pub struct KThreadWorker {
    /// Worker flags (e.g. `KTHREAD_WORKER_UNBOUND`).
    pub flags: AtomicU32,
    /// TID of the worker thread (`None` before `kthread_create_worker` is
    /// called or after the worker is destroyed).
    pub task_tid: Option<u32>,
    /// Pending work items.
    pub work_list: Mutex<VecDeque<Arc<KThreadWork>>>,
    /// Pending delayed work items.
    pub delayed_work_list: Mutex<VecDeque<Arc<KThreadDelayedWork>>>,
}

impl KThreadWorker {
    /// Create an uninitialised worker.  Call `kthread_create_worker` or
    /// `kthread_create_worker_on_cpu` to associate it with a thread.
    pub fn new() -> Self {
        KThreadWorker {
            flags: AtomicU32::new(0),
            task_tid: None,
            work_list: Mutex::new(VecDeque::new()),
            delayed_work_list: Mutex::new(VecDeque::new()),
        }
    }

    /// Enqueue a work item.
    ///
    /// Returns `true` if the item was successfully enqueued, `false` if it
    /// was already queued.
    pub fn queue_work(&self, work: Arc<KThreadWork>) -> bool {
        if work.queued.swap(true, Ordering::SeqCst) {
            // Already queued.
            return false;
        }
        self.work_list.lock().push_back(work);

        if let Some(tid) = self.task_tid {
            kthread_wake(tid);
        }
        true
    }

    /// Block until the given work item has completed execution.
    ///
    /// Yields the CPU while waiting for the work item to finish.
    pub fn flush_work(&self, work: &KThreadWork) {
        while !work.done.load(Ordering::SeqCst) {
            crate::process::thread::yield_thread();
        }
    }

    /// Cancel a queued work item without executing it.
    ///
    /// Returns `true` if the item was cancelled before it ran.
    pub fn cancel_work_sync(&self, work: &Arc<KThreadWork>) -> bool {
        if !work.queued.load(Ordering::SeqCst) {
            return false;
        }

        let mut list = self.work_list.lock();
        // Find and remove by pointer identity.
        let before = list.len();
        list.retain(|w| !Arc::ptr_eq(w, work));
        let removed = list.len() < before;

        if removed {
            work.queued.store(false, Ordering::SeqCst);
            work.done.store(true, Ordering::SeqCst);
        }
        removed
    }

    /// Enqueue a delayed work item.
    ///
    /// Items are not automatically promoted to `work_list` by this struct;
    /// a timer subsystem integration is needed for that.  Returns `true` if
    /// newly enqueued.
    pub fn queue_delayed_work(&self, dwork: Arc<KThreadDelayedWork>) -> bool {
        if dwork.work.queued.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.delayed_work_list.lock().push_back(dwork);
        true
    }

    /// Flush all pending and delayed work, blocking until the queue is empty.
    pub fn flush(&self) {
        loop {
            let is_empty =
                self.work_list.lock().is_empty() && self.delayed_work_list.lock().is_empty();
            if is_empty {
                break;
            }
            crate::process::thread::yield_thread();
        }
    }

    /// Drain and execute all ready work items in the queue.
    ///
    /// Called from `kthread_worker_fn` on each wakeup.
    fn process_work(&self) {
        loop {
            let maybe_work = self.work_list.lock().pop_front();
            match maybe_work {
                Some(work) => {
                    (work.func)(&work);
                    work.queued.store(false, Ordering::SeqCst);
                    work.done.store(true, Ordering::SeqCst);
                }
                None => break,
            }
        }
    }
}

impl Default for KThreadWorker {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker thread main function.
///
/// Intended to be the `threadfn` of a kthread.  `data` must point to a
/// `KThreadWorker`.  The worker processes all queued items then sleeps until
/// woken by `queue_work`.
///
/// Equivalent to Linux `kthread_worker_fn()`.
///
/// # Safety
/// `data` must be a valid pointer to a `KThreadWorker` that lives at least
/// as long as the thread.
pub fn kthread_worker_fn(worker: &KThreadWorker, kthread: &KThread) -> i32 {
    loop {
        // Park if requested.
        if kthread.test_flags(KTHREAD_SHOULD_PARK) {
            kthread_parkme(kthread);
        }

        // Stop if requested.
        if kthread.test_flags(KTHREAD_SHOULD_STOP) {
            break;
        }

        // Execute all pending work.
        worker.process_work();

        // Yield CPU when no work is pending — the thread will be woken
        // via kthread_wake() when new work is queued.
        if worker.work_list.lock().is_empty() {
            crate::process::thread::yield_thread();
        }
    }
    0
}

// ---------------------------------------------------------------------------
// High-level helpers — create_worker / destroy_worker
// ---------------------------------------------------------------------------

/// Create a kthread and attach it to `worker`, running `kthread_worker_fn`.
///
/// Equivalent to `kthread_create_worker()` in Linux.
///
/// # Safety
/// `worker` must remain valid for the lifetime of the created thread.
pub fn kthread_create_worker(worker: &'static KThreadWorker, name: &str) -> Result<u32, i32> {
    // We need a bare function pointer, so we use a trampoline.
    fn trampoline(data: *mut u8) -> i32 {
        // SAFETY: caller guarantees the pointer is valid.
        let worker = unsafe { &*(data as *const KThreadWorker) };
        let kthread = KThread::new(trampoline as fn(*mut u8) -> i32, data, "worker");
        kthread_worker_fn(worker, &kthread)
    }

    let tid = kthread_run(trampoline, worker as *const _ as *mut u8, name)?;
    Ok(tid)
}

/// Destroy a worker: stop its thread, wait for it to exit, then flush any
/// remaining work.
pub fn kthread_destroy_worker(worker: &'static KThreadWorker, tid: u32) {
    kthread_stop(tid);
    // Drain any leftover work.
    worker.process_work();
}

// ---------------------------------------------------------------------------
// Tests (cfg(test) — only compiled in a hosted test environment)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_fn(_data: *mut u8) -> i32 {
        0
    }
    fn noop_work(_work: &KThreadWork) {}

    #[test]
    fn test_kthread_flags() {
        let kt = KThread::new(dummy_fn, core::ptr::null_mut(), "test");
        assert!(!kthread_should_stop(&kt));
        assert!(!kthread_should_park(&kt));
        kt.set_flags(KTHREAD_SHOULD_STOP);
        assert!(kthread_should_stop(&kt));
        kt.clear_flags(KTHREAD_SHOULD_STOP);
        assert!(!kthread_should_stop(&kt));
    }

    #[test]
    fn test_kthread_name() {
        let kt = KThread::new(dummy_fn, core::ptr::null_mut(), "hello");
        assert_eq!(kt.name_str(), "hello");
    }

    #[test]
    fn test_kthread_name_truncation() {
        // Name longer than TASK_COMM_LEN-1 should be truncated.
        let long_name = "this_name_is_definitely_too_long";
        let kt = KThread::new(dummy_fn, core::ptr::null_mut(), long_name);
        assert_eq!(kt.name_str().len(), TASK_COMM_LEN - 1);
    }

    #[test]
    fn test_worker_queue_and_cancel() {
        use alloc::sync::Arc;
        let worker = KThreadWorker::new();
        let work = Arc::new(KThreadWork::new(noop_work));

        assert!(worker.queue_work(Arc::clone(&work)));
        // Already queued — should return false.
        assert!(!worker.queue_work(Arc::clone(&work)));
        assert!(worker.cancel_work_sync(&work));
        // After cancel, done should be set.
        assert!(work.done.load(Ordering::SeqCst));
    }

    #[test]
    fn test_worker_process() {
        use alloc::sync::Arc;
        let worker = KThreadWorker::new();
        let work = Arc::new(KThreadWork::new(noop_work));
        worker.queue_work(Arc::clone(&work));
        worker.process_work();
        assert!(work.done.load(Ordering::SeqCst));
    }
}
