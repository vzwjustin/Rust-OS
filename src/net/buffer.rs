//! # Network Buffer Management
//!
//! This module provides efficient buffer management for network operations,
//! including packet buffers, ring buffers, buffer pools, and zero-copy operations.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
// use alloc::string::ToString; // Unused
// use core::mem;
// use core::ptr;
// use core::slice;
use spin::{Mutex, RwLock};

use super::NetworkError;

// Helper trait for slice to vec conversion in no_std
trait SliceToVec<T> {
    fn to_vec(&self) -> Vec<T>;
}

impl<T: Clone> SliceToVec<T> for [T] {
    fn to_vec(&self) -> Vec<T> {
        let mut vec = Vec::with_capacity(self.len());
        for item in self {
            vec.push(item.clone());
        }
        vec
    }
}

/// Default buffer size for network packets
pub const DEFAULT_BUFFER_SIZE: usize = 2048;

/// Maximum buffer size
pub const MAX_BUFFER_SIZE: usize = 65536;

/// Minimum buffer size
pub const MIN_BUFFER_SIZE: usize = 64;

/// Default number of buffers in pool
pub const DEFAULT_POOL_SIZE: usize = 256;

/// Buffer alignment for optimal performance
pub const BUFFER_ALIGNMENT: usize = 64;

/// Network buffer metadata
#[derive(Debug, Clone)]
pub struct BufferMetadata {
    /// Buffer ID for tracking
    pub id: u32,
    /// Timestamp when buffer was allocated
    pub allocated_at: u64,
    /// Last used timestamp
    pub last_used: u64,
    /// Number of references to this buffer
    pub ref_count: u32,
    /// Buffer flags
    pub flags: BufferFlags,
    /// Protocol-specific data
    pub protocol_data: u32,
}

impl BufferMetadata {
    pub fn new(id: u32, timestamp: u64) -> Self {
        Self {
            id,
            allocated_at: timestamp,
            last_used: timestamp,
            ref_count: 1,
            flags: BufferFlags::empty(),
            protocol_data: 0,
        }
    }
}

/// Buffer flags for various purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferFlags {
    bits: u32,
}

impl BufferFlags {
    /// No flags set
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Buffer contains IPv4 packet
    pub const fn ipv4() -> Self {
        Self { bits: 1 << 0 }
    }

    /// Buffer contains IPv6 packet
    pub const fn ipv6() -> Self {
        Self { bits: 1 << 1 }
    }

    /// Buffer contains TCP packet
    pub const fn tcp() -> Self {
        Self { bits: 1 << 2 }
    }

    /// Buffer contains UDP packet
    pub const fn udp() -> Self {
        Self { bits: 1 << 3 }
    }

    /// Buffer contains ICMP packet
    pub const fn icmp() -> Self {
        Self { bits: 1 << 4 }
    }

    /// Buffer is part of a fragmented packet
    pub const fn fragmented() -> Self {
        Self { bits: 1 << 5 }
    }

    /// Buffer needs checksum calculation
    pub const fn checksum_needed() -> Self {
        Self { bits: 1 << 6 }
    }

    /// Buffer is read-only
    pub const fn readonly() -> Self {
        Self { bits: 1 << 7 }
    }

    /// Check if flag is set
    pub const fn contains(self, flag: Self) -> bool {
        (self.bits & flag.bits) == flag.bits
    }

    /// Set a flag
    pub fn set(&mut self, flag: Self) {
        self.bits |= flag.bits;
    }

    /// Clear a flag
    pub fn clear(&mut self, flag: Self) {
        self.bits &= !flag.bits;
    }

    /// Toggle a flag
    pub fn toggle(&mut self, flag: Self) {
        self.bits ^= flag.bits;
    }
}

/// Network packet buffer
#[derive(Debug)]
pub struct PacketBuffer {
    /// Buffer data
    data: Box<[u8]>,
    /// Current data length (may be less than capacity)
    length: usize,
    /// Read position
    read_pos: usize,
    /// Write position
    write_pos: usize,
    /// Buffer metadata
    metadata: BufferMetadata,
}

