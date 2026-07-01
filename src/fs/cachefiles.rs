//! Cachefiles network filesystem cache.
//!
//! Provides cachefiles, a kernel subsystem that serves as a cache backing for
//! network filesystems (NFS, AFS, CIFS). This implementation tracks the cache
//! size, used blocks, and cached pages in memory, keyed by a cache object key.
//! Each registered backing directory has its own `CacheFilesState`.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

use crate::fs::{FsError, FsResult};

/// Default block size for cache accounting (4 KiB).
pub const CACHEFILES_BLOCK_SIZE: u64 = 4096;

/// CacheFiles configuration.
#[derive(Debug, Clone)]
pub struct CacheFilesConfig {
    /// Cache directory path.
    pub cache_dir: String,
    /// Enable logging.
    pub enable_logging: bool,
    /// Maximum cache size in blocks (0 = unlimited).
    pub max_blocks: u64,
}

impl Default for CacheFilesConfig {
    fn default() -> Self {
        Self {
            cache_dir: String::new(),
            enable_logging: false,
            max_blocks: 0,
        }
    }
}

/// CacheFiles object metadata.
#[derive(Debug, Clone)]
pub struct CacheObject {
    /// Object key (opaque identifier supplied by the network filesystem).
    pub key: Vec<u8>,
    /// Object size in bytes.
    pub size: u64,
    /// Number of blocks occupied (rounded up from `size`).
    pub blocks: u64,
    /// Cached page data for the object, keyed by page index.
    pub pages: BTreeMap<u64, Vec<u8>>,
}

impl CacheObject {
    fn new(key: Vec<u8>, size: u64) -> Self {
        let blocks = (size + CACHEFILES_BLOCK_SIZE - 1) / CACHEFILES_BLOCK_SIZE;
        Self {
            key,
            size,
            blocks,
            pages: BTreeMap::new(),
        }
    }
}

/// Per-backing-directory cache state.
#[derive(Debug)]
struct CacheFilesState {
    /// Configuration.
    config: CacheFilesConfig,
    /// Cached objects keyed by their object key bytes.
    objects: BTreeMap<Vec<u8>, CacheObject>,
    /// Total blocks currently in use across all objects.
    used_blocks: u64,
    /// Total cached pages across all objects.
    cached_pages: u64,
}

/// Global table of cachefiles backings keyed by cache directory path.
static CACHEFILES_TABLE: RwLock<BTreeMap<String, CacheFilesState>> = RwLock::new(BTreeMap::new());

/// Initialize the cachefiles subsystem.
///
/// Clears all registered backing caches.
pub fn init() -> FsResult<()> {
    CACHEFILES_TABLE.write().clear();
    Ok(())
}

/// Register a cachefiles-backed filesystem.
///
/// Creates an empty cache state for the given directory. If a backing for
/// `cache_dir` already exists it is replaced (the old cache contents are
/// dropped).
pub fn register_backing(cache_dir: &str) -> FsResult<()> {
    if cache_dir.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    let state = CacheFilesState {
        config: CacheFilesConfig {
            cache_dir: cache_dir.to_string(),
            ..CacheFilesConfig::default()
        },
        objects: BTreeMap::new(),
        used_blocks: 0,
        cached_pages: 0,
    };
    CACHEFILES_TABLE.write().insert(cache_dir.to_string(), state);
    Ok(())
}

/// Register a backing with an explicit configuration.
pub fn register_backing_with_config(config: CacheFilesConfig) -> FsResult<()> {
    if config.cache_dir.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    let dir = config.cache_dir.clone();
    let state = CacheFilesState {
        config,
        objects: BTreeMap::new(),
        used_blocks: 0,
        cached_pages: 0,
    };
    CACHEFILES_TABLE.write().insert(dir, state);
    Ok(())
}

/// Unregister a cachefiles backing, dropping all cached objects.
pub fn unregister_backing(cache_dir: &str) -> FsResult<()> {
    CACHEFILES_TABLE
        .write()
        .remove(cache_dir)
        .ok_or(FsError::NotFound)?;
    Ok(())
}

/// Insert (or replace) a cached object in a backing cache.
///
/// Returns `NoSpaceLeft` if adding the object would exceed the configured
/// `max_blocks` (when non-zero).
pub fn cache_object(cache_dir: &str, key: Vec<u8>, size: u64) -> FsResult<()> {
    let mut table = CACHEFILES_TABLE.write();
    let state = table.get_mut(cache_dir).ok_or(FsError::NotFound)?;

    let new_obj = CacheObject::new(key.clone(), size);
    // If replacing an existing object, subtract its old block usage first.
    if let Some(old) = state.objects.get(&key) {
        state.used_blocks = state.used_blocks.saturating_sub(old.blocks);
        state.cached_pages = state.cached_pages.saturating_sub(old.pages.len() as u64);
    }

    if state.config.max_blocks > 0
        && state.used_blocks + new_obj.blocks > state.config.max_blocks
    {
        return Err(FsError::NoSpaceLeft);
    }

    state.used_blocks += new_obj.blocks;
    state.objects.insert(key, new_obj);
    Ok(())
}

