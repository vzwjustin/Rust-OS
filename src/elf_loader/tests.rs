//! ELF Loader Integration Tests

#![cfg(test)]

use super::*;
use crate::elf_loader::types::*;

/// Create a minimal valid ELF64 header for testing
fn create_test_elf_header() -> [u8; 64] {
    let mut data = [0u8; 64];

    // ELF magic
    data[0..4].copy_from_slice(&ELF_MAGIC);

    // ELF class (64-bit)
    data[EI_CLASS] = ELFCLASS64;

    // Data encoding (little-endian)
    data[EI_DATA] = ELFDATA2LSB;

    // Version
    data[EI_VERSION] = EV_CURRENT;

    // ELF type (executable) - bytes 16-17
    data[16] = (ET_EXEC & 0xff) as u8;
    data[17] = ((ET_EXEC >> 8) & 0xff) as u8;

    // Machine (x86_64) - bytes 18-19
    data[18] = (EM_X86_64 & 0xff) as u8;
    data[19] = ((EM_X86_64 >> 8) & 0xff) as u8;

    // Version (4 bytes) - bytes 20-23
    data[20] = 1;

    // Entry point - bytes 24-31
    let entry: u64 = 0x400000;
    for i in 0..8 {
        data[24 + i] = ((entry >> (i * 8)) & 0xff) as u8;
    }

    // ELF header size - bytes 52-53
    data[52] = 64;
    data[53] = 0;

    // Program header entry size - bytes 54-55
    data[54] = 56;
    data[55] = 0;

    data
}

#[test_case]
fn test_valid_elf_header() {
    let data = create_test_elf_header();
    assert!(elf_validate(&data).is_ok());
}

#[test_case]
fn test_invalid_magic() {
    let mut data = create_test_elf_header();
    data[0] = 0; // Corrupt magic
    assert_eq!(elf_validate(&data), Err(ElfError::InvalidMagic));
}

#[test_case]
fn test_invalid_class() {
    let mut data = create_test_elf_header();
    data[EI_CLASS] = 1; // 32-bit
    assert_eq!(elf_validate(&data), Err(ElfError::InvalidClass));
}

#[test_case]
fn test_invalid_endianness() {
    let mut data = create_test_elf_header();
    data[EI_DATA] = 2; // Big-endian
    assert_eq!(elf_validate(&data), Err(ElfError::InvalidEndianness));
}

#[test_case]
fn test_invalid_machine() {
    let mut data = create_test_elf_header();
    data[18] = 3; // i386 instead of x86_64
    assert_eq!(elf_validate(&data), Err(ElfError::InvalidMachine));
}

#[test_case]
fn test_segment_flags() {
    // Read-only
    let ro_flags = SegmentFlags {
        readable: true,
        writable: false,
        executable: false,
    };
    let page_flags = ro_flags.to_page_flags();
    assert!(page_flags.contains(PageTableFlags::PRESENT));
    assert!(!page_flags.contains(PageTableFlags::WRITABLE));
    assert!(page_flags.contains(PageTableFlags::NO_EXECUTE));

    // Read-write
    let rw_flags = SegmentFlags {
        readable: true,
        writable: true,
        executable: false,
    };
    let page_flags = rw_flags.to_page_flags();
    assert!(page_flags.contains(PageTableFlags::WRITABLE));
    assert!(page_flags.contains(PageTableFlags::NO_EXECUTE));

    // Read-execute
    let rx_flags = SegmentFlags {
        readable: true,
        writable: false,
        executable: true,
    };
    let page_flags = rx_flags.to_page_flags();
    assert!(!page_flags.contains(PageTableFlags::WRITABLE));
    assert!(!page_flags.contains(PageTableFlags::NO_EXECUTE));
}

#[test_case]
fn test_pie_detection() {
    let mut data = create_test_elf_header();

    // Set as PIE (ET_DYN)
    data[16] = (ET_DYN & 0xff) as u8;
    data[17] = ((ET_DYN >> 8) & 0xff) as u8;

    // PIE executables can have zero entry point initially
    for i in 24..32 {
        data[i] = 0;
    }

    assert!(elf_validate(&data).is_ok());
}

#[test_case]
fn test_buffer_too_small() {
    let data = [0u8; 32]; // Less than header size
    assert_eq!(elf_validate(&data), Err(ElfError::BufferTooSmall));
}

/// Create a complete minimal ELF executable with program headers
fn create_minimal_executable() -> Vec<u8> {
    let mut data = Vec::new();

    // ELF header
    let header = create_test_elf_header();
    data.extend_from_slice(&header);

    // Adjust header to include program headers
    // Program header offset at byte 32-39 (after ELF header at 64)
    let ph_offset: u64 = 64;
    for i in 0..8 {
        data[32 + i] = ((ph_offset >> (i * 8)) & 0xff) as u8;
    }

    // Program header count at bytes 56-57 (1 segment)
    data[56] = 1;
    data[57] = 0;

    // Add one LOAD segment (56 bytes)
    let mut ph = vec![0u8; 56];

    // p_type = PT_LOAD
    ph[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());

    // p_flags = PF_R | PF_X
    let flags = PF_R | PF_X;
    ph[4..8].copy_from_slice(&flags.to_le_bytes());

    // p_offset = 0x1000
    ph[8..16].copy_from_slice(&0x1000u64.to_le_bytes());

    // p_vaddr = 0x400000
    ph[16..24].copy_from_slice(&0x400000u64.to_le_bytes());

    // p_paddr = 0x400000
    ph[24..32].copy_from_slice(&0x400000u64.to_le_bytes());

    // p_filesz = 0x1000
    ph[32..40].copy_from_slice(&0x1000u64.to_le_bytes());

    // p_memsz = 0x1000
    ph[40..48].copy_from_slice(&0x1000u64.to_le_bytes());

    // p_align = 0x1000
    ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());

    data.extend_from_slice(&ph);

    // Pad to offset 0x1000
    while data.len() < 0x1000 {
        data.push(0);
    }

    // Add segment data (0x1000 bytes of NOP instructions for simplicity)
    data.extend_from_slice(&[0x90; 0x1000]); // NOP sled

    data
}

#[test_case]
fn test_load_minimal_executable() {
    let binary = create_minimal_executable();

    let result = elf_load(&binary, None);
    assert!(result.is_ok());

    let image = result.unwrap();
    assert_eq!(image.entry_point, VirtAddr::new(0x400000));
    assert_eq!(image.is_pie, false);
    assert_eq!(image.segments.len(), 1);

    let segment = &image.segments[0];
    assert_eq!(segment.vaddr, VirtAddr::new(0x400000));
    assert_eq!(segment.mem_size, 0x1000);
    assert_eq!(segment.file_size, 0x1000);
    assert!(segment.flags.readable);
    assert!(!segment.flags.writable);
    assert!(segment.flags.executable);
}

#[test_case]
fn test_segment_type_names() {
    let load_seg = Elf64ProgramHeader {
        p_type: PT_LOAD,
        p_flags: PF_R,
        p_offset: 0,
        p_vaddr: 0,
        p_paddr: 0,
        p_filesz: 0,
        p_memsz: 0,
        p_align: 0,
    };
    assert_eq!(load_seg.type_name(), "LOAD");

    let dynamic_seg = Elf64ProgramHeader {
        p_type: PT_DYNAMIC,
        p_flags: PF_R,
        p_offset: 0,
        p_vaddr: 0,
        p_paddr: 0,
        p_filesz: 0,
        p_memsz: 0,
        p_align: 0,
    };
    assert_eq!(dynamic_seg.type_name(), "DYNAMIC");
}
