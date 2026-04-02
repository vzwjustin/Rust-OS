//! Virtual Memory Manager
//!
//! Implements mmap, munmap, mprotect, brk, and sbrk for virtual memory management.

use x86_64::{PhysAddr, VirtAddr};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::memory_region::{MemoryRegion, MemoryType, ProtectionFlags};
use super::page_table::{PageTableFlags, PageTableManager};
use super::{MmapFlags, MemoryStats};

/// Virtual memory error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    /// Invalid address
    InvalidAddress,
    /// Invalid size
    InvalidSize,
    /// Out of memory
    OutOfMemory,
    /// Permission denied
    PermissionDenied,
    /// Region not found
    RegionNotFound,
    /// Region already mapped
    AlreadyMapped,
    /// Invalid flags
    InvalidFlags,
    /// Address not aligned
    NotAligned,
    /// Not initialized
    NotInitialized,
    /// Already initialized
    AlreadyInitialized,
    /// Invalid operation
    InvalidOperation,
}

/// Result type for virtual memory operations
pub type VmResult<T> = Result<T, VmError>;

/// Virtual memory manager
pub struct VirtualMemoryManager {
    /// Memory regions indexed by start address
    regions: BTreeMap<u64, MemoryRegion>,
    /// Page table manager
    page_table_manager: PageTableManager,
    /// Current program break (for brk/sbrk)
    program_break: VirtAddr,
    /// Initial program break
    initial_break: VirtAddr,
    /// Next free address for mmap allocations
    next_mmap_addr: VirtAddr,
    /// Physical memory offset for page table access
    physical_memory_offset: VirtAddr,
    /// Statistics
    stats: MemoryStats,
}

impl VirtualMemoryManager {
    /// User space memory layout constants
    const USER_HEAP_START: u64 = 0x0000_1000_0000;
    const USER_HEAP_END: u64 = 0x0000_4000_0000; // 1GB heap space
    const MMAP_START: u64 = 0x0000_4000_0000;
    const MMAP_END: u64 = 0x0000_8000_0000; // 1GB mmap space
    const STACK_TOP: u64 = 0x0000_8000_0000;

    /// Create a new virtual memory manager
    pub fn new(physical_memory_offset: VirtAddr) -> Self {
        let initial_break = VirtAddr::new(Self::USER_HEAP_START);

        Self {
            regions: BTreeMap::new(),
            page_table_manager: PageTableManager::new(physical_memory_offset),
            program_break: initial_break,
            initial_break,
            next_mmap_addr: VirtAddr::new(Self::MMAP_START),
            physical_memory_offset,
            stats: MemoryStats::default(),
        }
    }

    /// Map virtual memory region (mmap)
    pub fn mmap(
        &self,
        addr: usize,
        length: usize,
        prot: ProtectionFlags,
        flags: MmapFlags,
    ) -> VmResult<*mut u8> {
        // Validate parameters
        if length == 0 {
            return Err(VmError::InvalidSize);
        }

        // Align length to page boundary
        let aligned_length = (length + 4095) & !4095;

        // Determine start address
        let start_addr = if flags.fixed {
            if addr == 0 {
                return Err(VmError::InvalidAddress);
            }
            if addr % 4096 != 0 {
                return Err(VmError::NotAligned);
            }
            VirtAddr::new(addr as u64)
        } else if addr != 0 {
            // Hint address
            VirtAddr::new(addr as u64)
        } else {
            // Kernel chooses address
            self.next_mmap_addr
        };

        let end_addr = start_addr + aligned_length as u64;

        // Check for overlaps if fixed mapping
        if flags.fixed {
            // In fixed mode, unmap any existing mappings (Linux behavior)
            // For simplicity, we'll just check and fail for now
            if self.find_region_at(start_addr).is_some() {
                return Err(VmError::AlreadyMapped);
            }
        }

        // Create memory region
        let memory_type = if flags.anonymous {
            MemoryType::Anonymous
        } else if flags.shared {
            MemoryType::Shared
        } else {
            MemoryType::FileBacked
        };

        let mut region = MemoryRegion::new(start_addr, end_addr, prot, memory_type);
        region.shared = flags.shared;

        // Allocate physical frames and map pages
        // In a real implementation, this would allocate from the physical frame allocator
        self.map_region(&region)?;

        Ok(start_addr.as_mut_ptr())
    }

