//! Process Manager Module
//!
//! Provides high-level process management APIs including fork, exec, wait, exit.
//! This module wraps the core process management system with POSIX-like APIs.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

pub mod operations;
pub mod pcb;
pub mod table;

#[cfg(any())]
mod tests;

pub use pcb::{FileDescriptor, FileDescriptorType, ProcessControlBlock, ProcessState};
pub use table::ProcessTable;

use crate::process::{Pid, Priority};

/// Process Manager - Central coordinator for process lifecycle
pub struct ProcessManager {
    /// Process table containing all processes
    process_table: Mutex<ProcessTable>,
    /// Current running process ID
    current_pid: AtomicU32,
}

impl ProcessManager {
    /// Create a new process manager
    pub const fn new() -> Self {
        Self {
            process_table: Mutex::new(ProcessTable::new()),
            current_pid: AtomicU32::new(0),
        }
    }

    /// Initialize the process manager with init process
    pub fn init(&self) -> Result<(), &'static str> {
        let mut table = self.process_table.lock();

        // Create init process (PID 0)
        let init_pcb = ProcessControlBlock::new(0, None, "init", Priority::RealTime);
        table.insert(init_pcb)?;

        self.current_pid.store(0, Ordering::SeqCst);

        Ok(())
    }

    /// Get current process ID
    pub fn current_pid(&self) -> Pid {
        self.current_pid.load(Ordering::SeqCst)
    }

    /// Set current process ID
    pub fn set_current_pid(&self, pid: Pid) {
        self.current_pid.store(pid, Ordering::SeqCst);
    }

    /// Create a new process
    pub fn create_process(
        &self,
        parent_pid: Option<Pid>,
        name: &str,
        priority: Priority,
    ) -> Result<Pid, &'static str> {
        let mut table = self.process_table.lock();
        let pid = table.allocate_pid()?;

        let pcb = ProcessControlBlock::new(pid, parent_pid, name, priority);
        table.insert(pcb)?;

        Ok(pid)
    }

    /// Fork current process
    pub fn fork(&self, parent_pid: Pid) -> Result<Pid, &'static str> {
        operations::fork(parent_pid, &self.process_table)
    }

    /// Execute a program in a process
    pub fn exec(&self, pid: Pid, program: &[u8], args: &[&str]) -> Result<(), &'static str> {
        operations::exec(pid, program, args, &self.process_table)
    }

    /// Wait for any child process to exit
    pub fn wait(&self, parent_pid: Pid) -> Result<(Pid, i32), &'static str> {
        operations::wait(parent_pid, &self.process_table)
    }

    /// Wait for specific child process to exit
    pub fn waitpid(&self, parent_pid: Pid, child_pid: Pid) -> Result<i32, &'static str> {
        operations::waitpid(parent_pid, child_pid, &self.process_table)
    }

    /// Exit current process
    pub fn exit(&self, pid: Pid, status: i32) -> Result<(), &'static str> {
        operations::exit(pid, status, &self.process_table)
    }

    /// Get process control block
    pub fn get_process(&self, pid: Pid) -> Option<ProcessControlBlock> {
        let table = self.process_table.lock();
        table.get(pid)
    }

    /// Get parent process ID
    pub fn get_parent_pid(&self, pid: Pid) -> Option<Pid> {
        let table = self.process_table.lock();
        table.get(pid).and_then(|pcb| pcb.parent_pid)
    }

    /// List all processes
    pub fn list_processes(&self) -> Vec<(Pid, String, ProcessState, Priority)> {
        let table = self.process_table.lock();
        table.list_all()
    }

    /// Get process count
    pub fn process_count(&self) -> usize {
        let table = self.process_table.lock();
        table.count()
    }

    /// Get zombie processes for a parent
    pub fn get_zombie_children(&self, parent_pid: Pid) -> Vec<Pid> {
        let table = self.process_table.lock();
        table.get_zombie_children(parent_pid)
    }

    /// Clean up zombie process
    pub fn cleanup_zombie(&self, pid: Pid) -> Result<(), &'static str> {
        let mut table = self.process_table.lock();
        table.remove(pid)
    }

    /// Update process state
    pub fn set_process_state(&self, pid: Pid, state: ProcessState) -> Result<(), &'static str> {
        let mut table = self.process_table.lock();
        table.set_state(pid, state)
    }

    /// Allocate file descriptor for process
    pub fn allocate_fd(&self, pid: Pid, fd_type: FileDescriptorType) -> Result<u32, &'static str> {
        let mut table = self.process_table.lock();
        table.allocate_fd(pid, fd_type)
    }

    /// Close file descriptor
    pub fn close_fd(&self, pid: Pid, fd: u32) -> Result<(), &'static str> {
        let mut table = self.process_table.lock();
        table.close_fd(pid, fd)
    }

    /// Get file descriptor
    pub fn get_fd(&self, pid: Pid, fd: u32) -> Option<FileDescriptor> {
        let table = self.process_table.lock();
        table.get_fd(pid, fd)
    }
}

/// Global process manager instance
static PROCESS_MANAGER: ProcessManager = ProcessManager::new();

/// Get global process manager
pub fn get_process_manager() -> &'static ProcessManager {
    &PROCESS_MANAGER
}

/// Initialize process management system
pub fn init() -> Result<(), &'static str> {
    PROCESS_MANAGER.init()
}

/// Get current process ID
pub fn current_pid() -> Pid {
    PROCESS_MANAGER.current_pid()
}

/// Get process control block for current process
pub fn current_process() -> Option<ProcessControlBlock> {
    let pid = current_pid();
    PROCESS_MANAGER.get_process(pid)
}
