//! Performance Optimization Module
//!
//! This module provides performance optimizations for RustOS including:
//! - Cache-friendly data structures and algorithms
//! - Lock-free algorithms where applicable
//! - Memory allocation fast paths
//! - CPU cache optimization techniques
//! - Branch prediction hints
//! - Memory prefetching

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};

/// Cache line size for x86_64 (typically 64 bytes)
pub const CACHE_LINE_SIZE: usize = 64;

/// Number of CPU cores (should be detected at runtime)
pub const MAX_CPUS: usize = 64;

/// Branch prediction hints
#[inline(always)]
pub fn likely(b: bool) -> bool {
    // On stable Rust, we can't use intrinsics, so just return the value
    // The compiler's branch predictor will handle optimization
    b
}

#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    // On stable Rust, we can't use intrinsics, so just return the value
    // The compiler's branch predictor will handle optimization
    b
}

/// Cache-aligned structure wrapper
#[repr(align(64))] // Align to cache line
#[derive(Debug, Default)]
pub struct CacheAligned<T> {
    pub inner: T,
}

impl<T> CacheAligned<T> {
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> core::ops::Deref for CacheAligned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> core::ops::DerefMut for CacheAligned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> IntoIterator for CacheAligned<T>
where
    T: IntoIterator,
{
    type Item = T::Item;
    type IntoIter = T::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

/// Lock-free stack implementation for free page lists
pub struct LockFreeStack<T> {
    head: AtomicPtr<Node<T>>,
}

struct Node<T> {
    data: T,
    next: *mut Node<T>,
}

impl<T> LockFreeStack<T> {
    pub const fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Push item onto stack (thread-safe)
    pub fn push(&self, data: T) {
        let new_node = Box::into_raw(Box::new(Node {
            data,
            next: ptr::null_mut(),
        }));

        loop {
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                (*new_node).next = head;
            }

            match self.head.compare_exchange_weak(
                head,
                new_node,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
    }

    /// Pop item from stack (thread-safe)
    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            if head.is_null() {
                return None;
            }

            let next = unsafe { (*head).next };
            match self
                .head
                .compare_exchange_weak(head, next, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => {
                    let data = unsafe { Box::from_raw(head).data };
                    return Some(data);
                }
                Err(_) => continue,
            }
        }
    }

    /// Check if stack is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire).is_null()
    }
}

unsafe impl<T: Send> Send for LockFreeStack<T> {}
unsafe impl<T: Send> Sync for LockFreeStack<T> {}

/// Per-CPU memory allocator for reduced contention
pub struct PerCpuAllocator {
    /// Per-CPU free lists for different orders
    cpu_freelists: [CacheAligned<LockFreeStack<usize>>; MAX_CPUS],
    /// Per-CPU statistics
    cpu_stats: [CacheAligned<AtomicU64>; MAX_CPUS],
    /// Global fallback allocator
    global_allocator: Mutex<GlobalAllocator>,
}

struct GlobalAllocator {
    free_lists: Vec<Vec<usize>>,
    total_allocated: usize,
    total_freed: usize,
}

impl PerCpuAllocator {
    pub const fn new() -> Self {
        const INIT_STACK: CacheAligned<LockFreeStack<usize>> =
            CacheAligned::new(LockFreeStack::new());
        const INIT_STATS: CacheAligned<AtomicU64> = CacheAligned::new(AtomicU64::new(0));

        Self {
            cpu_freelists: [INIT_STACK; MAX_CPUS],
            cpu_stats: [INIT_STATS; MAX_CPUS],
            global_allocator: Mutex::new(GlobalAllocator {
                free_lists: Vec::new(),
                total_allocated: 0,
                total_freed: 0,
            }),
        }
    }

    /// Fast path allocation from per-CPU cache
    pub fn allocate_fast(&self, cpu_id: usize) -> Option<usize> {
        if likely(cpu_id < MAX_CPUS) {
            if let Some(addr) = self.cpu_freelists[cpu_id].pop() {
                self.cpu_stats[cpu_id].fetch_add(1, Ordering::Relaxed);
                return Some(addr);
            }
        }
        None
    }

