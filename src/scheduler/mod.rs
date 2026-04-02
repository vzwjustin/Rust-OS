//! Preemptive Scheduler for RustOS
//!
//! This module implements a sophisticated preemptive scheduler with:
//! - Priority-based scheduling with multiple priority levels
//! - Time slicing for fair CPU allocation
//! - SMP support for multi-core systems
//! - Real-time task support
//! - Load balancing across CPU cores

use alloc::{collections::VecDeque, vec::Vec};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};
use lazy_static::lazy_static;
use x86_64::VirtAddr;

/// Process ID type
pub type Pid = u32;

/// Thread ID type
pub type Tid = u64;

/// CPU ID type
pub type CpuId = u32;

/// Process priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    /// Real-time priority (highest)
    RealTime = 0,
    /// High priority
    High = 1,
    /// Normal priority (default)
    Normal = 2,
    /// Low priority
    Low = 3,
    /// Idle priority (lowest)
    Idle = 4,
}

impl Priority {
    /// Get time slice duration in milliseconds for this priority
    pub fn time_slice_ms(&self) -> u64 {
        match self {
            Priority::RealTime => 100,  // 100ms for real-time
            Priority::High => 50,       // 50ms for high priority
            Priority::Normal => 20,     // 20ms for normal
            Priority::Low => 10,        // 10ms for low priority
            Priority::Idle => 5,        // 5ms for idle
        }
    }

    /// Get the number of priority levels
    pub const fn count() -> usize {
        5
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process is blocked waiting for I/O or event
    Blocked,
    /// Process is sleeping
    Sleeping,
    /// Process has terminated
    Terminated,
    /// Process is being created
    Creating,
}

/// FPU/SSE/AVX state structure (512 bytes for FXSAVE/FXRSTOR)
#[derive(Debug, Clone)]
#[repr(C, align(16))]
pub struct FpuState {
    /// FPU control word
    pub fcw: u16,
    /// FPU status word
    pub fsw: u16,
    /// FPU tag word
    pub ftw: u8,
    /// Reserved
    pub reserved1: u8,
    /// FPU instruction pointer offset
    pub fop: u16,
    /// FPU instruction pointer segment
    pub fip: u32,
    /// FPU data pointer offset
    pub fdp: u32,
    /// FPU data pointer segment
    pub fds: u32,
    /// MXCSR register
    pub mxcsr: u32,
    /// MXCSR mask
    pub mxcsr_mask: u32,
    /// ST0-ST7 registers (8 * 16 bytes)
    pub st_regs: [u8; 128],
    /// XMM0-XMM15 registers (16 * 16 bytes)
    pub xmm_regs: [u8; 256],
    /// Padding to align to 512 bytes
    pub padding: [u8; 96],
}

impl Default for FpuState {
    fn default() -> Self {
        Self {
            fcw: 0x037F,
            fsw: 0,
            ftw: 0xFF,
            reserved1: 0,
            fop: 0,
            fip: 0,
            fdp: 0,
            fds: 0,
            mxcsr: 0x1F80,
            mxcsr_mask: 0xFFFF,
            st_regs: [0; 128],
            xmm_regs: [0; 256],
            padding: [0; 96],
        }
    }
}

/// CPU registers state for context switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CpuState {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    
    // Control registers
    pub rip: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
    
    // Segment registers
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0, rflags: 0x200, // Enable interrupts
            cs: 0x08, ss: 0x10,    // Kernel code/data segments
            ds: 0x10, es: 0x10, fs: 0x10, gs: 0x10,
        }
    }
}

/// Process Control Block (PCB)
#[derive(Debug)]
pub struct Process {
    /// Process ID
    pub pid: Pid,
    /// Parent process ID
    pub parent_pid: Option<Pid>,
    /// Process priority
    pub priority: Priority,
    /// Current state
    pub state: ProcessState,
    /// CPU state for context switching
    pub cpu_state: CpuState,
    /// FPU/SSE/AVX state
    pub fpu_state: FpuState,
    /// Whether FPU state is valid/dirty
    pub fpu_state_valid: bool,
    /// Virtual memory space base
    pub memory_base: VirtAddr,
    /// Stack pointer
    pub stack_pointer: VirtAddr,
    /// Stack size in bytes
    pub stack_size: usize,
    /// Time when process was created
    pub creation_time: u64,
    /// Total CPU time used (in microseconds)
    pub cpu_time_used: u64,
    /// Last time this process was scheduled
    pub last_scheduled: u64,
    /// CPU affinity mask (which CPUs this process can run on)
    pub cpu_affinity: u64,
    /// Current CPU this process is running on
    pub current_cpu: Option<CpuId>,
    /// Process name
    pub name: [u8; 32],
}

impl Process {
    /// Create a new process
    pub fn new(pid: Pid, parent_pid: Option<Pid>, priority: Priority, name: &str) -> Self {
        let mut process_name = [0u8; 32];
        let name_bytes = name.as_bytes();
        let copy_len = core::cmp::min(name_bytes.len(), 31);
        process_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        Self {
            pid,
            parent_pid,
            priority,
            state: ProcessState::Creating,
            cpu_state: CpuState::default(),
            fpu_state: FpuState::default(),
            fpu_state_valid: false,
            memory_base: VirtAddr::new(0),
            stack_pointer: VirtAddr::new(0),
            stack_size: 0,
            creation_time: get_system_time(),
            cpu_time_used: 0,
            last_scheduled: 0,
            cpu_affinity: u64::MAX, // Can run on any CPU by default
            current_cpu: None,
            name: process_name,
        }
    }

