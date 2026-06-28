//! Process Synchronization and Thread Safety
//!
//! This module provides synchronization primitives and thread safety mechanisms
//! for RustOS processes, including mutexes, semaphores, and condition variables.

use super::{get_process_manager, Pid};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};

/// Synchronization primitive types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncType {
    Mutex,
    Semaphore,
    ConditionVariable,
    RwLock,
    Barrier,
}

/// Unique identifier for synchronization objects
pub type SyncId = u32;

/// Process synchronization state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// Process is waiting for the synchronization object
    Waiting,
    /// Process has acquired the synchronization object
    Acquired,
    /// Process was woken up from waiting
    Woken,
}

/// Wait queue entry for blocked processes
#[derive(Debug, Clone)]
struct WaitQueueEntry {
    /// Process ID
    pid: Pid,
    /// Priority of the waiting process
    priority: super::Priority,
    /// Time when process started waiting
    wait_start_time: u64,
}

/// Generic synchronization object
#[derive(Debug)]
struct SyncObject {
    /// Type of synchronization primitive
    sync_type: SyncType,
    /// Unique identifier
    id: SyncId,
    /// Current state/value (meaning depends on type)
    value: AtomicU32,
    /// Maximum value (for semaphores)
    max_value: u32,
    /// Queue of waiting processes
    wait_queue: Mutex<VecDeque<WaitQueueEntry>>,
    /// Owner process (for mutexes)
    owner: AtomicU32,
    /// Reference count
    ref_count: AtomicUsize,
}

impl SyncObject {
    /// Create a new synchronization object
    fn new(sync_type: SyncType, id: SyncId, initial_value: u32, max_value: u32) -> Self {
        Self {
            sync_type,
            id,
            value: AtomicU32::new(initial_value),
            max_value,
            wait_queue: Mutex::new(VecDeque::new()),
            owner: AtomicU32::new(0),
            ref_count: AtomicUsize::new(1),
        }
    }

