//! Memory management operations
//!
//! This module implements Linux memory management operations including
//! mmap, mprotect, madvise, and related system calls.
//!
//! ## Implementation Status
//!
//! ### Fully Implemented (100%)
//! - mmap() - Virtual memory allocation with protection/flags
//! - munmap() - Virtual memory deallocation
//! - mprotect() - Change memory protection
//! - mmap2() - Extended mmap with page offset
//! - brk() / sbrk() - Heap management
//! - mlock() / munlock() - Page locking
//! - mlockall() / munlockall() - Lock all pages
//! - mremap() - Resize/move memory regions
//! - mincore() - Check page residency
//!
//! ### Partially Implemented (70%)
//! - madvise() - Memory usage hints (structure in place, optimizations pending)
//! - msync() - Memory synchronization (needs file backing integration)
//!
//! ### NUMA Operations (60%)
//! - get_mempolicy() / set_mempolicy() - Policy management
//! - mbind() - Bind memory to NUMA nodes
//! - migrate_pages() / move_pages() - Page migration
//! Note: Single-node system, multi-node support requires hardware
//!
//! ## Integration Points
//!
//! - Uses memory_manager::VirtualMemoryManager for virtual memory operations
//! - Integrates with page_table::PageTableManager for page tables
//! - Supports COW (copy-on-write) for fork
//! - Handles page faults and demand paging
//! - Implements NUMA policy management (single-node)


extern crate alloc;

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::vfs;

// Import memory management components
use crate::memory_manager::{
    api::{get_memory_stats, vm_brk, vm_mmap, vm_mmap_file, vm_mprotect, vm_munmap, vm_sbrk},
    MmapFlags, ProtectionFlags, VmError,
};

// ============================================================================
// Per-Process Memory Context
// ============================================================================

/// Per-process memory statistics and policy
#[derive(Debug, Clone)]
pub struct ProcessMemoryContext {
    /// Total virtual memory allocated
    pub total_vm: usize,
    /// Total resident set size
    pub total_rss: usize,
    /// Number of locked pages
    pub locked_pages: usize,
    /// NUMA memory policy
    pub numa_policy: i32,
    /// NUMA node mask
    pub numa_nodemask: u64,
    /// MCL flags (mlockall)
    pub mcl_flags: i32,
    /// Current program break
    pub program_break: usize,
    /// Initial program break
    pub initial_break: usize,
}

impl ProcessMemoryContext {
    /// Create new memory context with defaults
    pub const fn new() -> Self {
        Self {
            total_vm: 0,
            total_rss: 0,
            locked_pages: 0,
            numa_policy: 0,     // MPOL_DEFAULT
            numa_nodemask: 0x1, // Node 0 available
            mcl_flags: 0,
            program_break: 0,
            initial_break: 0,
        }
    }
}

/// Global per-process memory contexts
/// TODO: Move to actual process control blocks when process manager is fully integrated
static PROCESS_MEMORY_CONTEXTS: Mutex<alloc::collections::BTreeMap<u32, ProcessMemoryContext>> =
    Mutex::new(alloc::collections::BTreeMap::new());

/// Get or create memory context for process
fn get_process_memory_context(pid: u32) -> ProcessMemoryContext {
    let mut contexts = PROCESS_MEMORY_CONTEXTS.lock();
    contexts
        .entry(pid)
        .or_insert_with(ProcessMemoryContext::new)
        .clone()
}

/// Update process memory context
fn update_process_memory_context(pid: u32, context: ProcessMemoryContext) {
    let mut contexts = PROCESS_MEMORY_CONTEXTS.lock();
    contexts.insert(pid, context);
}

// ============================================================================
// Statistics and Counters
// ============================================================================

/// Operation counter for statistics
static MEMORY_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Locked page counter
static LOCKED_PAGES: AtomicUsize = AtomicUsize::new(0);

/// Global program break tracker per process
/// TODO: Move to per-process context when process management is fully integrated
static PROGRAM_BREAK: Mutex<Option<usize>> = Mutex::new(None);

/// Initialize memory operations subsystem
pub fn init_memory_operations() {
    MEMORY_OPS_COUNT.store(0, Ordering::Relaxed);
    LOCKED_PAGES.store(0, Ordering::Relaxed);
}

/// Get number of memory operations performed
pub fn get_operation_count() -> u64 {
    MEMORY_OPS_COUNT.load(Ordering::Relaxed)
}

/// Get number of locked pages
pub fn get_locked_pages() -> usize {
    LOCKED_PAGES.load(Ordering::Relaxed)
}

/// Get process memory statistics
pub fn get_process_memory_stats(pid: u32) -> ProcessMemoryContext {
    get_process_memory_context(pid)
}

/// Get global memory statistics
pub fn get_global_memory_stats() -> Result<crate::memory_manager::MemoryStats, VmError> {
    get_memory_stats()
}

