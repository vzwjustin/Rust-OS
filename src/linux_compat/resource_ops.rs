//! Resource limit operations
//!
//! This module implements Linux resource limit operations including
//! getrlimit, setrlimit, prlimit, and resource usage tracking.

#![no_std]

extern crate alloc;

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxResult, LinuxError};

/// Operation counter for statistics
static RESOURCE_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize resource operations subsystem
pub fn init_resource_operations() {
    RESOURCE_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of resource operations performed
pub fn get_operation_count() -> u64 {
    RESOURCE_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    RESOURCE_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// Resource Limit Constants
// ============================================================================

pub mod rlimit_resource {
    /// Max CPU time in seconds
    pub const RLIMIT_CPU: i32 = 0;
    /// Max file size
    pub const RLIMIT_FSIZE: i32 = 1;
    /// Max data size
    pub const RLIMIT_DATA: i32 = 2;
    /// Max stack size
    pub const RLIMIT_STACK: i32 = 3;
    /// Max core file size
    pub const RLIMIT_CORE: i32 = 4;
    /// Max resident set size
    pub const RLIMIT_RSS: i32 = 5;
    /// Max number of processes
    pub const RLIMIT_NPROC: i32 = 6;
    /// Max number of open files
    pub const RLIMIT_NOFILE: i32 = 7;
    /// Max locked-in-memory address space
    pub const RLIMIT_MEMLOCK: i32 = 8;
    /// Max address space
    pub const RLIMIT_AS: i32 = 9;
    /// Max file locks
    pub const RLIMIT_LOCKS: i32 = 10;
    /// Max pending signals
    pub const RLIMIT_SIGPENDING: i32 = 11;
    /// Max bytes in POSIX message queues
    pub const RLIMIT_MSGQUEUE: i32 = 12;
    /// Max nice priority
    pub const RLIMIT_NICE: i32 = 13;
    /// Max real-time priority
    pub const RLIMIT_RTPRIO: i32 = 14;
    /// Max real-time timeout in microseconds
    pub const RLIMIT_RTTIME: i32 = 15;
}

/// Resource limit value for "unlimited"
pub const RLIM_INFINITY: u64 = !0;

/// Resource limit structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RLimit {
    /// Soft limit
    pub rlim_cur: u64,
    /// Hard limit (ceiling for rlim_cur)
    pub rlim_max: u64,
}

impl RLimit {
    pub fn unlimited() -> Self {
        RLimit {
            rlim_cur: RLIM_INFINITY,
            rlim_max: RLIM_INFINITY,
        }
    }

    pub fn new(cur: u64, max: u64) -> Self {
        RLimit {
            rlim_cur: cur,
            rlim_max: max,
        }
    }
}

// ============================================================================
// Resource Usage Structure
// ============================================================================

/// Resource usage structure (already defined in types, but extending here)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RUsage {
    /// User CPU time
    pub ru_utime: TimeVal,
    /// System CPU time
    pub ru_stime: TimeVal,
    /// Maximum resident set size
    pub ru_maxrss: i64,
    /// Integral shared memory size
    pub ru_ixrss: i64,
    /// Integral unshared data size
    pub ru_idrss: i64,
    /// Integral unshared stack size
    pub ru_isrss: i64,
    /// Page reclaims (soft page faults)
    pub ru_minflt: i64,
    /// Page faults (hard page faults)
    pub ru_majflt: i64,
    /// Swaps
    pub ru_nswap: i64,
    /// Block input operations
    pub ru_inblock: i64,
    /// Block output operations
    pub ru_oublock: i64,
    /// IPC messages sent
    pub ru_msgsnd: i64,
    /// IPC messages received
    pub ru_msgrcv: i64,
    /// Signals received
    pub ru_nsignals: i64,
    /// Voluntary context switches
    pub ru_nvcsw: i64,
    /// Involuntary context switches
    pub ru_nivcsw: i64,
}