    /// Try to acquire the synchronization object
    fn try_acquire(&self, pid: Pid) -> Result<bool, &'static str> {
        match self.sync_type {
            SyncType::Mutex => self.try_acquire_mutex(pid),
            SyncType::Semaphore => self.try_acquire_semaphore(pid),
            SyncType::RwLock => self.try_acquire_rwlock(pid, false), // Read lock
            SyncType::ConditionVariable => self.try_wait_condvar(pid),
            SyncType::Barrier => self.try_wait_barrier(pid),
        }
    }

    /// Release the synchronization object
    fn release(&self, pid: Pid) -> Result<Vec<Pid>, &'static str> {
        match self.sync_type {
            SyncType::Mutex => self.release_mutex(pid),
            SyncType::Semaphore => self.release_semaphore(pid),
            SyncType::RwLock => self.release_rwlock(pid),
            SyncType::ConditionVariable => self.signal_condvar(pid),
            SyncType::Barrier => self.release_barrier(pid),
        }
    }

    /// Try to acquire mutex
    fn try_acquire_mutex(&self, pid: Pid) -> Result<bool, &'static str> {
        let current_owner = self.owner.load(Ordering::Acquire);

        if current_owner == 0 {
            // Mutex is free, try to acquire it
            match self
                .owner
                .compare_exchange(0, pid, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.value.store(1, Ordering::Release);
                    Ok(true)
                }
                Err(_) => Ok(false), // Someone else got it first
            }
        } else if current_owner == pid {
            // Already owned by this process (recursive mutex)
            let current_count = self.value.fetch_add(1, Ordering::AcqRel);
            if current_count < u32::MAX {
                Ok(true)
            } else {
                self.value.fetch_sub(1, Ordering::AcqRel);
                Err("Mutex recursion limit exceeded")
            }
        } else {
            // Owned by another process
            Ok(false)
        }
    }

    /// Release mutex
    fn release_mutex(&self, pid: Pid) -> Result<Vec<Pid>, &'static str> {
        let current_owner = self.owner.load(Ordering::Acquire);

        if current_owner != pid {
            return Err("Process does not own this mutex");
        }

        let current_count = self.value.load(Ordering::Acquire);
        if current_count == 0 {
            return Err("Mutex not acquired");
        }

        if current_count == 1 {
            // Last reference, release the mutex
            self.owner.store(0, Ordering::Release);
            self.value.store(0, Ordering::Release);

            // Wake up waiting processes
            let mut wait_queue = self.wait_queue.lock();
            if let Some(entry) = wait_queue.pop_front() {
                Ok(vec![entry.pid])
            } else {
                Ok(vec![])
            }
        } else {
            // Recursive release
            self.value.fetch_sub(1, Ordering::AcqRel);
            Ok(vec![])
        }
    }

    /// Try to acquire semaphore
    fn try_acquire_semaphore(&self, _pid: Pid) -> Result<bool, &'static str> {
        let current_value = self.value.load(Ordering::Acquire);

        if current_value > 0 {
            match self.value.compare_exchange(
                current_value,
                current_value - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false), // Value changed, try again later
            }
        } else {
            Ok(false)
        }
    }

    /// Release semaphore
    fn release_semaphore(&self, _pid: Pid) -> Result<Vec<Pid>, &'static str> {
        // ponytail: CAS loop so two concurrent releases can't both observe
        // value < max and then both fetch_add, exceeding max_value. The
        // increment only commits while value is still below max at compare time.
        loop {
            let current_value = self.value.load(Ordering::Acquire);
            if current_value >= self.max_value {
                return Err("Semaphore value exceeds maximum");
            }
            if self
                .value
                .compare_exchange(
                    current_value,
                    current_value + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break;
            }
        }

        // Wake up one waiting process
        let mut wait_queue = self.wait_queue.lock();
        if let Some(entry) = wait_queue.pop_front() {
            Ok(vec![entry.pid])
        } else {
            Ok(vec![])
        }
    }

    /// Try to acquire read/write lock
    fn try_acquire_rwlock(&self, pid: Pid, write_lock: bool) -> Result<bool, &'static str> {
        let current_value = self.value.load(Ordering::Acquire);

        if write_lock {
            // Try to acquire write lock
            if current_value == 0 {
                match self.value.compare_exchange(
                    0,
                    0x80000000,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        self.owner.store(pid, Ordering::Release);
                        Ok(true)
                    }
                    Err(_) => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            // Try to acquire read lock
            if (current_value & 0x80000000) == 0 && current_value < 0x7FFFFFFF {
                match self.value.compare_exchange(
                    current_value,
                    current_value + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
    }

    /// Release read/write lock
    fn release_rwlock(&self, pid: Pid) -> Result<Vec<Pid>, &'static str> {
        let current_value = self.value.load(Ordering::Acquire);

        if (current_value & 0x80000000) != 0 {
            // Write lock held
            if self.owner.load(Ordering::Acquire) != pid {
                return Err("Process does not own this write lock");
            }

            self.owner.store(0, Ordering::Release);
            self.value.store(0, Ordering::Release);
        } else if current_value > 0 {
            // Read lock held
            self.value.fetch_sub(1, Ordering::AcqRel);
        } else {
            return Err("No lock held");
        }

        // Wake up waiting processes
        let mut wait_queue = self.wait_queue.lock();
        let mut to_wake = Vec::new();

        // For rwlock, we might wake multiple readers or one writer
        if current_value == 1 || (current_value & 0x80000000) != 0 {
            // Last read lock or write lock released, wake appropriate waiters
            if let Some(entry) = wait_queue.pop_front() {
                to_wake.push(entry.pid);
            }
        }

        Ok(to_wake)
    }

    /// Try to wait on a condition variable.
    ///
    /// For a condition variable, "acquire" means the caller wants to block
    /// until the condition is signaled. We add the process to the wait queue
    /// and report `false` (not yet acquired — the caller should block).
    fn try_wait_condvar(&self, pid: Pid) -> Result<bool, &'static str> {
        self.add_to_wait_queue(pid, super::Priority::Normal);
        Ok(false)
    }

    /// Signal a condition variable.
    ///
    /// "Release" on a condition variable wakes one waiting process
    /// (signal). If the `value` counter is non-zero, a broadcast is
    /// requested instead and all waiters are woken.
    fn signal_condvar(&self, _pid: Pid) -> Result<Vec<Pid>, &'static str> {
        let mut wait_queue = self.wait_queue.lock();
        let broadcast = self.value.load(Ordering::Acquire) != 0;

        if broadcast {
            let to_wake: Vec<Pid> = wait_queue.drain(..).map(|e| e.pid).collect();
            Ok(to_wake)
        } else if let Some(entry) = wait_queue.pop_front() {
            Ok(vec![entry.pid])
        } else {
            Ok(vec![])
        }
    }

    /// Try to wait at a barrier.
    ///
    /// For a barrier, "acquire" means the calling process arrives at the
    /// barrier. The `value` counter tracks how many processes have arrived;
    /// `max_value` is the barrier threshold. When the count reaches the
    /// threshold, all waiting processes are released.
    fn try_wait_barrier(&self, pid: Pid) -> Result<bool, &'static str> {
        let current = self.value.fetch_add(1, Ordering::AcqRel);
        let target = self.max_value;

        if current + 1 >= target {
            // Barrier tripped — all waiters can proceed.
            Ok(true)
        } else {
            // Not enough arrivals yet; queue this process.
            self.add_to_wait_queue(pid, super::Priority::Normal);
            Ok(false)
        }
    }

    /// Release a barrier.
    ///
    /// When the barrier threshold has been reached, "release" wakes all
    /// waiting processes and resets the arrival counter for reuse.
    fn release_barrier(&self, _pid: Pid) -> Result<Vec<Pid>, &'static str> {
        let current = self.value.load(Ordering::Acquire);
        if current < self.max_value {
            // Barrier not yet tripped; nothing to release.
            return Ok(vec![]);
        }

        // Wake all waiters.
        let mut wait_queue = self.wait_queue.lock();
        let to_wake: Vec<Pid> = wait_queue.drain(..).map(|e| e.pid).collect();

        // Reset the arrival counter so the barrier can be reused.
        self.value.store(0, Ordering::Release);

        Ok(to_wake)
    }

    /// Add process to wait queue
    fn add_to_wait_queue(&self, pid: Pid, priority: super::Priority) {
        let mut wait_queue = self.wait_queue.lock();
        let entry = WaitQueueEntry {
            pid,
            priority,
            wait_start_time: super::get_system_time(),
        };

        // Insert in priority order (higher priority first)
        let insert_pos = wait_queue
            .iter()
            .position(|e| e.priority > priority)
            .unwrap_or(wait_queue.len());

        wait_queue.insert(insert_pos, entry);
    }

    /// Remove process from wait queue
    fn remove_from_wait_queue(&self, pid: Pid) -> bool {
        let mut wait_queue = self.wait_queue.lock();
        if let Some(pos) = wait_queue.iter().position(|e| e.pid == pid) {
            wait_queue.remove(pos);
            true
        } else {
            false
        }
    }
}

