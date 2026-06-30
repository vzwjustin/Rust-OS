//! Namespaces — Process namespace isolation
//!
//! Ported from Linux kernel/nsproxy.c and include/linux/nsproxy.h.
//! Provides per-process namespace isolation for:
//! - PID namespace (process ID virtualization)
//! - Mount namespace (filesystem mount tree isolation)
//! - Network namespace (network stack isolation)
//! - UTS namespace (hostname/domainname isolation)
//! - IPC namespace (System V IPC isolation)
//! - User namespace (UID/GID mapping)
//! - Cgroup namespace (cgroup path isolation)
//!
//! ## Architecture
//! Each process has an NsProxy that holds references to its namespaces.
//! unshare() and clone(CLONE_NEW*) create new namespaces.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// ── Clone flags for namespace creation ──────────────────────────────────

pub const CLONE_NEWNS: u32 = 0x00020000; // Mount namespace
pub const CLONE_NEWUTS: u32 = 0x04000000; // UTS namespace
pub const CLONE_NEWIPC: u32 = 0x08000000; // IPC namespace
pub const CLONE_NEWUSER: u32 = 0x10000000; // User namespace
pub const CLONE_NEWPID: u32 = 0x20000000; // PID namespace
pub const CLONE_NEWNET: u32 = 0x40000000; // Network namespace
pub const CLONE_NEWCGROUP: u32 = 0x02000000; // Cgroup namespace

// ── Namespace types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NsType {
    Mount,
    Uts,
    Ipc,
    User,
    Pid,
    Net,
    Cgroup,
}

impl NsType {
    pub fn from_clone_flag(flag: u32) -> Option<Self> {
        match flag {
            CLONE_NEWNS => Some(Self::Mount),
            CLONE_NEWUTS => Some(Self::Uts),
            CLONE_NEWIPC => Some(Self::Ipc),
            CLONE_NEWUSER => Some(Self::User),
            CLONE_NEWPID => Some(Self::Pid),
            CLONE_NEWNET => Some(Self::Net),
            CLONE_NEWCGROUP => Some(Self::Cgroup),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Mount => "mnt",
            Self::Uts => "uts",
            Self::Ipc => "ipc",
            Self::User => "user",
            Self::Pid => "pid",
            Self::Net => "net",
            Self::Cgroup => "cgroup",
        }
    }
}

// ── UTS namespace ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UtsNamespace {
    pub nodename: String,
    pub domainname: String,
    pub release: String,
    pub version: String,
    pub sysname: String,
    pub machine: String,
}

impl Default for UtsNamespace {
    fn default() -> Self {
        Self {
            nodename: String::from("rustos"),
            domainname: String::from("(none)"),
            release: String::from("5.0.0-rustos"),
            version: String::from("#1 RustOS"),
            sysname: String::from("Linux"),
            machine: String::from("x86_64"),
        }
    }
}

// ── PID namespace ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PidNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    pub level: u32,
    /// Map from global PID → local PID
    pub pid_map: BTreeMap<u32, u32>,
    /// Next local PID to assign
    pub next_local_pid: u32,
}

impl PidNamespace {
    fn new(id: u64, parent: Option<u64>, level: u32) -> Self {
        Self {
            id,
            parent,
            level,
            pid_map: BTreeMap::new(),
            next_local_pid: 1,
        }
    }

    /// Assign a local PID for a global PID in this namespace.
    pub fn assign_local_pid(&mut self, global_pid: u32) -> u32 {
        if let Some(&local) = self.pid_map.get(&global_pid) {
            return local;
        }
        let local = self.next_local_pid;
        self.next_local_pid += 1;
        self.pid_map.insert(global_pid, local);
        local
    }

    /// Get the local PID for a global PID.
    pub fn local_pid(&self, global_pid: u32) -> u32 {
        self.pid_map.get(&global_pid).copied().unwrap_or(global_pid)
    }