impl PacketBuffer {
    /// Create a new packet buffer
    pub fn new(size: usize, id: u32, timestamp: u64) -> Result<Self, NetworkError> {
        if size < MIN_BUFFER_SIZE || size > MAX_BUFFER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let data = vec![0u8; size].into_boxed_slice();
        let metadata = BufferMetadata::new(id, timestamp);

        Ok(Self {
            data,
            length: 0,
            read_pos: 0,
            write_pos: 0,
            metadata,
        })
    }

    /// Create buffer with data
    pub fn with_data(data: Vec<u8>, id: u32, timestamp: u64) -> Self {
        let length = data.len();
        let metadata = BufferMetadata::new(id, timestamp);

        Self {
            data: data.into_boxed_slice(),
            length,
            read_pos: 0,
            write_pos: length,
            metadata,
        }
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Get current data length
    pub fn len(&self) -> usize {
        self.length
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Get available space for writing
    pub fn available_space(&self) -> usize {
        self.capacity() - self.write_pos
    }

    /// Get available data for reading
    pub fn available_data(&self) -> usize {
        self.write_pos - self.read_pos
    }

    /// Get slice of all data
    pub fn data(&self) -> &[u8] {
        &self.data[..self.length]
    }

    /// Get mutable slice of all data
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.length]
    }

    /// Get slice of readable data
    pub fn readable_data(&self) -> &[u8] {
        &self.data[self.read_pos..self.write_pos]
    }

    /// Get mutable slice of writable space
    pub fn writable_space(&mut self) -> &mut [u8] {
        &mut self.data[self.write_pos..]
    }

    /// Write data to buffer
    pub fn write(&mut self, data: &[u8]) -> Result<usize, NetworkError> {
        let available = self.available_space();
        let write_len = core::cmp::min(data.len(), available);

        if write_len == 0 {
            return Err(NetworkError::BufferTooSmall);
        }

        self.data[self.write_pos..self.write_pos + write_len].copy_from_slice(&data[..write_len]);

        self.write_pos += write_len;
        self.length = core::cmp::max(self.length, self.write_pos);

        Ok(write_len)
    }

    /// Read data from buffer
    pub fn read(&mut self, buffer: &mut [u8]) -> usize {
        let available = self.available_data();
        let read_len = core::cmp::min(buffer.len(), available);

        buffer[..read_len].copy_from_slice(&self.data[self.read_pos..self.read_pos + read_len]);

        self.read_pos += read_len;
        read_len
    }

    /// Peek at data without consuming it
    pub fn peek(&self, buffer: &mut [u8]) -> usize {
        let available = self.available_data();
        let read_len = core::cmp::min(buffer.len(), available);

        buffer[..read_len].copy_from_slice(&self.data[self.read_pos..self.read_pos + read_len]);

        read_len
    }

    /// Consume data (advance read pointer)
    pub fn consume(&mut self, count: usize) -> usize {
        let available = self.available_data();
        let consume_len = core::cmp::min(count, available);
        self.read_pos += consume_len;
        consume_len
    }

    /// Reset buffer for reuse
    pub fn reset(&mut self) {
        self.length = 0;
        self.read_pos = 0;
        self.write_pos = 0;
        self.metadata.ref_count = 1;
    }

    /// Clone buffer data
    pub fn clone_data(&self) -> Vec<u8> {
        self.data().to_vec()
    }

    /// Resize buffer data length
    pub fn resize(&mut self, new_length: usize) -> Result<(), NetworkError> {
        if new_length > self.capacity() {
            return Err(NetworkError::BufferTooSmall);
        }

        self.length = new_length;
        self.write_pos = core::cmp::min(self.write_pos, new_length);
        self.read_pos = core::cmp::min(self.read_pos, new_length);

        Ok(())
    }

    /// Get buffer metadata
    pub fn metadata(&self) -> &BufferMetadata {
        &self.metadata
    }

