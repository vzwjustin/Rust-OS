//! Seccomp — Secure Computing Mode
//!
//! Ported from Linux kernel/seccomp.c.
//! Provides syscall filtering:
//! - Mode 1 (STRICT): only read/write/exit/sigreturn allowed
//! - Mode 2 (FILTER): user-defined BPF-like filters per syscall
//!
//! ## Filter model
//! Since we don't have a full BPF VM, filters are stored as a list of
//! syscall number → action rules. Each rule specifies an action:
//! ALLOW, KILL, TRAP (SIGSYS), ERRNO, TRACE, LOG.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

// ── Constants (from include/uapi/linux/seccomp.h) ───────────────────────

pub const SECCOMP_MODE_DISABLED: u32 = 0;
pub const SECCOMP_MODE_STRICT: u32 = 1;
pub const SECCOMP_MODE_FILTER: u32 = 2;

pub const SECCOMP_SET_MODE_STRICT: u32 = 0;
pub const SECCOMP_SET_MODE_FILTER: u32 = 1;
pub const SECCOMP_GET_ACTION_AVAIL: u32 = 2;
pub const SECCOMP_GET_NOTIF_SIZES: u32 = 3;

pub const SECCOMP_FILTER_FLAG_TSYNC: u32 = 1 << 0;
pub const SECCOMP_FILTER_FLAG_LOG: u32 = 1 << 1;
pub const SECCOMP_FILTER_FLAG_SPEC_ALLOW: u32 = 1 << 2;
pub const SECCOMP_FILTER_FLAG_NEW_LISTENER: u32 = 1 << 3;

// Return actions (upper 16 bits)
pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x80000000;
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x00000000;
pub const SECCOMP_RET_TRAP: u32 = 0x00030000;
pub const SECCOMP_RET_ERRNO: u32 = 0x00050000;
pub const SECCOMP_RET_USER_NOTIF: u32 = 0x7fc00000;
pub const SECCOMP_RET_TRACE: u32 = 0x7ff00000;
pub const SECCOMP_RET_LOG: u32 = 0x7ffc0000;
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;

pub const SECCOMP_RET_ACTION_FULL: u32 = 0xffff0000;
pub const SECCOMP_RET_ACTION: u32 = 0x7fff0000;
pub const SECCOMP_RET_DATA: u32 = 0x0000ffff;

// ── Seccomp data (passed to filter evaluation) ──────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SeccompData {
    pub nr: i32,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub args: [u64; 6],
}

// ── Filter rules ────────────────────────────────────────────────────────

/// A single filter rule: if syscall matches, apply the action.
#[derive(Debug, Clone)]
pub struct SeccompRule {
    pub syscall_nr: i32,
    pub action: u32, // SECCOMP_RET_* value
}

/// A filter is a list of rules, evaluated in order. First match wins.
/// If no rule matches, the default action is taken.
#[derive(Debug, Clone)]
pub struct SeccompFilter {
    pub rules: Vec<SeccompRule>,
    pub default_action: u32,
    pub flags: u32,
    pub log: bool,
}

impl SeccompFilter {
    pub fn new(default_action: u32, flags: u32) -> Self {
        Self {
            rules: Vec::new(),
            default_action,
            flags,
            log: flags & SECCOMP_FILTER_FLAG_LOG != 0,
        }
    }

    pub fn add_rule(&mut self, syscall_nr: i32, action: u32) {
        self.rules.push(SeccompRule { syscall_nr, action });
    }

    /// Evaluate the filter against a syscall. Returns the action to take.
    pub fn evaluate(&self, data: &SeccompData) -> u32 {
        for rule in &self.rules {
            if rule.syscall_nr == data.nr {
                if self.log {
                    crate::serial_println!(
                        "[seccomp] syscall {} matched rule → action {:#x}",
                        data.nr,
                        rule.action
                    );
                }
                return rule.action;
            }
        }
        if self.log {
            crate::serial_println!(
                "[seccomp] syscall {} no match → default {:#x}",
                data.nr,
                self.default_action
            );
        }
        self.default_action
    }
}