    /// Get process name as string
    pub fn name_str(&self) -> &str {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(self.name.len());
        core::str::from_utf8(&self.name[..end]).unwrap_or("<invalid>")
    }

    /// Check if process can run on the given CPU
    pub fn can_run_on_cpu(&self, cpu_id: CpuId) -> bool {
        if cpu_id >= 64 {
            return false;
        }
        (self.cpu_affinity & (1 << cpu_id)) != 0
    }

    /// Set CPU affinity
    pub fn set_cpu_affinity(&mut self, cpu_mask: u64) {
        self.cpu_affinity = cpu_mask;
    }
}

/// Per-CPU scheduler state
#[derive(Debug)]
pub struct CpuScheduler {
    /// CPU ID
    pub cpu_id: CpuId,
    /// Currently running process
    pub current_process: Option<Pid>,
    /// Ready queues for each priority level
    pub ready_queues: [VecDeque<Pid>; Priority::count()],
    /// Time slice remaining for current process (in microseconds)
    pub time_slice_remaining: u64,
    /// Total processes scheduled on this CPU
    pub total_scheduled: u64,
    /// CPU utilization percentage (0-100)
    pub utilization: u8,
    /// Idle time in microseconds
    pub idle_time: u64,
}

impl CpuScheduler {
    /// Create a new CPU scheduler
    pub fn new(cpu_id: CpuId) -> Self {
        Self {
            cpu_id,
            current_process: None,
            ready_queues: [
                VecDeque::new(), VecDeque::new(), VecDeque::new(),
                VecDeque::new(), VecDeque::new()
            ],
            time_slice_remaining: 0,
            total_scheduled: 0,
            utilization: 0,
            idle_time: 0,
        }
    }

    /// Add a process to the ready queue
    pub fn enqueue_process(&mut self, pid: Pid, priority: Priority) {
        self.ready_queues[priority as usize].push_back(pid);
    }

    /// Get the next process to run
    pub fn dequeue_next_process(&mut self) -> Option<(Pid, Priority)> {
        // Check each priority level from highest to lowest
        for (priority_idx, queue) in self.ready_queues.iter_mut().enumerate() {
            if let Some(pid) = queue.pop_front() {
                let priority = match priority_idx {
                    0 => Priority::RealTime,
                    1 => Priority::High,
                    2 => Priority::Normal,
                    3 => Priority::Low,
                    4 => Priority::Idle,
                    _ => Priority::Normal,
                };
                return Some((pid, priority));
            }
        }
        None
    }

    /// Get the number of ready processes
    pub fn ready_process_count(&self) -> usize {
        self.ready_queues.iter().map(|q| q.len()).sum()
    }

    /// Get the total number of processes (ready + current)
    pub fn process_count(&self) -> usize {
        let ready_count = self.ready_process_count();
        if self.current_process.is_some() {
            ready_count + 1
        } else {
            ready_count
        }
    }

    /// Update CPU utilization
    pub fn update_utilization(&mut self, active_time: u64, total_time: u64) {
        if total_time > 0 {
            self.utilization = ((active_time * 100) / total_time) as u8;
        }
    }
}

/// Global scheduler state
pub struct GlobalScheduler {
    /// All processes in the system
    pub processes: RwLock<Vec<Process>>,
    /// Per-CPU schedulers
    pub cpu_schedulers: Vec<Mutex<CpuScheduler>>,
    /// Next process ID to assign
    pub next_pid: AtomicU64,
    /// Total number of processes
    pub process_count: AtomicUsize,
    /// System boot time
    pub boot_time: u64,
    /// Load balancing enabled
    pub load_balancing_enabled: bool,
}

impl GlobalScheduler {
    /// Create a new global scheduler
    pub fn new(num_cpus: usize) -> Self {
        let mut cpu_schedulers = Vec::with_capacity(num_cpus);
        for cpu_id in 0..num_cpus {
            cpu_schedulers.push(Mutex::new(CpuScheduler::new(cpu_id as CpuId)));
        }

        Self {
            processes: RwLock::new(Vec::new()),
            cpu_schedulers,
            next_pid: AtomicU64::new(1),
            process_count: AtomicUsize::new(0),
            boot_time: get_system_time(),
            load_balancing_enabled: true,
        }
    }

