//! Optimized Data Structures for RustOS
//!
//! This module provides cache-friendly and lock-free data structures
//! optimized for kernel performance.

use alloc::alloc::{alloc, dealloc, Layout};
use alloc::boxed::Box;
use core::mem::{self, MaybeUninit};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Cache line size for x86-64 (typically 64 bytes)
pub const CACHE_LINE_SIZE: usize = 64;

/// Cache-aligned allocation trait
pub trait CacheAligned {
    fn cache_aligned_layout() -> Layout;
}

/// Lock-free MPSC (Multiple Producer, Single Consumer) queue
/// Optimized for interrupt-to-thread communication
pub struct LockFreeMpscQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
    _phantom: core::marker::PhantomData<T>,
}

struct Node<T> {
    next: AtomicPtr<Node<T>>,
    data: MaybeUninit<T>,
}

impl<T> LockFreeMpscQueue<T> {
    /// Create a new lock-free MPSC queue
    pub fn new() -> Self {
        let dummy = Box::leak(Box::new(Node {
            next: AtomicPtr::new(ptr::null_mut()),
            data: MaybeUninit::uninit(),
        }));

        Self {
            head: AtomicPtr::new(dummy),
            tail: AtomicPtr::new(dummy),
            _phantom: core::marker::PhantomData,
        }
    }

    /// Enqueue an item (lock-free, can be called from multiple producers)
    pub fn enqueue(&self, item: T) {
        let new_node = Box::leak(Box::new(Node {
            next: AtomicPtr::new(ptr::null_mut()),
            data: MaybeUninit::new(item),
        }));

        let prev_head = self.head.swap(new_node, Ordering::AcqRel);
        unsafe {
            (*prev_head).next.store(new_node, Ordering::Release);
        }
    }

    /// Dequeue an item (single consumer only)
    pub fn dequeue(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = unsafe { (*tail).next.load(Ordering::Acquire) };

        if next.is_null() {
            return None;
        }

        self.tail.store(next, Ordering::Relaxed);
        let data = unsafe { (*next).data.assume_init_read() };

        // Free the old tail node
        unsafe {
            let _ = Box::from_raw(tail);
        }

        Some(data)
    }

    /// Check if queue is empty (approximate)
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = unsafe { (*tail).next.load(Ordering::Acquire) };
        next.is_null()
    }
}

unsafe impl<T: Send> Send for LockFreeMpscQueue<T> {}
unsafe impl<T: Send> Sync for LockFreeMpscQueue<T> {}

/// Cache-friendly ring buffer with padding to avoid false sharing
#[repr(align(64))] // Cache line aligned
pub struct CacheFriendlyRingBuffer<T> {
    // Producer side (cache line 1)
    head: AtomicUsize,
    _pad1: [u8; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],

    // Consumer side (cache line 2)
    tail: AtomicUsize,
    _pad2: [u8; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],

    // Shared data
    capacity: usize,
    mask: usize,
    buffer: NonNull<T>,
}

impl<T> CacheFriendlyRingBuffer<T> {
    /// Create a new cache-friendly ring buffer
    /// Capacity must be a power of 2
    pub fn new(capacity: usize) -> Option<Self> {
        if !capacity.is_power_of_two() || capacity == 0 {
            return None;
        }

        let layout = Layout::array::<T>(capacity).ok()?;
        let buffer = NonNull::new(unsafe { alloc(layout) as *mut T })?;

        Some(Self {
            head: AtomicUsize::new(0),
            _pad1: [0; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],
            tail: AtomicUsize::new(0),
            _pad2: [0; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],
            capacity,
            mask: capacity - 1,
            buffer,
        })
    }

    /// Push an item to the buffer (single producer)
    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if (head + 1) & self.mask == tail {
            return Err(item); // Buffer full
        }

        unsafe {
            ptr::write(self.buffer.as_ptr().add(head), item);
        }

        self.head.store((head + 1) & self.mask, Ordering::Release);
        Ok(())
    }

    /// Pop an item from the buffer (single consumer)
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if tail == head {
            return None; // Buffer empty
        }

        let item = unsafe { ptr::read(self.buffer.as_ptr().add(tail)) };
        self.tail.store((tail + 1) & self.mask, Ordering::Release);
        Some(item)
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Relaxed)
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        (head + 1) & self.mask == tail
    }

    /// Get approximate length
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        (head.wrapping_sub(tail)) & self.mask
    }
}

impl<T> Drop for CacheFriendlyRingBuffer<T> {
    fn drop(&mut self) {
        // Drop all remaining items
        while self.pop().is_some() {}

        // Deallocate buffer
        let layout = Layout::array::<T>(self.capacity).unwrap();
        unsafe {
            dealloc(self.buffer.as_ptr() as *mut u8, layout);
        }
    }
}

unsafe impl<T: Send> Send for CacheFriendlyRingBuffer<T> {}
unsafe impl<T: Send> Sync for CacheFriendlyRingBuffer<T> {}

/// Lock-free stack using Treiber's algorithm
pub struct LockFreeStack<T> {
    head: AtomicPtr<StackNode<T>>,
}

struct StackNode<T> {
    data: T,
    next: *mut StackNode<T>,
}

impl<T> LockFreeStack<T> {
    /// Create a new lock-free stack
    pub const fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Push an item onto the stack
    pub fn push(&self, item: T) {
        let new_node = Box::leak(Box::new(StackNode {
            data: item,
            next: ptr::null_mut(),
        }));

        loop {
            let head = self.head.load(Ordering::Relaxed);
            new_node.next = head;

            match self.head.compare_exchange_weak(
                head,
                new_node,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue, // Retry
            }
        }
    }

