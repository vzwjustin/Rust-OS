//! Process Management Module
//!
//! This module provides comprehensive process management functionality for RustOS,
//! including process control blocks, scheduling, system calls, and context switching.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};

pub mod context;
pub mod dynamic_linker;
pub mod elf_loader;
pub mod integration;
pub mod ipc;
pub mod scheduler;
pub mod sync;
pub mod syscalls;
pub mod thread;

/// Process ID type
pub type Pid = u32;

/// Maximum number of processes that can exist simultaneously
pub const MAX_PROCESSES: usize = 1024;

/// Process states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process is blocked waiting for I/O or resources
    Blocked,
    /// Process is sleeping
    Sleeping,
    /// Process has terminated
    Terminated,
    /// Process has terminated but PCB still exists (waiting for parent to collect exit status)
    Zombie,
    /// Process has been completely cleaned up
    Dead,
}

/// Process priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// CPU register state for context switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CpuContext {
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

    // Segment registers
    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
    pub ss: u16,
}

impl Default for CpuContext {
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
            rflags: 0x202, // Enable interrupts by default
            cs: 0x08,
            ds: 0x10,
            es: 0x10,
            fs: 0x10,
            gs: 0x10,
            ss: 0x10,
        }
    }
}

/// Memory management information for a process
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Page directory physical address
    pub page_directory: u64,
    /// Virtual memory start address
    pub vm_start: u64,
    /// Virtual memory size
    pub vm_size: u64,
    /// Code segment start address
    pub code_start: u64,
    /// Code segment size
    pub code_size: u64,
    /// Data segment start address
    pub data_start: u64,
    /// Data segment size
    pub data_size: u64,
    /// Heap start address
    pub heap_start: u64,
    /// Heap size
    pub heap_size: u64,
    /// Stack start address
    pub stack_start: u64,
    /// Stack size
    pub stack_size: u64,
}

impl Default for MemoryInfo {
    fn default() -> Self {
        Self {
            page_directory: 0,
            vm_start: 0x400000,       // 4MB
            vm_size: 0x100000,        // 1MB default
            code_start: 0x400000,     // 4MB
            code_size: 0,             // Set during load
            data_start: 0x500000,     // 5MB
            data_size: 0,             // Set during load
            heap_start: 0x600000,     // 6MB
            heap_size: 0x100000,      // 1MB
            stack_start: 0x7FFFFF000, // Near top of user space
            stack_size: 0x2000,       // 8KB default stack
        }
    }
}

/// Process Control Block (PCB)
#[derive(Debug, Clone)]
pub struct ProcessControlBlock {
    /// Process ID
    pub pid: Pid,
    /// Parent process ID
    pub parent_pid: Option<Pid>,
    /// Process state
    pub state: ProcessState,
    /// Process priority
    pub priority: Priority,
    /// CPU context for context switching
    pub context: CpuContext,
    /// Memory management information
    pub memory: MemoryInfo,
    /// Process name
    pub name: [u8; 32],
    /// CPU time used (in ticks)
    pub cpu_time: u64,
    /// Time when process was created
    pub creation_time: u64,
    /// Exit status (valid only when state is Zombie)
    pub exit_status: Option<i32>,
    /// Exit code (alias for exit_status)
    pub exit_code: Option<i32>,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Current working directory
    pub cwd: alloc::string::String,
    /// File descriptor table
    pub fd_table: BTreeMap<u32, FileDescriptor>,
    /// Next file descriptor number
    pub next_fd: u32,
    /// Process scheduling information
    pub sched_info: SchedulingInfo,
    /// Main thread ID for this process
    pub main_thread: Option<thread::Tid>,
    /// File offsets for seek operations
    pub file_offsets: BTreeMap<u32, usize>,
    /// Wake time for sleeping processes
    pub wake_time: Option<u64>,
    /// Signal handlers
    pub signal_handlers: BTreeMap<u32, u64>,
    /// Pending signals
    pub pending_signals: alloc::vec::Vec<u32>,
    /// Program entry point address
    pub entry_point: u64,
    /// File descriptors map (alias for compatibility)
    pub file_descriptors: BTreeMap<u32, FileDescriptor>,
}

/// File descriptor information
#[derive(Debug, Clone)]
pub struct FileDescriptor {
    pub fd_type: FileDescriptorType,
    pub flags: u32,
    pub offset: u64,
}

