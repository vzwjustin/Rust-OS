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
use crate::vfs::{vfs_close, vfs_open, vfs_read, vfs_stat, OpenFlags};

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
    pub const DT_HASH: i64 = 4; // Symbol hash table address
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
    pub const DT_RUNPATH: i64 = 29; // Library search path
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
            _ => {
                // Ignore other tags for now
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

    /// Load a shared library file from filesystem
    ///
    /// Returns the library data if successfully loaded
    pub fn load_library_file(&self, path: &str) -> DynamicLinkerResult<Vec<u8>> {
        const MAX_LIBRARY_SIZE: usize = 64 * 1024 * 1024;

        let stat =
            vfs_stat(path).map_err(|_| DynamicLinkerError::LibraryNotFound(path.to_string()))?;

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

        let fd = vfs_open(path, OpenFlags::RDONLY, 0)
            .map_err(|_| DynamicLinkerError::LibraryNotFound(path.to_string()))?;

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

                    unsafe {
                        self.write_relocation_value(target, value)?;
                    }
                }
                relocation_types::R_X86_64_GLOB_DAT => {
                    // Symbol value: S
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());

                    // Resolve symbol by index
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
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

                    // Resolve symbol by index
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        // For eager binding, write symbol address directly
                        unsafe {
                            self.write_relocation_value(target, symbol_addr.as_u64())?;
                        }
                    } else {
                        // For lazy binding, we could write resolver stub address here
                        // For now, leave it unresolved (will be resolved on first call)
                        // This is optional - we could also error out like GLOB_DAT
                    }
                }
                relocation_types::R_X86_64_64 => {
                    // Direct 64-bit: S + A
                    let target = VirtAddr::new(base_address.as_u64() + reloc.offset.as_u64());

                    // Resolve symbol and add addend
                    if let Some(symbol_addr) = self.resolve_symbol_by_index(reloc.symbol) {
                        let value = symbol_addr.as_u64() + reloc.addend as u64;
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

        if let Some(runpath) = dynamic_info.runpath.clone() {
            self.add_runpath_entries(&runpath);
        } else if let Some(rpath) = dynamic_info.rpath.clone() {
            self.add_runpath_entries(&rpath);
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

        Ok(reloc_count)
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
        // Symbol table ends where string table begins (common layout)
        let sym_count = if strtab_offset > symtab_offset {
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
            let symbol = unsafe {
                core::ptr::read(binary_data[sym_offset..].as_ptr() as *const Elf64Symbol)
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
            let symbol = unsafe {
                core::ptr::read(binary_data[sym_offset..].as_ptr() as *const Elf64Symbol)
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
        // In a real kernel, we would check permissions first
        let ptr = addr.as_u64() as *mut u64;
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
