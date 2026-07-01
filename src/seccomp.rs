//! Linux-compatible seccomp state and syscall filtering.
//!
//! This models the security property that matters for RustOS today: once a
//! restrictive filter is installed, later filters cannot loosen it. Linux
//! evaluates every installed filter and applies action precedence, with the
//! numerically smallest action class being the most restrictive.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

pub const SECCOMP_MODE_DISABLED: u32 = 0;
pub const SECCOMP_MODE_STRICT: u32 = 1;
pub const SECCOMP_MODE_FILTER: u32 = 2;

pub const SECCOMP_SET_MODE_STRICT: u32 = 0;
pub const SECCOMP_SET_MODE_FILTER: u32 = 1;
pub const SECCOMP_GET_ACTION_AVAIL: u32 = 2;

/// Linux `AUDIT_ARCH_X86_64` (EM_X86_64 | __AUDIT_ARCH_64BIT | __AUDIT_ARCH_LE),
/// used to populate `seccomp_data.arch` for x86_64 BPF filters.
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;
pub const SECCOMP_GET_NOTIF_SIZES: u32 = 3;

pub const SECCOMP_FILTER_FLAG_TSYNC: u32 = 1 << 0;
pub const SECCOMP_FILTER_FLAG_LOG: u32 = 1 << 1;
pub const SECCOMP_FILTER_FLAG_SPEC_ALLOW: u32 = 1 << 2;
pub const SECCOMP_FILTER_FLAG_NEW_LISTENER: u32 = 1 << 3;
pub const SECCOMP_FILTER_FLAG_TSYNC_ESRCH: u32 = 1 << 4;
pub const SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV: u32 = 1 << 5;
const SECCOMP_FILTER_FLAG_MASK: u32 = SECCOMP_FILTER_FLAG_TSYNC
    | SECCOMP_FILTER_FLAG_LOG
    | SECCOMP_FILTER_FLAG_SPEC_ALLOW
    | SECCOMP_FILTER_FLAG_NEW_LISTENER
    | SECCOMP_FILTER_FLAG_TSYNC_ESRCH
    | SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV;
const SECCOMP_FILTER_UNSUPPORTED_FLAGS: u32 = SECCOMP_FILTER_FLAG_TSYNC
    | SECCOMP_FILTER_FLAG_NEW_LISTENER
    | SECCOMP_FILTER_FLAG_TSYNC_ESRCH
    | SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV;

pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x0000_0000;
pub const SECCOMP_RET_TRAP: u32 = 0x0003_0000;
pub const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
pub const SECCOMP_RET_USER_NOTIF: u32 = 0x7fc0_0000;
pub const SECCOMP_RET_TRACE: u32 = 0x7ff0_0000;
pub const SECCOMP_RET_LOG: u32 = 0x7ffc_0000;
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
pub const SECCOMP_RET_ACTION_FULL: u32 = 0xffff_0000;
pub const SECCOMP_RET_ACTION: u32 = 0x7fff_0000;
pub const SECCOMP_RET_DATA: u32 = 0x0000_ffff;

#[derive(Debug, Clone, Copy)]
pub struct SeccompData {
    pub nr: i32,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub args: [u64; 6],
}

#[derive(Debug, Clone)]
pub struct SeccompRule {
    pub syscall_nr: i32,
    pub action: u32,
}

#[derive(Debug, Clone)]
pub struct SeccompFilter {
    rules: Vec<SeccompRule>,
    default_action: u32,
    flags: u32,
}

impl SeccompFilter {
    pub fn new(default_action: u32, flags: u32) -> Self {
        Self {
            rules: Vec::new(),
            default_action,
            flags,
        }
    }

    pub fn add_rule(&mut self, syscall_nr: i32, action: u32) {
        self.rules.push(SeccompRule { syscall_nr, action });
    }

    pub fn evaluate(&self, data: &SeccompData) -> u32 {
        let mut action = self.default_action;

        for rule in &self.rules {
            if rule.syscall_nr == data.nr {
                action = most_restrictive(action, rule.action);
            }
        }

        if self.flags & SECCOMP_FILTER_FLAG_LOG != 0 && action != SECCOMP_RET_ALLOW {
            crate::println!("[seccomp] syscall {} action {:#x}", data.nr, action);
        }

        action
    }
}

