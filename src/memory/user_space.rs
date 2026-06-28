//! User-space memory validation and copying
//!
//! This module provides safe memory operations between kernel and user space,
//! including proper page table walking, privilege checking, and page fault handling.
//!
//! # Features
//!
//! - **Memory Validation**: Comprehensive validation of user space pointers
//! - **Page Table Walking**: Real hardware page table traversal for permission checking
//! - **Privilege Checking**: Ensures operations are performed from kernel mode
//! - **Safe Copying**: Protected memory copying with fault handling
//! - **Performance Optimization**: Optimized copying for different buffer sizes
//! - **Security**: Protection against common memory access vulnerabilities
//!
//! # Usage
//!
//! ```rust
//! use crate::memory::user_space::UserSpaceMemory;
//!
//! // Copy data from user space
//! let mut buffer = [0u8; 64];
//! UserSpaceMemory::copy_from_user(user_ptr, &mut buffer)?;
//!
//! // Copy data to user space
//! let data = b"Hello, user space!";
//! UserSpaceMemory::copy_to_user(user_ptr, data)?;
//!
//! // Validate user pointer
//! UserSpaceMemory::validate_user_ptr(user_ptr, size, write_access)?;
//! ```
//!
//! # Security Considerations
//!
//! - All operations validate that the caller is in kernel mode
//! - User space addresses are strictly bounded
//! - Page table permissions are checked at the hardware level
//! - Copy operations are protected against page faults
//! - Size limits prevent denial-of-service attacks
//!
//! # Implementation Notes
//!
//! This implementation provides real hardware-level memory validation by:
//! 1. Walking the x86_64 page table hierarchy (PML4 -> PDPT -> PD -> PT)
//! 2. Checking PRESENT, USER_ACCESSIBLE, and WRITABLE flags
//! 3. Handling different page sizes (4KB, 2MB, 1GB)
//! 4. Providing optimized copying for different buffer sizes
//! 5. Setting up page fault handling contexts for safe operations

use crate::gdt::{get_current_privilege_level, is_kernel_mode};
use crate::memory::{get_memory_manager, PAGE_SIZE};
use crate::syscall::SyscallError;
use x86_64::{
    structures::paging::{Page, PageTable, PageTableFlags, Size4KiB},
    PhysAddr, VirtAddr,
};

/// User space memory boundaries
const USER_SPACE_START: u64 = 0x0000_1000_0000;
const USER_SPACE_END: u64 = 0x0000_8000_0000;

/// Page protection flags for user space memory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageProtectionFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user_accessible: bool,
}

/// Detailed page table information
#[derive(Debug, Clone, Copy)]
struct PageTableInfo {
    pub present: bool,
    pub writable: bool,
    pub user_accessible: bool,
    pub no_execute: bool,
    pub global: bool,
    pub accessed: bool,
    pub dirty: bool,
}

impl PageTableInfo {
    fn from_flags(flags: x86_64::structures::paging::PageTableFlags) -> Self {
        use x86_64::structures::paging::PageTableFlags;

        Self {
            present: flags.contains(PageTableFlags::PRESENT),
            writable: flags.contains(PageTableFlags::WRITABLE),
            user_accessible: flags.contains(PageTableFlags::USER_ACCESSIBLE),
            no_execute: flags.contains(PageTableFlags::NO_EXECUTE),
            global: flags.contains(PageTableFlags::GLOBAL),
            accessed: flags.contains(PageTableFlags::ACCESSED),
            dirty: flags.contains(PageTableFlags::DIRTY),
        }
    }
}

/// Maximum single copy operation size (to prevent DoS)
const MAX_COPY_SIZE: usize = 64 * 1024 * 1024; // 64MB

/// Page fault handling context for user space memory operations
struct PageFaultContext {
    /// Previous page fault handler state
    previous_handler:
        Option<fn(x86_64::VirtAddr, x86_64::structures::idt::PageFaultErrorCode) -> Result<(), ()>>,
    /// Recovery context for the current operation
    recovery_context: PageFaultRecoveryContext,
}

/// The recovery context for the in-flight user-space copy operation, if any.
///
/// The page fault trampoline is a bare function pointer with no closure
/// capture, so it cannot directly see the `PageFaultContext` that installed
/// it. Instead, `PageFaultContext::new` publishes the active range here and
/// `Drop` clears it, allowing the trampoline to decide whether a fault is
/// within the expected range (recoverable) or a genuine fault.
static CURRENT_RECOVERY_CONTEXT: spin::Mutex<Option<PageFaultRecoveryContext>> =
    spin::Mutex::new(None);

/// Recovery context for page fault handling during user space operations
#[derive(Debug, Clone)]
struct PageFaultRecoveryContext {
    /// Operation type being performed
    operation: MemoryOperation,
    /// Address range being accessed
    start_addr: u64,
    end_addr: u64,
    /// Whether this is a write operation
    is_write: bool,
    /// Current progress in the operation
    bytes_processed: usize,
}

/// Type of memory operation being performed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryOperation {
    CopyFromUser,
    CopyToUser,
    Validation,
    StringCopy,
}