    /// Get mutable buffer metadata
    pub fn metadata_mut(&mut self) -> &mut BufferMetadata {
        &mut self.metadata
    }

    /// Increment reference count
    pub fn add_ref(&mut self) {
        self.metadata.ref_count += 1;
    }

    /// Decrement reference count
    pub fn release_ref(&mut self) -> u32 {
        if self.metadata.ref_count > 0 {
            self.metadata.ref_count -= 1;
        }
        self.metadata.ref_count
    }
}

/// Ring buffer for efficient packet queuing
#[derive(Debug)]
pub struct RingBuffer {
    /// Buffer storage
    buffers: Vec<Option<PacketBuffer>>,
    /// Read index
    read_index: usize,
    /// Write index
    write_index: usize,
    /// Number of elements in buffer
    count: usize,
    /// Buffer capacity
    capacity: usize,
}

impl RingBuffer {
    /// Create a new ring buffer
    pub fn new(capacity: usize) -> Self {
        let mut buffers = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffers.push(None);
        }

        Self {
            buffers,
            read_index: 0,
            write_index: 0,
            count: 0,
            capacity,
        }
    }

    /// Check if ring buffer is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Check if ring buffer is full
    pub fn is_full(&self) -> bool {
        self.count == self.capacity
    }

    /// Get number of elements in buffer
    pub fn len(&self) -> usize {
        self.count
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get available space
    pub fn available_space(&self) -> usize {
        self.capacity - self.count
    }

    /// Push buffer to ring buffer
    pub fn push(&mut self, buffer: PacketBuffer) -> Result<(), NetworkError> {
        if self.is_full() {
            return Err(NetworkError::InsufficientMemory);
        }

        self.buffers[self.write_index] = Some(buffer);
        self.write_index = (self.write_index + 1) % self.capacity;
        self.count += 1;

        Ok(())
    }

    /// Pop buffer from ring buffer
    pub fn pop(&mut self) -> Option<PacketBuffer> {
        if self.is_empty() {
            return None;
        }

        let buffer = self.buffers[self.read_index].take();
        self.read_index = (self.read_index + 1) % self.capacity;
        self.count -= 1;

        buffer
    }

    /// Peek at next buffer without removing it
    pub fn peek(&self) -> Option<&PacketBuffer> {
        if self.is_empty() {
            None
        } else {
            self.buffers[self.read_index].as_ref()
        }
    }

    /// Clear all buffers
    pub fn clear(&mut self) {
        for buffer_slot in &mut self.buffers {
            *buffer_slot = None;
        }
        self.read_index = 0;
        self.write_index = 0;
        self.count = 0;
    }
}

/// Buffer pool for efficient memory management
#[derive(Debug)]
pub struct BufferPool {
    /// Available buffers
    available_buffers: VecDeque<PacketBuffer>,
    /// Buffer size
    buffer_size: usize,
    /// Maximum pool size
    max_size: usize,
    /// Current pool size
    current_size: usize,
    /// Next buffer ID
    next_id: u32,
    /// Pool statistics
    stats: BufferPoolStats,
}

impl BufferPool {
    /// Create a new buffer pool
    pub fn new(buffer_size: usize, initial_size: usize) -> Self {
        let mut pool = Self {
            available_buffers: VecDeque::new(),
            buffer_size,
            max_size: initial_size * 2, // Allow growth
            current_size: 0,
            next_id: 1,
            stats: BufferPoolStats::default(),
        };

        // Pre-allocate initial buffers
        for _ in 0..initial_size {
            if let Ok(buffer) = PacketBuffer::new(buffer_size, pool.next_id, 0) {
                pool.available_buffers.push_back(buffer);
                pool.next_id += 1;
                pool.current_size += 1;
            }
        }

        pool
    }