    /// Resolve a namespace-local PID back to the global (kernel) PID.
    /// Returns `None` if the local PID is not mapped in this namespace.
    pub fn resolve_local_pid(&self, local_pid: u32) -> Option<u32> {
        self.pid_map
            .iter()
            .find(|(_, &v)| v == local_pid)
            .map(|(&k, _)| k)
    }
}

// ── Mount namespace ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MountNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    /// Mount tree root path
    pub root: String,
    /// List of mount points in this namespace
    pub mounts: Vec<MountEntry>,
}

#[derive(Debug, Clone)]
pub struct MountEntry {
    pub source: String,
    pub target: String,
    pub fs_type: String,
    pub flags: u32,
}

impl MountNamespace {
    fn new(id: u64, parent: Option<u64>) -> Self {
        Self {
            id,
            parent,
            root: String::from("/"),
            mounts: Vec::new(),
        }
    }
}

// ── Network namespace ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    /// Loopback address
    pub lo_addr: u32,
    /// Ethernet address (if any)
    pub eth_addr: Option<u32>,
    /// Routing table (simplified)
    pub routes: Vec<RouteEntry>,
}

#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub dest: u32,
    pub mask: u32,
    pub gateway: u32,
    pub interface: u32,
}

impl NetNamespace {
    fn new(id: u64, parent: Option<u64>) -> Self {
        Self {
            id,
            parent,
            lo_addr: 0x7F000001, // 127.0.0.1
            eth_addr: None,
            routes: Vec::new(),
        }
    }
}

// ── IPC namespace ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IpcNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    /// IPC key → ID mappings (isolated per namespace)
    pub ipc_keys: BTreeMap<u32, u32>,
}

impl IpcNamespace {
    fn new(id: u64, parent: Option<u64>) -> Self {
        Self {
            id,
            parent,
            ipc_keys: BTreeMap::new(),
        }
    }
}

// ── User namespace ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UserNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    pub level: u32,
    /// UID mappings: (parent_uid, child_uid, count)
    pub uid_map: Vec<IdMapEntry>,
    /// GID mappings: (parent_gid, child_gid, count)
    pub gid_map: Vec<IdMapEntry>,
}

#[derive(Debug, Clone)]
pub struct IdMapEntry {
    pub first_parent: u32,
    pub first_child: u32,
    pub count: u32,
}

impl UserNamespace {
    fn new(id: u64, parent: Option<u64>, level: u32) -> Self {
        Self {
            id,
            parent,
            level,
            uid_map: Vec::new(),
            gid_map: Vec::new(),
        }
    }

    /// Map a child UID to parent UID.
    pub fn map_uid_up(&self, child_uid: u32) -> u32 {
        for entry in &self.uid_map {
            if child_uid >= entry.first_child && child_uid < entry.first_child + entry.count {
                return entry.first_parent + (child_uid - entry.first_child);
            }
        }
        child_uid
    }

    /// Map a parent UID to child UID.
    pub fn map_uid_down(&self, parent_uid: u32) -> u32 {
        for entry in &self.uid_map {
            if parent_uid >= entry.first_parent && parent_uid < entry.first_parent + entry.count {
                return entry.first_child + (parent_uid - entry.first_parent);
            }
        }
        parent_uid
    }
}

// ── Cgroup namespace ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CgroupNamespace {
    pub id: u64,
    pub parent: Option<u64>,
    /// Root cgroup path for this namespace
    pub root_path: String,
}

impl CgroupNamespace {
    fn new(id: u64, parent: Option<u64>) -> Self {
        Self {
            id,
            parent,
            root_path: String::from("/"),
        }
    }
}

// ── NsProxy: per-process namespace set ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct NsProxy {
    pub uts: Arc<UtsNamespace>,
    pub pid: Arc<Mutex<PidNamespace>>,
    pub mnt: Arc<Mutex<MountNamespace>>,
    pub net: Arc<Mutex<NetNamespace>>,
    pub ipc: Arc<Mutex<IpcNamespace>>,
    pub user: Arc<UserNamespace>,
    pub cgroup: Arc<CgroupNamespace>,
}