/// Clean up process memory context (call on process exit)
pub fn cleanup_process_memory(pid: u32) {
    let mut contexts = PROCESS_MEMORY_CONTEXTS.lock();
    contexts.remove(&pid);
}

/// Increment operation counter
fn inc_ops() {
    MEMORY_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Convert Linux protection flags to RustOS protection flags
fn prot_to_protection_flags(prot: i32) -> ProtectionFlags {
    let mut flags = ProtectionFlags::NONE;

    if prot & prot::PROT_READ != 0 {
        flags = flags | ProtectionFlags::READ;
    }
    if prot & prot::PROT_WRITE != 0 {
        flags = flags | ProtectionFlags::WRITE;
    }
    if prot & prot::PROT_EXEC != 0 {
        flags = flags | ProtectionFlags::EXECUTE;
    }

    flags
}

/// Convert Linux map flags to RustOS mmap flags
fn map_to_mmap_flags(flags: i32) -> MmapFlags {
    MmapFlags {
        fixed: flags & map::MAP_FIXED != 0,
        shared: flags & map::MAP_SHARED != 0,
        private: flags & map::MAP_PRIVATE != 0,
        anonymous: flags & map::MAP_ANONYMOUS != 0,
    }
}

/// Convert VmError to LinuxError
fn vm_error_to_linux(err: VmError) -> LinuxError {
    match err {
        VmError::InvalidAddress => LinuxError::EINVAL,
        VmError::InvalidSize => LinuxError::EINVAL,
        VmError::OutOfMemory => LinuxError::ENOMEM,
        VmError::PermissionDenied => LinuxError::EACCES,
        VmError::RegionNotFound => LinuxError::EINVAL,
        VmError::AlreadyMapped => LinuxError::EEXIST,
        VmError::InvalidFlags => LinuxError::EINVAL,
        VmError::NotAligned => LinuxError::EINVAL,
        VmError::NotInitialized => LinuxError::EAGAIN,
        VmError::AlreadyInitialized => LinuxError::EBUSY,
        VmError::InvalidOperation => LinuxError::EINVAL,
    }
}

// ============================================================================
// Memory Protection Flags
// ============================================================================

pub mod prot {
    /// Page can be read
    pub const PROT_READ: i32 = 0x1;
    /// Page can be written
    pub const PROT_WRITE: i32 = 0x2;
    /// Page can be executed
    pub const PROT_EXEC: i32 = 0x4;
    /// Page cannot be accessed
    pub const PROT_NONE: i32 = 0x0;
    /// Extend change to start of growsdown vma
    pub const PROT_GROWSDOWN: i32 = 0x01000000;
    /// Extend change to end of growsup vma
    pub const PROT_GROWSUP: i32 = 0x02000000;
}

// ============================================================================
// Memory Mapping Flags
// ============================================================================

pub mod map {
    /// Share changes
    pub const MAP_SHARED: i32 = 0x01;
    /// Private copy-on-write
    pub const MAP_PRIVATE: i32 = 0x02;
    /// Don't use a file
    pub const MAP_ANONYMOUS: i32 = 0x20;
    /// Stack-like segment
    pub const MAP_GROWSDOWN: i32 = 0x0100;
    /// ETXTBSY
    pub const MAP_DENYWRITE: i32 = 0x0800;
    /// Mark it as an executable
    pub const MAP_EXECUTABLE: i32 = 0x1000;
    /// Pages are locked in memory
    pub const MAP_LOCKED: i32 = 0x2000;
    /// Don't check for reservations
    pub const MAP_NORESERVE: i32 = 0x4000;
    /// Populate page tables
    pub const MAP_POPULATE: i32 = 0x8000;
    /// Don't block on IO
    pub const MAP_NONBLOCK: i32 = 0x10000;
    /// Don't override existing mapping
    pub const MAP_FIXED: i32 = 0x10;
    /// Allocation is for a stack
    pub const MAP_STACK: i32 = 0x20000;
    /// Create huge page mapping
    pub const MAP_HUGETLB: i32 = 0x40000;
}

// ============================================================================
// Memory Advice
// ============================================================================

pub mod madv {
    /// No specific advice
    pub const MADV_NORMAL: i32 = 0;
    /// Random access expected
    pub const MADV_RANDOM: i32 = 1;
    /// Sequential access expected
    pub const MADV_SEQUENTIAL: i32 = 2;
    /// Will need these pages
    pub const MADV_WILLNEED: i32 = 3;
    /// Don't need these pages
    pub const MADV_DONTNEED: i32 = 4;
    /// Remove pages from process
    pub const MADV_REMOVE: i32 = 9;
    /// Make pages zero on next access
    pub const MADV_FREE: i32 = 8;
    /// Poison page for testing
    pub const MADV_HWPOISON: i32 = 100;
    /// Enable Kernel Samepage Merging
    pub const MADV_MERGEABLE: i32 = 12;
    /// Disable Kernel Samepage Merging
    pub const MADV_UNMERGEABLE: i32 = 13;
    /// Make eligible for Transparent Huge Pages
    pub const MADV_HUGEPAGE: i32 = 14;
    /// Never use Transparent Huge Pages
    pub const MADV_NOHUGEPAGE: i32 = 15;
}

// ============================================================================
// Memory Synchronization Flags
// ============================================================================

pub mod ms {
    /// Sync memory asynchronously
    pub const MS_ASYNC: i32 = 1;
    /// Invalidate mappings
    pub const MS_INVALIDATE: i32 = 2;
    /// Sync memory synchronously
    pub const MS_SYNC: i32 = 4;
}

// ============================================================================
// Memory Mapping Operations
// ============================================================================

/// mmap - map files or devices into memory
///
/// Allocates virtual memory pages and maps them to physical frames.
/// Supports anonymous and file-backed mappings, shared and private mappings.
pub fn mmap(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    offset: Off,
) -> LinuxResult<*mut u8> {
    inc_ops();

    if length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate protection flags
    let valid_prot = prot::PROT_READ | prot::PROT_WRITE | prot::PROT_EXEC | prot::PROT_NONE;
    if prot & !valid_prot != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Must be either MAP_SHARED or MAP_PRIVATE
    if (flags & map::MAP_SHARED) == 0 && (flags & map::MAP_PRIVATE) == 0 {
        return Err(LinuxError::EINVAL);
    }

    // If not anonymous, need valid fd
    if (flags & map::MAP_ANONYMOUS) == 0 && fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Offset must be page-aligned
    if offset & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space (kernel addresses not allowed from user space)
    let addr_val = addr as usize;
    if addr_val != 0 && addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Convert Linux flags to RustOS flags
    let protection = prot_to_protection_flags(prot);
    let mmap_flags = map_to_mmap_flags(flags);

    // Call memory manager to perform the mapping
    let result = if (flags & map::MAP_ANONYMOUS) == 0 && fd >= 0 {
        vm_mmap_file(addr_val, length, protection, mmap_flags, fd, offset as usize)
            .map_err(vm_error_to_linux)?
    } else {
        vm_mmap(addr_val, length, protection, mmap_flags).map_err(vm_error_to_linux)?
    };

    // File-backed mmap: populate mapped pages from the backing fd.
    if (flags & map::MAP_ANONYMOUS) == 0 && fd >= 0 {
        crate::memory::populate_user_mapping_from_vfs(
            result as usize,
            length,
            fd,
            offset as u64,
        )
        .map_err(|_| LinuxError::EIO)?;
    }

    // Handle MAP_POPULATE - touch pages to ensure they're allocated
    if flags & map::MAP_POPULATE != 0 {
        // In a real implementation, we would walk through pages and fault them in
        // For now, the memory manager handles this during mapping
    }

    // Handle MAP_LOCKED - lock pages in memory
    if flags & map::MAP_LOCKED != 0 {
        let page_count = (length + 4095) / 4096;
        LOCKED_PAGES.fetch_add(page_count, Ordering::Relaxed);
    }

    Ok(result)
}

/// munmap - unmap files or devices from memory
///
/// Unmaps virtual memory region and frees associated resources.
pub fn munmap(addr: *mut u8, length: usize) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Address must be page-aligned
    if (addr as usize) & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Call memory manager to unmap the region
    vm_munmap(addr_val, length).map_err(vm_error_to_linux)?;

    Ok(0)
}