#[derive(Debug, Clone)]
pub struct SeccompState {
    pub mode: u32,
    pub filters: Vec<SeccompFilter>,
    pub filter_count: u32,
}

impl Default for SeccompState {
    fn default() -> Self {
        Self {
            mode: SECCOMP_MODE_DISABLED,
            filters: Vec::new(),
            filter_count: 0,
        }
    }
}

impl SeccompState {
    pub fn is_active(&self) -> bool {
        self.mode != SECCOMP_MODE_DISABLED
    }

    fn evaluate(&self, data: &SeccompData) -> u32 {
        let mut action = SECCOMP_RET_ALLOW;

        for filter in &self.filters {
            action = most_restrictive(action, filter.evaluate(data));
        }

        action
    }
}

static SECCOMP_STATES: RwLock<BTreeMap<u32, SeccompState>> = RwLock::new(BTreeMap::new());
static SECCOMP_FILTER_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn get_state(pid: u32) -> SeccompState {
    SECCOMP_STATES.read().get(&pid).cloned().unwrap_or_default()
}

pub fn set_state(pid: u32, state: SeccompState) {
    SECCOMP_STATES.write().insert(pid, state);
}

fn most_restrictive(left: u32, right: u32) -> u32 {
    let left_action = left & SECCOMP_RET_ACTION_FULL;
    let right_action = right & SECCOMP_RET_ACTION_FULL;

    if right_action < left_action {
        right
    } else {
        left
    }
}

fn strict_allows(syscall_nr: i32) -> bool {
    const STRICT_ALLOWED: [i32; 5] = [0, 1, 60, 231, 35];
    STRICT_ALLOWED.contains(&syscall_nr)
}

pub fn sys_seccomp(pid: u32, op: u32, flags: u32, filter_data: *const u8) -> i32 {
    match op {
        SECCOMP_SET_MODE_STRICT => {
            if flags != 0 || !filter_data.is_null() {
                return -22;
            }

            let state = SeccompState {
                mode: SECCOMP_MODE_STRICT,
                filters: Vec::new(),
                filter_count: 0,
            };
            set_state(pid, state);
            0
        }
        SECCOMP_SET_MODE_FILTER => {
            if flags & !SECCOMP_FILTER_FLAG_MASK != 0 {
                return -22;
            }
            if (flags & SECCOMP_FILTER_FLAG_TSYNC) != 0
                && (flags & SECCOMP_FILTER_FLAG_NEW_LISTENER) != 0
                && (flags & SECCOMP_FILTER_FLAG_TSYNC_ESRCH) == 0
            {
                return -22;
            }
            if (flags & SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV) != 0
                && (flags & SECCOMP_FILTER_FLAG_NEW_LISTENER) == 0
            {
                return -22;
            }
            if flags & SECCOMP_FILTER_UNSUPPORTED_FLAGS != 0 {
                return -95;
            }

            if filter_data.is_null() {
                return -14;
            }

            let Some(filter) = load_filter_from_user(filter_data, flags) else {
                return -22;
            };

            let mut state = get_state(pid);
            state.mode = SECCOMP_MODE_FILTER;
            state.filters.push(filter);
            state.filter_count = state.filters.len() as u32;
            set_state(pid, state);
            SECCOMP_FILTER_COUNT.fetch_add(1, Ordering::Relaxed);

            if flags & SECCOMP_FILTER_FLAG_TSYNC != 0 {
                // RustOS does not yet have thread-group-wide seccomp state to sync.
            }

            0
        }
        SECCOMP_GET_ACTION_AVAIL => {
            if flags != 0 {
                return -22;
            }
            if filter_data.is_null() {
                return -14;
            }

            let action = unsafe { *(filter_data as *const u32) };
            match action {
                SECCOMP_RET_KILL_PROCESS
                | SECCOMP_RET_KILL_THREAD
                | SECCOMP_RET_TRAP
                | SECCOMP_RET_ERRNO
                | SECCOMP_RET_LOG
                | SECCOMP_RET_ALLOW => 0,
                _ => -22,
            }
        }
        SECCOMP_GET_NOTIF_SIZES => {
            if flags != 0 {
                return -22;
            }
            if filter_data.is_null() {
                return -14;
            }

            unsafe {
                let ptr = filter_data as *mut u16;
                *ptr = 80;
                *ptr.add(1) = 24;
                *ptr.add(2) = 64;
            }
            0
        }
        _ => -38,
    }
}

