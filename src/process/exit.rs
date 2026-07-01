//! Process Exit / Wait Implementation
//!
//! Ports Linux `kernel/exit.c` to Rust for RustOS.  The public surface
//! mirrors Linux semantics while using RustOS types (`ProcessControlBlock`,
//! `ProcessManager`, `Pid`, etc.).
//!
//! Subsystem teardown helpers (`exit_mm`, `exit_files`, etc.) perform real
//! resource cleanup using the VMM, VFS, IPC, and scheduler subsystems.

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
    /// User CPU time in clock ticks (USER_HZ = 100).
    pub user_time_ticks: u64,
    /// System CPU time in clock ticks.
    pub system_time_ticks: u64,
    /// Minor (soft) page faults.
    pub minor_faults: u64,
    /// Major (hard) page faults.
    pub major_faults: u64,
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
/// Mirrors `exit_mm()` in Linux.  Walks the VMM's region list and
/// unmaps each region, then zeroes the PCB's `MemoryInfo`.
pub fn exit_mm(pid: Pid) {
    let pm = crate::process::get_process_manager();

    // Unmap all VMM regions for this process.
    let vmm = crate::memory_manager::get_virtual_memory_manager();
    let mut vmm_guard = vmm.lock();
    if let Some(ref mut vm) = *vmm_guard {
        let regions = vm.dump_memory_map();
        for (start, _end, _prot, _typ) in &regions {
            let _ = vm.munmap(start.as_u64() as usize, 4096);
        }
    }
    drop(vmm_guard);

    pm.with_process_mut(pid, |pcb| {
        pcb.memory.vm_size = 0;
        pcb.memory.heap_size = 0;
        pcb.memory.stack_size = 0;
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
}

// ────────────────────────────────────────────────────────────────────────────
// exit_signals
// ────────────────────────────────────────────────────────────────────────────

/// Clean up signal state on exit.
///
/// Mirrors `exit_signals()` in Linux.  Clears the pending-signal queue and
/// signal-handler table for the exiting process.  If the process is a
/// session leader, sends SIGHUP to all processes in the session.
pub fn exit_signals(pid: Pid) {
    let pm = crate::process::get_process_manager();
    let is_session_leader = pm
        .get_process(pid)
        .map(|pcb| pcb.sid == pcb.pid)
        .unwrap_or(false);
    let sid = pm.get_process(pid).map(|pcb| pcb.sid).unwrap_or(0);

    pm.with_process_mut(pid, |pcb| {
        pcb.pending_signals.clear();
        pcb.signal_handlers.clear();
    });

    // If the exiting process is a session leader, send SIGHUP to all
    // processes in the session (POSIX requires this).
    if is_session_leader && sid != 0 {
        let ipc = crate::process::ipc::get_ipc_manager();
        // Collect all PIDs in the session to avoid holding locks during send.
        let session_pids: alloc::vec::Vec<u32> = pm
            .find_processes(|p| p.sid == sid && p.pid != pid)
            .iter()
            .map(|p| p.pid)
            .collect();
        for target_pid in session_pids {
            let _ = ipc.send_signal(target_pid, crate::process::ipc::Signal::SIGHUP, pid);
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// release_task
// ────────────────────────────────────────────────────────────────────────────

/// Remove a zombie task from the process table, freeing its PCB.
///
/// Mirrors `release_task()` in Linux.  Called after the parent has collected
/// the exit status via `wait4`.  Removes the process from the global table,
/// which drops the PCB and reclaims all its memory.
pub fn release_task(pid: Pid) {
    let pm = crate::process::get_process_manager();
    let _ = pm.terminate_process(pid, 0);
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
    let pm = crate::process::get_process_manager();
    let pid = pm.current_process();

    // ── Tear down subsystems ────────────────────────────────────────────────

    // 0. Clear child TID and futex-wake if CLONE_CHILD_CLEARTID was set.
    let clear_tid_addr = pm.get_process(pid).map(|p| p.clear_child_tid).unwrap_or(0);
    if clear_tid_addr != 0 {
        // SAFETY: the address was provided by a userspace clone() call and
        // is valid for writing a single zero word.
        unsafe {
            core::ptr::write_volatile(clear_tid_addr as *mut u32, 0);
        }
        // Futex wake: wake any thread waiting on this address.
        let _ = crate::futex::futex_wake(clear_tid_addr as *mut i32, 1, 0xffff_ffff);
    }

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
            let _ = crate::process::ipc::get_ipc_manager().send_signal(
                ppid,
                crate::process::ipc::Signal::SIGCHLD,
                pid,
            );
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
/// process that shares the same parent (thread-group members), then calls
/// `do_exit` for the calling thread.
pub fn do_group_exit(code: i32) -> ! {
    let pm = crate::process::get_process_manager();
    let pid = pm.current_process();

    // Collect all processes with the same parent (thread-group members).
    let parent_pid = pm.get_process(pid).and_then(|p| p.parent_pid).unwrap_or(0);
    let ipc = crate::process::ipc::get_ipc_manager();
    let siblings: alloc::vec::Vec<u32> = pm
        .find_processes(|p| {
            p.parent_pid == Some(parent_pid)
                && p.pid != pid
                && matches!(p.state, ProcessState::Ready | ProcessState::Running)
        })
        .iter()
        .map(|p| p.pid)
        .collect();
    for sibling_pid in siblings {
        let _ = ipc.send_signal(sibling_pid, crate::process::ipc::Signal::SIGKILL, pid);
    }

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
pub fn wait_task_zombie(child_pid: Pid, options: u32) -> Result<WaitResult, i32> {
    const ECHILD: i32 = -10;

    let pm = crate::process::get_process_manager();

    // Confirm the target is a Zombie.
    let child = pm.get_process(child_pid).ok_or(ECHILD)?;
    if !matches!(child.state, ProcessState::Zombie) {
        return Err(ECHILD);
    }

    let exit_code = child.exit_status.unwrap_or(0);
    let wstatus = encode_wstatus_exit(exit_code);
    let user_time_ticks = child.user_time_ticks;
    let system_time_ticks = child.system_time_ticks;
    let minor_faults = child.minor_faults;
    let major_faults = child.major_faults;

    if options & WNOWAIT == 0 {
        // Actually reap: remove the zombie from the table.
        let _ = pm.terminate_process(child_pid, exit_code);
    }

    Ok(WaitResult {
        pid: child_pid,
        wstatus,
        user_time_ticks,
        system_time_ticks,
        minor_faults,
        major_faults,
    })
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
pub fn do_wait(parent_pid: Pid, pid_filter: i64, options: u32) -> Result<Option<WaitResult>, i32> {
    const ECHILD: i32 = -10;

    let wo = WaitOptions::from_raw(options);
    let pm = crate::process::get_process_manager();

    // Determine the caller's process group once, outside the closure.
    let caller_pgid = pm.get_process(parent_pid).map(|p| p.pgid).unwrap_or(0);

    // Use the ProcessManager's built-in zombie-finder.
    let zombie = pm.find_zombie_child(parent_pid, |pcb| {
        match pid_filter {
            -1 => true,                        // any child
            0 => pcb.pgid == caller_pgid,      // same pgrp as caller
            n if n > 0 => pcb.pid == n as Pid, // specific PID
            n => pcb.pgid == (-n) as u32,      // specific pgrp
        }
    });

    if let Some(child_pcb) = zombie {
        let child_pid = child_pcb.pid;
        let exit_code = child_pcb.exit_status.unwrap_or(0);
        let wstatus = encode_wstatus_exit(exit_code);

        if !wo.no_wait {
            // Reap the zombie.
            let _ = pm.terminate_process(child_pid, exit_code);
        }

        return Ok(Some(WaitResult {
            pid: child_pid,
            wstatus,
            user_time_ticks: child_pcb.user_time_ticks,
            system_time_ticks: child_pcb.system_time_ticks,
            minor_faults: child_pcb.minor_faults,
            major_faults: child_pcb.major_faults,
        }));
    }

    // No zombie found.
    if wo.no_hang {
        // Caller asked not to block.
        return Ok(None);
    }

    // Block until a child changes state — yield CPU and re-scan.
    // This mirrors Linux's wait_event loop: the task is removed from
    // the run queue and woken when a child exits (via SIGCHLD).
    loop {
        crate::scheduler::yield_cpu();

        // Re-scan for zombie children matching the pid filter.
        let zombie = pm.find_zombie_child(parent_pid, |pcb| match pid_filter {
            -1 => true,
            0 => pcb.pgid == caller_pgid,
            n if n > 0 => pcb.pid == n as Pid,
            n => pcb.pgid == (-n) as u32,
        });
        if let Some(child_pcb) = zombie {
            let child_pid = child_pcb.pid;
            let exit_code = child_pcb.exit_status.unwrap_or(0);
            let wstatus = encode_wstatus_exit(exit_code);

            if !wo.no_wait {
                let _ = pm.terminate_process(child_pid, exit_code);
            }

            return Ok(Some(WaitResult {
                pid: child_pid,
                wstatus,
                user_time_ticks: child_pcb.user_time_ticks,
                system_time_ticks: child_pcb.system_time_ticks,
                minor_faults: child_pcb.minor_faults,
                major_faults: child_pcb.major_faults,
            }));
        }

        // Check if we still have any children at all.
        let has_children = pm
            .find_processes(|pcb| pcb.parent_pid == Some(parent_pid))
            .into_iter()
            .any(|pcb| !matches!(pcb.state, ProcessState::Zombie | ProcessState::Dead));
        if !has_children {
            return Err(ECHILD);
        }
    }
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
/// `ru` receives a `struct rusage` populated from the child's PCB.
///
/// Returns the PID of the collected child, `0` with `WNOHANG` if no child
/// has exited yet, or a negative errno.
pub fn sys_wait4(
    pid: i32,
    stat_addr: *mut u32,
    options: u32,
    ru: *mut u8, // struct rusage — written if non-null
) -> i64 {
    let pm = crate::process::get_process_manager();
    let caller_pid = pm.current_process();

    match do_wait(caller_pid, pid as i64, options) {
        Ok(Some(result)) => {
            if !stat_addr.is_null() {
                // Safety: caller guarantees a valid userspace pointer.
                unsafe { stat_addr.write_volatile(result.wstatus) };
            }
            // Write rusage from the child's PCB accounting fields.
            if !ru.is_null() {
                let rusage = crate::linux_compat::types::Rusage {
                    ru_utime: crate::linux_compat::types::TimeVal {
                        tv_sec: (result.user_time_ticks / 100) as i64,
                        tv_usec: ((result.user_time_ticks % 100) * 10_000) as i64,
                    },
                    ru_stime: crate::linux_compat::types::TimeVal {
                        tv_sec: (result.system_time_ticks / 100) as i64,
                        tv_usec: ((result.system_time_ticks % 100) * 10_000) as i64,
                    },
                    ru_maxrss: 0,
                    ru_ixrss: 0,
                    ru_idrss: 0,
                    ru_isrss: 0,
                    ru_minflt: result.minor_faults as i64,
                    ru_majflt: result.major_faults as i64,
                    ru_nswap: 0,
                    ru_inblock: 0,
                    ru_oublock: 0,
                    ru_msgsnd: 0,
                    ru_msgrcv: 0,
                    ru_nsignals: 0,
                    ru_nvcsw: 0,
                    ru_nivcsw: 0,
                };
                // Safety: caller guarantees a valid userspace pointer for
                // sizeof(struct rusage) bytes.
                unsafe {
                    core::ptr::write_unaligned(
                        ru as *mut crate::linux_compat::types::Rusage,
                        rusage,
                    );
                }
            }
            result.pid as i64
        }
        Ok(None) => 0, // WNOHANG, no child ready
        Err(e) => e as i64,
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
