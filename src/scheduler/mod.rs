//! Scheduler subsystem facade for RustOS
//!
//! All process state and scheduling decisions are maintained by the canonical
//! process manager (`crate::process::ProcessManager`). This module provides a
//! stable scheduler subsystem API that delegates to the process manager, so
//! there is exactly one process table and one scheduler in the kernel.
//!
//! The per-CPU scheduler state in this module is maintained as a live facade
//! over the process manager's scheduler, so callers can inspect CPU-local queue
//! and load state without introducing a second process table.

pub mod cfs;
pub mod completion;
pub mod load_balance;
pub mod rt;
pub mod sched_class;
pub mod wait;

pub use sched_class::SchedPolicy;

use crate::process;
pub use crate::process::{Pid, Priority, ProcessState};
use alloc::{collections::VecDeque, vec, vec::Vec};
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

/// Thread ID type
pub type Tid = u64;

/// CPU ID type
pub type CpuId = u32;

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
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0x200, // Enable interrupts
            cs: 0x08,
            ss: 0x10, // Kernel code/data segments
            ds: 0x10,
            es: 0x10,
            fs: 0x10,
            gs: 0x10,
        }
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
    /// Create a new CPU scheduler state
    pub const fn new(cpu_id: CpuId) -> Self {
        Self {
            cpu_id,
            current_process: None,
            ready_queues: [
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
            ],
            time_slice_remaining: 0,
            total_scheduled: 0,
            utilization: 0,
            idle_time: 0,
        }
    }

    /// Rebuild this CPU's scheduler snapshot from the canonical process table.
    pub fn sync_from_process_manager(&mut self) {
        for queue in self.ready_queues.iter_mut() {
            queue.clear();
        }

        let pm = process::get_process_manager();
        let cpu_mask = if self.cpu_id < u64::BITS {
            1u64 << self.cpu_id
        } else {
            0
        };
        let processes =
            pm.find_processes(|pcb| cpu_mask == 0 || (pcb.sched_info.cpu_affinity & cpu_mask) != 0);

        let current_pid = pm.current_process();
        self.current_process = (current_pid != 0).then_some(current_pid);

        let mut runnable = 0usize;
        let mut total_on_cpu = 0usize;
        for pcb in &processes {
            total_on_cpu += 1;
            match pcb.state {
                ProcessState::Ready => {
                    if let Some(queue) = self.ready_queues.get_mut(pcb.priority as usize) {
                        queue.push_back(pcb.pid);
                        runnable += 1;
                    }
                }
                ProcessState::Running => {
                    runnable += 1;
                }
                _ => {}
            }
        }

        self.utilization = if total_on_cpu == 0 {
            0
        } else {
            ((runnable * 100) / total_on_cpu).min(100) as u8
        };

        if self.current_process.is_none() {
            self.time_slice_remaining = 0;
        }
    }

    /// Get the number of ready processes queued for this CPU.
    pub fn ready_process_count(&self) -> usize {
        self.ready_queues.iter().map(VecDeque::len).sum()
    }

    /// Get the total number of runnable processes tracked by this CPU facade.
    pub fn process_count(&self) -> usize {
        self.ready_process_count() + usize::from(self.current_process.is_some())
    }
}

/// Global scheduler state — delegates to the canonical process manager.
pub struct GlobalScheduler {
    initialized: AtomicBool,
}