/// Synchronization manager
pub struct SyncManager {
    /// All synchronization objects
    objects: RwLock<BTreeMap<SyncId, SyncObject>>,
    /// Next sync ID to allocate
    next_id: AtomicU32,
    /// Process synchronization states
    process_states: RwLock<BTreeMap<Pid, BTreeMap<SyncId, SyncState>>>,
    /// Deadlock detection
    deadlock_detector: Mutex<DeadlockDetector>,
}

impl SyncManager {
    /// Create a new synchronization manager
    pub const fn new() -> Self {
        Self {
            objects: RwLock::new(BTreeMap::new()),
            next_id: AtomicU32::new(1),
            process_states: RwLock::new(BTreeMap::new()),
            deadlock_detector: Mutex::new(DeadlockDetector::new()),
        }
    }

    /// Create a mutex
    pub fn create_mutex(&self) -> SyncId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mutex = SyncObject::new(SyncType::Mutex, id, 0, 1);

        {
            let mut objects = self.objects.write();
            objects.insert(id, mutex);
        }

        id
    }

    /// Create a semaphore
    pub fn create_semaphore(&self, initial_value: u32, max_value: u32) -> SyncId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let semaphore = SyncObject::new(SyncType::Semaphore, id, initial_value, max_value);

        {
            let mut objects = self.objects.write();
            objects.insert(id, semaphore);
        }

        id
    }

    /// Create a read-write lock
    pub fn create_rwlock(&self) -> SyncId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let rwlock = SyncObject::new(SyncType::RwLock, id, 0, 0x7FFFFFFF);

        {
            let mut objects = self.objects.write();
            objects.insert(id, rwlock);
        }

        id
    }

    /// Create a condition variable.
    ///
    /// The `value` field is used as a broadcast flag: when non-zero,
    /// `release` wakes all waiters instead of just one.
    pub fn create_condvar(&self) -> SyncId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let condvar = SyncObject::new(SyncType::ConditionVariable, id, 0, 1);

        {
            let mut objects = self.objects.write();
            objects.insert(id, condvar);
        }

        id
    }

    /// Create a barrier that waits for `count` processes to arrive.
    pub fn create_barrier(&self, count: u32) -> SyncId {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let barrier = SyncObject::new(SyncType::Barrier, id, 0, count);

        {
            let mut objects = self.objects.write();
            objects.insert(id, barrier);
        }

        id
    }

    /// Acquire a synchronization object
    pub fn acquire(&self, sync_id: SyncId, pid: Pid) -> Result<bool, &'static str> {
        let objects = self.objects.read();
        let sync_obj = objects.get(&sync_id).ok_or("Invalid sync object ID")?;

        // Check for deadlock before attempting acquisition
        {
            let mut detector = self.deadlock_detector.lock();
            if detector.would_deadlock(pid, sync_id, &objects)? {
                return Err("Deadlock detected");
            }
        }

        match sync_obj.try_acquire(pid) {
            Ok(true) => {
                // Successfully acquired
                {
                    let mut states = self.process_states.write();
                    states
                        .entry(pid)
                        .or_insert_with(BTreeMap::new)
                        .insert(sync_id, SyncState::Acquired);
                }
                Ok(true)
            }
            Ok(false) => {
                // Need to wait
                let process_manager = get_process_manager();
                if let Some(pcb) = process_manager.get_process(pid) {
                    sync_obj.add_to_wait_queue(pid, pcb.priority);
                    process_manager.block_process(pid)?;

                    {
                        let mut states = self.process_states.write();
                        states
                            .entry(pid)
                            .or_insert_with(BTreeMap::new)
                            .insert(sync_id, SyncState::Waiting);
                    }
                }
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Release a synchronization object
    pub fn release(&self, sync_id: SyncId, pid: Pid) -> Result<(), &'static str> {
        let objects = self.objects.read();
        let sync_obj = objects.get(&sync_id).ok_or("Invalid sync object ID")?;

        let processes_to_wake = sync_obj.release(pid)?;

        // Update process state
        {
            let mut states = self.process_states.write();
            if let Some(process_states) = states.get_mut(&pid) {
                process_states.remove(&sync_id);
            }
        }

        // Wake up waiting processes
        let process_manager = get_process_manager();
        for wake_pid in processes_to_wake {
            process_manager.unblock_process(wake_pid)?;

            {
                let mut states = self.process_states.write();
                if let Some(process_states) = states.get_mut(&wake_pid) {
                    process_states.insert(sync_id, SyncState::Woken);
                }
            }
        }

        Ok(())
    }

    /// Destroy a synchronization object
    pub fn destroy(&self, sync_id: SyncId) -> Result<(), &'static str> {
        let mut objects = self.objects.write();
        if let Some(sync_obj) = objects.remove(&sync_id) {
            // Wake up all waiting processes with an error
            let mut wait_queue = sync_obj.wait_queue.lock();
            let process_manager = get_process_manager();

            while let Some(entry) = wait_queue.pop_front() {
                let _ = process_manager.unblock_process(entry.pid);
            }

            Ok(())
        } else {
            Err("Invalid sync object ID")
        }
    }

    /// Get synchronization statistics
    pub fn get_stats(&self) -> SyncStats {
        let objects = self.objects.read();
        let states = self.process_states.read();

        let mut stats = SyncStats::default();

        for (_, obj) in objects.iter() {
            match obj.sync_type {
                SyncType::Mutex => stats.mutex_count += 1,
                SyncType::Semaphore => stats.semaphore_count += 1,
                SyncType::RwLock => stats.rwlock_count += 1,
                _ => stats.other_count += 1,
            }

            let wait_queue = obj.wait_queue.lock();
            stats.total_waiting += wait_queue.len();
        }

        stats.total_objects = objects.len();
        stats.process_states = states.len();

        stats
    }
}