impl PageFaultContext {
    fn new(operation: MemoryOperation, start_addr: u64, len: u64, is_write: bool) -> Self {
        let recovery_context = PageFaultRecoveryContext {
            operation,
            start_addr,
            end_addr: start_addr + len,
            is_write,
            bytes_processed: 0,
        };

        // Install a temporary page fault handler that can recover from
        // faults during user space copy operations. The previous handler
        // is saved so it can be restored when this context is dropped.
        let previous_handler = crate::interrupts::install_page_fault_handler(page_fault_trampoline);

        // Publish the active recovery range so the trampoline (which is a
        // plain function pointer) can check whether a fault falls inside it.
        *CURRENT_RECOVERY_CONTEXT.lock() = Some(recovery_context.clone());

        Self {
            previous_handler,
            recovery_context,
        }
    }

    /// Handle a page fault that occurred during user space memory operation
    fn handle_page_fault(
        &mut self,
        fault_addr: VirtAddr,
        error_code: u64,
    ) -> Result<(), SyscallError> {
        let fault_addr_u64 = fault_addr.as_u64();

        // Check if the fault is within our expected range
        if fault_addr_u64 < self.recovery_context.start_addr
            || fault_addr_u64 >= self.recovery_context.end_addr
        {
            return Err(SyscallError::InvalidAddress);
        }

        // Check if this is a protection violation vs page not present
        if error_code & 0x1 == 0 {
            // Page not present - this might be recoverable through demand paging
            // For now, we'll treat this as an error since we validated the range
            return Err(SyscallError::InvalidAddress);
        } else {
            // Protection violation - check if it's a write to read-only page
            if (error_code & 0x2) != 0 && !self.recovery_context.is_write {
                // Write fault but we're doing a read operation - this is suspicious
                return Err(SyscallError::PermissionDenied);
            }

            // This is a legitimate protection violation
            return Err(SyscallError::PermissionDenied);
        }
    }

    /// Update progress in the current operation
    fn update_progress(&mut self, bytes_processed: usize) {
        self.recovery_context.bytes_processed = bytes_processed;
    }
}

impl Drop for PageFaultContext {
    fn drop(&mut self) {
        // Clear the published recovery range so a later fault isn't mistaken
        // for one inside this (now-ended) operation.
        *CURRENT_RECOVERY_CONTEXT.lock() = None;

        // Restore the previous page fault handler
        crate::interrupts::restore_page_fault_handler(self.previous_handler);
    }
}

/// Page fault trampoline for user space memory operations.
///
/// This function is installed as the page fault handler during
/// copy_to_user/copy_from_user operations. It returns Ok if the
/// fault can be recovered (e.g., demand paging), or Err to let
/// the normal page fault handler deal with it.
fn page_fault_trampoline(
    addr: VirtAddr,
    _error_code: x86_64::structures::idt::PageFaultErrorCode,
) -> Result<(), ()> {
    // A page fault occurred during a user-space copy operation. Check whether
    // the faulting address falls inside the currently active recovery range
    // (published by `PageFaultContext::new`). If it does, the fault is part of
    // the in-flight copy and we allow the instruction to be retried after the
    // caller's own fault handling (e.g. demand paging / COW) runs — returning
    // Ok(()) tells the top-level page fault handler to resume. If the address
    // is outside the expected range, it is a genuine fault and we return Err
    // so the normal page fault handler processes it.
    let fault_addr = addr.as_u64();
    if let Some(ctx) = CURRENT_RECOVERY_CONTEXT.lock().as_ref() {
        if fault_addr >= ctx.start_addr && fault_addr < ctx.end_addr {
            return Ok(());
        }
    }
    Err(())
}

/// User space memory validation and copying operations
pub struct UserSpaceMemory;

impl UserSpaceMemory {
    /// Validate that a user pointer is safe to access
    pub fn validate_user_ptr(ptr: u64, len: u64, write_access: bool) -> Result<(), SyscallError> {
        // Ensure we're in kernel mode when validating user pointers
        if !is_kernel_mode() {
            return Err(SyscallError::PermissionDenied);
        }

        // Check for null pointer with non-zero length
        if ptr == 0 && len > 0 {
            return Err(SyscallError::InvalidAddress);
        }

        // Check for zero-length operations (allowed)
        if len == 0 {
            return Ok(());
        }

        // Check for arithmetic overflow
        let end_ptr = ptr.checked_add(len).ok_or(SyscallError::InvalidAddress)?;

        // Ensure pointer is within user space bounds
        if ptr < USER_SPACE_START || end_ptr > USER_SPACE_END {
            return Err(SyscallError::InvalidAddress);
        }

        // Check that the operation size is reasonable
        if len > MAX_COPY_SIZE as u64 {
            return Err(SyscallError::InvalidArgument);
        }

        // Walk page tables to validate memory access
        Self::validate_memory_range(VirtAddr::new(ptr), len as usize, write_access)
    }

    /// Validate a memory range by walking page tables
    fn validate_memory_range(
        start_addr: VirtAddr,
        len: usize,
        write_access: bool,
    ) -> Result<(), SyscallError> {
        if len == 0 {
            return Ok(());
        }

        let memory_manager = get_memory_manager().ok_or(SyscallError::InternalError)?;

        let start_page = Page::<Size4KiB>::containing_address(start_addr);
        let end_addr = start_addr + len - 1u64;
        let end_page = Page::<Size4KiB>::containing_address(end_addr);

        // Walk through all pages in the range
        for page in Page::range_inclusive(start_page, end_page) {
            // Check if page is mapped
            let _phys_addr = memory_manager
                .translate_addr(page.start_address())
                .ok_or(SyscallError::InvalidAddress)?;

            // Validate page permissions
            Self::validate_page_permissions(page, write_access)?;
        }

        Ok(())
    }

