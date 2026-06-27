//! ELF Binary Loader for RustOS
//!
//! This module provides a complete ELF64 binary loader for x86_64 architecture.
//! It supports loading static executables (ET_EXEC) and position-independent
//! executables (ET_DYN/PIE) into process address space.
//!
//! Features:
//! - ELF64 header validation and parsing
//! - Program header parsing and segment loading
//! - Memory mapping with proper permissions (R/W/X)
//! - BSS segment initialization
//! - Stack setup for process execution
//! - Entry point resolution
//! - Support for both static and PIE executables

#![allow(dead_code)]

use alloc::vec::Vec;
use x86_64::structures::paging::{FrameAllocator, Mapper, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

mod loader;
pub mod parser;
mod types;

/// Result type for ELF operations
pub type Result<T> = core::result::Result<T, ElfError>;

/// ELF loader errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Invalid ELF magic number
    InvalidMagic,
    /// Unsupported ELF class (not 64-bit)
    InvalidClass,
    /// Invalid endianness (not little-endian)
    InvalidEndianness,
    /// Unsupported ELF version
    InvalidVersion,
    /// Not an executable or shared object
    InvalidType,
    /// Not x86_64 architecture
    InvalidMachine,
    /// Invalid entry point address
    InvalidEntryPoint,
    /// No loadable segments found
    NoLoadableSegments,
    /// Segment alignment error
    InvalidAlignment,
    /// Segment size overflow
    SizeOverflow,
    /// Invalid segment flags
    InvalidFlags,
    /// Memory allocation failed
    AllocationFailed,
    /// Page mapping failed
    MappingFailed,
    /// Buffer too small
    BufferTooSmall,
    /// Invalid program header
    InvalidProgramHeader,
    /// Segment overlap detected
    SegmentOverlap,
}

/// Loaded ELF image representation
#[derive(Debug)]
pub struct ElfImage {
    /// Entry point virtual address
    pub entry_point: VirtAddr,
    /// Loaded segments with their virtual addresses
    pub segments: Vec<LoadedSegment>,
    /// Whether this is a position-independent executable
    pub is_pie: bool,
    /// Base address for PIE executables
    pub base_address: VirtAddr,
    /// Program break (end of data segment) for heap allocation
    pub program_break: VirtAddr,
    /// Recommended stack address
    pub stack_address: VirtAddr,
}

/// A loaded ELF segment in memory
#[derive(Debug, Clone)]
pub struct LoadedSegment {
    /// Virtual address where segment is loaded
    pub vaddr: VirtAddr,
    /// Size in memory (may be larger than file size for BSS)
    pub mem_size: usize,
    /// Size in file
    pub file_size: usize,
    /// Segment flags (readable, writable, executable)
    pub flags: SegmentFlags,
    /// Segment type
    pub segment_type: SegmentType,
}

/// Segment permission flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
}

impl SegmentFlags {
    /// Convert to page table flags
    pub fn to_page_flags(&self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        if self.writable {
            flags |= PageTableFlags::WRITABLE;
        }

        if !self.executable {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        flags
    }
}

/// Segment types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentType {
    Load,
    Dynamic,
    Interp,
    Note,
    Phdr,
    Tls,
    GnuEhFrame,
    GnuStack,
    GnuRelro,
    Other(u32),
}

/// Main API: Validate ELF binary format
///
/// # Arguments
/// * `binary_data` - Raw ELF binary data
///
/// # Returns
/// Ok(()) if valid, Err(ElfError) otherwise
pub fn elf_validate(binary_data: &[u8]) -> Result<()> {
    parser::parse_elf_header(binary_data)?;
    Ok(())
}

/// Main API: Load ELF binary and prepare for execution
///
/// # Arguments
/// * `binary_data` - Raw ELF binary data
/// * `load_bias` - Optional base address for PIE executables
///
/// # Returns
/// Loaded ELF image ready for execution
pub fn elf_load(binary_data: &[u8], load_bias: Option<VirtAddr>) -> Result<ElfImage> {
    loader::load_elf_image(binary_data, load_bias)
}

/// Main API: Get entry point from loaded image
///
/// # Arguments
/// * `image` - Loaded ELF image
///
/// # Returns
/// Entry point virtual address
pub fn elf_get_entry_point(image: &ElfImage) -> VirtAddr {
    image.entry_point
}

/// Main API: Map ELF segments into page table
///
/// # Arguments
/// * `image` - Loaded ELF image
/// * `binary_data` - Original binary data
/// * `mapper` - Page table mapper
/// * `frame_allocator` - Frame allocator for physical memory
///
/// # Returns
/// Ok(()) if successful, Err(ElfError) otherwise
pub fn elf_map_segments<M, A>(
    image: &ElfImage,
    binary_data: &[u8],
    mapper: &mut M,
    frame_allocator: &mut A,
) -> Result<()>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    loader::map_segments_to_page_table(image, binary_data, mapper, frame_allocator)
}

/// Helper: Create initial stack for process
///
/// # Arguments
/// * `mapper` - Page table mapper
/// * `frame_allocator` - Frame allocator
/// * `stack_bottom` - Bottom (highest address) of stack
/// * `stack_size` - Size of stack in bytes
///
/// # Returns
/// Stack pointer (top of stack)
pub fn elf_create_stack<M, A>(
    mapper: &mut M,
    frame_allocator: &mut A,
    stack_bottom: VirtAddr,
    stack_size: usize,
) -> Result<VirtAddr>
where
    M: Mapper<Size4KiB>,
    A: FrameAllocator<Size4KiB>,
{
    loader::create_process_stack(mapper, frame_allocator, stack_bottom, stack_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_segment_flags_conversion() {
        let flags = SegmentFlags {
            readable: true,
            writable: false,
            executable: true,
        };

        let page_flags = flags.to_page_flags();
        assert!(page_flags.contains(PageTableFlags::PRESENT));
        assert!(page_flags.contains(PageTableFlags::USER_ACCESSIBLE));
        assert!(!page_flags.contains(PageTableFlags::WRITABLE));
        assert!(!page_flags.contains(PageTableFlags::NO_EXECUTE));
    }
}