impl FileDescriptor {
    /// Create a new file descriptor from a VFS Inode
    pub fn from_inode(inode: crate::fs::Inode, flags: u32) -> Self {
        Self {
            fd_type: FileDescriptorType::VfsFile { inode },
            flags,
            offset: 0,
        }
    }

    /// Create a new file descriptor from a raw VFS file descriptor
    pub fn from_vfs_fd(vfs_fd: i32, flags: u32) -> Self {
        Self {
            fd_type: FileDescriptorType::VfsHandle { vfs_fd },
            flags,
            offset: 0,
        }
    }

    /// Create a standard input descriptor
    pub fn stdin() -> Self {
        Self {
            fd_type: FileDescriptorType::StandardInput,
            flags: 0,
            offset: 0,
        }
    }

    /// Create a standard output descriptor
    pub fn stdout() -> Self {
        Self {
            fd_type: FileDescriptorType::StandardOutput,
            flags: 0,
            offset: 0,
        }
    }

    /// Create a standard error descriptor
    pub fn stderr() -> Self {
        Self {
            fd_type: FileDescriptorType::StandardError,
            flags: 0,
            offset: 0,
        }
    }

    /// Read from this file descriptor
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, crate::fs::FsError> {
        match &self.fd_type {
            FileDescriptorType::VfsFile { inode } => {
                let bytes_read = inode.read(self.offset, buffer)?;
                self.offset += bytes_read as u64;
                Ok(bytes_read)
            }
            FileDescriptorType::StandardInput => {
                // For stdin, read from keyboard buffer
                Ok(0)
            }
            _ => Err(crate::fs::FsError::BadFileDescriptor),
        }
    }

    /// Write to this file descriptor
    pub fn write(&mut self, data: &[u8]) -> Result<usize, crate::fs::FsError> {
        match &self.fd_type {
            FileDescriptorType::VfsFile { inode } => {
                let bytes_written = inode.write(self.offset, data)?;
                self.offset += bytes_written as u64;
                Ok(bytes_written)
            }
            FileDescriptorType::StandardOutput | FileDescriptorType::StandardError => {
                // For stdout/stderr, write to serial console
                for &byte in data {
                    crate::serial_print!("{}", byte as char);
                }
                Ok(data.len())
            }
            _ => Err(crate::fs::FsError::BadFileDescriptor),
        }
    }

    /// Get the VFS inode if this is a VFS file
    pub fn inode(&self) -> Option<&crate::fs::Inode> {
        match &self.fd_type {
            FileDescriptorType::VfsFile { inode } => Some(inode),
            _ => None,
        }
    }

    /// Get current file offset
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Set file offset
    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset;
    }
}

#[derive(Debug, Clone)]
pub enum FileDescriptorType {
    StandardInput,
    StandardOutput,
    StandardError,
    VfsFile { inode: crate::fs::Inode },
    VfsHandle { vfs_fd: i32 },
    Socket { socket_id: u32 },
    Pipe { pipe_id: u32 },
}

/// Scheduling-specific information
#[derive(Debug, Clone)]
pub struct SchedulingInfo {
    /// Time slice remaining (for round-robin)
    pub time_slice: u32,
    /// Default time slice for this process
    pub default_time_slice: u32,
    /// Number of times process has been scheduled
    pub schedule_count: u64,
    /// Last time process was scheduled
    pub last_scheduled: u64,
    /// CPU affinity mask
    pub cpu_affinity: u64,
}

