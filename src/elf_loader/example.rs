//! Example Usage of ELF Loader
//!
//! This file demonstrates how to use the ELF loader to load and execute
//! user-space programs in RustOS.

#![allow(dead_code)]

use super::*;
use x86_64::{VirtAddr, structures::paging::{FrameAllocator, Size4KiB, PhysFrame}};
use crate::memory;
use crate::process;

/// Example 1: Simple validation of an ELF binary
///
/// This is the simplest use case - just checking if a binary is a valid ELF file.
pub fn example_validate_binary(binary_data: &[u8]) {
    match elf_validate(binary_data) {
        Ok(()) => {
            println!("✅ Valid ELF binary");
            println!("   - Format: ELF64");
            println!("   - Architecture: x86_64");
            println!("   - Ready for loading");
        }
        Err(ElfError::InvalidMagic) => {
            println!("❌ Not an ELF file - invalid magic number");
        }
        Err(ElfError::InvalidMachine) => {
            println!("❌ Wrong architecture - not x86_64");
        }
        Err(e) => {
            println!("❌ Invalid ELF: {:?}", e);
        }
    }
}

/// Example 2: Load and inspect an ELF binary
///
/// This demonstrates loading an ELF binary and examining its segments
/// without actually mapping it into memory.
pub fn example_load_and_inspect(binary_data: &[u8]) -> Result<()> {
    // Load the ELF image
    let image = elf_load(binary_data, None)?;

    // Display basic information
    println!("ELF Image Information:");
    println!("  Entry Point: {:?}", image.entry_point);
    println!("  Is PIE: {}", image.is_pie);
    println!("  Base Address: {:?}", image.base_address);
    println!("  Program Break: {:?}", image.program_break);
    println!("  Stack Address: {:?}", image.stack_address);
    println!();

    // Display segment information
    println!("Segments ({} total):", image.segments.len());
    for (i, segment) in image.segments.iter().enumerate() {
        println!("  Segment {}:", i);
        println!("    Virtual Address: {:?}", segment.vaddr);
        println!("    Memory Size: {} bytes ({} KB)",
                 segment.mem_size, segment.mem_size / 1024);
        println!("    File Size: {} bytes", segment.file_size);
        println!("    Permissions: {}{}{}",
                 if segment.flags.readable { "R" } else { "-" },
                 if segment.flags.writable { "W" } else { "-" },
                 if segment.flags.executable { "X" } else { "-" });
        println!("    Type: {:?}", segment.segment_type);

        // Calculate BSS size (zero-initialized data)
        let bss_size = segment.mem_size.saturating_sub(segment.file_size);
        if bss_size > 0 {
            println!("    BSS Size: {} bytes", bss_size);
        }
        println!();
    }

    Ok(())
}

/// Example 3: Load a static executable and map it into memory
///
/// This is the complete flow for loading a static executable:
/// 1. Validate the binary
/// 2. Load the ELF image
/// 3. Create a page table
/// 4. Map segments into the page table
/// 5. Create a stack
/// 6. Return entry point and stack pointer
pub fn example_load_static_executable<A>(
    binary_data: &[u8],
    frame_allocator: &mut A,
) -> Result<(VirtAddr, VirtAddr)>
where
    A: FrameAllocator<Size4KiB>,
{
    // Step 1: Validate
    elf_validate(binary_data)?;
    println!("✅ Binary validated");

    // Step 2: Load ELF image
    let image = elf_load(binary_data, None)?;
    println!("✅ ELF image loaded");
    println!("   Entry point: {:?}", image.entry_point);

    // Step 3: Create process page table
    // This creates a new page table for the process with kernel mappings.
    let mut mapper = create_process_page_table(frame_allocator)?;
    println!("✅ Page table created");

    // Step 4: Map segments
    elf_map_segments(&image, binary_data, &mut mapper, frame_allocator)?;
    println!("✅ Segments mapped:");
    for segment in &image.segments {
        println!("   {:?} at {:?} ({}{}{})",
                 segment.segment_type,
                 segment.vaddr,
                 if segment.flags.readable { "R" } else { "-" },
                 if segment.flags.writable { "W" } else { "-" },
                 if segment.flags.executable { "X" } else { "-" });
    }

    // Step 5: Create stack (8 MB)
    let stack_size = 8 * 1024 * 1024;
    let stack_pointer = elf_create_stack(
        &mut mapper,
        frame_allocator,
        image.stack_address,
        stack_size,
    )?;
    println!("✅ Stack created");
    println!("   Stack pointer: {:?}", stack_pointer);
    println!("   Stack size: {} MB", stack_size / (1024 * 1024));

    // Return entry point and stack pointer for process creation
    Ok((image.entry_point, stack_pointer))
}