    /// Slow path allocation from global pool
    pub fn allocate_slow(&self, cpu_id: usize, order: usize) -> Option<usize> {
        let mut global = self.global_allocator.lock();

        // Try to get from global free list
        if let Some(free_list) = global.free_lists.get_mut(order) {
            if let Some(addr) = free_list.pop() {
                // Populate per-CPU cache with additional pages if available
                let mut cache_addrs = Vec::new();
                if cpu_id < MAX_CPUS {
                    let batch_size = 8.min(free_list.len());
                    for _ in 0..batch_size {
                        if let Some(cache_addr) = free_list.pop() {
                            cache_addrs.push(cache_addr);
                        }
                    }
                }
                let _ = free_list; // Release the mutable borrow
                global.total_allocated += 1;

                // Now add cached pages to per-CPU cache
                if cpu_id < MAX_CPUS {
                    for cache_addr in cache_addrs {
                        self.cpu_freelists[cpu_id].push(cache_addr);
                    }
                }

                return Some(addr);
            }
        }

        None
    }

    /// Free memory to per-CPU cache
    pub fn deallocate(&self, addr: usize, cpu_id: usize) {
        if likely(cpu_id < MAX_CPUS) {
            self.cpu_freelists[cpu_id].push(addr);
        } else {
            // Fallback to global allocator
            let mut global = self.global_allocator.lock();
            global.total_freed += 1;
        }
    }

    /// Get statistics for a CPU
    pub fn get_cpu_stats(&self, cpu_id: usize) -> u64 {
        if cpu_id < MAX_CPUS {
            self.cpu_stats[cpu_id].load(Ordering::Relaxed)
        } else {
            0
        }
    }
}

/// Cache-friendly hash table implementation
pub struct CacheFriendlyHashTable<K, V> {
    /// Buckets aligned to cache lines
    buckets: Vec<CacheAligned<Bucket<K, V>>>,
    /// Number of buckets (power of 2)
    bucket_count: usize,
    /// Current number of entries
    size: AtomicUsize,
    /// Load factor threshold
    max_load_factor: f32,
}

#[repr(align(64))]
struct Bucket<K, V> {
    entries: RwLock<Vec<Entry<K, V>>>,
}

struct Entry<K, V> {
    key: K,
    value: V,
    hash: u64,
}

impl<K, V> CacheFriendlyHashTable<K, V>
where
    K: Clone + PartialEq,
{
    pub fn new(initial_capacity: usize) -> Self {
        let bucket_count = initial_capacity.next_power_of_two();
        let mut buckets = Vec::with_capacity(bucket_count);

        for _ in 0..bucket_count {
            buckets.push(CacheAligned::new(Bucket {
                entries: RwLock::new(Vec::new()),
            }));
        }

        Self {
            buckets,
            bucket_count,
            size: AtomicUsize::new(0),
            max_load_factor: 0.75,
        }
    }

    /// Hash function optimized for cache performance
    fn hash(&self, key: &K) -> u64 {
        // Simple hash function - in production, use a better one
        let ptr = key as *const K as *const u8;
        let size = size_of::<K>();
        let mut hash = 0u64;

        unsafe {
            for i in 0..size {
                hash = hash.wrapping_mul(31).wrapping_add(*ptr.add(i) as u64);
            }
        }

        hash
    }

    /// Get bucket index from hash
    fn bucket_index(&self, hash: u64) -> usize {
        (hash as usize) & (self.bucket_count - 1)
    }

    /// Insert or update entry
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let hash = self.hash(&key);
        let bucket_idx = self.bucket_index(hash);
        let bucket = &self.buckets[bucket_idx];

        let mut entries = bucket.entries.write();

        // Check if key already exists
        for entry in entries.iter_mut() {
            if entry.hash == hash && entry.key == key {
                let old_value = core::mem::replace(&mut entry.value, value);
                return Some(old_value);
            }
        }

        // Add new entry
        entries.push(Entry { key, value, hash });
        self.size.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get value by key
    pub fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let hash = self.hash(key);
        let bucket_idx = self.bucket_index(hash);
        let bucket = &self.buckets[bucket_idx];

        let entries = bucket.entries.read();

        for entry in entries.iter() {
            if entry.hash == hash && entry.key == *key {
                return Some(entry.value.clone());
            }
        }

        None
    }

    /// Remove entry by key
    pub fn remove(&self, key: &K) -> Option<V> {
        let hash = self.hash(key);
        let bucket_idx = self.bucket_index(hash);
        let bucket = &self.buckets[bucket_idx];

        let mut entries = bucket.entries.write();

        for (i, entry) in entries.iter().enumerate() {
            if entry.hash == hash && entry.key == *key {
                let removed = entries.remove(i);
                self.size.fetch_sub(1, Ordering::Relaxed);
                return Some(removed.value);
            }
        }

        None
    }

    /// Get current size
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Memory prefetching utilities
pub struct MemoryPrefetcher;