// ── Per-process seccomp state ───────────────────────────────────────────

/// Seccomp state for a process.
#[derive(Clone)]
pub struct SeccompState {
    pub mode: u32,
    pub filter: Option<SeccompFilter>,
    pub filter_count: u32,
}

impl Default for SeccompState {
    fn default() -> Self {
        Self {
            mode: SECCOMP_MODE_DISABLED,
            filter: None,
            filter_count: 0,
        }
    }
}

impl SeccompState {
    pub fn is_active(&self) -> bool {
        self.mode != SECCOMP_MODE_DISABLED
    }
}

// ── Global state: PID → SeccompState ────────────────────────────────────

static SECCOMP_STATES: RwLock<BTreeMap<u32, SeccompState>> = RwLock::new(BTreeMap::new());
static SECCOMP_FILTER_COUNT: AtomicU64 = AtomicU64::new(0);

fn get_state(pid: u32) -> SeccompState {
    SECCOMP_STATES.read().get(&pid).cloned().unwrap_or_default()
}

fn set_state(pid: u32, state: SeccompState) {
    SECCOMP_STATES.write().insert(pid, state);
}

// ── Strict mode allowed syscalls ────────────────────────────────────────

const STRICT_ALLOWED: &[i32] = &[
    0,   // read
    1,   // write
    60,  // exit
    231, // exit_group
    35,  // sigreturn (rt_sigreturn)
];

fn is_strict_allowed(syscall_nr: i32) -> bool {
    STRICT_ALLOWED.contains(&syscall_nr)
}

// ── Public API ──────────────────────────────────────────────────────────

/// Set seccomp mode (called by seccomp() syscall).
pub fn seccomp_set_mode(op: u32, flags: u32, filter_data: *const u8) -> i32 {
    let pid = crate::process::current_pid();

    match op {
        SECCOMP_SET_MODE_STRICT => {
            let state = SeccompState {
                mode: SECCOMP_MODE_STRICT,
                filter: None,
                filter_count: 0,
            };
            set_state(pid, state);
            crate::serial_println!("[seccomp] pid {} set to STRICT mode", pid);
            0
        }

        SECCOMP_SET_MODE_FILTER => {
            if filter_data.is_null() {
                return -14; // EFAULT
            }

            // Read the BPF filter from userspace.
            // In Linux, this is a struct sock_fprog with an array of sock_filter.
            // We simplify: the filter_data points to a serialized list of rules.
            // Format: [default_action:u32] [nr_rules:u32] [rule: syscall_nr:i32, action:u32]*
            let filter = parse_filter(filter_data, flags);
            if filter.is_none() {
                return -22; // EINVAL
            }

            let filter = filter.unwrap();
            SECCOMP_FILTER_COUNT.fetch_add(1, Ordering::Relaxed);

            // Check if we already have a filter — if so, chain (prepend)
            let mut state = get_state(pid);
            if let Some(ref mut existing) = state.filter {
                // Chain: new filter runs first, then existing
                let mut combined = filter;
                combined.rules.extend(existing.rules.clone());
                state.filter = Some(combined);
            } else {
                state.filter = Some(filter);
            }
            state.mode = SECCOMP_MODE_FILTER;
            state.filter_count += 1;

            // TSYNC: synchronize all threads (simplified — we don't have threads yet)
            if flags & SECCOMP_FILTER_FLAG_TSYNC != 0 {
                // Would sync all threads here
            }

            set_state(pid, state);
            crate::serial_println!("[seccomp] pid {} set to FILTER mode", pid);
            0
        }

        SECCOMP_GET_ACTION_AVAIL => {
            // Check if an action is supported
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
                _ => -22, // EINVAL
            }
        }

        SECCOMP_GET_NOTIF_SIZES => {
            // Return sizes of notification structures
            if filter_data.is_null() {
                return -14;
            }
            // struct seccomp_notif_sizes { u16 notif; u16 resp; u16 data; }
            unsafe {
                let ptr = filter_data as *mut u16;
                *ptr = 80; // sizeof(seccomp_notif)
                *ptr.add(1) = 24; // sizeof(seccomp_notif_resp)
                *ptr.add(2) = 64; // sizeof(seccomp_data)
            }
            0
        }

        _ => -38, // ENOSYS
    }
}

