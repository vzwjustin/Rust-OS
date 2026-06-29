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

static MODULES: RwLock<BTreeMap<String, ModuleImage>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

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

fn fallback_name() -> String {
    alloc::format!("module{}", NEXT_ID.load(Ordering::SeqCst))
}

fn load_module(image: Vec<u8>, params: String, flags: u32) -> Result<i32, i32> {
    validate_module_elf(&image)?;
    let name = module_name_from_modinfo(&image).unwrap_or_else(fallback_name);
    let mut modules = MODULES.write();
    if modules.contains_key(&name) {
        return Err(17); // EEXIST
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    modules.insert(
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

pub fn get_module(name: &str) -> Option<ModuleImage> {
    MODULES.read().get(name).cloned()
}

/// Initialize module subsystem.
pub fn init() {
    MODULES.write().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
    crate::serial_println!("[module] Module image registry initialized");
}
