//! Linux audit subsystem — syscall/path event logging with filter rules.
//!
//! Mirrors the core model of Linux's `kernel/audit.c`, `kernel/auditsc.c`
//! and `kernel/auditfilter.c`:
//!   - [`AuditRule`] holds a list of field comparators ([`AuditField`] /
//!     [`AuditCmp`]), evaluated like `audit_filter_rules()`: within a rule
//!     all fields must match (AND); across rules, any match is sufficient
//!     (OR). A rule with no fields matches every event of its type.
//!   - [`AuditContext`] is a per-pid accumulator analogous to the per-task
//!     `struct audit_context`: opened at syscall entry, accumulates the
//!     touched paths for the syscall, and is consumed (drained + cleared)
//!     when the syscall exits and the AUDIT_SYSCALL record is emitted.
//!   - Records are formatted as `audit(<sec>.<msec>:<serial>): type=...
//!     key=value ...` strings, single line, matching the on-wire shape of
//!     Linux audit records (sequence id + uptime-based timestamp; this
//!     kernel has no wall-clock RTC reader wired into this subsystem, so
//!     the timestamp is uptime-since-boot, not epoch time).
//!   - A minimal audit_watch-style hook: [`audit_watch_notify`] is called
//!     from the existing inotify dispatch point ([`crate::inotify::notify_path`])
//!     for every VFS path event, and matches against watch rules. There is
//!     no audit_tree (directory subtree) equivalent — see the module-level
//!     gap note near [`AuditWatch`].
//!
//! Status is exposed via `/proc/sys/kernel/audit*` through [`install_proc`].

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

use crate::vfs::{InodeOps, InodeType, Stat, VfsError, VfsResult};

pub const MAX_RECORDS: usize = 4096;
pub const MAX_FILTERS: usize = 64;
pub const MAX_WATCHES: usize = 64;
/// Cap on path names accumulated per in-flight syscall context, mirroring
/// Linux's bounded `audit_names` list (there it's bounded by allocation
/// pressure; here we use a fixed cap to keep this lock-free-adjacent and
/// avoid unbounded growth on a runaway syscall).
pub const MAX_CONTEXT_NAMES: usize = 16;

pub const AUDIT_FILTER_USER: u32 = 0;
pub const AUDIT_FILTER_TYPE: u32 = 5;
pub const AUDIT_FILTER_EXIT: u32 = 4;

/// Audit event categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditType {
    Syscall,
    Path,
    Net,
    Cap,
    User,
    Kernel,
}

impl AuditType {
    fn tag(self) -> &'static str {
        match self {
            Self::Syscall => "SYSCALL",
            Self::Path => "PATH",
            Self::Net => "NET",
            Self::Cap => "CAP",
            Self::User => "USER",
            Self::Kernel => "KERNEL",
        }
    }
}

/// Filesystem audit operation (maps to [`AuditType::Path`] / exec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOp {
    Open,
    Openat,
    Read,
    Write,
    Exec,
    Other,
}

/// One audit log record.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub seq: u64,
    pub type_: AuditType,
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
    pub success: bool,
    pub message: String,
    pub timestamp_ns: u64,
    pub syscall_nr: Option<i32>,
    pub path: Option<String>,
}

/// A single audit_log record formatted Linux-style:
/// `audit(<sec>.<msec>:<serial>): type=TAG key=value ...`
fn format_record(rec: &AuditRecord) -> String {
    let sec = rec.timestamp_ns / 1_000_000_000;
    let msec = (rec.timestamp_ns / 1_000_000) % 1000;
    let mut s = format!(
        "audit({}.{:03}:{}): type={} pid={} uid={} gid={} success={}",
        sec,
        msec,
        rec.seq,
        rec.type_.tag(),
        rec.pid,
        rec.uid,
        rec.gid,
        if rec.success { "yes" } else { "no" }
    );
    if let Some(nr) = rec.syscall_nr {
        s.push_str(&format!(" syscall={}", nr));
    }
    if let Some(ref p) = rec.path {
        s.push_str(&format!(" path=\"{}\"", p));
    }
    if !rec.message.is_empty() {
        s.push(' ');
        s.push_str(&rec.message);
    }
    s
}

