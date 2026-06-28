//! Memory Manager Usage Examples
//!
//! Demonstrates common usage patterns for the RustOS memory management system.

#![allow(dead_code)]

use super::api::*;
use super::{MmapFlags, ProtectionFlags};
use crate::{print, println};
use x86_64::{PhysAddr, VirtAddr};

/// Example 1: Basic memory allocation with mmap
pub fn example_basic_allocation() -> Result<(), super::VmError> {
    // Allocate 4KB of anonymous memory
    let size = 4096;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();

    let ptr = vm_mmap(0, size, prot, flags)?;

    // Use the allocated memory
    unsafe {
        // Write to the memory
        for i in 0..1024 {
            *ptr.add(i) = (i % 256) as u8;
        }

        // Read back
        for i in 0..1024 {
            assert_eq!(*ptr.add(i), (i % 256) as u8);
        }
    }

    // Clean up
    vm_munmap(ptr as usize, size)?;

    Ok(())
}

/// Example 2: Large memory allocation
pub fn example_large_allocation() -> Result<(), super::VmError> {
    // Allocate 1MB
    let size = 1024 * 1024;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();

    let ptr = vm_mmap(0, size, prot, flags)?;

    // Memory is available for use
    unsafe {
        // Initialize first and last pages
        *ptr = 0x42;
        *ptr.add(size - 1) = 0x43;
    }

    vm_munmap(ptr as usize, size)?;

    Ok(())
}

/// Example 3: Fixed address mapping
pub fn example_fixed_mapping() -> Result<(), super::VmError> {
    // Map at specific address
    let addr = 0x4000_0000;
    let size = 8192;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::fixed();

    let ptr = vm_mmap(addr, size, prot, flags)?;

    // Verify we got the requested address
    assert_eq!(ptr as usize, addr);

    vm_munmap(addr, size)?;

    Ok(())
}

/// Example 4: Memory protection changes
pub fn example_protection_change() -> Result<(), super::VmError> {
    let size = 4096;

    // Allocate read-write memory
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();
    let ptr = vm_mmap(0, size, prot, flags)?;

    // Write some data
    unsafe {
        *ptr = 0xFF;
    }

    // Change to read-only
    let addr = ptr as usize;
    let new_prot = ProtectionFlags::READ;
    vm_mprotect(addr, size, new_prot)?;

    // Now writes would cause page fault
    // unsafe { *ptr = 0x00; } // Would fault!

    // Change back to read-write
    vm_mprotect(addr, size, ProtectionFlags::READ_WRITE)?;

    // Now writes work again
    unsafe {
        *ptr = 0x00;
    }

    vm_munmap(addr, size)?;

    Ok(())
}

/// Example 5: Executable memory for JIT
pub fn example_executable_memory() -> Result<(), super::VmError> {
    let size = 4096;

    // Allocate read-write memory first
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();
    let ptr = vm_mmap(0, size, prot, flags)?;

    // Write machine code
    unsafe {
        // x86_64: ret instruction (0xC3)
        *ptr = 0xC3;
    }

    // Change to executable
    let addr = ptr as usize;
    vm_mprotect(addr, size, ProtectionFlags::READ_EXEC)?;

    // Now can execute the code
    // let func: fn() = unsafe { core::mem::transmute(ptr) };
    // func(); // Would execute

    vm_munmap(addr, size)?;

    Ok(())
}

/// Example 6: Heap management with brk
pub fn example_heap_management() -> Result<(), super::VmError> {
    // Query current break
    let initial_brk = vm_brk(0)?;

    // Extend heap by 16KB
    let new_brk = initial_brk + 16384;
    let result_brk = vm_brk(new_brk)?;
    assert_eq!(result_brk, new_brk);

    // Use the heap
    let heap_ptr = initial_brk as *mut u8;
    unsafe {
        *heap_ptr = 42;
    }

    // Shrink heap
    vm_brk(initial_brk)?;

    Ok(())
}

/// Example 7: Using sbrk interface
pub fn example_sbrk() -> Result<(), super::VmError> {
    // Allocate 4KB
    let old_brk = vm_sbrk(4096)?;

    // Use the memory
    let ptr = old_brk as *mut u8;
    unsafe {
        *ptr = 100;
    }

    // Allocate more
    vm_sbrk(4096)?;

    // Free half
    vm_sbrk(-2048)?;

    Ok(())
}

/// Example 8: Multiple memory regions
pub fn example_multiple_regions() -> Result<(), super::VmError> {
    let size = 4096;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();

    // Allocate several regions
    let ptr1 = vm_mmap(0, size, prot, flags)?;
    let ptr2 = vm_mmap(0, size, prot, flags)?;
    let ptr3 = vm_mmap(0, size, prot, flags)?;

    // Use all regions
    unsafe {
        *ptr1 = 1;
        *ptr2 = 2;
        *ptr3 = 3;
    }

    // Free them
    vm_munmap(ptr1 as usize, size)?;
    vm_munmap(ptr2 as usize, size)?;
    vm_munmap(ptr3 as usize, size)?;

    Ok(())
}

/// Example 9: Page table operations
pub fn example_page_table() -> Result<(), super::VmError> {
    use super::PageTableFlags;

    // Create a new page table
    let mut page_table = page_table_create()?;

    // Map a virtual page
    let virt = VirtAddr::new(0x1000);
    let phys = PhysAddr::new(0x10000);
    let flags =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    page_table_map(&mut page_table, virt, phys, flags)?;

    // Translate virtual to physical
    if let Some(translated) = page_table_translate(&page_table, virt) {
        assert_eq!(translated, phys);
    }

    // Unmap the page
    page_table_unmap(&mut page_table, virt)?;

    Ok(())
}

