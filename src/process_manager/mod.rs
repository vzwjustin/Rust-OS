//! Process Manager Module
//!
//! POSIX-like process management facade over the canonical `crate::process` PCB table.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

pub mod operations;
pub mod pcb;
pub mod table;

pub use pcb::{FileDescriptor, FileDescriptorType, ProcessControlBlock, ProcessState};
pub use table::ProcessTable;

use crate::process::{self, Pid, Priority};

/// Process Manager - delegates to the kernel process table.
pub struct ProcessManager;

impl ProcessManager {
    fn kernel(&self) -> &'static process::ProcessManager {
        process::get_process_manager()
    }

    /// Initialize the process manager with init process
    pub fn init(&self) -> Result<(), &'static str> {
        process::init()
    }

    /// Get current process ID
    pub fn current_pid(&self) -> Pid {
        process::current_pid()
    }

    /// Set current process ID
    pub fn set_current_pid(&self, pid: Pid) {
        self.kernel().set_current_process(pid);
    }

    /// Create a new process
    pub fn create_process(
        &self,
        parent_pid: Option<Pid>,
        name: &str,
        priority: Priority,
    ) -> Result<Pid, &'static str> {
        self.kernel().create_process(name, parent_pid, priority)
    }

    /// Fork current process
    pub fn fork(&self, parent_pid: Pid) -> Result<Pid, &'static str> {
        process::integration::get_integration_manager().fork_process(parent_pid)
    }

    /// Execute a program in a process
    pub fn exec(
        &self,
        pid: Pid,
        program: &[u8],
        args: &[&str],
        envp: &[&str],
    ) -> Result<(), &'static str> {
        process::exec::exec_elf_binary(pid, program, args, envp)
    }

    /// Wait for any child process to exit
    pub fn wait(&self, parent_pid: Pid) -> Result<(Pid, i32), &'static str> {
        self.kernel()
            .reap_zombie_child(parent_pid, |_| true)
    }

    /// Wait for specific child process to exit
    pub fn waitpid(&self, parent_pid: Pid, child_pid: Pid) -> Result<i32, &'static str> {
        let (_, status) = self.kernel().reap_zombie_child(parent_pid, |pcb| {
            pcb.pid == child_pid
        })?;
        Ok(status)
    }

    /// Exit current process
    pub fn exit(&self, pid: Pid, status: i32) -> Result<(), &'static str> {
        self.kernel().terminate_process(pid, status)
    }

    /// Get process control block
    pub fn get_process(&self, pid: Pid) -> Option<ProcessControlBlock> {
        self.kernel()
            .get_process(pid)
            .map(ProcessControlBlock::from_kernel)
    }

    /// Set a process working directory.
    pub fn set_cwd(&self, pid: Pid, cwd: &str) -> Result<(), &'static str> {
        self.kernel()
            .with_process_mut(pid, |pcb| pcb.cwd = String::from(cwd))
            .ok_or("Process not found")
    }

    /// Get parent process ID
    pub fn get_parent_pid(&self, pid: Pid) -> Option<Pid> {
        self.kernel()
            .get_process(pid)
            .and_then(|pcb| pcb.parent_pid)
    }

    /// List all processes
    pub fn list_processes(&self) -> Vec<(Pid, String, ProcessState, Priority)> {
        self.kernel()
            .find_processes(|_| true)
            .into_iter()
            .map(|pcb| {
                let name = core::str::from_utf8(
                    &pcb.name[..pcb.name.iter().position(|&b| b == 0).unwrap_or(32)],
                )
                .unwrap_or("unknown");
                (
                    pcb.pid,
                    String::from(name),
                    map_kernel_state(pcb.state),
                    pcb.priority,
                )
            })
            .collect()
    }

    /// Get process count
    pub fn process_count(&self) -> usize {
        self.kernel().process_count()
    }

    /// Get zombie processes for a parent
    pub fn get_zombie_children(&self, parent_pid: Pid) -> Vec<Pid> {
        let kernel = self.kernel();
        let mut zombies = Vec::new();
        for pid in 0..=parent_pid.saturating_add(1024) {
            if let Some(pcb) = kernel.get_process(pid) {
                if pcb.parent_pid == Some(parent_pid) && matches!(pcb.state, process::ProcessState::Zombie) {
                    zombies.push(pid);
                }
            }
        }
        zombies
    }

    /// Clean up zombie process
    pub fn cleanup_zombie(&self, pid: Pid) -> Result<(), &'static str> {
        if let Some(parent_pid) = self.get_parent_pid(pid) {
            let _ = self
                .kernel()
                .reap_zombie_child(parent_pid, |pcb| pcb.pid == pid);
        }
        Ok(())
    }

    /// Update process state
    pub fn set_process_state(&self, pid: Pid, state: ProcessState) -> Result<(), &'static str> {
        self.kernel()
            .with_process_mut(pid, |pcb| pcb.set_state(map_pm_state(state)))
            .ok_or("Process not found")
    }

    /// Allocate file descriptor for process
    pub fn allocate_fd(&self, pid: Pid, fd_type: FileDescriptorType) -> Result<u32, &'static str> {
        self.kernel()
            .with_process_mut(pid, |pcb| pcb.allocate_fd(map_pm_fd_type(fd_type)))
            .ok_or("Process not found")
    }

    /// Close file descriptor
    pub fn close_fd(&self, pid: Pid, fd: u32) -> Result<(), &'static str> {
        match self
            .kernel()
            .with_process_mut(pid, |pcb| pcb.close_fd(fd))
        {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(e),
            None => Err("Process not found"),
        }
    }

    /// Point stdout/stderr at pipe write ends (used by GSpawn).
    pub fn redirect_stdio_to_pipes(
        &self,
        pid: Pid,
        stdout_pipe: Option<u32>,
        stderr_pipe: Option<u32>,
    ) -> Result<(), &'static str> {
        self.kernel()
            .with_process_mut(pid, |pcb| {
                if let Some(pipe_id) = stdout_pipe {
                    pcb.fd_table.insert(
                        1,
                        process::FileDescriptor {
                            fd_type: process::FileDescriptorType::Pipe { pipe_id },
                            flags: 0,
                            offset: 0,
                        },
                    );
                }
                if let Some(pipe_id) = stderr_pipe {
                    pcb.fd_table.insert(
                        2,
                        process::FileDescriptor {
                            fd_type: process::FileDescriptorType::Pipe { pipe_id },
                            flags: 0,
                            offset: 0,
                        },
                    );
                }
            })
            .ok_or("Process not found")
    }

    /// Get file descriptor
    pub fn get_fd(&self, pid: Pid, fd: u32) -> Option<FileDescriptor> {
        self.kernel()
            .get_process(pid)
            .and_then(|pcb| pcb.fd_table.get(&fd).cloned())
            .map(|fd| FileDescriptor::from_kernel(&fd))
    }
}