    /// Allocate a buffer from the pool
    pub fn allocate(&mut self, timestamp: u64) -> Result<PacketBuffer, NetworkError> {
        if let Some(mut buffer) = self.available_buffers.pop_front() {
            buffer.reset();
            buffer.metadata.last_used = timestamp;
            self.stats.allocations += 1;
            Ok(buffer)
        } else if self.current_size < self.max_size {
            // Create new buffer if pool can grow
            let buffer = PacketBuffer::new(self.buffer_size, self.next_id, timestamp)?;
            self.next_id += 1;
            self.current_size += 1;
            self.stats.allocations += 1;
            self.stats.expansions += 1;
            Ok(buffer)
        } else {
            self.stats.allocation_failures += 1;
            Err(NetworkError::InsufficientMemory)
        }
    }

    /// Return buffer to the pool
    pub fn deallocate(&mut self, buffer: PacketBuffer) {
        if buffer.metadata.ref_count <= 1 && self.available_buffers.len() < self.max_size {
            self.available_buffers.push_back(buffer);
            self.stats.deallocations += 1;
        } else {
            // Buffer is still referenced or pool is full
            self.current_size -= 1;
            self.stats.deallocations += 1;
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> &BufferPoolStats {
        &self.stats
    }

    /// Get available buffer count
    pub fn available_count(&self) -> usize {
        self.available_buffers.len()
    }

    /// Get total buffer count
    pub fn total_count(&self) -> usize {
        self.current_size
    }

    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.available_buffers.is_empty()
    }

    /// Resize pool
    pub fn resize(&mut self, new_max_size: usize) {
        self.max_size = new_max_size;

        // Trim if necessary
        while self.available_buffers.len() > new_max_size {
            self.available_buffers.pop_back();
            self.current_size -= 1;
        }
    }

    /// Clear the pool
    pub fn clear(&mut self) {
        self.available_buffers.clear();
        self.current_size = 0;
        self.stats.clears += 1;
    }
}

/// Buffer pool statistics
#[derive(Debug, Default, Clone)]
pub struct BufferPoolStats {
    pub allocations: u64,
    pub deallocations: u64,
    pub allocation_failures: u64,
    pub expansions: u64,
    pub clears: u64,
    pub peak_usage: usize,
}

/// Buffer chain for handling large packets
#[derive(Debug)]
pub struct BufferChain {
    /// Chain of buffers
    buffers: Vec<PacketBuffer>,
    /// Total data length across all buffers
    total_length: usize,
    /// Current read position (buffer index, offset)
    read_position: (usize, usize),
}

impl BufferChain {
    /// Create a new buffer chain
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            total_length: 0,
            read_position: (0, 0),
        }
    }

    /// Create buffer chain from single buffer
    pub fn from_buffer(buffer: PacketBuffer) -> Self {
        let total_length = buffer.len();
        Self {
            buffers: vec![buffer],
            total_length,
            read_position: (0, 0),
        }
    }

    /// Add buffer to chain
    pub fn add_buffer(&mut self, buffer: PacketBuffer) {
        self.total_length += buffer.len();
        self.buffers.push(buffer);
    }

    /// Get total length of all data in chain
    pub fn total_length(&self) -> usize {
        self.total_length
    }

    /// Get number of buffers in chain
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Check if chain is empty
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Read data from chain
    pub fn read(&mut self, output: &mut [u8]) -> usize {
        let mut bytes_read = 0;
        let mut output_offset = 0;

        while output_offset < output.len() && self.read_position.0 < self.buffers.len() {
            let buffer = &mut self.buffers[self.read_position.0];
            let buffer_data = buffer.readable_data();

            if self.read_position.1 >= buffer_data.len() {
                // Move to next buffer
                self.read_position.0 += 1;
                self.read_position.1 = 0;
                continue;
            }

            let available = buffer_data.len() - self.read_position.1;
            let to_read = core::cmp::min(available, output.len() - output_offset);

            output[output_offset..output_offset + to_read].copy_from_slice(
                &buffer_data[self.read_position.1..self.read_position.1 + to_read],
            );

            self.read_position.1 += to_read;
            output_offset += to_read;
            bytes_read += to_read;
        }

        bytes_read
    }

    /// Flatten chain into single buffer
    pub fn flatten(
        &self,
        pool: &mut BufferPool,
        timestamp: u64,
    ) -> Result<PacketBuffer, NetworkError> {
        let mut flattened = pool.allocate(timestamp)?;

        let mut data = Vec::with_capacity(self.total_length);
        for buffer in &self.buffers {
            data.extend_from_slice(buffer.data());
        }

        flattened.write(&data)?;
        Ok(flattened)
    }

    /// Clear the chain
    pub fn clear(&mut self) {
        self.buffers.clear();
        self.total_length = 0;
        self.read_position = (0, 0);
    }

    /// Get buffer at index
    pub fn get_buffer(&self, index: usize) -> Option<&PacketBuffer> {
        self.buffers.get(index)
    }

    /// Get mutable buffer at index
    pub fn get_buffer_mut(&mut self, index: usize) -> Option<&mut PacketBuffer> {
        self.buffers.get_mut(index)
    }
}