    /// Unmap virtual memory region (munmap)
    pub fn munmap(&self, addr: usize, length: usize) -> VmResult<()> {
        if length == 0 {
            return Err(VmError::InvalidSize);
        }

        if addr % 4096 != 0 {
            return Err(VmError::NotAligned);
        }

        let start_addr = VirtAddr::new(addr as u64);
        let aligned_length = (length + 4095) & !4095;
        let end_addr = start_addr + aligned_length as u64;

        // Find and unmap all regions in range
        self.unmap_range(start_addr, end_addr)?;

        Ok(())
    }

    /// Change memory protection (mprotect)
    pub fn mprotect(&self, addr: usize, length: usize, prot: ProtectionFlags) -> VmResult<()> {
        if length == 0 {
            return Err(VmError::InvalidSize);
        }

        if addr % 4096 != 0 {
            return Err(VmError::NotAligned);
        }

        let start_addr = VirtAddr::new(addr as u64);
        let aligned_length = (length + 4095) & !4095;
        let end_addr = start_addr + aligned_length as u64;

        // Find regions in range and update protection
        self.protect_range(start_addr, end_addr, prot)?;

        Ok(())
    }

    /// Change program break (brk)
    pub fn brk(&self, addr: usize) -> VmResult<usize> {
        // Query current break
        if addr == 0 {
            return Ok(self.program_break.as_u64() as usize);
        }

        let new_break = VirtAddr::new(addr as u64);

        // Validate new break
        if new_break < self.initial_break {
            return Err(VmError::InvalidAddress);
        }

        if new_break.as_u64() > Self::USER_HEAP_END {
            return Err(VmError::OutOfMemory);
        }

        // Expanding heap
        if new_break > self.program_break {
            let start = self.program_break;
            let end = new_break;
            let aligned_end = VirtAddr::new((end.as_u64() + 4095) & !4095);

            // Map new pages
            let region = MemoryRegion::new(
                start,
                aligned_end,
                ProtectionFlags::READ_WRITE,
                MemoryType::Heap,
            );
            self.map_region(&region)?;
        }
        // Shrinking heap
        else if new_break < self.program_break {
            let start = new_break;
            let end = self.program_break;

            // Unmap pages
            self.unmap_range(start, end)?;
        }

        Ok(new_break.as_u64() as usize)
    }

    /// Allocate program break (sbrk)
    pub fn sbrk(&self, increment: isize) -> VmResult<usize> {
        let current_break = self.program_break.as_u64() as usize;

        if increment == 0 {
            return Ok(current_break);
        }

        let new_break = if increment > 0 {
            current_break
                .checked_add(increment as usize)
                .ok_or(VmError::OutOfMemory)?
        } else {
            current_break
                .checked_sub((-increment) as usize)
                .ok_or(VmError::InvalidAddress)?
        };

        self.brk(new_break)?;
        Ok(current_break)
    }

    /// Find region containing address
    fn find_region_at(&self, addr: VirtAddr) -> Option<&MemoryRegion> {
        for region in self.regions.values() {
            if region.contains(addr) {
                return Some(region);
            }
        }
        None
    }

    /// Map a memory region to physical frames
    fn map_region(&self, region: &MemoryRegion) -> VmResult<()> {
        let page_count = region.size_in_pages();

        // Convert protection flags to page table flags
        let mut pt_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        if region.protection.is_writable() {
            pt_flags = pt_flags | PageTableFlags::WRITABLE;
        }

        if !region.protection.is_executable() {
            pt_flags = pt_flags | PageTableFlags::NO_EXECUTE;
        }

        // Map each page
        let mut virt_addr = region.start;
        for _ in 0..page_count {
            // In a real implementation, allocate a physical frame here
            let _phys_addr = PhysAddr::new(0x1000_0000); // Placeholder

            // Map the page (would use actual page table here)
            // page_table.map(virt_addr, phys_addr, pt_flags)?;

            virt_addr += 4096u64;
        }

        Ok(())
    }

    /// Unmap a range of addresses
    fn unmap_range(&self, start: VirtAddr, end: VirtAddr) -> VmResult<()> {
        let page_count = ((end.as_u64() - start.as_u64()) / 4096) as usize;

        let mut virt_addr = start;
        for _ in 0..page_count {
            // Unmap the page (would use actual page table here)
            // page_table.unmap(virt_addr)?;

            virt_addr += 4096u64;
        }

        Ok(())
    }

