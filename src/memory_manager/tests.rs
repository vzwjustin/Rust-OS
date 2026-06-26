//! Memory Manager Integration Tests

#![cfg(test)]

use super::*;
use super::api::*;
use x86_64::{PhysAddr, VirtAddr};

#[test_case]
fn test_protection_flags_operations() {
    let read = ProtectionFlags::READ;
    let write = ProtectionFlags::WRITE;
    let exec = ProtectionFlags::EXECUTE;

    // Test individual flags
    assert!(read.is_readable());
    assert!(!read.is_writable());
    assert!(!read.is_executable());

    // Test combined flags
    let rw = read | write;
    assert!(rw.is_readable());
    assert!(rw.is_writable());
    assert!(!rw.is_executable());

    let rwx = rw | exec;
    assert!(rwx.is_readable());
    assert!(rwx.is_writable());
    assert!(rwx.is_executable());

    // Test READ_WRITE constant
    assert!(ProtectionFlags::READ_WRITE.is_readable());
    assert!(ProtectionFlags::READ_WRITE.is_writable());
}

#[test_case]
fn test_page_table_flags() {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    assert!(flags.contains(PageTableFlags::PRESENT));
    assert!(flags.contains(PageTableFlags::WRITABLE));
    assert!(!flags.contains(PageTableFlags::USER_ACCESSIBLE));
    assert!(!flags.contains(PageTableFlags::NO_EXECUTE));
}

#[test_case]
fn test_page_table_flags_conversion() {
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE;

    let x64_flags = flags.to_x64_flags();
    let converted = PageTableFlags::from_x64_flags(x64_flags);

    assert!(converted.contains(PageTableFlags::PRESENT));
    assert!(converted.contains(PageTableFlags::WRITABLE));
    assert!(converted.contains(PageTableFlags::USER_ACCESSIBLE));
}

#[test_case]
fn test_memory_region_creation() {
    let start = VirtAddr::new(0x1000);
    let end = VirtAddr::new(0x3000);
    let prot = ProtectionFlags::READ_WRITE;

    let region = MemoryRegion::anonymous(start, end, prot);

    assert_eq!(region.start, start);
    assert_eq!(region.end, end);
    assert_eq!(region.size(), 0x2000);
    assert_eq!(region.size_in_pages(), 2);
    assert_eq!(region.memory_type, MemoryType::Anonymous);
}

#[test_case]
fn test_memory_region_contains() {
    let start = VirtAddr::new(0x1000);
    let end = VirtAddr::new(0x3000);
    let region = MemoryRegion::anonymous(start, end, ProtectionFlags::READ_WRITE);

    assert!(region.contains(VirtAddr::new(0x1000)));
    assert!(region.contains(VirtAddr::new(0x2000)));
    assert!(!region.contains(VirtAddr::new(0x3000))); // End is exclusive
    assert!(!region.contains(VirtAddr::new(0x4000)));
}

#[test_case]
fn test_memory_region_overlap() {
    let region1 = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x3000),
        ProtectionFlags::READ_WRITE,
    );

    let region2 = MemoryRegion::anonymous(
        VirtAddr::new(0x2000),
        VirtAddr::new(0x4000),
        ProtectionFlags::READ_WRITE,
    );

    let region3 = MemoryRegion::anonymous(
        VirtAddr::new(0x4000),
        VirtAddr::new(0x5000),
        ProtectionFlags::READ_WRITE,
    );

    assert!(region1.overlaps(&region2));
    assert!(region2.overlaps(&region1));
    assert!(!region1.overlaps(&region3));
}

#[test_case]
fn test_memory_region_split() {
    let start = VirtAddr::new(0x1000);
    let end = VirtAddr::new(0x5000);
    let region = MemoryRegion::anonymous(start, end, ProtectionFlags::READ_WRITE);

    let split_addr = VirtAddr::new(0x3000);
    let (first, second) = region.split_at(split_addr).unwrap();

    assert_eq!(first.start, start);
    assert_eq!(first.end, split_addr);
    assert_eq!(second.start, split_addr);
    assert_eq!(second.end, end);
    assert_eq!(first.size() + second.size(), region.size());
}

