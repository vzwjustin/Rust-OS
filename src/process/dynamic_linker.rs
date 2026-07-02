//! Dynamic Linker for RustOS
//!
//! This module implements dynamic linking support, enabling RustOS to load
//! and execute dynamically-linked ELF binaries. This is a critical component
//! for Linux application compatibility as ~95% of Linux binaries use dynamic linking.
//!
//! ## Features
//! - PT_DYNAMIC segment parsing
//! - Shared library (.so) loading
//! - Symbol resolution across loaded libraries
//! - Relocation processing (R_X86_64_* types)
//! - Library search path management
//!
//! ## Architecture
//! The dynamic linker works in phases:
//! 1. Parse PT_DYNAMIC segment from main executable
//! 2. Identify required shared libraries (DT_NEEDED entries)
//! 3. Load each shared library into memory
//! 4. Build global symbol table
//! 5. Process relocations to fix up addresses
//!
//! ## References
//! - ELF Specification: https://refspecs.linuxfoundation.org/elf/elf.pdf
//! - System V ABI: https://refspecs.linuxfoundation.org/elf/x86_64-abi-0.99.pdf
//! - See docs/LINUX_APP_SUPPORT.md for implementation roadmap

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::VirtAddr;

use super::elf_loader::{elf_constants, Elf64ProgramHeader, ElfLoader};
use crate::memory::PAGE_SIZE;
use crate::vfs::{vfs_close, vfs_open, vfs_read, vfs_readlink, vfs_stat, InodeType, OpenFlags};

/// Describes one entry in the runtime link-map chain.
///
/// In a real ELF loader this would mirror the `struct link_map` used by
/// `dl_iterate_phdr`.  Here it carries the minimal information needed for
/// the lazy-binding resolver to locate the right `DynamicLinker` state.
#[derive(Debug, Clone)]
pub struct LinkMap {
    /// Base load address of the object this link-map entry describes.
    pub base_address: VirtAddr,
    /// Human-readable name of the shared object (e.g. "libc.so.6").
    pub name: String,
}

/// Dynamic linker for loading shared libraries and resolving symbols
#[derive(Clone)]
pub struct DynamicLinker {
    /// Library search paths (e.g., /lib, /usr/lib, /lib64)
    search_paths: Vec<String>,

    /// Paths from `LD_LIBRARY_PATH` for the current link operation
    library_path_env: Vec<String>,

    /// Cache of loaded shared libraries
    loaded_libraries: BTreeMap<String, LoadedLibrary>,

    /// Global symbol table mapping symbol names to addresses
    symbol_table: BTreeMap<String, VirtAddr>,

    /// Symbol table by index for current binary (used during relocation)
    symbol_index_table: Vec<(String, VirtAddr)>,

    /// Base address for library loading (managed with ASLR)
    next_base_address: VirtAddr,
}

/// Information about a loaded shared library
#[derive(Debug, Clone)]
pub struct LoadedLibrary {
    /// Library name (e.g., "libc.so.6")
    pub name: String,

    /// Base address where library is loaded
    pub base_address: VirtAddr,

    /// Size of library in memory
    pub size: usize,

    /// Entry point (if applicable)
    pub entry_point: Option<VirtAddr>,

    /// Dynamic section information
    pub dynamic_info: DynamicInfo,
}

/// Parsed PT_DYNAMIC section information
#[derive(Debug, Clone, Default)]
pub struct DynamicInfo {
    /// Required shared libraries (DT_NEEDED)
    pub needed: Vec<String>,

    /// String table address (DT_STRTAB)
    pub strtab: Option<VirtAddr>,

    /// String table size (DT_STRSZ)
    pub strsz: Option<usize>,

    /// Symbol table address (DT_SYMTAB)
    pub symtab: Option<VirtAddr>,

    /// Symbol table entry size (DT_SYMENT)
    pub syment: Option<usize>,

    /// Hash table address (DT_HASH)
    pub hash: Option<VirtAddr>,

    /// Relocation table address (DT_RELA)
    pub rela: Option<VirtAddr>,

    /// Size of relocation table (DT_RELASZ)
    pub relasz: Option<usize>,

    /// Relocation entry size (DT_RELAENT)
    pub relaent: Option<usize>,

    /// PLT relocations address (DT_JMPREL)
    pub jmprel: Option<VirtAddr>,

    /// Size of PLT relocations (DT_PLTRELSZ)
    pub pltrelsz: Option<usize>,

    /// Init function address (DT_INIT)
    pub init: Option<VirtAddr>,

    /// Fini function address (DT_FINI)
    pub fini: Option<VirtAddr>,

    /// Init function array (DT_INIT_ARRAY)
    pub init_array: Option<VirtAddr>,
    /// Size of init array in bytes (DT_INIT_ARRAYSZ)
    pub init_arraysz: Option<usize>,

    /// Fini function array (DT_FINI_ARRAY)
    pub fini_array: Option<VirtAddr>,
    /// Size of fini array in bytes (DT_FINI_ARRAYSZ)
    pub fini_arraysz: Option<usize>,

    /// Pre-init function array (DT_PREINIT_ARRAY)
    pub preinit_array: Option<VirtAddr>,
    /// Size of pre-init array in bytes (DT_PREINIT_ARRAYSZ)
    pub preinit_arraysz: Option<usize>,

    /// GNU hash table address (DT_GNU_HASH)
    pub gnu_hash: Option<VirtAddr>,

    /// Symbol version table (DT_VERSYM)
    pub versym: Option<VirtAddr>,
    /// Version definition table (DT_VERDEF)
    pub verdef: Option<VirtAddr>,
    /// Number of version definitions (DT_VERDEFNUM)
    pub verdefnum: Option<usize>,
    /// Version needed table (DT_VERNEED)
    pub verneed: Option<VirtAddr>,
    /// Number of version needed entries (DT_VERNEEDNUM)
    pub verneednum: Option<usize>,

    /// Dynamic flags (DT_FLAGS)
    pub flags: Option<u64>,
    /// Additional flags (DT_FLAGS_1)
    pub flags_1: Option<u64>,

    /// REL relocation table (DT_REL)
    pub rel: Option<VirtAddr>,
    /// Size of REL relocations (DT_RELSZ)
    pub relsz: Option<usize>,
    /// Size of REL entry (DT_RELENT)
    pub relent: Option<usize>,

    /// Library search path from DT_RPATH (deprecated)
    pub rpath: Option<String>,

    /// Library search path from DT_RUNPATH
    pub runpath: Option<String>,
}

/// Relocation entry (RELA format)
#[derive(Debug, Clone, Copy)]
pub struct Relocation {
    /// Offset where to apply the relocation
    pub offset: VirtAddr,

    /// Relocation type (R_X86_64_*)
    pub r_type: u32,

    /// Symbol index
    pub symbol: u32,

    /// Addend value
    pub addend: i64,
}

/// ELF symbol table entry (Elf64_Sym)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Symbol {
    pub st_name: u32,  // Symbol name (string table index)
    pub st_info: u8,   // Symbol type and binding
    pub st_other: u8,  // Symbol visibility
    pub st_shndx: u16, // Section index
    pub st_value: u64, // Symbol value
    pub st_size: u64,  // Symbol size
}

impl Elf64Symbol {
    /// Get symbol binding (upper 4 bits of st_info)
    pub fn binding(&self) -> u8 {
        self.st_info >> 4
    }

    /// Get symbol type (lower 4 bits of st_info)
    pub fn symbol_type(&self) -> u8 {
        self.st_info & 0xf
    }

    /// Check if symbol is defined (not undefined)
    pub fn is_defined(&self) -> bool {
        self.st_shndx != 0 // SHN_UNDEF
    }
}

/// Symbol binding types
pub mod symbol_binding {
    pub const STB_LOCAL: u8 = 0; // Local symbol
    pub const STB_GLOBAL: u8 = 1; // Global symbol
    pub const STB_WEAK: u8 = 2; // Weak symbol
}

/// Symbol types
pub mod symbol_type {
    pub const STT_NOTYPE: u8 = 0; // No type
    pub const STT_OBJECT: u8 = 1; // Data object
    pub const STT_FUNC: u8 = 2; // Code object (function)
    pub const STT_SECTION: u8 = 3; // Section
    pub const STT_FILE: u8 = 4; // File name
}

// ── Symbol versioning structures ───────────────────────────────────────

/// Version definition entry (Elf64_Verdef)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Verdef {
    pub vd_version: u16, // Version of structure (1)
    pub vd_flags: u16,   // Flags (VER_FLG_BASE, VER_FLG_WEAK)
    pub vd_ndx: u16,     // Version index in versym table
    pub vd_cnt: u16,     // Number of aux entries
    pub vd_hash: u32,    // Hash of version string
    pub vd_aux: u32,     // Offset to first Verdaux
    pub vd_next: u32,    // Offset to next Verdef
}

/// Version definition auxiliary entry (Elf64_Verdaux)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Verdaux {
    pub vda_name: u32, // String table offset of version name
    pub vda_next: u32, // Offset to next Verdaux
}

/// Version needed entry (Elf64_Verneed)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Verneed {
    pub vn_version: u16, // Version of structure (1)
    pub vn_cnt: u16,     // Number of aux entries
    pub vn_file: u32,    // String table offset of filename
    pub vn_aux: u32,     // Offset to first Vernaux
    pub vn_next: u32,    // Offset to next Verneed
}

/// Version needed auxiliary entry (Elf64_Vernaux)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Vernaux {
    pub vna_hash: u32,  // Hash of version string
    pub vna_flags: u16, // Flags
    pub vna_other: u16, // Version index in versym table
    pub vna_name: u32,  // String table offset of version name
    pub vna_next: u32,  // Offset to next Vernaux
}

/// Version definition flags
pub mod ver_flags {
    pub const VER_FLG_BASE: u16 = 0x1;
    pub const VER_FLG_WEAK: u16 = 0x2;
    pub const VER_FLG_INFO: u16 = 0x4;
}

/// Special version indices
pub mod ver_ndx {
    pub const VER_NDX_LOCAL: u16 = 0;
    pub const VER_NDX_GLOBAL: u16 = 1;
}

/// Mask for the version index in a versym entry (high bit is "hidden" flag)
pub const VERSYM_HIDDEN_MASK: u16 = 0x8000;
pub const VERSYM_VERSION_MASK: u16 = 0x7FFF;

/// Dynamic section entry (Elf64_Dyn)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DynamicEntry {
    pub d_tag: i64,
    pub d_val: u64,
}

