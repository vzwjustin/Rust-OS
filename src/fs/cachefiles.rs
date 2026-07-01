//! Cachefiles network filesystem cache stub.
//!
//! Provides a stub for cachefiles, a kernel subsystem that serves as a
//! cache backing for network filesystems (NFS, AFS, CIFS). Real implementation
//! would manage a local filesystem cache and coordinate with the network
//! filesystem driver. See linux-master fs/cachefiles/ for reference.

// TODO: port from linux-master fs/cachefiles/

/// CacheFiles configuration (stub).
#[derive(Debug, Clone)]
pub struct CacheFilesConfig {
    /// Cache directory path (stub)
    pub cache_dir: alloc::string::String,
    /// Enable logging (stub)
    pub enable_logging: bool,
}

impl Default for CacheFilesConfig {
    fn default() -> Self {
        Self {
            cache_dir: alloc::string::String::new(),
            enable_logging: false,
        }
    }
}

/// CacheFiles object metadata (stub).
#[derive(Debug, Clone)]
pub struct CacheObject {
    /// Object key (stub)
    pub key: alloc::vec::Vec<u8>,
    /// Object size (stub)
    pub size: u64,
}

/// Initialize cachefiles subsystem (stub).
pub fn init() -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/cachefiles/main.c cachefiles_init()
    Ok(())
}

/// Register a cachefiles-backed filesystem (stub).
pub fn register_backing(_cache_dir: &str) -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/cachefiles/rdwr.c
    Err(crate::fs::FsError::NotSupported)
}