#[test_case]
fn test_memory_region_merge() {
    let region1 = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x3000),
        ProtectionFlags::READ_WRITE,
    );

    let region2 = MemoryRegion::anonymous(
        VirtAddr::new(0x3000),
        VirtAddr::new(0x5000),
        ProtectionFlags::READ_WRITE,
    );

    let merged = region1.try_merge(&region2).unwrap();

    assert_eq!(merged.start, VirtAddr::new(0x1000));
    assert_eq!(merged.end, VirtAddr::new(0x5000));
    assert_eq!(merged.size(), 0x4000);
}

#[test_case]
fn test_memory_region_merge_incompatible() {
    let region1 = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x3000),
        ProtectionFlags::READ_WRITE,
    );

    let region2 = MemoryRegion::anonymous(
        VirtAddr::new(0x3000),
        VirtAddr::new(0x5000),
        ProtectionFlags::READ_EXEC, // Different protection
    );

    assert!(region1.try_merge(&region2).is_none());
}

#[test_case]
fn test_memory_region_protection_update() {
    let mut region = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x2000),
        ProtectionFlags::READ_WRITE,
    );

    assert!(region.protection.is_writable());

    region.set_protection(ProtectionFlags::READ);

    assert!(!region.protection.is_writable());
    assert!(region.protection.is_readable());
}

#[test_case]
fn test_mmap_flags() {
    let anon_private = MmapFlags::anonymous_private();
    assert!(anon_private.anonymous);
    assert!(anon_private.private);
    assert!(!anon_private.shared);
    assert!(!anon_private.fixed);

    let shared = MmapFlags::shared();
    assert!(shared.shared);
    assert!(!shared.private);
    assert!(!shared.anonymous);

    let fixed = MmapFlags::fixed();
    assert!(fixed.fixed);
}

#[test_case]
fn test_memory_region_validation() {
    // Valid region
    let valid = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x2000),
        ProtectionFlags::READ_WRITE,
    );
    assert!(valid.is_valid());

    // Invalid: start >= end
    let invalid = MemoryRegion::anonymous(
        VirtAddr::new(0x2000),
        VirtAddr::new(0x1000),
        ProtectionFlags::READ_WRITE,
    );
    assert!(!invalid.is_valid());
}

#[test_case]
fn test_memory_types() {
    assert_eq!(
        core::mem::size_of::<MemoryType>(),
        core::mem::size_of::<u8>()
    );

    let types = [
        MemoryType::Anonymous,
        MemoryType::FileBacked,
        MemoryType::Shared,
        MemoryType::Stack,
        MemoryType::Heap,
        MemoryType::Code,
        MemoryType::Data,
        MemoryType::Device,
    ];

    // All types should be distinct
    for (i, t1) in types.iter().enumerate() {
        for (j, t2) in types.iter().enumerate() {
            if i != j {
                assert_ne!(t1, t2);
            }
        }
    }
}

#[test_case]
fn test_protection_flag_combinations() {
    // Test all valid combinations
    let none = ProtectionFlags::NONE;
    let r = ProtectionFlags::READ;
    let w = ProtectionFlags::WRITE;
    let x = ProtectionFlags::EXECUTE;
    let rw = r | w;
    let rx = r | x;
    let wx = w | x;
    let rwx = r | w | x;

    assert!(!none.is_readable());
    assert!(r.is_readable());
    assert!(rw.is_readable() && rw.is_writable());
    assert!(rx.is_readable() && rx.is_executable());
    assert!(rwx.is_readable() && rwx.is_writable() && rwx.is_executable());
}

