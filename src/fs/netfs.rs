//! Network filesystem helper library.
//!
//! Provides the netfs subsystem, a shared helper library for network-based
//! filesystems (NFS, CIFS, AFS, etc.) to perform read/write operations with
//! caching and buffer management. This implementation tracks per-mount server
//! connection state, mount options, and a page cache keyed by `(inode, offset)`
//! so that repeated reads of the same region are served from memory.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

use crate::fs::{FsError, FsResult};

/// Page-cache page size (4096 bytes).
pub const NETFS_PAGE_SIZE: u64 = 4096;

/// Network filesystem I/O request.
#[derive(Debug, Clone)]
pub struct NetFsRequest {
    /// Request type (e.g., read, write). Mirrors `netfs_io_source`.
    pub request_type: u32,
    /// File offset (must be page-aligned for cached reads).
    pub offset: u64,
    /// Length of I/O in bytes.
    pub length: u64,
    /// Inode the request targets.
    pub inode: u64,
}

/// Read request type constant.
pub const NETFS_READ: u32 = 0;
/// Write request type constant.
pub const NETFS_WRITE: u32 = 1;

/// Network filesystem cache state for a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// Data is cached and valid.
    Cached,
    /// Data is not cached.
    NotCached,
    /// Cache state unknown (needs revalidation).
    Unknown,
}

/// A cached page keyed by `(inode, page_index)`.
#[derive(Debug, Clone)]
struct CachedPage {
    /// The page data (always `NETFS_PAGE_SIZE` bytes).
    data: Vec<u8>,
    /// Current cache state.
    state: CacheState,
}

/// Server connection state for a network filesystem mount.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    /// Server hostname or address.
    pub server_addr: String,
    /// Server port.
    pub port: u16,
    /// Whether the connection is currently established.
    pub connected: bool,
    /// Mount/export path on the server.
    pub export_path: String,
}

/// Mount options for a network filesystem.
#[derive(Debug, Clone)]
pub struct MountOptions {
    /// Read-only mount.
    pub read_only: bool,
    /// Enable page caching.
    pub cache_enabled: bool,
    /// Revalidation timeout in ticks (0 = always revalidate).
    pub reval_timeout: u64,
    /// Soft mount (fail on server timeout) vs hard mount (retry forever).
    pub soft: bool,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            cache_enabled: true,
            reval_timeout: 60,
            soft: false,
        }
    }
}

/// Per-mount netfs state.
#[derive(Debug)]
struct NetFsMount {
    /// Server connection info.
    connection: ServerConnection,
    /// Mount options.
    options: MountOptions,
    /// Page cache keyed by `(inode, page_index)`.
    pages: BTreeMap<(u64, u64), CachedPage>,
}

/// Global table of netfs mounts keyed by a mount id.
static NETFS_MOUNTS: RwLock<BTreeMap<u32, NetFsMount>> = RwLock::new(BTreeMap::new());
/// Next mount id allocator.
static NEXT_MOUNT_ID: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(1);

/// Initialize the netfs subsystem.
///
/// Clears all registered mounts and their caches.
pub fn init() -> FsResult<()> {
    NETFS_MOUNTS.write().clear();
    Ok(())
}

/// Register a network filesystem mount and return its mount id.
///
/// The connection starts in the `connected` state if `server_addr` is non-empty.
pub fn register_mount(
    server_addr: &str,
    port: u16,
    export_path: &str,
    options: MountOptions,
) -> FsResult<u32> {
    let id = NEXT_MOUNT_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    let mount = NetFsMount {
        connection: ServerConnection {
            server_addr: server_addr.to_string(),
            port,
            connected: !server_addr.is_empty(),
            export_path: export_path.to_string(),
        },
        options,
        pages: BTreeMap::new(),
    };
    NETFS_MOUNTS.write().insert(id, mount);
    Ok(id)
}

/// Unregister a network filesystem mount, dropping its cache.
pub fn unregister_mount(mount_id: u32) -> FsResult<()> {
    NETFS_MOUNTS
        .write()
        .remove(&mount_id)
        .ok_or(FsError::NotFound)?;
    Ok(())
}

/// Get a snapshot of the server connection for a mount.
pub fn get_connection(mount_id: u32) -> FsResult<ServerConnection> {
    let mounts = NETFS_MOUNTS.read();
    let m = mounts.get(&mount_id).ok_or(FsError::NotFound)?;
    Ok(m.connection.clone())
}

/// Mark a mount's server connection as connected/disconnected.
pub fn set_connected(mount_id: u32, connected: bool) -> FsResult<()> {
    let mut mounts = NETFS_MOUNTS.write();
    let m = mounts.get_mut(&mount_id).ok_or(FsError::NotFound)?;
    m.connection.connected = connected;
    Ok(())
}

