//! Process Operations - fork, exec, wait, exit
//!
//! Implements POSIX-like process management operations.

use alloc::vec::Vec;
use spin::Mutex;

use super::pcb::{ProcessControlBlock, ProcessState};
use super::table::ProcessTable;
use crate::process::Pid;

/// Fork the current process - creates a copy of the parent process
pub fn fork(parent_pid: Pid, process_table: &Mutex<ProcessTable>) -> Result<Pid, &'static str> {
    let mut table = process_table.lock();

    // Get parent process
    let parent = table.get(parent_pid).ok_or("Parent process not found")?;

    // Allocate new PID for child
    let child_pid = table.allocate_pid()?;

    // Clone parent PCB for child
    let child = parent.clone_for_fork(child_pid);

    // Insert child into process table
    table.insert(child)?;

    // Note: In a full implementation, we would:
    // 1. Copy page tables with COW (copy-on-write) semantics
    // 2. Clone kernel stack
    // 3. Set return value in child's context to 0
    // 4. Set return value in parent's context to child_pid
    // 5. Add child to scheduler

    Ok(child_pid)
}

/// Execute a new program in the process - replaces process image
pub fn exec(
    pid: Pid,
    program: &[u8],
    args: &[&str],
    process_table: &Mutex<ProcessTable>,
) -> Result<(), &'static str> {
    let mut table = process_table.lock();

    // Get process
    let pcb = table.get_mut(pid).ok_or("Process not found")?;

    // Verify process is not zombie
    if pcb.is_zombie() {
        return Err("Cannot exec zombie process");
    }

    // Parse program (stub - would integrate with ELF loader)
    let entry_point = parse_program(program)?;

    // Clear old process image
    // Note: In full implementation, would:
    // 1. Free old memory pages
    // 2. Load new program from ELF
    // 3. Setup new stack and heap
    // 4. Initialize new page table
    // 5. Setup program arguments on stack

    // Set new entry point
    pcb.set_entry_point(entry_point);

    // Set arguments
    pcb.set_args(args);

    // Reset CPU context to start at entry point
    pcb.context.rip = entry_point;
    pcb.context.rsp = pcb.memory.stack_start + pcb.memory.stack_size - 16;

    // Reset process state
    pcb.state = ProcessState::Ready;
    pcb.cpu_time = 0;

    Ok(())
}

/// Wait for any child process to exit - blocks until child exits
pub fn wait(
    parent_pid: Pid,
    process_table: &Mutex<ProcessTable>,
) -> Result<(Pid, i32), &'static str> {
    loop {
        let mut table = process_table.lock();

        // Get parent process
        let parent = table.get(parent_pid).ok_or("Parent process not found")?;

        // Check if parent has any children
        if parent.child_count == 0 {
            return Err("No child processes");
        }

        // Look for zombie children
        let zombie_children = table.get_zombie_children(parent_pid);

        if let Some(&child_pid) = zombie_children.first() {
            // Found a zombie child - collect its exit status
            let child = table.get(child_pid).ok_or("Child process not found")?;

            let exit_status = child.exit_status.unwrap_or(-1);

            // Remove zombie child from process table
            drop(table);
            cleanup_process(child_pid, process_table)?;

            return Ok((child_pid, exit_status));
        }

        // No zombie children yet - in full implementation, would block here
        // For now, return error to indicate would block
        drop(table);

        // Yield CPU to allow children to exit
        crate::process::scheduler::yield_cpu();

        // In a real implementation, we would block the process here
        // and wake it up when a child exits via signal
        break;
    }

    Err("Would block waiting for child")
}

/// Wait for specific child process to exit
pub fn waitpid(
    parent_pid: Pid,
    child_pid: Pid,
    process_table: &Mutex<ProcessTable>,
) -> Result<i32, &'static str> {
    loop {
        let table = process_table.lock();

        // Verify child exists and parent is correct
        let child = table.get(child_pid).ok_or("Child process not found")?;

        if child.parent_pid != Some(parent_pid) {
            return Err("Not a child of this process");
        }

        // Check if child is zombie
        if child.is_zombie() {
            let exit_status = child.exit_status.unwrap_or(-1);
            drop(table);

            // Cleanup zombie
            cleanup_process(child_pid, process_table)?;

            return Ok(exit_status);
        }

        drop(table);

        // Yield CPU to allow child to exit
        crate::process::scheduler::yield_cpu();

        // In real implementation, would block here
        break;
    }

    Err("Would block waiting for specific child")
}