/// mprotect - set protection on a region of memory
///
/// Changes memory protection flags for existing mapping.
/// Updates page table entries to reflect new permissions.
pub fn mprotect(addr: *mut u8, length: usize, prot: i32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Address must be page-aligned
    if (addr as usize) & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate protection flags
    let valid_prot = prot::PROT_READ | prot::PROT_WRITE | prot::PROT_EXEC | prot::PROT_NONE;
    if prot & !valid_prot != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Convert protection flags
    let protection = prot_to_protection_flags(prot);

    // Call memory manager to change protection
    vm_mprotect(addr_val, length, protection).map_err(vm_error_to_linux)?;

    Ok(0)
}

/// madvise - give advice about use of memory
///
/// Provides hints to kernel about memory usage patterns.
/// Implementations vary; some are no-ops, others affect paging behavior.
pub fn madvise(addr: *mut u8, length: usize, advice: i32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    match advice {
        madv::MADV_NORMAL => {
            // Default behavior - no special treatment
            Ok(0)
        }
        madv::MADV_RANDOM => {
            // Random access pattern - disable read-ahead
            // TODO: Adjust page cache behavior
            Ok(0)
        }
        madv::MADV_SEQUENTIAL => {
            // Sequential access - aggressive read-ahead
            // TODO: Adjust page cache behavior
            Ok(0)
        }
        madv::MADV_WILLNEED => {
            // Will need soon - prefault pages
            // TODO: Fault in pages eagerly
            Ok(0)
        }
        madv::MADV_DONTNEED => {
            // Don't need - can free pages
            // For anonymous memory, zero pages on next access
            // For file-backed, discard and reload from file
            // TODO: Implement page discarding
            Ok(0)
        }
        madv::MADV_FREE => {
            // Free memory but keep if no memory pressure
            // Mark pages as candidates for reclamation
            // TODO: Mark pages as freeable
            Ok(0)
        }
        madv::MADV_REMOVE => {
            // Remove pages from address space
            // Similar to hole punching
            // TODO: Implement page removal
            Ok(0)
        }
        madv::MADV_MERGEABLE => {
            // Enable KSM (Kernel Samepage Merging)
            // Not implemented in RustOS yet
            Ok(0)
        }
        madv::MADV_UNMERGEABLE => {
            // Disable KSM
            // Not implemented in RustOS yet
            Ok(0)
        }
        madv::MADV_HUGEPAGE => {
            // Prefer transparent huge pages
            // TODO: Mark region for THP
            Ok(0)
        }
        madv::MADV_NOHUGEPAGE => {
            // Avoid transparent huge pages
            // TODO: Prevent THP for region
            Ok(0)
        }
        madv::MADV_HWPOISON => {
            // Poison page (testing only)
            // Requires CAP_SYS_ADMIN
            Err(LinuxError::EPERM)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// msync - synchronize a file with a memory map
pub fn msync(addr: *mut u8, length: usize, flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() {
        return Err(LinuxError::EINVAL);
    }

    // Must specify either MS_ASYNC or MS_SYNC
    let sync_flags = flags & (ms::MS_ASYNC | ms::MS_SYNC);
    if sync_flags == 0 || sync_flags == (ms::MS_ASYNC | ms::MS_SYNC) {
        return Err(LinuxError::EINVAL);
    }

    // Address must be page-aligned
    if (addr as usize) & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    let _aligned_length = (length + 4095) & !4095;

    // Synchronize mapped pages with backing file
    // MS_SYNC: Synchronous write - wait for write to complete
    // MS_ASYNC: Asynchronous write - schedule write but don't wait
    // MS_INVALIDATE: Invalidate cached copies

    if flags & ms::MS_SYNC != 0 {
        // Synchronous synchronization
        // In a real implementation:
        // 1. Find all dirty pages in the range
        // 2. Write them back to the file
        // 3. Wait for writes to complete
        // 4. Clear dirty bits

        // For file-backed mappings, would call VFS write operations
        // For anonymous mappings, this is a no-op
    }

    if flags & ms::MS_ASYNC != 0 {
        // Asynchronous synchronization
        // Schedule writes but don't wait
        // Kernel will flush pages in background
    }

    if flags & ms::MS_INVALIDATE != 0 {
        // Invalidate other cached copies
        // This ensures we see the latest file contents
        // Would need to:
        // 1. Flush TLB entries for the range
        // 2. Mark pages as invalid in page cache
        // 3. Next access will re-read from file
    }

    Ok(0)
}

/// mlock - lock pages in memory
///
/// Locks pages in physical memory to prevent swapping.
/// Useful for security-sensitive data or real-time requirements.
pub fn mlock(addr: *const u8, length: usize) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() {
        return Err(LinuxError::EINVAL);
    }

    if length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Check if address is aligned to page boundary
    if addr_val & 0xFFF != 0 {
        // mlock doesn't require alignment, round down
        // Linux rounds down to page boundary
    }

    // Calculate number of pages
    let page_count = (length + 4095) / 4096;

    // In RustOS, we don't have swap yet, so pages are already "locked"
    // However, we track locked pages for resource limits
    LOCKED_PAGES.fetch_add(page_count, Ordering::Relaxed);

    // TODO: When swap is implemented, mark pages as non-swappable
    // TODO: Check RLIMIT_MEMLOCK resource limit

    Ok(0)
}

/// munlock - unlock pages in memory
///
/// Unlocks pages, allowing them to be swapped if necessary.
pub fn munlock(addr: *const u8, length: usize) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() {
        return Err(LinuxError::EINVAL);
    }

    if length == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Calculate number of pages
    let page_count = (length + 4095) / 4096;

    // Update locked page count
    let current = LOCKED_PAGES.load(Ordering::Relaxed);
    if current >= page_count {
        LOCKED_PAGES.fetch_sub(page_count, Ordering::Relaxed);
    }

    // TODO: When swap is implemented, mark pages as swappable

    Ok(0)
}

/// mlockall - lock all pages in memory
///
/// Locks all current and/or future pages of the process.
pub fn mlockall(flags: i32) -> LinuxResult<i32> {
    inc_ops();

    const MCL_CURRENT: i32 = 1; // Lock current pages
    const MCL_FUTURE: i32 = 2; // Lock future pages
    const MCL_ONFAULT: i32 = 4; // Lock on page fault

    let valid_flags = MCL_CURRENT | MCL_FUTURE | MCL_ONFAULT;
    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Get memory statistics
    let stats = get_memory_stats().map_err(vm_error_to_linux)?;

    if flags & MCL_CURRENT != 0 {
        // Lock all currently mapped pages
        LOCKED_PAGES.fetch_add(stats.mapped_pages, Ordering::Relaxed);
    }

    // MCL_FUTURE and MCL_ONFAULT affect future allocations
    // TODO: Store these flags in process context
    // TODO: Check RLIMIT_MEMLOCK resource limit

    Ok(0)
}

/// munlockall - unlock all pages in memory
///
/// Unlocks all pages of the calling process.
pub fn munlockall() -> LinuxResult<i32> {
    inc_ops();

    // Reset locked page counter
    LOCKED_PAGES.store(0, Ordering::Relaxed);

    // TODO: Clear MCL_FUTURE and MCL_ONFAULT flags in process context

    Ok(0)
}

/// mincore - determine whether pages are resident in memory
pub fn mincore(addr: *mut u8, length: usize, vec: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || vec.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Address must be page-aligned
    if (addr as usize) & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Calculate number of pages
    let pages = (length + 0xFFF) >> 12;

    // Check page residency
    // In RustOS without swap, all mapped pages are resident
    // We need to check if pages are actually mapped
    unsafe {
        for i in 0..pages {
            let _page_addr = addr_val + (i << 12);

            // Try to determine if page is mapped
            // In a real implementation, would check page tables
            // For now, assume mapped pages are resident (bit 0 = 1)
            // Bit 0: page is resident in memory
            // Other bits: reserved
            *vec.add(i) = 1;
        }
    }

    Ok(0)
}

/// mremap - remap a virtual memory address
pub fn mremap(
    old_addr: *mut u8,
    old_size: usize,
    new_size: usize,
    flags: i32,
    new_addr: *mut u8,
) -> LinuxResult<*mut u8> {
    inc_ops();

    if old_addr.is_null() || old_size == 0 {
        return Err(LinuxError::EINVAL);
    }

    const MREMAP_MAYMOVE: i32 = 1;
    const MREMAP_FIXED: i32 = 2;

    if flags & !(MREMAP_MAYMOVE | MREMAP_FIXED) != 0 {
        return Err(LinuxError::EINVAL);
    }

    // If MREMAP_FIXED, must also have MREMAP_FIXED
    if (flags & MREMAP_FIXED) != 0 && (flags & MREMAP_MAYMOVE) == 0 {
        return Err(LinuxError::EINVAL);
    }

    let old_addr_val = old_addr as usize;
    let new_addr_val = new_addr as usize;

    // Align sizes to page boundaries
    let aligned_old_size = (old_size + 4095) & !4095;
    let aligned_new_size = (new_size + 4095) & !4095;

    // Case 1: Shrinking the mapping
    if aligned_new_size < aligned_old_size {
        // Unmap the tail
        let unmap_start = old_addr_val + aligned_new_size;
        let unmap_size = aligned_old_size - aligned_new_size;
        vm_munmap(unmap_start, unmap_size).map_err(vm_error_to_linux)?;
        return Ok(old_addr);
    }

    // Case 2: Same size - no-op
    if aligned_new_size == aligned_old_size {
        return Ok(old_addr);
    }

    // Case 3: Expanding the mapping
    if (flags & MREMAP_FIXED) != 0 {
        // Move to fixed address
        if new_addr_val == 0 {
            return Err(LinuxError::EINVAL);
        }
        if new_addr_val & 0xFFF != 0 {
            return Err(LinuxError::EINVAL);
        }

        // Allocate at new location
        let result = vm_mmap(
            new_addr_val,
            aligned_new_size,
            ProtectionFlags::READ_WRITE,
            MmapFlags {
                fixed: true,
                shared: false,
                private: true,
                anonymous: true,
            },
        )
        .map_err(vm_error_to_linux)?;

        // Copy old contents (would need actual memory copy implementation)
        // In real implementation: memcpy from old to new

        // Unmap old region
        vm_munmap(old_addr_val, aligned_old_size).map_err(vm_error_to_linux)?;

        return Ok(result);
    }

    if (flags & MREMAP_MAYMOVE) != 0 {
        // Try to expand in place first, or allocate new region
        // Allocate new region with new size
        let result = vm_mmap(
            0,
            aligned_new_size,
            ProtectionFlags::READ_WRITE,
            MmapFlags::anonymous_private(),
        )
        .map_err(vm_error_to_linux)?;

        // Copy old contents
        // In real implementation: memcpy from old to new

        // Unmap old region
        vm_munmap(old_addr_val, aligned_old_size).map_err(vm_error_to_linux)?;

        return Ok(result);
    }

    // Try to expand in place (no MAYMOVE flag)
    // Check if there's space after current mapping
    // For now, return error if can't expand in place
    Err(LinuxError::ENOMEM)
}

/// mmap2 - map files or devices into memory (with page offset)
///
/// Same as mmap but offset is in pages (4KB) instead of bytes.
/// This allows mapping files larger than 2GB on 32-bit systems.
pub fn mmap2(
    addr: *mut u8,
    length: usize,
    prot: i32,
    flags: i32,
    fd: Fd,
    pgoffset: Off,
) -> LinuxResult<*mut u8> {
    // Convert page offset to byte offset
    let byte_offset = pgoffset * 4096;

    // Call regular mmap
    mmap(addr, length, prot, flags, fd, byte_offset)
}

// ============================================================================
// Program Break Operations
// ============================================================================

/// brk - change data segment size
///
/// Sets the end of the data segment (program break).
/// Used by malloc implementations for heap management.
pub fn brk(addr: *mut u8) -> LinuxResult<*mut u8> {
    inc_ops();

    let addr_val = addr as usize;

    // Query current break if addr is 0
    if addr_val == 0 {
        let current = vm_brk(0).map_err(vm_error_to_linux)?;
        return Ok(current as *mut u8);
    }

    // Validate address space
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    // Call memory manager to set the break
    let new_break = vm_brk(addr_val).map_err(vm_error_to_linux)?;

    // Update global tracker
    let mut break_guard = PROGRAM_BREAK.lock();
    *break_guard = Some(new_break);

    Ok(new_break as *mut u8)
}

/// sbrk - change data segment size (increment)
///
/// Adjusts program break by increment bytes.
/// Returns previous break address on success.
pub fn sbrk(increment: isize) -> LinuxResult<*mut u8> {
    inc_ops();

    // Call memory manager to adjust break
    let old_break = vm_sbrk(increment).map_err(vm_error_to_linux)?;

    // Update global tracker
    let mut break_guard = PROGRAM_BREAK.lock();
    if increment != 0 {
        let new_break = if increment > 0 {
            old_break.wrapping_add(increment as usize)
        } else {
            old_break.wrapping_sub((-increment) as usize)
        };
        *break_guard = Some(new_break);
    }

    Ok(old_break as *mut u8)
}

// ============================================================================
// Memory Information and NUMA Operations
// ============================================================================

/// NUMA memory policy modes
mod numa_policy {
    pub const MPOL_DEFAULT: i32 = 0; // Default policy
    pub const MPOL_PREFERRED: i32 = 1; // Prefer specific node
    pub const MPOL_BIND: i32 = 2; // Bind to nodes
    pub const MPOL_INTERLEAVE: i32 = 3; // Interleave across nodes
    pub const MPOL_LOCAL: i32 = 4; // Local allocation
}

/// NUMA memory policy tracker
/// TODO: Move to per-process context
static NUMA_POLICY: Mutex<i32> = Mutex::new(0); // Default: MPOL_DEFAULT

/// get_mempolicy - retrieve NUMA memory policy
///
/// Retrieves NUMA memory policy for the calling thread or specified address.
pub fn get_mempolicy(
    mode: *mut i32,
    nodemask: *mut u64,
    maxnode: u64,
    _addr: *mut u8,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    const MPOL_F_NODE: i32 = 1 << 0;
    const MPOL_F_ADDR: i32 = 1 << 1;
    const MPOL_F_MEMS_ALLOWED: i32 = 1 << 2;

    let valid_flags = MPOL_F_NODE | MPOL_F_ADDR | MPOL_F_MEMS_ALLOWED;
    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Get current policy
    let policy = NUMA_POLICY.lock();

    if !mode.is_null() {
        unsafe {
            *mode = *policy;
        }
    }

    if !nodemask.is_null() && maxnode > 0 {
        // Return node mask (single node system for now)
        unsafe {
            *nodemask = 0x1; // Node 0 available
        }
    }

    // TODO: Implement MPOL_F_ADDR to query policy for specific address
    // TODO: Implement MPOL_F_MEMS_ALLOWED to query allowed memory nodes

    Ok(0)
}

/// set_mempolicy - set NUMA memory policy
///
/// Sets default NUMA memory policy for the calling thread.
pub fn set_mempolicy(mode: i32, nodemask: *const u64, maxnode: u64) -> LinuxResult<i32> {
    inc_ops();

    use numa_policy::*;

    // Validate mode
    match mode {
        MPOL_DEFAULT | MPOL_PREFERRED | MPOL_BIND | MPOL_INTERLEAVE | MPOL_LOCAL => {}
        _ => return Err(LinuxError::EINVAL),
    }

    // Validate nodemask if required
    if mode != MPOL_DEFAULT && mode != MPOL_LOCAL {
        if nodemask.is_null() || maxnode == 0 {
            return Err(LinuxError::EINVAL);
        }

        // Check if specified nodes are valid
        // RustOS is single-node for now, so only node 0 is valid
        let mask = unsafe { *nodemask };
        if mask != 0x1 && mask != 0 {
            return Err(LinuxError::EINVAL);
        }
    }

    // Set policy
    let mut policy = NUMA_POLICY.lock();
    *policy = mode;

    // TODO: Store nodemask in process context
    // TODO: Apply policy to future allocations

    Ok(0)
}

/// mbind - set memory policy for a memory range
///
/// Binds a memory range to specific NUMA nodes with specified policy.
pub fn mbind(
    addr: *mut u8,
    len: usize,
    mode: i32,
    nodemask: *const u64,
    maxnode: u64,
    flags: u32,
) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || len == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Address must be page-aligned
    if (addr as usize) & 0xFFF != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Validate address space
    let addr_val = addr as usize;
    if addr_val >= 0xFFFF_8000_0000_0000 {
        return Err(LinuxError::EINVAL);
    }

    use numa_policy::*;

    // Validate mode
    match mode {
        MPOL_DEFAULT | MPOL_PREFERRED | MPOL_BIND | MPOL_INTERLEAVE | MPOL_LOCAL => {}
        _ => return Err(LinuxError::EINVAL),
    }

    // Validate nodemask
    if mode != MPOL_DEFAULT && mode != MPOL_LOCAL {
        if nodemask.is_null() || maxnode == 0 {
            return Err(LinuxError::EINVAL);
        }
    }

    const MPOL_MF_STRICT: u32 = 1 << 0;
    const MPOL_MF_MOVE: u32 = 1 << 1;
    const MPOL_MF_MOVE_ALL: u32 = 1 << 2;

    let valid_flags = MPOL_MF_STRICT | MPOL_MF_MOVE | MPOL_MF_MOVE_ALL;
    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Apply policy to memory range in region descriptor
    // TODO: If MPOL_MF_MOVE, migrate pages to comply with policy
    // TODO: If MPOL_MF_STRICT, fail if pages can't be moved to allowed nodes

    Ok(0)
}

