//! Task Scheduler Implementation
//!
//! This module implements multiple scheduling algorithms including round-robin
//! and priority-based scheduling for RustOS processes.

use super::{get_system_time, Pid, Priority};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

/// Scheduling algorithm types
#[derive(Debug, Clone, Copy)]
pub enum SchedulingAlgorithm {
    /// Round-robin scheduling
    RoundRobin,
    /// Priority-based scheduling
    Priority,
    /// Multilevel feedback queue
    MultilevelFeedback,
}

/// Process queue for scheduling
#[derive(Debug)]
struct ProcessQueue {
    /// Processes in this queue
    processes: VecDeque<Pid>,
    /// Time slice for processes in this queue (ms)
    time_slice: u32,
    /// Priority level of this queue
    priority: Priority,
}

impl ProcessQueue {
    fn new(priority: Priority, time_slice: u32) -> Self {
        Self {
            processes: VecDeque::new(),
            time_slice,
            priority,
        }
    }

    fn add_process(&mut self, pid: Pid) {
        if !self.processes.contains(&pid) {
            self.processes.push_back(pid);
        }
    }

    fn remove_process(&mut self, pid: Pid) -> bool {
        let old_len = self.processes.len();
        self.processes.retain(|&p| p != pid);
        self.processes.len() != old_len
    }

    fn next_process(&mut self) -> Option<Pid> {
        self.processes.pop_front()
    }

    fn rotate_to_back(&mut self, pid: Pid) {
        self.remove_process(pid);
        self.processes.push_back(pid);
    }

    fn is_empty(&self) -> bool {
        self.processes.is_empty()
    }

    fn len(&self) -> usize {
        self.processes.len()
    }
}

/// Process scheduling statistics
#[derive(Debug, Clone)]
pub struct SchedulingStats {
    /// Total context switches
    pub context_switches: u64,
    /// Total scheduling decisions
    pub scheduling_decisions: u64,
    /// Average wait time per process
    pub average_wait_time: f32,
    /// CPU utilization percentage
    pub cpu_utilization: f32,
    /// Last scheduling decision time
    pub last_schedule_time: u64,
}

impl Default for SchedulingStats {
    fn default() -> Self {
        Self {
            context_switches: 0,
            scheduling_decisions: 0,
            average_wait_time: 0.0,
            cpu_utilization: 0.0,
            last_schedule_time: 0,
        }
    }
}

/// Main scheduler implementation
pub struct Scheduler {
    /// Current scheduling algorithm
    algorithm: SchedulingAlgorithm,
    /// Process queues by priority
    queues: BTreeMap<Priority, ProcessQueue>,
    /// Currently running process
    current_process: Option<Pid>,
    /// Process information for scheduling
    process_info: BTreeMap<Pid, ProcessSchedulingInfo>,
    /// Scheduling statistics
    stats: SchedulingStats,
    /// Time slice counter for current process
    current_time_slice: u32,
    /// Maximum time slice in ms
    max_time_slice: u32,
    /// Minimum time slice in ms
    min_time_slice: u32,
}

/// Per-process scheduling information
#[derive(Debug, Clone)]
struct ProcessSchedulingInfo {
    /// Process priority
    priority: Priority,
    /// Time when process was last scheduled
    last_scheduled: u64,
    /// Total CPU time used
    total_cpu_time: u64,
    /// Time when process became ready
    ready_time: u64,
    /// Number of times process has been scheduled
    schedule_count: u64,
    /// Process is currently blocked
    blocked: bool,
}

impl Scheduler {
    /// Create a new scheduler
    pub const fn new() -> Self {
        Self {
            algorithm: SchedulingAlgorithm::MultilevelFeedback,
            queues: BTreeMap::new(),
            current_process: None,
            process_info: BTreeMap::new(),
            stats: SchedulingStats {
                context_switches: 0,
                scheduling_decisions: 0,
                average_wait_time: 0.0,
                cpu_utilization: 0.0,
                last_schedule_time: 0,
            },
            current_time_slice: 0,
            max_time_slice: 100, // 100ms
            min_time_slice: 5,   // 5ms
        }
    }

    /// Initialize the scheduler
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Initialize priority queues
        self.queues.insert(
            Priority::RealTime,
            ProcessQueue::new(Priority::RealTime, 50),
        );
        self.queues
            .insert(Priority::High, ProcessQueue::new(Priority::High, 25));
        self.queues
            .insert(Priority::Normal, ProcessQueue::new(Priority::Normal, 10));
        self.queues
            .insert(Priority::Low, ProcessQueue::new(Priority::Low, 5));
        self.queues
            .insert(Priority::Idle, ProcessQueue::new(Priority::Idle, 1));