/// Example 10: Memory statistics
pub fn example_memory_stats() -> Result<(), super::VmError> {
    // Allocate some memory
    let ptr = vm_mmap(
        0,
        8192,
        ProtectionFlags::READ_WRITE,
        MmapFlags::anonymous_private(),
    )?;

    // Get statistics
    let stats = get_memory_stats()?;

    // Check stats
    assert!(stats.total_allocated >= 8192);
    assert!(stats.region_count > 0);
    assert!(stats.mapped_pages >= 2);

    vm_munmap(ptr as usize, 8192)?;

    Ok(())
}

/// Example 11: Stack allocation for threads
pub fn example_thread_stack() -> Result<(), super::VmError> {
    // Allocate 1MB stack
    let stack_size = 1024 * 1024;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();

    let stack_bottom = vm_mmap(0, stack_size, prot, flags)?;

    // Stack grows down, so stack pointer starts at top
    let _stack_top = unsafe { stack_bottom.add(stack_size) };

    // Initialize stack with guard value
    unsafe {
        *(stack_bottom as *mut u32) = 0xDEADBEEF;
    }

    // Clean up
    vm_munmap(stack_bottom as usize, stack_size)?;

    Ok(())
}

/// Example 12: Guard pages for stack overflow protection
pub fn example_guard_pages() -> Result<(), super::VmError> {
    let page_size = 4096;
    let stack_pages = 256; // 1MB stack

    // Allocate space for guard page + stack
    let total_size = page_size * (stack_pages + 1);
    let base = vm_mmap(
        0,
        total_size,
        ProtectionFlags::READ_WRITE,
        MmapFlags::anonymous_private(),
    )?;

    // Make first page a guard page (no access)
    vm_mprotect(base as usize, page_size, ProtectionFlags::NONE)?;

    // Stack starts after guard page
    let _stack_start = unsafe { base.add(page_size) };

    // Access to guard page would cause fault
    // unsafe { *base = 0; } // Would fault!

    vm_munmap(base as usize, total_size)?;

    Ok(())
}

/// Example 13: Shared memory (IPC)
pub fn example_shared_memory() -> Result<(), super::VmError> {
    let size = 4096;
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::shared();

    // Allocate shared memory
    let ptr = vm_mmap(0, size, prot, flags)?;

    // This memory can be shared between processes
    unsafe {
        *ptr = 0x42;
    }

    vm_munmap(ptr as usize, size)?;

    Ok(())
}

/// Example 14: Memory region alignment
pub fn example_alignment() -> Result<(), super::VmError> {
    // Allocate non-page-aligned size
    let size = 5000; // Not 4K aligned
    let prot = ProtectionFlags::READ_WRITE;
    let flags = MmapFlags::anonymous_private();

    // Will be rounded up to 8192 (2 pages)
    let ptr = vm_mmap(0, size, prot, flags)?;

    // Address is page-aligned
    assert_eq!((ptr as usize) % 4096, 0);

    // Can access the full rounded-up size
    unsafe {
        *ptr.add(5000) = 0; // Still valid
        *ptr.add(8191) = 0; // Last byte of second page
    }

    vm_munmap(ptr as usize, size)?;

    Ok(())
}

/// Example 15: Complete memory lifecycle
pub fn example_complete_lifecycle() -> Result<(), super::VmError> {
    // 1. Allocate
    let size = 4096;
    let ptr = vm_mmap(
        0,
        size,
        ProtectionFlags::READ_WRITE,
        MmapFlags::anonymous_private(),
    )?;

    // 2. Use memory
    unsafe {
        for i in 0..1024 {
            *ptr.add(i) = i as u8;
        }
    }

    // 3. Change protection
    vm_mprotect(ptr as usize, size, ProtectionFlags::READ)?;

    // 4. Check statistics
    let stats = get_memory_stats()?;
    assert!(stats.total_allocated >= size);

    // 5. Restore write access
    vm_mprotect(ptr as usize, size, ProtectionFlags::READ_WRITE)?;

    // 6. Free memory
    vm_munmap(ptr as usize, size)?;

    Ok(())
}

/// Run all examples
pub fn run_all_examples() {
    println!("Running memory manager examples...\n");

    macro_rules! run_example {
        ($name:expr, $func:expr) => {
            print!("{}: ", $name);
            match $func {
                Ok(_) => println!("✓ PASSED"),
                Err(e) => println!("✗ FAILED: {:?}", e),
            }
        };
    }

    run_example!("Basic allocation", example_basic_allocation());
    run_example!("Large allocation", example_large_allocation());
    run_example!("Fixed mapping", example_fixed_mapping());
    run_example!("Protection change", example_protection_change());
    run_example!("Executable memory", example_executable_memory());
    run_example!("Heap management (brk)", example_heap_management());
    run_example!("Heap management (sbrk)", example_sbrk());
    run_example!("Multiple regions", example_multiple_regions());
    run_example!("Page table operations", example_page_table());
    run_example!("Memory statistics", example_memory_stats());
    run_example!("Thread stack", example_thread_stack());
    run_example!("Guard pages", example_guard_pages());
    run_example!("Shared memory", example_shared_memory());
    run_example!("Memory alignment", example_alignment());
    run_example!("Complete lifecycle", example_complete_lifecycle());

    println!("\nAll examples completed!");
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test_all_examples() {
        // In a test environment, we would initialize the memory manager first
        // For now, these serve as documentation
    }
}
