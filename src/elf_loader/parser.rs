//! ELF Binary Parser
//!
//! Validates and parses ELF64 headers and program headers.

use super::*;
use crate::elf_loader::types::*;

/// Parse and validate ELF header
pub fn parse_elf_header(binary_data: &[u8]) -> Result<Elf64Header> {
    // Check minimum size
    if binary_data.len() < Elf64Header::SIZE {
        return Err(ElfError::BufferTooSmall);
    }

    // Parse header
    let header = Elf64Header::from_bytes(binary_data).ok_or(ElfError::BufferTooSmall)?;

    // Validate magic number
    if !header.validate_magic() {
        return Err(ElfError::InvalidMagic);
    }

    // Validate ELF class (must be 64-bit)
    if !header.is_64bit() {
        return Err(ElfError::InvalidClass);
    }

    // Validate endianness (must be little-endian)
    if !header.is_little_endian() {
        return Err(ElfError::InvalidEndianness);
    }

    // Validate version
    if !header.is_current_version() {
        return Err(ElfError::InvalidVersion);
    }

    // Validate type (must be executable or shared object)
    if !header.is_executable() && !header.is_pie() {
        return Err(ElfError::InvalidType);
    }

    // Validate machine (must be x86_64)
    if !header.is_x86_64() {
        return Err(ElfError::InvalidMachine);
    }

    // Validate entry point (must be non-zero for executables)
    if header.is_executable() && header.e_entry == 0 {
        return Err(ElfError::InvalidEntryPoint);
    }

    Ok(header)
}

/// Parse all program headers
pub fn parse_program_headers(binary_data: &[u8]) -> Result<Vec<Elf64ProgramHeader>> {
    let header = parse_elf_header(binary_data)?;

    let ph_offset = header.program_header_offset();
    let ph_count = header.program_header_count();
    let ph_size = header.program_header_entry_size();

    // Validate program header entry size
    if ph_size != Elf64ProgramHeader::SIZE {
        return Err(ElfError::InvalidProgramHeader);
    }

    // Calculate total size needed
    let total_size = ph_offset
        .checked_add(
            ph_count
                .checked_mul(ph_size)
                .ok_or(ElfError::SizeOverflow)?,
        )
        .ok_or(ElfError::SizeOverflow)?;

    if binary_data.len() < total_size {
        return Err(ElfError::BufferTooSmall);
    }

    // Parse each program header
    let mut program_headers = Vec::new();
    for i in 0..ph_count {
        let offset = ph_offset + i * ph_size;
        let ph_data = &binary_data[offset..offset + ph_size];

        let ph = Elf64ProgramHeader::from_bytes(ph_data).ok_or(ElfError::InvalidProgramHeader)?;

        program_headers.push(ph);
    }

    Ok(program_headers)
}

/// Get loadable program headers only
pub fn get_loadable_segments(binary_data: &[u8]) -> Result<Vec<Elf64ProgramHeader>> {
    let all_headers = parse_program_headers(binary_data)?;

    let loadable: Vec<_> = all_headers
        .into_iter()
        .filter(|ph| ph.is_loadable())
        .collect();

    if loadable.is_empty() {
        return Err(ElfError::NoLoadableSegments);
    }

    Ok(loadable)
}

/// Validate segment for loading
pub fn validate_segment(segment: &Elf64ProgramHeader, binary_data: &[u8]) -> Result<()> {
    // Check alignment (must be power of 2)
    let align = segment.alignment();
    if align > 0 && (align & (align - 1)) != 0 {
        return Err(ElfError::InvalidAlignment);
    }

    // Check file size doesn't exceed memory size
    if segment.file_size() > segment.mem_size() {
        return Err(ElfError::SizeOverflow);
    }

    // Check offset and size don't overflow
    let offset = segment.offset();
    let end_offset = offset
        .checked_add(segment.file_size())
        .ok_or(ElfError::SizeOverflow)?;

    if end_offset > binary_data.len() {
        return Err(ElfError::BufferTooSmall);
    }

    // Validate flags (at least one permission must be set)
    if segment.p_flags == 0 {
        return Err(ElfError::InvalidFlags);
    }

    Ok(())
}

/// Check for overlapping segments
pub fn check_segment_overlap(segments: &[Elf64ProgramHeader]) -> Result<()> {
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            let seg1 = &segments[i];
            let seg2 = &segments[j];

            let start1 = seg1.vaddr();
            let end1 = start1 + seg1.mem_size() as u64;
            let start2 = seg2.vaddr();
            let end2 = start2 + seg2.mem_size() as u64;

            // Check if segments overlap
            if start1 < end2 && start2 < end1 {
                return Err(ElfError::SegmentOverlap);
            }
        }
    }

    Ok(())
}

/// Calculate the total address range needed for the executable
pub fn calculate_address_range(segments: &[Elf64ProgramHeader]) -> (u64, u64) {
    let mut min_addr = u64::MAX;
    let mut max_addr = 0u64;

    for segment in segments {
        let start = segment.vaddr();
        let end = start + segment.mem_size() as u64;

        min_addr = min_addr.min(start);
        max_addr = max_addr.max(end);
    }

    (min_addr, max_addr)
}

/// Get segment data from binary
pub fn get_segment_data<'a>(
    segment: &Elf64ProgramHeader,
    binary_data: &'a [u8],
) -> Result<&'a [u8]> {
    let offset = segment.offset();
    let size = segment.file_size();

    if offset + size > binary_data.len() {
        return Err(ElfError::BufferTooSmall);
    }

    Ok(&binary_data[offset..offset + size])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_invalid_magic() {
        let data = [0u8; 64];
        assert!(matches!(
            parse_elf_header(&data),
            Err(ElfError::InvalidMagic)
        ));
    }

    #[test_case]
    fn test_buffer_too_small() {
        let data = [0u8; 32];
        assert!(matches!(
            parse_elf_header(&data),
            Err(ElfError::BufferTooSmall)
        ));
    }

    #[test_case]
    fn test_segment_overlap_detection() {
        // Create mock program headers with overlap
        let seg1 = Elf64ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_X,
            p_offset: 0x1000,
            p_vaddr: 0x400000,
            p_paddr: 0x400000,
            p_filesz: 0x1000,
            p_memsz: 0x1000,
            p_align: 0x1000,
        };

        let seg2 = Elf64ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_W,
            p_offset: 0x2000,
            p_vaddr: 0x400800, // Overlaps with seg1
            p_paddr: 0x400800,
            p_filesz: 0x1000,
            p_memsz: 0x1000,
            p_align: 0x1000,
        };

        let segments = vec![seg1, seg2];
        assert_eq!(
            check_segment_overlap(&segments),
            Err(ElfError::SegmentOverlap)
        );
    }

    #[test_case]
    fn test_address_range_calculation() {
        let seg1 = Elf64ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_X,
            p_offset: 0,
            p_vaddr: 0x400000,
            p_paddr: 0,
            p_filesz: 0x1000,
            p_memsz: 0x1000,
            p_align: 0x1000,
        };

        let seg2 = Elf64ProgramHeader {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_W,
            p_offset: 0,
            p_vaddr: 0x600000,
            p_paddr: 0,
            p_filesz: 0x2000,
            p_memsz: 0x2000,
            p_align: 0x1000,
        };

        let segments = vec![seg1, seg2];
        let (min, max) = calculate_address_range(&segments);

        assert_eq!(min, 0x400000);
        assert_eq!(max, 0x602000);
    }
}