        self.stats.last_schedule_time = get_system_time();
        Ok(())
    }

    fn remove_from_all_queues(&mut self, pid: Pid) {
        for queue in self.queues.values_mut() {
            queue.remove_process(pid);
        }
    }

    /// Add a process to the scheduler
    pub fn add_process(&mut self, pid: Pid, priority: Priority) -> Result<(), &'static str> {
        self.remove_from_all_queues(pid);

        // Add process info
        self.process_info.insert(
            pid,
            ProcessSchedulingInfo {
                priority,
                last_scheduled: 0,
                total_cpu_time: 0,
                ready_time: get_system_time(),
                schedule_count: 0,
                blocked: false,
            },
        );

        // Add to appropriate queue
        if let Some(queue) = self.queues.get_mut(&priority) {
            queue.add_process(pid);
        } else {
            return Err("Invalid priority level");
        }

        Ok(())
    }

    /// Remove a process from the scheduler
    pub fn remove_process(&mut self, pid: Pid) -> Result<(), &'static str> {
        // Remove from process info
        if self.process_info.remove(&pid).is_some() {
            self.remove_from_all_queues(pid);

            // If this was the current process, clear it
            if self.current_process == Some(pid) {
                self.current_process = None;
                self.current_time_slice = 0;
            }
        } else {
            return Err("Process not found in scheduler");
        }

        Ok(())
    }

    /// Block a process (remove from ready queue but keep info)
    pub fn block_process(&mut self, pid: Pid) -> Result<(), &'static str> {
        if let Some(info) = self.process_info.get_mut(&pid) {
            info.blocked = true;

            self.remove_from_all_queues(pid);

            // If this was the current process, clear it
            if self.current_process == Some(pid) {
                self.current_process = None;
                self.current_time_slice = 0;
            }
        } else {
            return Err("Process not found in scheduler");
        }

        Ok(())
    }

    /// Unblock a process (add back to ready queue)
    pub fn unblock_process(&mut self, pid: Pid) -> Result<(), &'static str> {
        if let Some(info) = self.process_info.get_mut(&pid) {
            info.blocked = false;
            info.ready_time = get_system_time();
            let priority = info.priority;

            self.remove_from_all_queues(pid);

            // Add back to queue
            if let Some(queue) = self.queues.get_mut(&priority) {
                queue.add_process(pid);
            }
        } else {
            return Err("Process not found in scheduler");
        }

        Ok(())
    }

    /// Perform scheduling decision
    pub fn schedule(&mut self) -> Result<Option<Pid>, &'static str> {
        self.stats.scheduling_decisions += 1;
        let current_time = get_system_time();

        if let Some(current_pid) = self.current_process {
            let current_is_runnable = self
                .process_info
                .get(&current_pid)
                .map(|info| !info.blocked)
                .unwrap_or(false);

            if !current_is_runnable {
                self.current_process = None;
                self.current_time_slice = 0;
            }
        }

        // Check if current process should be preempted
        let should_preempt = self.should_preempt(current_time);

        if !should_preempt && self.current_process.is_some() {
            // Continue with current process
            return Ok(self.current_process);
        }

        // If we're preempting, put current process back in queue
        if let Some(current_pid) = self.current_process {
            if let Some(info) = self.process_info.get(&current_pid) {
                if !info.blocked {
                    let priority = info.priority;
                    self.remove_from_all_queues(current_pid);
                    if let Some(queue) = self.queues.get_mut(&priority) {
                        queue.rotate_to_back(current_pid);
                    }
                }
            }
        }

        // Select next process based on algorithm
        let next_process = match self.algorithm {
            SchedulingAlgorithm::RoundRobin => self.round_robin_schedule(),
            SchedulingAlgorithm::Priority => self.priority_schedule(),
            SchedulingAlgorithm::MultilevelFeedback => self.multilevel_feedback_schedule(),
        };

        // Update scheduling info for selected process
        if let Some(pid) = next_process {
            let mut wait_info = None;

            if let Some(info) = self.process_info.get_mut(&pid) {
                info.last_scheduled = current_time;
                info.schedule_count += 1;

                wait_info = Some((current_time.saturating_sub(info.ready_time), info.priority));
            }

            if let Some((wait_time, priority)) = wait_info {
                self.update_average_wait_time(wait_time as f32);
                self.current_time_slice = self
                    .queues
                    .get(&priority)
                    .map(|q| q.time_slice)
                    .unwrap_or(self.min_time_slice);
            }

            // Update context switch count if process changed
            if self.current_process != next_process {
                self.stats.context_switches += 1;
            }
        }

        self.current_process = next_process;
        self.stats.last_schedule_time = current_time;

        Ok(next_process)
    }

    /// Check if current process should be preempted
    fn should_preempt(&self, current_time: u64) -> bool {
        if self.current_process.is_none() {
            return true;
        }

        // Time slice expired
        let time_since_schedule = current_time.saturating_sub(self.stats.last_schedule_time);
        if time_since_schedule >= self.current_time_slice as u64 {
            return true;
        }

        // Higher priority process available
        if let Some(current_pid) = self.current_process {
            if let Some(current_info) = self.process_info.get(&current_pid) {
                // Check if any higher priority queue has processes
                for (&priority, queue) in &self.queues {
                    if priority < current_info.priority && !queue.is_empty() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Round-robin scheduling
    fn round_robin_schedule(&mut self) -> Option<Pid> {
        // Find first non-empty queue
        let process_info = &self.process_info;
        for (_, queue) in self.queues.iter_mut() {
            while let Some(pid) = queue.next_process() {
                if let Some(info) = process_info.get(&pid) {
                    if !info.blocked {
                        return Some(pid);
                    }
                }
            }
        }
        None
    }

    /// Priority-based scheduling
    fn priority_schedule(&mut self) -> Option<Pid> {
        // Start from highest priority queue
        let process_info = &self.process_info;
        for (_, queue) in self.queues.iter_mut() {
            while let Some(pid) = queue.next_process() {
                if let Some(info) = process_info.get(&pid) {
                    if !info.blocked {
                        return Some(pid);
                    }
                }
            }
        }
        None
    }

    /// Multilevel feedback queue scheduling with aging
    fn multilevel_feedback_schedule(&mut self) -> Option<Pid> {
        // Aging: promote processes that have waited too long to prevent
        // starvation.  A process that has been ready for longer than the
        // aging threshold (10x its queue's time slice) is moved to a
        // higher-priority queue before the selection pass.
        const AGING_FACTOR: u32 = 10;
        let now = get_system_time();

        // Collect promotions to apply (can't mutate queues while iterating)
        let mut promotions: Vec<(Pid, Priority, Priority)> = Vec::new();
        for (&priority, queue) in &self.queues {
            let aging_threshold = queue.time_slice.saturating_mul(AGING_FACTOR) as u64;
            for &pid in &queue.processes {
                if let Some(info) = self.process_info.get(&pid) {
                    if info.blocked {
                        continue;
                    }

                    let wait_time = now.saturating_sub(info.ready_time);
                    if wait_time > aging_threshold {
                        // Promote to the next higher priority if possible
                        let higher = match priority {
                            Priority::Idle => Some(Priority::Low),
                            Priority::Low => Some(Priority::Normal),
                            Priority::Normal => Some(Priority::High),
                            Priority::High => Some(Priority::RealTime),
                            Priority::RealTime => None,
                        };
                        if let Some(new_pri) = higher {
                            promotions.push((pid, priority, new_pri));
                        }
                    }
                }
            }
        }

        // Apply promotions
        for (pid, old_pri, new_pri) in promotions {
            if let Some(queue) = self.queues.get_mut(&old_pri) {
                queue.remove_process(pid);
            }
            if let Some(queue) = self.queues.get_mut(&new_pri) {
                queue.add_process(pid);
            }
            if let Some(info) = self.process_info.get_mut(&pid) {
                info.priority = new_pri;
                info.ready_time = now; // Reset wait timer after promotion
            }
        }

        // Now select from highest priority queue first
        self.priority_schedule()
    }

    /// Update average wait time
    fn update_average_wait_time(&mut self, new_wait_time: f32) {
        let total_decisions = self.stats.scheduling_decisions as f32;
        self.stats.average_wait_time = (self.stats.average_wait_time * (total_decisions - 1.0)
            + new_wait_time)
            / total_decisions;
    }

    /// Get current process
    pub fn current_process(&self) -> Option<Pid> {
        self.current_process
    }

    /// Get scheduling statistics
    pub fn get_stats(&self) -> &SchedulingStats {
        &self.stats
    }

    /// Returns true when the current time slice has counted down to zero,
    /// indicating that the running process should be preempted.
    pub fn time_slice_expired(&self) -> bool {
        self.current_time_slice == 0
    }

    /// Set scheduling algorithm
    pub fn set_algorithm(&mut self, algorithm: SchedulingAlgorithm) {
        self.algorithm = algorithm;
    }

    /// Get ready queue length
    pub fn ready_queue_length(&self) -> usize {
        self.queues.values().map(|q| q.len()).sum()
    }

    /// Update process priority (for priority inheritance, etc.)
    pub fn update_process_priority(
        &mut self,
        pid: Pid,
        new_priority: Priority,
    ) -> Result<(), &'static str> {
        let blocked = if let Some(info) = self.process_info.get_mut(&pid) {
            info.priority = new_priority;
            info.blocked
        } else {
            return Err("Process not found");
        };

        // Move process between queues if not blocked
        if !blocked {
            self.remove_from_all_queues(pid);

            // Add to new queue
            if let Some(new_queue) = self.queues.get_mut(&new_priority) {
                new_queue.add_process(pid);
            }
        }

        Ok(())
    }

    /// Tick the scheduler (called by timer interrupt)
    pub fn tick(&mut self) {
        // Decrement current time slice
        if self.current_time_slice > 0 {
            self.current_time_slice -= 1;
        }

        // Update CPU utilization
        let current_time = get_system_time();
        let time_diff = current_time.saturating_sub(self.stats.last_schedule_time);
        if time_diff > 0 {
            let utilization = if self.current_process.is_some() {
                100.0
            } else {
                0.0
            };
            self.stats.cpu_utilization = (self.stats.cpu_utilization * 0.9) + (utilization * 0.1);
        }

        // Update process CPU time
        if let Some(current_pid) = self.current_process {
            if let Some(info) = self.process_info.get_mut(&current_pid) {
                info.total_cpu_time += 1;
            }
        }
    }
}

/// Process scheduling statistics
#[derive(Debug, Clone)]
pub struct ProcessSchedulingStats {
    pub pid: Pid,
    pub priority: Priority,
    pub nice: i8,
    pub weight: u32,
    pub vruntime: u64,
    pub total_cpu_time: u64,
    pub schedule_count: u64,
    pub blocked: bool,
    pub cpu_affinity: u64,
}

/// Global scheduler functions for external access

/// Create a new process and add it to the scheduler
pub fn create_process(
    parent_pid: Option<Pid>,
    priority: Priority,
    name: &str,
) -> Result<Pid, &'static str> {
    let process_manager = super::get_process_manager();
    process_manager.create_process(name, parent_pid, priority)
}

/// Schedule the next process to run (called by timer interrupt)
pub fn schedule() -> Result<Option<Pid>, &'static str> {
    let process_manager = super::get_process_manager();
    process_manager.schedule()
}