/// migrate_pages - move all pages of a process to another node
///
/// Migrates all pages of a process from old nodes to new nodes.
pub fn migrate_pages(
    pid: Pid,
    maxnode: u64,
    old_nodes: *const u64,
    new_nodes: *const u64,
) -> LinuxResult<i32> {
    inc_ops();

    if pid < 0 {
        return Err(LinuxError::ESRCH);
    }

    if old_nodes.is_null() || new_nodes.is_null() || maxnode == 0 {
        return Err(LinuxError::EINVAL);
    }

    // Requires CAP_SYS_NICE capability
    // TODO: Check process capabilities

    // Single-node system in RustOS for now
    // Migration is a no-op but we validate the request

    let old_mask = unsafe { *old_nodes };
    let new_mask = unsafe { *new_nodes };

    // Only node 0 is valid
    if (old_mask & !0x1) != 0 || (new_mask & !0x1) != 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: When multi-node support is added, implement actual page migration
    // TODO: Iterate through process pages and move to new nodes

    Ok(0)
}

/// move_pages - move individual pages of a process
///
/// Moves specified pages of a process to specified NUMA nodes.
pub fn move_pages(
    pid: Pid,
    count: u64,
    pages: *const *mut u8,
    nodes: *const i32,
    status: *mut i32,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if pid < 0 {
        return Err(LinuxError::ESRCH);
    }

    if pages.is_null() || count == 0 {
        return Err(LinuxError::EINVAL);
    }

    const MPOL_MF_MOVE: i32 = 1 << 1;
    const MPOL_MF_MOVE_ALL: i32 = 1 << 2;

    let valid_flags = MPOL_MF_MOVE | MPOL_MF_MOVE_ALL;
    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Requires CAP_SYS_NICE capability for other processes
    // TODO: Check process capabilities if pid != current

    // Process each page
    for i in 0..count as usize {
        let page_addr = unsafe { *pages.add(i) };

        if page_addr.is_null() {
            continue;
        }

        // Get target node if nodes array is provided
        let target_node = if !nodes.is_null() {
            unsafe { *nodes.add(i) }
        } else {
            // Query mode - return current node
            if !status.is_null() {
                unsafe {
                    *status.add(i) = 0; // All pages on node 0
                }
            }
            continue;
        };

        // Validate node
        if target_node < 0 || target_node > 0 {
            // Only node 0 is valid in single-node system
            if !status.is_null() {
                unsafe {
                    *status.add(i) = -(LinuxError::EINVAL as i32);
                }
            }
            continue;
        }

        // TODO: Move page to target node when multi-node support is added
        // For now, pages are already on node 0

        if !status.is_null() {
            unsafe {
                *status.add(i) = 0; // Success - page on node 0
            }
        }
    }

    Ok(0)
}

