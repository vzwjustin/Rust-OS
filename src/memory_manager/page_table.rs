//! Page Table Management
//!
//! Implements x86_64 4-level paging with PML4, PDPT, PD, and PT structures.

use spin::Mutex;
use x86_64::{
    structures::paging::{
        FrameAllocator, Page, PageTableFlags as X64Flags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};

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
        let start_phys = PhysAddr::new(0x1000_0000);
        let end_phys = PhysAddr::new(0x2000_0000);

        let mut allocator = SimpleFrameAllocator::new(start_phys, end_phys);
        let root_frame = allocator.allocate_frame().ok_or(VmError::OutOfMemory)?;

        // Zero the new P4 table so unused entries are not-present
        let p4_virt = VirtAddr::new(0xFFFF_8000_0000_0000) + root_frame.start_address().as_u64();
        unsafe {
            core::ptr::write_bytes(p4_virt.as_u64() as *mut u8, 0, 4096);
        }

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

    /// Create an OffsetPageTable reference for this page table.
    ///
    /// # Safety
    /// The caller must ensure that the physical_memory_offset correctly
    /// maps all physical memory and that no other code is concurrently
    /// modifying the page table.
    unsafe fn offset_page_table(&self) -> x86_64::structures::paging::OffsetPageTable<'_> {
        let p4_virt = self.physical_memory_offset + self.root_phys.as_u64();
        x86_64::structures::paging::OffsetPageTable::new(
            &mut *(p4_virt.as_u64() as *mut x86_64::structures::paging::PageTable),
            self.physical_memory_offset,
        )
    }

    /// Map a virtual address to a physical address
    pub fn map(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) -> VmResult<()> {
        use x86_64::structures::paging::Mapper;

        let page = Page::<Size4KiB>::containing_address(virt);
        let frame = PhysFrame::<Size4KiB>::containing_address(phys);
        let x64_flags = flags.to_x64_flags();

        let mut allocator = self.frame_allocator.lock();
        // SAFETY: offset_page_table requires that physical_memory_offset
        // maps all physical memory, which is set up by the bootloader.
        unsafe {
            let mut pt = self.offset_page_table();
            pt.map_to(page, frame, x64_flags, &mut *allocator)
                .map_err(|_| VmError::InvalidOperation)?;
        }
        Ok(())
    }

    /// Unmap a virtual address
    pub fn unmap(&mut self, virt: VirtAddr) -> VmResult<()> {
        use x86_64::structures::paging::Mapper;

        let page = Page::<Size4KiB>::containing_address(virt);

        unsafe {
            let mut pt = self.offset_page_table();
            pt.unmap(page).map_err(|_| VmError::InvalidOperation)?;
        }
        Ok(())
    }

    /// Translate virtual address to physical address
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        use x86_64::structures::paging::Translate;
        unsafe {
            let pt = self.offset_page_table();
            pt.translate_addr(virt)
        }
    }

    /// Update flags for a page
    pub fn update_flags(&mut self, virt: VirtAddr, flags: PageTableFlags) -> VmResult<()> {
        use x86_64::structures::paging::{Mapper, PageTableFlags as X64F};

        let page = Page::<Size4KiB>::containing_address(virt);
        let x64_flags = flags.to_x64_flags();

        unsafe {
            let mut pt = self.offset_page_table();
            // SAFETY: updating flags on an existing mapping. The frame
            // remains the same; only the permission bits change.
            pt.update_flags(page, x64_flags)
                .map_err(|_| VmError::InvalidOperation)?;
        }

        // Suppress unused warning for X64F import when not needed directly
        let _ = X64F::empty();
        Ok(())
    }

    /// Clone the page table — copies all present mappings from the source
    /// P4 table into a newly allocated P4 table.
    pub fn clone_table(&self) -> VmResult<Self> {
        let start_phys = PhysAddr::new(0x1000_0000);
        let end_phys = PhysAddr::new(0x2000_0000);

        let mut allocator = SimpleFrameAllocator::new(start_phys, end_phys);
        let root_frame = allocator.allocate_frame().ok_or(VmError::OutOfMemory)?;

        let new_root_phys = root_frame.start_address();

        // Zero the new P4 table
        let new_p4_virt = self.physical_memory_offset + new_root_phys.as_u64();
        unsafe {
            core::ptr::write_bytes(new_p4_virt.as_u64() as *mut u8, 0, 4096);
        }

        // Copy all 512 P4 entries from the source to the new table.
        // This is a shallow copy — both tables point to the same lower-
        // level tables. For fork, the caller should use COW instead.
        let src_p4_virt = self.physical_memory_offset + self.root_phys.as_u64();
        unsafe {
            core::ptr::copy_nonoverlapping(
                src_p4_virt.as_u64() as *const u64,
                new_p4_virt.as_u64() as *mut u64,
                512,
            );
        }

        Ok(Self {
            root_phys: new_root_phys,
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

    /// Switch to a different page table by loading CR3
    pub fn switch_table(&self, root_phys: PhysAddr) {
        use x86_64::registers::control::Cr3;
        // SAFETY: Loading CR3 with a valid P4 table physical address
        // switches the active address space. The caller must ensure
        // root_phys points to a valid 4 KiB-aligned P4 table.
        unsafe {
            Cr3::write(
                PhysFrame::containing_address(root_phys),
                x86_64::registers::control::Cr3Flags::empty(),
            );
        }
    }

    /// Flush TLB for a specific address
    pub fn flush_tlb(&self, virt: VirtAddr) {
        use x86_64::instructions::tlb;
        tlb::flush(virt);
    }

    /// Flush entire TLB
    pub fn flush_tlb_all(&self) {
        use x86_64::instructions::tlb;
        tlb::flush_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_page_table_flags() {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER_ACCESSIBLE));
    }

    #[test_case]
    fn test_flags_conversion() {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        let x64_flags = flags.to_x64_flags();
        let converted_back = PageTableFlags::from_x64_flags(x64_flags);

        assert!(converted_back.contains(PageTableFlags::PRESENT));
        assert!(converted_back.contains(PageTableFlags::WRITABLE));
    }
}