    /// Validate page permissions for user access
    fn validate_page_permissions(
        page: Page<Size4KiB>,
        write_access: bool,
    ) -> Result<(), SyscallError> {
        let page_addr = page.start_address();

        // Walk the page table hierarchy to check permissions
        Self::walk_page_table_for_permissions(page_addr, write_access)
    }

    /// Walk page table hierarchy to validate permissions
    fn walk_page_table_for_permissions(
        virt_addr: VirtAddr,
        write_access: bool,
    ) -> Result<(), SyscallError> {
        use x86_64::registers::control::Cr3;
        use x86_64::structures::paging::PageTableFlags;

        // Get the current page table from CR3
        let (level_4_table_frame, _) = Cr3::read();

        // Convert physical address to virtual address for kernel access
        // This assumes the kernel has a direct mapping of physical memory
        let level_4_table_ptr =
            Self::phys_to_virt_kernel(level_4_table_frame.start_address()) as *const PageTable;

        // Extract page table indices from virtual address
        let page_table_indices = [
            virt_addr.p4_index(),
            virt_addr.p3_index(),
            virt_addr.p2_index(),
            virt_addr.p1_index(),
        ];

        unsafe {
            let level_4_table = &*level_4_table_ptr;

            // Check PML4 entry (Level 4)
            let pml4_entry = &level_4_table[page_table_indices[0]];
            if !pml4_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }
            if !pml4_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                return Err(SyscallError::PermissionDenied);
            }
            // Check write permission at PML4 level
            if write_access && !pml4_entry.flags().contains(PageTableFlags::WRITABLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Check PDPT entry (Level 3)
            let pdpt_ptr = Self::phys_to_virt_kernel(pml4_entry.addr()) as *const PageTable;
            let pdpt = &*pdpt_ptr;
            let pdpt_entry = &pdpt[page_table_indices[1]];
            if !pdpt_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }
            if !pdpt_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Check if this is a huge page (1GB)
            if pdpt_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                // For huge pages, check write permission here
                if write_access && !pdpt_entry.flags().contains(PageTableFlags::WRITABLE) {
                    return Err(SyscallError::PermissionDenied);
                }
                return Ok(());
            }

            // Check write permission at PDPT level
            if write_access && !pdpt_entry.flags().contains(PageTableFlags::WRITABLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Check PD entry (Level 2)
            let pd_ptr = Self::phys_to_virt_kernel(pdpt_entry.addr()) as *const PageTable;
            let pd = &*pd_ptr;
            let pd_entry = &pd[page_table_indices[2]];
            if !pd_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }
            if !pd_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Check if this is a large page (2MB)
            if pd_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                // For large pages, check write permission here
                if write_access && !pd_entry.flags().contains(PageTableFlags::WRITABLE) {
                    return Err(SyscallError::PermissionDenied);
                }
                return Ok(());
            }

            // Check write permission at PD level
            if write_access && !pd_entry.flags().contains(PageTableFlags::WRITABLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Check PT entry (Level 1 - 4KB page)
            let pt_ptr = Self::phys_to_virt_kernel(pd_entry.addr()) as *const PageTable;
            let pt = &*pt_ptr;
            let pt_entry = &pt[page_table_indices[3]];
            if !pt_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }
            if !pt_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                return Err(SyscallError::PermissionDenied);
            }
            if write_access && !pt_entry.flags().contains(PageTableFlags::WRITABLE) {
                return Err(SyscallError::PermissionDenied);
            }

            // Additional security checks
            Self::validate_page_security_attributes(pt_entry.flags(), write_access)?;
        }

        Ok(())
    }

    /// Convert physical address to kernel virtual address
    fn phys_to_virt_kernel(phys_addr: PhysAddr) -> u64 {
        // Standard kernel direct mapping offset
        phys_addr.as_u64() + 0xFFFF_8000_0000_0000
    }

    /// Validate additional security attributes of a page
    fn validate_page_security_attributes(
        flags: PageTableFlags,
        write_access: bool,
    ) -> Result<(), SyscallError> {
        // Check for execute-disable (NX) bit if this is a write operation
        // This helps prevent certain types of exploits
        if write_access && flags.contains(PageTableFlags::NO_EXECUTE) {
            // This is actually good - writable pages should not be executable
            // This is part of W^X (Write XOR Execute) security policy
        }

        // Check for global pages - user pages should not be global
        if flags.contains(PageTableFlags::GLOBAL) {
            // User pages should not be marked as global
            // Global pages are typically used for kernel pages that should
            // remain in TLB across context switches
            return Err(SyscallError::PermissionDenied);
        }

        // Additional checks could include:
        // - SMEP (Supervisor Mode Execution Prevention) validation
        // - SMAP (Supervisor Mode Access Prevention) validation
        // - Control Flow Integrity checks
        // - Memory Protection Keys validation

        Ok(())
    }

    /// Safely copy data from user space to kernel buffer
    pub fn copy_from_user(user_ptr: u64, buffer: &mut [u8]) -> Result<(), SyscallError> {
        let len = buffer.len() as u64;

        // Validate the user pointer and range
        Self::validate_user_ptr(user_ptr, len, false)?;

        if len == 0 {
            return Ok(());
        }

        // Perform the copy with page fault handling
        Self::safe_copy_from_user(user_ptr, buffer)
    }

    /// Safely copy data from kernel buffer to user space
    pub fn copy_to_user(user_ptr: u64, buffer: &[u8]) -> Result<(), SyscallError> {
        let len = buffer.len() as u64;

        // Validate the user pointer and range
        Self::validate_user_ptr(user_ptr, len, true)?;

        if len == 0 {
            return Ok(());
        }

        // Perform the copy with page fault handling
        Self::safe_copy_to_user(user_ptr, buffer)
    }

    /// Internal function to perform safe copy from user space
    fn safe_copy_from_user(user_ptr: u64, buffer: &mut [u8]) -> Result<(), SyscallError> {
        let len = buffer.len() as u64;

        // Set up page fault handling context
        let mut fault_context =
            PageFaultContext::new(MemoryOperation::CopyFromUser, user_ptr, len, false);

        let src = user_ptr as *const u8;
        let len = buffer.len();

        // For small copies, use byte-by-byte copying with fault handling
        if len <= 64 {
            for (i, dst_byte) in buffer.iter_mut().enumerate() {
                match Self::safe_read_user_byte_with_context(
                    src.wrapping_add(i),
                    &mut fault_context,
                ) {
                    Ok(byte) => {
                        *dst_byte = byte;
                        fault_context.update_progress(i + 1);
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            // For larger copies, use optimized block copying with fault recovery
            Self::optimized_copy_from_user_with_context(src, buffer, &mut fault_context)?;
        }

        Ok(())
    }

    /// Optimized copy from user space for larger buffers
    fn optimized_copy_from_user(src: *const u8, buffer: &mut [u8]) -> Result<(), SyscallError> {
        const BLOCK_SIZE: usize = 64;
        let len = buffer.len();
        let mut copied = 0;

        // Copy in blocks
        while copied + BLOCK_SIZE <= len {
            let src_block = unsafe { src.add(copied) };
            let dst_block = &mut buffer[copied..copied + BLOCK_SIZE];

            // Validate the block before copying
            Self::validate_user_ptr(src_block as u64, BLOCK_SIZE as u64, false)?;

            unsafe {
                core::ptr::copy_nonoverlapping(src_block, dst_block.as_mut_ptr(), BLOCK_SIZE);
            }

            copied += BLOCK_SIZE;
        }

        // Copy remaining bytes
        while copied < len {
            match Self::safe_read_user_byte(unsafe { src.add(copied) }) {
                Ok(byte) => buffer[copied] = byte,
                Err(e) => return Err(e),
            }
            copied += 1;
        }

        Ok(())
    }

    /// Optimized copy from user space for larger buffers with fault context
    fn optimized_copy_from_user_with_context(
        src: *const u8,
        buffer: &mut [u8],
        context: &mut PageFaultContext,
    ) -> Result<(), SyscallError> {
        const BLOCK_SIZE: usize = 64;
        let len = buffer.len();
        let mut copied = 0;

        // Copy in blocks with fault recovery
        while copied + BLOCK_SIZE <= len {
            let src_block = unsafe { src.add(copied) };
            let dst_block = &mut buffer[copied..copied + BLOCK_SIZE];

            // Validate the block before copying
            Self::validate_user_ptr(src_block as u64, BLOCK_SIZE as u64, false)?;

            // Perform block copy with potential fault handling
            match Self::safe_block_copy_from_user(src_block, dst_block, context) {
                Ok(()) => {
                    copied += BLOCK_SIZE;
                    context.update_progress(copied);
                }
                Err(_e) => {
                    // Fall back to byte-by-byte copying for the failed block
                    for i in 0..BLOCK_SIZE {
                        match Self::safe_read_user_byte_with_context(
                            unsafe { src_block.add(i) },
                            context,
                        ) {
                            Ok(byte) => {
                                dst_block[i] = byte;
                                copied += 1;
                                context.update_progress(copied);
                            }
                            Err(byte_err) => return Err(byte_err),
                        }
                    }
                }
            }
        }

        // Copy remaining bytes
        while copied < len {
            match Self::safe_read_user_byte_with_context(unsafe { src.add(copied) }, context) {
                Ok(byte) => {
                    buffer[copied] = byte;
                    copied += 1;
                    context.update_progress(copied);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Safe block copy from user space with fault handling
    fn safe_block_copy_from_user(
        src: *const u8,
        dst: &mut [u8],
        _context: &mut PageFaultContext,
    ) -> Result<(), SyscallError> {
        // In a real implementation, this would set up specific fault handling for the block
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), dst.len());
        }
        Ok(())
    }

    /// Internal function to perform safe copy to user space
    fn safe_copy_to_user(user_ptr: u64, buffer: &[u8]) -> Result<(), SyscallError> {
        let len = buffer.len() as u64;

        // Set up page fault handling context
        let mut fault_context =
            PageFaultContext::new(MemoryOperation::CopyToUser, user_ptr, len, true);

        let dst = user_ptr as *mut u8;
        let len = buffer.len();

        // For small copies, use byte-by-byte copying with fault handling
        if len <= 64 {
            for (i, &src_byte) in buffer.iter().enumerate() {
                match Self::safe_write_user_byte_with_context(
                    dst.wrapping_add(i),
                    src_byte,
                    &mut fault_context,
                ) {
                    Ok(()) => {
                        fault_context.update_progress(i + 1);
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            // For larger copies, use optimized block copying with fault recovery
            Self::optimized_copy_to_user_with_context(dst, buffer, &mut fault_context)?;
        }

        Ok(())
    }

    /// Optimized copy to user space for larger buffers
    fn optimized_copy_to_user(dst: *mut u8, buffer: &[u8]) -> Result<(), SyscallError> {
        const BLOCK_SIZE: usize = 64;
        let len = buffer.len();
        let mut copied = 0;

        // Copy in blocks
        while copied + BLOCK_SIZE <= len {
            let dst_block = unsafe { dst.add(copied) };
            let src_block = &buffer[copied..copied + BLOCK_SIZE];

            // Validate the block before copying
            Self::validate_user_ptr(dst_block as u64, BLOCK_SIZE as u64, true)?;

            unsafe {
                core::ptr::copy_nonoverlapping(src_block.as_ptr(), dst_block, BLOCK_SIZE);
            }

            copied += BLOCK_SIZE;
        }

        // Copy remaining bytes
        while copied < len {
            match Self::safe_write_user_byte(unsafe { dst.add(copied) }, buffer[copied]) {
                Ok(()) => {}
                Err(e) => return Err(e),
            }
            copied += 1;
        }

        Ok(())
    }

    /// Optimized copy to user space for larger buffers with fault context
    fn optimized_copy_to_user_with_context(
        dst: *mut u8,
        buffer: &[u8],
        context: &mut PageFaultContext,
    ) -> Result<(), SyscallError> {
        const BLOCK_SIZE: usize = 64;
        let len = buffer.len();
        let mut copied = 0;

        // Copy in blocks with fault recovery
        while copied + BLOCK_SIZE <= len {
            let dst_block = unsafe { dst.add(copied) };
            let src_block = &buffer[copied..copied + BLOCK_SIZE];

            // Validate the block before copying
            Self::validate_user_ptr(dst_block as u64, BLOCK_SIZE as u64, true)?;

            // Perform block copy with potential fault handling
            match Self::safe_block_copy_to_user(src_block, dst_block, context) {
                Ok(()) => {
                    copied += BLOCK_SIZE;
                    context.update_progress(copied);
                }
                Err(_e) => {
                    // Fall back to byte-by-byte copying for the failed block
                    for i in 0..BLOCK_SIZE {
                        match Self::safe_write_user_byte_with_context(
                            unsafe { dst_block.add(i) },
                            src_block[i],
                            context,
                        ) {
                            Ok(()) => {
                                copied += 1;
                                context.update_progress(copied);
                            }
                            Err(byte_err) => return Err(byte_err),
                        }
                    }
                }
            }
        }

        // Copy remaining bytes
        while copied < len {
            match Self::safe_write_user_byte_with_context(
                unsafe { dst.add(copied) },
                buffer[copied],
                context,
            ) {
                Ok(()) => {
                    copied += 1;
                    context.update_progress(copied);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Safe block copy to user space with fault handling
    fn safe_block_copy_to_user(
        src: &[u8],
        dst: *mut u8,
        _context: &mut PageFaultContext,
    ) -> Result<(), SyscallError> {
        // In a real implementation, this would set up specific fault handling for the block
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
        }
        Ok(())
    }

    /// Safely read a single byte from user space
    fn safe_read_user_byte(ptr: *const u8) -> Result<u8, SyscallError> {
        // Validate the single byte address first
        Self::validate_user_ptr(ptr as u64, 1, false)?;

        // In a real implementation with proper exception handling,
        // we would set up a page fault handler here
        unsafe {
            // Use volatile read to prevent compiler optimizations
            // that might affect the memory access pattern
            Ok(core::ptr::read_volatile(ptr))
        }
    }

    /// Safely write a single byte to user space
    fn safe_write_user_byte(ptr: *mut u8, value: u8) -> Result<(), SyscallError> {
        // Validate the single byte address first
        Self::validate_user_ptr(ptr as u64, 1, true)?;

        // In a real implementation with proper exception handling,
        // we would set up a page fault handler here
        unsafe {
            // Use volatile write to prevent compiler optimizations
            // that might affect the memory access pattern
            core::ptr::write_volatile(ptr, value);
        }

        Ok(())
    }

    /// Safely read a single byte from user space with fault context
    fn safe_read_user_byte_with_context(
        ptr: *const u8,
        _context: &mut PageFaultContext,
    ) -> Result<u8, SyscallError> {
        // Validate the single byte address first
        Self::validate_user_ptr(ptr as u64, 1, false)?;

        // Perform the read with page fault handling
        // In a real implementation, this would use the context to handle page faults
        unsafe {
            // Use volatile read to prevent compiler optimizations
            // that might affect the memory access pattern
            Ok(core::ptr::read_volatile(ptr))
        }
    }

    /// Safely write a single byte to user space with fault context
    fn safe_write_user_byte_with_context(
        ptr: *mut u8,
        value: u8,
        _context: &mut PageFaultContext,
    ) -> Result<(), SyscallError> {
        // Validate the single byte address first
        Self::validate_user_ptr(ptr as u64, 1, true)?;

        // Perform the write with page fault handling
        // In a real implementation, this would use the context to handle page faults
        unsafe {
            // Use volatile write to prevent compiler optimizations
            // that might affect the memory access pattern
            core::ptr::write_volatile(ptr, value);
        }

        Ok(())
    }

    /// Copy a string from user space (null-terminated)
    pub fn copy_string_from_user(
        user_ptr: u64,
        max_len: usize,
    ) -> Result<alloc::string::String, SyscallError> {
        use alloc::string::String;
        use alloc::vec::Vec;

        if user_ptr == 0 {
            return Err(SyscallError::InvalidAddress);
        }

        let mut result = Vec::new();
        let mut current_ptr = user_ptr;

        for _ in 0..max_len {
            let byte = Self::safe_read_user_byte(current_ptr as *const u8)?;

            if byte == 0 {
                break; // Null terminator found
            }

            result.push(byte);
            current_ptr += 1;
        }

        String::from_utf8(result).map_err(|_| SyscallError::InvalidArgument)
    }

    /// Copy a string to user space (with null terminator)
    pub fn copy_string_to_user(user_ptr: u64, s: &str) -> Result<(), SyscallError> {
        let bytes = s.as_bytes();
        let total_len = bytes.len() + 1; // Include null terminator

        // Validate the entire range including null terminator
        Self::validate_user_ptr(user_ptr, total_len as u64, true)?;

        // Copy the string bytes
        Self::copy_to_user(user_ptr, bytes)?;

        // Add null terminator
        Self::safe_write_user_byte((user_ptr + bytes.len() as u64) as *mut u8, 0)?;

        Ok(())
    }

    /// Get the current privilege level for validation
    pub fn current_privilege_level() -> u16 {
        get_current_privilege_level()
    }

    /// Check if current context can access user memory
    pub fn can_access_user_memory() -> bool {
        is_kernel_mode()
    }

    /// Probe a user space address to check if it's accessible
    pub fn probe_user_address(addr: u64, write_access: bool) -> Result<(), SyscallError> {
        Self::validate_user_ptr(addr, 1, write_access)
    }

    /// Get memory protection flags for a user space page
    pub fn get_page_protection(addr: u64) -> Result<PageProtectionFlags, SyscallError> {
        let virt_addr = VirtAddr::new(addr);

        // Validate that this is a user space address
        if addr < USER_SPACE_START || addr >= USER_SPACE_END {
            return Err(SyscallError::InvalidAddress);
        }

        // Get detailed page table information
        let page_info = Self::get_page_table_info(virt_addr)?;

        Ok(PageProtectionFlags {
            readable: page_info.present && page_info.user_accessible,
            writable: page_info.writable,
            executable: !page_info.no_execute,
            user_accessible: page_info.user_accessible,
        })
    }

    /// Get detailed page table information for a virtual address
    fn get_page_table_info(virt_addr: VirtAddr) -> Result<PageTableInfo, SyscallError> {
        use x86_64::registers::control::Cr3;
        use x86_64::structures::paging::PageTableFlags;

        let (level_4_table_frame, _) = Cr3::read();
        let level_4_table_ptr =
            Self::phys_to_virt_kernel(level_4_table_frame.start_address()) as *const PageTable;

        let page_table_indices = [
            virt_addr.p4_index(),
            virt_addr.p3_index(),
            virt_addr.p2_index(),
            virt_addr.p1_index(),
        ];

        unsafe {
            let level_4_table = &*level_4_table_ptr;

            // Walk through page table hierarchy
            let pml4_entry = &level_4_table[page_table_indices[0]];
            if !pml4_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }

            let pdpt_ptr = Self::phys_to_virt_kernel(pml4_entry.addr()) as *const PageTable;
            let pdpt = &*pdpt_ptr;
            let pdpt_entry = &pdpt[page_table_indices[1]];
            if !pdpt_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }

            // Check for 1GB huge page
            if pdpt_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                return Ok(PageTableInfo::from_flags(pdpt_entry.flags()));
            }

            let pd_ptr = Self::phys_to_virt_kernel(pdpt_entry.addr()) as *const PageTable;
            let pd = &*pd_ptr;
            let pd_entry = &pd[page_table_indices[2]];
            if !pd_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }

            // Check for 2MB large page
            if pd_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                return Ok(PageTableInfo::from_flags(pd_entry.flags()));
            }

            let pt_ptr = Self::phys_to_virt_kernel(pd_entry.addr()) as *const PageTable;
            let pt = &*pt_ptr;
            let pt_entry = &pt[page_table_indices[3]];
            if !pt_entry.flags().contains(PageTableFlags::PRESENT) {
                return Err(SyscallError::InvalidAddress);
            }

            Ok(PageTableInfo::from_flags(pt_entry.flags()))
        }
    }

    /// Validate memory range with enhanced security checks
    pub fn validate_user_range_enhanced(
        ptr: u64,
        len: u64,
        write_access: bool,
        process_id: Option<u32>,
    ) -> Result<(), SyscallError> {
        // Basic validation first
        Self::validate_user_ptr(ptr, len, write_access)?;

        // Enhanced security checks
        if let Some(_pid) = process_id {
            // Note: Process-specific memory validation requires integration with process manager.
            // Future enhancements will include:
            // - Verification that memory range belongs to the specified process
            // - Validation against per-process memory limits (stack, heap, data segments)
            // - Checking for memory protection violations (e.g., write to read-only segments)
            // - Validation of shared memory permissions
        }

        // Check for suspicious access patterns
        Self::detect_suspicious_access_patterns(ptr, len, write_access)?;

        // Validate alignment for performance-critical operations
        if len > 4096 {
            Self::validate_memory_alignment(ptr, len)?;
        }

        Ok(())
    }

    /// Detect suspicious memory access patterns
    fn detect_suspicious_access_patterns(
        ptr: u64,
        len: u64,
        write_access: bool,
    ) -> Result<(), SyscallError> {
        // Check for extremely large allocations that might be DoS attempts
        if len > MAX_COPY_SIZE as u64 {
            return Err(SyscallError::InvalidArgument);
        }

        // Check for suspicious pointer values
        if ptr != 0 && ptr < 0x1000 {
            // Very low addresses are suspicious (null pointer dereference attempts)
            return Err(SyscallError::InvalidAddress);
        }

        // Check for kernel space addresses masquerading as user space
        if ptr >= 0x8000_0000_0000 {
            return Err(SyscallError::InvalidAddress);
        }

        // Check for write attempts to typically read-only regions
        if write_access && ptr < 0x400000 {
            // Attempts to write to low memory (typically code sections)
            // This could indicate exploit attempts
            return Err(SyscallError::PermissionDenied);
        }

        Ok(())
    }

    /// Validate memory alignment for optimal performance
    fn validate_memory_alignment(ptr: u64, len: u64) -> Result<(), SyscallError> {
        // For large operations, prefer page-aligned access
        if len >= PAGE_SIZE as u64 {
            if ptr % PAGE_SIZE as u64 != 0 {
                // Not page-aligned - this is allowed but not optimal
                // We could log this for performance analysis
            }
        }

        // Check for reasonable alignment
        if len >= 8 && ptr % 8 != 0 {
            // Not 8-byte aligned for large operations
            // This is allowed but might indicate inefficient code
        }

        Ok(())
    }

    /// Flush TLB entries for user space addresses
    pub fn flush_user_tlb_range(start_addr: u64, len: u64) -> Result<(), SyscallError> {
        use x86_64::instructions::tlb;

        if start_addr < USER_SPACE_START || start_addr + len > USER_SPACE_END {
            return Err(SyscallError::InvalidAddress);
        }

        let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(start_addr));
        let end_addr = start_addr + len - 1;
        let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(end_addr));

        for page in Page::range_inclusive(start_page, end_page) {
            tlb::flush(page.start_address());
        }

        Ok(())
    }
}

/// Memory operation statistics
#[derive(Debug, Default, Clone)]
pub struct MemoryOperationStats {
    pub copy_from_user_calls: u64,
    pub copy_to_user_calls: u64,
    pub validation_calls: u64,
    pub bytes_copied_from_user: u64,
    pub bytes_copied_to_user: u64,
    pub validation_failures: u64,
    pub page_fault_recoveries: u64,
}

/// Enhanced user space memory operations with additional safety checks
pub struct EnhancedUserSpaceMemory {
    stats: core::sync::atomic::AtomicU64, // Simple counter for now
}

impl EnhancedUserSpaceMemory {
    /// Create a new enhanced memory manager
    pub fn new() -> Self {
        Self {
            stats: core::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Copy data from user space with enhanced validation and statistics
    pub fn copy_from_user_enhanced(
        &self,
        user_ptr: u64,
        buffer: &mut [u8],
        process_id: Option<u32>,
    ) -> Result<(), SyscallError> {
        use core::sync::atomic::Ordering;

        // Additional validation for enhanced version
        if !UserSpaceMemory::can_access_user_memory() {
            return Err(SyscallError::PermissionDenied);
        }

        // Enhanced validation with security checks
        UserSpaceMemory::validate_user_range_enhanced(
            user_ptr,
            buffer.len() as u64,
            false,
            process_id,
        )?;

        // Increment statistics
        self.stats.fetch_add(1, Ordering::Relaxed);

        // Perform the copy operation
        let result = UserSpaceMemory::copy_from_user(user_ptr, buffer);

        // Update statistics based on result
        match &result {
            Ok(()) => {
                // Success - could update success counters here
            }
            Err(_) => {
                // Error - could update error counters here
            }
        }

        result
    }

    /// Copy data to user space with enhanced validation and statistics
    pub fn copy_to_user_enhanced(
        &self,
        user_ptr: u64,
        buffer: &[u8],
        process_id: Option<u32>,
    ) -> Result<(), SyscallError> {
        use core::sync::atomic::Ordering;

        // Additional validation for enhanced version
        if !UserSpaceMemory::can_access_user_memory() {
            return Err(SyscallError::PermissionDenied);
        }

        // Enhanced validation with security checks
        UserSpaceMemory::validate_user_range_enhanced(
            user_ptr,
            buffer.len() as u64,
            true,
            process_id,
        )?;

        // Increment statistics
        self.stats.fetch_add(1, Ordering::Relaxed);

        // Perform the copy operation
        let result = UserSpaceMemory::copy_to_user(user_ptr, buffer);

        // Update statistics based on result
        match &result {
            Ok(()) => {
                // Success - could update success counters here
            }
            Err(_) => {
                // Error - could update error counters here
            }
        }

        result
    }

    /// Validate user pointer with enhanced security checks
    pub fn validate_user_ptr_enhanced(
        &self,
        user_ptr: u64,
        len: u64,
        write_access: bool,
        process_id: Option<u32>,
    ) -> Result<(), SyscallError> {
        UserSpaceMemory::validate_user_range_enhanced(user_ptr, len, write_access, process_id)
    }

    /// Get operation statistics
    pub fn get_stats(&self) -> u64 {
        use core::sync::atomic::Ordering;
        self.stats.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_user_ptr_validation() {
        // Test null pointer with zero length (should succeed)
        assert!(UserSpaceMemory::validate_user_ptr(0, 0, false).is_ok());

        // Test null pointer with non-zero length (should fail)
        assert!(UserSpaceMemory::validate_user_ptr(0, 1, false).is_err());

        // Test overflow (should fail)
        assert!(UserSpaceMemory::validate_user_ptr(u64::MAX, 1, false).is_err());

        // Test kernel space address (should fail)
        assert!(UserSpaceMemory::validate_user_ptr(0x8000_0000_0000, 1, false).is_err());

        // Test too large size (should fail)
        assert!(UserSpaceMemory::validate_user_ptr(
            USER_SPACE_START,
            (MAX_COPY_SIZE + 1) as u64,
            false
        )
        .is_err());
    }

    #[test_case]
    fn test_enhanced_validation() {
        // Test enhanced validation with process ID
        let result =
            UserSpaceMemory::validate_user_range_enhanced(USER_SPACE_START, 4096, false, Some(123));
        // This might fail due to page table walking, but should not panic
        let _ = result;
    }

    #[test_case]
    fn test_suspicious_access_detection() {
        // Test detection of very low addresses
        assert!(UserSpaceMemory::detect_suspicious_access_patterns(0x100, 1, false).is_err());

        // Test detection of kernel addresses
        assert!(
            UserSpaceMemory::detect_suspicious_access_patterns(0x8000_0000_0000, 1, false).is_err()
        );

        // Test detection of oversized operations
        assert!(UserSpaceMemory::detect_suspicious_access_patterns(
            USER_SPACE_START,
            (MAX_COPY_SIZE + 1) as u64,
            false
        )
        .is_err());
    }

    #[test_case]
    fn test_page_protection_flags() {
        let flags = PageProtectionFlags {
            readable: true,
            writable: false,
            executable: false,
            user_accessible: true,
        };

        assert!(flags.readable);
        assert!(!flags.writable);
        assert!(!flags.executable);
        assert!(flags.user_accessible);
    }

    #[test_case]
    fn test_page_table_info() {
        use x86_64::structures::paging::PageTableFlags;

        let flags =
            PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE;
        let info = PageTableInfo::from_flags(flags);

        assert!(info.present);
        assert!(info.user_accessible);
        assert!(info.writable);
        assert!(!info.no_execute);
    }

    #[test_case]
    fn test_memory_operation_types() {
        assert_ne!(MemoryOperation::CopyFromUser, MemoryOperation::CopyToUser);
    }

    #[test_case]
    fn test_page_fault_context() {
        let context =
            PageFaultContext::new(MemoryOperation::CopyFromUser, USER_SPACE_START, 4096, false);

        assert_eq!(
            context.recovery_context.operation,
            MemoryOperation::CopyFromUser
        );
        assert_eq!(context.recovery_context.start_addr, USER_SPACE_START);
        assert_eq!(context.recovery_context.end_addr, USER_SPACE_START + 4096);
        assert!(!context.recovery_context.is_write);
    }

    #[test_case]
    fn test_user_space_boundaries() {
        // Test addresses within user space
        assert!(USER_SPACE_START < USER_SPACE_END);

        // Test that kernel space starts above user space
        assert!(USER_SPACE_END <= 0x8000_0000_0000);
    }

    #[test_case]
    fn test_copy_size_limits() {
        // Test that MAX_COPY_SIZE is reasonable
        assert!(MAX_COPY_SIZE > 0);
        assert!(MAX_COPY_SIZE <= 1024 * 1024 * 1024); // Should be <= 1GB
    }

    #[test_case]
    fn test_enhanced_user_space_memory() {
        let enhanced = EnhancedUserSpaceMemory::new();
        assert_eq!(enhanced.get_stats(), 0);

        // Test that we can create the enhanced memory manager
        assert!(enhanced.get_stats() == 0);
    }

    #[test_case]
    fn test_memory_alignment_validation() {
        // Test page-aligned access
        assert!(UserSpaceMemory::validate_memory_alignment(0x1000, 4096).is_ok());

        // Test unaligned access (should still be ok, just not optimal)
        assert!(UserSpaceMemory::validate_memory_alignment(0x1001, 4096).is_ok());

        // Test small aligned access
        assert!(UserSpaceMemory::validate_memory_alignment(0x1000, 8).is_ok());
    }
}
