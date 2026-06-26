//! Kernel Thread Support
//!
//! This module provides comprehensive kernel threading support with synchronization
//! primitives, thread-local storage, and advanced threading features for RustOS.

use super::{get_system_time, CpuContext, Pid, Priority};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};

/// Thread ID type
pub type Tid = u32;

/// Maximum number of threads per process
pub const MAX_THREADS_PER_PROCESS: usize = 128;

/// Maximum total threads in system
pub const MAX_SYSTEM_THREADS: usize = 4096;

/// Thread states (similar to process states but thread-specific)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Thread is ready to run
    Ready,
    /// Thread is currently running
    Running,
    /// Thread is blocked waiting for I/O or synchronization
    Blocked,
    /// Thread is sleeping
    Sleeping,
    /// Thread has terminated but not yet cleaned up
    Zombie,
    /// Thread has been completely cleaned up
    Dead,
}

/// Thread types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadType {
    /// User-space thread
    User,
    /// Kernel thread
    Kernel,
    /// Interrupt handler thread
    Interrupt,
    /// Work queue thread
    WorkQueue,
}

/// Thread synchronization primitives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    /// Not waiting
    None,
    /// Waiting on mutex
    Mutex(u32),
    /// Waiting on semaphore
    Semaphore(u32),
    /// Waiting on condition variable
    ConditionVariable(u32),
    /// Waiting for I/O
    IO(u32),
    /// Waiting for timer
    Timer(u64),
    /// Waiting for child thread
    Join(Tid),
}

/// Thread Control Block (TCB)
#[derive(Debug, Clone)]
pub struct ThreadControlBlock {
    /// Thread ID
    pub tid: Tid,
    /// Process ID this thread belongs to
    pub pid: Pid,
    /// Thread state
    pub state: ThreadState,
    /// Thread type
    pub thread_type: ThreadType,
    /// Thread priority
    pub priority: Priority,
    /// CPU context for context switching
    pub context: CpuContext,
    /// Kernel stack pointer
    pub kernel_stack: u64,
    /// User stack pointer (for user threads)
    pub user_stack: u64,
    /// Stack size
    pub stack_size: usize,
    /// Thread name
    pub name: [u8; 32],
    /// CPU time used (in ticks)
    pub cpu_time: u64,
    /// Time when thread was created
    pub creation_time: u64,
    /// Time when thread was last scheduled
    pub last_scheduled: u64,
    /// Exit status (valid only when state is Zombie)
    pub exit_status: Option<i32>,
    /// What the thread is waiting for
    pub wait_reason: WaitReason,
    /// Wake up time (for sleeping threads)
    pub wake_time: Option<u64>,
    /// Thread-local storage pointer
    pub tls_pointer: u64,
    /// CPU affinity mask
    pub cpu_affinity: u64,
    /// Thread scheduling information
    pub sched_info: ThreadSchedulingInfo,
}

/// Thread-specific scheduling information
#[derive(Debug, Clone)]
pub struct ThreadSchedulingInfo {
    /// Virtual runtime for CFS scheduling
    pub vruntime: u64,
    /// Nice value (-20 to 19)
    pub nice: i8,
    /// Weight based on nice value
    pub weight: u32,
    /// Time slice remaining
    pub time_slice: u32,
    /// Number of times thread has been scheduled
    pub schedule_count: u64,
    /// Last time thread was preempted
    pub last_preempted: u64,
    /// Thread is pinned to specific CPU
    pub pinned_cpu: Option<u8>,
}

impl ThreadControlBlock {
    /// Create a new TCB
    pub fn new(
        tid: Tid,
        pid: Pid,
        thread_type: ThreadType,
        priority: Priority,
        name: &str,
        kernel_stack: u64,
        user_stack: u64,
        stack_size: usize,
    ) -> Self {
        let mut tcb = Self {
            tid,
            pid,
            state: ThreadState::Ready,
            thread_type,
            priority,
            context: CpuContext::default(),
            kernel_stack,
            user_stack,
            stack_size,
            name: [0; 32],
            cpu_time: 0,
            creation_time: get_system_time(),
            last_scheduled: 0,
            exit_status: None,
            wait_reason: WaitReason::None,
            wake_time: None,
            tls_pointer: 0,
            cpu_affinity: 0xFFFFFFFFFFFFFFFF, // All CPUs by default
            sched_info: ThreadSchedulingInfo {
                vruntime: 0,
                nice: 0,
                weight: 1024,   // Default weight
                time_slice: 10, // 10ms default
                schedule_count: 0,
                last_preempted: 0,
                pinned_cpu: None,
            },
        };

        // Set thread name
        let name_bytes = name.as_bytes();
        let copy_len = core::cmp::min(name_bytes.len(), 31);
        tcb.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        tcb
    }

