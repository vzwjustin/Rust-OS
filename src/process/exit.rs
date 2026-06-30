//! Process Exit / Wait Implementation
//!
//! Ports Linux `kernel/exit.c` to Rust for RustOS.  The public surface
//! mirrors Linux semantics while using RustOS types (`ProcessControlBlock`,
//! `ProcessManager`, `Pid`, etc.).
//!
//! Subsystem teardown helpers (`exit_mm`, `exit_files`, etc.) are provided
//! as stubs that perform the bookkeeping RustOS already supports, with
//! TODO markers for deeper integration.

#![allow(dead_code, unused_variables)]

extern crate alloc;

use super::{Pid, ProcessState};

// ────────────────────────────────────────────────────────────────────────────
// Wait option flags  (include/uapi/linux/wait.h)
// ────────────────────────────────────────────────────────────────────────────

/// Do not block if no child has exited.
pub const WNOHANG: u32 = 1;
/// Also report children that have been stopped by a signal.
pub const WUNTRACED: u32 = 2;
/// Report children that have been continued via SIGCONT.
pub const WCONTINUED: u32 = 8;
/// Do not remove the zombie from the process table (Linux extension).
pub const WNOWAIT: u32 = 0x0100_0000;

// ────────────────────────────────────────────────────────────────────────────
// Signal numbers needed by exit logic
// ────────────────────────────────────────────────────────────────────────────

/// Sent to a process group when the session leader exits.
pub const SIGHUP: u32 = 1;
/// Sent to threads in a group when one member calls `exit_group`.
pub const SIGKILL: u32 = 9;

// ────────────────────────────────────────────────────────────────────────────
// wstatus encoding helpers  (POSIX)
// ────────────────────────────────────────────────────────────────────────────

/// Encode an exit-code into a `wstatus` value (normal exit).
///
/// `wait4` stores `(exit_code & 0xff) << 8` in the stat buffer for a normal
/// exit.
#[inline]
pub fn encode_wstatus_exit(code: i32) -> u32 {
    ((code as u32) & 0xff) << 8
}

/// Encode a signal number into a `wstatus` value (killed by signal).
#[inline]
pub fn encode_wstatus_signal(sig: u32) -> u32 {
    sig & 0x7f
}

/// Encode a stop signal into a `wstatus` value (stopped by signal).
#[inline]
pub fn encode_wstatus_stopped(sig: u32) -> u32 {
    0x7f | ((sig & 0xff) << 8)
}

// ────────────────────────────────────────────────────────────────────────────
// WaitResult
// ────────────────────────────────────────────────────────────────────────────

/// Outcome returned by `wait_task_zombie` / `do_wait` to their callers.
#[derive(Debug, Clone)]
pub struct WaitResult {
    /// PID of the collected child.
    pub pid: Pid,
    /// Raw `wstatus` word as seen by userspace `wait4(2)`.
    pub wstatus: u32,
}

// ────────────────────────────────────────────────────────────────────────────
// WaitOptions  (parsed form of the raw flags word)
// ────────────────────────────────────────────────────────────────────────────

/// Decoded options supplied to `do_wait`.
#[derive(Debug, Clone, Default)]
pub struct WaitOptions {
    /// If set, return immediately when no child has exited.
    pub no_hang: bool,
    /// Also collect stopped children.
    pub untraced: bool,
    /// Also collect continued children.
    pub continued: bool,
    /// Collect without removing the zombie (peek semantics).
    pub no_wait: bool,
}

