//! Resource limit operations
//!
//! This module implements Linux resource limit operations including
//! getrlimit, setrlimit, prlimit, and resource usage tracking.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process::{self, Priority, ResourceLimit};

/// Operation counter for statistics
static RESOURCE_OPS_COUNT: AtomicU64 = AtomicU64::new(0);
static IO_PRIORITIES: spin::RwLock<BTreeMap<i32, i32>> = spin::RwLock::new(BTreeMap::new());

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

fn copy_struct_to_user<T: Copy>(dst: *mut T, value: &T) -> LinuxResult<()> {
    super::copy_struct_to_user(dst, value)
}

fn copy_struct_from_user<T: Copy>(src: *const T) -> LinuxResult<T> {
    super::copy_struct_from_user(src)
}

fn current_pid() -> u32 {
    process::current_pid()
}

fn resolve_pid(pid: Pid) -> LinuxResult<u32> {
    if pid < 0 {
        return Err(LinuxError::EINVAL);
    }
    Ok(if pid == 0 { current_pid() } else { pid as u32 })
}

fn current_euid() -> u32 {
    process::get_process_manager()
        .get_process(current_pid())
        .map(|pcb| pcb.euid)
        .unwrap_or(0)
}

fn pcb_rlimit_to_api(limit: ResourceLimit) -> RLimit {
    RLimit {
        rlim_cur: limit.rlim_cur,
        rlim_max: limit.rlim_max,
    }
}