    /// Get thread name as string
    pub fn name_str(&self) -> &str {
        let name_len = self.name.iter().position(|&x| x == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..name_len]).unwrap_or("invalid")
    }

    /// Set thread state
    pub fn set_state(&mut self, state: ThreadState) {
        self.state = state;
    }

    /// Check if thread is runnable
    pub fn is_runnable(&self) -> bool {
        matches!(self.state, ThreadState::Ready)
    }

    /// Check if thread should wake up
    pub fn should_wake_up(&self, current_time: u64) -> bool {
        if let Some(wake_time) = self.wake_time {
            current_time >= wake_time
        } else {
            false
        }
    }

    /// Set thread to sleep for specified duration
    pub fn sleep(&mut self, duration_ms: u64) {
        self.state = ThreadState::Sleeping;
        self.wake_time = Some(get_system_time() + duration_ms);
    }

    /// Wake up sleeping thread
    pub fn wake_up(&mut self) {
        if self.state == ThreadState::Sleeping {
            self.state = ThreadState::Ready;
            self.wake_time = None;
        }
    }
}

/// Thread Manager - manages all kernel threads
pub struct ThreadManager {
    /// All threads in the system
    threads: RwLock<BTreeMap<Tid, ThreadControlBlock>>,
    /// Thread count
    thread_count: AtomicUsize,
    /// Next TID to allocate
    next_tid: AtomicU32,
    /// Threads per process
    process_threads: RwLock<BTreeMap<Pid, Vec<Tid>>>,
    /// Ready queue for threads
    ready_queue: Mutex<VecDeque<Tid>>,
    /// Sleeping threads queue
    sleeping_threads: Mutex<Vec<Tid>>,
    /// Thread synchronization objects
    sync_objects: Mutex<SyncObjectManager>,
    /// Current running thread ID (per CPU - simplified to single CPU for now)
    current_thread: AtomicU32,
}

impl ThreadManager {
    /// Create a new thread manager
    pub const fn new() -> Self {
        Self {
            threads: RwLock::new(BTreeMap::new()),
            thread_count: AtomicUsize::new(0),
            next_tid: AtomicU32::new(1),
            process_threads: RwLock::new(BTreeMap::new()),
            ready_queue: Mutex::new(VecDeque::new()),
            sleeping_threads: Mutex::new(Vec::new()),
            sync_objects: Mutex::new(SyncObjectManager::new()),
            current_thread: AtomicU32::new(0), // Start with kernel thread (TID 0)
        }
    }

    /// Initialize the thread manager
    pub fn init(&self) -> Result<(), &'static str> {
        // Create main kernel thread (TID 0)
        let kernel_tcb = ThreadControlBlock::new(
            0,
            0, // Kernel process PID
            ThreadType::Kernel,
            Priority::RealTime,
            "kernel_main",
            0,      // Will be set by kernel
            0,      // Kernel threads don't have user stacks
            0x2000, // 8KB kernel stack
        );

        {
            let mut threads = self.threads.write();
            threads.insert(0, kernel_tcb);
        }

        {
            let mut process_threads = self.process_threads.write();
            process_threads.insert(0, vec![0]);
        }

        self.thread_count.store(1, Ordering::SeqCst);

