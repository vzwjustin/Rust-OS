//! Disk I/O Buffer Management
//!
//! This module provides a sophisticated buffer cache system for disk I/O operations,
//! including read-ahead, write-back, and LRU eviction policies.

use crate::drivers::storage::{read_storage_sectors, write_storage_sectors, StorageError};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    vec,
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::{Mutex, RwLock};

/// Buffer cache configuration
const BUFFER_CACHE_SIZE: usize = 1024; // Number of buffers to cache
const BUFFER_SIZE: usize = 4096; // Size of each buffer (4KB)
const SECTORS_PER_BUFFER: usize = BUFFER_SIZE / 512; // 8 sectors per buffer
const MAX_DIRTY_BUFFERS: usize = 256; // Maximum dirty buffers before forced flush
const READ_AHEAD_BUFFERS: usize = 4; // Number of buffers to read ahead

/// Buffer states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    /// Buffer is clean and up-to-date
    Clean,
    /// Buffer has been modified and needs to be written back
    Dirty,
    /// Buffer is currently being read from disk
    Reading,
    /// Buffer is currently being written to disk
    Writing,
    /// Buffer contains invalid data
    Invalid,
}

/// Buffer descriptor
#[derive(Debug)]
pub struct Buffer {
    /// Device ID
    device_id: u32,
    /// Block number (in buffer-sized blocks)
    block_num: u64,
    /// Buffer data
    data: Vec<u8>,
    /// Buffer state
    state: BufferState,
    /// Reference count
    ref_count: u32,
    /// Last access time (for LRU)
    last_access: u64,
    /// Dirty flag
    dirty: bool,
}

impl Buffer {
    /// Create new buffer
    fn new(device_id: u32, block_num: u64) -> Self {
        Self {
            device_id,
            block_num,
            data: vec![0u8; BUFFER_SIZE],
            state: BufferState::Invalid,
            ref_count: 0,
            last_access: 0,
            dirty: false,
        }
    }

    /// Mark buffer as accessed
    fn touch(&mut self) {
        self.last_access = get_current_time();
    }

    /// Check if buffer can be evicted
    fn can_evict(&self) -> bool {
        self.ref_count == 0
            && self.state != BufferState::Reading
            && self.state != BufferState::Writing
    }
}

/// Buffer cache statistics
#[derive(Debug, Default, Clone)]
pub struct BufferCacheStats {
    /// Total cache hits
    pub cache_hits: u64,
    /// Total cache misses
    pub cache_misses: u64,
    /// Total read operations
    pub reads: u64,
    /// Total write operations
    pub writes: u64,
    /// Total flush operations
    pub flushes: u64,
    /// Current number of dirty buffers
    pub dirty_buffers: u64,
    /// Current cache utilization
    pub cache_utilization: u64,
}

/// Buffer cache implementation
pub struct BufferCache {
    /// Buffer storage
    buffers: RwLock<BTreeMap<(u32, u64), Box<Buffer>>>,
    /// LRU queue for eviction
    lru_queue: Mutex<VecDeque<(u32, u64)>>,
    /// Dirty buffer queue
    dirty_queue: Mutex<VecDeque<(u32, u64)>>,
    /// Cache statistics
    stats: RwLock<BufferCacheStats>,
    /// Next access time counter
    access_counter: AtomicU64,
    /// Background flush enabled
    flush_enabled: AtomicBool,
}

impl BufferCache {
    /// Create new buffer cache
    pub fn new() -> Self {
        Self {
            buffers: RwLock::new(BTreeMap::new()),
            lru_queue: Mutex::new(VecDeque::new()),
            dirty_queue: Mutex::new(VecDeque::new()),
            stats: RwLock::new(BufferCacheStats::default()),
            access_counter: AtomicU64::new(1),
            flush_enabled: AtomicBool::new(true),
        }
    }