/// Get scheduler statistics
pub fn get_scheduler_stats() -> SchedulingStats {
    let process_manager = super::get_process_manager();
    let scheduler = process_manager.scheduler.lock();
    scheduler.get_stats().clone()
}

/// Timer tick notification to scheduler
pub fn timer_tick(_delta_ms: u64) {
    let process_manager = super::get_process_manager();
    let pid = process_manager.current_process();
    if pid != 0 {
        crate::cgroup::charge_cpu_time(pid, _delta_ms.saturating_mul(1_000_000));
    }
    let mut scheduler = process_manager.scheduler.lock();
    scheduler.tick();
}

/// Yield the CPU to the next process (cooperative multitasking)
/// This is the missing function that was referenced in interrupts.rs
pub fn yield_cpu() {
    crate::user_sched::service_pending(yield_cpu_sched_tail as *const () as u64);
    yield_cpu_sched_tail();
}

/// Called from the timer ISR (after EOI) to preempt if the current process has
/// exhausted its time slice.  `time::timer_tick()` already called `scheduler.tick()`
/// via the integration layer, so we only need to check expiry here and context-switch.
/// Must be async-signal-safe: no allocation, no blocking owned locks.
pub fn tick_and_maybe_preempt() {
    let process_manager = super::get_process_manager();
    let expired = {
        // try_lock avoids deadlock if the ISR fires while someone else holds the
        // scheduler lock (rare, but possible on the same CPU before SMP).
        if let Some(sched) = process_manager.scheduler.try_lock() {
            sched.time_slice_expired()
        } else {
            false
        }
    };
    if expired {
        yield_cpu_tail();
    }
}

