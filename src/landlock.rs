//! Landlock — unprivileged sandboxing via access control rulesets
//!
//! Ported from Linux security/landlock/ (ruleset.c, syscalls.c, setup.c).
//! Provides:
//! - landlock_create_ruleset(): create a ruleset with handled access rights
//! - landlock_add_rule(): add a rule (path beneath or net port) to a ruleset
//! - landlock_restrict_self(): enforce a ruleset on the calling process
//!
//! Landlock allows unprivileged processes to restrict their own access to
//! filesystem paths and network ports. Rules are additive — a process can
//! only further restrict itself, never broaden access.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── ABI version ─────────────────────────────────────────────────────────

pub const LANDLOCK_ABI_VERSION: u32 = 6;

// ── Create ruleset flags ────────────────────────────────────────────────

pub const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;

// ── Add rule flags ──────────────────────────────────────────────────────

pub const LANDLOCK_ADD_RULE_QUIET: u32 = 1 << 0;

// ── Restrict self flags ─────────────────────────────────────────────────

pub const LANDLOCK_RESTRICT_SELF_LOG_SAME_EXEC_OFF: u32 = 1 << 0;
pub const LANDLOCK_RESTRICT_SELF_LOG_NEW_EXEC_ON: u32 = 1 << 1;
pub const LANDLOCK_RESTRICT_SELF_LOG_SUBDOMAINS_OFF: u32 = 1 << 2;
pub const LANDLOCK_RESTRICT_SELF_TSYNC: u32 = 1 << 3;

// ── Rule types ──────────────────────────────────────────────────────────

pub const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
pub const LANDLOCK_RULE_NET_PORT: u32 = 2;

// ── Filesystem access rights ────────────────────────────────────────────

pub const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
pub const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
pub const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
pub const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
pub const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
pub const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
pub const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
pub const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
pub const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
pub const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
pub const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
pub const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
pub const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
pub const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13;
pub const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;
pub const LANDLOCK_ACCESS_FS_IOCTL_DEV: u64 = 1 << 15;

// ── Network access rights ───────────────────────────────────────────────

pub const LANDLOCK_ACCESS_NET_BIND_TCP: u64 = 1 << 0;
pub const LANDLOCK_ACCESS_NET_CONNECT_TCP: u64 = 1 << 1;
pub const LANDLOCK_ACCESS_NET_BIND_UDP: u64 = 1 << 2;
pub const LANDLOCK_ACCESS_NET_CONNECT_UDP: u64 = 1 << 3;

// ── Scope flags ─────────────────────────────────────────────────────────

pub const LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
pub const LANDLOCK_SCOPE_SIGNAL: u64 = 1 << 1;

// ── Data structures ─────────────────────────────────────────────────────

/// Ruleset attributes (matches Linux struct landlock_ruleset_attr)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LandlockRulesetAttr {
    pub handled_access_fs: u64,
    pub handled_access_net: u64,
    pub scoped: u64,
    pub quiet_access_fs: u64,
    pub quiet_access_net: u64,
    pub quiet_scoped: u64,
}

/// Path beneath rule (matches Linux struct landlock_path_beneath_attr)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LandlockPathBeneathAttr {
    pub allowed_access: u64,
    pub parent_fd: i32,
}

/// Network port rule (matches Linux struct landlock_net_port_attr)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LandlockNetPortAttr {
    pub allowed_access: u64,
    pub port: u64,
}

// ── Ruleset state ───────────────────────────────────────────────────────

/// A single rule within a ruleset
#[derive(Clone, Debug)]
pub enum LandlockRule {
    PathBeneath {
        allowed_access: u64,
        path: String,
        quiet: bool,
    },
    NetPort {
        allowed_access: u64,
        port: u64,
        quiet: bool,
    },
}

/// A Landlock ruleset
pub struct LandlockRuleset {
    pub id: u32,
    pub handled_access_fs: u64,
    pub handled_access_net: u64,
    pub scoped: u64,
    pub rules: Vec<LandlockRule>,
    pub enforced: bool,
}