impl WaitOptions {
    /// Parse raw `options` word (WNOHANG | WUNTRACED | …).
    pub fn from_raw(options: u32) -> Self {
        Self {
            no_hang: options & WNOHANG != 0,
            untraced: options & WUNTRACED != 0,
            continued: options & WCONTINUED != 0,
            no_wait: options & WNOWAIT != 0,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// exit_mm
// ────────────────────────────────────────────────────────────────────────────

/// Release the process's memory map.
///
/// Mirrors `exit_mm()` in Linux.  In RustOS the VMM is not yet fully wired
/// to the PCB, so we clear the `MemoryInfo` bookkeeping and leave the page
/// tables for the VMM to reclaim.
pub fn exit_mm(pid: Pid) {
    let pm = crate::process::get_process_manager();
    pm.with_process_mut(pid, |pcb| {
        // Zero out the virtual memory descriptor so future accesses fault.
        pcb.memory.vm_size    = 0;
        pcb.memory.heap_size  = 0;
        pcb.memory.stack_size = 0;
        // TODO: walk VMAs, unmap pages, drop page-table structures.
    });
}

// ────────────────────────────────────────────────────────────────────────────
// exit_files
// ────────────────────────────────────────────────────────────────────────────

/// Release the process's open file descriptors.
///
/// Mirrors `exit_files()` in Linux.  All open file descriptors are closed
/// (their `FileDescriptor` entries are dropped) and the tables are cleared.
pub fn exit_files(pid: Pid) {
    let pm = crate::process::get_process_manager();
    pm.with_process_mut(pid, |pcb| {
        pcb.fd_table.clear();
        pcb.file_descriptors.clear();
    });
    // TODO: flush/sync dirty pages for files with write access.
    // TODO: decrement struct file refcounts (shared fd tables).
}

// ────────────────────────────────────────────────────────────────────────────
// exit_signals
// ────────────────────────────────────────────────────────────────────────────

/// Clean up signal state on exit.
///
/// Mirrors `exit_signals()` in Linux.  Clears the pending-signal queue and
/// signal-handler table for the exiting process.
pub fn exit_signals(pid: Pid) {
    let pm = crate::process::get_process_manager();
    pm.with_process_mut(pid, |pcb| {
        pcb.pending_signals.clear();
        pcb.signal_handlers.clear();
    });
    // TODO: notify thread-group siblings, wake waiters, handle SIGHUP on
    // session-leader exit.
}

// ────────────────────────────────────────────────────────────────────────────
// release_task
// ────────────────────────────────────────────────────────────────────────────

/// Remove a zombie task from the process table, freeing its PCB.
///
/// Mirrors `release_task()` in Linux.  Called after the parent has collected
/// the exit status via `wait4`.
///
/// In RustOS this simply removes the process from the global table; the PCB
/// is dropped automatically as the map entry is deleted.
pub fn release_task(pid: Pid) {
    let pm = crate::process::get_process_manager();
    // `terminate_process` marks the PCB as Zombie and removes it from the
    // scheduler.  We use exit_status=0 as a sentinel; the real code should
    // pass the already-stored code.
    let _ = pm.terminate_process(pid, 0);
    // TODO: reclaim kernel stack, release PID in PID namespace.
}

// ────────────────────────────────────────────────────────────────────────────
// do_exit  (the main exit path — never returns)
// ────────────────────────────────────────────────────────────────────────────

/// Main process-exit handler.
///
/// Mirrors `do_exit()` in Linux `kernel/exit.c`.  Tears down all process
/// resources, marks the PCB as Zombie, and yields to the scheduler so that
/// the parent can collect the exit status.  Never returns.
///
/// # Safety
///
/// This function does not return.  The caller must ensure no kernel stack
/// variables require cleanup after this point.
pub fn do_exit(code: i32) -> ! {
    let pm  = crate::process::get_process_manager();
    let pid = pm.current_process();

    // ── Tear down subsystems ────────────────────────────────────────────────

    // 1. Remove from thread group / notify siblings.
    exit_signals(pid);

    // 2. Release memory map.
    exit_mm(pid);

    // 3. Close all file descriptors.
    exit_files(pid);

    // 4. cgroup charge accounting.
    crate::cgroup::fork_uncharge(pid);

    // 5. Namespace teardown.
    crate::namespace::clear(pid);

    // ── Mark as zombie and save exit code ───────────────────────────────────

    // `retire_spawned_process` sets state → Zombie, stores exit_status, fires
    // ptrace exit event, clears rseq/namespace/cgroup/scheduler entries.
    let _ = pm.retire_spawned_process(pid, code);

    // ── Wake the parent ─────────────────────────────────────────────────────

    // Signal the parent process so it can collect us via wait4.
    if let Some(pcb) = pm.get_process(pid) {
        if let Some(ppid) = pcb.parent_pid {
            // Send SIGCHLD to the parent.
            let _ = crate::process::ipc::get_ipc_manager()
                .send_signal(ppid, crate::process::ipc::Signal::SIGCHLD, pid);
        }
    }

    // ── Schedule away — we never come back ─────────────────────────────────

    // Force the scheduler to pick the next runnable task.  The Zombie PCB
    // stays in the table until the parent calls wait4.
    pm.schedule();

    // SAFETY: schedule() should not return when the current task is a Zombie.
    // If it does (e.g. during early boot before a parent exists), halt.
    unsafe {
        core::arch::asm!("cli; hlt", options(noreturn, nomem, nostack));
    }
}

// ────────────────────────────────────────────────────────────────────────────
// do_group_exit
// ────────────────────────────────────────────────────────────────────────────

/// Exit all threads in the current thread group.
///
/// Mirrors `do_group_exit()` in Linux.  Sends `SIGKILL` to every other
/// thread in the group, then calls `do_exit` for the calling thread.
///
/// In RustOS thread groups are managed by the `thread` module; we collect
/// group members via the process table by matching `tgid`.  Since RustOS
/// `ProcessControlBlock` does not yet have a `tgid` field, we fall back to
/// exiting only the current process.
pub fn do_group_exit(code: i32) -> ! {
    let pm  = crate::process::get_process_manager();
    let pid = pm.current_process();

    // TODO: once PCB gains a `tgid` field, iterate all processes with
    // tgid == current tgid and send them SIGKILL.

    do_exit(code)
}

// ────────────────────────────────────────────────────────────────────────────
// wait_task_zombie
// ────────────────────────────────────────────────────────────────────────────

/// Collect a zombie child and return its exit status.
///
/// Mirrors `wait_task_zombie()` in Linux.  The zombie is removed from the
/// process table (unless `WNOWAIT` is set).
///
/// Returns `Ok(WaitResult)` on success, `Err(-ECHILD)` if the target is not
/// a zombie child of the caller.
pub fn wait_task_zombie(
    child_pid: Pid,
    options: u32,
) -> Result<WaitResult, i32> {
    const ECHILD: i32 = -10;

    let pm = crate::process::get_process_manager();

    // Confirm the target is a Zombie.
    let child = pm.get_process(child_pid).ok_or(ECHILD)?;
    if !matches!(child.state, ProcessState::Zombie) {
        return Err(ECHILD);
    }

    let exit_code = child.exit_status.unwrap_or(0);
    let wstatus   = encode_wstatus_exit(exit_code);

    if options & WNOWAIT == 0 {
        // Actually reap: remove the zombie from the table.
        let _ = pm.terminate_process(child_pid, exit_code);
    }

    Ok(WaitResult { pid: child_pid, wstatus })
}

// ────────────────────────────────────────────────────────────────────────────
// do_wait  (core wait logic)
// ────────────────────────────────────────────────────────────────────────────

/// Core `wait` implementation.
///
/// Mirrors `do_wait()` in Linux.  Searches for a zombie child of `parent_pid`
/// that satisfies `pid_filter`:
///
/// - `pid_filter > 0`  → wait for that specific child.
/// - `pid_filter == 0` → wait for any child in the same process group.
/// - `pid_filter == -1` → wait for any child.
/// - `pid_filter < -1` → wait for any child in process group `|pid_filter|`.
///
/// If no qualifying zombie exists and `WNOHANG` is set, returns `Ok(None)`.
/// If no children exist at all, returns `Err(-ECHILD)`.
pub fn do_wait(
    parent_pid: Pid,
    pid_filter: i64,
    options: u32,
) -> Result<Option<WaitResult>, i32> {
    const ECHILD: i32 = -10;

    let wo = WaitOptions::from_raw(options);
    let pm = crate::process::get_process_manager();

    // Determine the caller's process group once, outside the closure.
    let caller_pgid = pm.get_process(parent_pid).map(|p| p.pgid).unwrap_or(0);

    // Use the ProcessManager's built-in zombie-finder.
    let zombie = pm.find_zombie_child(parent_pid, |pcb| {
        match pid_filter {
            -1    => true,                             // any child
            0     => pcb.pgid == caller_pgid,          // same pgrp as caller
            n if n > 0 => pcb.pid == n as Pid,        // specific PID
            n     => pcb.pgid == (-n) as u32,          // specific pgrp
        }
    });

    if let Some(child_pcb) = zombie {
        let child_pid  = child_pcb.pid;
        let exit_code  = child_pcb.exit_status.unwrap_or(0);
        let wstatus    = encode_wstatus_exit(exit_code);

        if !wo.no_wait {
            // Reap the zombie.
            let _ = pm.terminate_process(child_pid, exit_code);
        }

        return Ok(Some(WaitResult { pid: child_pid, wstatus }));
    }

    // No zombie found.
    if wo.no_hang {
        // Caller asked not to block.
        return Ok(None);
    }

    // Block until a child changes state.
    // TODO: put current task to sleep on a wait-queue and wake it when a
    // child exits (via SIGCHLD handler or schedule loop).
    // For now, return ECHILD so callers can retry.
    Err(ECHILD)
}

// ────────────────────────────────────────────────────────────────────────────
// sys_wait4  (syscall entry point)
// ────────────────────────────────────────────────────────────────────────────

/// `wait4(2)` syscall handler.
///
/// Collects a child process exit status.  `pid` semantics are identical to
/// `waitpid(2)`:
///
/// - `pid > 0`  → specific child.
/// - `pid == 0` → any child in the caller's process group.
/// - `pid == -1` → any child.
/// - `pid < -1` → any child in the process group `|pid|`.
///
/// `stat_addr` receives the `wstatus` word; `options` is `WNOHANG | …`;
/// `ru` (rusage) is a stub.
///
/// Returns the PID of the collected child, `0` with `WNOHANG` if no child
/// has exited yet, or a negative errno.
pub fn sys_wait4(
    pid: i32,
    stat_addr: *mut u32,
    options: u32,
    ru: *mut u8, // struct rusage — stub, ignored
) -> i64 {
    let pm = crate::process::get_process_manager();
    let caller_pid = pm.current_process();

    match do_wait(caller_pid, pid as i64, options) {
        Ok(Some(result)) => {
            if !stat_addr.is_null() {
                // Safety: caller guarantees a valid userspace pointer.
                unsafe { stat_addr.write_volatile(result.wstatus) };
            }
            result.pid as i64
        }
        Ok(None) => 0, // WNOHANG, no child ready
        Err(e)   => e as i64,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// sys_exit  /  sys_exit_group
// ────────────────────────────────────────────────────────────────────────────

/// `exit(2)` syscall handler — exit the calling thread.
pub fn sys_exit(status: i32) -> ! {
    do_exit(status)
}

/// `exit_group(2)` syscall handler — exit all threads in the thread group.
pub fn sys_exit_group(status: i32) -> ! {
    do_group_exit(status)
}