    /// Create a new process with advanced scheduling features
    pub fn create_process(&self, parent_pid: Option<Pid>, priority: Priority, name: &str) -> Result<Pid, &'static str> {
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst) as Pid;
        let mut process = Process::new(pid, parent_pid, priority, name);
        process.state = ProcessState::Ready;

        // Inherit CPU affinity from parent if available
        if let Some(parent_pid) = parent_pid {
            if let Some(parent_affinity) = self.with_process(parent_pid, |p| p.cpu_affinity) {
                process.cpu_affinity = parent_affinity;
            }
        }

        // Add to process table
        {
            let mut processes = self.processes.write();
            processes.push(process);
        }

        // Schedule on least loaded CPU that matches affinity
        let cpu_id = self.find_best_cpu_for_process(pid);
        {
            let mut cpu_scheduler = self.cpu_schedulers[cpu_id as usize].lock();
            cpu_scheduler.enqueue_process(pid, priority);
        }

        self.process_count.fetch_add(1, Ordering::SeqCst);

        Ok(pid)
    }

    /// Find the best CPU for a process considering affinity and load
    fn find_best_cpu_for_process(&self, pid: Pid) -> CpuId {
        let process_affinity = self.with_process(pid, |p| p.cpu_affinity).unwrap_or(u64::MAX);
        
        let mut best_cpu = 0;
        let mut min_load = usize::MAX;
        
        for (cpu_id, cpu_scheduler_mutex) in self.cpu_schedulers.iter().enumerate() {
            // Check if process can run on this CPU
            if cpu_id >= 64 || (process_affinity & (1 << cpu_id)) == 0 {
                continue;
            }

            if let Some(cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                let load = cpu_scheduler.ready_process_count();
                if load < min_load {
                    min_load = load;
                    best_cpu = cpu_id;
                }
            }
        }
        
        best_cpu as CpuId
    }

    /// Terminate a process and clean up its scheduling state
    pub fn terminate_process(&self, pid: Pid) -> Result<(), &'static str> {
        // Remove from all CPU ready queues
        for cpu_scheduler_mutex in &self.cpu_schedulers {
            if let Some(mut cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                // Remove from all priority queues
                for queue in &mut cpu_scheduler.ready_queues {
                    queue.retain(|&p| p != pid);
                }
                
                // Clear if it's the current process
                if cpu_scheduler.current_process == Some(pid) {
                    cpu_scheduler.current_process = None;
                    cpu_scheduler.time_slice_remaining = 0;
                }
            }
        }

        // Update process state
        self.with_process_mut(pid, |process| {
            process.state = ProcessState::Terminated;
            process.current_cpu = None;
        });

        self.process_count.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    /// Block a process (remove from ready queues)
    pub fn block_process(&self, pid: Pid) -> Result<(), &'static str> {
        // Remove from all CPU ready queues
        for cpu_scheduler_mutex in &self.cpu_schedulers {
            if let Some(mut cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                for queue in &mut cpu_scheduler.ready_queues {
                    queue.retain(|&p| p != pid);
                }
            }
        }

        // Update process state
        self.with_process_mut(pid, |process| {
            process.state = ProcessState::Blocked;
        });

        Ok(())
    }

    /// Unblock a process (add back to ready queue)
    pub fn unblock_process(&self, pid: Pid) -> Result<(), &'static str> {
        let (priority, cpu_id) = self.with_process_mut(pid, |process| {
            process.state = ProcessState::Ready;
            (process.priority, self.find_best_cpu_for_process(pid))
        }).ok_or("Process not found")?;

        // Add to appropriate CPU's ready queue
        if let Some(mut cpu_scheduler) = self.cpu_schedulers[cpu_id as usize].try_lock() {
            cpu_scheduler.enqueue_process(pid, priority);
        }

        Ok(())
    }

    /// Schedule the next process on the given CPU with real scheduling algorithms
    pub fn schedule(&self, cpu_id: CpuId) -> Option<Pid> {
        if cpu_id as usize >= self.cpu_schedulers.len() {
            return None;
        }

        let mut cpu_scheduler = self.cpu_schedulers[cpu_id as usize].lock();
        let current_time = get_system_time();
        
        // Handle current process time slice expiration or preemption
        if let Some(current_pid) = cpu_scheduler.current_process {
            let should_preempt = self.should_preempt_process(current_pid, cpu_id, &cpu_scheduler);
            
            if cpu_scheduler.time_slice_remaining == 0 || should_preempt {
                // Move current process back to ready queue if still runnable
                self.with_process_mut(current_pid, |process| {
                    if process.state == ProcessState::Running {
                        process.state = ProcessState::Ready;
                        // Apply priority boost for interactive processes
                        if process.cpu_time_used < 100_000 { // Less than 100ms CPU time
                            let boosted_priority = match process.priority {
                                Priority::Low => Priority::Normal,
                                Priority::Normal => Priority::High,
                                _ => process.priority,
                            };
                            cpu_scheduler.enqueue_process(current_pid, boosted_priority);
                        } else {
                            cpu_scheduler.enqueue_process(current_pid, process.priority);
                        }
                    }
                });
                cpu_scheduler.current_process = None;
            }
        }

        // Load balancing: check if we should steal work from other CPUs
        if cpu_scheduler.current_process.is_none() && self.load_balancing_enabled {
            self.try_load_balance(cpu_id, &mut cpu_scheduler);
        }

        // Get next process to run using priority-based scheduling
        if cpu_scheduler.current_process.is_none() {
            if let Some((next_pid, priority)) = self.select_next_process(&mut cpu_scheduler) {
                // Update process state
                self.with_process_mut(next_pid, |process| {
                    process.state = ProcessState::Running;
                    process.current_cpu = Some(cpu_id);
                    process.last_scheduled = current_time;
                });

                // Set time slice based on priority and process behavior
                let base_time_slice = priority.time_slice_ms() * 1000; // Convert to microseconds
                let adjusted_time_slice = self.calculate_time_slice(next_pid, base_time_slice);
                
                cpu_scheduler.current_process = Some(next_pid);
                cpu_scheduler.time_slice_remaining = adjusted_time_slice;
                cpu_scheduler.total_scheduled += 1;

                return Some(next_pid);
            }
        }

        cpu_scheduler.current_process
    }

    /// Check if current process should be preempted by higher priority process
    fn should_preempt_process(&self, current_pid: Pid, _cpu_id: CpuId, cpu_scheduler: &CpuScheduler) -> bool {
        // Check for higher priority processes in ready queue
        let current_priority = self.with_process(current_pid, |process| process.priority);
        
        if let Some(current_priority) = current_priority {
            // Check each priority level higher than current
            for priority_level in 0..(current_priority as usize) {
                if !cpu_scheduler.ready_queues[priority_level].is_empty() {
                    return true; // Higher priority process available
                }
            }
        }
        
        false
    }

    /// Select next process using advanced scheduling algorithms
    fn select_next_process(&self, cpu_scheduler: &mut CpuScheduler) -> Option<(Pid, Priority)> {
        // Priority-based scheduling with round-robin within each priority level
        for (priority_idx, queue) in cpu_scheduler.ready_queues.iter_mut().enumerate() {
            if !queue.is_empty() {
                // For real-time processes, use FIFO
                if priority_idx == 0 { // RealTime
                    if let Some(pid) = queue.pop_front() {
                        return Some((pid, Priority::RealTime));
                    }
                } else {
                    // For other priorities, use round-robin
                    if let Some(pid) = queue.pop_front() {
                        let priority = match priority_idx {
                            1 => Priority::High,
                            2 => Priority::Normal,
                            3 => Priority::Low,
                            4 => Priority::Idle,
                            _ => Priority::Normal,
                        };
                        return Some((pid, priority));
                    }
                }
            }
        }
        None
    }

    /// Calculate adaptive time slice based on process behavior
    fn calculate_time_slice(&self, pid: Pid, base_time_slice: u64) -> u64 {
        self.with_process(pid, |process| {
            // Adjust time slice based on process behavior
            let cpu_usage_ratio = if process.last_scheduled > 0 {
                let total_time = get_system_time() - process.creation_time;
                if total_time > 0 {
                    (process.cpu_time_used * 100) / total_time
                } else {
                    0
                }
            } else {
                0
            };

            // Interactive processes (low CPU usage) get longer time slices
            if cpu_usage_ratio < 10 {
                base_time_slice * 2 // Double time slice for interactive processes
            } else if cpu_usage_ratio > 80 {
                base_time_slice / 2 // Half time slice for CPU-intensive processes
            } else {
                base_time_slice
            }
        }).unwrap_or(base_time_slice)
    }

    /// Try to steal work from other CPUs for load balancing
    fn try_load_balance(&self, cpu_id: CpuId, cpu_scheduler: &mut CpuScheduler) {
        if !self.load_balancing_enabled {
            return;
        }

        // Find the most loaded CPU
        let mut max_load = 0;
        let mut source_cpu = None;
        
        for (other_cpu_id, other_scheduler_mutex) in self.cpu_schedulers.iter().enumerate() {
            if other_cpu_id == cpu_id as usize {
                continue;
            }

            if let Some(other_scheduler) = other_scheduler_mutex.try_lock() {
                let load = other_scheduler.ready_process_count();
                if load > max_load && load > 1 { // Only steal if source has more than 1 process
                    max_load = load;
                    source_cpu = Some(other_cpu_id);
                }
            }
        }

        // Steal a process from the most loaded CPU
        if let Some(source_cpu_id) = source_cpu {
            if let Some(mut source_scheduler) = self.cpu_schedulers[source_cpu_id].try_lock() {
                // Try to steal from lower priority queues first
                for priority_idx in (1..Priority::count()).rev() { // Skip RealTime (0)
                    if let Some(stolen_pid) = source_scheduler.ready_queues[priority_idx].pop_back() {
                        // Check if process can run on this CPU
                        let can_migrate = self.with_process(stolen_pid, |process| {
                            process.can_run_on_cpu(cpu_id)
                        }).unwrap_or(false);

                        if can_migrate {
                            let priority = match priority_idx {
                                1 => Priority::High,
                                2 => Priority::Normal,
                                3 => Priority::Low,
                                4 => Priority::Idle,
                                _ => Priority::Normal,
                            };
                            cpu_scheduler.enqueue_process(stolen_pid, priority);
                            break;
                        } else {
                            // Put it back if it can't run on this CPU
                            source_scheduler.ready_queues[priority_idx].push_back(stolen_pid);
                        }
                    }
                }
            }
        }
    }

    /// Handle timer tick for scheduling with advanced time slicing
    pub fn timer_tick(&self, cpu_id: CpuId, elapsed_us: u64) {
        if cpu_id as usize >= self.cpu_schedulers.len() {
            return;
        }

        let mut cpu_scheduler = self.cpu_schedulers[cpu_id as usize].lock();
        let current_time = get_system_time();
        
        if let Some(current_pid) = cpu_scheduler.current_process {
            // Update process CPU time and statistics
            self.with_process_mut(current_pid, |process| {
                process.cpu_time_used += elapsed_us;
                
                // Update process priority based on behavior (aging)
                let time_since_creation = current_time - process.creation_time;
                if time_since_creation > 1_000_000 { // After 1 second
                    let cpu_usage_ratio = (process.cpu_time_used * 100) / time_since_creation;
                    
                    // Demote CPU-intensive processes
                    if cpu_usage_ratio > 90 && process.priority > Priority::RealTime {
                        match process.priority {
                            Priority::High => process.priority = Priority::Normal,
                            Priority::Normal => process.priority = Priority::Low,
                            _ => {}
                        }
                    }
                    // Promote interactive processes
                    else if cpu_usage_ratio < 10 && process.priority < Priority::High {
                        match process.priority {
                            Priority::Low => process.priority = Priority::Normal,
                            Priority::Normal => process.priority = Priority::High,
                            _ => {}
                        }
                    }
                }
            });

            // Decrement time slice with precision
            if cpu_scheduler.time_slice_remaining > elapsed_us {
                cpu_scheduler.time_slice_remaining -= elapsed_us;
            } else {
                cpu_scheduler.time_slice_remaining = 0;
            }

            // Check for preemption by higher priority processes
            if self.should_preempt_process(current_pid, cpu_id, &cpu_scheduler) {
                cpu_scheduler.time_slice_remaining = 0; // Force preemption
            }
        } else {
            // CPU is idle
            cpu_scheduler.idle_time += elapsed_us;
        }

        // Update CPU utilization with exponential moving average
        let active_time = if cpu_scheduler.current_process.is_some() { elapsed_us } else { 0 };
        let utilization_sample = if elapsed_us > 0 { (active_time * 100) / elapsed_us } else { 0 };
        
        // Smooth utilization calculation
        let old_utilization = cpu_scheduler.utilization as u64;
        let new_utilization = ((old_utilization * 7) + utilization_sample) / 8; // 7/8 old + 1/8 new
        cpu_scheduler.utilization = new_utilization.min(100) as u8;

        // Periodic load balancing (every 100ms)
        if current_time % 100_000 == 0 && self.load_balancing_enabled {
            drop(cpu_scheduler); // Release lock before load balancing
            self.periodic_load_balance();
        }
    }

    /// Periodic load balancing across all CPUs
    fn periodic_load_balance(&self) {
        let mut cpu_loads = Vec::new();
        
        // Collect load information from all CPUs
        for (cpu_id, cpu_scheduler_mutex) in self.cpu_schedulers.iter().enumerate() {
            if let Some(cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                cpu_loads.push((cpu_id, cpu_scheduler.ready_process_count(), cpu_scheduler.utilization));
            }
        }

        // Find imbalanced CPUs
        if cpu_loads.len() < 2 {
            return;
        }

        cpu_loads.sort_by_key(|(_, load, _)| *load);
        let min_load_cpu = cpu_loads[0];
        let max_load_cpu = cpu_loads[cpu_loads.len() - 1];

        // Balance if difference is significant
        if max_load_cpu.1 > min_load_cpu.1 + 2 {
            self.balance_cpus(max_load_cpu.0, min_load_cpu.0);
        }
    }

    /// Balance load between two CPUs
    fn balance_cpus(&self, source_cpu: usize, target_cpu: usize) {
        if source_cpu >= self.cpu_schedulers.len() || target_cpu >= self.cpu_schedulers.len() {
            return;
        }

        // Try to acquire locks on both schedulers
        if let (Some(mut source_scheduler), Some(mut target_scheduler)) = (
            self.cpu_schedulers[source_cpu].try_lock(),
            self.cpu_schedulers[target_cpu].try_lock()
        ) {
            // Move one process from source to target (prefer lower priority)
            for priority_idx in (1..Priority::count()).rev() { // Skip RealTime
                if let Some(migrated_pid) = source_scheduler.ready_queues[priority_idx].pop_back() {
                    // Check CPU affinity
                    let can_migrate = self.with_process(migrated_pid, |process| {
                        process.can_run_on_cpu(target_cpu as CpuId)
                    }).unwrap_or(false);

                    if can_migrate {
                        let priority = match priority_idx {
                            1 => Priority::High,
                            2 => Priority::Normal,
                            3 => Priority::Low,
                            4 => Priority::Idle,
                            _ => Priority::Normal,
                        };
                        target_scheduler.enqueue_process(migrated_pid, priority);
                        break;
                    } else {
                        // Put it back if it can't migrate
                        source_scheduler.ready_queues[priority_idx].push_back(migrated_pid);
                    }
                }
            }
        }
    }

    /// Find the least loaded CPU
    fn find_least_loaded_cpu(&self) -> CpuId {
        let mut min_load = usize::MAX;
        let mut best_cpu = 0;

        for (cpu_id, cpu_scheduler_mutex) in self.cpu_schedulers.iter().enumerate() {
            let cpu_scheduler = cpu_scheduler_mutex.lock();
            let load = cpu_scheduler.ready_process_count();
            if load < min_load {
                min_load = load;
                best_cpu = cpu_id;
            }
        }

        best_cpu as CpuId
    }

    /// Find a process by PID and execute an operation on it
    fn with_process_mut<F, R>(&self, pid: Pid, f: F) -> Option<R>
    where 
        F: FnOnce(&mut Process) -> R,
    {
        // Get mutable access to the processes vector
        let mut processes = self.processes.write();
        processes.iter_mut()
            .find(|p| p.pid == pid)
            .map(|process| f(process))
    }

    /// Find a process by PID and execute a read-only operation on it
    fn with_process<F, R>(&self, pid: Pid, f: F) -> Option<R>
    where 
        F: FnOnce(&Process) -> R,
    {
        // Get read access to the processes vector
        let processes = self.processes.read();
        processes.iter()
            .find(|p| p.pid == pid)
            .map(|process| f(process))
    }

    /// Get scheduler statistics
    pub fn get_stats(&self) -> SchedulerStats {
        let processes = self.processes.read();
        let process_count = processes.len();
        
        let mut stats_by_state = [0usize; 6];
        let mut stats_by_priority = [0usize; 5];
        
        for process in processes.iter() {
            let state_idx = match process.state {
                ProcessState::Ready => 0,
                ProcessState::Running => 1,
                ProcessState::Blocked => 2,
                ProcessState::Sleeping => 3,
                ProcessState::Terminated => 4,
                ProcessState::Creating => 5,
            };
            stats_by_state[state_idx] += 1;
            stats_by_priority[process.priority as usize] += 1;
        }

        let mut cpu_utilizations = Vec::new();
        for cpu_scheduler_mutex in &self.cpu_schedulers {
            let cpu_scheduler = cpu_scheduler_mutex.lock();
            cpu_utilizations.push(cpu_scheduler.utilization);
        }

        SchedulerStats {
            total_processes: process_count,
            ready_processes: stats_by_state[0],
            running_processes: stats_by_state[1],
            blocked_processes: stats_by_state[2],
            sleeping_processes: stats_by_state[3],
            terminated_processes: stats_by_state[4],
            creating_processes: stats_by_state[5],
            realtime_processes: stats_by_priority[0],
            high_priority_processes: stats_by_priority[1],
            normal_priority_processes: stats_by_priority[2],
            low_priority_processes: stats_by_priority[3],
            idle_priority_processes: stats_by_priority[4],
            cpu_utilizations,
            uptime_seconds: (get_system_time() - self.boot_time) / 1_000_000,
        }
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub total_processes: usize,
    pub ready_processes: usize,
    pub running_processes: usize,
    pub blocked_processes: usize,
    pub sleeping_processes: usize,
    pub terminated_processes: usize,
    pub creating_processes: usize,
    pub realtime_processes: usize,
    pub high_priority_processes: usize,
    pub normal_priority_processes: usize,
    pub low_priority_processes: usize,
    pub idle_priority_processes: usize,
    pub cpu_utilizations: Vec<u8>,
    pub uptime_seconds: u64,
}