/// Look up the cache state of a page without fetching its data.
pub fn page_state(mount_id: u32, inode: u64, offset: u64) -> FsResult<CacheState> {
    let mounts = NETFS_MOUNTS.read();
    let m = mounts.get(&mount_id).ok_or(FsError::NotFound)?;
    if !m.options.cache_enabled {
        return Ok(CacheState::NotCached);
    }
    let page_index = offset / NETFS_PAGE_SIZE;
    Ok(m.pages
        .get(&(inode, page_index))
        .map(|p| p.state)
        .unwrap_or(CacheState::NotCached))
}

/// Insert (or replace) a cached page for a mount.
pub fn cache_page(mount_id: u32, inode: u64, offset: u64, data: &[u8]) -> FsResult<()> {
    let mut mounts = NETFS_MOUNTS.write();
    let m = mounts.get_mut(&mount_id).ok_or(FsError::NotFound)?;
    if !m.options.cache_enabled {
        return Ok(());
    }
    let page_index = offset / NETFS_PAGE_SIZE;
    let mut page_data = alloc::vec![0u8; NETFS_PAGE_SIZE as usize];
    let copy_len = core::cmp::min(data.len(), page_data.len());
    page_data[..copy_len].copy_from_slice(&data[..copy_len]);
    m.pages.insert(
        (inode, page_index),
        CachedPage {
            data: page_data,
            state: CacheState::Cached,
        },
    );
    Ok(())
}

/// Invalidate a cached page (e.g. after a server-side modification).
pub fn invalidate_page(mount_id: u32, inode: u64, offset: u64) -> FsResult<()> {
    let mut mounts = NETFS_MOUNTS.write();
    let m = mounts.get_mut(&mount_id).ok_or(FsError::NotFound)?;
    let page_index = offset / NETFS_PAGE_SIZE;
    m.pages.remove(&(inode, page_index));
    Ok(())
}

/// Number of pages currently cached for a mount.
pub fn cached_page_count(mount_id: u32) -> FsResult<usize> {
    let mounts = NETFS_MOUNTS.read();
    let m = mounts.get(&mount_id).ok_or(FsError::NotFound)?;
    Ok(m.pages.len())
}

/// Submit a netfs read request.
///
/// If the requested page is present in the cache and the mount is cache-enabled,
/// the data is served from the page cache. Otherwise the request is recorded
/// against the server connection: if the server is not connected, `IoError` is
/// returned; if connected, the requested `length` is returned as the number of
/// bytes that would be transferred (the caller fills the buffer from the wire).
pub fn submit_read(req: &NetFsRequest) -> FsResult<usize> {
    // Without a mount id in the request we cannot look up a specific mount, so
    // we use the first registered mount as the default target. This mirrors the
    // single-mount-per-call model used by the simpler netfs helpers.
    let mut mounts = NETFS_MOUNTS.write();
    let (_id, m) = mounts.iter_mut().next().ok_or(FsError::NotFound)?;
    let _ = _id;

    if !m.connection.connected {
        return Err(FsError::IoError);
    }

    if m.options.cache_enabled {
        let page_index = req.offset / NETFS_PAGE_SIZE;
        if let Some(page) = m.pages.get(&(req.inode, page_index)) {
            if page.state == CacheState::Cached {
                // Serve from cache: return up to `length` bytes from the page.
                let page_offset = (req.offset % NETFS_PAGE_SIZE) as usize;
                let available = page.data.len().saturating_sub(page_offset);
                let to_read = core::cmp::min(req.length as usize, available);
                return Ok(to_read);
            }
        }
    }

    // Cache miss: the wire transfer would read `length` bytes.
    Ok(req.length as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_cache() {
        init().unwrap();
        let id = register_mount("10.0.0.1", 2049, "/export", MountOptions::default()).unwrap();
        let conn = get_connection(id).unwrap();
        assert!(conn.connected);
        assert_eq!(page_state(id, 1, 0).unwrap(), CacheState::NotCached);
        let data = [0xAAu8; NETFS_PAGE_SIZE as usize];
        cache_page(id, 1, 0, &data).unwrap();
        assert_eq!(page_state(id, 1, 0).unwrap(), CacheState::Cached);
        assert_eq!(cached_page_count(id).unwrap(), 1);
        invalidate_page(id, 1, 0).unwrap();
        assert_eq!(page_state(id, 1, 0).unwrap(), CacheState::NotCached);
        unregister_mount(id).unwrap();
    }

    #[test]
    fn test_submit_read_disconnected() {
        init().unwrap();
        let id = register_mount("server", 2049, "/x", MountOptions::default()).unwrap();
        set_connected(id, false).unwrap();
        let req = NetFsRequest {
            request_type: NETFS_READ,
            offset: 0,
            length: 100,
            inode: 1,
        };
        assert_eq!(submit_read(&req), Err(FsError::IoError));
    }
}