    /// Change protection for a range of addresses
    fn protect_range(
        &self,
        start: VirtAddr,
        end: VirtAddr,
        prot: ProtectionFlags,
    ) -> VmResult<()> {
        let page_count = ((end.as_u64() - start.as_u64()) / 4096) as usize;

        // Convert protection flags to page table flags
        let mut pt_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        if prot.is_writable() {
            pt_flags = pt_flags | PageTableFlags::WRITABLE;
        }

        if !prot.is_executable() {
            pt_flags = pt_flags | PageTableFlags::NO_EXECUTE;
        }

        let mut virt_addr = start;
        for _ in 0..page_count {
            // Update page flags (would use actual page table here)
            // page_table.update_flags(virt_addr, pt_flags)?;

            virt_addr += 4096u64;
        }

        // Flush TLB for the range
        self.page_table_manager.flush_tlb(start);

        Ok(())
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        let total_allocated: usize = self.regions.values().map(|r| r.size()).sum();
        let region_count = self.regions.len();
        let mapped_pages: usize = self.regions.values().map(|r| r.size_in_pages()).sum();

        MemoryStats {
            total_allocated,
            region_count,
            current_brk: self.program_break.as_u64() as usize,
            mapped_pages,
        }
    }

    /// Allocate a stack for a new thread
    pub fn allocate_stack(&self, size: usize) -> VmResult<VirtAddr> {
        let aligned_size = (size + 4095) & !4095;

        let flags = MmapFlags::anonymous_private();
        let prot = ProtectionFlags::READ_WRITE;

        let stack_addr = self.mmap(0, aligned_size, prot, flags)?;
        Ok(VirtAddr::new(stack_addr as u64))
    }

    /// Create a new address space (for fork)
    pub fn clone_address_space(&self) -> VmResult<Self> {
        let mut new_vm = Self::new(self.physical_memory_offset);

        // Copy all regions
        for region in self.regions.values() {
            let new_region = region.clone();
            new_vm.regions.insert(new_region.start.as_u64(), new_region);
        }

        // Copy program break
        new_vm.program_break = self.program_break;
        new_vm.initial_break = self.initial_break;

        Ok(new_vm)
    }

    /// Handle page fault
    pub fn handle_page_fault(&self, addr: VirtAddr, error_code: u64) -> VmResult<()> {
        // Find region containing faulting address
        let region = self.find_region_at(addr).ok_or(VmError::InvalidAddress)?;

        // Check if fault is due to copy-on-write
        if region.copy_on_write && (error_code & 0x2) != 0 {
            // Write to COW page - allocate new physical frame and copy
            return self.handle_cow_fault(addr, region);
        }

        // Check if fault is due to missing permission
        let needs_write = (error_code & 0x2) != 0;
        let needs_exec = (error_code & 0x10) != 0;

        if needs_write && !region.protection.is_writable() {
            return Err(VmError::PermissionDenied);
        }

        if needs_exec && !region.protection.is_executable() {
            return Err(VmError::PermissionDenied);
        }

        // Other page fault handling...
        Ok(())
    }

    /// Handle copy-on-write page fault
    fn handle_cow_fault(&self, _addr: VirtAddr, _region: &MemoryRegion) -> VmResult<()> {
        // Allocate new physical frame
        // Copy old page contents to new frame
        // Update page table entry to point to new frame and mark writable
        // Remove COW flag

        Ok(())
    }

    /// Get page table manager
    pub fn page_table_manager(&self) -> &PageTableManager {
        &self.page_table_manager
    }

    /// Dump memory map (for debugging)
    pub fn dump_memory_map(&self) -> Vec<(VirtAddr, VirtAddr, ProtectionFlags, MemoryType)> {
        self.regions
            .values()
            .map(|r| (r.start, r.end, r.protection, r.memory_type))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_creation() {
        let vm = VirtualMemoryManager::new(VirtAddr::new(0xFFFF_8000_0000_0000));
        let stats = vm.stats();

        assert_eq!(stats.region_count, 0);
        assert_eq!(stats.total_allocated, 0);
    }

    #[test]
    fn test_protection_flags_conversion() {
        let prot = ProtectionFlags::READ_WRITE;
        assert!(prot.is_readable());
        assert!(prot.is_writable());
        assert!(!prot.is_executable());
    }
}