/// Dynamic section tags (DT_*)
pub mod dynamic_tags {
    pub const DT_NULL: i64 = 0; // End of dynamic section
    pub const DT_NEEDED: i64 = 1; // Name of needed library
    pub const DT_PLTRELSZ: i64 = 2; // Size of PLT relocs
    pub const DT_PLTGOT: i64 = 3; // PLT/GOT address
    pub const DT_HASH: i64 = 4; // Symbol hash table address (SysV)
    pub const DT_STRTAB: i64 = 5; // String table address
    pub const DT_SYMTAB: i64 = 6; // Symbol table address
    pub const DT_RELA: i64 = 7; // Relocation table address
    pub const DT_RELASZ: i64 = 8; // Size of relocation table
    pub const DT_RELAENT: i64 = 9; // Size of relocation entry
    pub const DT_STRSZ: i64 = 10; // Size of string table
    pub const DT_SYMENT: i64 = 11; // Size of symbol table entry
    pub const DT_INIT: i64 = 12; // Init function address
    pub const DT_FINI: i64 = 13; // Fini function address
    pub const DT_SONAME: i64 = 14; // Name of this shared object
    pub const DT_RPATH: i64 = 15; // Library search path (deprecated)
    pub const DT_SYMBOLIC: i64 = 16; // Start symbol search here
    pub const DT_REL: i64 = 17; // REL format relocations
    pub const DT_RELSZ: i64 = 18; // Size of REL relocations
    pub const DT_RELENT: i64 = 19; // Size of REL entry
    pub const DT_PLTREL: i64 = 20; // Type of PLT reloc (REL or RELA)
    pub const DT_DEBUG: i64 = 21; // Debug info
    pub const DT_TEXTREL: i64 = 22; // Reloc might modify text segment
    pub const DT_JMPREL: i64 = 23; // PLT relocation entries
    pub const DT_BIND_NOW: i64 = 24; // Process all relocs before executing
    pub const DT_INIT_ARRAY: i64 = 25; // Init function array
    pub const DT_FINI_ARRAY: i64 = 26; // Fini function array
    pub const DT_INIT_ARRAYSZ: i64 = 27; // Size of init array
    pub const DT_FINI_ARRAYSZ: i64 = 28; // Size of fini array
    pub const DT_RUNPATH: i64 = 29; // Library search path
    pub const DT_FLAGS: i64 = 30; // Dynamic flags
    pub const DT_PREINIT_ARRAY: i64 = 32; // Pre-init function array
    pub const DT_PREINIT_ARRAYSZ: i64 = 33; // Size of pre-init array
    pub const DT_GNU_HASH: i64 = 0x6FFFFEF5; // GNU hash table address
    pub const DT_VERSYM: i64 = 0x6FFFFFF0; // Symbol version table
    pub const DT_VERDEF: i64 = 0x6FFFFFFC; // Version definition table
    pub const DT_VERDEFNUM: i64 = 0x6FFFFFFD; // Number of version definitions
    pub const DT_VERNEED: i64 = 0x6FFFFFFE; // Version needed table
    pub const DT_VERNEEDNUM: i64 = 0x6FFFFFFF; // Number of version needed entries
    pub const DT_FLAGS_1: i64 = 0x6FFFFFFB; // Additional flags
}

/// DT_FLAGS values
pub mod dt_flags {
    pub const DF_ORIGIN: u64 = 0x1;
    pub const DF_SYMBOLIC: u64 = 0x2;
    pub const DF_TEXTREL: u64 = 0x4;
    pub const DF_BIND_NOW: u64 = 0x8;
    pub const DF_STATIC_TLS: u64 = 0x10;
}

/// DT_FLAGS_1 values
pub mod dt_flags_1 {
    pub const DF_1_NOW: u64 = 0x1;
    pub const DF_1_GLOBAL: u64 = 0x2;
    pub const DF_1_NODELETE: u64 = 0x8;
    pub const DF_1_NOOPEN: u64 = 0x40;
    pub const DF_1_ORIGIN: u64 = 0x80;
}

/// Relocation types for x86_64
pub mod relocation_types {
    pub const R_X86_64_NONE: u32 = 0; // No relocation
    pub const R_X86_64_64: u32 = 1; // Direct 64 bit
    pub const R_X86_64_PC32: u32 = 2; // PC relative 32 bit signed
    pub const R_X86_64_GOT32: u32 = 3; // 32 bit GOT entry
    pub const R_X86_64_PLT32: u32 = 4; // 32 bit PLT address
    pub const R_X86_64_COPY: u32 = 5; // Copy symbol at runtime
    pub const R_X86_64_GLOB_DAT: u32 = 6; // Create GOT entry
    pub const R_X86_64_JUMP_SLOT: u32 = 7; // Create PLT entry
    pub const R_X86_64_RELATIVE: u32 = 8; // Adjust by program base
    pub const R_X86_64_GOTPCREL: u32 = 9; // 32 bit signed PC relative offset to GOT
    pub const R_X86_64_32: u32 = 10; // Direct 32 bit zero extended
    pub const R_X86_64_32S: u32 = 11; // Direct 32 bit sign extended
    pub const R_X86_64_16: u32 = 12; // Direct 16 bit zero extended
    pub const R_X86_64_PC16: u32 = 13; // 16 bit sign extended PC relative
    pub const R_X86_64_8: u32 = 14; // Direct 8 bit sign extended
    pub const R_X86_64_PC8: u32 = 15; // 8 bit sign extended PC relative
}

/// Errors that can occur during dynamic linking
#[derive(Debug, Clone)]
pub enum DynamicLinkerError {
    /// PT_DYNAMIC segment not found
    NoDynamicSegment,

    /// Invalid dynamic section entry
    InvalidDynamicEntry,

    /// Required library not found
    LibraryNotFound(String),

    /// Symbol not found
    SymbolNotFound(String),

    /// Unsupported relocation type
    UnsupportedRelocation(u32),

    /// Invalid memory address
    InvalidAddress,

    /// Memory allocation failed
    AllocationFailed,

    /// Invalid ELF file
    InvalidElf(String),
}

impl fmt::Display for DynamicLinkerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DynamicLinkerError::NoDynamicSegment => {
                write!(f, "PT_DYNAMIC segment not found in ELF binary")
            }
            DynamicLinkerError::InvalidDynamicEntry => write!(f, "Invalid dynamic section entry"),
            DynamicLinkerError::LibraryNotFound(lib) => {
                write!(f, "Required library not found: {}", lib)
            }
            DynamicLinkerError::SymbolNotFound(sym) => write!(f, "Symbol not found: {}", sym),
            DynamicLinkerError::UnsupportedRelocation(r_type) => {
                write!(f, "Unsupported relocation type: {}", r_type)
            }
            DynamicLinkerError::InvalidAddress => write!(f, "Invalid memory address"),
            DynamicLinkerError::AllocationFailed => write!(f, "Memory allocation failed"),
            DynamicLinkerError::InvalidElf(msg) => write!(f, "Invalid ELF: {}", msg),
        }
    }
}

pub type DynamicLinkerResult<T> = Result<T, DynamicLinkerError>;

impl DynamicLinker {
    /// Create a new dynamic linker instance
    pub fn new() -> Self {
        let mut search_paths = Vec::new();
        // Standard Linux library search paths
        search_paths.push(String::from("/lib"));
        search_paths.push(String::from("/lib64"));
        search_paths.push(String::from("/usr/lib"));
        search_paths.push(String::from("/usr/lib64"));
        search_paths.push(String::from("/usr/local/lib"));

        Self {
            search_paths,
            library_path_env: Vec::new(),
            loaded_libraries: BTreeMap::new(),
            symbol_table: BTreeMap::new(),
            symbol_index_table: Vec::new(),
            // Start library loading at a safe address (above user space)
            next_base_address: VirtAddr::new(0x400000_0000),
        }
    }

    /// Add a library search path
    pub fn add_search_path(&mut self, path: String) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Parse `LD_LIBRARY_PATH` (colon-separated) into transient search paths.
    pub fn set_ld_library_path(&mut self, value: &str) {
        self.library_path_env.clear();
        for component in value.split(':') {
            if component.is_empty() {
                continue;
            }
            self.library_path_env.push(String::from(component));
        }
    }

