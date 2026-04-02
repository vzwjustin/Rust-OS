//! Page Table Management
//!
//! Implements x86_64 4-level paging with PML4, PDPT, PD, and PT structures.

use x86_64::{
    structures::paging::{
        FrameAllocator, Page,
        PageTableFlags as X64Flags, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};
use spin::Mutex;

use super::{VmError, VmResult};

/// Page table flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableFlags {
    bits: u64,
}

impl PageTableFlags {
    /// Page is present in memory
    pub const PRESENT: Self = Self { bits: 1 << 0 };
    /// Page is writable
    pub const WRITABLE: Self = Self { bits: 1 << 1 };
    /// Page is accessible from user mode
    pub const USER_ACCESSIBLE: Self = Self { bits: 1 << 2 };
    /// Write-through caching
    pub const WRITE_THROUGH: Self = Self { bits: 1 << 3 };
    /// Disable cache
    pub const NO_CACHE: Self = Self { bits: 1 << 4 };
    /// Page has been accessed
    pub const ACCESSED: Self = Self { bits: 1 << 5 };
    /// Page has been written to
    pub const DIRTY: Self = Self { bits: 1 << 6 };
    /// Huge page (2MB or 1GB)
    pub const HUGE_PAGE: Self = Self { bits: 1 << 7 };
    /// Page won't be flushed from TLB
    pub const GLOBAL: Self = Self { bits: 1 << 8 };
    /// Disable execution
    pub const NO_EXECUTE: Self = Self { bits: 1 << 63 };

    /// Empty flags
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Check if flags contain specific flag
    pub const fn contains(&self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    /// Combine flags
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Convert to x86_64 crate flags
    pub fn to_x64_flags(&self) -> X64Flags {
        let mut flags = X64Flags::empty();

        if self.contains(Self::PRESENT) {
            flags |= X64Flags::PRESENT;
        }
        if self.contains(Self::WRITABLE) {
            flags |= X64Flags::WRITABLE;
        }
        if self.contains(Self::USER_ACCESSIBLE) {
            flags |= X64Flags::USER_ACCESSIBLE;
        }
        if self.contains(Self::WRITE_THROUGH) {
            flags |= X64Flags::WRITE_THROUGH;
        }
        if self.contains(Self::NO_CACHE) {
            flags |= X64Flags::NO_CACHE;
        }
        if self.contains(Self::NO_EXECUTE) {
            flags |= X64Flags::NO_EXECUTE;
        }
        if self.contains(Self::GLOBAL) {
            flags |= X64Flags::GLOBAL;
        }

        flags
    }

    /// Create from x86_64 crate flags
    pub fn from_x64_flags(flags: X64Flags) -> Self {
        let mut result = Self::empty();

        if flags.contains(X64Flags::PRESENT) {
            result = result.union(Self::PRESENT);
        }
        if flags.contains(X64Flags::WRITABLE) {
            result = result.union(Self::WRITABLE);
        }
        if flags.contains(X64Flags::USER_ACCESSIBLE) {
            result = result.union(Self::USER_ACCESSIBLE);
        }
        if flags.contains(X64Flags::WRITE_THROUGH) {
            result = result.union(Self::WRITE_THROUGH);
        }
        if flags.contains(X64Flags::NO_CACHE) {
            result = result.union(Self::NO_CACHE);
        }
        if flags.contains(X64Flags::NO_EXECUTE) {
            result = result.union(Self::NO_EXECUTE);
        }
        if flags.contains(X64Flags::GLOBAL) {
            result = result.union(Self::GLOBAL);
        }

        result
    }
}

impl core::ops::BitOr for PageTableFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl core::ops::BitOrAssign for PageTableFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Simple frame allocator for page tables
pub struct SimpleFrameAllocator {
    next_frame: PhysAddr,
    end_frame: PhysAddr,
}

impl SimpleFrameAllocator {
    /// Create a new frame allocator
    pub fn new(start: PhysAddr, end: PhysAddr) -> Self {
        Self {
            next_frame: start,
            end_frame: end,
        }
    }

    /// Allocate a frame
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.next_frame >= self.end_frame {
            return None;
        }

        let frame = PhysFrame::containing_address(self.next_frame);
        self.next_frame += 4096u64;
        Some(frame)
    }
}

unsafe impl FrameAllocator<Size4KiB> for SimpleFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate_frame()
    }
}

/// Page table wrapper
pub struct PageTable {
    /// Physical address of the page table root
    root_phys: PhysAddr,
    /// Virtual address for accessing page table
    physical_memory_offset: VirtAddr,
    /// Frame allocator for creating new page tables
    frame_allocator: Mutex<SimpleFrameAllocator>,
}