/// A field within an [`AuditRule`], mirroring the subset of Linux
/// `audit_field` types relevant to a syscall-centric kernel: identity
/// (uid/gid/pid), the syscall number, the exit/success state and a path
/// prefix match. Linux additionally supports inode/arch/session/etc.
/// fields not modeled here because this kernel has no equivalent data
/// source wired through `audit_log_*` yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditField {
    Uid,
    Gid,
    Pid,
    SyscallNr,
    ExitCode,
    Success,
}

/// Comparator applied between an [`AuditField`] and its rule value,
/// mirroring Linux's `audit_filter.h` comparators (`Audit_equal`,
/// `Audit_not_equal`, `Audit_gt`, `Audit_lt`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditCmp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

/// One field comparator: `field <op> value`.
#[derive(Debug, Clone, Copy)]
pub struct AuditFieldCmp {
    pub field: AuditField,
    pub op: AuditCmp,
    pub value: i64,
}

impl AuditFieldCmp {
    pub fn new(field: AuditField, op: AuditCmp, value: i64) -> Self {
        Self { field, op, value }
    }

    fn eval(actual: i64, op: AuditCmp, value: i64) -> bool {
        match op {
            AuditCmp::Eq => actual == value,
            AuditCmp::Ne => actual != value,
            AuditCmp::Gt => actual > value,
            AuditCmp::Ge => actual >= value,
            AuditCmp::Lt => actual < value,
            AuditCmp::Le => actual <= value,
        }
    }

    /// Evaluate this comparator against an event. Returns `None` if the
    /// event has no data for this field (e.g. `SyscallNr` against a
    /// non-syscall event) — Linux's `audit_filter_rules` treats an
    /// unsatisfiable field as "rule does not match", which callers must
    /// honor (an absent field is never silently skipped).
    fn matches(&self, ev: &AuditEvent) -> bool {
        let actual = match self.field {
            AuditField::Uid => Some(ev.uid as i64),
            AuditField::Gid => Some(ev.gid as i64),
            AuditField::Pid => Some(ev.pid as i64),
            AuditField::SyscallNr => ev.syscall_nr.map(|n| n as i64),
            AuditField::ExitCode => ev.exit_code,
            AuditField::Success => Some(if ev.success { 1 } else { 0 }),
        };
        match actual {
            Some(a) => Self::eval(a, self.op, self.value),
            None => false,
        }
    }
}

/// Snapshot of an audit-able event, passed to the rule matcher. This is
/// the RustOS analogue of the fields Linux's `audit_filter_rules` reads
/// out of `struct audit_context` / `task_struct` at filter time.
struct AuditEvent<'a> {
    type_: AuditType,
    pid: u32,
    uid: u32,
    gid: u32,
    success: bool,
    syscall_nr: Option<i32>,
    exit_code: Option<i64>,
    message: &'a str,
}

/// Filter rule: a filter-list placement (`filter_type`, e.g.
/// `AUDIT_FILTER_USER` / `AUDIT_FILTER_EXIT`), an [`AuditType`] gate, an
/// optional message-prefix match, and zero or more [`AuditFieldCmp`]
/// comparators. All comparators must match (AND); a rule with an empty
/// `fields` list matches every event of its `type_` (preserving the
/// previous simple uid/gid/prefix behavior via `register_rule`).
#[derive(Debug, Clone)]
pub struct AuditRule {
    pub id: u32,
    pub filter_type: u32,
    pub type_: AuditType,
    pub uid: u32,
    pub gid: u32,
    pub prefix: String,
    pub enabled: bool,
    pub uid_exact: bool,
    pub gid_exact: bool,
    pub fields: Vec<AuditFieldCmp>,
}

impl AuditRule {
    /// Build a rule with the legacy uid/gid/prefix-only fields and no
    /// extra comparators. Used by both [`register_rule`] and [`init`] so
    /// there is a single place that knows every `AuditRule` field.
    fn simple(filter_type: u32, type_: AuditType, prefix: &str) -> Self {
        Self {
            id: 0,
            filter_type,
            type_,
            uid: u32::MAX,
            gid: u32::MAX,
            prefix: String::from(prefix),
            enabled: true,
            uid_exact: false,
            gid_exact: false,
            fields: Vec::new(),
        }
    }