/// Store a page of data for a cached object.
pub fn cache_page(
    cache_dir: &str,
    key: &[u8],
    page_index: u64,
    data: &[u8],
) -> FsResult<()> {
    let mut table = CACHEFILES_TABLE.write();
    let state = table.get_mut(cache_dir).ok_or(FsError::NotFound)?;
    let obj = state.objects.get_mut(key).ok_or(FsError::NotFound)?;
    let was_present = obj.pages.contains_key(&page_index);
    obj.pages.insert(page_index, data.to_vec());
    if !was_present {
        state.cached_pages += 1;
    }
    Ok(())
}

/// Read a cached page for an object.
///
/// Returns the page bytes, or `NotFound` if the page is not cached.
pub fn read_page(
    cache_dir: &str,
    key: &[u8],
    page_index: u64,
) -> FsResult<Vec<u8>> {
    let table = CACHEFILES_TABLE.read();
    let state = table.get(cache_dir).ok_or(FsError::NotFound)?;
    let obj = state.objects.get(key).ok_or(FsError::NotFound)?;
    obj.pages.get(&page_index).cloned().ok_or(FsError::NotFound)
}

/// Invalidate (drop) a cached page for an object.
pub fn invalidate_page(
    cache_dir: &str,
    key: &[u8],
    page_index: u64,
) -> FsResult<()> {
    let mut table = CACHEFILES_TABLE.write();
    let state = table.get_mut(cache_dir).ok_or(FsError::NotFound)?;
    let obj = state.objects.get_mut(key).ok_or(FsError::NotFound)?;
    if obj.pages.remove(&page_index).is_some() {
        state.cached_pages = state.cached_pages.saturating_sub(1);
    }
    Ok(())
}

/// Remove a cached object from a backing cache.
pub fn invalidate_object(cache_dir: &str, key: &[u8]) -> FsResult<()> {
    let mut table = CACHEFILES_TABLE.write();
    let state = table.get_mut(cache_dir).ok_or(FsError::NotFound)?;
    if let Some(old) = state.objects.remove(key) {
        state.used_blocks = state.used_blocks.saturating_sub(old.blocks);
        state.cached_pages = state.cached_pages.saturating_sub(old.pages.len() as u64);
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// Get the number of blocks currently in use for a backing cache.
pub fn used_blocks(cache_dir: &str) -> FsResult<u64> {
    let table = CACHEFILES_TABLE.read();
    let state = table.get(cache_dir).ok_or(FsError::NotFound)?;
    Ok(state.used_blocks)
}

/// Get the number of cached pages for a backing cache.
pub fn cached_pages(cache_dir: &str) -> FsResult<u64> {
    let table = CACHEFILES_TABLE.read();
    let state = table.get(cache_dir).ok_or(FsError::NotFound)?;
    Ok(state.cached_pages)
}

/// Get the number of cached objects for a backing cache.
pub fn object_count(cache_dir: &str) -> FsResult<usize> {
    let table = CACHEFILES_TABLE.read();
    let state = table.get(cache_dir).ok_or(FsError::NotFound)?;
    Ok(state.objects.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_cache_object() {
        init().unwrap();
        register_backing("/var/cache/nfs").unwrap();
        cache_object("/var/cache/nfs", b"file1".to_vec(), 8192).unwrap();
        assert_eq!(object_count("/var/cache/nfs").unwrap(), 1);
        assert_eq!(used_blocks("/var/cache/nfs").unwrap(), 2);
        cache_page("/var/cache/nfs", b"file1", 0, &[1, 2, 3]).unwrap();
        assert_eq!(cached_pages("/var/cache/nfs").unwrap(), 1);
        let page = read_page("/var/cache/nfs", b"file1", 0).unwrap();
        assert_eq!(page, vec![1, 2, 3]);
        invalidate_object("/var/cache/nfs", b"file1").unwrap();
        assert_eq!(used_blocks("/var/cache/nfs").unwrap(), 0);
    }

    #[test]
    fn test_max_blocks_enforced() {
        init().unwrap();
        let cfg = CacheFilesConfig {
            cache_dir: "/c".to_string(),
            enable_logging: false,
            max_blocks: 1,
        };
        register_backing_with_config(cfg).unwrap();
        // 2 blocks > max of 1.
        assert_eq!(
            cache_object("/c", b"k".to_vec(), 8192),
            Err(FsError::NoSpaceLeft)
        );
    }
}