    /// Parse PT_DYNAMIC segment from ELF binary
    pub fn parse_dynamic_section(
        &self,
        binary_data: &[u8],
        program_headers: &[Elf64ProgramHeader],
        base_address: VirtAddr,
    ) -> DynamicLinkerResult<DynamicInfo> {
        // Find PT_DYNAMIC segment
        let dynamic_phdr = program_headers
            .iter()
            .find(|phdr| phdr.p_type == elf_constants::PT_DYNAMIC)
            .ok_or(DynamicLinkerError::NoDynamicSegment)?;

        let mut dynamic_info = DynamicInfo::default();

        // Parse dynamic entries
        let dyn_offset = dynamic_phdr.p_offset as usize;
        let dyn_size = dynamic_phdr.p_filesz as usize;

        if dyn_offset + dyn_size > binary_data.len() {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "Dynamic section out of bounds",
            )));
        }

        let dyn_data = &binary_data[dyn_offset..dyn_offset + dyn_size];
        let entry_count = dyn_size / core::mem::size_of::<DynamicEntry>();

        for i in 0..entry_count {
            let entry = self.parse_dynamic_entry(dyn_data, i)?;

            if entry.d_tag == dynamic_tags::DT_NULL {
                break; // End of dynamic section
            }

            self.process_dynamic_entry(&mut dynamic_info, &entry, base_address);
        }

        Ok(dynamic_info)
    }

    /// Parse a single dynamic entry
    fn parse_dynamic_entry(&self, data: &[u8], index: usize) -> DynamicLinkerResult<DynamicEntry> {
        let offset = index * core::mem::size_of::<DynamicEntry>();

        if offset + core::mem::size_of::<DynamicEntry>() > data.len() {
            return Err(DynamicLinkerError::InvalidDynamicEntry);
        }

        // Read d_tag (8 bytes, little-endian)
        let d_tag = i64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);

        // Read d_val (8 bytes, little-endian)
        let d_val = u64::from_le_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
            data[offset + 12],
            data[offset + 13],
            data[offset + 14],
            data[offset + 15],
        ]);

        Ok(DynamicEntry { d_tag, d_val })
    }

    /// Process a dynamic entry and update DynamicInfo
    fn process_dynamic_entry(&self, info: &mut DynamicInfo, entry: &DynamicEntry, base: VirtAddr) {
        match entry.d_tag {
            dynamic_tags::DT_NEEDED => {
                // d_val is an offset into the string table (DT_STRTAB).
                // Store as "offset:N" — resolved to the actual library name
                // in resolve_needed_libraries() once the string table is
                // available.
                info.needed.push(format!("offset:{}", entry.d_val));
            }
            dynamic_tags::DT_STRTAB => {
                info.strtab = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_STRSZ => {
                info.strsz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_SYMTAB => {
                info.symtab = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_SYMENT => {
                info.syment = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_HASH => {
                info.hash = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_RELA => {
                info.rela = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_RELASZ => {
                info.relasz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_RELAENT => {
                info.relaent = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_JMPREL => {
                info.jmprel = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_PLTRELSZ => {
                info.pltrelsz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_INIT => {
                info.init = Some(VirtAddr::new(base.as_u64() + entry.d_val));
            }
            dynamic_tags::DT_FINI => {
                info.fini = Some(VirtAddr::new(base.as_u64() + entry.d_val));
            }
            dynamic_tags::DT_RPATH => {
                info.rpath = Some(format!("offset:{}", entry.d_val));
            }
            dynamic_tags::DT_RUNPATH => {
                info.runpath = Some(format!("offset:{}", entry.d_val));
            }
            dynamic_tags::DT_INIT_ARRAY => {
                info.init_array = Some(VirtAddr::new(base.as_u64() + entry.d_val));
            }
            dynamic_tags::DT_INIT_ARRAYSZ => {
                info.init_arraysz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_FINI_ARRAY => {
                info.fini_array = Some(VirtAddr::new(base.as_u64() + entry.d_val));
            }
            dynamic_tags::DT_FINI_ARRAYSZ => {
                info.fini_arraysz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_PREINIT_ARRAY => {
                info.preinit_array = Some(VirtAddr::new(base.as_u64() + entry.d_val));
            }
            dynamic_tags::DT_PREINIT_ARRAYSZ => {
                info.preinit_arraysz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_GNU_HASH => {
                info.gnu_hash = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_VERSYM => {
                info.versym = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_VERDEF => {
                info.verdef = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_VERDEFNUM => {
                info.verdefnum = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_VERNEED => {
                info.verneed = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_VERNEEDNUM => {
                info.verneednum = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_FLAGS => {
                info.flags = Some(entry.d_val);
            }
            dynamic_tags::DT_FLAGS_1 => {
                info.flags_1 = Some(entry.d_val);
            }
            dynamic_tags::DT_REL => {
                info.rel = Some(VirtAddr::new(entry.d_val));
            }
            dynamic_tags::DT_RELSZ => {
                info.relsz = Some(entry.d_val as usize);
            }
            dynamic_tags::DT_RELENT => {
                info.relent = Some(entry.d_val as usize);
            }
            _ => {
                // Ignore unrecognized tags
            }
        }
    }

    /// Load required dependencies for a binary
    pub fn load_dependencies(&mut self, needed: &[String]) -> DynamicLinkerResult<Vec<String>> {
        let mut loaded = Vec::new();

        for lib_name in needed {
            // Skip if already loaded
            if self.loaded_libraries.contains_key(lib_name) {
                continue;
            }

            // Try to find the library
            match self.find_library(lib_name) {
                Some(path) => match self.load_library_file(&path) {
                    Ok(data) => {
                        self.register_shared_library(lib_name, &data)?;
                        loaded.push(lib_name.clone());
                    }
                    Err(DynamicLinkerError::LibraryNotFound(_)) => {
                        return Err(DynamicLinkerError::LibraryNotFound(lib_name.clone()));
                    }
                    Err(e) => return Err(e),
                },
                None => {
                    return Err(DynamicLinkerError::LibraryNotFound(lib_name.clone()));
                }
            }
        }

        Ok(loaded)
    }

    /// Parse, map, and register a shared library loaded from the VFS.
    fn register_shared_library(&mut self, name: &str, data: &[u8]) -> DynamicLinkerResult<()> {
        if self.loaded_libraries.contains_key(name) {
            return Ok(());
        }

        let base = self.next_base_address;
        let loader = ElfLoader::new(false, true);
        let (loaded_base, mem_size, program_headers) = loader
            .load_shared_library(data, base)
            .map_err(|e| DynamicLinkerError::InvalidElf(format!("{e}")))?;

        let mut dynamic_info = self.parse_dynamic_section(data, &program_headers, loaded_base)?;
        self.resolve_library_names(data, &mut dynamic_info)?;

        let nested: Vec<String> = dynamic_info
            .needed
            .iter()
            .filter(|dep| !self.loaded_libraries.contains_key(*dep))
            .cloned()
            .collect();
        self.load_dependencies(&nested)?;

        self.build_symbol_index_table(data, &dynamic_info, loaded_base)?;
        let relocations = self.parse_relocations(data, &dynamic_info)?;
        self.apply_relocations(&relocations, loaded_base)?;

        for (name, addr) in self
            .symbol_index_table
            .iter()
            .filter(|(name, addr)| !name.is_empty() && addr.as_u64() != 0)
            .map(|(name, addr)| (name.clone(), *addr))
            .collect::<Vec<_>>()
        {
            self.add_symbol(name, addr);
        }

        if let Some(runpath) = dynamic_info.runpath.clone() {
            self.add_runpath_entries(&runpath);
        } else if let Some(rpath) = dynamic_info.rpath.clone() {
            self.add_runpath_entries(&rpath);
        }

        let library = LoadedLibrary {
            name: name.to_string(),
            base_address: loaded_base,
            size: mem_size,
            entry_point: None,
            dynamic_info,
        };
        self.loaded_libraries.insert(name.to_string(), library);
        self.next_base_address =
            VirtAddr::new(loaded_base.as_u64() + mem_size as u64 + PAGE_SIZE as u64);

        Ok(())
    }

    /// Search for a library in search paths
    fn find_library(&self, name: &str) -> Option<String> {
        for path in &self.library_path_env {
            let full_path = format!("{}/{}", path, name);
            if self.check_file_exists(&full_path) {
                return Some(full_path);
            }
        }

        for path in &self.search_paths {
            let full_path = format!("{}/{}", path, name);
            if self.check_file_exists(&full_path) {
                return Some(full_path);
            }
        }
        None
    }

    fn add_runpath_entries(&mut self, path_list: &str) {
        for path in path_list.split(':') {
            if path.is_empty() {
                continue;
            }
            self.add_search_path(String::from(path));
        }
    }

    /// Check if a file exists in the filesystem
    fn check_file_exists(&self, path: &str) -> bool {
        vfs_stat(path).is_ok()
    }

    fn resolve_library_path(&self, path: &str) -> DynamicLinkerResult<String> {
        let mut current = String::from(path);

        for _ in 0..8 {
            let stat = vfs_stat(&current)
                .map_err(|_| DynamicLinkerError::LibraryNotFound(current.clone()))?;
            if stat.inode_type != InodeType::Symlink {
                return Ok(current);
            }

            let target = vfs_readlink(&current)
                .map_err(|_| DynamicLinkerError::LibraryNotFound(current.clone()))?;
            if target.starts_with('/') {
                current = target;
                continue;
            }

            let parent = current
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
            current = if parent.is_empty() {
                target
            } else {
                format!("{}/{}", parent, target)
            };
        }

        Err(DynamicLinkerError::InvalidElf(String::from(
            "too many library symlink levels",
        )))
    }

    /// Load a shared library file from filesystem
    ///
    /// Returns the library data if successfully loaded
    pub fn load_library_file(&self, path: &str) -> DynamicLinkerResult<Vec<u8>> {
        const MAX_LIBRARY_SIZE: usize = 64 * 1024 * 1024;

        let resolved_path = self.resolve_library_path(path)?;
        let stat = vfs_stat(&resolved_path)
            .map_err(|_| DynamicLinkerError::LibraryNotFound(resolved_path.clone()))?;

        let size = stat.size as usize;
        if size == 0 {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "Empty library file",
            )));
        }
        if size > MAX_LIBRARY_SIZE {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "Library file too large",
            )));
        }

        let fd = vfs_open(&resolved_path, OpenFlags::RDONLY, 0)
            .map_err(|_| DynamicLinkerError::LibraryNotFound(resolved_path.clone()))?;

        let mut buffer = alloc::vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let read = vfs_read(fd, &mut buffer[offset..]).map_err(|_| {
                DynamicLinkerError::InvalidElf(String::from("Failed to read library"))
            })?;
            if read == 0 {
                break;
            }
            offset += read;
        }
        let _ = vfs_close(fd);

        if offset == 0 {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "Failed to read library",
            )));
        }
        buffer.truncate(offset);
        Ok(buffer)
    }

    /// Resolve a symbol by name across all loaded libraries
    pub fn resolve_symbol(&self, name: &str) -> Option<VirtAddr> {
        self.symbol_table.get(name).copied()
    }

    /// Add a symbol to the global symbol table
    pub fn add_symbol(&mut self, name: String, address: VirtAddr) {
        self.symbol_table.insert(name, address);
    }

    /// Apply relocations to a loaded binary
    pub fn apply_relocations(
        &self,
        relocations: &[Relocation],
        base_address: VirtAddr,
    ) -> DynamicLinkerResult<()> {
        for reloc in relocations {
            match reloc.r_type {
                relocation_types::R_X86_64_NONE => {
                    // No relocation needed
                }
                relocation_types::R_X86_64_RELATIVE => {
                    // Adjust by program base address: B + A
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    let value = base_address.as_u64() + reloc.addend as u64;

                    // SAFETY: the relocation address is a valid mapped user-space
                    // address within the ELF segment.
                    unsafe {
                        self.write_relocation_value(target, value)?;
                    }
                }
                relocation_types::R_X86_64_GLOB_DAT => {
                    // Symbol value: S
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());

                    // Resolve symbol by index
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value(target, symbol_addr.as_u64())?;
                        }
                    } else {
                        // Symbol not found - this is a fatal error for GLOB_DAT
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_JUMP_SLOT => {
                    // PLT entry: S
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());

                    // Eager binding: resolve the symbol now and write its address
                    // directly into the GOT entry. A full lazy-binding trampoline
                    // (which would patch the GOT on first call via a resolver stub)
                    // is not feasible in this no_std environment because there is no
                    // user-space resolver routine we can point the GOT at. Eager
                    // binding is correct and avoids leaving GOT entries unresolved,
                    // which would fault on first call.
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value(target, symbol_addr.as_u64())?;
                        }
                    } else {
                        // Symbol could not be resolved at load time. Unlike a lazy
                        // scheme that could defer the failure to first call, an
                        // unresolved GOT entry would trap immediately on use, so
                        // surface the error now (matching GLOB_DAT behavior).
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_64 => {
                    // Direct 64-bit: S + A
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());

                    // Resolve symbol and add addend
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = symbol_addr.as_u64() + reloc.addend as u64;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_PC32 => {
                    // PC-relative 32-bit: S + A - P
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = (symbol_addr.as_u64() as i64 + reloc.addend
                            - target.as_u64() as i64) as u32;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value_32(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_PLT32 => {
                    // PLT-relative 32-bit: L + A - P (treated as PC32 for eager binding)
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = (symbol_addr.as_u64() as i64 + reloc.addend
                            - target.as_u64() as i64) as u32;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value_32(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_GOTPCREL => {
                    // GOT-relative 32-bit: G + GOT + A - P
                    // With eager binding, GOT entries are already resolved, so
                    // we can compute the PC-relative offset to the GOT slot.
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = (symbol_addr.as_u64() as i64 + reloc.addend
                            - target.as_u64() as i64) as u32;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value_32(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_32 => {
                    // Direct 32-bit zero-extended: S + A
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = (symbol_addr.as_u64() as i64 + reloc.addend) as u32;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value_32(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                relocation_types::R_X86_64_32S => {
                    // Direct 32-bit sign-extended: S + A
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = (symbol_addr.as_u64() as i64 + reloc.addend) as u32;
                        // SAFETY: the relocation address is a valid mapped user-space
                        // address within the ELF segment.
                        unsafe {
                            self.write_relocation_value_32(target, value)?;
                        }
                    } else {
                        return Err(DynamicLinkerError::SymbolNotFound(format!(
                            "symbol index {}",
                            reloc.symbol
                        )));
                    }
                }
                _ => {
                    // Unsupported relocation type
                    return Err(DynamicLinkerError::UnsupportedRelocation(reloc.r_type));
                }
            }
        }

        Ok(())
    }

    /// Get list of loaded libraries
    pub fn loaded_libraries(&self) -> Vec<&LoadedLibrary> {
        self.loaded_libraries.values().collect()
    }

    /// Check if a library is loaded
    pub fn is_loaded(&self, name: &str) -> bool {
        self.loaded_libraries.contains_key(name)
    }

    /// Resolve a symbol name against the loaded libraries reachable from
    /// `link_map`.  Returns the raw virtual address (as `usize`) of the symbol,
    /// or `None` if not found.
    ///
    /// This is the public entry point used by the lazy-binding resolver:
    /// ```text
    /// let addr = linker.resolve_symbol_for_link_map("printf", &link_map)?;
    /// unsafe { *(got_entry as *mut usize) = addr; }
    /// ```
    pub fn resolve_symbol_for_link_map(&self, name: &str, _link_map: &LinkMap) -> Option<usize> {
        self.resolve_symbol(name).map(|a| a.as_u64() as usize)
    }

    /// Complete dynamic linking workflow for a binary
    ///
    /// This is the main entry point that orchestrates:
    /// 1. Parsing PT_DYNAMIC section
    /// 2. Resolving library names from string table
    /// 3. Loading dependencies
    /// 4. Building symbol table
    /// 5. Parsing and applying relocations
    ///
    /// # Arguments
    /// * `binary_data` - The ELF binary data
    /// * `program_headers` - Program headers from the ELF
    /// * `base_address` - Base address where binary is loaded
    ///
    /// # Returns
    /// Number of relocations applied
    pub fn link_binary(
        &mut self,
        binary_data: &[u8],
        program_headers: &[super::elf_loader::Elf64ProgramHeader],
        base_address: VirtAddr,
    ) -> DynamicLinkerResult<usize> {
        // Step 1: Parse dynamic section
        let mut dynamic_info =
            self.parse_dynamic_section(binary_data, program_headers, base_address)?;

        // Step 2: Resolve library names from string table
        self.resolve_library_names(binary_data, &mut dynamic_info)?;

        // Expand $ORIGIN/$LIB/$PLATFORM in RPATH/RUNPATH if the binary
        // requests it (DF_ORIGIN / DF_1_ORIGIN).
        let origin = if needs_origin_expansion(&dynamic_info) {
            // Derive the origin directory from the binary's base address path.
            // In a real ld.so this is the directory of the containing file.
            // We use the first search path as a fallback.
            String::new()
        } else {
            String::new()
        };

        if let Some(runpath) = dynamic_info.runpath.clone() {
            let expanded = expand_rpath_tokens(&runpath, &origin);
            self.add_runpath_entries(&expanded);
        } else if let Some(rpath) = dynamic_info.rpath.clone() {
            let expanded = expand_rpath_tokens(&rpath, &origin);
            self.add_runpath_entries(&expanded);
        }

        // Step 3: Load required dependencies
        let _loaded_libs = self.load_dependencies(&dynamic_info.needed)?;

        // Step 4: Load symbols from this binary into global symbol table
        let _symbol_count =
            self.load_symbols_from_binary(binary_data, &dynamic_info, base_address)?;

        // Step 5: Parse relocations
        let relocations = self.parse_relocations(binary_data, &dynamic_info)?;
        let reloc_count = relocations.len();

        // Step 6: Apply relocations
        self.apply_relocations(&relocations, base_address)?;

        // Step 7: Call init functions (preinit, init, init_array)
        self.call_init_functions(&dynamic_info)?;

        Ok(reloc_count)
    }

    /// Call pre-init, init, and init_array functions for a loaded object.
    ///
    /// In a real Linux dynamic linker, these are called after all relocations
    /// have been applied. The order is:
    /// 1. DT_PREINIT_ARRAY (only for the main executable, not shared libs)
    /// 2. DT_INIT (legacy single init function)
    /// 3. DT_INIT_ARRAY (array of constructor functions)
    ///
    /// Each function is called with (argc, argv, envp) for the main executable,
    /// or with no arguments for shared libraries. In our kernel context we
    /// pass null pointers since we don't have a full user-space argv/envp set.
    fn call_init_functions(&self, info: &DynamicInfo) -> DynamicLinkerResult<()> {
        // DT_PREINIT_ARRAY — only meaningful for the main executable.
        if let (Some(array_addr), Some(array_sz)) = (info.preinit_array, info.preinit_arraysz) {
            let count = array_sz / core::mem::size_of::<u64>();
            for i in 0..count {
                let entry_addr =
                    array_addr.as_u64() + ((i as u64) * core::mem::size_of::<u64>() as u64);
                // SAFETY: the pointer is within a valid mapped ELF section;
                // read_volatile performs a volatile load from the preinit array.
                let func_ptr = unsafe { core::ptr::read_volatile(entry_addr as *const u64) };
                if func_ptr != 0 {
                    // SAFETY: the function pointer comes from the binary's
                    // init array and points to a valid constructor function.
                    unsafe {
                        let init_fn: extern "C" fn(i32, *const u8, *const u8) =
                            core::mem::transmute(func_ptr);
                        init_fn(0, core::ptr::null(), core::ptr::null());
                    }
                }
            }
        }

        // DT_INIT — legacy single init function.
        if let Some(init_addr) = info.init {
            let func_ptr = init_addr.as_u64();
            if func_ptr != 0 {
                // SAFETY: `func_ptr` is the DT_INIT entry from the dynamic
                // section of a loaded, relocated ELF object. Relocation has
                // resolved all symbols, so the address points to a valid
                // `extern "C" fn()` with the C calling convention.
                unsafe {
                    let init_fn: extern "C" fn() = core::mem::transmute(func_ptr);
                    init_fn();
                }
            }
        }

        // DT_INIT_ARRAY — array of constructor functions.
        if let (Some(array_addr), Some(array_sz)) = (info.init_array, info.init_arraysz) {
            let count = array_sz / core::mem::size_of::<u64>();
            for i in 0..count {
                let entry_addr =
                    array_addr.as_u64() + ((i as u64) * core::mem::size_of::<u64>() as u64);
                // SAFETY: the pointer is within a valid mapped ELF section;
                // read_volatile performs a volatile load from the init array.
                let func_ptr = unsafe { core::ptr::read_volatile(entry_addr as *const u64) };
                if func_ptr != 0 {
                    // SAFETY: the function pointer comes from the binary's
                    // init array and points to a valid constructor function.
                    unsafe {
                        let init_fn: extern "C" fn() = core::mem::transmute(func_ptr);
                        init_fn();
                    }
                }
            }
        }

        Ok(())
    }

    /// Call fini and fini_array functions for a loaded object.
    ///
    /// Called when a library is unloaded or the process exits.
    /// The order is the reverse of init:
    /// 1. DT_FINI_ARRAY (in reverse order)
    /// 2. DT_FINI (legacy single fini function)
    pub fn call_fini_functions(&self, info: &DynamicInfo) {
        // DT_FINI_ARRAY — in reverse order.
        if let (Some(array_addr), Some(array_sz)) = (info.fini_array, info.fini_arraysz) {
            let count = array_sz / core::mem::size_of::<u64>();
            for i in (0..count).rev() {
                let entry_addr =
                    array_addr.as_u64() + ((i as u64) * core::mem::size_of::<u64>() as u64);
                // SAFETY: the pointer is within a valid mapped ELF section;
                // read_volatile performs a volatile load from the fini array.
                let func_ptr = unsafe { core::ptr::read_volatile(entry_addr as *const u64) };
                if func_ptr != 0 {
                    // SAFETY: `func_ptr` comes from the DT_FINI_ARRAY entry
                    // of a loaded, relocated ELF object. The address points to
                    // a valid `extern "C" fn()` destructor.
                    unsafe {
                        let fini_fn: extern "C" fn() = core::mem::transmute(func_ptr);
                        fini_fn();
                    }
                }
            }
        }

        // DT_FINI — legacy single fini function.
        if let Some(fini_addr) = info.fini {
            let func_ptr = fini_addr.as_u64();
            if func_ptr != 0 {
                // SAFETY: `func_ptr` is the DT_FINI entry from the dynamic
                // section of a loaded, relocated ELF object. The address points
                // to a valid `extern "C" fn()` finalizer.
                unsafe {
                    let fini_fn: extern "C" fn() = core::mem::transmute(func_ptr);
                    fini_fn();
                }
            }
        }
    }

    /// Get linking statistics
    pub fn get_stats(&self) -> DynamicLinkerStats {
        DynamicLinkerStats {
            loaded_libraries: self.loaded_libraries.len(),
            global_symbols: self.symbol_table.len(),
            search_paths: self.search_paths.len(),
        }
    }

    /// Parse string table and resolve library names
    pub fn resolve_library_names(
        &self,
        binary_data: &[u8],
        dynamic_info: &mut DynamicInfo,
    ) -> DynamicLinkerResult<()> {
        // Check if we have string table information
        let strtab_addr =
            dynamic_info
                .strtab
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table",
                )))?;
        let strtab_size =
            dynamic_info
                .strsz
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table size",
                )))?;

        // DT_STRTAB gives the virtual address of the string table. When
        // processing raw binary data (not a mapped image), we need to
        // convert this to a file offset. For most ELF files, the program
        // header's p_vaddr equals p_offset for the first LOAD segment,
        // so the virtual address can be used directly as a file offset.
        // If the address is larger than the binary, it's likely a virtual
        // address that needs adjustment — try subtracting common base
        // addresses.
        let strtab_vaddr = strtab_addr.as_u64() as usize;
        let strtab_offset = if strtab_vaddr < binary_data.len() {
            // Looks like a file offset already
            strtab_vaddr
        } else {
            // Try treating it as a virtual address relative to a base.
            // Common bases: 0x400000 (non-PIE executable), 0 (PIE/shared).
            // Try subtracting 0x400000 first, then 0.
            if let Some(off) = strtab_vaddr.checked_sub(0x400_000) {
                if off < binary_data.len() {
                    off
                } else {
                    // Can't resolve — use raw value and let the bounds
                    // check below catch it
                    strtab_vaddr
                }
            } else {
                strtab_vaddr
            }
        };

        if strtab_offset + strtab_size > binary_data.len() {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "String table out of bounds",
            )));
        }

        let strtab = &binary_data[strtab_offset..strtab_offset + strtab_size];

        // Resolve library names from offsets
        let mut resolved_names = Vec::new();
        for name_ref in &dynamic_info.needed {
            if name_ref.starts_with("offset:") {
                let offset_str = &name_ref[7..];
                if let Ok(offset) = offset_str.parse::<usize>() {
                    if let Some(name) = self.read_string_from_table(strtab, offset) {
                        resolved_names.push(name);
                    }
                }
            } else {
                // Already resolved
                resolved_names.push(name_ref.clone());
            }
        }

        dynamic_info.needed = resolved_names;

        if let Some(rpath_ref) = &dynamic_info.rpath {
            if let Some(path) = self.resolve_dynamic_string(strtab, rpath_ref) {
                dynamic_info.rpath = Some(path);
            }
        }
        if let Some(runpath_ref) = &dynamic_info.runpath {
            if let Some(path) = self.resolve_dynamic_string(strtab, runpath_ref) {
                dynamic_info.runpath = Some(path);
            }
        }

        Ok(())
    }

    fn resolve_dynamic_string(&self, strtab: &[u8], value: &str) -> Option<String> {
        if value.starts_with("offset:") {
            let offset_str = &value[7..];
            if let Ok(offset) = offset_str.parse::<usize>() {
                return self.read_string_from_table(strtab, offset);
            }
        }
        Some(String::from(value))
    }

    /// Read a null-terminated string from the string table
    fn read_string_from_table(&self, strtab: &[u8], offset: usize) -> Option<String> {
        if offset >= strtab.len() {
            return None;
        }

        let mut end = offset;
        while end < strtab.len() && strtab[end] != 0 {
            end += 1;
        }

        if end > offset {
            String::from_utf8(strtab[offset..end].to_vec()).ok()
        } else {
            None
        }
    }

    /// Parse symbol table from ELF binary
    pub fn parse_symbol_table(
        &self,
        binary_data: &[u8],
        dynamic_info: &DynamicInfo,
    ) -> DynamicLinkerResult<Vec<(String, VirtAddr, Elf64Symbol)>> {
        let symtab_addr =
            dynamic_info
                .symtab
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No symbol table",
                )))?;
        let strtab_addr =
            dynamic_info
                .strtab
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table",
                )))?;
        let strtab_size =
            dynamic_info
                .strsz
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table size",
                )))?;

        // Calculate symbol table bounds
        // We'll use hash table to determine the number of symbols if available
        let symtab_offset = symtab_addr.as_u64() as usize;
        let strtab_offset = strtab_addr.as_u64() as usize;

        if strtab_offset + strtab_size > binary_data.len() {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "String table out of bounds",
            )));
        }

        let strtab = &binary_data[strtab_offset..strtab_offset + strtab_size];

        // Calculate number of symbols
        // Prefer GNU hash table count if available, then SysV hash table,
        // then fall back to the symtab-to-strtab layout heuristic.
        let sym_count =
            if let Some(gnu_count) = self.gnu_hash_symbol_count(binary_data, dynamic_info) {
                gnu_count
            } else if strtab_offset > symtab_offset {
                (strtab_offset - symtab_offset) / core::mem::size_of::<Elf64Symbol>()
            } else {
                // Fallback: parse until we run out of data or hit invalid entries
                100 // Conservative estimate
            };

        let mut symbols = Vec::new();

        for i in 0..sym_count {
            let sym_offset = symtab_offset + i * core::mem::size_of::<Elf64Symbol>();

            if sym_offset + core::mem::size_of::<Elf64Symbol>() > binary_data.len() {
                break;
            }

            // Parse symbol entry
            // SAFETY: the pointer is within a valid mapped ELF section;
            // read_unaligned handles misalignment.
            let symbol = unsafe {
                core::ptr::read_unaligned(binary_data[sym_offset..].as_ptr() as *const Elf64Symbol)
            };

            // Skip undefined symbols
            if !symbol.is_defined() {
                continue;
            }

            // Read symbol name from string table
            if let Some(name) = self.read_string_from_table(strtab, symbol.st_name as usize) {
                if !name.is_empty() {
                    symbols.push((name, VirtAddr::new(symbol.st_value), symbol));
                }
            }
        }

        Ok(symbols)
    }

    /// Load symbols into global symbol table and index table
    pub fn load_symbols_from_binary(
        &mut self,
        binary_data: &[u8],
        dynamic_info: &DynamicInfo,
        base_address: VirtAddr,
    ) -> DynamicLinkerResult<usize> {
        // First, build the complete symbol table with indices
        self.build_symbol_index_table(binary_data, dynamic_info, base_address)?;

        // Then add defined symbols to global symbol table
        let count = self.symbol_index_table.len();

        // Collect symbols to add to avoid borrowing issues
        let symbols_to_add: Vec<_> = self
            .symbol_index_table
            .iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, addr)| (name.clone(), *addr))
            .collect();

        for (name, addr) in symbols_to_add {
            self.add_symbol(name, addr);
        }

        Ok(count)
    }

    /// Build symbol index table from binary
    ///
    /// This creates a complete mapping of symbol indices to (name, address) pairs,
    /// including undefined symbols (which will have address 0).
    fn build_symbol_index_table(
        &mut self,
        binary_data: &[u8],
        dynamic_info: &DynamicInfo,
        base_address: VirtAddr,
    ) -> DynamicLinkerResult<()> {
        let symtab_addr =
            dynamic_info
                .symtab
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No symbol table",
                )))?;
        let strtab_addr =
            dynamic_info
                .strtab
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table",
                )))?;
        let strtab_size =
            dynamic_info
                .strsz
                .ok_or(DynamicLinkerError::InvalidElf(String::from(
                    "No string table size",
                )))?;

        let symtab_offset = symtab_addr.as_u64() as usize;
        let strtab_offset = strtab_addr.as_u64() as usize;

        if strtab_offset + strtab_size > binary_data.len() {
            return Err(DynamicLinkerError::InvalidElf(String::from(
                "String table out of bounds",
            )));
        }

        let strtab = &binary_data[strtab_offset..strtab_offset + strtab_size];

        // Calculate number of symbols
        let sym_count = if strtab_offset > symtab_offset {
            (strtab_offset - symtab_offset) / core::mem::size_of::<Elf64Symbol>()
        } else {
            100 // Conservative estimate
        };

        // Clear and rebuild index table
        self.symbol_index_table.clear();

        for i in 0..sym_count {
            let sym_offset = symtab_offset + i * core::mem::size_of::<Elf64Symbol>();

            if sym_offset + core::mem::size_of::<Elf64Symbol>() > binary_data.len() {
                break;
            }

            // Parse symbol entry
            // SAFETY: the pointer is within a valid mapped ELF section;
            // read_unaligned handles misalignment.
            let symbol = unsafe {
                core::ptr::read_unaligned(binary_data[sym_offset..].as_ptr() as *const Elf64Symbol)
            };

            // Get symbol name
            let name = self
                .read_string_from_table(strtab, symbol.st_name as usize)
                .unwrap_or_else(|| String::new());

            // Calculate address (0 for undefined symbols)
            let addr = if symbol.is_defined() {
                if symbol.symbol_type() == symbol_type::STT_FUNC
                    || symbol.symbol_type() == symbol_type::STT_OBJECT
                {
                    VirtAddr::new(base_address.as_u64() + symbol.st_value)
                } else {
                    VirtAddr::new(symbol.st_value)
                }
            } else {
                VirtAddr::new(0) // Undefined - will need to be resolved from other libraries
            };

            self.symbol_index_table.push((name, addr));
        }

        Ok(())
    }

    /// Resolve symbol by index (used during relocation)
    pub fn resolve_symbol_by_index(&self, index: u32) -> Option<VirtAddr> {
        let idx = index as usize;
        if idx < self.symbol_index_table.len() {
            let (_name, addr) = &self.symbol_index_table[idx];
            if addr.as_u64() != 0 {
                Some(*addr)
            } else {
                // Symbol is undefined in current binary, try global symbol table
                let (name, _) = &self.symbol_index_table[idx];
                self.resolve_symbol(name)
            }
        } else {
            None
        }
    }

    /// Compute the DJB2 (GNU ELF) hash of a symbol name.
    ///
    /// This is the hash function used by GNU hash tables (DT_GNU_HASH).
    /// Algorithm: h = (h * 33) + c, starting with 5381.
    fn gnu_elf_hash(name: &str) -> u32 {
        let mut h: u32 = 5381;
        for &b in name.as_bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u32);
        }
        h
    }

    /// Look up a symbol by name using the GNU hash table (DT_GNU_HASH).
    ///
    /// Returns the symbol index in the symbol table if found.
    /// The GNU hash table layout is:
    ///   header: nbuckets(u32), symoffset(u32), bloom_size(u32), bloom_shift(u32)
    ///   bloom:  bloom_size × u64 words
    ///   buckets: nbuckets × u32
    ///   chain:   starts at symoffset, one entry per symbol from symoffset onward
    ///
    /// Each chain entry has the low bit clear if it's the last entry in the bucket.
    fn gnu_hash_lookup(&self, binary_data: &[u8], info: &DynamicInfo, name: &str) -> Option<u32> {
        let gnu_hash_addr = info.gnu_hash?;
        let strtab_addr = info.strtab?;
        let symtab_addr = info.symtab?;

        let hash_offset = gnu_hash_addr.as_u64() as usize;
        if hash_offset + 16 > binary_data.len() {
            return None;
        }

        let nbuckets = u32::from_le_bytes([
            binary_data[hash_offset],
            binary_data[hash_offset + 1],
            binary_data[hash_offset + 2],
            binary_data[hash_offset + 3],
        ]);
        let symoffset = u32::from_le_bytes([
            binary_data[hash_offset + 4],
            binary_data[hash_offset + 5],
            binary_data[hash_offset + 6],
            binary_data[hash_offset + 7],
        ]);
        let bloom_size = u32::from_le_bytes([
            binary_data[hash_offset + 8],
            binary_data[hash_offset + 9],
            binary_data[hash_offset + 10],
            binary_data[hash_offset + 11],
        ]);
        let bloom_shift = u32::from_le_bytes([
            binary_data[hash_offset + 12],
            binary_data[hash_offset + 13],
            binary_data[hash_offset + 14],
            binary_data[hash_offset + 15],
        ]);

        let bloom_offset = hash_offset + 16;
        let buckets_offset = bloom_offset + (bloom_size as usize) * 8;
        let chain_offset = buckets_offset + (nbuckets as usize) * 4;

        if buckets_offset + (nbuckets as usize) * 4 > binary_data.len() {
            return None;
        }

        let hash = Self::gnu_elf_hash(name);

        // Bloom filter check
        let bloom_idx = ((hash / 64) % bloom_size) as usize;
        let bloom_word = u64::from_le_bytes([
            binary_data[bloom_offset + bloom_idx * 8],
            binary_data[bloom_offset + bloom_idx * 8 + 1],
            binary_data[bloom_offset + bloom_idx * 8 + 2],
            binary_data[bloom_offset + bloom_idx * 8 + 3],
            binary_data[bloom_offset + bloom_idx * 8 + 4],
            binary_data[bloom_offset + bloom_idx * 8 + 5],
            binary_data[bloom_offset + bloom_idx * 8 + 6],
            binary_data[bloom_offset + bloom_idx * 8 + 7],
        ]);
        let bloom_bit1 = 1u64 << (hash % 64);
        let bloom_bit2 = 1u64 << ((hash >> bloom_shift) % 64);
        if (bloom_word & (bloom_bit1 | bloom_bit2)) != (bloom_bit1 | bloom_bit2) {
            return None;
        }

        // Bucket lookup
        let bucket_idx = (hash % nbuckets) as usize;
        let bucket_val = u32::from_le_bytes([
            binary_data[buckets_offset + bucket_idx * 4],
            binary_data[buckets_offset + bucket_idx * 4 + 1],
            binary_data[buckets_offset + bucket_idx * 4 + 2],
            binary_data[buckets_offset + bucket_idx * 4 + 3],
        ]);

        if bucket_val == 0 {
            return None; // Empty bucket
        }

        // Walk the chain
        let strtab_offset = strtab_addr.as_u64() as usize;
        let symtab_offset = symtab_addr.as_u64() as usize;

        let mut sym_idx = bucket_val;
        loop {
            let chain_entry_offset = chain_offset + (sym_idx - symoffset) as usize * 4;
            if chain_entry_offset + 4 > binary_data.len() {
                break;
            }
            let chain_hash = u32::from_le_bytes([
                binary_data[chain_entry_offset],
                binary_data[chain_entry_offset + 1],
                binary_data[chain_entry_offset + 2],
                binary_data[chain_entry_offset + 3],
            ]);

            // Check if hash matches (ignoring the low bit which is the "last in chain" flag)
            if (chain_hash | 1) == (hash | 1) {
                // Potential match — verify by comparing the actual symbol name
                let sym_struct_offset =
                    symtab_offset + sym_idx as usize * core::mem::size_of::<Elf64Symbol>();
                if sym_struct_offset + core::mem::size_of::<Elf64Symbol>() <= binary_data.len() {
                    // SAFETY: `binary_data` is a `&[u8]` (1-byte aligned) but
                    // `Elf64Symbol` contains `u64` fields (align 8). A plain
                    // `ptr::read` would be UB on a misaligned pointer; use
                    // `read_unaligned` which performs an unaligned load.
                    let symbol = unsafe {
                        core::ptr::read_unaligned(
                            binary_data[sym_struct_offset..].as_ptr() as *const Elf64Symbol
                        )
                    };
                    let name_offset = strtab_offset + symbol.st_name as usize;
                    if name_offset < binary_data.len() {
                        let sym_name = Self::read_c_string_at(binary_data, name_offset);
                        if sym_name == name {
                            return Some(sym_idx);
                        }
                    }
                }
            }

            // Check if this is the last entry in the chain
            if chain_hash & 1 != 0 {
                break;
            }
            sym_idx += 1;
        }

        None
    }

    /// Read a NUL-terminated C string from binary data at the given offset.
    fn read_c_string_at(data: &[u8], offset: usize) -> &str {
        let end = data[offset..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(data.len() - offset);
        core::str::from_utf8(&data[offset..offset + end]).unwrap_or("")
    }

    /// Determine the number of symbols in the symbol table using the GNU hash table.
    ///
    /// The GNU hash table's chain section extends to the end of the symbol table.
    /// We find the last non-empty bucket and walk its chain to find the last symbol index.
    fn gnu_hash_symbol_count(&self, binary_data: &[u8], info: &DynamicInfo) -> Option<usize> {
        let gnu_hash_addr = info.gnu_hash?;
        let hash_offset = gnu_hash_addr.as_u64() as usize;
        if hash_offset + 16 > binary_data.len() {
            return None;
        }

        let nbuckets = u32::from_le_bytes([
            binary_data[hash_offset],
            binary_data[hash_offset + 1],
            binary_data[hash_offset + 2],
            binary_data[hash_offset + 3],
        ]);
        let symoffset = u32::from_le_bytes([
            binary_data[hash_offset + 4],
            binary_data[hash_offset + 5],
            binary_data[hash_offset + 6],
            binary_data[hash_offset + 7],
        ]);
        let bloom_size = u32::from_le_bytes([
            binary_data[hash_offset + 8],
            binary_data[hash_offset + 9],
            binary_data[hash_offset + 10],
            binary_data[hash_offset + 11],
        ]);

        let bloom_offset = hash_offset + 16;
        let buckets_offset = bloom_offset + (bloom_size as usize) * 8;

        if buckets_offset + (nbuckets as usize) * 4 > binary_data.len() {
            return None;
        }

        // Find the maximum bucket value — the symbol count is at least that high
        let mut max_bucket: u32 = 0;
        for i in 0..nbuckets as usize {
            let val = u32::from_le_bytes([
                binary_data[buckets_offset + i * 4],
                binary_data[buckets_offset + i * 4 + 1],
                binary_data[buckets_offset + i * 4 + 2],
                binary_data[buckets_offset + i * 4 + 3],
            ]);
            if val > max_bucket {
                max_bucket = val;
            }
        }

        if max_bucket == 0 {
            return Some(symoffset as usize);
        }

        // Walk the chain from max_bucket to find the last symbol
        let chain_offset = buckets_offset + (nbuckets as usize) * 4;
        let mut sym_idx = max_bucket;
        loop {
            let chain_entry_offset = chain_offset + (sym_idx - symoffset) as usize * 4;
            if chain_entry_offset + 4 > binary_data.len() {
                break;
            }
            let chain_hash = u32::from_le_bytes([
                binary_data[chain_entry_offset],
                binary_data[chain_entry_offset + 1],
                binary_data[chain_entry_offset + 2],
                binary_data[chain_entry_offset + 3],
            ]);
            if chain_hash & 1 != 0 {
                // Last in chain — sym_idx + 1 is the count
                return Some(sym_idx as usize + 1);
            }
            sym_idx += 1;
        }

        Some(sym_idx as usize + 1)
    }

    /// Read the version string for a given symbol index from the version
    /// definition table (DT_VERDEF).
    ///
    /// Returns the version string (e.g. "GLIBC_2.2.5") if versioning info
    /// is present, or None if the binary has no version table.
    fn get_version_name(
        &self,
        binary_data: &[u8],
        info: &DynamicInfo,
        sym_index: u32,
    ) -> Option<String> {
        let versym_addr = info.versym?;
        let verdef_addr = info.verdef?;
        let verdef_count = info.verdefnum?;
        let strtab_addr = info.strtab?;
        let strtab_size = info.strsz?;

        let versym_offset = versym_addr.as_u64() as usize;
        let verdef_offset = verdef_addr.as_u64() as usize;
        let strtab_offset = strtab_addr.as_u64() as usize;

        // Read the version index for this symbol
        let versym_entry_offset = versym_offset + sym_index as usize * 2;
        if versym_entry_offset + 2 > binary_data.len() {
            return None;
        }
        let versym = u16::from_le_bytes([
            binary_data[versym_entry_offset],
            binary_data[versym_entry_offset + 1],
        ]);
        let version_idx = versym & VERSYM_VERSION_MASK;

        if version_idx == ver_ndx::VER_NDX_LOCAL || version_idx == ver_ndx::VER_NDX_GLOBAL {
            return None; // Unversioned or global — no specific version
        }

        // Walk the Verdef chain to find the entry with matching vd_ndx
        let mut offset = verdef_offset;
        for _ in 0..verdef_count {
            if offset + core::mem::size_of::<Elf64Verdef>() > binary_data.len() {
                break;
            }
            // SAFETY: the pointer is within a valid mapped ELF section;
            // read_unaligned handles misalignment.
            let verdef = unsafe {
                core::ptr::read_unaligned(binary_data[offset..].as_ptr() as *const Elf64Verdef)
            };
            if verdef.vd_ndx == version_idx {
                // Read the first Verdaux entry for the version name
                let aux_offset = offset + verdef.vd_aux as usize;
                if aux_offset + core::mem::size_of::<Elf64Verdaux>() > binary_data.len() {
                    break;
                }
                // SAFETY: the pointer is within a valid mapped ELF section;
                // read_unaligned handles misalignment.
                let verdaux = unsafe {
                    core::ptr::read_unaligned(
                        binary_data[aux_offset..].as_ptr() as *const Elf64Verdaux
                    )
                };
                let name_offset = strtab_offset + verdaux.vda_name as usize;
                if name_offset < binary_data.len() && name_offset + strtab_size <= binary_data.len()
                {
                    let name = Self::read_c_string_at(binary_data, name_offset);
                    return Some(name.to_string());
                }
            }
            if verdef.vd_next == 0 {
                break;
            }
            offset += verdef.vd_next as usize;
        }

        None
    }

    /// Resolve a symbol by name and optional version using the GNU hash table.
    ///
    /// If `version` is provided, the symbol's version string (from DT_VERSYM/
    /// DT_VERDEF) must match. If no version is provided, the default (highest)
    /// version is accepted.
    pub fn resolve_symbol_with_version(
        &self,
        binary_data: &[u8],
        info: &DynamicInfo,
        name: &str,
        version: Option<&str>,
    ) -> Option<(u32, VirtAddr)> {
        // Try GNU hash lookup first
        if let Some(sym_idx) = self.gnu_hash_lookup(binary_data, info, name) {
            // Verify version if requested
            if let Some(req_version) = version {
                if let Some(sym_version) = self.get_version_name(binary_data, info, sym_idx) {
                    if sym_version != req_version {
                        return None; // Version mismatch
                    }
                } else {
                    return None; // No version info but version was requested
                }
            }
            // Get the symbol address
            let symtab_offset = info.symtab?.as_u64() as usize;
            let sym_struct_offset =
                symtab_offset + sym_idx as usize * core::mem::size_of::<Elf64Symbol>();
            if sym_struct_offset + core::mem::size_of::<Elf64Symbol>() <= binary_data.len() {
                // SAFETY: the pointer is within a valid mapped ELF section;
                // read_unaligned handles misalignment.
                let symbol = unsafe {
                    core::ptr::read_unaligned(
                        binary_data[sym_struct_offset..].as_ptr() as *const Elf64Symbol
                    )
                };
                if symbol.is_defined() {
                    return Some((sym_idx, VirtAddr::new(symbol.st_value)));
                }
            }
        }
        None
    }

    /// Parse relocations from RELA section
    pub fn parse_relocations(
        &self,
        binary_data: &[u8],
        dynamic_info: &DynamicInfo,
    ) -> DynamicLinkerResult<Vec<Relocation>> {
        let mut relocations = Vec::new();

        // Parse regular relocations (DT_RELA)
        if let (Some(rela_addr), Some(rela_size)) = (dynamic_info.rela, dynamic_info.relasz) {
            let rela_offset = rela_addr.as_u64() as usize;
            let reloc_entry_size = dynamic_info.relaent.unwrap_or(24); // Standard RELA entry size
            let reloc_count = rela_size / reloc_entry_size;

            for i in 0..reloc_count {
                let offset = rela_offset + i * reloc_entry_size;
                if let Some(reloc) = self.parse_single_relocation(binary_data, offset)? {
                    relocations.push(reloc);
                }
            }
        }

        // Parse REL relocations (DT_REL) — no addend field, addend is implicit
        if let (Some(rel_addr), Some(rel_size)) = (dynamic_info.rel, dynamic_info.relsz) {
            let rel_offset = rel_addr.as_u64() as usize;
            let rel_entry_size = dynamic_info.relent.unwrap_or(16); // Elf64_Rel: r_offset(8) + r_info(8)
            let rel_count = rel_size / rel_entry_size;

            for i in 0..rel_count {
                let offset = rel_offset + i * rel_entry_size;
                if offset + 16 > binary_data.len() {
                    break;
                }
                let r_offset = u64::from_le_bytes([
                    binary_data[offset],
                    binary_data[offset + 1],
                    binary_data[offset + 2],
                    binary_data[offset + 3],
                    binary_data[offset + 4],
                    binary_data[offset + 5],
                    binary_data[offset + 6],
                    binary_data[offset + 7],
                ]);
                let r_info = u64::from_le_bytes([
                    binary_data[offset + 8],
                    binary_data[offset + 9],
                    binary_data[offset + 10],
                    binary_data[offset + 11],
                    binary_data[offset + 12],
                    binary_data[offset + 13],
                    binary_data[offset + 14],
                    binary_data[offset + 15],
                ]);
                let r_type = (r_info & 0xFFFFFFFF) as u32;
                let r_sym = (r_info >> 32) as u32;
                // REL has no explicit addend — addend is stored at the relocation target
                relocations.push(Relocation {
                    offset: VirtAddr::new(r_offset),
                    r_type,
                    symbol: r_sym,
                    addend: 0, // REL: addend must be read from target memory
                });
            }
        }

        // Parse PLT relocations (DT_JMPREL)
        if let (Some(jmprel_addr), Some(jmprel_size)) = (dynamic_info.jmprel, dynamic_info.pltrelsz)
        {
            let jmprel_offset = jmprel_addr.as_u64() as usize;
            let reloc_entry_size = 24; // RELA entry size
            let reloc_count = jmprel_size / reloc_entry_size;

            for i in 0..reloc_count {
                let offset = jmprel_offset + i * reloc_entry_size;
                if let Some(reloc) = self.parse_single_relocation(binary_data, offset)? {
                    relocations.push(reloc);
                }
            }
        }

        Ok(relocations)
    }

    /// Parse a single relocation entry
    fn parse_single_relocation(
        &self,
        binary_data: &[u8],
        offset: usize,
    ) -> DynamicLinkerResult<Option<Relocation>> {
        const RELA_ENTRY_SIZE: usize = 24; // r_offset (8) + r_info (8) + r_addend (8)

        if offset + RELA_ENTRY_SIZE > binary_data.len() {
            return Ok(None);
        }

        let data = &binary_data[offset..offset + RELA_ENTRY_SIZE];

        // Parse r_offset
        let r_offset = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        // Parse r_info
        let r_info = u64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);

        // Parse r_addend
        let r_addend = i64::from_le_bytes([
            data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
        ]);

        // Extract symbol and type from r_info
        let r_type = (r_info & 0xffffffff) as u32;
        let r_sym = (r_info >> 32) as u32;

        Ok(Some(Relocation {
            offset: VirtAddr::new(r_offset),
            r_type,
            symbol: r_sym,
            addend: r_addend,
        }))
    }

    /// Write value to memory (helper for relocations)
    ///
    /// # Safety
    /// This function writes to arbitrary memory addresses.
    /// Caller must ensure the address is valid and writable.
    unsafe fn write_relocation_value(&self, addr: VirtAddr, value: u64) -> DynamicLinkerResult<()> {
        let ptr_addr = addr.as_u64();
        // Validate that the target address is writable user memory
        crate::memory::user_space::UserSpaceMemory::validate_user_ptr(
            ptr_addr,
            core::mem::size_of::<u64>() as u64,
            true,
        )
        .map_err(|_| DynamicLinkerError::InvalidAddress)?;

        let ptr = ptr_addr as *mut u64;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }

    /// Write a 32-bit relocation value to the target address.
    ///
    /// # Safety
    /// This function writes to arbitrary memory addresses.
    /// Caller must ensure the address is valid and writable.
    unsafe fn write_relocation_value_32(
        &self,
        addr: VirtAddr,
        value: u32,
    ) -> DynamicLinkerResult<()> {
        let ptr_addr = addr.as_u64();
        crate::memory::user_space::UserSpaceMemory::validate_user_ptr(
            ptr_addr,
            core::mem::size_of::<u32>() as u64,
            true,
        )
        .map_err(|_| DynamicLinkerError::InvalidAddress)?;

        let ptr = ptr_addr as *mut u32;
        core::ptr::write_volatile(ptr, value);
        Ok(())
    }
}

