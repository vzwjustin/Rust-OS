//! Process Operations - fork, exec, wait, exit
//!
//! Thin wrappers over the unified kernel process table.

use super::pcb::ProcessControlBlock;
use crate::process::{self, Pid, Priority};

/// Fork the current process - creates a copy of the parent process
pub fn fork(parent_pid: Pid) -> Result<Pid, &'static str> {
    process::integration::get_integration_manager().fork_process(parent_pid)
}

/// Execute a new program in the process - replaces process image
pub fn exec(pid: Pid, program: &[u8], args: &[&str], envp: &[&str]) -> Result<(), &'static str> {
    process::exec::exec_elf_binary(pid, program, args, envp)
}

/// Wait for any child process to exit - blocks until child exits
pub fn wait(parent_pid: Pid) -> Result<(Pid, i32), &'static str> {
    process::get_process_manager().reap_zombie_child(parent_pid, |_| true)
}

/// Wait for specific child process to exit
pub fn waitpid(parent_pid: Pid, child_pid: Pid) -> Result<i32, &'static str> {
    let (_, status) =
        process::get_process_manager().reap_zombie_child(parent_pid, |pcb| pcb.pid == child_pid)?;
    Ok(status)
}

/// Exit current process with status code
pub fn exit(pid: Pid, status: i32) -> Result<(), &'static str> {
    process::get_process_manager().terminate_process(pid, status)
}

/// Get process ID
pub fn getpid() -> Pid {
    process::current_pid()
}

/// Get parent process ID
pub fn getppid(pid: Pid) -> Result<Pid, &'static str> {
    process::get_process_manager()
        .get_process(pid)
        .and_then(|pcb| pcb.parent_pid)
        .ok_or("No parent process")
}

/// Create a new process (not a standard POSIX call, but useful for kernel)
pub fn process_create(
    name: &str,
    parent_pid: Option<Pid>,
    priority: Priority,
) -> Result<Pid, &'static str> {
    process::get_process_manager().create_process(name, parent_pid, priority)
}

/// Terminate a process (like kill)
pub fn process_terminate(pid: Pid, status: i32) -> Result<(), &'static str> {
    process::get_process_manager().terminate_process(pid, status)
}

/// Get current process control block
pub fn process_get_current() -> Result<ProcessControlBlock, &'static str> {
    let pid = process::current_pid();
    process::get_process_manager()
        .get_process(pid)
        .map(ProcessControlBlock::from_kernel)
        .ok_or("Current process not found")
}
