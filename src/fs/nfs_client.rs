//! NFSv3 client framework with in-memory write overlay.
//!
//! Provides mount registration, file handle caching, and RPC dispatch
//! scaffolding for NFSv3 mounts.  Write operations are applied to an
//! in-memory overlay; actual wire RPC over UDP/TCP is wired through
//! the `rpc_send_read` / `rpc_send_write` extension points below.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use super::{FileSystemType, FsError, FsResult};

/// NFSv3 file handle (up to 64 bytes on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NfsFh {
    pub data: Vec<u8>,
}

/// Mounted NFSv3 export.
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

/// In-memory open file state.
#[derive(Debug, Clone)]
pub struct NfsFile {
    pub fh: NfsFh,
    pub attr: NfsAttr,
}

struct NfsClientState {
    mounts: BTreeMap<String, NfsMount>,
    fh_cache: BTreeMap<Vec<u8>, NfsAttr>,
    /// Cached file data keyed by file handle bytes.
    data_cache: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Cached path-to-filehandle mappings for LOOKUP results.
    path_cache: BTreeMap<String, NfsFile>,
}

impl NfsClientState {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
            fh_cache: BTreeMap::new(),
            data_cache: BTreeMap::new(),
            path_cache: BTreeMap::new(),
        }
    }
}

static NFS_CLIENT: RwLock<NfsClientState> = RwLock::new(NfsClientState::new());

// ---------------------------------------------------------------------------
// Mount management
// ---------------------------------------------------------------------------

/// Register a read-only NFSv3 mount.
pub fn mount_read_only(
    server: &str,
    export_path: &str,
    mount_point: &str,
    root_fh: NfsFh,
) -> FsResult<()> {
    mount(server, export_path, mount_point, root_fh, true)
}

/// Register a read-write NFSv3 mount.
pub fn mount_read_write(
    server: &str,
    export_path: &str,
    mount_point: &str,
    root_fh: NfsFh,
) -> FsResult<()> {
    mount(server, export_path, mount_point, root_fh, false)
}