impl ProcessControlBlock {
    /// Create a new PCB with the given PID and parent
    pub fn new(pid: Pid, parent_pid: Option<Pid>, name: &str) -> Self {
        let fd_table = BTreeMap::new();
        let mut pcb = Self {
            pid,
            parent_pid,
            state: ProcessState::Ready,
            priority: Priority::default(),
            context: CpuContext::default(),
            memory: MemoryInfo::default(),
            name: [0; 32],
            cpu_time: 0,
            creation_time: get_system_time(),
            exit_status: None,
            exit_code: None,
            uid: 0,
            gid: 0,
            cwd: alloc::string::String::from("/"),
            fd_table: fd_table.clone(),
            next_fd: 3, // 0, 1, 2 reserved for stdin, stdout, stderr
            sched_info: SchedulingInfo {
                time_slice: 10, // 10ms default
                default_time_slice: 10,
                schedule_count: 0,
                last_scheduled: 0,
                cpu_affinity: 0xFFFFFFFFFFFFFFFF, // All CPUs
            },
            main_thread: None,
            file_offsets: BTreeMap::new(),
            wake_time: None,
            signal_handlers: BTreeMap::new(),
            pending_signals: alloc::vec::Vec::new(),
            entry_point: 0,
            file_descriptors: fd_table,
        };

        // Set process name
        let name_bytes = name.as_bytes();
        let copy_len = core::cmp::min(name_bytes.len(), 31);
        pcb.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        // Initialize standard file descriptors
        let stdin_fd = FileDescriptor {
            fd_type: FileDescriptorType::StandardInput,
            flags: 0,
            offset: 0,
        };
        let stdout_fd = FileDescriptor {
            fd_type: FileDescriptorType::StandardOutput,
            flags: 0,
            offset: 0,
        };
        let stderr_fd = FileDescriptor {
            fd_type: FileDescriptorType::StandardError,
            flags: 0,
            offset: 0,
        };

        pcb.fd_table.insert(0, stdin_fd.clone());
        pcb.fd_table.insert(1, stdout_fd.clone());
        pcb.fd_table.insert(2, stderr_fd.clone());

        pcb.file_descriptors.insert(0, stdin_fd);
        pcb.file_descriptors.insert(1, stdout_fd);
        pcb.file_descriptors.insert(2, stderr_fd);

        pcb
    }

    /// Get process name as string
    pub fn name_str(&self) -> &str {
        let name_len = self.name.iter().position(|&x| x == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..name_len]).unwrap_or("invalid")
    }

    /// Set process state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }

    /// Check if process is runnable
    pub fn is_runnable(&self) -> bool {
        matches!(self.state, ProcessState::Ready)
    }

    /// Allocate a new file descriptor
    pub fn allocate_fd(&mut self, fd_type: FileDescriptorType) -> u32 {
        let fd = self.next_fd;
        self.fd_table.insert(
            fd,
            FileDescriptor {
                fd_type,
                flags: 0,
                offset: 0,
            },
        );
        self.next_fd += 1;
        fd
    }

    /// Close a file descriptor
    pub fn close_fd(&mut self, fd: u32) -> Result<(), &'static str> {
        if fd < 3 {
            return Err("Cannot close standard file descriptors");
        }
        self.fd_table.remove(&fd).ok_or("Invalid file descriptor")?;
        Ok(())
    }
}

/// Process Manager - central coordinator for all process operations
pub struct ProcessManager {
    /// All processes in the system
    processes: RwLock<BTreeMap<Pid, ProcessControlBlock>>,
    /// Currently running process ID
    current_process: AtomicU32,
    /// Next PID to allocate
    next_pid: AtomicU32,
    /// Process count
    process_count: AtomicUsize,
    /// Scheduler instance
    scheduler: Mutex<scheduler::Scheduler>,
    /// System call dispatcher
    syscall_dispatcher: Mutex<syscalls::SyscallDispatcher>,
}

impl ProcessManager {
    /// Create a new process manager
    pub const fn new() -> Self {
        Self {
            processes: RwLock::new(BTreeMap::new()),
            current_process: AtomicU32::new(0),
            next_pid: AtomicU32::new(1),
            process_count: AtomicUsize::new(0),
            scheduler: Mutex::new(scheduler::Scheduler::new()),
            syscall_dispatcher: Mutex::new(syscalls::SyscallDispatcher::new()),
        }
    }

    /// Initialize the process manager with kernel process
    pub fn init(&self) -> Result<(), &'static str> {
        // Create kernel process (PID 0)
        let kernel_pcb = ProcessControlBlock::new(0, None, "kernel");

        {
            let mut processes = self.processes.write();
            processes.insert(0, kernel_pcb);
        }

        self.process_count.store(1, Ordering::SeqCst);
        self.current_process.store(0, Ordering::SeqCst);

        // Initialize scheduler
        {
            let mut scheduler = self.scheduler.lock();
            scheduler.init()?;
            scheduler.add_process(0, Priority::RealTime)?;
        }