    fn matches(&self, ev: &AuditEvent) -> bool {
        if !self.enabled {
            return false;
        }
        if self.type_ != ev.type_ {
            return false;
        }
        if self.uid_exact && self.uid != ev.uid {
            return false;
        }
        if self.gid_exact && self.gid != ev.gid {
            return false;
        }
        if !self.prefix.is_empty() && !ev.message.starts_with(&self.prefix) {
            return false;
        }
        // Field comparators: AND within the rule. A field the event can't
        // supply data for makes the rule not match (see AuditFieldCmp::matches).
        self.fields.iter().all(|f| f.matches(ev))
    }
}

/// Per-pid syscall audit context, analogous to Linux's per-task
/// `struct audit_context`. Opened at syscall entry by
/// [`audit_syscall_entry`], accumulates touched path names via
/// [`audit_log_path`], and is drained + closed by [`audit_log_syscall`]
/// at syscall exit.
#[derive(Debug, Clone, Default)]
struct AuditContext {
    names: Vec<String>,
}

/// A file/directory watch rule, the minimal analogue of Linux's
/// `audit_watch` (`kernel/audit_watch.c`). Triggers when a VFS path event
/// matches `path` (exact or as a directory prefix) and `mask` (an
/// `IN_*`-style inotify mask, since that's the only fsnotify-like signal
/// this kernel exposes — see [`crate::inotify::notify_path`]).
///
/// Gap: there is no `audit_tree` (recursive subtree watch keyed by inode,
/// re-anchored across renames) equivalent. Linux's audit_tree builds on
/// fsnotify "marks" attached directly to inodes and survives rename via
/// inode identity; this kernel's inotify layer matches purely by path
/// string (see `notify_path`'s `starts_with` check), so a faithful
/// audit_tree would need inode-keyed marks that don't exist here yet.
/// Building that would be speculative infra beyond this pass's scope, so
/// only the flat audit_watch model (path/prefix + mask) is implemented.
#[derive(Debug, Clone)]
pub struct AuditWatch {
    pub id: u32,
    pub path: String,
    pub mask: u32,
    pub enabled: bool,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static RULES: RwLock<Vec<AuditRule>> = RwLock::new(Vec::new());
static WATCHES: RwLock<Vec<AuditWatch>> = RwLock::new(Vec::new());
static CONTEXTS: RwLock<BTreeMap<u32, AuditContext>> = RwLock::new(BTreeMap::new());
static RECORDS: RwLock<VecDeque<AuditRecord>> = RwLock::new(VecDeque::new());
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);
static NEXT_FILTER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_WATCH_ID: AtomicU64 = AtomicU64::new(1);
static LOST: AtomicU64 = AtomicU64::new(0);

fn current_creds() -> (u32, u32, u32) {
    let pid = crate::process::current_pid();
    if pid == 0 {
        return (0, 0, 0);
    }
    let pm = crate::process::get_process_manager();
    if let Some(pcb) = pm.get_process(pid) {
        (pid, pcb.euid, pcb.egid)
    } else {
        (pid, 0, 0)
    }
}

fn should_log(ev: &AuditEvent) -> bool {
    let rules = RULES.read();
    if rules.is_empty() {
        return true;
    }
    rules.iter().any(|r| r.matches(ev))
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Release);
}

/// Register an audit filter rule (legacy prefix API, no field comparators).
pub fn register_rule(type_: AuditType, prefix: &str) {
    add_filter(AuditRule::simple(AUDIT_FILTER_TYPE, type_, prefix));
}

pub fn add_filter(mut rule: AuditRule) {
    let mut rules = RULES.write();
    rule.id = NEXT_FILTER_ID.fetch_add(1, Ordering::Relaxed) as u32;
    if rules.len() >= MAX_FILTERS {
        rules.remove(0);
    }
    rules.push(rule);
}

pub fn remove_filter(id: u32) -> bool {
    let mut rules = RULES.write();
    if let Some(pos) = rules.iter().position(|r| r.id == id) {
        rules.remove(pos);
        true
    } else {
        false
    }
}