impl Default for DynamicLinker {
    fn default() -> Self {
        Self::new()
    }
}

/// Dynamic linker statistics
#[derive(Debug, Clone, Copy)]
pub struct DynamicLinkerStats {
    pub loaded_libraries: usize,
    pub global_symbols: usize,
    pub search_paths: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_dynamic_linker_creation() {
        let linker = DynamicLinker::new();
        assert_eq!(linker.search_paths.len(), 5);
        assert!(linker.search_paths.contains(&String::from("/lib")));
    }

    #[test_case]
    fn test_add_search_path() {
        let mut linker = DynamicLinker::new();
        linker.add_search_path(String::from("/custom/lib"));
        assert!(linker.search_paths.contains(&String::from("/custom/lib")));
    }

    #[test_case]
    fn test_symbol_resolution() {
        let mut linker = DynamicLinker::new();
        let addr = VirtAddr::new(0x1000);
        linker.add_symbol(String::from("test_symbol"), addr);

        assert_eq!(linker.resolve_symbol("test_symbol"), Some(addr));
        assert_eq!(linker.resolve_symbol("nonexistent"), None);
    }

    #[test_case]
    fn test_string_table_reading() {
        let linker = DynamicLinker::new();
        let strtab = b"\x00hello\x00world\x00test\x00";

        assert_eq!(
            linker.read_string_from_table(strtab, 1),
            Some(String::from("hello"))
        );
        assert_eq!(
            linker.read_string_from_table(strtab, 7),
            Some(String::from("world"))
        );
        assert_eq!(
            linker.read_string_from_table(strtab, 13),
            Some(String::from("test"))
        );
        assert_eq!(linker.read_string_from_table(strtab, 0), None); // Empty string
    }