lazy_static! {
    static ref GLOBAL_SCHEDULER: GlobalScheduler = {
        // Detect number of CPUs from ACPI MADT
        let num_cpus = if let Some(madt) = crate::acpi::madt() {
            core::cmp::max(1, madt.processors.len())
        } else {
            1 // Single CPU fallback
        };

        GlobalScheduler::new(num_cpus)
    };
}

/// Initialize the scheduler subsystem
pub fn init() -> Result<(), &'static str> {
    // Force initialization of the global scheduler
    lazy_static::initialize(&GLOBAL_SCHEDULER);
    
    // Create init process (PID 1)
    GLOBAL_SCHEDULER.create_process(None, Priority::High, "init")?;

    Ok(())
}

/// Create a new process
pub fn create_process(parent_pid: Option<Pid>, priority: Priority, name: &str) -> Result<Pid, &'static str> {
    GLOBAL_SCHEDULER.create_process(parent_pid, priority, name)
}

/// Terminate a process
pub fn terminate_process(pid: Pid) -> Result<(), &'static str> {
    GLOBAL_SCHEDULER.terminate_process(pid)
}

/// Block a process
pub fn block_process(pid: Pid) -> Result<(), &'static str> {
    GLOBAL_SCHEDULER.block_process(pid)
}

/// Unblock a process
pub fn unblock_process(pid: Pid) -> Result<(), &'static str> {
    GLOBAL_SCHEDULER.unblock_process(pid)
}