impl Default for NsProxy {
    fn default() -> Self {
        Self {
            uts: Arc::new(UtsNamespace::default()),
            pid: Arc::new(Mutex::new(PidNamespace::new(0, None, 0))),
            mnt: Arc::new(Mutex::new(MountNamespace::new(0, None))),
            net: Arc::new(Mutex::new(NetNamespace::new(0, None))),
            ipc: Arc::new(Mutex::new(IpcNamespace::new(0, None))),
            user: Arc::new(UserNamespace::new(0, None, 0)),
            cgroup: Arc::new(CgroupNamespace::new(0, None)),
        }
    }
}

// ── Global state ────────────────────────────────────────────────────────

static NS_PROXIES: RwLock<BTreeMap<u32, NsProxy>> = RwLock::new(BTreeMap::new());
static NEXT_NS_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_NS_FD_ID: AtomicU32 = AtomicU32::new(1);
static NS_FD_HANDLES: RwLock<BTreeMap<u32, NamespaceHandle>> = RwLock::new(BTreeMap::new());

#[derive(Clone)]
struct NamespaceHandle {
    ns_type: NsType,
    ns: NsProxy,
}

/// Get the NsProxy for a process.
pub fn get_nsproxy(pid: u32) -> NsProxy {
    NS_PROXIES.read().get(&pid).cloned().unwrap_or_default()
}

/// Set the NsProxy for a process.
fn set_nsproxy(pid: u32, ns: NsProxy) {
    NS_PROXIES.write().insert(pid, ns);
}

fn clone_with_single_namespace(mut current: NsProxy, handle: &NamespaceHandle) -> NsProxy {
    match handle.ns_type {
        NsType::Mount => current.mnt = handle.ns.mnt.clone(),
        NsType::Uts => current.uts = handle.ns.uts.clone(),
        NsType::Ipc => current.ipc = handle.ns.ipc.clone(),
        NsType::User => current.user = handle.ns.user.clone(),
        NsType::Pid => current.pid = handle.ns.pid.clone(),
        NsType::Net => current.net = handle.ns.net.clone(),
        NsType::Cgroup => current.cgroup = handle.ns.cgroup.clone(),
    }
    current
}

/// Open a namespace handle fd for a process namespace type.
pub fn open_namespace_fd(pid: u32, ns_type: NsType, flags: u32) -> i32 {
    if crate::process::get_process_manager()
        .get_process(pid)
        .is_none()
    {
        return -3; // ESRCH
    }
    let id = NEXT_NS_FD_ID.fetch_add(1, Ordering::SeqCst);
    NS_FD_HANDLES.write().insert(
        id,
        NamespaceHandle {
            ns_type,
            ns: get_nsproxy(pid),
        },
    );
    crate::linux_compat::special_fd::register_namespace(id, flags)
}

pub fn close_namespace_fd(id: u32) {
    NS_FD_HANDLES.write().remove(&id);
}