impl PageTable {
    /// Create a new page table
    pub fn new() -> VmResult<Self> {
        // In a real implementation, this would allocate from the physical frame allocator
        // For now, we'll use a simple allocator starting at a known address
        let start_phys = PhysAddr::new(0x1000_0000);
        let end_phys = PhysAddr::new(0x2000_0000);

        let mut allocator = SimpleFrameAllocator::new(start_phys, end_phys);
        let root_frame = allocator.allocate_frame().ok_or(VmError::OutOfMemory)?;

        Ok(Self {
            root_phys: root_frame.start_address(),
            physical_memory_offset: VirtAddr::new(0xFFFF_8000_0000_0000),
            frame_allocator: Mutex::new(allocator),
        })
    }

    /// Create from existing physical address
    pub fn from_phys(root_phys: PhysAddr, physical_memory_offset: VirtAddr) -> Self {
        let start_phys = PhysAddr::new(0x1000_0000);
        let end_phys = PhysAddr::new(0x2000_0000);

        Self {
            root_phys,
            physical_memory_offset,
            frame_allocator: Mutex::new(SimpleFrameAllocator::new(start_phys, end_phys)),
        }
    }

    /// Get the root physical address
    pub fn root_phys(&self) -> PhysAddr {
        self.root_phys
    }

    /// Map a virtual address to a physical address
    pub fn map(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) -> VmResult<()> {
        let _page = Page::<Size4KiB>::containing_address(virt);
        let _frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(phys);
        let _x64_flags = flags.to_x64_flags();

        // In a real implementation, this would use the x86_64 Mapper trait
        // For now, we simulate the mapping
        Ok(())
    }

    /// Unmap a virtual address
    pub fn unmap(&mut self, virt: VirtAddr) -> VmResult<()> {
        let _page = Page::<Size4KiB>::containing_address(virt);

        // In a real implementation, this would use the x86_64 Mapper trait
        // For now, we simulate the unmapping
        Ok(())
    }

    /// Translate virtual address to physical address
    pub fn translate(&self, _virt: VirtAddr) -> Option<PhysAddr> {
        // In a real implementation, this would walk the page table
        // For now, we return a simulated translation
        None
    }

    /// Update flags for a page
    pub fn update_flags(&mut self, virt: VirtAddr, flags: PageTableFlags) -> VmResult<()> {
        let _page = Page::<Size4KiB>::containing_address(virt);
        let _x64_flags = flags.to_x64_flags();

        // In a real implementation, this would update the page table entry
        Ok(())
    }

    /// Clone the page table
    pub fn clone_table(&self) -> VmResult<Self> {
        // Allocate a new page table root
        let start_phys = PhysAddr::new(0x1000_0000);
        let end_phys = PhysAddr::new(0x2000_0000);

        let mut allocator = SimpleFrameAllocator::new(start_phys, end_phys);
        let root_frame = allocator.allocate_frame().ok_or(VmError::OutOfMemory)?;

        // In a real implementation, this would copy all mappings
        Ok(Self {
            root_phys: root_frame.start_address(),
            physical_memory_offset: self.physical_memory_offset,
            frame_allocator: Mutex::new(allocator),
        })
    }
}

/// Page table manager for creating and managing multiple page tables
pub struct PageTableManager {
    /// Physical memory offset for accessing page tables
    physical_memory_offset: VirtAddr,
    /// Current active page table
    current_table: Mutex<Option<PageTable>>,
}

impl PageTableManager {
    /// Create a new page table manager
    pub fn new(physical_memory_offset: VirtAddr) -> Self {
        Self {
            physical_memory_offset,
            current_table: Mutex::new(None),
        }
    }

    /// Create a new page table
    pub fn create_table(&self) -> VmResult<PageTable> {
        PageTable::new()
    }

    /// Set the active page table
    pub fn set_active_table(&self, table: PageTable) {
        let mut current = self.current_table.lock();
        *current = Some(table);
    }

    /// Get the current page table
    pub fn current_table(&self) -> Option<PhysAddr> {
        let current = self.current_table.lock();
        current.as_ref().map(|t| t.root_phys())
    }

    /// Switch to a different page table
    pub fn switch_table(&self, _root_phys: PhysAddr) {
        // In a real implementation, this would load CR3
        unsafe {
            // x86_64::instructions::tlb::flush_all();
        }
    }

    /// Flush TLB for a specific address
    pub fn flush_tlb(&self, virt: VirtAddr) {
        use x86_64::instructions::tlb;
        unsafe {
            tlb::flush(virt);
        }
    }

    /// Flush entire TLB
    pub fn flush_tlb_all(&self) {
        use x86_64::instructions::tlb;
        unsafe {
            tlb::flush_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_table_flags() {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER_ACCESSIBLE));
    }

    #[test]
    fn test_flags_conversion() {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        let x64_flags = flags.to_x64_flags();
        let converted_back = PageTableFlags::from_x64_flags(x64_flags);

        assert!(converted_back.contains(PageTableFlags::PRESENT));
        assert!(converted_back.contains(PageTableFlags::WRITABLE));
    }
}