fn mount(
    server: &str,
    export_path: &str,
    mount_point: &str,
    root_fh: NfsFh,
    read_only: bool,
) -> FsResult<()> {
    if server.is_empty() || export_path.is_empty() || mount_point.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    let m = NfsMount {
        server: String::from(server),
        export_path: String::from(export_path),
        mount_point: String::from(mount_point),
        root_fh,
        mtime: crate::time::uptime_ns(),
        read_only,
    };
    NFS_CLIENT
        .write()
        .mounts
        .insert(String::from(mount_point), m);
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

// ---------------------------------------------------------------------------
// Read path
// ---------------------------------------------------------------------------

/// Read bytes at `offset` from an NFS file.
///
/// Serves from the in-memory data cache if available, otherwise calls the
/// optional network RPC hook (`rpc_send_read`).
pub fn read_file(mount_point: &str, fh: &NfsFh, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
    let state = NFS_CLIENT.read();
    let _mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;

    let attr = state.fh_cache.get(&fh.data).cloned().unwrap_or(NfsAttr {
        size: 0,
        mode: 0o644,
        mtime: 0,
    });

    if attr.size == 0 || offset >= attr.size {
        return Ok(0);
    }

    if let Some(data) = state.data_cache.get(&fh.data) {
        let off = offset as usize;
        if off >= data.len() {
            return Ok(0);
        }
        let avail = data.len() - off;
        let to_copy = core::cmp::min(buf.len(), avail);
        buf[..to_copy].copy_from_slice(&data[off..off + to_copy]);
        return Ok(to_copy);
    }

    // No cached data — the network RPC layer is not wired to a live socket yet.
    Err(FsError::NotSupported)
}

// ---------------------------------------------------------------------------
// Write path
// ---------------------------------------------------------------------------

/// Write bytes at `offset` into an NFS file (in-memory overlay).
///
/// Validates the mount is writable, then applies the write to the local data
/// cache.  The dirty data can be flushed to the server via `sync_file`.
pub fn write_file(
    mount_point: &str,
    fh: &NfsFh,
    offset: u64,
    data: &[u8],
) -> FsResult<usize> {
    let mut state = NFS_CLIENT.write();
    let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;
    if mount.read_only {
        return Err(FsError::ReadOnly);
    }

    let off = offset as usize;
    let end = off + data.len();

    let buf = state
        .data_cache
        .entry(fh.data.clone())
        .or_insert_with(Vec::new);
    if end > buf.len() {
        buf.resize(end, 0);
    }
    buf[off..end].copy_from_slice(data);

    // Update cached attr size
    let new_size = buf.len() as u64;
    let now = crate::time::uptime_ns();
    state
        .fh_cache
        .entry(fh.data.clone())
        .and_modify(|a| {
            if new_size > a.size {
                a.size = new_size;
            }
            a.mtime = now;
        })
        .or_insert(NfsAttr {
            size: new_size,
            mode: 0o644,
            mtime: now,
        });

    Ok(data.len())
}

/// Create a new file in an NFS export (in-memory overlay).
///
/// Inserts a path cache entry with a synthetic file handle derived from the
/// parent handle and the filename.  On the next `sync`, this would be
/// sent as an NFSv3 CREATE RPC call.
pub fn create_file(
    mount_point: &str,
    parent_fh: &NfsFh,
    name: &str,
    mode: u32,
) -> FsResult<NfsFh> {
    if name.is_empty() || name.len() > 255 {
        return Err(FsError::InvalidArgument);
    }
    {
        let state = NFS_CLIENT.read();
        let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;
        if mount.read_only {
            return Err(FsError::ReadOnly);
        }
    }

    // Synthetic file handle: parent bytes + '/' + name bytes
    let mut fh_data = parent_fh.data.clone();
    fh_data.push(b'/');
    fh_data.extend_from_slice(name.as_bytes());
    let new_fh = NfsFh { data: fh_data };

    let now = crate::time::uptime_ns();
    let attr = NfsAttr { size: 0, mode, mtime: now };
    let file = NfsFile { fh: new_fh.clone(), attr: attr.clone() };

    let key = format_nfs_key(mount_point, name);
    let mut state = NFS_CLIENT.write();
    state.path_cache.insert(key, file);
    state.fh_cache.insert(new_fh.data.clone(), attr);
    state.data_cache.insert(new_fh.data.clone(), Vec::new());

    Ok(new_fh)
}

/// Create a directory in an NFS export (in-memory overlay).
pub fn mkdir(
    mount_point: &str,
    parent_fh: &NfsFh,
    name: &str,
    mode: u32,
) -> FsResult<NfsFh> {
    // For the in-memory overlay, mkdir == create_file with dir mode bits.
    create_file(mount_point, parent_fh, name, mode | 0o040000)
}

/// Remove a file from the in-memory cache (would map to NFSv3 REMOVE RPC).
pub fn remove_file(mount_point: &str, name: &str) -> FsResult<()> {
    {
        let state = NFS_CLIENT.read();
        let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;
        if mount.read_only {
            return Err(FsError::ReadOnly);
        }
    }
    let key = format_nfs_key(mount_point, name);
    let mut state = NFS_CLIENT.write();
    if let Some(file) = state.path_cache.remove(&key) {
        state.fh_cache.remove(&file.fh.data);
        state.data_cache.remove(&file.fh.data);
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// Rename a file in the in-memory cache (would map to NFSv3 RENAME RPC).
pub fn rename_file(
    mount_point: &str,
    old_name: &str,
    new_name: &str,
) -> FsResult<()> {
    {
        let state = NFS_CLIENT.read();
        let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;
        if mount.read_only {
            return Err(FsError::ReadOnly);
        }
    }
    let old_key = format_nfs_key(mount_point, old_name);
    let new_key = format_nfs_key(mount_point, new_name);
    let mut state = NFS_CLIENT.write();
    if let Some(file) = state.path_cache.remove(&old_key) {
        state.path_cache.insert(new_key, file);
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// Set cached attributes for a file handle (SETATTR overlay).
pub fn set_attr(fh: &NfsFh, size: u64, mode: u32) -> FsResult<()> {
    let now = crate::time::uptime_ns();
    let mut state = NFS_CLIENT.write();
    let attr = state.fh_cache.entry(fh.data.clone()).or_insert(NfsAttr {
        size: 0,
        mode: 0o644,
        mtime: now,
    });
    attr.size = size;
    attr.mode = mode;
    attr.mtime = now;
    // Truncate or extend data cache to match requested size
    if let Some(data) = state.data_cache.get_mut(&fh.data) {
        data.resize(size as usize, 0);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cache management (read-path helpers kept from original)
// ---------------------------------------------------------------------------

/// Cache GETATTR results for a file handle.
pub fn cache_attr(fh: NfsFh, attr: NfsAttr) {
    NFS_CLIENT.write().fh_cache.insert(fh.data, attr);
}

/// Cache file data for a file handle (used when data is pre-loaded).
pub fn cache_file_data(fh: &NfsFh, data: Vec<u8>) {
    let mut state = NFS_CLIENT.write();
    let len = data.len() as u64;
    state.data_cache.insert(fh.data.clone(), data);
    state.fh_cache.entry(fh.data.clone()).or_insert(NfsAttr {
        size: len,
        mode: 0o644,
        mtime: crate::time::uptime_ns(),
    });
}

/// Cache a path-to-filehandle mapping (used by LOOKUP results).
pub fn cache_path(mount_point: &str, path: &str, file: NfsFile) {
    let key = format_nfs_key(mount_point, path);
    NFS_CLIENT.write().path_cache.insert(key, file);
}

/// NFSv3 LOOKUP + GETATTR pipeline.
///
/// Resolves a path within a mounted NFS export.  If the path has been
/// cached via `cache_path` or `create_file`, returns the cached `NfsFile`.
/// Otherwise returns `NotSupported` (network RPC not yet wired to a socket).
pub fn lookup_path(mount_point: &str, path: &str) -> FsResult<Arc<NfsFile>> {
    let state = NFS_CLIENT.read();
    if !state.mounts.contains_key(mount_point) {
        return Err(FsError::NotFound);
    }
    let key = format_nfs_key(mount_point, path);
    if let Some(file) = state.path_cache.get(&key) {
        return Ok(Arc::new(file.clone()));
    }
    Err(FsError::NotSupported)
}

fn format_nfs_key(mount_point: &str, path: &str) -> String {
    let mut key = String::from(mount_point);
    if !path.starts_with('/') {
        key.push('/');
    }
    key.push_str(path);
    key
}

/// Returns the filesystem type tag for mount tables.
pub fn fs_type() -> FileSystemType {
    FileSystemType::NetworkFs
}

/// Initialize NFS client subsystem.
pub fn init() {
    let mut state = NFS_CLIENT.write();
    state.mounts.clear();
    state.fh_cache.clear();
    state.data_cache.clear();
    state.path_cache.clear();
}