        Ok(())
    }

    /// Create a new process
    pub fn create_process(
        &self,
        name: &str,
        parent_pid: Option<Pid>,
        priority: Priority,
    ) -> Result<Pid, &'static str> {
        // ponytail: take the processes write lock first so the count limit check
        // and the insert are a single atomic step (no TOCTOU where two callers
        // both pass the check and overshoot MAX_PROCESSES). The PID is only
        // allocated after the limit passes, so a rejected create never burns a PID.
        let pid = {
            let mut processes = self.processes.write();

            if self.process_count.load(Ordering::SeqCst) >= MAX_PROCESSES {
                return Err("Maximum process count exceeded");
            }

            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
            let mut pcb = ProcessControlBlock::new(pid, parent_pid, name);
            pcb.priority = priority;

            processes.insert(pid, pcb);
            self.process_count.fetch_add(1, Ordering::SeqCst);
            pid
        };

        // Add to scheduler
        {
            let mut scheduler = self.scheduler.lock();
            scheduler.add_process(pid, priority)?;
        }

        // Initialize IPC state for new process
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.init_process_signals(pid)?;

        Ok(pid)
    }

    /// Terminate a process
    pub fn terminate_process(&self, pid: Pid, exit_status: i32) -> Result<(), &'static str> {
        {
            let mut processes = self.processes.write();
            if let Some(pcb) = processes.get_mut(&pid) {
                pcb.set_state(ProcessState::Zombie);
                pcb.exit_status = Some(exit_status);
            } else {
                return Err("Process not found");
            }
        }

        // Terminate all threads for this process
        self.terminate_process_threads(pid)?;

        // Cleanup IPC resources
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.cleanup_process_ipc(pid)?;

        // Remove from scheduler
        {
            let mut scheduler = self.scheduler.lock();
            scheduler.remove_process(pid)?;
        }

        Ok(())
    }

    /// Get process information
    pub fn get_process(&self, pid: Pid) -> Option<ProcessControlBlock> {
        let processes = self.processes.read();
        processes.get(&pid).cloned()
    }

    /// Mutate a process in place under the processes lock.
    ///
    /// `get_process` returns a *clone*, so any mutation made to it is dropped.
    /// Syscall handlers that need to persist changes (fd table, file offsets,
    /// heap size, priority, ...) must route through this helper so the edit
    /// lands on the real `ProcessControlBlock`.
    ///
    /// Returns `None` if no process with `pid` exists, otherwise `Some(f(...))`.
    ///
    /// ponytail: the closure runs while the `processes` write lock is held, so it
    /// must NOT call back into `ProcessManager` methods that lock `processes`
    /// (e.g. `get_process`) or the scheduler — do any VFS / memory work before
    /// calling this and keep the closure to plain field/map updates.
    pub fn with_process_mut<F, R>(&self, pid: Pid, f: F) -> Option<R>
    where
        F: FnOnce(&mut ProcessControlBlock) -> R,
    {
        let mut processes = self.processes.write();
        processes.get_mut(&pid).map(f)
    }

    /// Get current running process ID
    pub fn current_process(&self) -> Pid {
        self.current_process.load(Ordering::SeqCst)
    }

    /// Get process count
    pub fn process_count(&self) -> usize {
        self.process_count.load(Ordering::SeqCst)
    }

    /// Schedule next process (called by timer interrupt)
    pub fn schedule(&self) -> Result<Option<Pid>, &'static str> {
        let mut scheduler = self.scheduler.lock();
        scheduler.schedule()
    }

    /// Update current process
    pub fn set_current_process(&self, pid: Pid) {
        self.current_process.store(pid, Ordering::SeqCst);
    }

    /// Handle system call
    pub fn handle_syscall(&self, syscall_number: u64, args: &[u64]) -> Result<u64, &'static str> {
        let mut dispatcher = self.syscall_dispatcher.lock();
        dispatcher.dispatch(syscall_number, args, self)
    }

    /// Block current process
    pub fn block_process(&self, pid: Pid) -> Result<(), &'static str> {
        {
            let mut processes = self.processes.write();
            if let Some(pcb) = processes.get_mut(&pid) {
                pcb.set_state(ProcessState::Blocked);
            } else {
                return Err("Process not found");
            }
        }

        // Remove from scheduler ready queue
        {
            let mut scheduler = self.scheduler.lock();
            scheduler.block_process(pid)?;
        }

        Ok(())
    }

    /// Unblock a process
    pub fn unblock_process(&self, pid: Pid) -> Result<(), &'static str> {
        {
            let mut processes = self.processes.write();
            if let Some(pcb) = processes.get_mut(&pid) {
                pcb.set_state(ProcessState::Ready);
            } else {
                return Err("Process not found");
            }
        }

        // Add back to scheduler
        {
            let mut scheduler = self.scheduler.lock();
            let priority = {
                let processes = self.processes.read();
                processes
                    .get(&pid)
                    .map(|p| p.priority)
                    .unwrap_or(Priority::Normal)
            };
            scheduler.add_process(pid, priority)?;
        }

        Ok(())
    }

    /// List all processes
    pub fn list_processes(&self) -> Vec<(Pid, String, ProcessState, Priority)> {
        let processes = self.processes.read();
        processes
            .iter()
            .map(|(&pid, pcb)| (pid, pcb.name_str().to_string(), pcb.state, pcb.priority))
            .collect()
    }

    /// Create a thread for a process
    pub fn create_thread(
        &self,
        pid: Pid,
        name: &str,
        priority: Priority,
        stack_size: usize,
        entry_point: u64,
    ) -> Result<thread::Tid, &'static str> {
        // Verify process exists
        {
            let processes = self.processes.read();
            if !processes.contains_key(&pid) {
                return Err("Process not found");
            }
        }

        // Create the thread
        let thread_manager = thread::get_thread_manager();
        let tid =
            thread_manager.create_user_thread(pid, name, priority, stack_size, entry_point)?;

        // If this is the first thread for the process, mark it as main thread
        {
            let mut processes = self.processes.write();
            if let Some(pcb) = processes.get_mut(&pid) {
                if pcb.main_thread.is_none() {
                    pcb.main_thread = Some(tid);
                }
            }
        }

        Ok(tid)
    }

    /// Get all threads for a process
    pub fn get_process_threads(&self, pid: Pid) -> Vec<thread::Tid> {
        let thread_manager = thread::get_thread_manager();
        thread_manager.get_process_threads(pid)
    }

    /// Terminate all threads for a process
    pub fn terminate_process_threads(&self, pid: Pid) -> Result<(), &'static str> {
        let thread_manager = thread::get_thread_manager();
        let threads = thread_manager.get_process_threads(pid);

        for tid in threads {
            thread_manager.terminate_thread(tid, -1)?;
        }

        Ok(())
    }

    /// Create a pipe for a process
    pub fn create_pipe(&self) -> Result<(u32, u32), &'static str> {
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.create_pipe()
    }

    /// Create shared memory segment
    pub fn create_shared_memory(
        &self,
        size: usize,
        permissions: ipc::SharedMemoryPermissions,
    ) -> Result<ipc::IpcId, &'static str> {
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.create_shared_memory(size, permissions)
    }

    /// Send signal to process
    pub fn send_signal(
        &self,
        target_pid: Pid,
        signal: ipc::Signal,
        sender_pid: Pid,
    ) -> Result<(), &'static str> {
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.send_signal(target_pid, signal, sender_pid)
    }

    /// Set signal handler for process
    pub fn set_signal_handler(
        &self,
        pid: Pid,
        signal: ipc::Signal,
        disposition: ipc::SignalDisposition,
    ) -> Result<(), &'static str> {
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.set_signal_handler(pid, signal, disposition)
    }

    /// Get pending signals for process
    pub fn get_pending_signals(&self, pid: Pid) -> Vec<ipc::SignalInfo> {
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.get_pending_signals(pid)
    }
}