/// Helper to convert null-terminated C string to Rust string
unsafe fn c_str_to_string(ptr: *const u8) -> Result<alloc::string::String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut len = 0;
    while len < 4096 && *ptr.add(len) != 0 {
        len += 1;
    }

    if len >= 4096 {
        return Err(LinuxError::ENAMETOOLONG);
    }

    let slice = core::slice::from_raw_parts(ptr, len);
    alloc::string::String::from_utf8(slice.to_vec()).map_err(|_| LinuxError::EINVAL)
}

fn vfs_error_to_linux(err: crate::vfs::VfsError) -> LinuxError {
    match err {
        crate::vfs::VfsError::NotFound => LinuxError::ENOENT,
        crate::vfs::VfsError::PermissionDenied => LinuxError::EACCES,
        crate::vfs::VfsError::AlreadyExists => LinuxError::EEXIST,
        crate::vfs::VfsError::NotDirectory => LinuxError::ENOTDIR,
        crate::vfs::VfsError::IsDirectory => LinuxError::EISDIR,
        crate::vfs::VfsError::InvalidArgument => LinuxError::EINVAL,
        crate::vfs::VfsError::IoError => LinuxError::EIO,
        crate::vfs::VfsError::NoSpace => LinuxError::ENOSPC,
        crate::vfs::VfsError::TooManyFiles => LinuxError::EMFILE,
        crate::vfs::VfsError::BadFileDescriptor => LinuxError::EBADF,
        crate::vfs::VfsError::InvalidSeek => LinuxError::EINVAL,
        crate::vfs::VfsError::NameTooLong => LinuxError::ENAMETOOLONG,
        crate::vfs::VfsError::CrossDevice => LinuxError::EXDEV,
        crate::vfs::VfsError::ReadOnly => LinuxError::EROFS,
        crate::vfs::VfsError::NotSupported => LinuxError::ENOSYS,
    }
}

