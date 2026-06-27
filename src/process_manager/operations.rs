//! Process Operations - fork, exec, wait, exit
//!
//! Implements POSIX-like process management operations.

use alloc::vec::Vec;
use spin::Mutex;

use super::pcb::{ProcessControlBlock, ProcessState};
use super::table::ProcessTable;
use crate::process::elf_loader::{ElfLoader, ElfLoaderError};
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

    let elf_loader = ElfLoader::new(true, true);
    let loaded_binary = elf_loader
        .load_elf_binary(program, pid)
        .map_err(map_elf_loader_error)?;

    let rsp = build_argv_stack(loaded_binary.stack_top.as_u64(), args)?;

    pcb.memory.code_start = loaded_binary.base_address.as_u64();
    pcb.memory.code_size = loaded_binary
        .code_regions
        .iter()
        .map(|r| r.size as u64)
        .sum();
    pcb.memory.data_start = loaded_binary
        .data_regions
        .first()
        .map(|r| r.start.as_u64())
        .unwrap_or(0);
    pcb.memory.data_size = loaded_binary
        .data_regions
        .iter()
        .map(|r| r.size as u64)
        .sum();
    pcb.memory.heap_start = loaded_binary.heap_start.as_u64();
    pcb.memory.heap_size = 8 * 1024;
    pcb.memory.stack_start = loaded_binary
        .stack_top
        .as_u64()
        .saturating_sub(8 * 1024 * 1024);
    pcb.memory.stack_size = 8 * 1024 * 1024;

    pcb.set_entry_point(loaded_binary.entry_point.as_u64());
    pcb.set_args(args);
    pcb.context.rsp = rsp;
    pcb.context.rbp = rsp;
    pcb.context.rax = 0;
    pcb.context.rbx = 0;
    pcb.context.rcx = 0;
    pcb.context.rdx = 0;
    pcb.context.rsi = 0;
    pcb.context.rdi = 0;

    pcb.fd_table.retain(|&fd, _| fd <= 2);

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

fn map_elf_loader_error(err: ElfLoaderError) -> &'static str {
    match err {
        ElfLoaderError::MemoryAllocationFailed | ElfLoaderError::MappingFailed => "Out of memory",
        _ => "Invalid program format",
    }
}

/// Build a minimal argc/argv stack for process_manager exec().
fn build_argv_stack(stack_top: u64, args: &[&str]) -> Result<u64, &'static str> {
    let mut sp = stack_top & !0xF;

    let mut arg_addrs = Vec::with_capacity(args.len());
    for arg in args.iter().rev() {
        let bytes = arg.as_bytes();
        sp = sp.wrapping_sub((bytes.len() + 1) as u64);
        let addr = sp;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
            (addr as *mut u8).add(bytes.len()).write(0);
        }
        arg_addrs.push(addr);
    }
    arg_addrs.reverse();

    sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(0);
    }
    for &addr in arg_addrs.iter().rev() {
        sp = sp.wrapping_sub(8);
        unsafe {
            (sp as *mut u64).write(addr);
        }
    }
    sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(args.len() as u64);
    }

    if sp % 16 != 0 {
        return Err("Stack alignment failed");
    }

    Ok(sp)
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