/// Deadlock detection system
#[derive(Debug)]
struct DeadlockDetector {
    /// Resource allocation graph (process -> resource)
    allocation_graph: BTreeMap<Pid, Vec<SyncId>>,
    /// Resource waiting graph (process -> resource)
    waiting_graph: BTreeMap<Pid, Vec<SyncId>>,
}

impl DeadlockDetector {
    const fn new() -> Self {
        Self {
            allocation_graph: BTreeMap::new(),
            waiting_graph: BTreeMap::new(),
        }
    }

    /// Check if acquiring a resource would cause deadlock
    fn would_deadlock(
        &mut self,
        pid: Pid,
        sync_id: SyncId,
        objects: &BTreeMap<SyncId, SyncObject>,
    ) -> Result<bool, &'static str> {
        // Simple deadlock detection: check for cycles in wait-for graph

        // Add the potential wait edge
        self.waiting_graph
            .entry(pid)
            .or_insert_with(Vec::new)
            .push(sync_id);

        // Check for cycles using DFS
        let result = self.has_cycle(pid, objects);

        // Remove the potential wait edge
        if let Some(waiting_list) = self.waiting_graph.get_mut(&pid) {
            waiting_list.retain(|&id| id != sync_id);
            if waiting_list.is_empty() {
                self.waiting_graph.remove(&pid);
            }
        }

