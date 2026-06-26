//! ELF64 Type Definitions
//!
//! Low-level ELF format structures and constants for x86_64 architecture.

use core::mem;

/// ELF magic number bytes
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// ELF class identifiers
pub const ELFCLASS64: u8 = 2;

/// ELF data encoding
pub const ELFDATA2LSB: u8 = 1; // Little-endian

/// ELF version
pub const EV_CURRENT: u8 = 1;

/// ELF file types
pub const ET_EXEC: u16 = 2; // Executable file
pub const ET_DYN: u16 = 3; // Shared object (PIE)

/// Machine architectures
pub const EM_X86_64: u16 = 62; // AMD x86-64

/// Program header types
pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
pub const PT_GNU_EH_FRAME: u32 = 0x6474e550;
pub const PT_GNU_STACK: u32 = 0x6474e551;
pub const PT_GNU_RELRO: u32 = 0x6474e552;

/// Program header flags
pub const PF_X: u32 = 1; // Execute
pub const PF_W: u32 = 2; // Write
pub const PF_R: u32 = 4; // Read

/// ELF identification indexes
pub const EI_MAG0: usize = 0;
pub const EI_MAG1: usize = 1;
pub const EI_MAG2: usize = 2;
pub const EI_MAG3: usize = 3;
pub const EI_CLASS: usize = 4;
pub const EI_DATA: usize = 5;
pub const EI_VERSION: usize = 6;
pub const EI_OSABI: usize = 7;
pub const EI_ABIVERSION: usize = 8;
pub const EI_PAD: usize = 9;
pub const EI_NIDENT: usize = 16;

/// ELF64 Header (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    /// ELF identification
    pub e_ident: [u8; EI_NIDENT],
    /// Object file type (ET_EXEC, ET_DYN, etc.)
    pub e_type: u16,
    /// Machine architecture (EM_X86_64)
    pub e_machine: u16,
    /// Object file version
    pub e_version: u32,
    /// Entry point virtual address
    pub e_entry: u64,
    /// Program header table file offset
    pub e_phoff: u64,
    /// Section header table file offset
    pub e_shoff: u64,
    /// Processor-specific flags
    pub e_flags: u32,
    /// ELF header size in bytes
    pub e_ehsize: u16,
    /// Program header table entry size
    pub e_phentsize: u16,
    /// Program header table entry count
    pub e_phnum: u16,
    /// Section header table entry size
    pub e_shentsize: u16,
    /// Section header table entry count
    pub e_shnum: u16,
    /// Section header string table index
    pub e_shstrndx: u16,
}

/// ELF64 Program Header (56 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    /// Segment type (PT_LOAD, PT_DYNAMIC, etc.)
    pub p_type: u32,
    /// Segment flags (PF_R, PF_W, PF_X)
    pub p_flags: u32,
    /// Segment file offset
    pub p_offset: u64,
    /// Segment virtual address
    pub p_vaddr: u64,
    /// Segment physical address (usually ignored)
    pub p_paddr: u64,
    /// Segment size in file
    pub p_filesz: u64,
    /// Segment size in memory
    pub p_memsz: u64,
    /// Segment alignment (must be power of 2)
    pub p_align: u64,
}

impl Elf64Header {
    /// Size of ELF64 header
    pub const SIZE: usize = mem::size_of::<Self>();