pub fn seccomp_set_mode(pid: u32, op: u32, flags: u32, filter_data: u64) -> i32 {
    sys_seccomp(pid, op, flags, filter_data as *const u8)
}

fn load_filter_from_user(ptr: *const u8, flags: u32) -> Option<SeccompFilter> {
    use crate::memory::user_space::UserSpaceMemory;

    let base = ptr as u64;
    let mut header = [0u8; 8];
    UserSpaceMemory::copy_from_user(base, &mut header).ok()?;

    let default_action = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
    let nr_rules = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
    if nr_rules > 256 {
        return None;
    }

    let mut filter = SeccompFilter::new(default_action, flags);
    let rules_len = (nr_rules as usize).checked_mul(8)?;
    let rules_addr = base.checked_add(8)?;
    let mut rules = alloc::vec![0u8; rules_len];
    UserSpaceMemory::copy_from_user(rules_addr, &mut rules).ok()?;

    for idx in 0..nr_rules as usize {
        let off = idx.checked_mul(8)?;
        let syscall_nr =
            i32::from_ne_bytes([rules[off], rules[off + 1], rules[off + 2], rules[off + 3]]);
        let action = u32::from_ne_bytes([
            rules[off + 4],
            rules[off + 5],
            rules[off + 6],
            rules[off + 7],
        ]);
        filter.add_rule(syscall_nr, action);
    }

    Some(filter)
}

pub fn check_syscall(pid: u32, syscall_nr: i32, args: &[u64; 6]) -> Result<(), i32> {
    let state = get_state(pid);
    if !state.is_active() {
        return Ok(());
    }

    if state.mode == SECCOMP_MODE_STRICT {
        if strict_allows(syscall_nr) {
            return Ok(());
        }
        let pm = crate::process::get_process_manager();
        let _ = pm.terminate_process(pid, 9);
        return Err(-31);
    }

    if state.mode == SECCOMP_MODE_FILTER {
        let data = SeccompData {
            nr: syscall_nr,
            arch: AUDIT_ARCH_X86_64,
            instruction_pointer: 0,
            args: *args,
        };

        let action = state.evaluate(&data);
        match action & SECCOMP_RET_ACTION_FULL {
            SECCOMP_RET_ALLOW | SECCOMP_RET_LOG => Ok(()),
            SECCOMP_RET_ERRNO => {
                let errno = (action & SECCOMP_RET_DATA) as i32;
                Err(-errno)
            }
            SECCOMP_RET_TRAP => Err(-31),
            SECCOMP_RET_KILL_PROCESS | SECCOMP_RET_KILL_THREAD => {
                let pm = crate::process::get_process_manager();
                let _ = pm.terminate_process(pid, 9);
                Err(-9)
            }
            SECCOMP_RET_TRACE | SECCOMP_RET_USER_NOTIF => Err(-38),
            _ => Err(-9),
        }
    } else {
        Ok(())
    }
}

/// Subsystem init hook (matches Linux's per-subsystem `init()` boot pattern).
/// Seccomp state is created lazily per-pid in [`SECCOMP_STATES`], so there is
/// nothing to set up eagerly here.
pub fn init() {}

pub fn get_mode(pid: u32) -> u32 {
    get_state(pid).mode
}

pub fn clear_state(pid: u32) {
    SECCOMP_STATES.write().remove(&pid);
}

pub fn filter_count() -> u64 {
    SECCOMP_FILTER_COUNT.load(Ordering::Relaxed)
}