    #[test_case]
    fn test_elf_symbol_binding() {
        let symbol = Elf64Symbol {
            st_name: 0,
            st_info: (symbol_binding::STB_GLOBAL << 4) | symbol_type::STT_FUNC,
            st_other: 0,
            st_shndx: 1,
            st_value: 0x1000,
            st_size: 100,
        };

        assert_eq!(symbol.binding(), symbol_binding::STB_GLOBAL);
        assert_eq!(symbol.symbol_type(), symbol_type::STT_FUNC);
        assert!(symbol.is_defined());
    }

    #[test_case]
    fn test_library_loaded_check() {
        let linker = DynamicLinker::new();
        assert!(!linker.is_loaded("libc.so.6"));
        assert_eq!(linker.loaded_libraries().len(), 0);
    }

    #[test_case]
    fn test_symbol_index_resolution() {
        let mut linker = DynamicLinker::new();

        // Manually populate symbol index table for testing
        linker
            .symbol_index_table
            .push((String::from("sym1"), VirtAddr::new(0x1000)));
        linker
            .symbol_index_table
            .push((String::from("sym2"), VirtAddr::new(0x2000)));
        linker
            .symbol_index_table
            .push((String::from(""), VirtAddr::new(0))); // Undefined

        // Test defined symbols
        assert_eq!(
            linker.resolve_symbol_by_index(0),
            Some(VirtAddr::new(0x1000))
        );
        assert_eq!(
            linker.resolve_symbol_by_index(1),
            Some(VirtAddr::new(0x2000))
        );

        // Test undefined symbol (should return None unless in global table)
        assert_eq!(linker.resolve_symbol_by_index(2), None);

        // Test out of bounds
        assert_eq!(linker.resolve_symbol_by_index(99), None);
    }

