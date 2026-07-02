//! Cachefiles network filesystem cache.
//!
//! Provides a backing cache layer that uses a local filesystem as a cache
//! for network filesystems (NFS, AFS, CIFS). The cache stores copies of
//! remote files locally so that subsequent reads can be served from the
//! backing filesystem instead of going over the network. When space runs
//! low the cache evicts the least-recently-used entries.
//!
//! See linux-master `fs/cachefiles/` for the reference implementation.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::cmp;
use lazy_static::lazy_static;
use spin::RwLock;

/// Default maximum cache size in bytes (256 MiB).
const DEFAULT_CACHE_LIMIT: u64 = 256 * 1024 * 1024;

/// Default minimum free space that must be preserved on the backing
/// filesystem before the cache will refuse new entries (16 MiB).
const DEFAULT_MIN_FREE: u64 = 16 * 1024 * 1024;

/// Metadata describing a file stored in the backing filesystem.
#[derive(Debug, Clone)]
pub struct BackingStat {
    /// File size in bytes.
    pub size: u64,
    /// Free space remaining on the backing filesystem in bytes.
    pub free_space: u64,
    /// Whether the file exists.
    pub exists: bool,
}

/// Trait abstracting the local filesystem used as the cache backing store.
///
/// This mirrors the role of the cachefiles "cache" directory in Linux: a
/// regular directory tree where cached objects are stored as plain files.
/// The default in-kernel implementation is [`InMemoryBackingFs`], which
/// keeps everything in RAM; a real block-backed filesystem would implement
/// this trait against its own inode store.
pub trait BackingFs: Send + Sync + core::fmt::Debug {
    /// Read the contents of `path` into `buf`, returning the number of
    /// bytes copied. Reads past EOF return zero.
    fn read_file(&self, path: &str, offset: u64, buf: &mut [u8]) -> FsResult<usize>;

    /// Write `data` to `path` at `offset`, growing the file as needed.
    fn write_file(&self, path: &str, offset: u64, data: &[u8]) -> FsResult<usize>;

    /// Delete `path` from the backing filesystem.
    fn delete_file(&self, path: &str) -> FsResult<()>;

    /// Stat `path`, returning size/free-space/existence information.
    fn stat(&self, path: &str) -> FsResult<BackingStat>;
}

/// In-memory backing filesystem used when no real block device is available.
///
/// Files are stored as `Vec<u8>` in a `BTreeMap` keyed by path. A fixed
/// capacity simulates the free-space accounting of a real disk.
#[derive(Debug)]
pub struct InMemoryBackingFs {
    files: RwLock<BTreeMap<String, Vec<u8>>>,
    /// Total virtual capacity in bytes.
    capacity: u64,
}

impl InMemoryBackingFs {
    /// Create a new in-memory backing filesystem with the given capacity.
    pub fn new(capacity: u64) -> Self {
        Self {
            files: RwLock::new(BTreeMap::new()),
            capacity,
        }
    }
}

impl BackingFs for InMemoryBackingFs {
    fn read_file(&self, path: &str, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        let files = self.files.read();
        let file = files.get(path).ok_or(FsError::NotFound)?;
        let len = file.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buf.len(), file.len());
        let n = end - start;
        buf[..n].copy_from_slice(&file[start..end]);
        Ok(n)
    }

    fn write_file(&self, path: &str, offset: u64, data: &[u8]) -> FsResult<usize> {
        let mut files = self.files.write();
        let file = files.entry(path.to_string()).or_default();
        let required = (offset as usize).saturating_add(data.len());
        if file.len() < required {
            file.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + data.len();
        file[start..end].copy_from_slice(data);
        Ok(data.len())
    }

    fn delete_file(&self, path: &str) -> FsResult<()> {
        self.files.write().remove(path);
        Ok(())
    }

    fn stat(&self, path: &str) -> FsResult<BackingStat> {
        let files = self.files.read();
        let (size, exists) = match files.get(path) {
            Some(f) => (f.len() as u64, true),
            None => (0, false),
        };
        let used = files.values().map(|f| f.len() as u64).sum();
        let free_space = self.capacity.saturating_sub(used);
        Ok(BackingStat {
            size,
            free_space,
            exists,
        })
    }
}

