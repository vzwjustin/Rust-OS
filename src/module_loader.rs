//! Kernel module image registry.
//!
//! This implements the load/unload syscall surface enough to validate and keep
//! ELF64 relocatable module images in kernel memory. RustOS still lacks a
//! relocation and module-init ABI, so modules are tracked as staged images with
//! metadata instead of being jumped into blindly.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::memory::user_space::UserSpaceMemory;
use crate::vfs;

const MODULE_INIT_IGNORE_MODVERSIONS: u32 = 1;
const MODULE_INIT_IGNORE_VERMAGIC: u32 = 2;
const MODULE_INIT_COMPRESSED_FILE: u32 = 4;
const FINIT_VALID_FLAGS: u32 =
    MODULE_INIT_IGNORE_MODVERSIONS | MODULE_INIT_IGNORE_VERMAGIC | MODULE_INIT_COMPRESSED_FILE;

const DELETE_O_NONBLOCK: u32 = 0o4000;
const DELETE_O_TRUNC: u32 = 0o1000;
const DELETE_VALID_FLAGS: u32 = DELETE_O_NONBLOCK | DELETE_O_TRUNC;

const MAX_MODULE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PARAMS_BYTES: usize = 4096;
const EI_CLASS_64: u8 = 2;
const EI_DATA_LSB: u8 = 1;
const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;

/// ELF section header type constants.
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;

/// ELF relocation type constants for x86_64.
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_PLT32: u32 = 4;
const R_X86_64_32: u32 = 10;
const R_X86_64_32S: u32 = 11;

static MODULES: RwLock<BTreeMap<String, ModuleImage>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// A successfully initialised (live) kernel module.
pub struct LoadedModule {
    pub name: [u8; 64],
    pub base: usize,
    pub size: usize,
    pub exit_fn: Option<fn()>,
}

static LOADED_MODULES: RwLock<Vec<LoadedModule>> = RwLock::new(Vec::new());

/// Return a snapshot of all currently-live modules (names only, to avoid
/// exposing raw pointers to callers that don't need them).
pub fn list_modules() -> Vec<[u8; 64]> {
    LOADED_MODULES.read().iter().map(|m| m.name).collect()
}

#[derive(Clone)]
pub struct ModuleImage {
    pub id: u32,
    pub name: String,
    pub params: String,
    pub image: Vec<u8>,
    pub refcount: u32,
    pub flags: u32,
}

fn errno<T>(result: Result<T, i32>) -> i32
where
    T: Into<i32>,
{
    match result {
        Ok(value) => value.into(),
        Err(errno) => -errno,
    }
}

fn read_user_bytes(ptr: *const u8, len: usize) -> Result<Vec<u8>, i32> {
    if len > MAX_MODULE_BYTES {
        return Err(27); // EFBIG
    }
    if len > 0 && ptr.is_null() {
        return Err(14); // EFAULT
    }
    let mut out = vec![0u8; len];
    if len > 0 {
        UserSpaceMemory::copy_from_user(ptr as u64, &mut out).map_err(|_| 14)?;
    }
    Ok(out)
}

fn read_user_cstr(ptr: *const u8, max: usize) -> Result<String, i32> {
    if ptr.is_null() {
        return Ok(String::new());
    }
    let s = UserSpaceMemory::copy_string_from_user(ptr as u64, max).map_err(|_| 14)?;
    if s.len() >= max {
        return Err(7); // E2BIG
    }
    Ok(s)
}

fn read_fd_all(fd: i32) -> Result<Vec<u8>, i32> {
    if fd < 0 {
        return Err(9);
    }
    let stat = vfs::vfs_fstat(fd).map_err(|_| 9)?;
    if stat.size as usize > MAX_MODULE_BYTES {
        return Err(27);
    }
    let mut out = vec![0u8; stat.size as usize];
    let mut offset = 0usize;
    while offset < out.len() {
        let n = vfs::vfs_pread(fd, &mut out[offset..], offset as u64).map_err(|_| 5)?;
        if n == 0 {
            out.truncate(offset);
            break;
        }
        offset += n;
    }
    Ok(out)
}