impl RUsage {
    pub fn zero() -> Self {
        RUsage {
            ru_utime: TimeVal { tv_sec: 0, tv_usec: 0 },
            ru_stime: TimeVal { tv_sec: 0, tv_usec: 0 },
            ru_maxrss: 0,
            ru_ixrss: 0,
            ru_idrss: 0,
            ru_isrss: 0,
            ru_minflt: 0,
            ru_majflt: 0,
            ru_nswap: 0,
            ru_inblock: 0,
            ru_oublock: 0,
            ru_msgsnd: 0,
            ru_msgrcv: 0,
            ru_nsignals: 0,
            ru_nvcsw: 0,
            ru_nivcsw: 0,
        }
    }
}

// ============================================================================
// Resource Limit Operations
// ============================================================================

/// getrlimit - get resource limits
pub fn getrlimit(resource: i32, rlim: *mut RLimit) -> LinuxResult<i32> {
    inc_ops();

    if rlim.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Validate resource
    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Get actual resource limits from process
    // For now, return default limits
    unsafe {
        *rlim = match resource {
            rlimit_resource::RLIMIT_NOFILE => RLimit::new(1024, 4096),
            rlimit_resource::RLIMIT_NPROC => RLimit::new(4096, 16384),
            rlimit_resource::RLIMIT_STACK => RLimit::new(8 * 1024 * 1024, RLIM_INFINITY),
            rlimit_resource::RLIMIT_AS => RLimit::unlimited(),
            _ => RLimit::unlimited(),
        };
    }

    Ok(0)
}

/// setrlimit - set resource limits
pub fn setrlimit(resource: i32, rlim: *const RLimit) -> LinuxResult<i32> {
    inc_ops();

    if rlim.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Validate resource
    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    unsafe {
        let limit = *rlim;

        // Soft limit cannot exceed hard limit
        if limit.rlim_cur > limit.rlim_max {
            return Err(LinuxError::EINVAL);
        }

        // TODO: Set resource limits
        // Raising hard limit requires CAP_SYS_RESOURCE capability
    }

    Ok(0)
}

/// prlimit - get/set resource limits of arbitrary process
pub fn prlimit(
    pid: Pid,
    resource: i32,
    new_limit: *const RLimit,
    old_limit: *mut RLimit,
) -> LinuxResult<i32> {
    inc_ops();

    // Validate resource
    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    // pid == 0 means current process
    let target_pid = if pid == 0 { 1 } else { pid };

    if target_pid < 0 {
        return Err(LinuxError::EINVAL);
    }

    // Get old limit if requested
    if !old_limit.is_null() {
        // TODO: Get limits from target process
        unsafe {
            *old_limit = RLimit::unlimited();
        }
    }

    // Set new limit if provided
    if !new_limit.is_null() {
        unsafe {
            let limit = *new_limit;

            if limit.rlim_cur > limit.rlim_max {
                return Err(LinuxError::EINVAL);
            }

            // TODO: Set limits for target process
            // Requires appropriate permissions
        }
    }

    Ok(0)
}

// ============================================================================
// Priority Operations
// ============================================================================