impl LandlockRuleset {
    fn new(id: u32, attr: &LandlockRulesetAttr) -> Self {
        Self {
            id,
            handled_access_fs: attr.handled_access_fs,
            handled_access_net: attr.handled_access_net,
            scoped: attr.scoped,
            rules: Vec::new(),
            enforced: false,
        }
    }
}

// ── Per-process Landlock state ──────────────────────────────────────────

/// Landlock state attached to a process
pub struct ProcessLandlockState {
    pub enforced_rulesets: Vec<u32>,
    pub no_new_privs: bool,
}

// ── Global state ────────────────────────────────────────────────────────

static RULESETS: RwLock<BTreeMap<u32, Mutex<LandlockRuleset>>> = RwLock::new(BTreeMap::new());
static PROCESS_STATES: RwLock<BTreeMap<u32, Mutex<ProcessLandlockState>>> =
    RwLock::new(BTreeMap::new());
static NEXT_RULESET_ID: AtomicU32 = AtomicU32::new(1);

// ── Syscall implementations ─────────────────────────────────────────────

/// landlock_create_ruleset — create a new ruleset
///
/// `attr` is a pointer to landlock_ruleset_attr.
/// `size` is the size of the attr struct.
/// `flags` controls behavior (LANDLOCK_CREATE_RULESET_VERSION).
///
/// Returns a ruleset fd on success, negative errno on failure.
pub fn landlock_create_ruleset(attr: *const LandlockRulesetAttr, size: usize, flags: u32) -> i32 {
    // If VERSION flag is set, return the ABI version
    if flags & LANDLOCK_CREATE_RULESET_VERSION != 0 {
        return LANDLOCK_ABI_VERSION as i32;
    }

    if attr.is_null() || size < core::mem::size_of::<LandlockRulesetAttr>() {
        return -22; // EINVAL
    }

    if flags & !LANDLOCK_CREATE_RULESET_VERSION != 0 {
        return -22;
    }

    let attr_val = unsafe { *attr };

    // Validate access masks
    let valid_fs_mask = LANDLOCK_ACCESS_FS_EXECUTE
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_READ_FILE
        | LANDLOCK_ACCESS_FS_READ_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_CHAR
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_MAKE_REG
        | LANDLOCK_ACCESS_FS_MAKE_SOCK
        | LANDLOCK_ACCESS_FS_MAKE_FIFO
        | LANDLOCK_ACCESS_FS_MAKE_BLOCK
        | LANDLOCK_ACCESS_FS_MAKE_SYM
        | LANDLOCK_ACCESS_FS_REFER
        | LANDLOCK_ACCESS_FS_TRUNCATE
        | LANDLOCK_ACCESS_FS_IOCTL_DEV;

    let valid_net_mask = LANDLOCK_ACCESS_NET_BIND_TCP
        | LANDLOCK_ACCESS_NET_CONNECT_TCP
        | LANDLOCK_ACCESS_NET_BIND_UDP
        | LANDLOCK_ACCESS_NET_CONNECT_UDP;

    let valid_scope_mask = LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | LANDLOCK_SCOPE_SIGNAL;

    if attr_val.handled_access_fs & !valid_fs_mask != 0
        || attr_val.handled_access_net & !valid_net_mask != 0
        || attr_val.scoped & !valid_scope_mask != 0
    {
        return -22;
    }

    let id = NEXT_RULESET_ID.fetch_add(1, Ordering::SeqCst);
    let ruleset = LandlockRuleset::new(id, &attr_val);
    RULESETS.write().insert(id, Mutex::new(ruleset));

    let fd =
        crate::linux_compat::special_fd::register_landlock_ruleset(id, crate::vfs::OpenFlags::RDWR);
    if fd < 0 {
        RULESETS.write().remove(&id);
        return -23; // ENFILE
    }

    crate::serial_println!(
        "[landlock] create_ruleset: fs={:#x} net={:#x} fd={}",
        attr_val.handled_access_fs,
        attr_val.handled_access_net,
        fd
    );
    fd
}