/// Global process manager instance
static PROCESS_MANAGER: ProcessManager = ProcessManager::new();

/// Get the global process manager
pub fn get_process_manager() -> &'static ProcessManager {
    &PROCESS_MANAGER
}

/// Initialize the process management system
pub fn init() -> Result<(), &'static str> {
    // Initialize core process management
    PROCESS_MANAGER.init()?;

    // Initialize thread management
    thread::init()?;

    // Initialize IPC system
    ipc::init()?;

    // Initialize integration with other kernel systems
    integration::init()?;

    Ok(())
}

/// Get current system time in milliseconds (integrated with hardware timer system)
pub fn get_system_time() -> u64 {
    // Use the hardware timer system for accurate time
    crate::time::uptime_ms()
}

/// Update system time tracking (called by timer interrupt)
pub fn tick_system_time() {
    // This function is now a no-op since we use hardware timer system directly
    // The actual time tracking is handled by the hardware timer interrupt in time.rs
    // This function is kept for compatibility with existing code
}

use core::sync::atomic::AtomicU64;

/// Get the currently running process ID
///
/// Returns the PID of the process currently executing on this CPU.
/// Returns 0 if running in kernel context with no user process.
pub fn current_pid() -> Pid {
    get_process_manager().current_process()
}

/// Terminate the current process
pub fn terminate_current_process() {
    let process_manager = get_process_manager();
    let pid = current_pid();
    let _ = process_manager.terminate_process(pid, 0);
}

/// Re-export send_signal from IPC module for convenience
pub use ipc::send_signal;