fn validate_rlimit(limit: &RLimit) -> LinuxResult<()> {
    if limit.rlim_cur > limit.rlim_max {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

fn priority_to_nice(priority: Priority) -> i32 {
    match priority {
        Priority::RealTime => -20,
        Priority::High => -10,
        Priority::Normal => 0,
        Priority::Low => 10,
        Priority::Idle => 19,
    }
}

fn nice_to_priority(nice: i32) -> Priority {
    match nice {
        p if p <= -15 => Priority::RealTime,
        p if p <= -5 => Priority::High,
        p if p <= 5 => Priority::Normal,
        p if p <= 15 => Priority::Low,
        _ => Priority::Idle,
    }
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
            ru_utime: TimeVal {
                tv_sec: 0,
                tv_usec: 0,
            },
            ru_stime: TimeVal {
                tv_sec: 0,
                tv_usec: 0,
            },
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

    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    let limit = process::get_process_manager()
        .get_process(current_pid())
        .map(|pcb| pcb_rlimit_to_api(pcb.rlimits.limits[resource as usize]))
        .ok_or(LinuxError::ESRCH)?;

    copy_struct_to_user(rlim, &limit)?;

    Ok(0)
}

/// setrlimit - set resource limits
pub fn setrlimit(resource: i32, rlim: *const RLimit) -> LinuxResult<i32> {
    inc_ops();

    if rlim.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    let limit: RLimit = copy_struct_from_user(rlim)?;
    validate_rlimit(&limit)?;

    let pid = current_pid();
    let is_root = current_euid() == 0;

    process::get_process_manager()
        .with_process_mut(pid, |pcb| {
            let current = pcb.rlimits.limits[resource as usize];
            let new_max = if limit.rlim_max > current.rlim_max && !is_root {
                return Err(LinuxError::EPERM);
            } else {
                limit.rlim_max
            };
            if limit.rlim_cur > new_max {
                return Err(LinuxError::EINVAL);
            }
            pcb.rlimits.limits[resource as usize] = ResourceLimit {
                rlim_cur: limit.rlim_cur,
                rlim_max: new_max,
            };
            Ok(0)
        })
        .ok_or(LinuxError::ESRCH)?
}

/// prlimit - get/set resource limits of arbitrary process
pub fn prlimit(
    pid: Pid,
    resource: i32,
    new_limit: *const RLimit,
    old_limit: *mut RLimit,
) -> LinuxResult<i32> {
    inc_ops();

    if resource < 0 || resource > rlimit_resource::RLIMIT_RTTIME {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = resolve_pid(pid)?;
    let caller_pid = current_pid();
    let is_root = current_euid() == 0;

    if target_pid != caller_pid && !is_root {
        return Err(LinuxError::EPERM);
    }

    if !old_limit.is_null() {
        let old = process::get_process_manager()
            .get_process(target_pid)
            .map(|pcb| pcb_rlimit_to_api(pcb.rlimits.limits[resource as usize]))
            .ok_or(LinuxError::ESRCH)?;
        copy_struct_to_user(old_limit, &old)?;
    }

    if !new_limit.is_null() {
        let limit: RLimit = copy_struct_from_user(new_limit)?;
        validate_rlimit(&limit)?;

        process::get_process_manager()
            .with_process_mut(target_pid, |pcb| {
                let current = pcb.rlimits.limits[resource as usize];
                let new_max = if limit.rlim_max > current.rlim_max && !is_root {
                    return Err(LinuxError::EPERM);
                } else {
                    limit.rlim_max
                };
                if limit.rlim_cur > new_max {
                    return Err(LinuxError::EINVAL);
                }
                pcb.rlimits.limits[resource as usize] = ResourceLimit {
                    rlim_cur: limit.rlim_cur,
                    rlim_max: new_max,
                };
                Ok(())
            })
            .ok_or(LinuxError::ESRCH)??;
    }

    Ok(0)
}

// ============================================================================
// Priority Operations
// ============================================================================

/// getpriority - get program scheduling priority
pub fn getpriority(which: i32, who: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 { current_pid() } else { who as u32 };
            process::scheduler::get_process_priority(target_pid)
                .map(priority_to_nice)
                .ok_or(LinuxError::ESRCH)
        }
        PRIO_PGRP => {
            let pgid = if who == 0 {
                process::get_process_manager()
                    .get_process(current_pid())
                    .map(|pcb| pcb.pgid)
                    .ok_or(LinuxError::ESRCH)?
            } else {
                who as u32
            };
            process::get_process_manager()
                .find_processes(|pcb| pcb.pgid == pgid)
                .into_iter()
                .map(|pcb| priority_to_nice(pcb.priority))
                .min()
                .ok_or(LinuxError::ESRCH)
        }
        PRIO_USER => {
            let uid = if who == 0 { current_euid() } else { who as u32 };
            process::get_process_manager()
                .find_processes(|pcb| pcb.uid == uid)
                .into_iter()
                .map(|pcb| priority_to_nice(pcb.priority))
                .min()
                .ok_or(LinuxError::ESRCH)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// setpriority - set program scheduling priority
pub fn setpriority(which: i32, who: i32, prio: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    if prio < -20 || prio > 19 {
        return Err(LinuxError::EINVAL);
    }

    let is_root = current_euid() == 0;
    if !is_root && prio < 0 {
        return Err(LinuxError::EACCES);
    }

    let priority = nice_to_priority(prio);

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 { current_pid() } else { who as u32 };
            process::scheduler::set_process_priority(target_pid, priority)
                .map_err(|_| LinuxError::ESRCH)?;
            Ok(0)
        }
        PRIO_PGRP => {
            let pgid = if who == 0 {
                process::get_process_manager()
                    .get_process(current_pid())
                    .map(|pcb| pcb.pgid)
                    .ok_or(LinuxError::ESRCH)?
            } else {
                who as u32
            };
            let pids: alloc::vec::Vec<u32> = process::get_process_manager()
                .find_processes(|pcb| pcb.pgid == pgid)
                .into_iter()
                .map(|pcb| pcb.pid)
                .collect();
            if pids.is_empty() {
                return Err(LinuxError::ESRCH);
            }
            for pid in pids {
                process::scheduler::set_process_priority(pid, priority)
                    .map_err(|_| LinuxError::ESRCH)?;
            }
            Ok(0)
        }
        PRIO_USER => {
            let uid = if who == 0 { current_euid() } else { who as u32 };
            let pids: alloc::vec::Vec<u32> = process::get_process_manager()
                .find_processes(|pcb| pcb.uid == uid)
                .into_iter()
                .map(|pcb| pcb.pid)
                .collect();
            if pids.is_empty() {
                return Err(LinuxError::ESRCH);
            }
            for pid in pids {
                process::scheduler::set_process_priority(pid, priority)
                    .map_err(|_| LinuxError::ESRCH)?;
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// nice - change process priority
pub fn nice(inc: i32) -> LinuxResult<i32> {
    inc_ops();

    let current_nice = getpriority(0, 0)?;
    let new_nice = (current_nice + inc).clamp(-20, 19);
    setpriority(0, 0, new_nice)?;
    Ok(new_nice)
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

fn resolve_sched_pid(pid: Pid) -> LinuxResult<u32> {
    if pid < 0 {
        return Err(LinuxError::ESRCH);
    }
    Ok(if pid == 0 { current_pid() } else { pid as u32 })
}

fn validate_sched_param(policy: i32, param: &SchedParam) -> LinuxResult<()> {
    match policy {
        sched_policy::SCHED_FIFO | sched_policy::SCHED_RR => {
            if param.sched_priority < 1 || param.sched_priority > 99 {
                return Err(LinuxError::EINVAL);
            }
        }
        sched_policy::SCHED_NORMAL
        | sched_policy::SCHED_BATCH
        | sched_policy::SCHED_IDLE
        | sched_policy::SCHED_DEADLINE => {
            if param.sched_priority != 0 {
                return Err(LinuxError::EINVAL);
            }
        }
        _ => return Err(LinuxError::EINVAL),
    }
    Ok(())
}

/// sched_setscheduler - set scheduling policy and parameters
pub fn sched_setscheduler(pid: Pid, policy: i32, param: *const SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match policy {
        sched_policy::SCHED_NORMAL
        | sched_policy::SCHED_FIFO
        | sched_policy::SCHED_RR
        | sched_policy::SCHED_BATCH
        | sched_policy::SCHED_IDLE
        | sched_policy::SCHED_DEADLINE => {}
        _ => return Err(LinuxError::EINVAL),
    }

    let sched_param: SchedParam = copy_struct_from_user(param)?;
    validate_sched_param(policy, &sched_param)?;

    let target_pid = resolve_sched_pid(pid)?;
    let caller_pid = current_pid();
    let is_root = current_euid() == 0;

    if target_pid != caller_pid && !is_root {
        return Err(LinuxError::EPERM);
    }

    if (policy == sched_policy::SCHED_FIFO || policy == sched_policy::SCHED_RR) && !is_root {
        return Err(LinuxError::EPERM);
    }

    process::get_process_manager()
        .with_process_mut(target_pid, |pcb| {
            pcb.sched_info.sched_policy = policy;
            pcb.sched_info.sched_priority = sched_param.sched_priority;
            Ok(0)
        })
        .ok_or(LinuxError::ESRCH)?
}

/// sched_getscheduler - get scheduling policy
pub fn sched_getscheduler(pid: Pid) -> LinuxResult<i32> {
    inc_ops();

    let target_pid = resolve_sched_pid(pid)?;

    process::get_process_manager()
        .get_process(target_pid)
        .map(|pcb| pcb.sched_info.sched_policy)
        .ok_or(LinuxError::ESRCH)
}

/// sched_setparam - set scheduling parameters
pub fn sched_setparam(pid: Pid, param: *const SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target_pid = resolve_sched_pid(pid)?;
    let sched_param: SchedParam = copy_struct_from_user(param)?;

    process::get_process_manager()
        .with_process_mut(target_pid, |pcb| {
            validate_sched_param(pcb.sched_info.sched_policy, &sched_param)?;
            pcb.sched_info.sched_priority = sched_param.sched_priority;
            Ok(0)
        })
        .ok_or(LinuxError::ESRCH)?
}

/// sched_getparam - get scheduling parameters
pub fn sched_getparam(pid: Pid, param: *mut SchedParam) -> LinuxResult<i32> {
    inc_ops();

    if param.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target_pid = resolve_sched_pid(pid)?;

    let priority = process::get_process_manager()
        .get_process(target_pid)
        .map(|pcb| pcb.sched_info.sched_priority)
        .ok_or(LinuxError::ESRCH)?;

    copy_struct_to_user(
        param,
        &SchedParam {
            sched_priority: priority,
        },
    )?;
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
pub fn sched_rr_get_interval(pid: Pid, tp: *mut TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if tp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target_pid = resolve_sched_pid(pid)?;

    let interval_ns = process::get_process_manager()
        .get_process(target_pid)
        .map(|pcb| pcb.sched_info.rr_interval_ns)
        .ok_or(LinuxError::ESRCH)?;

    let interval = TimeSpec {
        tv_sec: (interval_ns / 1_000_000_000) as i64,
        tv_nsec: (interval_ns % 1_000_000_000) as i64,
    };
    copy_struct_to_user(tp, &interval)?;

    Ok(0)
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_getrlimit() {
        let mut rlim = RLimit::unlimited();
        assert!(getrlimit(rlimit_resource::RLIMIT_NOFILE, &mut rlim).is_ok());
        assert!(rlim.rlim_cur > 0);
    }

    #[test_case]
    fn test_setrlimit_validation() {
        let invalid = RLimit {
            rlim_cur: 1000,
            rlim_max: 500,
        };
        assert!(setrlimit(rlimit_resource::RLIMIT_NOFILE, &invalid).is_err());
    }

    #[test_case]
    fn test_priority() {
        assert!(getpriority(0, 0).is_ok());
        assert!(setpriority(0, 0, 10).is_ok());
        assert!(setpriority(0, 0, -30).is_err());
    }

    #[test_case]
    fn test_scheduler_policy() {
        assert_eq!(
            sched_get_priority_max(sched_policy::SCHED_FIFO).unwrap(),
            99
        );
        assert_eq!(sched_get_priority_min(sched_policy::SCHED_FIFO).unwrap(), 1);
    }
}

pub fn ioprio_set(which: i32, who: i32, ioprio: i32) -> LinuxResult<i32> {
    inc_ops();
    if which < 1 || which > 3 {
        return Err(LinuxError::EINVAL);
    }
    let key = if who == 0 {
        crate::process::current_pid() as i32
    } else {
        who
    };
    IO_PRIORITIES.write().insert(key, ioprio);
    Ok(0)
}

pub fn ioprio_get(which: i32, who: i32) -> LinuxResult<i32> {
    inc_ops();
    if which < 1 || which > 3 {
        return Err(LinuxError::EINVAL);
    }
    let key = if who == 0 {
        crate::process::current_pid() as i32
    } else {
        who
    };
    Ok(*IO_PRIORITIES.read().get(&key).unwrap_or(&0))
}