/// getpriority - get program scheduling priority
pub fn getpriority(which: i32, _who: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    match which {
        PRIO_PROCESS | PRIO_PGRP | PRIO_USER => {
            // TODO: Get priority
            // Return priority value (0-39, where 20 is normal)
            Ok(20)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// setpriority - set program scheduling priority
pub fn setpriority(which: i32, _who: i32, prio: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    // Priority range is -20 to 19
    if prio < -20 || prio > 19 {
        return Err(LinuxError::EINVAL);
    }

    match which {
        PRIO_PROCESS | PRIO_PGRP | PRIO_USER => {
            // TODO: Set priority
            // Requires appropriate permissions for nice values < 0
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// nice - change process priority
pub fn nice(_inc: i32) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Adjust process priority by inc
    // Return new priority value
    Ok(20)
}

// ============================================================================
// Scheduler Operations
// ============================================================================

/// Scheduler policies
pub mod sched_policy {
    /// Standard round-robin time-sharing
    pub const SCHED_NORMAL: i32 = 0;
    /// First-in, first-out
    pub const SCHED_FIFO: i32 = 1;
    /// Round-robin
    pub const SCHED_RR: i32 = 2;
    /// Batch processing
    pub const SCHED_BATCH: i32 = 3;
    /// Very low priority background jobs
    pub const SCHED_IDLE: i32 = 5;
    /// Sporadic server
    pub const SCHED_DEADLINE: i32 = 6;
}

/// Scheduling parameters
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SchedParam {
    pub sched_priority: i32,
}

/// sched_setscheduler - set scheduling policy and parameters
pub fn sched_setscheduler(_pid: Pid, policy: i32, param: *const SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match policy {
        sched_policy::SCHED_NORMAL | sched_policy::SCHED_FIFO |
        sched_policy::SCHED_RR | sched_policy::SCHED_BATCH |
        sched_policy::SCHED_IDLE | sched_policy::SCHED_DEADLINE => {
            // TODO: Set scheduler policy
            // SCHED_FIFO and SCHED_RR require real-time permissions
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// sched_getscheduler - get scheduling policy
pub fn sched_getscheduler(_pid: Pid) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Get scheduler policy for process
    Ok(sched_policy::SCHED_NORMAL)
}

/// sched_setparam - set scheduling parameters
pub fn sched_setparam(_pid: Pid, param: *const SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Set scheduling parameters
    Ok(0)
}

/// sched_getparam - get scheduling parameters
pub fn sched_getparam(_pid: Pid, param: *mut SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Get scheduling parameters
    unsafe {
        (*param).sched_priority = 0;
    }
    Ok(0)
}

/// sched_get_priority_max - get maximum priority value
pub fn sched_get_priority_max(policy: i32) -> LinuxResult<i32> {
    inc_ops();

    match policy {
        sched_policy::SCHED_NORMAL | sched_policy::SCHED_BATCH | sched_policy::SCHED_IDLE => Ok(0),
        sched_policy::SCHED_FIFO | sched_policy::SCHED_RR => Ok(99),
        _ => Err(LinuxError::EINVAL),
    }
}

/// sched_get_priority_min - get minimum priority value
pub fn sched_get_priority_min(policy: i32) -> LinuxResult<i32> {
    inc_ops();

    match policy {
        sched_policy::SCHED_NORMAL | sched_policy::SCHED_BATCH | sched_policy::SCHED_IDLE => Ok(0),
        sched_policy::SCHED_FIFO | sched_policy::SCHED_RR => Ok(1),
        _ => Err(LinuxError::EINVAL),
    }
}

/// sched_rr_get_interval - get SCHED_RR interval
pub fn sched_rr_get_interval(_pid: Pid, tp: *mut TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if tp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Get actual RR interval
    // Default is typically 100ms
    unsafe {
        (*tp).tv_sec = 0;
        (*tp).tv_nsec = 100_000_000;
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getrlimit() {
        let mut rlim = RLimit::unlimited();
        assert!(getrlimit(rlimit_resource::RLIMIT_NOFILE, &mut rlim).is_ok());
        assert!(rlim.rlim_cur > 0);
    }

    #[test]
    fn test_setrlimit_validation() {
        // Soft limit cannot exceed hard limit
        let invalid = RLimit { rlim_cur: 1000, rlim_max: 500 };
        assert!(setrlimit(rlimit_resource::RLIMIT_NOFILE, &invalid).is_err());
    }

    #[test]
    fn test_priority() {
        assert!(getpriority(0, 0).is_ok());
        assert!(setpriority(0, 0, 10).is_ok());
        assert!(setpriority(0, 0, -30).is_err()); // Out of range
    }

    #[test]
    fn test_scheduler_policy() {
        assert_eq!(sched_get_priority_max(sched_policy::SCHED_FIFO).unwrap(), 99);
        assert_eq!(sched_get_priority_min(sched_policy::SCHED_FIFO).unwrap(), 1);
    }
}