/// Create a new namespace of the given type, returning a new NsProxy.
fn create_namespace(ns_type: NsType, parent: &NsProxy) -> NsProxy {
    let id = NEXT_NS_ID.fetch_add(1, Ordering::SeqCst);
    let mut new_ns = parent.clone();

    match ns_type {
        NsType::Uts => {
            new_ns.uts = Arc::new(UtsNamespace::default());
        }
        NsType::Pid => {
            let parent_id = parent.pid.lock().id;
            let parent_level = parent.pid.lock().level;
            new_ns.pid = Arc::new(Mutex::new(PidNamespace::new(
                id,
                Some(parent_id),
                parent_level + 1,
            )));
        }
        NsType::Mount => {
            let parent_id = parent.mnt.lock().id;
            new_ns.mnt = Arc::new(Mutex::new(MountNamespace::new(id, Some(parent_id))));
        }
        NsType::Net => {
            let parent_id = parent.net.lock().id;
            new_ns.net = Arc::new(Mutex::new(NetNamespace::new(id, Some(parent_id))));
        }
        NsType::Ipc => {
            let parent_id = parent.ipc.lock().id;
            new_ns.ipc = Arc::new(Mutex::new(IpcNamespace::new(id, Some(parent_id))));
        }
        NsType::User => {
            let parent_id = parent.user.id;
            let parent_level = parent.user.level;
            new_ns.user = Arc::new(UserNamespace::new(id, Some(parent_id), parent_level + 1));
        }
        NsType::Cgroup => {
            let parent_id = parent.cgroup.id;
            new_ns.cgroup = Arc::new(CgroupNamespace::new(id, Some(parent_id)));
        }
    }

    new_ns
}

// ── Public API: unshare ─────────────────────────────────────────────────

/// unshare — create new namespaces for the calling process.
/// Returns 0 on success, negative errno on failure.
pub fn unshare(flags: u32) -> i32 {
    if flags == 0 {
        return -22; // EINVAL
    }

    let pid = crate::process::current_pid();
    let current_ns = get_nsproxy(pid);

    let mut new_ns = current_ns.clone();

    // Process each requested namespace
    let ns_types = [
        (CLONE_NEWNS, NsType::Mount),
        (CLONE_NEWUTS, NsType::Uts),
        (CLONE_NEWIPC, NsType::Ipc),
        (CLONE_NEWUSER, NsType::User),
        (CLONE_NEWPID, NsType::Pid),
        (CLONE_NEWNET, NsType::Net),
        (CLONE_NEWCGROUP, NsType::Cgroup),
    ];

    let mut created_any = false;
    for (flag, ns_type) in &ns_types {
        if flags & flag != 0 {
            new_ns = create_namespace(*ns_type, &new_ns);
            created_any = true;
            crate::serial_println!(
                "[ns] unshare: pid {} created new {} namespace",
                pid,
                ns_type.name()
            );
        }
    }

    if !created_any {
        return -22; // EINVAL
    }

    set_nsproxy(pid, new_ns);
    0
}

/// setns — reassociate with a namespace via fd.
/// Returns 0 on success, negative errno on failure.
pub fn setns(fd: i32, nstype: u32) -> i32 {
    if fd < 0 {
        return -9; // EBADF
    }
    if crate::vfs::vfs_fd_kind(fd).is_err() {
        return -9; // EBADF
    }
    let id = match crate::linux_compat::special_fd::get_namespace_id(fd) {
        Some(id) => id,
        None => return -22, // EINVAL: valid fd, but not a namespace fd
    };
    let handle = match NS_FD_HANDLES.read().get(&id).cloned() {
        Some(handle) => handle,
        None => return -9,
    };
    if nstype != 0 {
        let Some(requested) = NsType::from_clone_flag(nstype) else {
            return -22;
        };
        if requested != handle.ns_type {
            return -22;
        }
    }

    let pid = crate::process::current_pid();
    let current_ns = get_nsproxy(pid);
    let new_ns = clone_with_single_namespace(current_ns, &handle);
    set_nsproxy(pid, new_ns);
    crate::serial_println!(
        "[ns] setns: pid {} joined {} namespace from fd {}",
        pid,
        handle.ns_type.name(),
        fd
    );
    0
}