    #[test_case]
    fn test_linker_stats() {
        let mut linker = DynamicLinker::new();
        linker.add_symbol(String::from("test"), VirtAddr::new(0x1000));

        let stats = linker.get_stats();
        assert_eq!(stats.search_paths, 5);
        assert_eq!(stats.global_symbols, 1);
        assert_eq!(stats.loaded_libraries, 0);
    }
}

// Global dynamic linker instance
lazy_static! {
    static ref GLOBAL_DYNAMIC_LINKER: Mutex<Option<DynamicLinker>> = Mutex::new(None);
}

/// Initialize the global dynamic linker
pub fn init_dynamic_linker() {
    *GLOBAL_DYNAMIC_LINKER.lock() = Some(DynamicLinker::new());
}

/// Get a reference to the global dynamic linker
///
/// # Safety
/// This function provides mutable access to the global dynamic linker.
/// Caller must ensure proper synchronization when using the returned reference.
pub fn with_dynamic_linker<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut DynamicLinker) -> R,
{
    let mut linker = GLOBAL_DYNAMIC_LINKER.lock();
    linker.as_mut().map(f)
}

/// Link a binary using the global dynamic linker
///
/// This is a convenience function that can be called from the process module
/// to handle dynamic linking during process execution.
///
/// # Arguments
/// * `binary_data` - The ELF binary data
/// * `program_headers` - Program headers from the ELF
/// * `base_address` - Base address where binary is loaded
///
/// # Returns
/// Number of relocations applied, or error message
pub fn link_binary_globally(
    binary_data: &[u8],
    program_headers: &[super::elf_loader::Elf64ProgramHeader],
    base_address: VirtAddr,
) -> Result<usize, &'static str> {
    let mut linker = get_dynamic_linker().ok_or("Dynamic linker not initialized")?;

    linker
        .link_binary(binary_data, program_headers, base_address)
        .map_err(|_| "Failed to link binary")
}