/// Example 4: Load a PIE (Position-Independent Executable)
///
/// PIE executables can be loaded at any base address, which is useful for
/// security (ASLR - Address Space Layout Randomization).
pub fn example_load_pie_executable<A>(
    binary_data: &[u8],
    base_address: VirtAddr,
    frame_allocator: &mut A,
) -> Result<(VirtAddr, VirtAddr)>
where
    A: FrameAllocator<Size4KiB>,
{
    // Load with custom base address
    let image = elf_load(binary_data, Some(base_address))?;

    if !image.is_pie {
        println!("⚠️  Warning: Binary is not PIE, but loaded at custom address");
    } else {
        println!("✅ PIE executable loaded at base: {:?}", image.base_address);
    }

    // Rest of the loading process is the same as static executables
    let mut mapper = create_process_page_table(frame_allocator)?;
    elf_map_segments(&image, binary_data, &mut mapper, frame_allocator)?;

    let stack_size = 8 * 1024 * 1024;
    let stack_pointer = elf_create_stack(
        &mut mapper,
        frame_allocator,
        image.stack_address,
        stack_size,
    )?;

    Ok((image.entry_point, stack_pointer))
}

/// Example 5: Complete process creation flow
///
/// This demonstrates the full integration with the process management system.
pub fn example_create_process_from_elf<A>(
    program_name: &str,
    binary_data: &[u8],
    frame_allocator: &mut A,
) -> Result<ProcessId>
where
    A: FrameAllocator<Size4KiB>,
{
    println!("Creating process: {}", program_name);

    // Validate the binary first
    elf_validate(binary_data)?;

    // Load the ELF image
    let image = elf_load(binary_data, None)?;

    // Create process page table
    let mut mapper = create_process_page_table(frame_allocator)?;

    // Map all segments
    elf_map_segments(&image, binary_data, &mut mapper, frame_allocator)?;

    // Create stack
    let stack_size = 8 * 1024 * 1024;
    let stack_pointer = elf_create_stack(
        &mut mapper,
        frame_allocator,
        image.stack_address,
        stack_size,
    )?;

    // Create the process
    // This would integrate with your process management system
    let process_id = create_process(
        program_name,
        image.entry_point,
        stack_pointer,
        mapper,
        image.program_break, // Initial heap start
    )?;

    println!("✅ Process created: ID = {:?}", process_id);
    println!("   Entry point: {:?}", image.entry_point);
    println!("   Stack: {:?}", stack_pointer);
    println!("   Heap start: {:?}", image.program_break);

    Ok(process_id)
}

/// Example 6: Error handling
///
/// Demonstrates proper error handling when loading ELF binaries.
pub fn example_error_handling(binary_data: &[u8]) {
    match elf_load(binary_data, None) {
        Ok(image) => {
            println!("Loaded successfully: entry at {:?}", image.entry_point);
        }
        Err(ElfError::InvalidMagic) => {
            println!("Error: Not an ELF file");
            println!("Hint: Check file format");
        }
        Err(ElfError::InvalidClass) => {
            println!("Error: Not a 64-bit ELF");
            println!("Hint: RustOS only supports ELF64");
        }
        Err(ElfError::InvalidMachine) => {
            println!("Error: Wrong architecture");
            println!("Hint: Only x86_64 binaries are supported");
        }
        Err(ElfError::NoLoadableSegments) => {
            println!("Error: No loadable segments found");
            println!("Hint: Binary may be corrupted or incomplete");
        }
        Err(ElfError::SegmentOverlap) => {
            println!("Error: Segments overlap in memory");
            println!("Hint: Binary may be malformed or intentionally malicious");
        }
        Err(ElfError::AllocationFailed) => {
            println!("Error: Out of memory");
            println!("Hint: System may need more RAM or memory is fragmented");
        }
        Err(e) => {
            println!("Error loading ELF: {:?}", e);
        }
    }
}

/// Example 7: Memory usage analysis
///
/// Calculate how much memory a binary will require.
pub fn example_memory_analysis(binary_data: &[u8]) -> Result<()> {
    let image = elf_load(binary_data, None)?;

    let mut total_memory = 0usize;
    let mut code_memory = 0usize;
    let mut data_memory = 0usize;

    for segment in &image.segments {
        total_memory += segment.mem_size;

        if segment.flags.executable && !segment.flags.writable {
            code_memory += segment.mem_size;
        } else if segment.flags.writable {
            data_memory += segment.mem_size;
        }
    }

    println!("Memory Requirements:");
    println!("  Code segments: {} KB", code_memory / 1024);
    println!("  Data segments: {} KB", data_memory / 1024);
    println!("  Total program: {} KB", total_memory / 1024);
    println!("  Stack (default): 8192 KB");
    println!("  Estimated total: {} KB", (total_memory / 1024) + 8192);

    Ok(())
}