    /// Pop an item from the stack
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
                Err(_) => continue, // Retry
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

/// Cache-friendly hash table with linear probing
#[repr(align(64))]
pub struct CacheFriendlyHashTable<K, V> {
    buckets: NonNull<Bucket<K, V>>,
    capacity: usize,
    mask: usize,
    size: AtomicUsize,
    _padding: [u8; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],
}

#[repr(align(64))]
struct Bucket<K, V> {
    key: AtomicPtr<K>,
    value: AtomicPtr<V>,
    _padding: [u8; CACHE_LINE_SIZE - 2 * mem::size_of::<AtomicPtr<()>>()],
}

impl<K: Eq + core::hash::Hash, V> CacheFriendlyHashTable<K, V> {
    /// Create a new cache-friendly hash table
    pub fn new(capacity: usize) -> Option<Self> {
        if !capacity.is_power_of_two() || capacity == 0 {
            return None;
        }

        let layout = Layout::array::<Bucket<K, V>>(capacity).ok()?;
        let buckets = NonNull::new(unsafe { alloc(layout) as *mut Bucket<K, V> })?;

        // Initialize buckets
        unsafe {
            for i in 0..capacity {
                ptr::write(
                    buckets.as_ptr().add(i),
                    Bucket {
                        key: AtomicPtr::new(ptr::null_mut()),
                        value: AtomicPtr::new(ptr::null_mut()),
                        _padding: [0; CACHE_LINE_SIZE - 2 * mem::size_of::<AtomicPtr<()>>()],
                    },
                );
            }
        }

        Some(Self {
            buckets,
            capacity,
            mask: capacity - 1,
            size: AtomicUsize::new(0),
            _padding: [0; CACHE_LINE_SIZE - mem::size_of::<AtomicUsize>()],
        })
    }

    /// Insert a key-value pair
    pub fn insert(&self, key: K, value: V) -> Result<(), (K, V)> {
        use core::hash::Hasher;

        // Simple FNV-1a hasher for no_std environment
        struct SimpleHasher {
            state: u64,
        }

        impl SimpleHasher {
            fn new() -> Self {
                Self {
                    state: 0xcbf29ce484222325,
                }
            }
        }

        impl Hasher for SimpleHasher {
            fn finish(&self) -> u64 {
                self.state
            }

            fn write(&mut self, bytes: &[u8]) {
                for byte in bytes {
                    self.state ^= *byte as u64;
                    self.state = self.state.wrapping_mul(0x100000001b3);
                }
            }
        }

        let mut hasher = SimpleHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as usize;

        let mut index = hash & self.mask;

        // Use Option to avoid moving key/value until we actually need to insert them
        let mut key_option = Some(key);
        let mut value_option = Some(value);

        for _ in 0..self.capacity {
            let bucket = unsafe { &*self.buckets.as_ptr().add(index) };

            let current_key = bucket.key.load(Ordering::Acquire);
            if current_key.is_null() {
                // Empty slot found
                let key_to_insert = key_option.take().unwrap();
                let value_to_insert = value_option.take().unwrap();

                let key_ptr = Box::leak(Box::new(key_to_insert));
                let value_ptr = Box::leak(Box::new(value_to_insert));

                match bucket.key.compare_exchange(
                    ptr::null_mut(),
                    key_ptr,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        bucket.value.store(value_ptr, Ordering::Release);
                        self.size.fetch_add(1, Ordering::Relaxed);
                        return Ok(());
                    }
                    Err(_) => {
                        // Someone else inserted, clean up and restore our values
                        unsafe {
                            key_option = Some(*Box::from_raw(key_ptr));
                            value_option = Some(*Box::from_raw(value_ptr));
                        }
                    }
                }
            }

            // Linear probing
            index = (index + 1) & self.mask;
        }

        // Table full, return the original values
        Err((key_option.unwrap(), value_option.unwrap()))
    }

    /// Get the current size
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if table is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K, V> Drop for CacheFriendlyHashTable<K, V> {
    fn drop(&mut self) {
        // Clean up all allocated keys and values
        unsafe {
            for i in 0..self.capacity {
                let bucket = &*self.buckets.as_ptr().add(i);
                let key_ptr = bucket.key.load(Ordering::Relaxed);
                let value_ptr = bucket.value.load(Ordering::Relaxed);

                if !key_ptr.is_null() {
                    let _ = Box::from_raw(key_ptr);
                }
                if !value_ptr.is_null() {
                    let _ = Box::from_raw(value_ptr);
                }
            }

            // Deallocate buckets
            let layout = Layout::array::<Bucket<K, V>>(self.capacity).unwrap();
            dealloc(self.buckets.as_ptr() as *mut u8, layout);
        }
    }
}

unsafe impl<K: Send, V: Send> Send for CacheFriendlyHashTable<K, V> {}
unsafe impl<K: Send, V: Send> Sync for CacheFriendlyHashTable<K, V> {}

/// Memory prefetching utilities
pub mod prefetch {
    /// Prefetch data for reading
    #[inline(always)]
    pub fn prefetch_read<T>(ptr: *const T) {
        unsafe {
            core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    /// Prefetch data for writing
    #[inline(always)]
    pub fn prefetch_write<T>(ptr: *const T) {
        unsafe {
            core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T0);
        }
    }

    /// Prefetch cache line for exclusive access
    #[inline(always)]
    pub fn prefetch_exclusive<T>(ptr: *const T) {
        unsafe {
            // Use PREFETCHW instruction when available
            core::arch::asm!(
                "prefetchw ({})",
                in(reg) ptr,
                options(nostack, preserves_flags)
            );
        }
    }
}