        Ok(())
    }

    /// Create a new kernel thread
    pub fn create_kernel_thread<F>(
        &self,
        name: &str,
        priority: Priority,
        stack_size: usize,
        entry_point: F,
    ) -> Result<Tid, &'static str>
    where
        F: FnOnce() + Send + 'static,
    {
        // Convert function to address for storage
        // In a real kernel, we'd store the closure properly
        let entry_addr = core::ptr::addr_of!(entry_point) as *const () as u64;

        self.create_thread_internal(
            0, // Kernel process PID
            ThreadType::Kernel,
            name,
            priority,
            stack_size,
            entry_addr,
        )
    }

    /// Create a new user thread
    pub fn create_user_thread(
        &self,
        pid: Pid,
        name: &str,
        priority: Priority,
        stack_size: usize,
        entry_point: u64,
    ) -> Result<Tid, &'static str> {
        self.create_thread_internal(
            pid,
            ThreadType::User,
            name,
            priority,
            stack_size,
            entry_point,
        )
    }

    /// Internal thread creation
    fn create_thread_internal(
        &self,
        pid: Pid,
        thread_type: ThreadType,
        name: &str,
        priority: Priority,
        stack_size: usize,
        entry_point: u64,
    ) -> Result<Tid, &'static str> {
        if self.thread_count.load(Ordering::SeqCst) >= MAX_SYSTEM_THREADS {
            return Err("Maximum system thread count exceeded");
        }

        // Check per-process thread limit
        {
            let process_threads = self.process_threads.read();
            if let Some(threads) = process_threads.get(&pid) {
                if threads.len() >= MAX_THREADS_PER_PROCESS {
                    return Err("Maximum threads per process exceeded");
                }
            }
        }

        let tid = self.next_tid.fetch_add(1, Ordering::SeqCst);

        // Allocate stack memory
        let kernel_stack = self.allocate_stack(stack_size)?;
        let user_stack = if thread_type == ThreadType::User {
            self.allocate_user_stack(stack_size)?
        } else {
            0
        };

        let mut tcb = ThreadControlBlock::new(
            tid,
            pid,
            thread_type,
            priority,
            name,
            kernel_stack,
            user_stack,
            stack_size,
        );

        // Set up initial context
        tcb.context.rip = entry_point;
        tcb.context.rsp = if thread_type == ThreadType::User {
            user_stack + stack_size as u64
        } else {
            kernel_stack + stack_size as u64
        };

        // Set appropriate segments based on thread type
        if thread_type == ThreadType::User {
            tcb.context.cs = 0x18 | 3; // User code segment with RPL=3
            tcb.context.ds = 0x20 | 3; // User data segment with RPL=3
            tcb.context.es = 0x20 | 3;
            tcb.context.fs = 0x20 | 3;
            tcb.context.gs = 0x20 | 3;
            tcb.context.ss = 0x20 | 3; // User stack segment with RPL=3
        } else {
            tcb.context.cs = 0x08; // Kernel code segment
            tcb.context.ds = 0x10; // Kernel data segment
            tcb.context.es = 0x10;
            tcb.context.fs = 0x10;
            tcb.context.gs = 0x10;
            tcb.context.ss = 0x10; // Kernel stack segment
        }

        // Insert thread
        {
            let mut threads = self.threads.write();
            threads.insert(tid, tcb);
        }

        // Add to process thread list
        {
            let mut process_threads = self.process_threads.write();
            process_threads
                .entry(pid)
                .or_insert_with(Vec::new)
                .push(tid);
        }

        // Add to ready queue
        {
            let mut ready_queue = self.ready_queue.lock();
            ready_queue.push_back(tid);
        }

        self.thread_count.fetch_add(1, Ordering::SeqCst);

        Ok(tid)
    }

    /// Terminate a thread
    pub fn terminate_thread(&self, tid: Tid, exit_status: i32) -> Result<(), &'static str> {
        {
            let mut threads = self.threads.write();
            if let Some(tcb) = threads.get_mut(&tid) {
                tcb.set_state(ThreadState::Zombie);
                tcb.exit_status = Some(exit_status);
            } else {
                return Err("Thread not found");
            }
        }

        // Remove from ready queue
        {
            let mut ready_queue = self.ready_queue.lock();
            ready_queue.retain(|&t| t != tid);
        }

        Ok(())
    }

    /// Get thread information
    pub fn get_thread(&self, tid: Tid) -> Option<ThreadControlBlock> {
        let threads = self.threads.read();
        threads.get(&tid).cloned()
    }

    /// Block a thread
    pub fn block_thread(&self, tid: Tid, wait_reason: WaitReason) -> Result<(), &'static str> {
        {
            let mut threads = self.threads.write();
            if let Some(tcb) = threads.get_mut(&tid) {
                tcb.set_state(ThreadState::Blocked);
                tcb.wait_reason = wait_reason;
            } else {
                return Err("Thread not found");
            }
        }

        // Remove from ready queue
        {
            let mut ready_queue = self.ready_queue.lock();
            ready_queue.retain(|&t| t != tid);
        }

        Ok(())
    }

    /// Unblock a thread
    pub fn unblock_thread(&self, tid: Tid) -> Result<(), &'static str> {
        {
            let mut threads = self.threads.write();
            if let Some(tcb) = threads.get_mut(&tid) {
                tcb.set_state(ThreadState::Ready);
                tcb.wait_reason = WaitReason::None;
            } else {
                return Err("Thread not found");
            }
        }

        // Add back to ready queue
        {
            let mut ready_queue = self.ready_queue.lock();
            ready_queue.push_back(tid);
        }

        Ok(())
    }

    /// Sleep a thread for specified duration
    pub fn sleep_thread(&self, tid: Tid, duration_ms: u64) -> Result<(), &'static str> {
        {
            let mut threads = self.threads.write();
            if let Some(tcb) = threads.get_mut(&tid) {
                tcb.sleep(duration_ms);
            } else {
                return Err("Thread not found");
            }
        }

        // Remove from ready queue and add to sleeping queue
        {
            let mut ready_queue = self.ready_queue.lock();
            ready_queue.retain(|&t| t != tid);
        }

        {
            let mut sleeping_threads = self.sleeping_threads.lock();
            sleeping_threads.push(tid);
        }

        Ok(())
    }

    /// Wake up sleeping threads
    pub fn wake_sleeping_threads(&self) {
        let current_time = get_system_time();
        let mut threads_to_wake = Vec::new();

        {
            let sleeping_threads = self.sleeping_threads.lock();
            let threads = self.threads.read();

            for &tid in sleeping_threads.iter() {
                if let Some(tcb) = threads.get(&tid) {
                    if tcb.should_wake_up(current_time) {
                        threads_to_wake.push(tid);
                    }
                }
            }
        }

        for tid in threads_to_wake {
            {
                let mut threads = self.threads.write();
                if let Some(tcb) = threads.get_mut(&tid) {
                    tcb.wake_up();
                }
            }

            // Move from sleeping to ready queue
            {
                let mut sleeping_threads = self.sleeping_threads.lock();
                sleeping_threads.retain(|&t| t != tid);
            }

            {
                let mut ready_queue = self.ready_queue.lock();
                ready_queue.push_back(tid);
            }
        }
    }

    /// Get all threads for a process
    pub fn get_process_threads(&self, pid: Pid) -> Vec<Tid> {
        let process_threads = self.process_threads.read();
        process_threads.get(&pid).cloned().unwrap_or_default()
    }

    /// Get thread count
    pub fn thread_count(&self) -> usize {
        self.thread_count.load(Ordering::SeqCst)
    }

    /// Get next ready thread
    pub fn get_next_ready_thread(&self) -> Option<Tid> {
        let mut ready_queue = self.ready_queue.lock();
        ready_queue.pop_front()
    }

    /// Allocate kernel stack
    fn allocate_stack(&self, size: usize) -> Result<u64, &'static str> {
        use crate::memory::{get_memory_manager, MemoryProtection, MemoryRegionType};

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;
        let region = memory_manager
            .allocate_region(
                size,
                MemoryRegionType::KernelStack,
                MemoryProtection::KERNEL_DATA,
            )
            .map_err(|_| "Failed to allocate kernel stack")?;

        Ok(region.start.as_u64())
    }

    /// Allocate user stack
    fn allocate_user_stack(&self, size: usize) -> Result<u64, &'static str> {
        use crate::memory::{get_memory_manager, MemoryProtection, MemoryRegionType};

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;
        let region = memory_manager
            .allocate_region(
                size,
                MemoryRegionType::UserStack,
                MemoryProtection::USER_DATA,
            )
            .map_err(|_| "Failed to allocate user stack")?;

        Ok(region.start.as_u64())
    }

    /// List all threads
    pub fn list_threads(&self) -> Vec<(Tid, Pid, String, ThreadState, ThreadType)> {
        let threads = self.threads.read();
        threads
            .iter()
            .map(|(&tid, tcb)| {
                (
                    tid,
                    tcb.pid,
                    tcb.name_str().to_string(),
                    tcb.state,
                    tcb.thread_type,
                )
            })
            .collect()
    }

    /// Get synchronization object manager
    pub fn get_sync_objects(&self) -> &Mutex<SyncObjectManager> {
        &self.sync_objects
    }

    /// Get current running thread ID
    pub fn current_thread(&self) -> Tid {
        self.current_thread.load(Ordering::SeqCst) as Tid
    }

    /// Set current running thread ID (called by scheduler during context switch)
    pub fn set_current_thread(&self, tid: Tid) {
        self.current_thread.store(tid as u32, Ordering::SeqCst);
    }

    /// Get current thread from CPU context (attempts to determine from stack or registers)
    pub fn get_current_thread_from_context() -> Tid {
        // In a real implementation, this would examine CPU registers or stack
        // to determine the current thread. For now, we use the stored value.
        THREAD_MANAGER.current_thread()
    }
}