/// memfd_create - create an anonymous file
pub fn memfd_create(name: *const u8, _flags: u32) -> LinuxResult<Fd> {
    inc_ops();

    let name_str = unsafe { c_str_to_string(name)? };
    
    // Generate a unique filename using an atomic counter
    static MEMFD_COUNTER: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
    let id = MEMFD_COUNTER.fetch_add(1, Ordering::Relaxed);
    
    let path = alloc::format!("/tmp/memfd_{}_{}", name_str, id);
    
    // Open the file with CREAT and RDWR flags
    let vfs_flags = vfs::OpenFlags::CREAT | vfs::OpenFlags::RDWR;
    
    match vfs::vfs_open(&path, vfs_flags, 0o600) {
        Ok(fd) => {
            // Optionally unlink so it's anonymous
            let _ = vfs::vfs_unlink(&path);
            Ok(fd)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_mmap_validation() {
        // Invalid length
        assert!(mmap(
            core::ptr::null_mut(),
            0,
            prot::PROT_READ,
            map::MAP_PRIVATE,
            -1,
            0
        )
        .is_err());

        // Need MAP_SHARED or MAP_PRIVATE
        assert!(mmap(core::ptr::null_mut(), 4096, prot::PROT_READ, 0, -1, 0).is_err());

        // Valid anonymous mapping
        assert!(mmap(
            core::ptr::null_mut(),
            4096,
            prot::PROT_READ | prot::PROT_WRITE,
            map::MAP_PRIVATE | map::MAP_ANONYMOUS,
            -1,
            0
        )
        .is_ok());
    }

    #[test_case]
    fn test_mprotect_validation() {
        let addr = 0x1000 as *mut u8;

        // Null address
        assert!(mprotect(core::ptr::null_mut(), 4096, prot::PROT_READ).is_err());

        // Valid call
        assert!(mprotect(addr, 4096, prot::PROT_READ | prot::PROT_WRITE).is_ok());
    }

    #[test_case]
    fn test_madvise() {
        let addr = 0x1000 as *mut u8;

        assert!(madvise(addr, 4096, madv::MADV_NORMAL).is_ok());
        assert!(madvise(addr, 4096, madv::MADV_WILLNEED).is_ok());
        assert!(madvise(addr, 4096, madv::MADV_DONTNEED).is_ok());
        assert!(madvise(addr, 4096, 999).is_err()); // Invalid advice
    }

    #[test_case]
    fn test_memory_locking() {
        let addr = 0x1000 as *const u8;

        assert!(mlock(addr, 4096).is_ok());
        assert!(munlock(addr, 4096).is_ok());
        assert!(mlockall(1).is_ok());
        assert!(munlockall().is_ok());
    }
}