impl Default for BufferChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Global buffer manager
pub struct BufferManager {
    /// Small buffer pool (for headers, control packets)
    small_pool: Mutex<BufferPool>,
    /// Medium buffer pool (for typical packets)
    medium_pool: Mutex<BufferPool>,
    /// Large buffer pool (for jumbo frames)
    large_pool: Mutex<BufferPool>,
    /// Buffer manager statistics
    stats: RwLock<BufferManagerStats>,
}

impl BufferManager {
    /// Create a new buffer manager
    pub fn new() -> Self {
        Self {
            small_pool: Mutex::new(BufferPool::new(256, 64)),
            medium_pool: Mutex::new(BufferPool::new(DEFAULT_BUFFER_SIZE, DEFAULT_POOL_SIZE)),
            large_pool: Mutex::new(BufferPool::new(8192, 32)),
            stats: RwLock::new(BufferManagerStats::default()),
        }
    }

    /// Allocate buffer of appropriate size
    pub fn allocate(&self, size: usize, timestamp: u64) -> Result<PacketBuffer, NetworkError> {
        let mut stats = self.stats.write();
        stats.allocation_requests += 1;

        if size <= 256 {
            match self.small_pool.lock().allocate(timestamp) {
                Ok(buffer) => {
                    stats.small_allocations += 1;
                    Ok(buffer)
                }
                Err(e) => {
                    stats.allocation_failures += 1;
                    Err(e)
                }
            }
        } else if size <= DEFAULT_BUFFER_SIZE {
            match self.medium_pool.lock().allocate(timestamp) {
                Ok(buffer) => {
                    stats.medium_allocations += 1;
                    Ok(buffer)
                }
                Err(e) => {
                    stats.allocation_failures += 1;
                    Err(e)
                }
            }
        } else {
            match self.large_pool.lock().allocate(timestamp) {
                Ok(buffer) => {
                    stats.large_allocations += 1;
                    Ok(buffer)
                }
                Err(e) => {
                    stats.allocation_failures += 1;
                    Err(e)
                }
            }
        }
    }

    /// Deallocate buffer back to appropriate pool
    pub fn deallocate(&self, buffer: PacketBuffer) {
        let size = buffer.capacity();

        if size <= 256 {
            self.small_pool.lock().deallocate(buffer);
        } else if size <= DEFAULT_BUFFER_SIZE {
            self.medium_pool.lock().deallocate(buffer);
        } else {
            self.large_pool.lock().deallocate(buffer);
        }

        self.stats.write().deallocations += 1;
    }

    /// Get buffer manager statistics
    pub fn get_stats(&self) -> BufferManagerStats {
        self.stats.read().clone()
    }