/// Synchronization Object Manager
pub struct SyncObjectManager {
    /// Mutexes
    mutexes: BTreeMap<u32, KernelMutex>,
    /// Semaphores
    semaphores: BTreeMap<u32, KernelSemaphore>,
    /// Condition variables
    condition_variables: BTreeMap<u32, KernelConditionVariable>,
    /// Next object ID
    next_object_id: u32,
}

impl SyncObjectManager {
    pub const fn new() -> Self {
        Self {
            mutexes: BTreeMap::new(),
            semaphores: BTreeMap::new(),
            condition_variables: BTreeMap::new(),
            next_object_id: 1,
        }
    }

    /// Create a new mutex
    pub fn create_mutex(&mut self) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.mutexes.insert(id, KernelMutex::new());
        id
    }

    /// Create a new semaphore
    pub fn create_semaphore(&mut self, initial_count: u32) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.semaphores
            .insert(id, KernelSemaphore::new(initial_count));
        id
    }

    /// Create a new condition variable
    pub fn create_condition_variable(&mut self) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.condition_variables
            .insert(id, KernelConditionVariable::new());
        id
    }

    /// Lock a mutex
    pub fn lock_mutex(&mut self, mutex_id: u32, tid: Tid) -> Result<bool, &'static str> {
        if let Some(mutex) = self.mutexes.get_mut(&mutex_id) {
            Ok(mutex.lock(tid))
        } else {
            Err("Mutex not found")
        }
    }

    /// Unlock a mutex
    pub fn unlock_mutex(&mut self, mutex_id: u32, tid: Tid) -> Result<Option<Tid>, &'static str> {
        if let Some(mutex) = self.mutexes.get_mut(&mutex_id) {
            Ok(mutex.unlock(tid))
        } else {
            Err("Mutex not found")
        }
    }

    /// Acquire semaphore
    pub fn acquire_semaphore(&mut self, semaphore_id: u32, tid: Tid) -> Result<bool, &'static str> {
        if let Some(semaphore) = self.semaphores.get_mut(&semaphore_id) {
            Ok(semaphore.acquire(tid))
        } else {
            Err("Semaphore not found")
        }
    }

    /// Release semaphore
    pub fn release_semaphore(&mut self, semaphore_id: u32) -> Result<Option<Tid>, &'static str> {
        if let Some(semaphore) = self.semaphores.get_mut(&semaphore_id) {
            Ok(semaphore.release())
        } else {
            Err("Semaphore not found")
        }
    }
}