/// clone_ns — create new namespaces for a child process (called from clone).
pub fn clone_ns(parent_pid: u32, child_pid: u32, clone_flags: u64) {
    let ns_flags = clone_flags as u32;
    if ns_flags
        & (CLONE_NEWNS
            | CLONE_NEWUTS
            | CLONE_NEWIPC
            | CLONE_NEWUSER
            | CLONE_NEWPID
            | CLONE_NEWNET
            | CLONE_NEWCGROUP)
        == 0
    {
        // No new namespaces requested — child shares parent's
        let parent_ns = get_nsproxy(parent_pid);
        set_nsproxy(child_pid, parent_ns);
        return;
    }

    let parent_ns = get_nsproxy(parent_pid);
    let child_ns = {
        let ns = parent_ns.clone();
        let mut current = ns.clone();

        let ns_types = [
            (CLONE_NEWNS, NsType::Mount),
            (CLONE_NEWUTS, NsType::Uts),
            (CLONE_NEWIPC, NsType::Ipc),
            (CLONE_NEWUSER, NsType::User),
            (CLONE_NEWPID, NsType::Pid),
            (CLONE_NEWNET, NsType::Net),
            (CLONE_NEWCGROUP, NsType::Cgroup),
        ];

        for (flag, ns_type) in &ns_types {
            if ns_flags & flag != 0 {
                current = create_namespace(*ns_type, &current);
            }
        }

        current
    };

    // If CLONE_NEWPID, assign local PID 1 to the child
    if ns_flags & CLONE_NEWPID != 0 {
        child_ns.pid.lock().assign_local_pid(child_pid);
    }

    set_nsproxy(child_pid, child_ns);
}

// ── UTS namespace operations ────────────────────────────────────────────

/// Get the hostname (nodename) for the current process.
pub fn get_hostname() -> String {
    let pid = crate::process::current_pid();
    get_nsproxy(pid).uts.nodename.clone()
}

/// Set the hostname for the current process's UTS namespace.
pub fn set_hostname(name: &str) {
    let pid = crate::process::current_pid();
    let ns = get_nsproxy(pid);
    let mut uts = (*ns.uts).clone();
    uts.nodename = String::from(name);
    let mut new_ns = ns;
    new_ns.uts = Arc::new(uts);
    set_nsproxy(pid, new_ns);
}

/// Get the domainname for the current process.
pub fn get_domainname() -> String {
    let pid = crate::process::current_pid();
    get_nsproxy(pid).uts.domainname.clone()
}

/// Set the domainname for the current process's UTS namespace.
pub fn set_domainname(name: &str) {
    let pid = crate::process::current_pid();
    let ns = get_nsproxy(pid);
    let mut uts = (*ns.uts).clone();
    uts.domainname = String::from(name);
    let mut new_ns = ns;
    new_ns.uts = Arc::new(uts);
    set_nsproxy(pid, new_ns);
}

// ── PID namespace operations ────────────────────────────────────────────

/// Get the local PID for a global PID in the current PID namespace.
pub fn local_pid(global_pid: u32) -> u32 {
    let pid = crate::process::current_pid();
    let ns = get_nsproxy(pid);
    let pid_ns = ns.pid.lock();
    pid_ns.local_pid(global_pid)
}

/// Get the PID namespace level for the current process.
pub fn pid_ns_level() -> u32 {
    let pid = crate::process::current_pid();
    let ns = get_nsproxy(pid);
    let pid_ns = ns.pid.lock();
    pid_ns.level
}

// ── User namespace operations ───────────────────────────────────────────

/// Set UID mapping for a user namespace.
pub fn set_uid_map(pid: u32, first_parent: u32, first_child: u32, count: u32) -> i32 {
    let ns = get_nsproxy(pid);
    // Can only set mapping in a child user namespace
    if ns.user.parent.is_none() {
        return -22; // EINVAL — can't map in init namespace
    }

    // We need to get a mutable copy of the user namespace
    let user_ns = (*ns.user).clone();
    let mut user_ns = user_ns;
    user_ns.uid_map.push(IdMapEntry {
        first_parent,
        first_child,
        count,
    });

    let mut new_ns = ns;
    new_ns.user = Arc::new(user_ns);
    set_nsproxy(pid, new_ns);
    0
}

