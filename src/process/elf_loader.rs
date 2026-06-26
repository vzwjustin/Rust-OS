//! ELF64 Binary Loader for RustOS
//!
//! This module provides production-ready ELF64 executable loading with:
//! - Complete ELF64 parsing and validation
//! - ASLR (Address Space Layout Randomization)
//! - NX bit enforcement (No-Execute protection)
//! - W^X enforcement (Write XOR Execute)
//! - Stack guard pages
//! - Robust error handling

use crate::memory::{
    align_up, allocate_memory, allocate_memory_with_guards, protect_memory, translate_addr,
    MemoryError, MemoryProtection, MemoryRegionType, VirtualMemoryRegion, PAGE_SIZE,
    USER_SPACE_END, USER_SPACE_START,
};
use crate::process::Pid;
use alloc::vec::Vec;
use core::fmt;
use x86_64::{PhysAddr, VirtAddr};

/// ELF64 Header (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub e_ident: [u8; 16], // Magic number and other info
    pub e_type: u16,       // Object file type
    pub e_machine: u16,    // Architecture
    pub e_version: u32,    // Object file version
    pub e_entry: u64,      // Entry point virtual address
    pub e_phoff: u64,      // Program header table file offset
    pub e_shoff: u64,      // Section header table file offset
    pub e_flags: u32,      // Processor-specific flags
    pub e_ehsize: u16,     // ELF header size
    pub e_phentsize: u16,  // Program header table entry size
    pub e_phnum: u16,      // Program header table entry count
    pub e_shentsize: u16,  // Section header table entry size
    pub e_shnum: u16,      // Section header table entry count
    pub e_shstrndx: u16,   // Section header string table index
}

/// ELF64 Program Header (56 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,   // Segment type
    pub p_flags: u32,  // Segment flags
    pub p_offset: u64, // Segment file offset
    pub p_vaddr: u64,  // Segment virtual address
    pub p_paddr: u64,  // Segment physical address
    pub p_filesz: u64, // Segment size in file
    pub p_memsz: u64,  // Segment size in memory
    pub p_align: u64,  // Segment alignment
}

/// ELF Constants
pub mod elf_constants {
    // ELF Magic Number
    pub const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46]; // "\x7fELF"

    // ELF Classes
    pub const ELFCLASS64: u8 = 2;

    // Data Encoding
    pub const ELFDATA2LSB: u8 = 1; // Little-endian

    // ELF Version
    pub const EV_CURRENT: u8 = 1;

    // Object File Types
    pub const ET_EXEC: u16 = 2; // Executable file
    pub const ET_DYN: u16 = 3; // Shared object file (PIE)

    // Machine Types
    pub const EM_X86_64: u16 = 62; // AMD x86-64 architecture

    // Program Header Types
    pub const PT_NULL: u32 = 0; // Unused entry
    pub const PT_LOAD: u32 = 1; // Loadable segment
    pub const PT_DYNAMIC: u32 = 2; // Dynamic linking information
    pub const PT_INTERP: u32 = 3; // Interpreter pathname
    pub const PT_NOTE: u32 = 4; // Auxiliary information
    pub const PT_PHDR: u32 = 6; // Program header table
    pub const PT_TLS: u32 = 7; // Thread-Local Storage
    pub const PT_GNU_STACK: u32 = 0x6474e551; // Stack permissions

    // Segment Flags
    pub const PF_X: u32 = 0x1; // Execute
    pub const PF_W: u32 = 0x2; // Write
    pub const PF_R: u32 = 0x4; // Read

    // ELF Identification Indices
    pub const EI_CLASS: usize = 4;
    pub const EI_DATA: usize = 5;
    pub const EI_VERSION: usize = 6;
}

/// ELF Loader Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfLoaderError {
    InvalidMagic,
    UnsupportedClass,
    UnsupportedEndianness,
    UnsupportedVersion,
    UnsupportedArchitecture,
    InvalidFileType,
    InvalidHeaderSize,
    ProgramHeaderOutOfBounds,
    TooManyProgramHeaders,
    InvalidSegmentOffset,
    InvalidSegmentSize,
    SegmentOverlap,
    InvalidAlignment,
    InvalidVirtualAddress,
    InvalidPermissions,
    MemoryAllocationFailed,
    MappingFailed,
    InvalidEntryPoint,
    FileTooLarge,
    CorruptedBinary,
    FileTooSmall,
}

