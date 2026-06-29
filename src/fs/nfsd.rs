//! NFS server export registry (read-only export table).
//!
//! Maintains the set of filesystem paths exported over NFS and their
//! export options. Actual RPC dispatch is deferred; this module provides
//! the kernel-side export table used by mount integration and `/proc/fs/nfs/exports`.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

use super::FsError;
use super::FsResult;

/// NFS export flags (subset of Linux exportfs flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportFlags {
    pub read_only: bool,
    pub root_squash: bool,
    pub all_squash: bool,
    pub sync: bool,
    pub no_subtree_check: bool,
}

impl Default for ExportFlags {
    fn default() -> Self {
        Self {
            read_only: false,
            root_squash: true,
            all_squash: false,
            sync: true,
            no_subtree_check: true,
        }
    }
}

/// A single NFS export entry.
#[derive(Debug, Clone)]
pub struct NfsExport {
    pub path: String,
    pub clients: String,
    pub flags: ExportFlags,
    pub fsid: u32,
    pub anon_uid: u32,
    pub anon_gid: u32,
}

struct NfsdState {
    exports: BTreeMap<String, NfsExport>,
    next_fsid: u32,
}

impl NfsdState {
    const fn new() -> Self {
        Self {
            exports: BTreeMap::new(),
            next_fsid: 1,
        }
    }
}

static NFSD: RwLock<NfsdState> = RwLock::new(NfsdState::new());

/// Parse export options string (`rw,sync,no_subtree_check`, etc.).
pub fn parse_export_options(options: &str) -> ExportFlags {
    let mut flags = ExportFlags::default();
    for part in options.split(',') {
        match part.trim() {
            "ro" => flags.read_only = true,
            "rw" => flags.read_only = false,
            "root_squash" => flags.root_squash = true,
            "no_root_squash" => flags.root_squash = false,
            "all_squash" => flags.all_squash = true,
            "sync" => flags.sync = true,
            "async" => flags.sync = false,
            "no_subtree_check" => flags.no_subtree_check = true,
            "subtree_check" => flags.no_subtree_check = false,
            _ => {}
        }
    }
    flags
}

/// Add or replace an export for `path` visible to `clients`.
pub fn add_export(
    path: &str,
    clients: &str,
    options: &str,
    anon_uid: u32,
    anon_gid: u32,
) -> FsResult<u32> {
    if path.is_empty() || !path.starts_with('/') {
        return Err(FsError::InvalidArgument);
    }

    let mut state = NFSD.write();
    let fsid = if let Some(existing) = state.exports.get(path) {
        existing.fsid
    } else {
        let id = state.next_fsid;
        state.next_fsid = state.next_fsid.saturating_add(1);
        id
    };

    let export = NfsExport {
        path: String::from(path),
        clients: String::from(clients),
        flags: parse_export_options(options),
        fsid,
        anon_uid,
        anon_gid,
    };
    state.exports.insert(String::from(path), export);
    Ok(fsid)
}

/// Remove an export by filesystem path.
pub fn remove_export(path: &str) -> FsResult<()> {
    if NFSD.write().exports.remove(path).is_some() {
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// List all registered exports (read-only snapshot).
pub fn list_exports() -> Vec<NfsExport> {
    NFSD.read().exports.values().cloned().collect()
}

/// Check whether `path` is exported and return the entry if found.
pub fn lookup_export(path: &str) -> Option<NfsExport> {
    let state = NFSD.read();
    state.exports.get(path).cloned().or_else(|| {
        state
            .exports
            .iter()
            .filter(|(p, _)| path.starts_with(&format!("{}/", p)))
            .max_by_key(|(p, _)| p.len())
            .map(|(_, e)| e.clone())
    })
}

/// Returns `/proc/fs/nfs/exports`-style content.
pub fn exports_proc_content() -> String {
    let mut out = String::new();
    for export in list_exports() {
        let opts = if export.flags.read_only { "ro" } else { "rw" };
        out.push_str(&format!(
            "{} {}({},{}{})\n",
            export.path,
            export.clients,
            opts,
            if export.flags.sync { "sync" } else { "async" },
            if export.flags.root_squash {
                ",root_squash"
            } else {
                ",no_root_squash"
            },
        ));
    }
    out
}

/// Initialize NFS server export table.
pub fn init() {
    let mut state = NFSD.write();
    state.exports.clear();
    state.next_fsid = 1;
}