    /// Get buffer for reading
    pub fn get_buffer(&self, device_id: u32, block_num: u64) -> Result<Vec<u8>, StorageError> {
        let key = (device_id, block_num);

        // Check if buffer is in cache
        {
            let mut buffers = self.buffers.write();
            if let Some(buffer) = buffers.get_mut(&key) {
                buffer.touch();
                buffer.ref_count += 1;

                // Update statistics
                {
                    let mut stats = self.stats.write();
                    stats.cache_hits += 1;
                }

                // Move to front of LRU queue
                self.update_lru(key);

                return Ok(buffer.data.clone());
            }
        }

        // Cache miss - need to read from disk
        {
            let mut stats = self.stats.write();
            stats.cache_misses += 1;
            stats.reads += 1;
        }

        // Ensure we have space in cache
        self.ensure_cache_space()?;

        // Create new buffer and read data
        let mut buffer = Buffer::new(device_id, block_num);
        buffer.state = BufferState::Reading;
        buffer.touch();
        buffer.ref_count = 1;

        // Read data from storage
        let start_sector = block_num * SECTORS_PER_BUFFER as u64;
        read_storage_sectors(device_id, start_sector, &mut buffer.data)?;

        buffer.state = BufferState::Clean;
        let data = buffer.data.clone();

        // Insert into cache
        {
            let mut buffers = self.buffers.write();
            buffers.insert(key, Box::new(buffer));
        }

        // Add to LRU queue
        {
            let mut lru = self.lru_queue.lock();
            lru.push_front(key);
        }

        // Trigger read-ahead
        self.read_ahead(device_id, block_num + 1);

        Ok(data)
    }

    /// Write buffer data
    pub fn write_buffer(
        &self,
        device_id: u32,
        block_num: u64,
        data: &[u8],
    ) -> Result<(), StorageError> {
        if data.len() != BUFFER_SIZE {
            return Err(StorageError::InvalidSector);
        }

        let key = (device_id, block_num);

        // Ensure we have space in cache
        self.ensure_cache_space()?;

        {
            let mut buffers = self.buffers.write();

            if let Some(buffer) = buffers.get_mut(&key) {
                // Update existing buffer
                buffer.data.copy_from_slice(data);
                buffer.state = BufferState::Dirty;
                buffer.dirty = true;
                buffer.touch();
                buffer.ref_count += 1;
            } else {
                // Create new buffer
                let mut buffer = Buffer::new(device_id, block_num);
                buffer.data.copy_from_slice(data);
                buffer.state = BufferState::Dirty;
                buffer.dirty = true;
                buffer.touch();
                buffer.ref_count = 1;

                buffers.insert(key, Box::new(buffer));
            }
        }

        // Add to dirty queue
        {
            let mut dirty = self.dirty_queue.lock();
            if !dirty.contains(&key) {
                dirty.push_back(key);
            }
        }

        // Update LRU
        self.update_lru(key);

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.writes += 1;
            stats.dirty_buffers += 1;
        }

        // Check if we need to flush dirty buffers
        {
            let dirty = self.dirty_queue.lock();
            if dirty.len() > MAX_DIRTY_BUFFERS {
                drop(dirty);
                self.flush_dirty_buffers()?;
            }
        }