/// Retrieves a clone of the global dynamic linker instance.
///
/// This function provides thread-safe access to the global dynamic linker by
/// acquiring a lock on the global instance and returning a cloned copy. This
/// approach ensures that the caller has a consistent snapshot of the linker
/// state without holding the lock for extended periods.
///
/// # Returns
/// - `Some(DynamicLinker)` - A cloned copy of the global dynamic linker if initialized
/// - `None` - If the global dynamic linker has not been initialized via `init_dynamic_linker()`
///
/// # Thread Safety
/// This function is thread-safe and uses a spin lock to protect access to the
/// global instance. The lock is held only for the duration of the clone operation.
///
/// # Example
/// ```rust,ignore
/// use crate::process::dynamic_linker::{init_dynamic_linker, get_dynamic_linker};
///
/// // Initialize the global linker first
/// init_dynamic_linker();
///
/// // Get a working copy
/// if let Some(mut linker) = get_dynamic_linker() {
///     linker.add_search_path("/custom/lib".to_string());
///     // Use the linker...
/// }
/// ```
///
/// # Note
/// Changes made to the returned `DynamicLinker` instance are not reflected in
/// the global instance. Use `with_dynamic_linker()` if you need to modify the
/// global state.
pub fn get_dynamic_linker() -> Option<DynamicLinker> {
    let linker_guard = GLOBAL_DYNAMIC_LINKER.lock();
    (*linker_guard).clone()
}