    /// Parse ELF64 header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        // Safety: size checked above. `data` is a byte slice that may not be aligned
        // to Self's alignment, so we read the struct by value with read_unaligned
        // instead of forming a (potentially misaligned) &Self reference (which is UB).
        Some(unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Self) })
    }

    /// Validate ELF magic number
    pub fn validate_magic(&self) -> bool {
        self.e_ident[EI_MAG0..=EI_MAG3] == ELF_MAGIC
    }

    /// Check if 64-bit ELF
    pub fn is_64bit(&self) -> bool {
        self.e_ident[EI_CLASS] == ELFCLASS64
    }

    /// Check if little-endian
    pub fn is_little_endian(&self) -> bool {
        self.e_ident[EI_DATA] == ELFDATA2LSB
    }

    /// Check if current version
    pub fn is_current_version(&self) -> bool {
        self.e_ident[EI_VERSION] == EV_CURRENT && self.e_version == 1
    }

    /// Check if executable
    pub fn is_executable(&self) -> bool {
        self.e_type == ET_EXEC
    }

    /// Check if position-independent executable
    pub fn is_pie(&self) -> bool {
        self.e_type == ET_DYN
    }

    /// Check if x86_64 architecture
    pub fn is_x86_64(&self) -> bool {
        self.e_machine == EM_X86_64
    }

    /// Get program header table offset
    pub fn program_header_offset(&self) -> usize {
        self.e_phoff as usize
    }

    /// Get program header count
    pub fn program_header_count(&self) -> usize {
        self.e_phnum as usize
    }

    /// Get program header entry size
    pub fn program_header_entry_size(&self) -> usize {
        self.e_phentsize as usize
    }
}

impl Elf64ProgramHeader {
    /// Size of program header
    pub const SIZE: usize = mem::size_of::<Self>();

    /// Parse program header from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        // Safety: size checked above. Program headers sit at an arbitrary e_phoff and
        // are therefore generally unaligned, so we read the struct by value with
        // read_unaligned instead of forming a misaligned &Self reference (which is UB).
        Some(unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Self) })
    }

    /// Check if this is a loadable segment
    pub fn is_loadable(&self) -> bool {
        self.p_type == PT_LOAD
    }

    /// Check if segment is readable
    pub fn is_readable(&self) -> bool {
        (self.p_flags & PF_R) != 0
    }

    /// Check if segment is writable
    pub fn is_writable(&self) -> bool {
        (self.p_flags & PF_W) != 0
    }

    /// Check if segment is executable
    pub fn is_executable(&self) -> bool {
        (self.p_flags & PF_X) != 0
    }

    /// Get virtual address
    pub fn vaddr(&self) -> u64 {
        self.p_vaddr
    }

    /// Get file offset
    pub fn offset(&self) -> usize {
        self.p_offset as usize
    }

    /// Get file size
    pub fn file_size(&self) -> usize {
        self.p_filesz as usize
    }

    /// Get memory size
    pub fn mem_size(&self) -> usize {
        self.p_memsz as usize
    }

    /// Get alignment requirement
    pub fn alignment(&self) -> usize {
        self.p_align as usize
    }

    /// Get segment type name
    pub fn type_name(&self) -> &'static str {
        match self.p_type {
            PT_NULL => "NULL",
            PT_LOAD => "LOAD",
            PT_DYNAMIC => "DYNAMIC",
            PT_INTERP => "INTERP",
            PT_NOTE => "NOTE",
            PT_SHLIB => "SHLIB",
            PT_PHDR => "PHDR",
            PT_TLS => "TLS",
            PT_GNU_EH_FRAME => "GNU_EH_FRAME",
            PT_GNU_STACK => "GNU_STACK",
            PT_GNU_RELRO => "GNU_RELRO",
            _ => "UNKNOWN",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_header_sizes() {
        assert_eq!(Elf64Header::SIZE, 64);
        assert_eq!(Elf64ProgramHeader::SIZE, 56);
    }

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_magic_validation() {
        let mut header = Elf64Header {
            e_ident: [0; EI_NIDENT],
            e_type: ET_EXEC,
            e_machine: EM_X86_64,
            e_version: 1,
            e_entry: 0,
            e_phoff: 0,
            e_shoff: 0,
            e_flags: 0,
            e_ehsize: 64,
            e_phentsize: 56,
            e_phnum: 0,
            e_shentsize: 0,
            e_shnum: 0,
            e_shstrndx: 0,
        };

        // Invalid magic
        assert!(!header.validate_magic());

        // Valid magic
        header.e_ident[0..4].copy_from_slice(&ELF_MAGIC);
        assert!(header.validate_magic());
    }
}