#[test_case]
fn test_vm_error_types() {
    let errors = [
        VmError::InvalidAddress,
        VmError::InvalidSize,
        VmError::OutOfMemory,
        VmError::PermissionDenied,
        VmError::RegionNotFound,
        VmError::AlreadyMapped,
        VmError::InvalidFlags,
        VmError::NotAligned,
        VmError::NotInitialized,
        VmError::AlreadyInitialized,
        VmError::InvalidOperation,
    ];

    // All errors should be copyable and comparable
    for e in &errors {
        let copied = *e;
        assert_eq!(copied, *e);
    }
}

#[test_case]
fn test_memory_stats_default() {
    let stats = MemoryStats::default();

    assert_eq!(stats.total_allocated, 0);
    assert_eq!(stats.region_count, 0);
    assert_eq!(stats.current_brk, 0);
    assert_eq!(stats.mapped_pages, 0);
}

#[test_case]
fn test_file_backed_region() {
    let region = MemoryRegion::file_backed(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x3000),
        ProtectionFlags::READ_WRITE,
        5, // file descriptor
        1024, // offset
    );

    assert_eq!(region.memory_type, MemoryType::FileBacked);
    assert_eq!(region.file_descriptor, Some(5));
    assert_eq!(region.file_offset, 1024);
}

#[test_case]
fn test_shared_region() {
    let region = MemoryRegion::shared(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x2000),
        ProtectionFlags::READ_WRITE,
    );

    assert_eq!(region.memory_type, MemoryType::Shared);
    assert!(region.shared);
}

#[test_case]
fn test_region_copy_on_write() {
    let mut region = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x2000),
        ProtectionFlags::READ_WRITE,
    );

    assert!(!region.copy_on_write);

    region.set_copy_on_write(true);
    assert!(region.copy_on_write);

    region.set_copy_on_write(false);
    assert!(!region.copy_on_write);
}

#[test_case]
fn test_region_naming() {
    let mut region = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x2000),
        ProtectionFlags::READ_WRITE,
    );

    assert!(region.name.is_none());

    region.set_name("test_heap".to_string());
    assert_eq!(region.name.as_ref().unwrap(), "test_heap");
}

#[test_case]
fn test_region_range() {
    let region = MemoryRegion::anonymous(
        VirtAddr::new(0x1000),
        VirtAddr::new(0x3000),
        ProtectionFlags::READ_WRITE,
    );

    let range = region.range();
    assert_eq!(range.start, 0x1000);
    assert_eq!(range.end, 0x3000);
}

#[test_case]
fn test_page_size_constants() {
    use super::super::memory;
    assert_eq!(memory::PAGE_SIZE, 4096);
    assert_eq!(1 << memory::PAGE_SHIFT, memory::PAGE_SIZE);
}

/// Run all tests
pub fn run_all_tests() {
    println!("Running memory manager tests...\n");

    // Protection flags
    test_protection_flags_operations();
    test_protection_flag_combinations();
    println!("✓ Protection flags tests passed");

    // Page table flags
    test_page_table_flags();
    test_page_table_flags_conversion();
    println!("✓ Page table flags tests passed");

    // Memory regions
    test_memory_region_creation();
    test_memory_region_contains();
    test_memory_region_overlap();
    test_memory_region_split();
    test_memory_region_merge();
    test_memory_region_merge_incompatible();
    test_memory_region_protection_update();
    test_memory_region_validation();
    test_file_backed_region();
    test_shared_region();
    test_region_copy_on_write();
    test_region_naming();
    test_region_range();
    println!("✓ Memory region tests passed");

    // Flags and configuration
    test_mmap_flags();
    test_memory_types();
    println!("✓ Configuration tests passed");

    // Error handling
    test_vm_error_types();
    println!("✓ Error handling tests passed");

    // Statistics
    test_memory_stats_default();
    println!("✓ Statistics tests passed");

    println!("\n✓ All tests passed!");
}
