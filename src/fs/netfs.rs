//! Network filesystem helper library stub.
//!
//! Provides a stub for the netfs subsystem, which is a shared helper library
//! for network-based filesystems (NFS, CIFS, AFS, etc.) to perform read/write
//! operations with caching and buffer management. Real implementation would
//! coordinate I/O between the VFS and network protocol handlers.
//! See linux-master fs/netfs/ for reference.

// TODO: port from linux-master fs/netfs/

/// Network filesystem I/O request (stub).
#[derive(Debug, Clone)]
pub struct NetFsRequest {
    /// Request type (e.g., read, write) (stub)
    pub request_type: u32,
    /// File offset (stub)
    pub offset: u64,
    /// Length of I/O (stub)
    pub length: u64,
}

/// Network filesystem cache state (stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// Data is cached and valid (stub)
    Cached,
    /// Data is not cached (stub)
    NotCached,
    /// Cache state unknown (stub)
    Unknown,
}

/// Initialize netfs subsystem (stub).
pub fn init() -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/netfs/main.c netfs_init()
    Ok(())
}

/// Submit a netfs read request (stub).
pub fn submit_read(_req: &NetFsRequest) -> crate::fs::FsResult<usize> {
    // TODO: port from linux-master fs/netfs/read_helper.c netfs_submit_read()
    Err(crate::fs::FsError::NotSupported)
}