extern "C" fn yield_cpu_sched_tail() {
    yield_cpu_tail();
}

fn yield_cpu_tail() {
    let process_manager = super::get_process_manager();

    // Schedule the next process
    if let Ok(Some(next_pid)) = process_manager.schedule() {
        let current_pid = process_manager.current_process();

        // Only perform a context switch if we're actually switching to a
        // different process. Switching to ourselves is a no-op.
        if current_pid == next_pid {
            return;
        }

        // Get the current and next process contexts. We clone the PCBs
        // because switch_context doesn't return until the current
        // process is scheduled back in, and we can't hold locks across
        // the switch.
        let current_pcb = process_manager.get_process(current_pid);
        let next_pcb = process_manager.get_process(next_pid);

        if let (Some(current), Some(next)) = (current_pcb, next_pcb) {
            // Look up each process's main thread so the context switch can update
            // TSS.RSP0 to the correct kernel stack.
            let current_thread_stack = current
                .main_thread
                .and_then(|tid| super::thread::get_thread_manager().get_thread(tid))
                .map(|t| t.kernel_stack + t.stack_size as u64)
                .unwrap_or(0);
            let next_thread_stack = next
                .main_thread
                .and_then(|tid| super::thread::get_thread_manager().get_thread(tid))
                .map(|t| t.kernel_stack + t.stack_size as u64)
                .unwrap_or(0);

            // Build ProcessContext from PCB data. FPU and kernel stack state are
            // persisted in the PCB / TCB so they survive across switches.
            let mut current_ctx = super::context::ProcessContext {
                cpu: current.context.clone(),
                fpu: current.fpu.clone(),
                kernel_stack: current_thread_stack,
                user_stack: 0,
                page_table: current.memory.page_directory,
            };

            let next_ctx = super::context::ProcessContext {
                cpu: next.context.clone(),
                fpu: next.fpu.clone(),
                kernel_stack: next_thread_stack,
                user_stack: 0,
                page_table: next.memory.page_directory,
            };

            // Update current process tracking before the switch
            process_manager.set_current_process(next_pid);

            // Perform the context switch. This does not return until the
            // current process is scheduled back in.
            // SAFETY: switch_context saves current registers into
            // current_ctx and loads next_ctx's registers. The page
            // table is switched via CR3 if different.
            unsafe {
                if let Err(e) = super::context::get_context_switcher().switch_context(
                    &mut current_ctx,
                    &next_ctx,
                    next_pid,
                ) {
                    crate::serial_println!("Context switch failed: {}", e);
                }
            }

            // When we return here, we've been scheduled back in.
            // Save the restored CPU and FPU context back to our PCB.
            let _ = process_manager.with_process_mut(current_pid, |pcb| {
                pcb.context = current_ctx.cpu;
                pcb.fpu = current_ctx.fpu;
            });
        } else {
            // Fallback: just update tracking if we can't get PCBs
            process_manager.set_current_process(next_pid);
        }
    }
}

