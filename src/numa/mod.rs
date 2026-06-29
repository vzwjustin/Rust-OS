//! NUMA node registry and memory policy backend.
//!
//! Backs Linux `set_mempolicy`, `get_mempolicy`, and `mbind` syscalls via
//! per-task defaults and a range binding table.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::RwLock;

use crate::linux_compat::LinuxError;
use crate::linux_compat::LinuxResult;
use crate::process::Pid;

/// Memory policy modes (Linux MPOL_*).
pub const MPOL_DEFAULT: i32 = 0;
pub const MPOL_PREFERRED: i32 = 1;
pub const MPOL_BIND: i32 = 2;
pub const MPOL_INTERLEAVE: i32 = 3;
pub const MPOL_LOCAL: i32 = 4;

/// One NUMA node.
#[derive(Debug, Clone)]
pub struct NumaNode {
    pub id: u32,
    pub online: bool,
    pub mem_total_kb: u64,
    pub mem_free_kb: u64,
}

/// Per-address-range policy binding.
#[derive(Debug, Clone, Copy)]
struct RangeBinding {
    start: usize,
    end: usize,
    mode: i32,
    nodemask: u64,
}

/// Per-process default NUMA policy.
#[derive(Debug, Clone, Copy, Default)]
struct TaskPolicy {
    mode: i32,
    nodemask: u64,
}

static NODES: RwLock<Vec<NumaNode>> = RwLock::new(Vec::new());
static TASK_POLICIES: RwLock<BTreeMap<Pid, TaskPolicy>> = RwLock::new(BTreeMap::new());
static RANGE_BINDINGS: RwLock<Vec<RangeBinding>> = RwLock::new(Vec::new());

fn validate_mode(mode: i32) -> LinuxResult<()> {
    match mode {
        MPOL_DEFAULT | MPOL_PREFERRED | MPOL_BIND | MPOL_INTERLEAVE | MPOL_LOCAL => Ok(()),
        _ => Err(LinuxError::EINVAL),
    }
}

/// Number of registered NUMA nodes.
pub fn node_count() -> u32 {
    NODES.read().len() as u32
}

/// True if `node` exists and is online.
pub fn is_valid_node(node: u32) -> bool {
    NODES.read().iter().any(|n| n.id == node && n.online)
}

/// Validate nodemask against registered nodes.
pub fn validate_nodemask(mask: u64, maxnode: u64) -> LinuxResult<()> {
    if maxnode == 0 {
        return Err(LinuxError::EINVAL);
    }
    let max = maxnode.min(64);
    for node in 0..max {
        if mask & (1 << node) != 0 && !is_valid_node(node as u32) {
            return Err(LinuxError::EINVAL);
        }
    }
    Ok(())
}

/// Register a NUMA node.
pub fn register_node(node: NumaNode) {
    let mut nodes = NODES.write();
    if let Some(existing) = nodes.iter_mut().find(|n| n.id == node.id) {
        *existing = node;
    } else {
        nodes.push(node);
        nodes.sort_by_key(|n| n.id);
    }
}

/// Set default policy for a task.
pub fn set_task_policy(pid: Pid, mode: i32, nodemask: u64) -> LinuxResult<()> {
    validate_mode(mode)?;
    if mode != MPOL_DEFAULT && mode != MPOL_LOCAL {
        validate_nodemask(nodemask, 64)?;
    }
    TASK_POLICIES.write().insert(
        pid,
        TaskPolicy {
            mode,
            nodemask: if mode == MPOL_DEFAULT || mode == MPOL_LOCAL {
                1
            } else {
                nodemask
            },
        },
    );
    Ok(())
}

/// Get default policy for a task (falls back to MPOL_DEFAULT on node 0).
pub fn get_task_policy(pid: Pid) -> (i32, u64) {
    TASK_POLICIES
        .read()
        .get(&pid)
        .map(|p| (p.mode, p.nodemask))
        .unwrap_or((MPOL_DEFAULT, 0x1))
}

/// Bind a virtual address range to a policy.
pub fn bind_range(
    addr: *mut u8,
    len: usize,
    mode: i32,
    nodemask: u64,
    _flags: u32,
) -> LinuxResult<()> {
    validate_mode(mode)?;
    if mode != MPOL_DEFAULT && mode != MPOL_LOCAL {
        validate_nodemask(nodemask, 64)?;
    }

    let start = addr as usize;
    let end = start.saturating_add(len);
    let binding = RangeBinding {
        start,
        end,
        mode,
        nodemask: if mode == MPOL_DEFAULT || mode == MPOL_LOCAL {
            0x1
        } else {
            nodemask
        },
    };

    let mut ranges = RANGE_BINDINGS.write();
    ranges.retain(|r| r.end <= start || r.start >= end);
    ranges.push(binding);
    ranges.sort_by_key(|r| r.start);
    Ok(())
}

/// Resolve effective policy for an address (range override, else task default).
pub fn lookup_policy(pid: Pid, addr: usize) -> (i32, u64) {
    if let Some(r) = RANGE_BINDINGS
        .read()
        .iter()
        .find(|r| addr >= r.start && addr < r.end)
    {
        return (r.mode, r.nodemask);
    }
    get_task_policy(pid)
}

/// Preferred node for allocation under `mode`/`nodemask`.
pub fn select_node_for_alloc(mode: i32, nodemask: u64) -> u32 {
    match mode {
        MPOL_BIND | MPOL_PREFERRED => {
            for node in 0..64 {
                if nodemask & (1 << node) != 0 && is_valid_node(node as u32) {
                    return node as u32;
                }
            }
            0
        }
        MPOL_INTERLEAVE => {
            static NEXT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
            let nodes: Vec<u32> = (0..64u32)
                .filter(|n| nodemask & (1 << n) != 0 && is_valid_node(*n))
                .collect();
            if nodes.is_empty() {
                0
            } else {
                let idx = NEXT.fetch_add(1, core::sync::atomic::Ordering::Relaxed) as usize;
                nodes[idx % nodes.len()]
            }
        }
        _ => 0,
    }
}

/// Initialize single-node NUMA (node 0) from available physical memory.
pub fn init() {
    let mem_kb = crate::memory::get_memory_manager()
        .map(|m| m.memory_stats().total_memory as u64 / 1024)
        .unwrap_or(512 * 1024);
    register_node(NumaNode {
        id: 0,
        online: true,
        mem_total_kb: mem_kb as u64,
        mem_free_kb: (mem_kb / 2) as u64,
    });
    crate::serial_println!("[numa] initialized ({} node(s))", node_count());
}