    /// Get pool utilization information
    pub fn get_pool_info(&self) -> (usize, usize, usize, usize, usize, usize) {
        let small = self.small_pool.lock();
        let medium = self.medium_pool.lock();
        let large = self.large_pool.lock();

        (
            small.available_count(),
            small.total_count(),
            medium.available_count(),
            medium.total_count(),
            large.available_count(),
            large.total_count(),
        )
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Buffer manager statistics
#[derive(Debug, Default, Clone)]
pub struct BufferManagerStats {
    pub allocation_requests: u64,
    pub small_allocations: u64,
    pub medium_allocations: u64,
    pub large_allocations: u64,
    pub deallocations: u64,
    pub allocation_failures: u64,
}

/// Global buffer manager instance
static BUFFER_MANAGER: spin::Once<BufferManager> = spin::Once::new();

/// Initialize global buffer manager
pub fn init_buffer_manager() {
    BUFFER_MANAGER.call_once(|| BufferManager::new());
}

/// Get reference to global buffer manager
pub fn buffer_manager() -> Option<&'static BufferManager> {
    BUFFER_MANAGER.get()
}

/// High-level buffer allocation function
pub fn allocate_buffer(size: usize, timestamp: u64) -> Result<PacketBuffer, NetworkError> {
    if let Some(manager) = buffer_manager() {
        manager.allocate(size, timestamp)
    } else {
        Err(NetworkError::InsufficientMemory)
    }
}

/// High-level buffer deallocation function
pub fn deallocate_buffer(buffer: PacketBuffer) {
    if let Some(manager) = buffer_manager() {
        manager.deallocate(buffer);
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_packet_buffer_creation() {
        let buffer = PacketBuffer::new(1024, 1, 0).unwrap();
        assert_eq!(buffer.capacity(), 1024);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_packet_buffer_write_read() {
        let mut buffer = PacketBuffer::new(1024, 1, 0).unwrap();
        let data = b"Hello, World!";

        let written = buffer.write(data).unwrap();
        assert_eq!(written, data.len());

        let mut read_buffer = [0u8; 20];
        let read = buffer.read(&mut read_buffer);
        assert_eq!(read, data.len());
        assert_eq!(&read_buffer[..read], data);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_ring_buffer() {
        let mut ring = RingBuffer::new(3);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        let buffer1 = PacketBuffer::new(100, 1, 0).unwrap();
        let buffer2 = PacketBuffer::new(100, 2, 0).unwrap();

        ring.push(buffer1).unwrap();
        ring.push(buffer2).unwrap();

        assert_eq!(ring.len(), 2);
        assert!(!ring.is_empty());

        let popped = ring.pop().unwrap();
        assert_eq!(popped.metadata().id, 1);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_buffer_pool() {
        let mut pool = BufferPool::new(1024, 5);
        assert_eq!(pool.available_count(), 5);

        let buffer = pool.allocate(0).unwrap();
        assert_eq!(pool.available_count(), 4);

        pool.deallocate(buffer);
        assert_eq!(pool.available_count(), 5);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_buffer_chain() {
        let mut chain = BufferChain::new();
        assert!(chain.is_empty());

        let buffer1 = PacketBuffer::with_data(b"Hello, ".to_vec(), 1, 0);
        let buffer2 = PacketBuffer::with_data(b"World!".to_vec(), 2, 0);

        chain.add_buffer(buffer1);
        chain.add_buffer(buffer2);

        assert_eq!(chain.total_length(), 13);
        assert_eq!(chain.buffer_count(), 2);

        let mut output = [0u8; 20];
        let read = chain.read(&mut output);
        assert_eq!(read, 13);
        assert_eq!(&output[..read], b"Hello, World!");
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_buffer_flags() {
        let mut flags = BufferFlags::empty();
        assert!(!flags.contains(BufferFlags::tcp()));

        flags.set(BufferFlags::tcp());
        assert!(flags.contains(BufferFlags::tcp()));

        flags.clear(BufferFlags::tcp());
        assert!(!flags.contains(BufferFlags::tcp()));
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_buffer_metadata() {
        let meta = BufferMetadata::new(42, 1000);
        assert_eq!(meta.id, 42);
        assert_eq!(meta.allocated_at, 1000);
        assert_eq!(meta.ref_count, 1);
    }
}