/// Block the current process and yield to scheduler
pub fn block_current_process() -> Result<(), &'static str> {
    let process_manager = super::get_process_manager();
    let current_pid = process_manager.current_process();

    // Block the current process
    process_manager.block_process(current_pid)?;

    // Yield to the next process
    yield_cpu();

    Ok(())
}

/// Wake up a blocked process
pub fn wake_process(pid: Pid) -> Result<(), &'static str> {
    let process_manager = super::get_process_manager();
    process_manager.unblock_process(pid)
}

/// Terminate a process and yield to scheduler
pub fn terminate_process(pid: Pid, exit_status: i32) -> Result<(), &'static str> {
    let process_manager = super::get_process_manager();

    // Terminate the process
    process_manager.terminate_process(pid, exit_status)?;

    // If we terminated the current process, yield to scheduler
    if pid == process_manager.current_process() {
        yield_cpu();
    }

    Ok(())
}

/// Set process priority
pub fn set_process_priority(pid: Pid, priority: Priority) -> Result<(), &'static str> {
    let process_manager = super::get_process_manager();
    let mut scheduler = process_manager.scheduler.lock();

    // Update priority in scheduler
    scheduler.update_process_priority(pid, priority)
}

/// Get process priority
pub fn get_process_priority(pid: Pid) -> Option<Priority> {
    let process_manager = super::get_process_manager();
    if let Some(process) = process_manager.get_process(pid) {
        Some(process.priority)
    } else {
        None
    }
}

/// Get current process from scheduler perspective
pub fn get_current_process() -> Pid {
    let process_manager = super::get_process_manager();
    process_manager.current_process()
}

/// Update scheduling algorithm
pub fn set_scheduling_algorithm(algorithm: SchedulingAlgorithm) {
    let process_manager = super::get_process_manager();
    let mut scheduler = process_manager.scheduler.lock();
    scheduler.set_algorithm(algorithm);
}

/// Get ready queue length for load balancing
pub fn get_ready_queue_length() -> usize {
    let process_manager = super::get_process_manager();
    let scheduler = process_manager.scheduler.lock();
    scheduler.ready_queue_length()
}