/// Exit current process with status code
pub fn exit(
    pid: Pid,
    status: i32,
    process_table: &Mutex<ProcessTable>,
) -> Result<(), &'static str> {
    let mut table = process_table.lock();

    // Get process
    let pcb = table.get_mut(pid).ok_or("Process not found")?;

    // Transition to zombie state
    pcb.zombify(status);

    // Note: In full implementation would:
    // 1. Close all file descriptors
    // 2. Free memory pages (but keep PCB)
    // 3. Reparent children to init
    // 4. Send SIGCHLD to parent
    // 5. Wake up parent if waiting
    // 6. Remove from scheduler

    // Reparent children if any
    let children = table.get_children(pid);
    drop(table);

    for child_pid in children {
        let mut table = process_table.lock();
        if let Some(child) = table.get_mut(child_pid) {
            child.parent_pid = Some(1); // Reparent to init (PID 1)
        }
    }

    // Remove from scheduler
    let pm = crate::process::get_process_manager();
    let _ = pm.block_process(pid);

    Ok(())
}

/// Get process ID
pub fn getpid(process_table: &Mutex<ProcessTable>) -> Pid {
    // In real implementation, would get from CPU-local storage
    let pm = crate::process_manager::get_process_manager();
    pm.current_pid()
}

/// Get parent process ID
pub fn getppid(pid: Pid, process_table: &Mutex<ProcessTable>) -> Result<Pid, &'static str> {
    let table = process_table.lock();
    let pcb = table.get(pid).ok_or("Process not found")?;
    pcb.parent_pid.ok_or("No parent process")
}

/// Cleanup a zombie process (internal helper)
fn cleanup_process(pid: Pid, process_table: &Mutex<ProcessTable>) -> Result<(), &'static str> {
    let mut table = process_table.lock();

    // Verify process is zombie
    if let Some(pcb) = table.get(pid) {
        if !pcb.is_zombie() {
            return Err("Process is not a zombie");
        }
    }

    // Remove from process table
    table.remove(pid)?;

    // Note: In full implementation would:
    // 1. Free all memory pages
    // 2. Close any remaining file descriptors
    // 3. Free kernel stack
    // 4. Free PCB memory

    Ok(())
}

/// Parse program and return entry point (stub)
fn parse_program(program: &[u8]) -> Result<u64, &'static str> {
    // This is a stub - in real implementation would:
    // 1. Parse ELF header
    // 2. Verify magic number
    // 3. Load segments into memory
    // 4. Resolve relocations
    // 5. Setup initial stack
    // 6. Return entry point

    if program.len() < 4 {
        return Err("Invalid program format");
    }

    // Check for ELF magic number
    if program[0..4] != [0x7f, b'E', b'L', b'F'] {
        return Err("Not an ELF binary");
    }

    // Return placeholder entry point
    // Real implementation would parse ELF and return actual entry point
    Ok(0x400000)
}

/// Create a new process (not a standard POSIX call, but useful for kernel)
pub fn process_create(
    name: &str,
    parent_pid: Option<Pid>,
    priority: crate::process::Priority,
) -> Result<Pid, &'static str> {
    let pm = crate::process_manager::get_process_manager();
    pm.create_process(parent_pid, name, priority)
}

/// Terminate a process (like kill)
pub fn process_terminate(pid: Pid, status: i32) -> Result<(), &'static str> {
    let pm = crate::process_manager::get_process_manager();
    let process_table = &pm.process_table;
    exit(pid, status, process_table)
}

/// Get current process control block
pub fn process_get_current() -> Result<ProcessControlBlock, &'static str> {
    let pm = crate::process_manager::get_process_manager();
    let pid = pm.current_pid();
    pm.get_process(pid).ok_or("Current process not found")
}
