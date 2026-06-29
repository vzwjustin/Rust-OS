//! Linux audit subsystem — syscall/path event logging with filter rules.
//!
//! Subsystems register audit rules; matching events are recorded in the ring
//! buffer and forwarded to the serial log. Status is exposed via
//! `/proc/sys/kernel/audit*` through [`install_proc`].

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::RwLock;

use crate::vfs::{InodeOps, InodeType, Stat, VfsError, VfsResult};

pub const MAX_RECORDS: usize = 4096;
pub const MAX_FILTERS: usize = 64;

pub const AUDIT_FILTER_USER: u32 = 0;
pub const AUDIT_FILTER_TYPE: u32 = 5;

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

/// Filter rule with uid/gid/type matching.
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
}

impl AuditRule {
    fn matches(&self, type_: AuditType, uid: u32, gid: u32, message: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.type_ != type_ {
            return false;
        }
        if self.uid_exact && self.uid != uid {
            return false;
        }
        if self.gid_exact && self.gid != gid {
            return false;
        }
        self.prefix.is_empty() || message.starts_with(&self.prefix)
    }
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static RULES: RwLock<Vec<AuditRule>> = RwLock::new(Vec::new());
static RECORDS: RwLock<VecDeque<AuditRecord>> = RwLock::new(VecDeque::new());
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);
static NEXT_FILTER_ID: AtomicU64 = AtomicU64::new(1);
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

fn should_log(type_: AuditType, uid: u32, gid: u32, message: &str) -> bool {
    let rules = RULES.read();
    if rules.is_empty() {
        return true;
    }
    rules.iter().any(|r| r.matches(type_, uid, gid, message))
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Release);
}

/// Register an audit filter rule (legacy prefix API).
pub fn register_rule(type_: AuditType, prefix: &str) {
    add_filter(AuditRule {
        id: 0,
        filter_type: AUDIT_FILTER_TYPE,
        type_,
        uid: u32::MAX,
        gid: u32::MAX,
        prefix: String::from(prefix),
        enabled: true,
        uid_exact: false,
        gid_exact: false,
    });
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

    crate::serial_println!(
        "[audit] {} pid={} uid={} gid={} ok={} {}",
        type_.tag(),
        pid,
        uid,
        gid,
        success,
        record.message
    );

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
    if !should_log(type_, effective_uid, cred_gid, message) {
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

/// Log a syscall audit event for the current task.
pub fn audit_log_syscall(syscall_nr: i32, args: &[u64; 6], result: i64) {
    if !is_enabled() {
        return;
    }
    let (pid, uid, gid) = current_creds();
    let success = result >= 0;
    let message = format!(
        "syscall={} args=[{:#x},{:#x},{:#x},{:#x},{:#x},{:#x}] ret={}",
        syscall_nr, args[0], args[1], args[2], args[3], args[4], args[5], result
    );
    if !should_log(AuditType::Syscall, uid, gid, &message) {
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

/// Log a filesystem path audit event for the current task.
pub fn audit_log_path(op: AuditOp, path: &str, success: bool) {
    if !is_enabled() {
        return;
    }
    let (pid, uid, gid) = current_creds();
    let type_ = match op {
        AuditOp::Exec => AuditType::Path,
        _ => AuditType::Path,
    };
    let message = format!("op={:?} path={path}", op);
    if !should_log(type_, uid, gid, &message) {
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
        "enabled={}\nlost={}\nlogged={}\nfilters={}\nrecords={}\n",
        if is_enabled() { 1 } else { 0 },
        lost_count(),
        log_count(),
        filter_count(),
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

// ── /proc/sys/kernel/audit* ─────────────────────────────────────────────

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
    RECORDS.write().clear();
    NEXT_SEQ.store(1, Ordering::Release);
    NEXT_FILTER_ID.store(1, Ordering::Release);
    LOST.store(0, Ordering::Release);
    ENABLED.store(false, Ordering::Release);

    register_rule(AuditType::Syscall, "");
    register_rule(AuditType::Path, "/etc/");
    register_rule(AuditType::Cap, "");
    register_rule(AuditType::Kernel, "");

    add_filter(AuditRule {
        id: 0,
        filter_type: AUDIT_FILTER_USER,
        type_: AuditType::Syscall,
        uid: 0,
        gid: u32::MAX,
        prefix: String::new(),
        enabled: true,
        uid_exact: true,
        gid_exact: false,
    });

    crate::serial_println!(
        "[audit] audit subsystem initialized ({} rules)",
        filter_count()
    );
}