impl GlobalScheduler {
    /// Create a new global scheduler
    pub const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
        }
    }

    /// Initialize the scheduler subsystem.
    /// Ensures the process manager is ready and creates the init process (PID 1).
    pub fn init(&self) -> Result<(), &'static str> {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        process::init()?;
        let pm = process::get_process_manager();
        pm.create_process("init", None, Priority::High)?;
        sync_current_cpu_scheduler(get_current_cpu_id());
        Ok(())
    }

    /// Create a new process via the canonical process manager.
    pub fn create_process(
        &self,
        parent_pid: Option<Pid>,
        priority: Priority,
        name: &str,
    ) -> Result<Pid, &'static str> {
        let result = process::get_process_manager().create_process(name, parent_pid, priority);
        if result.is_ok() {
            sync_current_cpu_scheduler(get_current_cpu_id());
        }
        result
    }

    /// Terminate a process via the canonical process manager.
    pub fn terminate_process(&self, pid: Pid) -> Result<(), &'static str> {
        let result = process::get_process_manager().terminate_process(pid, 0);
        if result.is_ok() {
            sync_current_cpu_scheduler(get_current_cpu_id());
        }
        result
    }

    /// Block a process via the canonical process manager.
    pub fn block_process(&self, pid: Pid) -> Result<(), &'static str> {
        let result = process::get_process_manager().block_process(pid);
        if result.is_ok() {
            sync_current_cpu_scheduler(get_current_cpu_id());
        }
        result
    }

    /// Unblock a process via the canonical process manager.
    pub fn unblock_process(&self, pid: Pid) -> Result<(), &'static str> {
        let result = process::get_process_manager().unblock_process(pid);
        if result.is_ok() {
            sync_current_cpu_scheduler(get_current_cpu_id());
        }
        result
    }

    /// Schedule the next process on the given CPU.
    pub fn schedule(&self, cpu_id: CpuId) -> Option<Pid> {
        let pm = process::get_process_manager();
        let next_pid = pm.schedule().ok().flatten();

        with_cpu_scheduler(cpu_id, |cpu_scheduler| {
            cpu_scheduler.sync_from_process_manager();
            cpu_scheduler.current_process = next_pid;
            if let Some(pid) = next_pid {
                cpu_scheduler.total_scheduled = cpu_scheduler.total_scheduled.saturating_add(1);
                cpu_scheduler.time_slice_remaining = pm
                    .get_process(pid)
                    .map(|pcb| pcb.priority.time_slice_ms() * 1_000)
                    .unwrap_or(0);
            } else {
                cpu_scheduler.time_slice_remaining = 0;
            }
        });

        next_pid
    }

    /// Handle timer tick for scheduling accounting.
    pub fn timer_tick(&self, cpu_id: CpuId, elapsed_us: u64) {
        process::get_process_manager().tick_cpu_time(elapsed_us);
        with_cpu_scheduler(cpu_id, |cpu_scheduler| {
            cpu_scheduler.sync_from_process_manager();
            if cpu_scheduler.current_process.is_some() {
                cpu_scheduler.time_slice_remaining = cpu_scheduler
                    .time_slice_remaining
                    .saturating_sub(elapsed_us);
            } else {
                cpu_scheduler.idle_time = cpu_scheduler.idle_time.saturating_add(elapsed_us);
            }
        });
    }

    /// Get scheduler statistics from the canonical process manager.
    pub fn get_stats(&self) -> SchedulerStats {
        let pm = process::get_process_manager();
        let processes = pm.find_processes(|_| true);

        let mut stats_by_state = [0usize; 6];
        let mut stats_by_priority = [0usize; 5];

        for pcb in &processes {
            let state_idx = match pcb.state {
                ProcessState::Ready => 0,
                ProcessState::Running => 1,
                ProcessState::Blocked => 2,
                ProcessState::Sleeping => 3,
                ProcessState::Zombie => 4,
                ProcessState::Terminated => 4,
                ProcessState::Dead => 4,
            };
            stats_by_state[state_idx] += 1;
            stats_by_priority[pcb.priority as usize] += 1;
        }

        SchedulerStats {
            total_processes: processes.len(),
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
            cpu_utilizations: Vec::new(),
            uptime_seconds: get_system_time() / 1_000_000,
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

static GLOBAL_SCHEDULER: GlobalScheduler = GlobalScheduler::new();
static LOAD_BALANCING_ENABLED: AtomicBool = AtomicBool::new(false);

lazy_static! {
    static ref CURRENT_CPU_SCHEDULER: Mutex<CpuScheduler> = Mutex::new(CpuScheduler::new(0));
}

fn with_cpu_scheduler<R>(cpu_id: CpuId, f: impl FnOnce(&mut CpuScheduler) -> R) -> R {
    let mut scheduler = CURRENT_CPU_SCHEDULER.lock();
    if scheduler.cpu_id != cpu_id {
        *scheduler = CpuScheduler::new(cpu_id);
    }
    f(&mut scheduler)
}

fn sync_current_cpu_scheduler(cpu_id: CpuId) {
    with_cpu_scheduler(cpu_id, |scheduler| scheduler.sync_from_process_manager());
}

/// Initialize the scheduler subsystem
pub fn init() -> Result<(), &'static str> {
    GLOBAL_SCHEDULER.init()
}

/// Create a new process
pub fn create_process(
    parent_pid: Option<Pid>,
    priority: Priority,
    name: &str,
) -> Result<Pid, &'static str> {
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
    process::scheduler::set_process_priority(pid, new_priority)
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
    process::get_process_manager().set_cpu_affinity(pid, cpu_mask)
}

/// Get CPU affinity for a process
pub fn get_process_affinity(pid: Pid) -> Option<u64> {
    process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.sched_info.cpu_affinity)
}

/// Force a process to migrate to a specific CPU by constraining its affinity.
pub fn migrate_process_to_cpu(pid: Pid, target_cpu: CpuId) -> Result<(), &'static str> {
    if target_cpu >= u64::BITS {
        return Err("target CPU is outside the affinity mask range");
    }

    set_process_affinity(pid, 1u64 << target_cpu)?;
    sync_current_cpu_scheduler(get_current_cpu_id());
    Ok(())
}

/// Get current CPU load information
pub fn get_cpu_loads() -> Vec<(CpuId, usize, u8)> {
    let cpu_id = get_current_cpu_id();
    let (ready, utilization) = with_cpu_scheduler(cpu_id, |scheduler| {
        scheduler.sync_from_process_manager();
        (scheduler.ready_process_count(), scheduler.utilization)
    });
    vec![(cpu_id, ready, utilization)]
}

/// Enable or disable load balancing policy state.
pub fn set_load_balancing(enabled: bool) {
    LOAD_BALANCING_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Yield CPU time to allow other processes to run
pub fn yield_cpu() {
    process::scheduler::yield_cpu();
}

/// Get a reference to the current CPU's scheduler (kept for API compatibility)
pub fn get_scheduler() -> Option<&'static Mutex<CpuScheduler>> {
    Some(&CURRENT_CPU_SCHEDULER)
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
