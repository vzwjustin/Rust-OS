//! ELF Loader Implementation
//!
//! Loads ELF executables into memory and sets up process address space.

use super::*;
use crate::elf_loader::types::*;
use crate::elf_loader::parser::*;
use x86_64::structures::paging::{
    PageTableFlags, Mapper, Page, Size4KiB, FrameAllocator,
};
use x86_64::VirtAddr;
use alloc::vec::Vec;
use crate::memory::PAGE_SIZE;

/// Default stack size (8 MB)
const DEFAULT_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Default user stack location (grows downward from here)
const DEFAULT_STACK_TOP: u64 = 0x0000_7fff_ffff_0000;

/// PIE base address (if not specified)
const DEFAULT_PIE_BASE: u64 = 0x0000_5555_5555_0000;

/// Load ELF binary and create image representation
pub fn load_elf_image(binary_data: &[u8], load_bias: Option<VirtAddr>) -> Result<ElfImage> {
    // Parse and validate ELF header
    let header = parse_elf_header(binary_data)?;

    // Get loadable segments
    let segments = get_loadable_segments(binary_data)?;

    // Validate all segments
    for segment in &segments {
        validate_segment(segment, binary_data)?;
    }

    // Check for overlapping segments
    check_segment_overlap(&segments)?;

    // Determine if PIE and calculate base address
    let is_pie = header.is_pie();
    let base_address = if is_pie {
        load_bias.unwrap_or(VirtAddr::new(DEFAULT_PIE_BASE))
    } else {
        VirtAddr::new(0)
    };

    // Calculate entry point
    let entry_point = if is_pie {
        base_address + header.e_entry
    } else {
        VirtAddr::new(header.e_entry)
    };

    // Load segments
    let mut loaded_segments = Vec::new();
    let mut max_addr = 0u64;

    for segment in segments {
        let vaddr = if is_pie {
            base_address + segment.vaddr()
        } else {
            VirtAddr::new(segment.vaddr())
        };

        let segment_end = vaddr.as_u64() + segment.mem_size() as u64;
        max_addr = max_addr.max(segment_end);

        let segment_type = match segment.p_type {
            PT_LOAD => SegmentType::Load,
            PT_DYNAMIC => SegmentType::Dynamic,
            PT_INTERP => SegmentType::Interp,
            PT_NOTE => SegmentType::Note,
            PT_PHDR => SegmentType::Phdr,
            PT_TLS => SegmentType::Tls,
            PT_GNU_EH_FRAME => SegmentType::GnuEhFrame,
            PT_GNU_STACK => SegmentType::GnuStack,
            PT_GNU_RELRO => SegmentType::GnuRelro,
            other => SegmentType::Other(other),
        };

        let flags = SegmentFlags {
            readable: segment.is_readable(),
            writable: segment.is_writable(),
            executable: segment.is_executable(),
        };

        loaded_segments.push(LoadedSegment {
            vaddr,
            mem_size: segment.mem_size(),
            file_size: segment.file_size(),
            flags,
            segment_type,
        });
    }

    // Calculate program break (for heap)
    // Align up to page boundary
    let program_break = VirtAddr::new(align_up(max_addr, PAGE_SIZE as u64));

    // Set stack address
    let stack_address = VirtAddr::new(DEFAULT_STACK_TOP);

    Ok(ElfImage {
        entry_point,
        segments: loaded_segments,
        is_pie,
        base_address,
        program_break,
        stack_address,
    })
}

/// Map ELF segments into page table
pub fn map_segments_to_page_table<M, A>(
    image: &ElfImage,
    binary_data: &[u8],
    mapper: &mut M,
    frame_allocator: &mut A,
) -> Result<()>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    // Parse program headers again to get segment data
    let segments = get_loadable_segments(binary_data)?;

    for (loaded_seg, prog_header) in image.segments.iter().zip(segments.iter()) {
        if loaded_seg.segment_type != SegmentType::Load {
            continue;
        }

        map_segment_to_page_table(
            loaded_seg,
            prog_header,
            binary_data,
            mapper,
            frame_allocator,
        )?;
    }

    Ok(())
}