        Ok(result)
    }

    /// Check for cycles in the wait-for graph
    fn has_cycle(&self, start_pid: Pid, objects: &BTreeMap<SyncId, SyncObject>) -> bool {
        let mut visited = BTreeMap::new();
        let mut recursion_stack = BTreeMap::new();

        self.dfs_cycle_check(start_pid, objects, &mut visited, &mut recursion_stack)
    }

    /// Depth-first search for cycle detection
    fn dfs_cycle_check(
        &self,
        pid: Pid,
        objects: &BTreeMap<SyncId, SyncObject>,
        visited: &mut BTreeMap<Pid, bool>,
        recursion_stack: &mut BTreeMap<Pid, bool>,
    ) -> bool {
        visited.insert(pid, true);
        recursion_stack.insert(pid, true);

        // Check all resources this process is waiting for
        if let Some(waiting_resources) = self.waiting_graph.get(&pid) {
            for &sync_id in waiting_resources {
                if let Some(sync_obj) = objects.get(&sync_id) {
                    let owner_pid = sync_obj.owner.load(Ordering::Acquire);

                    if owner_pid != 0 {
                        // Resource is owned by another process
                        if !visited.get(&owner_pid).copied().unwrap_or(false) {
                            if self.dfs_cycle_check(owner_pid, objects, visited, recursion_stack) {
                                return true;
                            }
                        } else if recursion_stack.get(&owner_pid).copied().unwrap_or(false) {
                            // Cycle detected
                            return true;
                        }
                    }
                }
            }
        }

        recursion_stack.insert(pid, false);
        false
    }
}

/// Synchronization statistics
#[derive(Debug, Default)]
pub struct SyncStats {
    pub total_objects: usize,
    pub mutex_count: usize,
    pub semaphore_count: usize,
    pub rwlock_count: usize,
    pub other_count: usize,
    pub total_waiting: usize,
    pub process_states: usize,
}

/// Global synchronization manager
static SYNC_MANAGER: SyncManager = SyncManager::new();

/// Get the global synchronization manager
pub fn get_sync_manager() -> &'static SyncManager {
    &SYNC_MANAGER
}

/// Initialize the synchronization system
pub fn init() -> Result<(), &'static str> {
    // Synchronization manager is ready to use
    Ok(())
}