/// A single cached object tracked by [`CacheFilesCache`].
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Cache key (typically the network filesystem's object key).
    pub key: String,
    /// Path of the cached file within the backing filesystem.
    pub local_path: String,
    /// Size of the cached object in bytes.
    pub size: u64,
    /// Last access time (ms since boot) — used for LRU eviction.
    pub last_access_time: u64,
    /// Whether the entry has pending writes not yet flushed to backing fs.
    pub dirty: bool,
}

/// A cachefiles cache instance bound to a backing filesystem.
#[derive(Debug)]
pub struct CacheFilesCache {
    /// Backing filesystem used to store cached file contents.
    backing: Arc<dyn BackingFs>,
    /// Base directory inside the backing fs for this cache.
    cache_dir: String,
    /// Index of cached objects keyed by cache key.
    index: RwLock<BTreeMap<String, CacheEntry>>,
    /// Maximum total bytes the cache is allowed to consume.
    cache_limit: u64,
    /// Minimum free space (bytes) that must remain on the backing fs.
    min_free: u64,
}

impl CacheFilesCache {
    /// Create a new cache backed by `backing_fs` rooted at `cache_dir`.
    pub fn new(backing_fs: Arc<dyn BackingFs>, cache_dir: &str) -> Self {
        Self {
            backing: backing_fs,
            cache_dir: cache_dir.to_string(),
            index: RwLock::new(BTreeMap::new()),
            cache_limit: DEFAULT_CACHE_LIMIT,
            min_free: DEFAULT_MIN_FREE,
        }
    }

    /// Override the default cache size limit.
    pub fn with_cache_limit(mut self, limit: u64) -> Self {
        self.cache_limit = limit;
        self
    }

    /// Override the default minimum free-space requirement.
    pub fn with_min_free(mut self, min_free: u64) -> Self {
        self.min_free = min_free;
        self
    }

