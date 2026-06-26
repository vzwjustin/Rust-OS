//! Process Control Block (PCB) Implementation
//!
//! Defines the ProcessControlBlock structure and related types for managing process state.

use crate::process::{CpuContext, MemoryInfo, Pid, Priority};
use alloc::collections::BTreeMap;
use alloc::string::String;

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

/// Process Control Block - stores all information about a process
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
    /// Process name (32 bytes max)
    pub name: [u8; 32],
    /// CPU time used (in ticks)
    pub cpu_time: u64,
    /// Time when process was created
    pub creation_time: u64,
    /// Exit status (valid only when state is Zombie)
    pub exit_status: Option<i32>,
    /// File descriptor table
    pub fd_table: BTreeMap<u32, FileDescriptor>,
    /// Next file descriptor number
    pub next_fd: u32,
    /// Program entry point address
    pub entry_point: u64,
    /// Arguments passed to the process
    pub args: [u8; 256],
    /// Number of children processes
    pub child_count: u32,
}

/// File descriptor information
#[derive(Debug, Clone)]
pub struct FileDescriptor {
    pub fd_type: FileDescriptorType,
    pub flags: u32,
    pub offset: u64,
}

/// File descriptor types
#[derive(Debug, Clone)]
pub enum FileDescriptorType {
    StandardInput,
    StandardOutput,
    StandardError,
    File { path: [u8; 256] },
    Socket { socket_id: u32 },
    Pipe { pipe_id: u32, read_end: bool },
    Device { device_id: u32 },
}

impl ProcessControlBlock {
    /// Create a new PCB with the given PID and parent
    pub fn new(pid: Pid, parent_pid: Option<Pid>, name: &str, priority: Priority) -> Self {
        let mut fd_table = BTreeMap::new();

        // Initialize standard file descriptors
        fd_table.insert(
            0,
            FileDescriptor {
                fd_type: FileDescriptorType::StandardInput,
                flags: 0,
                offset: 0,
            },
        );
        fd_table.insert(
            1,
            FileDescriptor {
                fd_type: FileDescriptorType::StandardOutput,
                flags: 0,
                offset: 0,
            },
        );
        fd_table.insert(
            2,
            FileDescriptor {
                fd_type: FileDescriptorType::StandardError,
                flags: 0,
                offset: 0,
            },
        );

        let mut name_bytes = [0u8; 32];
        let name_slice = name.as_bytes();
        let copy_len = core::cmp::min(name_slice.len(), 31);
        name_bytes[..copy_len].copy_from_slice(&name_slice[..copy_len]);

        Self {
            pid,
            parent_pid,
            state: ProcessState::Ready,
            priority,
            context: CpuContext::default(),
            memory: MemoryInfo::default(),
            name: name_bytes,
            cpu_time: 0,
            creation_time: get_system_time(),
            exit_status: None,
            fd_table,
            next_fd: 3, // 0, 1, 2 reserved for stdin, stdout, stderr
            entry_point: 0,
            args: [0u8; 256],
            child_count: 0,
        }
    }

    /// Get process name as string slice
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
        matches!(self.state, ProcessState::Ready | ProcessState::Running)
    }

    /// Check if process is zombie
    pub fn is_zombie(&self) -> bool {
        matches!(self.state, ProcessState::Zombie)
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

    /// Get file descriptor
    pub fn get_fd(&self, fd: u32) -> Option<&FileDescriptor> {
        self.fd_table.get(&fd)
    }

    /// Clone this PCB for fork (creates a copy with new PID)
    pub fn clone_for_fork(&self, new_pid: Pid) -> Self {
        let mut new_pcb = self.clone();
        new_pcb.pid = new_pid;
        new_pcb.parent_pid = Some(self.pid);
        new_pcb.state = ProcessState::Ready;
        new_pcb.cpu_time = 0;
        new_pcb.creation_time = get_system_time();
        new_pcb.exit_status = None;
        new_pcb.child_count = 0;

        // Clone file descriptor table (COW semantics would apply to file contents)
        new_pcb.fd_table = self.fd_table.clone();

        // Clone memory info (COW semantics apply - will copy on write)
        new_pcb.memory = self.memory.clone();

        new_pcb
    }

    /// Set entry point for exec
    pub fn set_entry_point(&mut self, entry: u64) {
        self.entry_point = entry;
        self.context.rip = entry;
    }

    /// Set arguments
    pub fn set_args(&mut self, args: &[&str]) {
        self.args = [0u8; 256];
        let mut offset = 0;

        for arg in args.iter().take(8) {
            // Max 8 arguments
            let arg_bytes = arg.as_bytes();
            let copy_len = core::cmp::min(arg_bytes.len(), 30);
            if offset + copy_len + 1 < 256 {
                self.args[offset..offset + copy_len].copy_from_slice(&arg_bytes[..copy_len]);
                offset += copy_len;
                self.args[offset] = 0; // Null terminator
                offset += 1;
            }
        }
    }

    /// Get arguments as vector of strings
    pub fn get_args(&self) -> alloc::vec::Vec<alloc::string::String> {
        let mut args = alloc::vec::Vec::new();
        let mut start = 0;

        while start < 256 {
            if self.args[start] == 0 {
                break;
            }

            let mut end = start;
            while end < 256 && self.args[end] != 0 {
                end += 1;
            }

            if let Ok(s) = core::str::from_utf8(&self.args[start..end]) {
                args.push(alloc::string::String::from(s));
            }

            start = end + 1;
        }

        args
    }
}

/// Get current system time in milliseconds
fn get_system_time() -> u64 {
    // Placeholder - integrate with hardware timer
    crate::process::get_system_time()
}

impl Default for ProcessControlBlock {
    fn default() -> Self {
        Self::new(0, None, "default", Priority::Normal)
    }
}

/// Helper functions for process state transitions
impl ProcessControlBlock {
    /// Transition to running state
    pub fn start_running(&mut self) {
        if self.state == ProcessState::Ready {
            self.state = ProcessState::Running;
        }
    }

    /// Transition to ready state
    pub fn stop_running(&mut self) {
        if self.state == ProcessState::Running {
            self.state = ProcessState::Ready;
        }
    }

    /// Transition to blocked state
    pub fn block(&mut self) {
        if matches!(self.state, ProcessState::Running | ProcessState::Ready) {
            self.state = ProcessState::Blocked;
        }
    }

    /// Transition to ready state from blocked
    pub fn unblock(&mut self) {
        if self.state == ProcessState::Blocked {
            self.state = ProcessState::Ready;
        }
    }

    /// Transition to zombie state with exit status
    pub fn zombify(&mut self, exit_status: i32) {
        self.state = ProcessState::Zombie;
        self.exit_status = Some(exit_status);
    }

    /// Mark as dead (ready for cleanup)
    pub fn mark_dead(&mut self) {
        self.state = ProcessState::Dead;
    }
}
