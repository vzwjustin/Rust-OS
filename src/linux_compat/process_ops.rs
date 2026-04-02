//! Linux process/thread operation APIs
//!
//! This module implements Linux-compatible process and thread operations
//! including user/group IDs, process groups, sessions, and resource usage.
//!
//! Integrated with RustOS process manager, scheduler, and ELF loader.

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxResult, LinuxError};

// Re-export types for external access
pub use super::types::Rusage;

// Import process management infrastructure
use crate::process::{self, Priority};
use crate::process::Pid as KernelPid;
use crate::process_manager;

/// Operation counter for statistics
static PROCESS_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize process operations subsystem
pub fn init_process_operations() {
    PROCESS_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of process operations performed
pub fn get_operation_count() -> u64 {
    PROCESS_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    PROCESS_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get current process PCB or return error
fn current_pcb() -> LinuxResult<process::ProcessControlBlock> {
    let pid = process::current_pid();
    let process_manager = process::get_process_manager();
    process_manager.get_process(pid)
        .ok_or(LinuxError::ESRCH)
}

/// Get any process PCB by PID
fn get_pcb(pid: KernelPid) -> LinuxResult<process::ProcessControlBlock> {
    let process_manager = process::get_process_manager();
    process_manager.get_process(pid)
        .ok_or(LinuxError::ESRCH)
}

//
// Process Lifecycle Operations
//

/// fork - create child process
pub fn fork() -> LinuxResult<Pid> {
    inc_ops();

    let parent_pid = process::current_pid();
    let process_mgr = process_manager::get_process_manager();

    // Use process_manager fork which handles:
    // - PCB cloning
    // - Memory COW setup
    // - File descriptor duplication
    // - Scheduler integration
    process_mgr.fork(parent_pid)
        .map(|child_pid| child_pid as i32)
        .map_err(|_| LinuxError::EAGAIN)
}

/// exec - execute new program in current process
pub fn exec(program: &[u8], args: &[&str]) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process_manager::get_process_manager();

    // Use process_manager exec which handles:
    // - ELF loading via elf_loader
    // - Memory replacement
    // - Argument setup
    // - Context initialization
    process_mgr.exec(pid, program, args)
        .map_err(|_| LinuxError::ENOEXEC)?;

    Ok(0)
}

/// wait - wait for any child process to exit
pub fn wait(status: *mut i32) -> LinuxResult<Pid> {
    inc_ops();

    let parent_pid = process::current_pid();
    let process_mgr = process_manager::get_process_manager();

    // Use process_manager wait which handles:
    // - Zombie child detection
    // - Exit status collection
    // - Zombie cleanup
    // - Blocking behavior
    match process_mgr.wait(parent_pid) {
        Ok((child_pid, exit_status)) => {
            // Write exit status to user pointer if provided
            if !status.is_null() {
                unsafe { *status = exit_status; }
            }
            Ok(child_pid as i32)
        }
        Err("No child processes") => Err(LinuxError::ECHILD),
        Err("Would block waiting for child") => Err(LinuxError::EAGAIN),
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// waitpid - wait for specific child process
pub fn waitpid(pid: Pid, status: *mut i32, _options: i32) -> LinuxResult<Pid> {
    inc_ops();

    if pid < -1 || pid == 0 {
        // TODO: Support process group waits (pid < 0)
        return Err(LinuxError::EINVAL);
    }

    let parent_pid = process::current_pid();
    let process_mgr = process_manager::get_process_manager();

    let target_pid = if pid == -1 {
        // Wait for any child - delegate to wait()
        return wait(status);
    } else {
        pid as u32 as KernelPid
    };

    // Use process_manager waitpid
    match process_mgr.waitpid(parent_pid, target_pid) {
        Ok(exit_status) => {
            if !status.is_null() {
                unsafe { *status = exit_status; }
            }
            Ok(target_pid as i32)
        }
        Err("Not a child of this process") => Err(LinuxError::ECHILD),
        Err("Child process not found") => Err(LinuxError::ECHILD),
        Err("Would block waiting for specific child") => Err(LinuxError::EAGAIN),
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// execve - execute program (Linux-compatible syscall interface)
pub fn execve(_filename: *const u8, _argv: *const *const u8, _envp: *const *const u8) -> LinuxResult<i32> {
    inc_ops();
    // TODO: Parse filename, argv, envp and call exec()
    // For now, return ENOSYS
    Err(LinuxError::ENOSYS)
}

/// wait4 - wait for process to change state (Linux-compatible syscall interface)
pub fn wait4(pid: Pid, wstatus: *mut i32, options: i32, rusage: *mut Rusage) -> LinuxResult<Pid> {
    inc_ops();

    // For now, ignore rusage (resource usage statistics)
    if !rusage.is_null() {
        // TODO: Collect and return resource usage statistics
        unsafe {
            core::ptr::write_bytes(rusage, 0, 1);
        }
    }

    // Delegate to waitpid
    waitpid(pid, wstatus, options)
}

/// exit - terminate current process
pub fn exit(status: i32) -> ! {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process_manager::get_process_manager();

    // Use process_manager exit which handles:
    // - State transition to Zombie
    // - Resource cleanup
    // - Child reparenting
    // - Parent notification
    // - Scheduler removal
    let _ = process_mgr.exit(pid, status);

    // Should never return, but if it does, halt
    loop {
        x86_64::instructions::hlt();
    }
}

//
// Process Identity Operations
//

/// getpid - get process ID
pub fn getpid() -> Pid {
    inc_ops();
    process::current_pid() as Pid
}

/// getppid - get parent process ID
pub fn getppid() -> Pid {
    inc_ops();

    match current_pcb() {
        Ok(pcb) => pcb.parent_pid.unwrap_or(0) as Pid,
        Err(_) => 0, // Return 0 if cannot get PCB
    }
}

//
// User/Group ID Operations (Stub - credentials not yet implemented in PCB)
//

/// getuid - get real user ID
pub fn getuid() -> Uid {
    inc_ops();
    // TODO: Add uid field to ProcessControlBlock
    // For now, return 0 (root)
    0
}

/// geteuid - get effective user ID
pub fn geteuid() -> Uid {
    inc_ops();
    // TODO: Add euid field to ProcessControlBlock
    0
}

/// getgid - get real group ID
pub fn getgid() -> Gid {
    inc_ops();
    // TODO: Add gid field to ProcessControlBlock
    0
}

/// getegid - get effective group ID
pub fn getegid() -> Gid {
    inc_ops();
    // TODO: Add egid field to ProcessControlBlock
    0
}

/// setuid - set user ID
pub fn setuid(uid: Uid) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Implement once credentials are added to PCB
    // Only root (UID 0) can change to any UID
    if getuid() != 0 && uid != getuid() {
        return Err(LinuxError::EPERM);
    }

    // TODO: Store in PCB credentials
    Ok(0)
}

/// seteuid - set effective user ID
pub fn seteuid(uid: Uid) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Implement with PCB credentials
    let current_uid = getuid();
    if current_uid != 0 && uid != current_uid && uid != geteuid() {
        return Err(LinuxError::EPERM);
    }

    Ok(0)
}

/// setgid - set group ID
pub fn setgid(gid: Gid) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Implement with PCB credentials
    if getuid() != 0 && gid != getgid() {
        return Err(LinuxError::EPERM);
    }

    Ok(0)
}

/// setegid - set effective group ID
pub fn setegid(gid: Gid) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Implement with PCB credentials
    let current_uid = getuid();
    if current_uid != 0 && gid != getgid() && gid != getegid() {
        return Err(LinuxError::EPERM);
    }

    Ok(0)
}

//
// Process Group and Session Operations (Stub - not yet in PCB)
//

/// getpgid - get process group ID
pub fn getpgid(pid: Pid) -> LinuxResult<Pid> {
    inc_ops();

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Verify process exists
    let _ = get_pcb(target_pid)?;

    // TODO: Add pgid field to ProcessControlBlock
    // For now, return the PID itself as pgid
    Ok(target_pid as i32)
}

/// setpgid - set process group ID
pub fn setpgid(pid: Pid, pgid: Pid) -> LinuxResult<i32> {
    inc_ops();

    if pid < 0 || pgid < 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Verify process exists
    let _ = get_pcb(target_pid)?;

    // TODO: Add pgid field to ProcessControlBlock and update it
    Ok(0)
}

/// getsid - get session ID
pub fn getsid(pid: Pid) -> LinuxResult<Pid> {
    inc_ops();

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Verify process exists
    let _ = get_pcb(target_pid)?;

    // TODO: Add sid field to ProcessControlBlock
    // For now, return 1 (init session)
    Ok(1)
}

/// setsid - create new session
pub fn setsid() -> LinuxResult<Pid> {
    inc_ops();

    let pid = process::current_pid();

    // TODO: Check if process is not a process group leader
    // TODO: Create new session and process group
    // TODO: Add session/pgid fields to PCB

    // Return new session ID (same as process ID)
    Ok(pid as i32)
}

/// getpgrp - get process group
pub fn getpgrp() -> Pid {
    inc_ops();

    // TODO: Get from PCB pgid field
    // For now, return current PID
    process::current_pid() as i32
}

//
// Scheduling and Priority Operations
//

/// sched_yield - yield the processor
pub fn sched_yield() -> LinuxResult<i32> {
    inc_ops();

    // Use scheduler's yield function
    process::scheduler::yield_cpu();
    Ok(0)
}

/// getpriority - get scheduling priority
pub fn getpriority(which: i32, who: i32) -> LinuxResult<i32> {
    inc_ops();

    // PRIO constants
    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 {
                process::current_pid()
            } else {
                who as u32
            };

            // Get priority from process manager
            if let Some(priority) = process::scheduler::get_process_priority(target_pid) {
                // Convert Priority enum to nice value (-20 to 19)
                let nice = match priority {
                    Priority::RealTime => -20,
                    Priority::High => -10,
                    Priority::Normal => 0,
                    Priority::Low => 10,
                    Priority::Idle => 19,
                };
                Ok(nice)
            } else {
                Err(LinuxError::ESRCH)
            }
        }
        PRIO_PGRP | PRIO_USER => {
            // TODO: Implement process group and user priority
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// setpriority - set scheduling priority
pub fn setpriority(which: i32, who: i32, prio: i32) -> LinuxResult<i32> {
    inc_ops();

    // PRIO constants
    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    // Priority range is -20 (highest) to 19 (lowest)
    if prio < -20 || prio > 19 {
        return Err(LinuxError::EINVAL);
    }

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 {
                process::current_pid()
            } else {
                who as u32
            };

            // Check permissions - only root can increase priority
            let current_uid = getuid();
            if current_uid != 0 && prio < 0 {
                return Err(LinuxError::EACCES);
            }

            // Convert nice value to Priority enum
            let priority = match prio {
                p if p <= -15 => Priority::RealTime,
                p if p <= -5 => Priority::High,
                p if p <= 5 => Priority::Normal,
                p if p <= 15 => Priority::Low,
                _ => Priority::Idle,
            };

            // Set priority via scheduler
            process::scheduler::set_process_priority(target_pid, priority)
                .map_err(|_| LinuxError::ESRCH)?;

            Ok(0)
        }
        PRIO_PGRP | PRIO_USER => {
            // TODO: Implement process group and user priority
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// nice - change process priority
pub fn nice(inc: i32) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();

    // Get current priority
    let current_nice = getpriority(0, 0)?;
    let new_nice = (current_nice + inc).clamp(-20, 19);

    // Set new priority
    setpriority(0, pid as i32, new_nice)?;

    Ok(new_nice)
}

//
// CPU Affinity Operations
//

/// sched_setaffinity - set CPU affinity
pub fn sched_setaffinity(pid: Pid, cpusetsize: usize, mask: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 || cpusetsize > 128 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Verify process exists
    let _ = get_pcb(target_pid)?;

    // Read CPU mask from user space
    let mut cpu_mask: u64 = 0;
    unsafe {
        let bytes = core::slice::from_raw_parts(mask, core::cmp::min(cpusetsize, 8));
        for (i, &byte) in bytes.iter().enumerate() {
            cpu_mask |= (byte as u64) << (i * 8);
        }
    }

    // Validate mask has at least one CPU
    if cpu_mask == 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Update cpu_affinity in PCB
    // TODO: Notify scheduler of affinity change

    Ok(0)
}

/// sched_getaffinity - get CPU affinity
pub fn sched_getaffinity(pid: Pid, cpusetsize: usize, mask: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Get CPU affinity from PCB
    let pcb = get_pcb(target_pid)?;
    let cpu_affinity = pcb.sched_info.cpu_affinity;

    // Write affinity mask to user space
    unsafe {
        let bytes = core::slice::from_raw_parts_mut(mask, cpusetsize);
        for (i, byte) in bytes.iter_mut().enumerate() {
            if i < 8 {
                *byte = ((cpu_affinity >> (i * 8)) & 0xFF) as u8;
            } else {
                *byte = 0;
            }
        }
    }

    Ok(core::cmp::min(cpusetsize, 8) as i32)
}

//
// Resource Usage Operations
//

/// getrusage - get resource usage
pub fn getrusage(who: i32, usage: *mut Rusage) -> LinuxResult<i32> {
    inc_ops();

    if usage.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // WHO constants
    const RUSAGE_SELF: i32 = 0;
    const RUSAGE_CHILDREN: i32 = -1;
    const RUSAGE_THREAD: i32 = 1;

    match who {
        RUSAGE_SELF => {
            let pcb = current_pcb()?;

            // Fill in resource usage from PCB
            unsafe {
                (*usage).ru_utime.tv_sec = (pcb.cpu_time / 1000000) as i64;
                (*usage).ru_utime.tv_usec = (pcb.cpu_time % 1000000) as i64;
                (*usage).ru_stime.tv_sec = 0; // TODO: Track system time separately
                (*usage).ru_stime.tv_usec = 0;

                // Memory usage from PCB memory info
                (*usage).ru_maxrss = ((pcb.memory.heap_size + pcb.memory.stack_size +
                                      pcb.memory.code_size + pcb.memory.data_size) / 1024) as i64;

                // TODO: Track page faults and other statistics
                (*usage).ru_minflt = 0;
                (*usage).ru_majflt = 0;
                (*usage).ru_nvcsw = pcb.sched_info.schedule_count as i64;
                (*usage).ru_nivcsw = 0;
            }
            Ok(0)
        }
        RUSAGE_CHILDREN => {
            // TODO: Accumulate resource usage from terminated children
            unsafe {
                core::ptr::write_bytes(usage, 0, 1);
            }
            Ok(0)
        }
        RUSAGE_THREAD => {
            // TODO: Implement thread-specific resource tracking
            unsafe {
                core::ptr::write_bytes(usage, 0, 1);
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

//
// Process Control Operations
//

/// prctl - process control operations
pub fn prctl(option: i32, arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64) -> LinuxResult<i32> {
    inc_ops();

    // Common prctl options
    const PR_SET_NAME: i32 = 15;
    const PR_GET_NAME: i32 = 16;
    const PR_SET_DUMPABLE: i32 = 4;
    const PR_GET_DUMPABLE: i32 = 3;
    const PR_SET_PDEATHSIG: i32 = 1;
    const PR_GET_PDEATHSIG: i32 = 2;

    match option {
        PR_SET_NAME => {
            let name_ptr = arg2 as *const u8;
            if name_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }

            // TODO: Update process name in PCB
            // Read name from user space and update PCB.name
            Ok(0)
        }
        PR_GET_NAME => {
            let name_ptr = arg2 as *mut u8;
            if name_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }

            // Copy process name from PCB to user space
            let pcb = current_pcb()?;
            unsafe {
                let dest = core::slice::from_raw_parts_mut(name_ptr, 16);
                let copy_len = core::cmp::min(dest.len(), pcb.name.len());
                dest[..copy_len].copy_from_slice(&pcb.name[..copy_len]);
            }
            Ok(0)
        }
        PR_SET_DUMPABLE => {
            // TODO: Add dumpable flag to PCB
            Ok(0)
        }
        PR_GET_DUMPABLE => {
            // TODO: Get dumpable flag from PCB
            Ok(1) // Default to dumpable
        }
        PR_SET_PDEATHSIG | PR_GET_PDEATHSIG => {
            // TODO: Implement parent death signal
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

//
// Capability Operations (Stub)
//

/// capget - get process capabilities
pub fn capget(hdrp: *mut u8, _datap: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if hdrp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Implement capabilities system
    // For now, return success with no capabilities
    Ok(0)
}

/// capset - set process capabilities
pub fn capset(hdrp: *const u8, _datap: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if hdrp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Implement capabilities system
    // Requires CAP_SETPCAP capability
    Err(LinuxError::EPERM)
}

//
// Process Times Operations
//

/// times - get process times
pub fn times(buf: *mut u8) -> LinuxResult<i64> {
    inc_ops();

    if !buf.is_null() {
        let pcb = current_pcb()?;

        // Fill in tms structure (4 x i64 = 32 bytes)
        // tms_utime, tms_stime, tms_cutime, tms_cstime
        unsafe {
            let tms = buf as *mut i64;
            *tms.offset(0) = (pcb.cpu_time / 10) as i64; // User time in clock ticks
            *tms.offset(1) = 0; // System time (TODO: track separately)
            *tms.offset(2) = 0; // Children user time (TODO: accumulate)
            *tms.offset(3) = 0; // Children system time (TODO: accumulate)
        }
    }

    // Return clock ticks since boot
    let uptime_ms = process::get_system_time();
    let clock_ticks = uptime_ms / 10; // Assume 100Hz clock
    Ok(clock_ticks as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getpid() {
        let pid = getpid();
        assert!(pid >= 0);
    }

    #[test]
    fn test_uid_gid_operations() {
        let uid = getuid();
        let gid = getgid();
        assert!(uid == 0); // Root for now
        assert!(gid == 0); // Root group

        let euid = geteuid();
        let egid = getegid();
        assert_eq!(uid, euid);
        assert_eq!(gid, egid);
    }

    #[test]
    fn test_process_group_operations() {
        let pid = getpid();
        let pgid = getpgid(0).unwrap();
        assert!(pgid > 0);

        let pgrp = getpgrp();
        assert!(pgrp > 0);
    }

    #[test]
    fn test_priority_operations() {
        assert!(sched_yield().is_ok());

        let prio = getpriority(0, 0);
        assert!(prio.is_ok());
    }

    #[test]
    fn test_session_operations() {
        let sid = getsid(0);
        assert!(sid.is_ok());
    }
}