pub fn clear_filters() {
    RULES.write().clear();
}

pub fn set_rule_enabled(index: usize, enabled: bool) -> bool {
    let mut rules = RULES.write();
    if let Some(rule) = rules.get_mut(index) {
        rule.enabled = enabled;
        true
    } else {
        false
    }
}

// ── Watch rules (audit_watch-style) ─────────────────────────────────────

/// Register a watch on `path` (exact match or directory prefix) for the
/// given inotify-style `mask`. Returns the watch id.
pub fn add_watch(path: &str, mask: u32) -> u32 {
    let mut watches = WATCHES.write();
    let id = NEXT_WATCH_ID.fetch_add(1, Ordering::Relaxed) as u32;
    if watches.len() >= MAX_WATCHES {
        watches.remove(0);
    }
    watches.push(AuditWatch {
        id,
        path: String::from(path),
        mask,
        enabled: true,
    });
    id
}

pub fn remove_watch(id: u32) -> bool {
    let mut watches = WATCHES.write();
    if let Some(pos) = watches.iter().position(|w| w.id == id) {
        watches.remove(pos);
        true
    } else {
        false
    }
}

pub fn watch_count() -> usize {
    WATCHES.read().len()
}

/// Called from [`crate::inotify::notify_path`] (the one real VFS
/// path-event hook in this kernel) for every file event. Matches the
/// event path/mask against registered watches and, if any match and
/// auditing is enabled, emits a PATH record tagged as a watch hit.
pub fn audit_watch_notify(path: &str, mask: u32) {
    if !is_enabled() {
        return;
    }
    let hit = {
        let watches = WATCHES.read();
        watches
            .iter()
            .any(|w| w.enabled && w.mask & mask != 0 && (path == w.path || path.starts_with(&w.path)))
    };
    if !hit {
        return;
    }
    let (pid, uid, gid) = current_creds();
    let message = format!("watch path=\"{}\" mask={:#x}", path, mask);
    store_record(AuditType::Path, pid, uid, gid, true, message, None, Some(path.to_string()));
}

fn store_record(
    type_: AuditType,
    pid: u32,
    uid: u32,
    gid: u32,
    success: bool,
    message: String,
    syscall_nr: Option<i32>,
    path: Option<String>,
) {
    let record = AuditRecord {
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        type_,
        pid,
        uid,
        gid,
        success,
        message,
        timestamp_ns: crate::time::uptime_ns(),
        syscall_nr,
        path,
    };

    crate::serial_println!("[audit] {}", format_record(&record));

    let mut buf = RECORDS.write();
    if buf.len() >= MAX_RECORDS {
        buf.pop_front();
        LOST.fetch_add(1, Ordering::Relaxed);
    }
    buf.push_back(record);
}

/// Record an audit event if auditing is enabled and filters match.
pub fn audit_log(type_: AuditType, pid: u32, uid: u32, success: bool, message: &str) {
    if !is_enabled() {
        return;
    }
    let (_, cred_uid, cred_gid) = if pid == 0 {
        (0, uid, 0)
    } else {
        current_creds()
    };
    let effective_uid = if uid != 0 { uid } else { cred_uid };
    let ev = AuditEvent {
        type_,
        pid,
        uid: effective_uid,
        gid: cred_gid,
        success,
        syscall_nr: None,
        exit_code: None,
        message,
    };
    if !should_log(&ev) {
        return;
    }
    store_record(
        type_,
        pid,
        effective_uid,
        cred_gid,
        success,
        String::from(message),
        None,
        None,
    );
}

// ── Per-syscall audit context ───────────────────────────────────────────