    /// Build the backing-filesystem path for a given cache key.
    fn local_path_for(&self, key: &str) -> String {
        // Sanitize the key into a path component. Replace path separators
        // and other unsafe characters so the key cannot escape cache_dir.
        let safe: String = key
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
                _ => '_',
            })
            .collect();
        format!("{}/{}", self.cache_dir, safe)
    }

    /// Total bytes currently consumed by cached objects.
    fn used(&self) -> u64 {
        self.index.read().values().map(|e| e.size).sum()
    }

    /// Look up a cached file by key, refreshing its LRU access time.
    ///
    /// Returns the backing-filesystem path of the cached object, or
    /// `None` if the key is not present in the cache.
    pub fn lookup(&self, key: &str) -> Option<String> {
        let mut index = self.index.write();
        if let Some(entry) = index.get_mut(key) {
            entry.last_access_time = get_current_time();
            return Some(entry.local_path.clone());
        }
        None
    }

    /// Store `data` in the cache under `key`, evicting older entries if
    /// the cache or backing filesystem is under space pressure.
    pub fn store(&self, key: &str, data: &[u8]) -> FsResult<()> {
        let size = data.len() as u64;

        // Evict until we have room for the new entry within the cache limit.
        if self.used().saturating_add(size) > self.cache_limit {
            self.cull(size);
        }

        // Also ensure the backing filesystem has enough free space.
        if !self.check_space() {
            self.cull(size);
        }

        let local_path = self.local_path_for(key);
        self.backing.write_file(&local_path, 0, data)?;

        let now = get_current_time();
        let mut index = self.index.write();
        index.insert(
            key.to_string(),
            CacheEntry {
                key: key.to_string(),
                local_path,
                size,
                last_access_time: now,
                dirty: false,
            },
        );
        Ok(())
    }

    /// Remove a cache entry and delete its backing file.
    pub fn invalidate(&self, key: &str) {
        let local_path = {
            let mut index = self.index.write();
            match index.remove(key) {
                Some(entry) => entry.local_path,
                None => return,
            }
        };
        let _ = self.backing.delete_file(&local_path);
    }

    /// Read cached data for `key` into `buf`, returning bytes read.
    pub fn read(&self, key: &str, buf: &mut [u8]) -> FsResult<usize> {
        let local_path = self.lookup(key).ok_or(FsError::NotFound)?;
        self.backing.read_file(&local_path, 0, buf)
    }

    /// Mark an entry as dirty (pending flush to backing fs).
    pub fn mark_dirty(&self, key: &str) {
        if let Some(entry) = self.index.write().get_mut(key) {
            entry.dirty = true;
        }
    }

    /// Return whether the backing filesystem has enough free space to
    /// satisfy the minimum-free requirement.
    pub fn check_space(&self) -> bool {
        // Stat the cache directory; for the in-memory backing fs the
        // directory may not exist yet, which we treat as "all free".
        match self.backing.stat(&self.cache_dir) {
            Ok(stat) => stat.free_space >= self.min_free,
            Err(_) => self.min_free == 0,
        }
    }

    /// Evict least-recently-used entries until at least `target_size`
    /// bytes of additional space are available within the cache limit.
    pub fn cull(&self, target_size: u64) {
        let mut index = self.index.write();
        let mut freed = 0u64;
        // Collect entries sorted by last_access_time ascending (oldest first).
        let mut entries: Vec<CacheEntry> = index.values().cloned().collect();
        entries.sort_by_key(|e| e.last_access_time);

        for entry in entries {
            if freed >= target_size {
                break;
            }
            // Evict the oldest entry.
            let _ = self.backing.delete_file(&entry.local_path);
            index.remove(&entry.key);
            freed += entry.size;
        }
    }

    /// Flush all dirty entries to the backing filesystem.
    ///
    /// For the in-memory backing fs writes are already persistent, so this
    /// simply clears the dirty flag. A real block-backed implementation
    /// would issue sync/writeback here.
    pub fn flush(&self) -> FsResult<()> {
        let mut index = self.index.write();
        for entry in index.values_mut() {
            entry.dirty = false;
        }
        Ok(())
    }

    /// Number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.index.read().len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.index.read().is_empty()
    }

    /// Resolve a synthetic inode number back to its cache key by scanning
    /// this cache's index. The inode is an FNV-1a hash of the key, so we
    /// match by recomputing the hash for each entry.
    fn key_for_inode(&self, inode: InodeNumber) -> FsResult<String> {
        let index = self.index.read();
        for entry in index.values() {
            if hash_inode(&entry.key) == inode {
                return Ok(entry.key.clone());
            }
        }
        Err(FsError::NotFound)
    }
}

impl FileSystem for CacheFilesCache {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::CacheFiles
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let index = self.index.read();
        let used: u64 = index.values().map(|e| e.size).sum();
        let count = index.len() as u64;
        let block_size = 4096u32;
        let total_blocks = self.cache_limit / block_size as u64;
        let used_blocks = (used + block_size as u64 - 1) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: 4096,
            free_inodes: 4096u64.saturating_sub(count),
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Treat the path as a cache key and create an empty cached object.
        self.store(path, &[])?;
        // Use a simple hash of the key as a synthetic inode number.
        Ok(hash_inode(path))
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        if self.lookup(path).is_none() {
            return Err(FsError::NotFound);
        }
        Ok(hash_inode(path))
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let key = self.key_for_inode(inode)?;
        let local_path = self.lookup(&key).ok_or(FsError::NotFound)?;
        self.backing.read_file(&local_path, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let key = self.key_for_inode(inode)?;
        let local_path = self.local_path_for(&key);
        let n = self.backing.write_file(&local_path, offset, buffer)?;
        let mut index = self.index.write();
        let now = get_current_time();
        let entry = index.entry(key.clone()).or_insert(CacheEntry {
            key: key.clone(),
            local_path: local_path.clone(),
            size: 0,
            last_access_time: now,
            dirty: false,
        });
        // Update size/last-access/dirty.
        if let Ok(stat) = self.backing.stat(&local_path) {
            entry.size = stat.size;
        }
        entry.last_access_time = now;
        entry.dirty = true;
        Ok(n)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let key = self.key_for_inode(inode)?;
        let index = self.index.read();
        let entry = index.get(&key).ok_or(FsError::NotFound)?;
        Ok(FileMetadata::new(inode, FileType::Regular, entry.size))
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // Cache entries don't support metadata changes.
        Ok(())
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        self.invalidate(path);
        Ok(())
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let index = self.index.read();
        let entries = index
            .values()
            .map(|e| DirectoryEntry {
                name: e.key.clone(),
                inode: hash_inode(&e.key),
                file_type: FileType::Regular,
            })
            .collect();
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        self.flush()
    }
}