impl fmt::Display for ElfLoaderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ElfLoaderError::InvalidMagic => write!(f, "Invalid ELF magic number"),
            ElfLoaderError::UnsupportedClass => write!(f, "Unsupported ELF class (not ELF64)"),
            ElfLoaderError::UnsupportedEndianness => write!(f, "Unsupported endianness"),
            ElfLoaderError::UnsupportedVersion => write!(f, "Unsupported ELF version"),
            ElfLoaderError::UnsupportedArchitecture => {
                write!(f, "Unsupported architecture (not x86_64)")
            }
            ElfLoaderError::InvalidFileType => write!(f, "Invalid ELF file type"),
            ElfLoaderError::InvalidHeaderSize => write!(f, "Invalid ELF header size"),
            ElfLoaderError::ProgramHeaderOutOfBounds => write!(f, "Program headers out of bounds"),
            ElfLoaderError::TooManyProgramHeaders => write!(f, "Too many program headers"),
            ElfLoaderError::InvalidSegmentOffset => write!(f, "Invalid segment file offset"),
            ElfLoaderError::InvalidSegmentSize => write!(f, "Invalid segment size"),
            ElfLoaderError::SegmentOverlap => write!(f, "Segment overlap detected"),
            ElfLoaderError::InvalidAlignment => write!(f, "Invalid segment alignment"),
            ElfLoaderError::InvalidVirtualAddress => write!(f, "Invalid virtual address"),
            ElfLoaderError::InvalidPermissions => write!(f, "Invalid segment permissions (W+X)"),
            ElfLoaderError::MemoryAllocationFailed => write!(f, "Memory allocation failed"),
            ElfLoaderError::MappingFailed => write!(f, "Memory mapping failed"),
            ElfLoaderError::InvalidEntryPoint => write!(f, "Invalid entry point address"),
            ElfLoaderError::FileTooLarge => write!(f, "File too large"),
            ElfLoaderError::CorruptedBinary => write!(f, "Corrupted binary data"),
            ElfLoaderError::FileTooSmall => write!(f, "File too small"),
        }
    }
}

/// Loaded binary information
#[derive(Debug, Clone)]
pub struct LoadedBinary {
    pub base_address: VirtAddr,
    pub entry_point: VirtAddr,
    pub heap_start: VirtAddr,
    pub stack_top: VirtAddr,
    pub code_regions: Vec<VirtualMemoryRegion>,
    pub data_regions: Vec<VirtualMemoryRegion>,
    /// Whether this binary requires dynamic linking
    pub is_dynamic: bool,
    /// Program headers (needed for dynamic linking)
    pub program_headers: Vec<Elf64ProgramHeader>,
}

/// ELF64 Binary Loader
pub struct ElfLoader {
    pub enable_aslr: bool,
    pub enable_nx: bool,
    pub enforce_wx: bool, // W^X enforcement
}

impl ElfLoader {
    /// Create a new ELF loader with security settings
    pub fn new(enable_aslr: bool, enable_nx: bool) -> Self {
        Self {
            enable_aslr,
            enable_nx,
            enforce_wx: true, // Always enforce W^X for security
        }
    }

    /// Parse ELF header from binary data
    fn parse_elf_header(&self, data: &[u8]) -> Result<Elf64Header, ElfLoaderError> {
        if data.len() < core::mem::size_of::<Elf64Header>() {
            return Err(ElfLoaderError::FileTooSmall);
        }

        // Safe to read header
        let header = unsafe { core::ptr::read(data.as_ptr() as *const Elf64Header) };

        Ok(header)
    }

    /// Validate ELF header
    fn validate_elf_header(&self, header: &Elf64Header) -> Result<(), ElfLoaderError> {
        // Check magic number
        if &header.e_ident[0..4] != elf_constants::ELF_MAGIC {
            return Err(ElfLoaderError::InvalidMagic);
        }

        // Check ELF class (must be 64-bit)
        if header.e_ident[elf_constants::EI_CLASS] != elf_constants::ELFCLASS64 {
            return Err(ElfLoaderError::UnsupportedClass);
        }

        // Check endianness (must be little-endian)
        if header.e_ident[elf_constants::EI_DATA] != elf_constants::ELFDATA2LSB {
            return Err(ElfLoaderError::UnsupportedEndianness);
        }

        // Check version
        if header.e_ident[elf_constants::EI_VERSION] != elf_constants::EV_CURRENT {
            return Err(ElfLoaderError::UnsupportedVersion);
        }

        if header.e_version != elf_constants::EV_CURRENT as u32 {
            return Err(ElfLoaderError::UnsupportedVersion);
        }

        // Check architecture (must be x86_64)
        if header.e_machine != elf_constants::EM_X86_64 {
            return Err(ElfLoaderError::UnsupportedArchitecture);
        }

        // Check file type (executable or PIE)
        if header.e_type != elf_constants::ET_EXEC && header.e_type != elf_constants::ET_DYN {
            return Err(ElfLoaderError::InvalidFileType);
        }

        // Check header size
        if header.e_ehsize != core::mem::size_of::<Elf64Header>() as u16 {
            return Err(ElfLoaderError::InvalidHeaderSize);
        }

        Ok(())
    }