/// Open (or reset) the audit context for the current task at syscall
/// entry, mirroring Linux's `audit_alloc`/`__audit_syscall_entry`. Cheap
/// no-op when auditing is disabled, so this is safe to call unconditionally
/// from the hot syscall-dispatch path.
///
/// `syscall_nr`/`args` are accepted (mirroring the Linux entry-hook
/// signature, and so callers don't need to special-case this call) but
/// are not retained: the exit-side record is built from the values
/// passed to [`audit_log_syscall`], which is always called with the
/// authoritative post-dispatch values. Only the accumulated path
/// `names` need to survive between entry and exit.
///
/// Gap: if a task is torn down between entry and exit without ever
/// reaching an `audit_log_syscall` call (e.g. killed mid-syscall), its
/// [`CONTEXTS`] entry is never removed. Per-pid keys mean a live pid's
/// next syscall overwrites its own stale entry, so this cannot grow
/// unboundedly while the pid is reused, but a genuinely dead pid's
/// entry lingers until reuse. Linux frees this deterministically at
/// task exit via `audit_free`; this kernel has no general task-exit
/// hook wired into `audit` yet, so this is left as a known leak rather
/// than building speculative cleanup infrastructure.
pub fn audit_syscall_entry(_syscall_nr: i32, _args: &[u64; 6]) {
    if !is_enabled() {
        return;
    }
    let pid = crate::process::current_pid();
    if pid == 0 {
        return;
    }
    CONTEXTS.write().insert(pid, AuditContext::default());
}

/// Record a path name against the current task's in-flight syscall
/// context, mirroring `__audit_inode`/`audit_getname`. Silently bounded
/// by [`MAX_CONTEXT_NAMES`]; no-op if there is no open context (e.g.
/// auditing was off at syscall entry) or auditing is disabled.
fn audit_context_add_name(path: &str) {
    if !is_enabled() {
        return;
    }
    let pid = crate::process::current_pid();
    if pid == 0 {
        return;
    }
    let mut contexts = CONTEXTS.write();
    if let Some(ctx) = contexts.get_mut(&pid) {
        if ctx.names.len() < MAX_CONTEXT_NAMES {
            ctx.names.push(String::from(path));
        }
    }
}

/// Log a syscall audit event for the current task, draining and closing
/// any open [`AuditContext`] (the names accumulated via
/// [`audit_log_path`] during the syscall), mirroring
/// `__audit_syscall_exit`.
pub fn audit_log_syscall(syscall_nr: i32, args: &[u64; 6], result: i64) {
    if !is_enabled() {
        return;
    }
    let (pid, uid, gid) = current_creds();
    let success = result >= 0;

    let names = if pid != 0 {
        CONTEXTS.write().remove(&pid).map(|ctx| ctx.names)
    } else {
        None
    }
    .unwrap_or_default();

    let mut message = format!(
        "syscall={} args=[{:#x},{:#x},{:#x},{:#x},{:#x},{:#x}] ret={}",
        syscall_nr, args[0], args[1], args[2], args[3], args[4], args[5], result
    );
    if !names.is_empty() {
        message.push_str(" names=[");
        for (i, n) in names.iter().enumerate() {
            if i > 0 {
                message.push(',');
            }
            message.push_str(n);
        }
        message.push(']');
    }

    let ev = AuditEvent {
        type_: AuditType::Syscall,
        pid,
        uid,
        gid,
        success,
        syscall_nr: Some(syscall_nr),
        exit_code: Some(result),
        message: &message,
    };
    if !should_log(&ev) {
        return;
    }
    store_record(
        AuditType::Syscall,
        pid,
        uid,
        gid,
        success,
        message,
        Some(syscall_nr),
        None,
    );
}

/// Log a filesystem path audit event for the current task, and (if a
/// syscall context is open) record the path into it so it shows up in
/// the eventual AUDIT_SYSCALL record's `names=[...]` list — mirroring how
/// Linux attaches `audit_names` entries to the in-flight `audit_context`.
pub fn audit_log_path(op: AuditOp, path: &str, success: bool) {
    audit_context_add_name(path);

    if !is_enabled() {
        return;
    }
    let (pid, uid, gid) = current_creds();
    let type_ = match op {
        AuditOp::Exec => AuditType::Path,
        _ => AuditType::Path,
    };
    let message = format!("op={:?} path={path}", op);
    let ev = AuditEvent {
        type_,
        pid,
        uid,
        gid,
        success,
        syscall_nr: None,
        exit_code: None,
        message: &message,
    };
    if !should_log(&ev) {
        return;
    }
    store_record(
        type_,
        pid,
        uid,
        gid,
        success,
        message,
        None,
        Some(path.to_string()),
    );
}