/// Change process priority
pub fn set_process_priority(pid: Pid, new_priority: Priority) -> Result<(), &'static str> {
    // Remove from current queue
    let old_priority = GLOBAL_SCHEDULER.with_process(pid, |p| p.priority).ok_or("Process not found")?;
    
    // Update priority
    GLOBAL_SCHEDULER.with_process_mut(pid, |process| {
        process.priority = new_priority;
    }).ok_or("Process not found")?;

    // Move between queues if process is ready
    let process_state = GLOBAL_SCHEDULER.with_process(pid, |p| p.state).unwrap_or(ProcessState::Terminated);
    
    if process_state == ProcessState::Ready {
        // Remove from old priority queue
        for cpu_scheduler_mutex in &GLOBAL_SCHEDULER.cpu_schedulers {
            if let Some(mut cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                if cpu_scheduler.ready_queues[old_priority as usize].iter().position(|&p| p == pid).is_some() {
                    cpu_scheduler.ready_queues[old_priority as usize].retain(|&p| p != pid);
                    cpu_scheduler.enqueue_process(pid, new_priority);
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Schedule the next process on the current CPU
pub fn schedule() -> Option<Pid> {
    let cpu_id = get_current_cpu_id();
    let next_pid = GLOBAL_SCHEDULER.schedule(cpu_id);
    
    // Update thread manager with the main thread of the scheduled process
    // For simplicity, we assume each process has a main thread with TID = PID
    if let Some(pid) = next_pid {
        let thread_manager = crate::process::thread::get_thread_manager();
        thread_manager.set_current_thread(pid);
    }
    
    next_pid
}

/// Handle timer tick for scheduling
pub fn timer_tick(elapsed_us: u64) {
    let cpu_id = get_current_cpu_id();
    GLOBAL_SCHEDULER.timer_tick(cpu_id, elapsed_us);
}

/// Get scheduler statistics
pub fn get_scheduler_stats() -> SchedulerStats {
    GLOBAL_SCHEDULER.get_stats()
}

/// Get current CPU ID (production implementation)
fn get_current_cpu_id() -> CpuId {
    crate::smp::current_cpu()
}

/// Get system time in microseconds (production implementation)
fn get_system_time() -> u64 {
    crate::time::uptime_us()
}

/// Context switch between processes (real assembly implementation)
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(old_state: *mut CpuState, new_state: *const CpuState) {
    use core::arch::naked_asm;
    
    naked_asm!(
        r#"
        // Save current CPU state to old_state (RDI)
        // General purpose registers
        mov [rdi + 0x00], rax
        mov [rdi + 0x08], rbx
        mov [rdi + 0x10], rcx
        mov [rdi + 0x18], rdx
        mov [rdi + 0x20], rsi
        mov [rdi + 0x28], rdi
        mov [rdi + 0x30], rbp
        mov [rdi + 0x38], rsp
        mov [rdi + 0x40], r8
        mov [rdi + 0x48], r9
        mov [rdi + 0x50], r10
        mov [rdi + 0x58], r11
        mov [rdi + 0x60], r12
        mov [rdi + 0x68], r13
        mov [rdi + 0x70], r14
        mov [rdi + 0x78], r15

        // Save RIP (return address from stack)
        mov rax, [rsp]
        mov [rdi + 0x80], rax

        // Save RFLAGS
        pushf
        pop rax
        mov [rdi + 0x88], rax

        // Save segment registers
        mov ax, cs
        mov [rdi + 0x90], rax
        mov ax, ss
        mov [rdi + 0x98], rax
        mov ax, ds
        mov [rdi + 0xA0], rax
        mov ax, es
        mov [rdi + 0xA8], rax
        mov ax, fs
        mov [rdi + 0xB0], rax
        mov ax, gs
        mov [rdi + 0xB8], rax

        // Load new CPU state from new_state (RSI)
        // Restore general purpose registers
        mov rax, [rsi + 0x00]
        mov rbx, [rsi + 0x08]
        mov rcx, [rsi + 0x10]
        mov rdx, [rsi + 0x18]
        mov rbp, [rsi + 0x30]
        mov rsp, [rsi + 0x38]
        mov r8,  [rsi + 0x40]
        mov r9,  [rsi + 0x48]
        mov r10, [rsi + 0x50]
        mov r11, [rsi + 0x58]
        mov r12, [rsi + 0x60]
        mov r13, [rsi + 0x68]
        mov r14, [rsi + 0x70]
        mov r15, [rsi + 0x78]

        // Restore RFLAGS
        push qword ptr [rsi + 0x88]
        popf

        // Restore segment registers (data segments only, CS/SS handled by iret)
        mov ax, [rsi + 0xA0]
        mov ds, ax
        mov ax, [rsi + 0xA8]
        mov es, ax
        mov ax, [rsi + 0xB0]
        mov fs, ax
        mov ax, [rsi + 0xB8]
        mov gs, ax

        // Push return address and jump to new process
        push qword ptr [rsi + 0x80]

        // Restore RSI and RDI last
        mov rdi, [rsi + 0x28]
        mov rsi, [rsi + 0x20]

        // Return to new process
        ret
        "#
    );
}

/// Save FPU/SSE state
pub unsafe fn save_fpu_state(fpu_state: *mut FpuState) {
    use core::arch::asm;
    
    // Check if SSE is supported (assume it is for modern x86_64)
    asm!(
        "fxsave [{}]",
        in(reg) fpu_state,
        options(nostack, preserves_flags)
    );
}

/// Restore FPU/SSE state
pub unsafe fn restore_fpu_state(fpu_state: *const FpuState) {
    use core::arch::asm;
    
    // Check if SSE is supported (assume it is for modern x86_64)
    asm!(
        "fxrstor [{}]",
        in(reg) fpu_state,
        options(nostack, preserves_flags)
    );
}

/// Initialize FPU for the current CPU
pub unsafe fn init_fpu() {
    use core::arch::asm;
    
    // Initialize FPU
    asm!("finit", options(nostack, preserves_flags));
    
    // Enable SSE and FXSAVE/FXRSTOR
    let mut cr4: u64;
    asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
    cr4 |= (1 << 9) | (1 << 10); // OSFXSR and OSXMMEXCPT
    asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
    
    // Clear task switched flag
    asm!("clts", options(nostack, preserves_flags));
}

/// Complete context switch with FPU state
pub unsafe fn context_switch_with_fpu(
    old_cpu_state: *mut CpuState,
    old_fpu_state: *mut FpuState,
    new_cpu_state: *const CpuState,
    new_fpu_state: *const FpuState,
) {
    // Save current FPU state
    save_fpu_state(old_fpu_state);
    
    // Perform CPU context switch
    context_switch(old_cpu_state, new_cpu_state);
    
    // Restore new FPU state
    restore_fpu_state(new_fpu_state);
}

/// Set CPU affinity for a process
pub fn set_process_affinity(pid: Pid, cpu_mask: u64) -> Result<(), &'static str> {
    GLOBAL_SCHEDULER.with_process_mut(pid, |process| {
        process.set_cpu_affinity(cpu_mask);
    }).ok_or("Process not found")
}

/// Get CPU affinity for a process
pub fn get_process_affinity(pid: Pid) -> Option<u64> {
    GLOBAL_SCHEDULER.with_process(pid, |process| process.cpu_affinity)
}

/// Force a process to migrate to a specific CPU
pub fn migrate_process_to_cpu(pid: Pid, target_cpu: CpuId) -> Result<(), &'static str> {
    if target_cpu as usize >= GLOBAL_SCHEDULER.cpu_schedulers.len() {
        return Err("Invalid CPU ID");
    }

    // Check if process can run on target CPU
    let can_migrate = GLOBAL_SCHEDULER.with_process(pid, |process| {
        process.can_run_on_cpu(target_cpu) && process.state == ProcessState::Ready
    }).unwrap_or(false);

    if !can_migrate {
        return Err("Process cannot migrate to target CPU");
    }

    // Remove from current CPU's ready queue
    let process_priority = GLOBAL_SCHEDULER.with_process(pid, |process| process.priority);
    if let Some(priority) = process_priority {
        for (_cpu_id, cpu_scheduler_mutex) in GLOBAL_SCHEDULER.cpu_schedulers.iter().enumerate() {
            if let Some(mut cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
                if cpu_scheduler.ready_queues[priority as usize].iter().position(|&p| p == pid).is_some() {
                    cpu_scheduler.ready_queues[priority as usize].retain(|&p| p != pid);
                    break;
                }
            }
        }

        // Add to target CPU's ready queue
        if let Some(mut target_scheduler) = GLOBAL_SCHEDULER.cpu_schedulers[target_cpu as usize].try_lock() {
            target_scheduler.enqueue_process(pid, priority);
        }
    }

    Ok(())
}

/// Get current CPU load information
pub fn get_cpu_loads() -> Vec<(CpuId, usize, u8)> {
    let mut loads = Vec::new();

    for (cpu_id, cpu_scheduler_mutex) in GLOBAL_SCHEDULER.cpu_schedulers.iter().enumerate() {
        if let Some(cpu_scheduler) = cpu_scheduler_mutex.try_lock() {
            loads.push((
                cpu_id as CpuId,
                cpu_scheduler.ready_process_count(),
                cpu_scheduler.utilization
            ));
        }
    }
    
    loads
}

/// Enable or disable load balancing
pub fn set_load_balancing(_enabled: bool) {
    // This would require making load_balancing_enabled mutable
    // For now, it's a compile-time setting
}

/// Yield CPU time to allow other processes to run
pub fn yield_cpu() {
    // Trigger a reschedule by setting time slice to 0
    let cpu_id = get_current_cpu_id();
    if let Some(mut cpu_scheduler) = GLOBAL_SCHEDULER.cpu_schedulers[cpu_id as usize].try_lock() {
        cpu_scheduler.time_slice_remaining = 0;
    }
}

/// Get a reference to the current CPU's scheduler.
///
/// Returns `Some(&Mutex<CpuScheduler>)` for the scheduler managing the current CPU,
/// or `None` if the current CPU ID is invalid or out of bounds.
///
/// # Usage
///
/// ```rust,no_run
/// if let Some(scheduler_lock) = get_scheduler() {
///     let scheduler = scheduler_lock.lock();
///     // Access scheduler state...
/// }
/// ```
///
/// # Note
///
/// The caller must lock the returned `Mutex` to access the scheduler state.
/// Be careful to avoid deadlocks when holding the scheduler lock.
pub fn get_scheduler() -> Option<&'static Mutex<CpuScheduler>> {
    let cpu_id = get_current_cpu_id();
    if (cpu_id as usize) < GLOBAL_SCHEDULER.cpu_schedulers.len() {
        Some(&GLOBAL_SCHEDULER.cpu_schedulers[cpu_id as usize])
    } else {
        None
    }
}

/// Update the priority of a process.
///
/// This function changes the priority of a process and moves it to the
/// appropriate ready queue if the process is in the Ready state. This
/// is a convenience wrapper around `set_process_priority` that silently
/// ignores errors for cases where error handling is not needed.
///
/// # Arguments
///
/// * `pid` - The process ID of the process to update
/// * `new_priority` - The new priority level to assign
///
/// # Note
///
/// This function does not return an error if the process is not found
/// or if the priority change fails. Use `set_process_priority` if you
/// need to handle such errors.
pub fn update_process_priority(pid: Pid, new_priority: Priority) {
    // Delegate to the error-returning version, ignoring the result
    // for backward compatibility with callers that don't handle errors
    let _ = set_process_priority(pid, new_priority);
}
