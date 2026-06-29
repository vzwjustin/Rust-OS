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
pub mod exec;
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
    /// FPU/SSE state for lazy FPU switching
    pub fpu: context::FpuState,
    /// Memory management information
    pub memory: MemoryInfo,
    /// Process name
    pub name: [u8; 32],
    /// Path of the executable image (set on execve)
    pub exec_path: alloc::string::String,
    /// CPU time used (in ticks)
    pub cpu_time: u64,
    /// User CPU time in clock ticks (USER_HZ)
    pub user_time_ticks: u64,
    /// System CPU time in clock ticks
    pub system_time_ticks: u64,
    /// Accumulated user CPU time of reaped children
    pub child_user_time: u64,
    /// Accumulated system CPU time of reaped children
    pub child_system_time: u64,
    /// Minor (soft) page faults
    pub minor_faults: u64,
    /// Major (hard) page faults
    pub major_faults: u64,
    /// Whether process can be dumped (core dumps, ptrace)
    pub dumpable: bool,
    /// Signal sent on parent death (0 = none)
    pub parent_death_signal: u32,
    /// Permitted capability mask
    pub cap_permitted: u64,
    /// Effective capability mask
    pub cap_effective: u64,
    /// Inheritable capability mask
    pub cap_inheritable: u64,
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
    /// Effective user ID
    pub euid: u32,
    /// Saved user ID
    pub suid: u32,
    /// Filesystem user ID
    pub fsuid: u32,
    /// Effective group ID
    pub egid: u32,
    /// Saved group ID
    pub sgid: u32,
    /// Filesystem group ID
    pub fsgid: u32,
    /// Process group ID
    pub pgid: Pid,
    /// Session ID
    pub sid: Pid,
    /// Supplementary group IDs
    pub supplementary_groups: Vec<u32>,
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
    /// mlockall() flags (MCL_CURRENT, MCL_FUTURE, MCL_ONFAULT)
    pub mlock_flags: i32,
    /// Default NUMA memory policy (MPOL_*)
    pub memory_policy: i32,
    /// NUMA node mask for memory policy
    pub nodemask: u64,
    /// Current program break (heap end)
    pub heap_break: usize,
    /// Initial program break at process start
    pub initial_break: usize,
    /// Number of locked pages (for RLIMIT_MEMLOCK accounting)
    pub locked_pages: usize,
    /// Per-process resource limits
    pub rlimits: ProcessRlimits,
    /// File creation mask (umask)
    pub umask: u32,
    /// Root directory path for this process (chroot)
    pub root_dir: alloc::string::String,
    /// ITIMER_REAL alarm deadline in uptime ticks (0 = no alarm armed)
    pub alarm_deadline: u64,
    /// ITIMER_REAL interval in ticks (0 = one-shot)
    pub alarm_interval: u64,
    /// Saved syscall info for restart_syscall (syscall number + args)
    pub restart_info: Option<(u64, [u64; 6])>,
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
            FileDescriptorType::VfsHandle { vfs_fd } => {
                crate::fs::vfs().seek(*vfs_fd, crate::fs::SeekFrom::Start(self.offset))?;
                let bytes_read = crate::fs::vfs().read(*vfs_fd, buffer)?;
                self.offset += bytes_read as u64;
                Ok(bytes_read)
            }
            FileDescriptorType::StandardInput => {
                // Non-blocking read from the keyboard event queue.
                // Each character event is encoded as a single byte; if no event
                // is pending we return 0 so the caller can block or poll.
                let mut written = 0usize;
                while written < buffer.len() {
                    match crate::keyboard::get_key_event() {
                        Some(crate::keyboard::KeyEvent::CharacterPress(c)) => {
                            if c == '\n' || c == '\r' {
                                buffer[written] = b'\n';
                                written += 1;
                                break;
                            } else if c.is_ascii() {
                                buffer[written] = c as u8;
                                written += 1;
                            }
                        }
                        Some(crate::keyboard::KeyEvent::SpecialPress(
                            crate::keyboard::SpecialKey::Enter,
                        )) => {
                            buffer[written] = b'\n';
                            written += 1;
                            break;
                        }
                        _ => break,
                    }
                }
                Ok(written)
            }
            FileDescriptorType::Pipe { pipe_id } => {
                let ipc_manager = crate::process::ipc::get_ipc_manager();
                let bytes_read = ipc_manager
                    .pipe_read(*pipe_id, buffer)
                    .map_err(|_| crate::fs::FsError::IoError)?;
                self.offset += bytes_read as u64;
                Ok(bytes_read)
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
            FileDescriptorType::VfsHandle { vfs_fd } => {
                crate::fs::vfs().seek(*vfs_fd, crate::fs::SeekFrom::Start(self.offset))?;
                let bytes_written = crate::fs::vfs().write(*vfs_fd, data)?;
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
            FileDescriptorType::Pipe { pipe_id } => {
                let ipc_manager = crate::process::ipc::get_ipc_manager();
                let bytes_written = ipc_manager
                    .pipe_write(*pipe_id, data)
                    .map_err(|_| crate::fs::FsError::IoError)?;
                self.offset += bytes_written as u64;
                Ok(bytes_written)
            }
            _ => Err(crate::fs::FsError::BadFileDescriptor),
        }
    }

    /// Get the current size of the underlying file, if applicable.
    /// Used by `lseek(SEEK_END)` to compute offsets from the end of the file.
    pub fn size(&self) -> Result<u64, crate::fs::FsError> {
        match &self.fd_type {
            FileDescriptorType::VfsFile { inode } => Ok(inode.size()),
            FileDescriptorType::VfsHandle { vfs_fd } => crate::vfs::vfs_fstat(*vfs_fd)
                .map(|stat| stat.size)
                .map_err(|_| crate::fs::FsError::IoError),
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

#[derive(Debug, Clone, PartialEq)]
pub enum FileDescriptorType {
    StandardInput,
    StandardOutput,
    StandardError,
    VfsFile { inode: crate::fs::Inode },
    VfsHandle { vfs_fd: i32 },
    Socket { socket_id: u32 },
    Pipe { pipe_id: u32 },
}

/// Single resource limit (soft + hard ceiling)
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimit {
    pub rlim_cur: u64,
    pub rlim_max: u64,
}

/// Linux RLIMIT_* indices 0..=15
pub const RLIMIT_COUNT: usize = 16;

/// Per-process resource limits
#[derive(Debug, Clone)]
pub struct ProcessRlimits {
    pub limits: [ResourceLimit; RLIMIT_COUNT],
}

impl Default for ProcessRlimits {
    fn default() -> Self {
        const INF: u64 = u64::MAX;
        let mut limits = [ResourceLimit {
            rlim_cur: INF,
            rlim_max: INF,
        }; RLIMIT_COUNT];
        // RLIMIT_NOFILE
        limits[7] = ResourceLimit {
            rlim_cur: 1024,
            rlim_max: 4096,
        };
        // RLIMIT_NPROC
        limits[6] = ResourceLimit {
            rlim_cur: 4096,
            rlim_max: 16384,
        };
        // RLIMIT_STACK
        limits[3] = ResourceLimit {
            rlim_cur: 8 * 1024 * 1024,
            rlim_max: INF,
        };
        // RLIMIT_MEMLOCK (64 KiB default for unprivileged processes)
        limits[8] = ResourceLimit {
            rlim_cur: 65536,
            rlim_max: 65536,
        };
        Self { limits }
    }
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
    /// Linux sched policy (SCHED_NORMAL, SCHED_FIFO, etc.)
    pub sched_policy: i32,
    /// Real-time scheduling priority (1-99 for FIFO/RR)
    pub sched_priority: i32,
    /// SCHED_RR quantum in nanoseconds
    pub rr_interval_ns: u64,
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
            fpu: context::FpuState::default(),
            memory: MemoryInfo::default(),
            name: [0; 32],
            exec_path: alloc::string::String::new(),
            cpu_time: 0,
            user_time_ticks: 0,
            system_time_ticks: 0,
            child_user_time: 0,
            child_system_time: 0,
            minor_faults: 0,
            major_faults: 0,
            dumpable: true,
            parent_death_signal: 0,
            cap_permitted: u64::MAX,
            cap_effective: u64::MAX,
            cap_inheritable: 0,
            creation_time: get_system_time(),
            exit_status: None,
            exit_code: None,
            uid: 0,
            gid: 0,
            euid: 0,
            suid: 0,
            fsuid: 0,
            egid: 0,
            sgid: 0,
            fsgid: 0,
            pgid: pid,
            sid: pid,
            supplementary_groups: Vec::new(),
            cwd: alloc::string::String::from("/"),
            fd_table: fd_table.clone(),
            next_fd: 3, // 0, 1, 2 reserved for stdin, stdout, stderr
            sched_info: SchedulingInfo {
                time_slice: 10, // 10ms default
                default_time_slice: 10,
                schedule_count: 0,
                last_scheduled: 0,
                cpu_affinity: 0xFFFFFFFFFFFFFFFF, // All CPUs
                sched_policy: 0,                  // SCHED_NORMAL
                sched_priority: 0,
                rr_interval_ns: 100_000_000, // 100 ms
            },
            main_thread: None,
            file_offsets: BTreeMap::new(),
            wake_time: None,
            signal_handlers: BTreeMap::new(),
            pending_signals: alloc::vec::Vec::new(),
            entry_point: 0,

            file_descriptors: fd_table,
            mlock_flags: 0,
            memory_policy: 0, // MPOL_DEFAULT
            nodemask: 0x1,    // Node 0 on single-node systems
            heap_break: 0,
            initial_break: 0,
            locked_pages: 0,
            rlimits: ProcessRlimits::default(),
            umask: 0o022,
            root_dir: alloc::string::String::from("/"),
            alarm_deadline: 0,
            alarm_interval: 0,
            restart_info: None,
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
            if let Some(parent) = parent_pid {
                if !crate::cgroup::can_fork(parent) {
                    return Err("cgroup pids controller denied fork");
                }
            }

            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
            let mut pcb = ProcessControlBlock::new(pid, parent_pid, name);
            pcb.priority = priority;

            if let Some(parent) = parent_pid {
                if let Some(parent_pcb) = processes.get(&parent) {
                    pcb.uid = parent_pcb.uid;
                    pcb.euid = parent_pcb.euid;
                    pcb.gid = parent_pcb.gid;
                    pcb.egid = parent_pcb.egid;
                    pcb.pgid = parent_pcb.pgid;
                    pcb.sid = parent_pcb.sid;
                    pcb.supplementary_groups = parent_pcb.supplementary_groups.clone();
                    pcb.dumpable = parent_pcb.dumpable;
                    pcb.parent_death_signal = parent_pcb.parent_death_signal;
                    pcb.cap_permitted = parent_pcb.cap_permitted;
                    pcb.cap_effective = parent_pcb.cap_effective;
                    pcb.cap_inheritable = parent_pcb.cap_inheritable;
                    pcb.sched_info.cpu_affinity = parent_pcb.sched_info.cpu_affinity;
                }
            }

            processes.insert(pid, pcb);
            self.process_count.fetch_add(1, Ordering::SeqCst);
            if let Some(parent) = parent_pid {
                if !crate::cgroup::fork_charge(parent, pid) {
                    processes.remove(&pid);
                    self.process_count.fetch_sub(1, Ordering::SeqCst);
                    return Err("cgroup pids controller charge failed");
                }
            }
            pid
        };

        // Add to scheduler
        {
            let mut scheduler = self.scheduler.lock();
            if let Err(e) = scheduler.add_process(pid, priority) {
                if parent_pid.is_some() {
                    crate::cgroup::fork_uncharge(pid);
                }
                let mut processes = self.processes.write();
                processes.remove(&pid);
                self.process_count.fetch_sub(1, Ordering::SeqCst);
                return Err(e);
            }
        }

        // Initialize IPC state for new process
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.init_process_signals(pid)?;

        Ok(pid)
    }

    /// Register a process created by `process_manager` (fork/exec) into the kernel scheduler.
    pub fn adopt_spawned_process(&self, pcb: ProcessControlBlock) -> Result<(), &'static str> {
        let pid = pcb.pid;
        let priority = pcb.priority;
        let parent_pid = pcb.parent_pid;
        if let Some(parent) = parent_pid {
            if !crate::cgroup::can_fork(parent) {
                return Err("cgroup pids controller denied fork");
            }
        }
        let is_new = {
            let mut processes = self.processes.write();
            if processes.contains_key(&pid) {
                processes.insert(pid, pcb);
                false
            } else {
                if self.process_count.load(Ordering::SeqCst) >= MAX_PROCESSES {
                    return Err("Maximum process count exceeded");
                }
                processes.insert(pid, pcb);
                self.process_count.fetch_add(1, Ordering::SeqCst);
                if let Some(parent) = parent_pid {
                    if !crate::cgroup::fork_charge(parent, pid) {
                        processes.remove(&pid);
                        self.process_count.fetch_sub(1, Ordering::SeqCst);
                        return Err("cgroup pids controller charge failed");
                    }
                }
                if pid >= self.next_pid.load(Ordering::SeqCst) {
                    self.next_pid.store(pid.saturating_add(1), Ordering::SeqCst);
                }
                true
            }
        };

        {
            let mut scheduler = self.scheduler.lock();
            if let Err(e) = scheduler.add_process(pid, priority) {
                if is_new {
                    crate::cgroup::fork_uncharge(pid);
                    let mut processes = self.processes.write();
                    processes.remove(&pid);
                    self.process_count.fetch_sub(1, Ordering::SeqCst);
                }
                return Err(e);
            }
        }

        if is_new {
            let ipc_manager = ipc::get_ipc_manager();
            ipc_manager.init_process_signals(pid)?;
        }

        Ok(())
    }

    /// Mark an externally spawned process as exited and remove it from scheduling.
    ///
    /// Used for fork/exec smoke-test children whose address-space ownership stays
    /// with the POSIX process manager; this avoids full kernel cleanup paths.
    pub fn retire_spawned_process(&self, pid: Pid, exit_status: i32) -> Result<(), &'static str> {
        {
            let mut processes = self.processes.write();
            let Some(pcb) = processes.get_mut(&pid) else {
                return Err("Process not found");
            };
            pcb.set_state(ProcessState::Zombie);
            pcb.exit_status = Some(exit_status);
            pcb.exit_code = Some(exit_status);
        }

        crate::ptrace::exit_event(pid, exit_status);
        crate::rseq::clear_for_pid(pid);
        crate::namespace::clear(pid);
        crate::privileged_syscalls::clear_for_pid(pid);
        crate::cgroup::fork_uncharge(pid);

        let mut scheduler = self.scheduler.lock();
        scheduler.remove_process(pid)
    }

    /// Terminate a process
    pub fn terminate_process(&self, pid: Pid, exit_status: i32) -> Result<(), &'static str> {
        {
            let mut processes = self.processes.write();
            if let Some(pcb) = processes.get_mut(&pid) {
                pcb.set_state(ProcessState::Zombie);
                pcb.exit_status = Some(exit_status);
                pcb.exit_code = Some(exit_status);
            } else {
                return Err("Process not found");
            }
        }

        crate::ptrace::exit_event(pid, exit_status);
        crate::rseq::clear_for_pid(pid);
        crate::namespace::clear(pid);
        crate::privileged_syscalls::clear_for_pid(pid);

        // Terminate all threads for this process
        self.terminate_process_threads(pid)?;

        // Cleanup IPC resources
        let ipc_manager = ipc::get_ipc_manager();
        ipc_manager.cleanup_process_ipc(pid)?;
        crate::cgroup::fork_uncharge(pid);

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
        crate::linux_integration::route_syscall(syscall_number, args)
            .map_err(|_| "System call failed")
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

    /// Return cloned PCBs matching a predicate
    pub fn find_processes<F>(&self, predicate: F) -> Vec<ProcessControlBlock>
    where
        F: Fn(&ProcessControlBlock) -> bool,
    {
        let processes = self.processes.read();
        processes
            .values()
            .filter(|pcb| predicate(pcb))
            .cloned()
            .collect()
    }

    /// Collect (pid, next_deadline) pairs for processes whose ITIMER_REAL
    /// alarm has expired as of `now_ms`. The caller is responsible for
    /// re-arming or clearing the deadline via `with_process_mut`.
    pub fn collect_expired_alarms(&self, now_ms: u64) -> Vec<(Pid, u64)> {
        let processes = self.processes.read();
        processes
            .iter()
            .filter_map(|(&pid, pcb)| {
                if pcb.alarm_deadline != 0 && now_ms >= pcb.alarm_deadline {
                    let next = if pcb.alarm_interval != 0 {
                        now_ms + pcb.alarm_interval
                    } else {
                        0
                    };
                    Some((pid, next))
                } else {
                    None
                }
            })
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

    /// Returns true if `parent_pid` has at least one child process.
    pub fn has_children(&self, parent_pid: Pid) -> bool {
        let processes = self.processes.read();
        processes
            .values()
            .any(|pcb| pcb.parent_pid == Some(parent_pid))
    }

    /// Reap the first zombie child of `parent_pid` matching `matches`.
    pub fn reap_zombie_child<F>(
        &self,
        parent_pid: Pid,
        matches: F,
    ) -> Result<(Pid, i32), &'static str>
    where
        F: Fn(&ProcessControlBlock) -> bool,
    {
        let child_pid = {
            let processes = self.processes.read();
            processes
                .iter()
                .find(|(_, pcb)| {
                    pcb.parent_pid == Some(parent_pid)
                        && matches!(pcb.state, ProcessState::Zombie)
                        && matches(pcb)
                })
                .map(|(&pid, _)| pid)
        };

        let Some(child_pid) = child_pid else {
            if self.has_children(parent_pid) {
                return Err("Would block waiting for child");
            }
            return Err("No child processes");
        };

        let (exit_status, child_user, child_sys) = {
            let processes = self.processes.read();
            let child = processes.get(&child_pid);
            let exit_status = child.and_then(|pcb| pcb.exit_status).unwrap_or(-1);
            let child_user = child
                .map(|pcb| {
                    if pcb.user_time_ticks > 0 {
                        pcb.user_time_ticks
                    } else {
                        pcb.cpu_time / 10
                    }
                })
                .unwrap_or(0);
            let child_sys = child.map(|pcb| pcb.system_time_ticks).unwrap_or(0);
            (exit_status, child_user, child_sys)
        };

        {
            let mut processes = self.processes.write();
            if let Some(parent) = processes.get_mut(&parent_pid) {
                parent.child_user_time = parent.child_user_time.saturating_add(child_user);
                parent.child_system_time = parent.child_system_time.saturating_add(child_sys);
            }
            processes.remove(&child_pid);
            self.process_count.fetch_sub(1, Ordering::SeqCst);
        }

        {
            let mut scheduler = self.scheduler.lock();
            let _ = scheduler.remove_process(child_pid);
        }

        Ok((child_pid, exit_status))
    }

    /// Find the first zombie child of `parent_pid` matching `matches` without reaping.
    pub fn find_zombie_child<F>(&self, parent_pid: Pid, matches: F) -> Option<ProcessControlBlock>
    where
        F: Fn(&ProcessControlBlock) -> bool,
    {
        let processes = self.processes.read();
        processes
            .values()
            .find(|pcb| {
                pcb.parent_pid == Some(parent_pid)
                    && matches!(pcb.state, ProcessState::Zombie)
                    && matches(pcb)
            })
            .cloned()
    }

    /// Return the highest scheduling priority (lowest nice value) among matching processes.
    pub fn max_nice_among<F>(&self, matches: F) -> Option<i32>
    where
        F: Fn(&ProcessControlBlock) -> bool,
    {
        let processes = self.processes.read();
        processes
            .values()
            .filter(|pcb| matches(pcb))
            .map(|pcb| priority_to_nice(pcb.priority))
            .min()
    }

    /// Set scheduler priority for all processes matching `matches`.
    pub fn set_priority_among<F>(&self, matches: F, priority: Priority) -> Result<(), &'static str>
    where
        F: Fn(&ProcessControlBlock) -> bool,
    {
        let pids: Vec<Pid> = {
            let processes = self.processes.read();
            processes
                .values()
                .filter(|pcb| matches(pcb))
                .map(|pcb| pcb.pid)
                .collect()
        };

        if pids.is_empty() {
            return Err("Process not found");
        }

        {
            let mut processes = self.processes.write();
            for pid in &pids {
                if let Some(pcb) = processes.get_mut(pid) {
                    pcb.priority = priority;
                }
            }
        }

        let mut scheduler = self.scheduler.lock();
        for pid in pids {
            scheduler.update_process_priority(pid, priority)?;
        }
        Ok(())
    }
}

/// Convert kernel priority to Linux nice value (-20..19).
pub fn priority_to_nice(priority: Priority) -> i32 {
    match priority {
        Priority::RealTime => -20,
        Priority::High => -10,
        Priority::Normal => 0,
        Priority::Low => 10,
        Priority::Idle => 19,
    }
}

/// Convert Linux nice value to kernel priority.
pub fn nice_to_priority(nice: i32) -> Priority {
    match nice {
        p if p <= -15 => Priority::RealTime,
        p if p <= -5 => Priority::High,
        p if p <= 5 => Priority::Normal,
        p if p <= 15 => Priority::Low,
        _ => Priority::Idle,
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