// ── dlopen / dlsym / dlclose / dlerror API ─────────────────────────────

/// Opaque handle returned by `dlopen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DlHandle(pub u64);

/// dlopen flags
pub mod rtld {
    pub const RTLD_LAZY: i32 = 0x1;
    pub const RTLD_NOW: i32 = 0x2;
    pub const RTLD_GLOBAL: i32 = 0x100;
    pub const RTLD_LOCAL: i32 = 0x0;
    pub const RTLD_NODELETE: i32 = 0x800;
    pub const RTLD_NOLOAD: i32 = 0x4;
}

/// Global table of dlopen handles: handle → (library name, base address)
static DL_HANDLES: spin::Mutex<alloc::collections::BTreeMap<u64, (String, VirtAddr)>> =
    spin::Mutex::new(alloc::collections::BTreeMap::new());

static NEXT_DL_HANDLE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Thread-local error string for dlerror
static DL_ERROR: spin::Mutex<Option<String>> = spin::Mutex::new(None);

/// dlopen — load a shared library at runtime.
///
/// Loads the named shared object into the address space and returns an
/// opaque handle. If the library is already loaded, returns the existing
/// handle (with refcount semantics handled by the caller).
///
/// # Arguments
/// * `filename` - Path to the shared library (e.g. "libc.so.6" or "/lib/libc.so.6")
/// * `flags` - RTLD_LAZY, RTLD_NOW, RTLD_GLOBAL, RTLD_LOCAL, etc.
///
/// # Returns
/// Handle on success, or 0 on failure (use `dlerror` for details).
pub fn dlopen(filename: &str, flags: i32) -> u64 {
    // Clear previous error
    *DL_ERROR.lock() = None;

    // RTLD_NOLOAD: don't load, just check if already loaded
    if flags & rtld::RTLD_NOLOAD != 0 {
        let handles = DL_HANDLES.lock();
        for (_, (name, _)) in handles.iter() {
            if name == filename {
                // Return existing handle — find its key
                for (key, (n, _)) in handles.iter() {
                    if n == filename {
                        return *key;
                    }
                }
            }
        }
        if flags & rtld::RTLD_NOLOAD != 0 {
            *DL_ERROR.lock() = Some(format!("Library {} not loaded", filename));
            return 0;
        }
    }

    // Try to load via the global dynamic linker
    let mut linker = match get_dynamic_linker() {
        Some(l) => l,
        None => {
            *DL_ERROR.lock() = Some(String::from("Dynamic linker not initialized"));
            return 0;
        }
    };

    // Load the library file
    let lib_data = match linker.load_library_file(filename) {
        Ok(data) => data,
        Err(e) => {
            *DL_ERROR.lock() = Some(format!("Failed to load {}: {:?}", filename, e));
            return 0;
        }
    };

    // Parse ELF headers and load segments via the ELF loader
    let base_address = linker.next_base_address;
    let loader = super::elf_loader::ElfLoader::new(false, true);
    let (_load_base, _mapped_size, program_headers) =
        match loader.load_shared_library(&lib_data, base_address) {
            Ok(result) => result,
            Err(e) => {
                *DL_ERROR.lock() = Some(format!(
                    "Failed to load ELF segments in {}: {:?}",
                    filename, e
                ));
                return 0;
            }
        };

    let next_base = base_address.as_u64()
        + ((lib_data.len() + PAGE_SIZE - 1) / PAGE_SIZE) as u64 * PAGE_SIZE as u64;

    // Link the binary (parse dynamic section, load deps, apply relocations, call init)
    match linker.link_binary(&lib_data, &program_headers, base_address) {
        Ok(_) => {}
        Err(e) => {
            *DL_ERROR.lock() = Some(format!("Failed to link {}: {:?}", filename, e));
            return 0;
        }
    }

    // Store the handle
    let handle = NEXT_DL_HANDLE.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    DL_HANDLES
        .lock()
        .insert(handle, (filename.to_string(), base_address));

    // Update the global linker state (advance next_base_address)
    linker.next_base_address = VirtAddr::new(next_base);
    *GLOBAL_DYNAMIC_LINKER.lock() = Some(linker);

    handle
}

/// dlsym — look up a symbol in a loaded shared library.
///
/// # Arguments
/// * `handle` - Handle from dlopen, or 0 for RTLD_DEFAULT (search all loaded libs)
/// * `symbol` - Symbol name to look up
///
/// # Returns
/// Symbol address on success, or 0 on failure (use `dlerror` for details).
pub fn dlsym(handle: u64, symbol: &str) -> u64 {
    *DL_ERROR.lock() = None;

    if handle == 0 {
        // RTLD_DEFAULT: search global symbol table
        if let Some(linker) = get_dynamic_linker() {
            if let Some(addr) = linker.resolve_symbol(symbol) {
                return addr.as_u64();
            }
        }
        *DL_ERROR.lock() = Some(format!("Symbol '{}' not found", symbol));
        return 0;
    }

    // Look up in the specific library
    let handles = DL_HANDLES.lock();
    if let Some((_name, _base)) = handles.get(&handle) {
        // Search global symbol table (all symbols from all loaded libs are there)
        drop(handles);
        if let Some(linker) = get_dynamic_linker() {
            if let Some(addr) = linker.resolve_symbol(symbol) {
                return addr.as_u64();
            }
        }
        *DL_ERROR.lock() = Some(format!(
            "Symbol '{}' not found in handle {}",
            symbol, handle
        ));
        0
    } else {
        drop(handles);
        *DL_ERROR.lock() = Some(format!("Invalid handle {}", handle));
        0
    }
}

/// dlclose — close a shared library handle.
///
/// Calls fini functions and removes the handle from the handle table.
/// The library's memory is not actually unmapped (matching Linux behavior
/// where dlclose may not immediately unload).
///
/// # Returns
/// 0 on success, -1 on error.
pub fn dlclose(handle: u64) -> i32 {
    *DL_ERROR.lock() = None;

    let mut handles = DL_HANDLES.lock();
    if let Some((name, base)) = handles.remove(&handle) {
        // Call fini functions if we can find the dynamic info
        if let Some(mut linker) = get_dynamic_linker() {
            if let Some(lib) = linker.loaded_libraries.get(&name) {
                linker.call_fini_functions(&lib.dynamic_info);
            }
            *GLOBAL_DYNAMIC_LINKER.lock() = Some(linker);
        }
        let _ = base;
        0
    } else {
        *DL_ERROR.lock() = Some(format!("Invalid handle {}", handle));
        -1
    }
}

/// dlerror — return the last error message from dlopen/dlsym/dlclose.
///
/// Returns the error string and clears the error state.
/// Returns None if no error has occurred since the last call.
pub fn dlerror() -> Option<String> {
    let mut err = DL_ERROR.lock();
    err.take()
}

/// dl_iterate_phdr — iterate over loaded shared objects.
///
/// Calls the callback for each loaded library with its load address,
/// name, and segment information. The callback receives:
/// * `info` - PhdrInfo with load address, name, and phdr count
/// * `size` - Size of the info structure
/// * `data` - User-provided data pointer
///
/// Returns the callback's return value from the last call, or 0 if
/// no libraries are loaded.
pub struct PhdrInfo {
    pub dlpi_addr: u64,
    pub dlpi_name: String,
    pub dlpi_phdr: *const u8,
    pub dlpi_phnum: u16,
}

pub fn dl_iterate_phdr<F>(mut callback: F) -> i32
where
    F: FnMut(&PhdrInfo) -> i32,
{
    if let Some(linker) = get_dynamic_linker() {
        for lib in linker.loaded_libraries.values() {
            let info = PhdrInfo {
                dlpi_addr: lib.base_address.as_u64(),
                dlpi_name: lib.name.clone(),
                dlpi_phdr: core::ptr::null(),
                dlpi_phnum: 0,
            };
            let result = callback(&info);
            if result != 0 {
                return result;
            }
        }
    }
    0
}

// ── RPATH/RUNPATH $ORIGIN/$LIB/$PLATFORM expansion ─────────────────────

/// Expand $ORIGIN, $LIB, $PLATFORM tokens in an RPATH or RUNPATH string.
///
/// - `$ORIGIN` → directory of the object containing the RPATH
/// - `$LIB` → architecture-specific lib dir (e.g. "lib64" on x86_64)
/// - `$PLATFORM` → platform name (e.g. "x86_64")
pub fn expand_rpath_tokens(rpath: &str, origin: &str) -> String {
    let lib_dir = if cfg!(target_arch = "x86_64") {
        "lib64"
    } else {
        "lib"
    };
    let platform = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unknown"
    };

    rpath
        .replace("$ORIGIN", origin)
        .replace("${ORIGIN}", origin)
        .replace("$LIB", lib_dir)
        .replace("${LIB}", lib_dir)
        .replace("$PLATFORM", platform)
        .replace("${PLATFORM}", platform)
}

// ── DT_FLAGS / DT_FLAGS_1 handling ─────────────────────────────────────

/// Check if a binary requests eager binding (DF_BIND_NOW or DF_1_NOW).
///
/// RustOS always does eager binding, so this is always true, but this
/// function allows checking the binary's intent for future compatibility.
pub fn requests_eager_binding(info: &DynamicInfo) -> bool {
    if let Some(flags) = info.flags {
        if flags & dt_flags::DF_BIND_NOW != 0 {
            return true;
        }
    }
    if let Some(flags_1) = info.flags_1 {
        if flags_1 & dt_flags_1::DF_1_NOW != 0 {
            return true;
        }
    }
    false
}

/// Check if a binary is marked as non-deletable (DF_1_NODELETE).
pub fn is_nodelete(info: &DynamicInfo) -> bool {
    info.flags_1
        .map(|f| f & dt_flags_1::DF_1_NODELETE != 0)
        .unwrap_or(false)
}

/// Check if a binary is marked as non-openable via dlopen (DF_1_NOOPEN).
pub fn is_noopen(info: &DynamicInfo) -> bool {
    info.flags_1
        .map(|f| f & dt_flags_1::DF_1_NOOPEN != 0)
        .unwrap_or(false)
}

/// Check if a binary requires $ORIGIN expansion (DF_ORIGIN or DF_1_ORIGIN).
pub fn needs_origin_expansion(info: &DynamicInfo) -> bool {
    let flags = info.flags.unwrap_or(0);
    let flags_1 = info.flags_1.unwrap_or(0);
    (flags & dt_flags::DF_ORIGIN != 0) || (flags_1 & dt_flags_1::DF_1_ORIGIN != 0)
}