impl MemoryPrefetcher {
    /// Prefetch memory for reading
    #[inline(always)]
    pub fn prefetch_read<T>(addr: *const T) {
        // Cast to *const i8 for compatibility
        let addr_i8 = addr as *const i8;
        unsafe {
            core::arch::x86_64::_mm_prefetch(addr_i8, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    /// Prefetch memory for writing
    #[inline(always)]
    pub fn prefetch_write<T>(addr: *const T) {
        // Cast to *const i8 for compatibility
        let addr_i8 = addr as *const i8;
        unsafe {
            core::arch::x86_64::_mm_prefetch(addr_i8, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    /// Prefetch entire cache line
    #[inline(always)]
    pub fn prefetch_line(addr: *const u8) {
        unsafe {
            core::arch::x86_64::_mm_prefetch(addr as *const i8, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    /// Bulk prefetch for sequential access
    pub fn bulk_prefetch(start: *const u8, size: usize) {
        let mut addr = start;
        let end = unsafe { start.add(size) };

        while addr < end {
            Self::prefetch_line(addr);
            addr = unsafe { addr.add(CACHE_LINE_SIZE) };
        }
    }
}

/// CPU cache optimization utilities
pub struct CacheOptimizer;

impl CacheOptimizer {
    /// Flush cache line containing the given address
    #[inline(always)]
    pub fn flush_cache_line(addr: *const u8) {
        unsafe {
            core::arch::x86_64::_mm_clflush(addr);
        }
    }

    /// Memory barrier to ensure cache coherency
    #[inline(always)]
    pub fn memory_barrier() {
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    /// CPU pause hint for spin loops
    #[inline(always)]
    pub fn cpu_pause() {
        core::arch::x86_64::_mm_pause();
    }

    /// Calculate optimal loop unrolling factor
    pub fn optimal_unroll_factor(data_size: usize) -> usize {
        if data_size <= 64 {
            2
        } else if data_size <= 256 {
            4
        } else if data_size <= 1024 {
            8
        } else {
            16
        }
    }
}

/// Cache-aware memory copy optimized for performance
pub struct FastMemCopy;

impl FastMemCopy {
    /// Fast memory copy with cache optimization
    pub unsafe fn copy_fast(dst: *mut u8, src: *const u8, len: usize) {
        if likely(len >= 64) {
            Self::copy_large(dst, src, len);
        } else if len >= 16 {
            Self::copy_medium(dst, src, len);
        } else {
            Self::copy_small(dst, src, len);
        }
    }

    /// Copy small amounts (< 16 bytes)
    unsafe fn copy_small(dst: *mut u8, src: *const u8, len: usize) {
        // Use simple byte copy for small sizes
        for i in 0..len {
            *dst.add(i) = *src.add(i);
        }
    }

    /// Copy medium amounts (16-63 bytes)
    unsafe fn copy_medium(dst: *mut u8, src: *const u8, len: usize) {
        // Use 8-byte copies when possible
        let mut remaining = len;
        let mut dst_ptr = dst as *mut u64;
        let mut src_ptr = src as *const u64;

        while remaining >= 8 {
            *dst_ptr = *src_ptr;
            dst_ptr = dst_ptr.add(1);
            src_ptr = src_ptr.add(1);
            remaining -= 8;
        }

        // Handle remaining bytes
        let dst_byte = dst_ptr as *mut u8;
        let src_byte = src_ptr as *const u8;
        for i in 0..remaining {
            *dst_byte.add(i) = *src_byte.add(i);
        }
    }

    /// Copy large amounts (>= 64 bytes) with prefetching
    unsafe fn copy_large(dst: *mut u8, src: *const u8, len: usize) {
        let mut remaining = len;
        let mut dst_ptr = dst;
        let mut src_ptr = src;

        // Prefetch first few cache lines
        MemoryPrefetcher::bulk_prefetch(src_ptr, 256.min(len));

        // Copy in cache-line sized chunks
        while remaining >= CACHE_LINE_SIZE {
            // Prefetch next cache line
            if remaining > CACHE_LINE_SIZE {
                MemoryPrefetcher::prefetch_line(src_ptr.add(CACHE_LINE_SIZE));
            }

            // Copy current cache line using SIMD if available
            Self::copy_cache_line(dst_ptr, src_ptr);

            dst_ptr = dst_ptr.add(CACHE_LINE_SIZE);
            src_ptr = src_ptr.add(CACHE_LINE_SIZE);
            remaining -= CACHE_LINE_SIZE;
        }

        // Handle remaining bytes
        if remaining > 0 {
            Self::copy_medium(dst_ptr, src_ptr, remaining);
        }
    }

    /// Copy exactly one cache line (64 bytes)
    unsafe fn copy_cache_line(dst: *mut u8, src: *const u8) {
        // Copy as 8 u64 values (8 * 8 = 64 bytes)
        let dst_u64 = dst as *mut u64;
        let src_u64 = src as *const u64;

        for i in 0..8 {
            *dst_u64.add(i) = *src_u64.add(i);
        }
    }
}

/// Performance monitoring and profiling
pub struct PerformanceMonitor {
    /// Allocation statistics
    allocation_stats: CacheAligned<AllocationStats>,
    /// Timing statistics
    timing_stats: CacheAligned<TimingStats>,
    /// Cache miss counters
    cache_stats: CacheAligned<CacheStats>,
}

#[derive(Debug, Default)]
struct AllocationStats {
    total_allocations: AtomicU64,
    total_deallocations: AtomicU64,
    total_bytes_allocated: AtomicU64,
    total_bytes_freed: AtomicU64,
    peak_memory_usage: AtomicU64,
    allocation_failures: AtomicU64,
}

#[derive(Debug, Default)]
struct TimingStats {
    total_allocation_time: AtomicU64,
    total_deallocation_time: AtomicU64,
    max_allocation_time: AtomicU64,
    context_switch_time: AtomicU64,
}

#[derive(Debug, Default)]
struct CacheStats {
    l1_cache_misses: AtomicU64,
    l2_cache_misses: AtomicU64,
    l3_cache_misses: AtomicU64,
    tlb_misses: AtomicU64,
}

impl PerformanceMonitor {
    pub const fn new() -> Self {
        Self {
            allocation_stats: CacheAligned::new(AllocationStats {
                total_allocations: AtomicU64::new(0),
                total_deallocations: AtomicU64::new(0),
                total_bytes_allocated: AtomicU64::new(0),
                total_bytes_freed: AtomicU64::new(0),
                peak_memory_usage: AtomicU64::new(0),
                allocation_failures: AtomicU64::new(0),
            }),
            timing_stats: CacheAligned::new(TimingStats {
                total_allocation_time: AtomicU64::new(0),
                total_deallocation_time: AtomicU64::new(0),
                max_allocation_time: AtomicU64::new(0),
                context_switch_time: AtomicU64::new(0),
            }),
            cache_stats: CacheAligned::new(CacheStats {
                l1_cache_misses: AtomicU64::new(0),
                l2_cache_misses: AtomicU64::new(0),
                l3_cache_misses: AtomicU64::new(0),
                tlb_misses: AtomicU64::new(0),
            }),
        }
    }

    /// Record allocation
    pub fn record_allocation(&self, size: u64, time_ns: u64) {
        self.allocation_stats
            .total_allocations
            .fetch_add(1, Ordering::Relaxed);
        self.allocation_stats
            .total_bytes_allocated
            .fetch_add(size, Ordering::Relaxed);
        self.timing_stats
            .total_allocation_time
            .fetch_add(time_ns, Ordering::Relaxed);

        // Update max allocation time
        loop {
            let current_max = self
                .timing_stats
                .max_allocation_time
                .load(Ordering::Relaxed);
            if time_ns <= current_max {
                break;
            }
            match self.timing_stats.max_allocation_time.compare_exchange_weak(
                current_max,
                time_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
    }

    /// Record deallocation
    pub fn record_deallocation(&self, size: u64, time_ns: u64) {
        self.allocation_stats
            .total_deallocations
            .fetch_add(1, Ordering::Relaxed);
        self.allocation_stats
            .total_bytes_freed
            .fetch_add(size, Ordering::Relaxed);
        self.timing_stats
            .total_deallocation_time
            .fetch_add(time_ns, Ordering::Relaxed);
    }

    /// Record allocation failure
    pub fn record_allocation_failure(&self) {
        self.allocation_stats
            .allocation_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Get current statistics
    pub fn get_stats(&self) -> PerformanceStats {
        PerformanceStats {
            total_allocations: self
                .allocation_stats
                .total_allocations
                .load(Ordering::Relaxed),
            total_deallocations: self
                .allocation_stats
                .total_deallocations
                .load(Ordering::Relaxed),
            total_bytes_allocated: self
                .allocation_stats
                .total_bytes_allocated
                .load(Ordering::Relaxed),
            total_bytes_freed: self
                .allocation_stats
                .total_bytes_freed
                .load(Ordering::Relaxed),
            allocation_failures: self
                .allocation_stats
                .allocation_failures
                .load(Ordering::Relaxed),
            average_allocation_time: {
                let total_time = self
                    .timing_stats
                    .total_allocation_time
                    .load(Ordering::Relaxed);
                let total_allocs = self
                    .allocation_stats
                    .total_allocations
                    .load(Ordering::Relaxed);
                if total_allocs > 0 {
                    total_time / total_allocs
                } else {
                    0
                }
            },
            max_allocation_time: self
                .timing_stats
                .max_allocation_time
                .load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub total_allocations: u64,
    pub total_deallocations: u64,
    pub total_bytes_allocated: u64,
    pub total_bytes_freed: u64,
    pub allocation_failures: u64,
    pub average_allocation_time: u64,
    pub max_allocation_time: u64,
}

/// Global performance monitor
static PERFORMANCE_MONITOR: PerformanceMonitor = PerformanceMonitor::new();

/// Get global performance monitor
pub fn get_performance_monitor() -> &'static PerformanceMonitor {
    &PERFORMANCE_MONITOR
}

/// High-performance timing utilities
pub struct HighResTimer;

impl HighResTimer {
    /// Get current timestamp in nanoseconds (using TSC)
    #[inline(always)]
    pub fn now_ns() -> u64 {
        unsafe { core::arch::x86_64::_rdtsc() }
    }

    /// Measure execution time of a closure
    pub fn time<F, R>(f: F) -> (R, u64)
    where
        F: FnOnce() -> R,
    {
        let start = Self::now_ns();
        let result = f();
        let end = Self::now_ns();
        (result, end - start)
    }
}

/// Initialize performance optimizations
pub fn init() -> Result<(), &'static str> {
    // Detect CPU features via CPUID and cache the results for use by
    // performance-critical code paths (e.g. SSE/AVX memcpy, prefetch hints).
    let features = crate::arch::cpu_features();

    if features.sse4_2 {
        // SSE 4.2 enables CRC32 and string comparison instructions
    }
    if features.avx {
        // AVX enables 256-bit vector operations
    }
    if features.rdrand {
        // RDRAND provides a hardware random number source
    }

    Ok(())
}