/// Kernel Mutex implementation
#[derive(Debug)]
pub struct KernelMutex {
    /// Current owner thread
    owner: Option<Tid>,
    /// Waiting threads
    waiters: VecDeque<Tid>,
}

impl KernelMutex {
    pub fn new() -> Self {
        Self {
            owner: None,
            waiters: VecDeque::new(),
        }
    }

    /// Try to lock the mutex
    pub fn lock(&mut self, tid: Tid) -> bool {
        if self.owner.is_none() {
            self.owner = Some(tid);
            true
        } else {
            self.waiters.push_back(tid);
            false
        }
    }

    /// Unlock the mutex
    pub fn unlock(&mut self, tid: Tid) -> Option<Tid> {
        if self.owner == Some(tid) {
            self.owner = self.waiters.pop_front();
            self.owner
        } else {
            None
        }
    }
}

/// Kernel Semaphore implementation
#[derive(Debug)]
pub struct KernelSemaphore {
    /// Current count
    count: u32,
    /// Waiting threads
    waiters: VecDeque<Tid>,
}

impl KernelSemaphore {
    pub fn new(initial_count: u32) -> Self {
        Self {
            count: initial_count,
            waiters: VecDeque::new(),
        }
    }

    /// Try to acquire the semaphore
    pub fn acquire(&mut self, tid: Tid) -> bool {
        if self.count > 0 {
            self.count -= 1;
            true
        } else {
            self.waiters.push_back(tid);
            false
        }
    }