/// Map a single segment into page table
fn map_segment_to_page_table<M, A>(
    segment: &LoadedSegment,
    prog_header: &Elf64ProgramHeader,
    binary_data: &[u8],
    mapper: &mut M,
    frame_allocator: &mut A,
) -> Result<()>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    let start_addr = segment.vaddr;
    let end_addr = start_addr + segment.mem_size as u64;

    // Get page table flags
    let flags = segment.flags.to_page_flags();

    // Map all pages for this segment
    let start_page: Page = Page::containing_address(start_addr);
    let end_page: Page = Page::containing_address(end_addr - 1u64);

    for page in Page::range_inclusive(start_page, end_page) {
        // Allocate a frame
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(ElfError::AllocationFailed)?;

        // Map the page
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| ElfError::MappingFailed)?
                .flush();
        }

        // Copy segment data to the page
        let page_offset = page.start_address().as_u64() - start_addr.as_u64();
        let segment_offset = prog_header.offset() + page_offset as usize;

        if segment_offset < prog_header.offset() + segment.file_size {
            // This page contains file data
            let bytes_to_copy = core::cmp::min(
                PAGE_SIZE,
                segment.file_size - page_offset as usize,
            );

            if bytes_to_copy > 0 && segment_offset + bytes_to_copy <= binary_data.len() {
                let src = &binary_data[segment_offset..segment_offset + bytes_to_copy];
                let dst = page.start_address().as_mut_ptr::<u8>();

                unsafe {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), dst, bytes_to_copy);

                    // Zero the rest of the page if needed
                    if bytes_to_copy < PAGE_SIZE {
                        core::ptr::write_bytes(dst.add(bytes_to_copy), 0, PAGE_SIZE - bytes_to_copy);
                    }
                }
            }
        } else {
            // This is BSS (zero-initialized data)
            unsafe {
                core::ptr::write_bytes(
                    page.start_address().as_mut_ptr::<u8>(),
                    0,
                    PAGE_SIZE,
                );
            }
        }
    }

    Ok(())
}

/// Create process stack
pub fn create_process_stack<M, A>(
    mapper: &mut M,
    frame_allocator: &mut A,
    stack_bottom: VirtAddr,
    stack_size: usize,
) -> Result<VirtAddr>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    // Calculate stack top (grows downward)
    let stack_top = stack_bottom - stack_size as u64;

    // Stack should be readable and writable, but not executable
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_EXECUTE;

    // Map stack pages
    let start_page: Page = Page::containing_address(stack_top);
    let end_page: Page = Page::containing_address(stack_bottom - 1u64);

    for page in Page::range_inclusive(start_page, end_page) {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(ElfError::AllocationFailed)?;

        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| ElfError::MappingFailed)?
                .flush();
        }

        // Zero the stack page
        unsafe {
            core::ptr::write_bytes(
                page.start_address().as_mut_ptr::<u8>(),
                0,
                PAGE_SIZE,
            );
        }
    }

    // Return stack pointer (top of stack, aligned to 16 bytes)
    Ok(VirtAddr::new(align_down(stack_bottom.as_u64(), 16)))
}

/// Align value up to alignment
fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

/// Align value down to alignment
fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alignment() {
        assert_eq!(align_up(0x1000, 0x1000), 0x1000);
        assert_eq!(align_up(0x1001, 0x1000), 0x2000);
        assert_eq!(align_up(0x1fff, 0x1000), 0x2000);

        assert_eq!(align_down(0x1000, 0x1000), 0x1000);
        assert_eq!(align_down(0x1001, 0x1000), 0x1000);
        assert_eq!(align_down(0x1fff, 0x1000), 0x1000);
    }

    #[test]
    fn test_segment_flags() {
        let flags = SegmentFlags {
            readable: true,
            writable: true,
            executable: false,
        };

        let page_flags = flags.to_page_flags();
        assert!(page_flags.contains(PageTableFlags::PRESENT));
        assert!(page_flags.contains(PageTableFlags::WRITABLE));
        assert!(page_flags.contains(PageTableFlags::NO_EXECUTE));
    }
}
