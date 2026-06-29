//! NFSv3 read-only client framework.
//!
//! Provides mount registration, file handle caching, and READ/RPC dispatch
//! scaffolding for read-only NFSv3 mounts. Full wire RPC over UDP/TCP is
//! deferred; this module defines the integration points with the VFS.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use super::{FileSystemType, FsError, FsResult};
use crate::vfs::{VfsError, VfsResult};

/// NFSv3 file handle (up to 64 bytes on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NfsFh {
    pub data: Vec<u8>,
}

/// Mounted NFSv3 export (read-only).
#[derive(Debug, Clone)]
pub struct NfsMount {
    pub server: String,
    pub export_path: String,
    pub mount_point: String,
    pub root_fh: NfsFh,
    pub mtime: u64,
    pub read_only: bool,
}

/// Cached file attributes from GETATTR.
#[derive(Debug, Clone)]
pub struct NfsAttr {
    pub size: u64,
    pub mode: u32,
    pub mtime: u64,
}

/// In-memory open file state for NFS reads.
#[derive(Debug, Clone)]
pub struct NfsFile {
    pub fh: NfsFh,
    pub attr: NfsAttr,
}

struct NfsClientState {
    mounts: BTreeMap<String, NfsMount>,
    fh_cache: BTreeMap<Vec<u8>, NfsAttr>,
}

impl NfsClientState {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
            fh_cache: BTreeMap::new(),
        }
    }
}

static NFS_CLIENT: RwLock<NfsClientState> = RwLock::new(NfsClientState::new());

/// Register a read-only NFSv3 mount.
pub fn mount_read_only(
    server: &str,
    export_path: &str,
    mount_point: &str,
    root_fh: NfsFh,
) -> FsResult<()> {
    if server.is_empty() || export_path.is_empty() || mount_point.is_empty() {
        return Err(FsError::InvalidArgument);
    }

    let mount = NfsMount {
        server: String::from(server),
        export_path: String::from(export_path),
        mount_point: String::from(mount_point),
        root_fh,
        mtime: crate::time::uptime_ns(),
        read_only: true,
    };

    NFS_CLIENT
        .write()
        .mounts
        .insert(String::from(mount_point), mount);
    Ok(())
}

/// Unmount an NFS mount point.
pub fn unmount(mount_point: &str) -> FsResult<()> {
    if NFS_CLIENT.write().mounts.remove(mount_point).is_some() {
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// List active NFS mounts.
pub fn list_mounts() -> Vec<NfsMount> {
    NFS_CLIENT.read().mounts.values().cloned().collect()
}

/// Read bytes at `offset` from an NFS file (framework).
pub fn read_file(mount_point: &str, fh: &NfsFh, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
    let state = NFS_CLIENT.read();
    let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;

    if !mount.read_only {
        return Err(FsError::ReadOnly);
    }

    let attr = state.fh_cache.get(&fh.data).cloned().unwrap_or(NfsAttr {
        size: 0,
        mode: 0o644,
        mtime: 0,
    });

    if attr.size == 0 || offset >= attr.size {
        return Ok(0);
    }

    // Framework: NFSv3 READ RPC via mount.server would go here.
    let _ = (mount, buf);
    Err(FsError::NotSupported)
}

/// Cache GETATTR results for a file handle.
pub fn cache_attr(fh: NfsFh, attr: NfsAttr) {
    NFS_CLIENT.write().fh_cache.insert(fh.data, attr);
}

/// Returns the filesystem type tag for mount tables.
pub fn fs_type() -> FileSystemType {
    FileSystemType::NetworkFs
}

/// NFSv3 LOOKUP + GETATTR pipeline hook (framework).
pub fn lookup_path(_mount_point: &str, _path: &str) -> FsResult<Arc<NfsFile>> {
    Err(FsError::NotSupported)
}

/// Initialize NFS client subsystem.
pub fn init() {
    let mut state = NFS_CLIENT.write();
    state.mounts.clear();
    state.fh_cache.clear();
}