    /// Parse program headers
    fn parse_program_headers(
        &self,
        data: &[u8],
        header: &Elf64Header,
    ) -> Result<Vec<Elf64ProgramHeader>, ElfLoaderError> {
        let phdr_size = core::mem::size_of::<Elf64ProgramHeader>();

        // Validate program header entry size
        if header.e_phentsize != phdr_size as u16 {
            return Err(ElfLoaderError::InvalidHeaderSize);
        }

        // Validate program header count
        if header.e_phnum > 100 {
            return Err(ElfLoaderError::TooManyProgramHeaders);
        }

        // Validate program headers are within file
        let phdr_table_size = header.e_phnum as usize * phdr_size;
        let phdr_end = header.e_phoff as usize + phdr_table_size;

        if phdr_end > data.len() {
            return Err(ElfLoaderError::ProgramHeaderOutOfBounds);
        }

        // Parse program headers
        let mut program_headers = Vec::with_capacity(header.e_phnum as usize);

        for i in 0..header.e_phnum {
            let offset = header.e_phoff as usize + (i as usize * phdr_size);
            let phdr = unsafe {
                core::ptr::read((data.as_ptr().add(offset)) as *const Elf64ProgramHeader)
            };
            program_headers.push(phdr);
        }

        Ok(program_headers)
    }

    /// Validate program headers
    fn validate_program_headers(
        &self,
        phdrs: &[Elf64ProgramHeader],
        file_size: usize,
    ) -> Result<(), ElfLoaderError> {
        for phdr in phdrs.iter() {
            // Only validate LOAD segments
            if phdr.p_type != elf_constants::PT_LOAD {
                continue;
            }

            // Validate file size
            let file_end = phdr
                .p_offset
                .checked_add(phdr.p_filesz)
                .ok_or(ElfLoaderError::InvalidSegmentOffset)?;

            if file_end > file_size as u64 {
                return Err(ElfLoaderError::InvalidSegmentOffset);
            }

            // Validate memory size
            if phdr.p_memsz < phdr.p_filesz {
                return Err(ElfLoaderError::InvalidSegmentSize);
            }

            // Validate virtual address is in user space
            if phdr.p_vaddr < USER_SPACE_START as u64 || phdr.p_vaddr >= USER_SPACE_END as u64 {
                return Err(ElfLoaderError::InvalidVirtualAddress);
            }

            // Validate alignment
            if phdr.p_align > 0 && phdr.p_align != 1 {
                if !phdr.p_align.is_power_of_two() {
                    return Err(ElfLoaderError::InvalidAlignment);
                }
                if phdr.p_vaddr % phdr.p_align != phdr.p_offset % phdr.p_align {
                    return Err(ElfLoaderError::InvalidAlignment);
                }
            }

            // Validate permissions (W^X enforcement)
            if self.enforce_wx {
                let writable = phdr.p_flags & elf_constants::PF_W != 0;
                let executable = phdr.p_flags & elf_constants::PF_X != 0;

                if writable && executable {
                    return Err(ElfLoaderError::InvalidPermissions);
                }
            }
        }

        Ok(())
    }

    /// Convert ELF flags to memory protection
    fn flags_to_protection(&self, flags: u32) -> MemoryProtection {
        let mut protection = MemoryProtection::empty();

        if flags & elf_constants::PF_R != 0 {
            protection.readable = true;
        }
        if flags & elf_constants::PF_W != 0 {
            protection.writable = true;
        }
        if self.enable_nx {
            protection.executable = flags & elf_constants::PF_X != 0;
        } else {
            protection.executable = true;
        }

        protection.user_accessible = true;
        protection
    }

    /// Calculate base address with optional ASLR
    fn calculate_base_address(&self, elf_type: u16) -> VirtAddr {
        let base = VirtAddr::new(USER_SPACE_START as u64);

        if self.enable_aslr {
            // Use ASLR offset from memory module
            let offset = crate::memory::generate_aslr_offset();
            VirtAddr::new(base.as_u64() + offset)
        } else {
            base
        }
    }