/// landlock_add_rule — add a rule to a ruleset
///
/// `ruleset_fd` is the ruleset fd.
/// `rule_type` is LANDLOCK_RULE_PATH_BENEATH or LANDLOCK_RULE_NET_PORT.
/// `rule_attr` is a pointer to the rule attribute struct.
/// `flags` controls behavior (LANDLOCK_ADD_RULE_QUIET).
///
/// Returns 0 on success, negative errno on failure.
pub fn landlock_add_rule(ruleset_fd: i32, rule_type: u32, rule_attr: *const u8, flags: u32) -> i32 {
    let id = match crate::linux_compat::special_fd::get_landlock_ruleset_id(ruleset_fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    if rule_attr.is_null() {
        return -14; // EFAULT
    }

    if flags & !LANDLOCK_ADD_RULE_QUIET != 0 {
        return -22;
    }

    let quiet = flags & LANDLOCK_ADD_RULE_QUIET != 0;

    let rulesets = RULESETS.read();
    let ruleset_mutex = match rulesets.get(&id) {
        Some(r) => r,
        None => return -9,
    };
    let mut ruleset = ruleset_mutex.lock();

    if ruleset.enforced {
        return -22; // EINVAL — can't modify enforced ruleset
    }

    match rule_type {
        LANDLOCK_RULE_PATH_BENEATH => {
            // SAFETY: rule_attr is a validated user pointer (checked for null above); LandlockPathBeneathAttr is repr(C) Copy.
            let attr = unsafe { *(rule_attr as *const LandlockPathBeneathAttr) };
            let allowed = attr.allowed_access;
            let parent_fd = attr.parent_fd;

            // Validate allowed_access is a subset of handled_access_fs
            if allowed & !ruleset.handled_access_fs != 0 {
                return -22;
            }

            // Resolve parent_fd to a real filesystem path via the VFS.
            // This ensures that subsequent access checks compare against
            // actual paths rather than opaque fd numbers.
            let path = match crate::vfs::vfs_fd_directory_path(parent_fd as i32) {
                Ok(dir_path) => dir_path,
                Err(_) => {
                    // If the fd can't be resolved, fall back to the fd
                    // number string so the rule is still recorded.
                    String::from("fd:") + &parent_fd.to_string()
                }
            };

            ruleset.rules.push(LandlockRule::PathBeneath {
                allowed_access: allowed,
                path,
                quiet,
            });

            crate::serial_println!("[landlock] add_rule: path_beneath allowed={:#x}", allowed);
        }
        LANDLOCK_RULE_NET_PORT => {
            // SAFETY: rule_attr is a validated user pointer (checked for null above); LandlockNetPortAttr is repr(C) Copy.
            let attr = unsafe { *(rule_attr as *const LandlockNetPortAttr) };
            let allowed = attr.allowed_access;
            let port = attr.port;

            if allowed & !ruleset.handled_access_net != 0 {
                return -22;
            }

            ruleset.rules.push(LandlockRule::NetPort {
                allowed_access: allowed,
                port,
                quiet,
            });

            crate::serial_println!(
                "[landlock] add_rule: net_port port={} allowed={:#x}",
                port,
                allowed
            );
        }
        _ => return -22, // EINVAL
    }

    0
}

/// landlock_restrict_self — enforce a ruleset on the calling process
///
/// `ruleset_fd` is the ruleset fd.
/// `flags` controls behavior (LANDLOCK_RESTRICT_SELF_*).
///
/// Returns 0 on success, negative errno on failure.
pub fn landlock_restrict_self(ruleset_fd: i32, flags: u32) -> i32 {
    let id = match crate::linux_compat::special_fd::get_landlock_ruleset_id(ruleset_fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let valid_flags = LANDLOCK_RESTRICT_SELF_LOG_SAME_EXEC_OFF
        | LANDLOCK_RESTRICT_SELF_LOG_NEW_EXEC_ON
        | LANDLOCK_RESTRICT_SELF_LOG_SUBDOMAINS_OFF
        | LANDLOCK_RESTRICT_SELF_TSYNC;

    if flags & !valid_flags != 0 {
        return -22;
    }

    // Mark ruleset as enforced
    {
        let rulesets = RULESETS.read();
        if let Some(ruleset_mutex) = rulesets.get(&id) {
            let mut ruleset = ruleset_mutex.lock();
            ruleset.enforced = true;
        } else {
            return -9;
        }
    }

    // Attach to the calling process
    let pid = crate::process::current_pid();
    let mut states = PROCESS_STATES.write();
    let state_mutex = states.entry(pid).or_insert_with(|| {
        Mutex::new(ProcessLandlockState {
            enforced_rulesets: Vec::new(),
            no_new_privs: false,
        })
    });
    let mut state = state_mutex.lock();
    state.enforced_rulesets.push(id);
    state.no_new_privs = true;

    crate::serial_println!("[landlock] restrict_self: pid={} ruleset={}", pid, id);
    0
}

/// Check if a process has a Landlock rule that denies a specific fs access.
/// Returns true if access is allowed, false if denied.
pub fn check_fs_access(pid: u32, path: &str, access: u64) -> bool {
    let states = PROCESS_STATES.read();
    let Some(state_mutex) = states.get(&pid) else {
        return true; // No Landlock state — allow
    };
    let state = state_mutex.lock();

    let rulesets = RULESETS.read();
    for &ruleset_id in &state.enforced_rulesets {
        let Some(ruleset_mutex) = rulesets.get(&ruleset_id) else {
            continue;
        };
        let ruleset = ruleset_mutex.lock();

        // If this access right is not handled by this ruleset, skip
        if ruleset.handled_access_fs & access == 0 {
            continue;
        }

        // Check if any rule allows this access for this path
        let mut allowed = false;
        for rule in &ruleset.rules {
            if let LandlockRule::PathBeneath {
                allowed_access,
                path: rule_path,
                ..
            } = rule
            {
                if path.starts_with(rule_path.as_str()) && (allowed_access & access) != 0 {
                    allowed = true;
                    break;
                }
            }
        }

        // If no rule allows it, and the ruleset handles this access, deny
        if !allowed {
            return false;
        }
    }

    true
}

/// Check if a process has a Landlock rule that denies a specific net access.
pub fn check_net_access(pid: u32, port: u64, access: u64) -> bool {
    let states = PROCESS_STATES.read();
    let Some(state_mutex) = states.get(&pid) else {
        return true;
    };
    let state = state_mutex.lock();

    let rulesets = RULESETS.read();
    for &ruleset_id in &state.enforced_rulesets {
        let Some(ruleset_mutex) = rulesets.get(&ruleset_id) else {
            continue;
        };
        let ruleset = ruleset_mutex.lock();

        if ruleset.handled_access_net & access == 0 {
            continue;
        }

        let mut allowed = false;
        for rule in &ruleset.rules {
            if let LandlockRule::NetPort {
                allowed_access,
                port: rule_port,
                ..
            } = rule
            {
                if *rule_port == port && (allowed_access & access) != 0 {
                    allowed = true;
                    break;
                }
            }
        }

        if !allowed {
            return false;
        }
    }

    true
}

/// Close a ruleset (called when the fd is closed).
pub fn close_ruleset(id: u32) {
    RULESETS.write().remove(&id);
}

/// Initialize the Landlock subsystem.
pub fn init() {
    crate::serial_println!(
        "[landlock] Landlock subsystem initialized (ABI v{})",
        LANDLOCK_ABI_VERSION
    );
}