fn map_kernel_state(state: process::ProcessState) -> ProcessState {
    match state {
        process::ProcessState::Ready => ProcessState::Ready,
        process::ProcessState::Running => ProcessState::Running,
        process::ProcessState::Blocked => ProcessState::Blocked,
        process::ProcessState::Sleeping => ProcessState::Sleeping,
        process::ProcessState::Terminated => ProcessState::Terminated,
        process::ProcessState::Zombie => ProcessState::Zombie,
        process::ProcessState::Dead => ProcessState::Dead,
    }
}

fn map_pm_state(state: ProcessState) -> process::ProcessState {
    match state {
        ProcessState::Ready => process::ProcessState::Ready,
        ProcessState::Running => process::ProcessState::Running,
        ProcessState::Blocked => process::ProcessState::Blocked,
        ProcessState::Sleeping => process::ProcessState::Sleeping,
        ProcessState::Terminated => process::ProcessState::Terminated,
        ProcessState::Zombie => process::ProcessState::Zombie,
        ProcessState::Dead => process::ProcessState::Dead,
    }
}

fn map_pm_fd_type(fd_type: FileDescriptorType) -> process::FileDescriptorType {
    match fd_type {
        FileDescriptorType::StandardInput => process::FileDescriptorType::StandardInput,
        FileDescriptorType::StandardOutput => process::FileDescriptorType::StandardOutput,
        FileDescriptorType::StandardError => process::FileDescriptorType::StandardError,
        FileDescriptorType::File { path } => {
            let _path_str = core::str::from_utf8(
                &path[..path.iter().position(|&b| b == 0).unwrap_or(path.len())],
            )
            .unwrap_or("");
            process::FileDescriptorType::VfsHandle {
                vfs_fd: -1,
            }
        }
        FileDescriptorType::Socket { socket_id } => {
            process::FileDescriptorType::Socket { socket_id }
        }
        FileDescriptorType::Pipe { pipe_id, .. } => process::FileDescriptorType::Pipe { pipe_id },
        FileDescriptorType::Device { .. } => process::FileDescriptorType::StandardOutput,
    }
}

/// Global process manager instance
static PROCESS_MANAGER: ProcessManager = ProcessManager;
static PROCESS_MANAGER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Get global process manager
pub fn get_process_manager() -> &'static ProcessManager {
    &PROCESS_MANAGER
}

/// Initialize process management system (idempotent).
pub fn init() -> Result<(), &'static str> {
    if PROCESS_MANAGER_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }
    PROCESS_MANAGER.init()?;
    PROCESS_MANAGER_INITIALIZED.store(true, Ordering::Release);
    Ok(())
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