    /// Load a single segment from ELF file
    fn load_segment(
        &self,
        phdr: &Elf64ProgramHeader,
        binary_data: &[u8],
        base_address: VirtAddr,
    ) -> Result<VirtualMemoryRegion, ElfLoaderError> {
        // Calculate load address (base + virtual address)
        let load_vaddr = VirtAddr::new(base_address.as_u64() + phdr.p_vaddr);

        // Determine protection and region type
        let protection = self.flags_to_protection(phdr.p_flags);
        let region_type = if protection.executable {
            MemoryRegionType::UserCode
        } else {
            MemoryRegionType::UserData
        };

        // Allocate memory for segment
        let aligned_size = align_up(phdr.p_memsz as usize, PAGE_SIZE);
        let region_start = allocate_memory(aligned_size, region_type, protection)
            .map_err(|_| ElfLoaderError::MemoryAllocationFailed)?;

        // Copy file data to memory if present
        if phdr.p_filesz > 0 {
            let file_offset = phdr.p_offset as usize;
            let file_size = phdr.p_filesz as usize;

            if file_offset + file_size > binary_data.len() {
                return Err(ElfLoaderError::InvalidSegmentOffset);
            }

            // Get physical address for copying
            if let Some(phys_addr) = translate_addr(region_start) {
                unsafe {
                    let dest_ptr = phys_addr.as_u64() as *mut u8;
                    let src_ptr = binary_data[file_offset..file_offset + file_size].as_ptr();
                    core::ptr::copy_nonoverlapping(src_ptr, dest_ptr, file_size);
                }
            } else {
                return Err(ElfLoaderError::MappingFailed);
            }
        }

        // Zero BSS region (memsz > filesz)
        if phdr.p_memsz > phdr.p_filesz {
            let bss_offset = phdr.p_filesz as usize;
            let bss_size = (phdr.p_memsz - phdr.p_filesz) as usize;

            let bss_start = VirtAddr::new(region_start.as_u64() + bss_offset as u64);
            if let Some(phys_addr) = translate_addr(bss_start) {
                unsafe {
                    let bss_ptr = phys_addr.as_u64() as *mut u8;
                    core::ptr::write_bytes(bss_ptr, 0, bss_size);
                }
            }
        }

        Ok(VirtualMemoryRegion {
            start: region_start,
            size: aligned_size,
            region_type,
            protection,
            mapped: true,
            physical_start: translate_addr(region_start).map(|p| p),
            reference_count: 1,
            aslr_offset: base_address.as_u64(),
        })
    }

    /// Load ELF binary into memory
    pub fn load_elf_binary(
        &self,
        binary_data: &[u8],
        _process_id: Pid,
    ) -> Result<LoadedBinary, ElfLoaderError> {
        // Parse and validate header
        let elf_header = self.parse_elf_header(binary_data)?;
        self.validate_elf_header(&elf_header)?;

        // Parse and validate program headers
        let program_headers = self.parse_program_headers(binary_data, &elf_header)?;
        self.validate_program_headers(&program_headers, binary_data.len())?;

        // Calculate base address with ASLR
        let base_address = self.calculate_base_address(elf_header.e_type);

        // Load all PT_LOAD segments
        let mut code_regions = Vec::new();
        let mut data_regions = Vec::new();

        for phdr in program_headers.iter() {
            if phdr.p_type != elf_constants::PT_LOAD {
                continue;
            }

            let region = self.load_segment(phdr, binary_data, base_address)?;

            if region.protection.executable {
                code_regions.push(region);
            } else {
                data_regions.push(region);
            }
        }

        // Find highest address for heap placement
        let mut max_addr = base_address.as_u64();
        for region in code_regions.iter().chain(data_regions.iter()) {
            let region_end = region.start.as_u64() + region.size as u64;
            if region_end > max_addr {
                max_addr = region_end;
            }
        }

        // Allocate heap (8KB initial size, after loaded segments)
        let heap_size = 8 * 1024;
        let heap_start = allocate_memory(
            heap_size,
            MemoryRegionType::UserHeap,
            MemoryProtection::USER_DATA,
        )
        .map_err(|_| ElfLoaderError::MemoryAllocationFailed)?;

        // Allocate stack with guard pages (8MB)
        let stack_size = 8 * 1024 * 1024;
        let stack_bottom = allocate_memory_with_guards(
            stack_size,
            MemoryRegionType::UserStack,
            MemoryProtection::USER_DATA,
        )
        .map_err(|_| ElfLoaderError::MemoryAllocationFailed)?;

        let stack_top = VirtAddr::new(stack_bottom.as_u64() + stack_size as u64);

        // Calculate entry point
        let entry_point = VirtAddr::new(base_address.as_u64() + elf_header.e_entry);

        // Validate entry point is within code region
        let entry_in_code = code_regions.iter().any(|r| r.contains(entry_point));
        if !entry_in_code && !code_regions.is_empty() {
            return Err(ElfLoaderError::InvalidEntryPoint);
        }

        // Check if binary requires dynamic linking
        let is_dynamic = program_headers.iter().any(|phdr| {
            phdr.p_type == elf_constants::PT_DYNAMIC || phdr.p_type == elf_constants::PT_INTERP
        });

        Ok(LoadedBinary {
            base_address,
            entry_point,
            heap_start,
            stack_top,
            code_regions,
            data_regions,
            is_dynamic,
            program_headers,
        })
    }
}

// Expose generate_aslr_offset from memory module
pub use crate::memory::generate_aslr_offset;