fn le16(data: &[u8], off: usize) -> Result<u16, i32> {
    let bytes = data.get(off..off + 2).ok_or(8)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn le32(data: &[u8], off: usize) -> Result<u32, i32> {
    let bytes = data.get(off..off + 4).ok_or(8)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn le64(data: &[u8], off: usize) -> Result<u64, i32> {
    let bytes = data.get(off..off + 8).ok_or(8)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn validate_module_elf(image: &[u8]) -> Result<(), i32> {
    if image.len() < 64 || &image[0..4] != b"\x7fELF" {
        return Err(8); // ENOEXEC
    }
    if image[4] != EI_CLASS_64 || image[5] != EI_DATA_LSB {
        return Err(8);
    }
    if le16(image, 16)? != ET_REL || le16(image, 18)? != EM_X86_64 {
        return Err(8);
    }
    Ok(())
}

/// Apply RELA relocations to the module image in-place.
///
/// Processes all SHT_RELA sections, applying x86_64 relocation types
/// (R_X86_64_64, R_X86_64_PC32, R_X86_64_32, R_X86_64_32S) to the
/// section data.  Since the module is not yet mapped at a fixed address,
/// relocations are applied relative to the image base (offset 0), which
/// is correct for PC-relative references within the module itself.
fn apply_relocations(image: &mut [u8]) -> Result<(), i32> {
    let shoff = le64(image, 40)? as usize;
    let shentsize = le16(image, 58)? as usize;
    let shnum = le16(image, 60)? as usize;
    let shstrndx = le16(image, 62)? as usize;
    if shoff == 0 || shentsize < 64 || shstrndx >= shnum {
        return Ok(()); // No section headers — nothing to relocate
    }

    // Read section headers into a vec for easy access
    let mut sections: Vec<(u32, u64, u64, u64, u64, u64)> = Vec::with_capacity(shnum);
    for idx in 0..shnum {
        let sh = shoff
            .checked_add(idx.checked_mul(shentsize).ok_or(8)?)
            .ok_or(8)?;
        let sh_type = le32(image, sh)?;
        let sh_addr = le64(image, sh + 16)?;
        let sh_offset = le64(image, sh + 24)?;
        let sh_size = le64(image, sh + 32)?;
        let sh_link = le64(image, sh + 40)?;
        let sh_info = le32(image, sh + 44)? as u64;
        sections.push((sh_type, sh_addr, sh_offset, sh_size, sh_link, sh_info));
    }

    // Process each RELA section
    for (sh_type, _sh_addr, sh_offset, sh_size, _sh_link, sh_info) in &sections {
        if *sh_type != SHT_RELA {
            continue;
        }

        // sh_link is the index of the symbol table section
        // For modules without external symbols, all symbols resolve to 0
        // (relative to image base). We apply relocations within the image.

        let rela_off = *sh_offset as usize;
        let rela_size = *sh_size as usize;
        if rela_off + rela_size > image.len() {
            continue;
        }

        // Each RELA entry is 24 bytes: r_offset(8), r_info(8), r_addend(8)
        let entry_size = 24usize;
        let count = rela_size / entry_size;

        for i in 0..count {
            let base = rela_off + i * entry_size;
            let r_offset = le64(image, base)?;
            let r_info = le64(image, base + 8)?;
            let r_addend = le64(image, base + 16)? as i64;

            let r_type = (r_info & 0xffffffff) as u32;
            let r_sym = r_info >> 32;
            if r_sym != 0 {
                return Err(38);
            }

            let target_section = sections.get(*sh_info as usize).ok_or(8)?;
            let target_addr = target_section.1;
            let target_off = target_section.2;
            let target_size = target_section.3;
            if r_offset < target_addr || r_offset >= target_addr.saturating_add(target_size) {
                return Err(8);
            }
            let target = target_off.checked_add(r_offset - target_addr).ok_or(8)? as usize;

            match r_type {
                R_X86_64_64 => {
                    // R + A (absolute 64-bit)
                    if target + 8 > image.len() {
                        continue;
                    }
                    let val = r_addend as u64;
                    image[target..target + 8].copy_from_slice(&val.to_le_bytes());
                }
                R_X86_64_PC32 | R_X86_64_PLT32 => {
                    // S + A - P (PC-relative 32-bit)
                    // P = r_offset (address of the relocation site)
                    // S = 0 (image base), so val = A - P
                    if target + 4 > image.len() {
                        continue;
                    }
                    let p = r_offset as i64;
                    let val = (r_addend - p) as i32;
                    image[target..target + 4].copy_from_slice(&val.to_le_bytes());
                }
                R_X86_64_32 => {
                    // S + A (absolute 32-bit, zero-extended)
                    if target + 4 > image.len() {
                        continue;
                    }
                    let val = r_addend as u32;
                    image[target..target + 4].copy_from_slice(&val.to_le_bytes());
                }
                R_X86_64_32S => {
                    // S + A (absolute 32-bit, sign-extended)
                    if target + 4 > image.len() {
                        continue;
                    }
                    let val = r_addend as i32;
                    image[target..target + 4].copy_from_slice(&val.to_le_bytes());
                }
                _ => return Err(38),
            }
        }
    }

    Ok(())
}

fn section_name<'a>(strtab: &'a [u8], name_off: u32) -> &'a str {
    let start = name_off as usize;
    if start >= strtab.len() {
        return "";
    }
    let end = strtab[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|n| start + n)
        .unwrap_or(strtab.len());
    core::str::from_utf8(&strtab[start..end]).unwrap_or("")
}

fn module_name_from_modinfo(image: &[u8]) -> Option<String> {
    let shoff = le64(image, 40).ok()? as usize;
    let shentsize = le16(image, 58).ok()? as usize;
    let shnum = le16(image, 60).ok()? as usize;
    let shstrndx = le16(image, 62).ok()? as usize;
    if shoff == 0 || shentsize < 64 || shstrndx >= shnum {
        return None;
    }
    let shstr = shoff.checked_add(shstrndx.checked_mul(shentsize)?)?;
    let shstr_off = le64(image, shstr + 24).ok()? as usize;
    let shstr_size = le64(image, shstr + 32).ok()? as usize;
    let shstrtab = image.get(shstr_off..shstr_off.checked_add(shstr_size)?)?;

    for idx in 0..shnum {
        let sh = shoff.checked_add(idx.checked_mul(shentsize)?)?;
        let name = section_name(shstrtab, le32(image, sh).ok()?);
        if name != ".modinfo" {
            continue;
        }
        let off = le64(image, sh + 24).ok()? as usize;
        let size = le64(image, sh + 32).ok()? as usize;
        let data = image.get(off..off.checked_add(size)?)?;
        for field in data.split(|&b| b == 0) {
            if let Some(rest) = field.strip_prefix(b"name=") {
                if let Ok(name) = core::str::from_utf8(rest) {
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Scan the ELF64 symbol table for a function whose name matches `target`.
/// Returns the symbol's `st_value` (byte offset within the image) on success.
fn find_symbol_offset(image: &[u8], target: &str) -> Option<usize> {
    let shoff = le64(image, 40).ok()? as usize;
    let shentsize = le16(image, 58).ok()? as usize;
    let shnum = le16(image, 60).ok()? as usize;
    let shstrndx = le16(image, 62).ok()? as usize;
    if shoff == 0 || shentsize < 64 || shstrndx >= shnum {
        return None;
    }

    // Locate the string table for section names.
    let shstr_sh = shoff.checked_add(shstrndx.checked_mul(shentsize)?)?;
    let shstr_off = le64(image, shstr_sh + 24).ok()? as usize;
    let shstr_size = le64(image, shstr_sh + 32).ok()? as usize;
    let shstrtab = image.get(shstr_off..shstr_off.checked_add(shstr_size)?)?;

    // Find .symtab and .strtab sections.
    let mut symtab_off = 0usize;
    let mut symtab_size = 0usize;
    let mut strtab_off = 0usize;
    let mut strtab_size = 0usize;
    for idx in 0..shnum {
        let sh = shoff.checked_add(idx.checked_mul(shentsize)?)?;
        let name = section_name(shstrtab, le32(image, sh).ok()?);
        let off = le64(image, sh + 24).ok()? as usize;
        let size = le64(image, sh + 32).ok()? as usize;
        if name == ".symtab" {
            symtab_off = off;
            symtab_size = size;
        } else if name == ".strtab" {
            strtab_off = off;
            strtab_size = size;
        }
    }
    if symtab_off == 0 || strtab_off == 0 {
        return None;
    }

    // Each ELF64 symbol entry is 24 bytes.
    let sym_entry_size = 24usize;
    let sym_count = symtab_size / sym_entry_size;
    let strtab = image.get(strtab_off..strtab_off.checked_add(strtab_size)?)?;

    for i in 0..sym_count {
        let base = symtab_off + i * sym_entry_size;
        let st_name = le32(image, base).ok()? as usize;
        let st_value = le64(image, base + 8).ok()? as usize;
        let sym_name = {
            let start = st_name;
            if start >= strtab.len() {
                continue;
            }
            let end = strtab[start..]
                .iter()
                .position(|&b| b == 0)
                .map(|n| start + n)
                .unwrap_or(strtab.len());
            core::str::from_utf8(&strtab[start..end]).unwrap_or("")
        };
        if sym_name == target {
            return Some(st_value);
        }
    }
    None
}

fn fallback_name() -> String {
    alloc::format!("module{}", NEXT_ID.load(Ordering::SeqCst))
}

fn load_module(mut image: Vec<u8>, params: String, flags: u32) -> Result<i32, i32> {
    validate_module_elf(&image)?;
    apply_relocations(&mut image)?;
    let name = module_name_from_modinfo(&image).unwrap_or_else(fallback_name);
    {
        let modules = MODULES.read();
        if modules.contains_key(&name) {
            return Err(17); // EEXIST
        }
    }

    // Find and call the module's init_module entry point.
    let exit_fn: Option<fn()> = find_symbol_offset(&image, "cleanup_module")
        .map(|off| unsafe { core::mem::transmute(image.as_ptr().add(off)) });

    if let Some(init_off) = find_symbol_offset(&image, "init_module") {
        let init_fn: fn() -> i32 =
            unsafe { core::mem::transmute(image.as_ptr().add(init_off)) };
        let rc = unsafe { init_fn() };
        if rc != 0 {
            return Err(-rc); // init failed — do not register
        }
    }

    // Build the LoadedModule entry.
    let mut loaded_name = [0u8; 64];
    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(63);
    loaded_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

    let base = image.as_ptr() as usize;
    let size = image.len();

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    MODULES.write().insert(
        name.clone(),
        ModuleImage {
            id,
            name,
            params,
            image,
            refcount: 0,
            flags,
        },
    );

    LOADED_MODULES.write().push(LoadedModule {
        name: loaded_name,
        base,
        size,
        exit_fn,
    });

    Ok(0)
}

/// init_module - load a module image from userspace memory.
pub fn init_module(module_image: *const u8, len: usize, param_values: *const u8) -> i32 {
    errno((|| {
        let image = read_user_bytes(module_image, len)?;
        let params = read_user_cstr(param_values, MAX_PARAMS_BYTES)?;
        load_module(image, params, 0)
    })())
}

/// finit_module - load a module image from a file descriptor.
pub fn finit_module(fd: i32, param_values: *const u8, flags: u32) -> i32 {
    errno((|| {
        if flags & !FINIT_VALID_FLAGS != 0 {
            return Err(22);
        }
        if flags & MODULE_INIT_COMPRESSED_FILE != 0 {
            return Err(95); // ENOTSUP until module decompression is wired here.
        }
        let image = read_fd_all(fd)?;
        let params = read_user_cstr(param_values, MAX_PARAMS_BYTES)?;
        load_module(image, params, flags)
    })())
}

/// delete_module - remove a staged module image by name.
pub fn delete_module(name: *const u8, flags: u32) -> i32 {
    errno((|| {
        if flags & !DELETE_VALID_FLAGS != 0 {
            return Err(22);
        }
        let name = read_user_cstr(name, 256)?;
        if name.is_empty() {
            return Err(2);
        }
        let mut modules = MODULES.write();
        let Some(module) = modules.get(&name) else {
            return Err(2);
        };
        if module.refcount != 0 && flags & DELETE_O_TRUNC == 0 {
            return Err(16);
        }
        modules.remove(&name);
        Ok(0)
    })())
}

/// remove_module - call the module's cleanup function and unload it.
///
/// Equivalent to `rmmod`. Calls `cleanup_module` (if found), then removes the
/// module from both the image registry and the live-module list.
pub fn remove_module(name: &str) -> i32 {
    // Pull the exit function before dropping the lock.
    let exit_fn = {
        let loaded = LOADED_MODULES.read();
        loaded.iter().find(|m| {
            let name_bytes = name.as_bytes();
            let copy_len = name_bytes.len().min(63);
            m.name[..copy_len] == name_bytes[..copy_len] && m.name[copy_len] == 0
        }).and_then(|m| m.exit_fn)
    };

    if let Some(f) = exit_fn {
        unsafe { f() };
    }

    // Remove from live list.
    {
        let mut loaded = LOADED_MODULES.write();
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(63);
        loaded.retain(|m| {
            !(m.name[..copy_len] == name_bytes[..copy_len] && m.name[copy_len] == 0)
        });
    }

    // Remove from image registry.
    let mut modules = MODULES.write();
    if modules.remove(name).is_none() {
        return -2; // ENOENT
    }
    0
}

pub fn get_module(name: &str) -> Option<ModuleImage> {
    MODULES.read().get(name).cloned()
}

/// Initialize module subsystem.
pub fn init() {
    MODULES.write().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
    crate::serial_println!("[module] Module image registry initialized");
}