        Ok(())
    }

    /// Flush all dirty buffers for a device
    pub fn flush_device(&self, device_id: u32) -> Result<(), StorageError> {
        let dirty_keys: Vec<(u32, u64)> = {
            let dirty = self.dirty_queue.lock();
            dirty
                .iter()
                .filter(|(dev_id, _)| *dev_id == device_id)
                .cloned()
                .collect()
        };

        for key in dirty_keys {
            self.flush_buffer(key)?;
        }

        Ok(())
    }

    /// Flush all dirty buffers
    pub fn flush_all(&self) -> Result<(), StorageError> {
        let dirty_keys: Vec<(u32, u64)> = {
            let dirty = self.dirty_queue.lock();
            dirty.iter().cloned().collect()
        };

        for key in dirty_keys {
            self.flush_buffer(key)?;
        }

        Ok(())
    }

    /// Flush dirty buffers (background operation)
    fn flush_dirty_buffers(&self) -> Result<(), StorageError> {
        if !self.flush_enabled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let flush_count = core::cmp::min(MAX_DIRTY_BUFFERS / 4, 64); // Flush up to 64 buffers
        let mut flushed = 0;

        while flushed < flush_count {
            let key = {
                let mut dirty = self.dirty_queue.lock();
                if let Some(key) = dirty.pop_front() {
                    key
                } else {
                    break;
                }
            };

            if let Err(_) = self.flush_buffer(key) {
                // Re-add to dirty queue on error
                let mut dirty = self.dirty_queue.lock();
                dirty.push_back(key);
                break;
            }

            flushed += 1;
        }

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.flushes += 1;
        }

        Ok(())
    }

    /// Flush a specific buffer
    fn flush_buffer(&self, key: (u32, u64)) -> Result<(), StorageError> {
        let (device_id, block_num) = key;
        let data = {
            let mut buffers = self.buffers.write();
            if let Some(buffer) = buffers.get_mut(&key) {
                if !buffer.dirty || buffer.state == BufferState::Writing {
                    return Ok(());
                }

                buffer.state = BufferState::Writing;
                buffer.data.clone()
            } else {
                return Ok(());
            }
        };

        // Write to storage
        let start_sector = block_num * SECTORS_PER_BUFFER as u64;
        let result = write_storage_sectors(device_id, start_sector, &data);

        // Update buffer state
        {
            let mut buffers = self.buffers.write();
            if let Some(buffer) = buffers.get_mut(&key) {
                if result.is_ok() {
                    buffer.state = BufferState::Clean;
                    buffer.dirty = false;
                } else {
                    buffer.state = BufferState::Dirty;
                    // Re-add to dirty queue
                    let mut dirty = self.dirty_queue.lock();
                    dirty.push_back(key);
                }
            }
        }

        // Update statistics
        if result.is_ok() {
            let mut stats = self.stats.write();
            stats.dirty_buffers = stats.dirty_buffers.saturating_sub(1);
        }

        result.map(|_| ())
    }

    /// Read-ahead operation
    fn read_ahead(&self, device_id: u32, start_block: u64) {
        for i in 0..READ_AHEAD_BUFFERS {
            let block_num = start_block + i as u64;
            let key = (device_id, block_num);

            // Check if already in cache
            {
                let buffers = self.buffers.read();
                if buffers.contains_key(&key) {
                    continue;
                }
            }

            // Asynchronously read buffer (simplified - would use background thread)
            if let Ok(_) = self.get_buffer(device_id, block_num) {
                // Buffer loaded successfully
            }
        }
    }

    /// Ensure cache has space for new buffers
    fn ensure_cache_space(&self) -> Result<(), StorageError> {
        let current_size = {
            let buffers = self.buffers.read();
            buffers.len()
        };

        if current_size < BUFFER_CACHE_SIZE {
            return Ok(());
        }

        // Need to evict some buffers
        let evict_count = current_size - BUFFER_CACHE_SIZE + 1;

        for _ in 0..evict_count {
            if let Some(key) = self.find_eviction_candidate() {
                self.evict_buffer(key)?;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Find buffer to evict using LRU policy
    fn find_eviction_candidate(&self) -> Option<(u32, u64)> {
        let lru = self.lru_queue.lock();
        let buffers = self.buffers.read();

        // Find oldest buffer that can be evicted
        for &key in lru.iter().rev() {
            if let Some(buffer) = buffers.get(&key) {
                if buffer.can_evict() {
                    return Some(key);
                }
            }
        }

        None
    }

    /// Evict a buffer from cache
    fn evict_buffer(&self, key: (u32, u64)) -> Result<(), StorageError> {
        // Flush if dirty
        {
            let buffers = self.buffers.read();
            if let Some(buffer) = buffers.get(&key) {
                if buffer.dirty {
                    drop(buffers);
                    self.flush_buffer(key)?;
                }
            }
        }

        // Remove from cache
        {
            let mut buffers = self.buffers.write();
            buffers.remove(&key);
        }

        // Remove from LRU queue
        {
            let mut lru = self.lru_queue.lock();
            if let Some(pos) = lru.iter().position(|&x| x == key) {
                lru.remove(pos);
            }
        }

        // Remove from dirty queue
        {
            let mut dirty = self.dirty_queue.lock();
            if let Some(pos) = dirty.iter().position(|&x| x == key) {
                dirty.remove(pos);
            }
        }

        Ok(())
    }

    /// Update LRU queue
    fn update_lru(&self, key: (u32, u64)) {
        let mut lru = self.lru_queue.lock();

        // Remove from current position
        if let Some(pos) = lru.iter().position(|&x| x == key) {
            lru.remove(pos);
        }

        // Add to front
        lru.push_front(key);
    }

    /// Release buffer reference
    pub fn release_buffer(&self, device_id: u32, block_num: u64) {
        let key = (device_id, block_num);
        let mut buffers = self.buffers.write();

        if let Some(buffer) = buffers.get_mut(&key) {
            buffer.ref_count = buffer.ref_count.saturating_sub(1);
        }
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> BufferCacheStats {
        let stats = self.stats.read();
        let mut result = stats.clone();

        // Update current utilization
        let buffers = self.buffers.read();
        result.cache_utilization = buffers.len() as u64;

        let dirty = self.dirty_queue.lock();
        result.dirty_buffers = dirty.len() as u64;

        result
    }

    /// Invalidate all buffers for a device
    pub fn invalidate_device(&self, device_id: u32) {
        let keys_to_remove: Vec<(u32, u64)> = {
            let buffers = self.buffers.read();
            buffers
                .keys()
                .filter(|(dev_id, _)| *dev_id == device_id)
                .cloned()
                .collect()
        };

        for key in keys_to_remove {
            let _ = self.evict_buffer(key);
        }
    }

    /// Enable/disable background flushing
    pub fn set_flush_enabled(&self, enabled: bool) {
        self.flush_enabled.store(enabled, Ordering::Relaxed);
    }
}

/// Global buffer cache instance
static BUFFER_CACHE: spin::Once<BufferCache> = spin::Once::new();

/// Initialize global buffer cache
pub fn init_buffer_cache() {
    BUFFER_CACHE.call_once(|| BufferCache::new());
}

/// Get reference to global buffer cache
pub fn buffer_cache() -> &'static BufferCache {
    BUFFER_CACHE.get().expect("Buffer cache not initialized")
}

/// Read data through buffer cache
pub fn buffered_read(
    device_id: u32,
    sector: u64,
    buffer: &mut [u8],
) -> Result<usize, StorageError> {
    let cache = buffer_cache();
    let sectors_per_buffer = SECTORS_PER_BUFFER as u64;
    let buffer_size = BUFFER_SIZE;

    let mut bytes_read = 0;
    let mut remaining = buffer.len();
    let mut current_sector = sector;

    while remaining > 0 {
        let block_num = current_sector / sectors_per_buffer;
        let block_offset = ((current_sector % sectors_per_buffer) * 512) as usize;

        let block_data = cache.get_buffer(device_id, block_num)?;

        let copy_len = core::cmp::min(remaining, buffer_size - block_offset);
        buffer[bytes_read..bytes_read + copy_len]
            .copy_from_slice(&block_data[block_offset..block_offset + copy_len]);

        bytes_read += copy_len;
        remaining -= copy_len;
        current_sector += (copy_len / 512) as u64;

        cache.release_buffer(device_id, block_num);
    }

    Ok(bytes_read)
}

/// Write data through buffer cache
pub fn buffered_write(device_id: u32, sector: u64, data: &[u8]) -> Result<usize, StorageError> {
    let cache = buffer_cache();
    let sectors_per_buffer = SECTORS_PER_BUFFER as u64;
    let buffer_size = BUFFER_SIZE;

    let mut bytes_written = 0;
    let mut remaining = data.len();
    let mut current_sector = sector;

    while remaining > 0 {
        let block_num = current_sector / sectors_per_buffer;
        let block_offset = ((current_sector % sectors_per_buffer) * 512) as usize;

        // For partial block writes, read existing data first
        let mut block_data = if block_offset != 0 || remaining < buffer_size {
            cache.get_buffer(device_id, block_num)?
        } else {
            vec![0u8; buffer_size]
        };

        let copy_len = core::cmp::min(remaining, buffer_size - block_offset);
        block_data[block_offset..block_offset + copy_len]
            .copy_from_slice(&data[bytes_written..bytes_written + copy_len]);

        cache.write_buffer(device_id, block_num, &block_data)?;

        bytes_written += copy_len;
        remaining -= copy_len;
        current_sector += (copy_len / 512) as u64;

        cache.release_buffer(device_id, block_num);
    }

    Ok(bytes_written)
}

/// Flush all buffers for a device
pub fn flush_device_buffers(device_id: u32) -> Result<(), StorageError> {
    buffer_cache().flush_device(device_id)
}

/// Flush all buffers
pub fn flush_all_buffers() -> Result<(), StorageError> {
    buffer_cache().flush_all()
}

/// Get current time in milliseconds
fn get_current_time() -> u64 {
    // Use system time for buffer cache timestamps
    crate::time::get_system_time_ms()
}