pub fn read_records(after_seq: u64, max: usize) -> Vec<AuditRecord> {
    RECORDS
        .read()
        .iter()
        .filter(|r| r.seq > after_seq)
        .take(max)
        .cloned()
        .collect()
}

pub fn record_count() -> u64 {
    NEXT_SEQ.load(Ordering::Relaxed).saturating_sub(1)
}

pub fn filter_count() -> usize {
    RULES.read().len()
}

pub fn log_count() -> usize {
    RECORDS.read().len()
}

pub fn lost_count() -> u64 {
    LOST.load(Ordering::Relaxed)
}

pub fn status_text() -> String {
    format!(
        "enabled={}\nlost={}\nlogged={}\nfilters={}\nwatches={}\nrecords={}\n",
        if is_enabled() { 1 } else { 0 },
        lost_count(),
        log_count(),
        filter_count(),
        watch_count(),
        record_count(),
    )
}

pub fn audit_enabled_text() -> String {
    if is_enabled() {
        String::from("1\n")
    } else {
        String::from("0\n")
    }
}

pub fn set_audit_enabled_from_proc(value: &str) -> Result<(), ()> {
    match value.trim() {
        "0" => {
            set_enabled(false);
            Ok(())
        }
        "1" => {
            set_enabled(true);
            Ok(())
        }
        _ => Err(()),
    }
}

// ── /proc/sys/kernel/audit* ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditProcKind {
    Enabled,
    Status,
}

struct AuditProcInode {
    ino: u64,
    kind: AuditProcKind,
    mode: u32,
}

impl AuditProcInode {
    fn new(ino: u64, kind: AuditProcKind, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, kind, mode })
    }

    fn read_content(&self) -> String {
        match self.kind {
            AuditProcKind::Enabled => audit_enabled_text(),
            AuditProcKind::Status => status_text(),
        }
    }

    fn write_content(&self, buf: &[u8]) -> VfsResult<usize> {
        match self.kind {
            AuditProcKind::Enabled => {
                let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
                set_audit_enabled_from_proc(text).map_err(|_| VfsError::InvalidArgument)?;
                Ok(buf.len())
            }
            AuditProcKind::Status => Err(VfsError::ReadOnly),
        }
    }
}

impl InodeOps for AuditProcInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.read_content();
        let bytes = content.as_bytes();
        let start = offset as usize;
        if start >= bytes.len() {
            return Ok(0);
        }
        let end = core::cmp::min(start + buf.len(), bytes.len());
        let n = end - start;
        buf[..n].copy_from_slice(&bytes[start..end]);
        Ok(n)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            return Err(VfsError::InvalidArgument);
        }
        self.write_content(buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.read_content().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<crate::vfs::DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

pub fn install_proc(kernel_dir: &Arc<dyn InodeOps>, ino: &mut u64) -> VfsResult<()> {
    *ino += 1;
    kernel_dir.attach_child(
        "audit_enabled",
        AuditProcInode::new(*ino, AuditProcKind::Enabled, 0o644),
    )?;
    *ino += 1;
    kernel_dir.attach_child(
        "audit",
        AuditProcInode::new(*ino, AuditProcKind::Status, 0o444),
    )?;
    Ok(())
}

pub fn init() {
    RULES.write().clear();
    WATCHES.write().clear();
    CONTEXTS.write().clear();
    RECORDS.write().clear();
    NEXT_SEQ.store(1, Ordering::Release);
    NEXT_FILTER_ID.store(1, Ordering::Release);
    NEXT_WATCH_ID.store(1, Ordering::Release);
    LOST.store(0, Ordering::Release);
    ENABLED.store(false, Ordering::Release);

    register_rule(AuditType::Syscall, "");
    register_rule(AuditType::Path, "/etc/");
    register_rule(AuditType::Cap, "");
    register_rule(AuditType::Kernel, "");

    add_filter(AuditRule {
        filter_type: AUDIT_FILTER_USER,
        uid: 0,
        uid_exact: true,
        ..AuditRule::simple(AUDIT_FILTER_USER, AuditType::Syscall, "")
    });

    crate::serial_println!(
        "[audit] audit subsystem initialized ({} rules, {} watches)",
        filter_count(),
        watch_count()
    );
}