/// Global registry of named cachefiles caches.
///
/// Mirrors Linux's `cachefiles_cache` list: network filesystems register
/// themselves against a named cache so that multiple netfs instances can
/// share a single backing store.
lazy_static! {
    static ref CACHE_REGISTRY: RwLock<BTreeMap<String, Arc<CacheFilesCache>>> =
        RwLock::new(BTreeMap::new());
}

/// Register a named cache in the global registry.
pub fn register_cache(name: &str, cache: Arc<CacheFilesCache>) -> FsResult<()> {
    let mut registry = CACHE_REGISTRY.write();
    if registry.contains_key(name) {
        return Err(FsError::AlreadyExists);
    }
    registry.insert(name.to_string(), cache);
    Ok(())
}

/// Remove a named cache from the global registry.
pub fn unregister_cache(name: &str) -> FsResult<()> {
    let mut registry = CACHE_REGISTRY.write();
    if registry.remove(name).is_none() {
        return Err(FsError::NotFound);
    }
    Ok(())
}

/// Look up a named cache in the global registry.
pub fn lookup_cache(name: &str) -> Option<Arc<CacheFilesCache>> {
    CACHE_REGISTRY.read().get(name).cloned()
}

/// Initialize the cachefiles subsystem.
///
/// Creates a default in-memory backing cache named "default" so that
/// network filesystems can start using the cache immediately. This
/// replaces the previous TODO stub.
pub fn cachefiles_init() -> FsResult<()> {
    let backing = Arc::new(InMemoryBackingFs::new(DEFAULT_CACHE_LIMIT));
    let cache = Arc::new(CacheFilesCache::new(backing, "/var/cache/fscache"));
    register_cache("default", cache)
}

/// Backwards-compatible alias matching the original stub's public name.
pub fn init() -> FsResult<()> {
    cachefiles_init()
}

/// Register a backing cache directory, creating a default in-memory
/// backing fs rooted at `cache_dir`. This replaces the previous stub
/// `register_backing` which always returned `NotSupported`.
pub fn register_backing(cache_dir: &str) -> FsResult<()> {
    let backing = Arc::new(InMemoryBackingFs::new(DEFAULT_CACHE_LIMIT));
    let cache = Arc::new(CacheFilesCache::new(backing, cache_dir));
    register_cache(cache_dir, cache)
}

/// Compute a synthetic inode number from a cache key.
///
/// Uses a simple FNV-1a hash so that `open`/`read`/`metadata` can round-trip
/// a key through an `InodeNumber` without storing a separate mapping.
fn hash_inode(key: &str) -> InodeNumber {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in key.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 {
        hash = 1;
    }
    hash
}

/// Legacy configuration structure retained for API compatibility.
#[derive(Debug, Clone)]
pub struct CacheFilesConfig {
    /// Cache directory path.
    pub cache_dir: String,
    /// Enable logging.
    pub enable_logging: bool,
}

impl Default for CacheFilesConfig {
    fn default() -> Self {
        Self {
            cache_dir: String::new(),
            enable_logging: false,
        }
    }
}

/// Legacy cache object metadata retained for API compatibility.
#[derive(Debug, Clone)]
pub struct CacheObject {
    /// Object key.
    pub key: Vec<u8>,
    /// Object size.
    pub size: u64,
}