/// Parse a BPF filter from userspace data.
/// Simplified format: [default_action:u32] [nr_rules:u32] [rules...]
fn parse_filter(data: *const u8, flags: u32) -> Option<SeccompFilter> {
    unsafe {
        let ptr = data as *const u32;
        let default_action = *ptr;
        let nr_rules = *ptr.add(1);

        if nr_rules > 256 {
            return None; // Sanity limit
        }

        let mut filter = SeccompFilter::new(default_action, flags);

        let rule_base = ptr.add(2) as *const (i32, u32);
        for i in 0..nr_rules {
            let (syscall_nr, action) = *rule_base.add(i as usize);
            filter.add_rule(syscall_nr, action);
        }

        Some(filter)
    }
}

/// Check if a syscall is allowed for the current process.
/// Called from the syscall dispatch path.
/// Returns Ok(()) if allowed, Err(errno) if blocked.
pub fn check_syscall(syscall_nr: i32, args: &[u64; 6]) -> Result<(), i32> {
    let pid = crate::process::current_pid();
    let state = get_state(pid);

    if !state.is_active() {
        return Ok(());
    }

    match state.mode {
        SECCOMP_MODE_STRICT => {
            if is_strict_allowed(syscall_nr) {
                Ok(())
            } else {
                crate::serial_println!(
                    "[seccomp] STRICT: killing pid {} for syscall {}",
                    pid,
                    syscall_nr
                );
                // Kill the process
                let pm = crate::process::get_process_manager();
                let _ = pm.terminate_process(pid, 9);
                Err(-31) // SIGSYS
            }
        }

        SECCOMP_MODE_FILTER => {
            if let Some(ref filter) = state.filter {
                let data = SeccompData {
                    nr: syscall_nr,
                    arch: 0xC000_003E, // AUDIT_ARCH_X86_64
                    instruction_pointer: 0,
                    args: *args,
                };

                let action = filter.evaluate(&data);
                let action_only = action & SECCOMP_RET_ACTION_FULL;

                match action_only {
                    SECCOMP_RET_ALLOW => Ok(()),
                    SECCOMP_RET_LOG => Ok(()),
                    SECCOMP_RET_ERRNO => {
                        let errno = (action & SECCOMP_RET_DATA) as i32;
                        Err(-errno)
                    }
                    SECCOMP_RET_TRAP => {
                        // Send SIGSYS to the process
                        Err(-31) // SIGSYS
                    }
                    SECCOMP_RET_KILL_PROCESS => {
                        let pm = crate::process::get_process_manager();
                        let _ = pm.terminate_process(pid, 9);
                        Err(-9)
                    }
                    SECCOMP_RET_KILL_THREAD => {
                        let pm = crate::process::get_process_manager();
                        let _ = pm.terminate_process(pid, 9);
                        Err(-9)
                    }
                    SECCOMP_RET_TRACE => {
                        // Pass to tracer if being traced, else KILL
                        Ok(()) // Simplified: allow
                    }
                    _ => Ok(()),
                }
            } else {
                Ok(())
            }
        }

        _ => Ok(()),
    }
}

/// Get the seccomp mode for a process.
pub fn get_mode(pid: u32) -> u32 {
    get_state(pid).mode
}

/// Check if a process has seccomp active.
pub fn is_active(pid: u32) -> bool {
    get_state(pid).is_active()
}

/// Clear seccomp state for a process (on exit).
pub fn clear(pid: u32) {
    SECCOMP_STATES.write().remove(&pid);
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[seccomp] seccomp subsystem initialized");
}

// ── Statistics ──────────────────────────────────────────────────────────

pub fn filter_count() -> u64 {
    SECCOMP_FILTER_COUNT.load(Ordering::Relaxed)
}