/// Set GID mapping for a user namespace.
pub fn set_gid_map(pid: u32, first_parent: u32, first_child: u32, count: u32) -> i32 {
    let ns = get_nsproxy(pid);
    if ns.user.parent.is_none() {
        return -22;
    }

    let user_ns = (*ns.user).clone();
    let mut user_ns = user_ns;
    user_ns.gid_map.push(IdMapEntry {
        first_parent,
        first_child,
        count,
    });

    let mut new_ns = ns;
    new_ns.user = Arc::new(user_ns);
    set_nsproxy(pid, new_ns);
    0
}

// ── Cleanup ─────────────────────────────────────────────────────────────

/// Clear namespace state for a process (on exit).
pub fn clear(pid: u32) {
    NS_PROXIES.write().remove(&pid);
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    // Create initial namespace set for PID 1 (init)
    let init_ns = NsProxy::default();
    set_nsproxy(1, init_ns);
    crate::serial_println!("[ns] namespace subsystem initialized");
}

// ── Info ────────────────────────────────────────────────────────────────

pub fn list_namespaces(pid: u32) -> Vec<(String, u64, Option<u64>)> {
    let ns = get_nsproxy(pid);
    let mut result = Vec::new();

    result.push((String::from("uts"), 0, None));
    {
        let pid_ns = ns.pid.lock();
        result.push((String::from("pid"), pid_ns.id, pid_ns.parent));
    }
    {
        let mnt_ns = ns.mnt.lock();
        result.push((String::from("mnt"), mnt_ns.id, mnt_ns.parent));
    }
    {
        let net_ns = ns.net.lock();
        result.push((String::from("net"), net_ns.id, net_ns.parent));
    }
    {
        let ipc_ns = ns.ipc.lock();
        result.push((String::from("ipc"), ipc_ns.id, ipc_ns.parent));
    }
    result.push((String::from("user"), ns.user.id, ns.user.parent));
    result.push((String::from("cgroup"), ns.cgroup.id, ns.cgroup.parent));

    result
}

// ── Isolation enforcement helpers ────────────────────────────────────────

/// Translate a namespace-local (virtual) PID to the global kernel PID.
///
/// Returns `None` when `virtual_pid` is not mapped in `ns` (process not
/// visible from within this PID namespace).
pub fn resolve_pid(virtual_pid: u32, ns: &PidNamespace) -> Option<u32> {
    ns.resolve_local_pid(virtual_pid)
}

/// Resolve `path` against a mount namespace.
///
/// Checks whether any mount point in `mnt_ns` is a prefix of `path`.  If a
/// match is found the path is considered namespace-local; otherwise it falls
/// through to the global VFS.  Returns the effective path string to use for
/// the VFS lookup.
pub fn resolve_path<'a>(path: &'a str, mnt_ns: &MountNamespace) -> &'a str {
    for entry in &mnt_ns.mounts {
        // A mount point with a matching prefix "claims" this path.
        if path.starts_with(entry.target.as_str()) {
            return path;
        }
    }
    // No override — use the global VFS path unchanged.
    path
}

/// Get the hostname stored in a UTS namespace directly.
pub fn get_uts_hostname(uts_ns: &UtsNamespace) -> &str {
    &uts_ns.nodename
}

/// Set the hostname on a UTS namespace directly (namespace-local operation).
pub fn set_uts_hostname(uts_ns: &mut UtsNamespace, name: &str) {
    uts_ns.nodename = String::from(name);
}

/// Map a namespace-local UID to the parent (global) UID using the user
/// namespace UID mappings.  Returns the unmapped UID if no mapping exists.
pub fn map_uid(uid: u32, user_ns: &UserNamespace) -> u32 {
    user_ns.map_uid_up(uid)
}

/// Map a namespace-local GID to the parent (global) GID.
pub fn map_gid(gid: u32, user_ns: &UserNamespace) -> u32 {
    for entry in &user_ns.gid_map {
        if gid >= entry.first_child && gid < entry.first_child + entry.count {
            return entry.first_parent + (gid - entry.first_child);
        }
    }
    gid
}