    /// Release the semaphore
    pub fn release(&mut self) -> Option<Tid> {
        if let Some(waiter) = self.waiters.pop_front() {
            Some(waiter)
        } else {
            self.count += 1;
            None
        }
    }
}

/// Kernel Condition Variable implementation
#[derive(Debug)]
pub struct KernelConditionVariable {
    /// Waiting threads
    waiters: VecDeque<Tid>,
}

impl KernelConditionVariable {
    pub fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
        }
    }

    /// Wait on condition variable
    pub fn wait(&mut self, tid: Tid) {
        self.waiters.push_back(tid);
    }

    /// Signal one waiting thread
    pub fn signal(&mut self) -> Option<Tid> {
        self.waiters.pop_front()
    }

    /// Signal all waiting threads
    pub fn broadcast(&mut self) -> Vec<Tid> {
        let waiters: Vec<Tid> = self.waiters.drain(..).collect();
        waiters
    }
}

/// Thread statistics
#[derive(Debug, Clone)]
pub struct ThreadStats {
    pub total_threads: usize,
    pub running_threads: usize,
    pub blocked_threads: usize,
    pub sleeping_threads: usize,
    pub kernel_threads: usize,
    pub user_threads: usize,
}

/// Global thread manager instance
static THREAD_MANAGER: ThreadManager = ThreadManager::new();

/// Get the global thread manager
pub fn get_thread_manager() -> &'static ThreadManager {
    &THREAD_MANAGER
}

/// Initialize the thread management system
pub fn init() -> Result<(), &'static str> {
    THREAD_MANAGER.init()
}

/// Create a kernel thread
pub fn create_kernel_thread<F>(
    name: &str,
    priority: Priority,
    stack_size: usize,
    entry_point: F,
) -> Result<Tid, &'static str>
where
    F: FnOnce() + Send + 'static,
{
    THREAD_MANAGER.create_kernel_thread(name, priority, stack_size, entry_point)
}

/// Sleep current thread
pub fn sleep_ms(duration_ms: u64) -> Result<(), &'static str> {
    // Get the current thread ID from the thread manager
    let current_tid = THREAD_MANAGER.current_thread();
    THREAD_MANAGER.sleep_thread(current_tid, duration_ms)
}

/// Yield current thread
pub fn yield_thread() {
    // In a real implementation, this would trigger a reschedule
    // by calling the scheduler directly
}

/// Join on a thread (wait for it to complete)
pub fn join_thread(tid: Tid) -> Result<i32, &'static str> {
    // In a real implementation, this would block the current thread
    // until the target thread completes
    if let Some(tcb) = THREAD_MANAGER.get_thread(tid) {
        if let Some(exit_status) = tcb.exit_status {
            Ok(exit_status)
        } else {
            Err("Thread not yet terminated")
        }
    } else {
        Err("Thread not found")
    }
}