// Helper types and functions for examples
// These use the kernel's real memory and process management subsystems.

type ProcessId = u32;

/// Wrapper that holds a lock guard on the kernel's PageTableManager
/// and exposes the inner OffsetPageTable as a Mapper<Size4KiB>.
struct KernelMapperGuard<'a> {
    guard: spin::MutexGuard<'a, memory::PageTableManager>,
}

impl<'a> x86_64::structures::paging::Mapper<Size4KiB> for KernelMapperGuard<'a> {
    unsafe fn map_to<A>(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        frame: x86_64::structures::paging::PhysFrame<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
        allocator: &mut A,
    ) -> core::result::Result<
        x86_64::structures::paging::MapperFlush<Size4KiB>,
        x86_64::structures::paging::mapper::MapToError<Size4KiB>,
    >
    where
        A: FrameAllocator<Size4KiB>,
    {
        self.guard.mapper_mut().map_to(page, frame, flags, allocator)
    }

    fn unmap(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
    ) -> core::result::Result<
        (
            x86_64::structures::paging::PhysFrame<Size4KiB>,
            x86_64::structures::paging::MapperFlush<Size4KiB>,
        ),
        x86_64::structures::paging::mapper::UnmapError,
    > {
        self.guard.mapper_mut().unmap(page)
    }

    unsafe fn update_flags(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> core::result::Result<
        x86_64::structures::paging::MapperFlush<Size4KiB>,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        self.guard.mapper_mut().update_flags(page, flags)
    }

    unsafe fn set_flags_p4_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> core::result::Result<
        x86_64::structures::paging::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        self.guard.mapper_mut().set_flags_p4_entry(page, flags)
    }

    unsafe fn set_flags_p3_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> core::result::Result<
        x86_64::structures::paging::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        self.guard.mapper_mut().set_flags_p3_entry(page, flags)
    }

    unsafe fn set_flags_p2_entry(
        &mut self,
        page: x86_64::structures::paging::Page<Size4KiB>,
        flags: x86_64::structures::paging::PageTableFlags,
    ) -> core::result::Result<
        x86_64::structures::paging::MapperFlushAll,
        x86_64::structures::paging::mapper::FlagUpdateError,
    > {
        self.guard.mapper_mut().set_flags_p2_entry(page, flags)
    }

    fn translate_page(
        &self,
        page: x86_64::structures::paging::Page<Size4KiB>,
    ) -> core::result::Result<
        x86_64::structures::paging::PhysFrame<Size4KiB>,
        x86_64::structures::paging::mapper::TranslateError,
    > {
        use x86_64::structures::paging::mapper::TranslateError;
        match self.guard.translate_addr(page.start_address()) {
            Some(phys) => Ok(PhysFrame::containing_address(phys)),
            None => Err(TranslateError::PageNotMapped),
        }
    }
}

impl<'a> x86_64::structures::paging::Translate for KernelMapperGuard<'a> {
    fn translate(&self, addr: VirtAddr) -> x86_64::structures::paging::mapper::TranslateResult {
        use x86_64::structures::paging::mapper::{TranslateResult, MappedFrame};
        match self.guard.translate_addr(addr) {
            Some(phys) => {
                let frame = MappedFrame::4KiB(PhysFrame::containing_address(phys));
                TranslateResult::Mapped {
                    frame,
                    offset: addr.as_u64() % 4096,
                    flags: x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::WRITABLE,
                }
            }
            None => TranslateResult::PageNotMapped,
        }
    }
}

fn create_process_page_table<A>(_allocator: &mut A) -> Result<KernelMapperGuard<'static>>
where
    A: FrameAllocator<Size4KiB>,
{
    let mm = memory::get_memory_manager().ok_or(ElfError::AllocationFailed)?;
    Ok(KernelMapperGuard {
        guard: mm.page_table_manager.lock(),
    })
}

fn create_process(
    name: &str,
    _entry: VirtAddr,
    _stack: VirtAddr,
    _mapper: KernelMapperGuard<'static>,
    _heap_start: VirtAddr,
) -> Result<ProcessId> {
    let pm = process::get_process_manager();
    pm.create_process(name, None, process::Priority::Normal)
        .map_err(|_| ElfError::AllocationFailed)
}
