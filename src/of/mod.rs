//! Flattened device tree (FDT / DTB) parser for ARM and other OF platforms.
//!
//! Implements the standard libfdt token stream layout so DTBs can be parsed
//! even on x86 builds where no DTB is supplied at boot.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// FDT magic (`0xd00dfeed` big-endian on wire, stored as BE in header).
const FDT_MAGIC: u32 = 0xD00D_FEED;

const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_NOP: u32 = 0x0000_0004;
const FDT_END: u32 = 0x0000_0009;

/// Parsed FDT header (big-endian fields stored as read from blob).
#[derive(Debug, Clone, Copy)]
pub struct FdtHeader {
    pub magic: u32,
    pub totalsize: u32,
    pub off_dt_struct: u32,
    pub off_dt_strings: u32,
    pub off_mem_rsvmap: u32,
    pub version: u32,
    pub last_comp_version: u32,
    pub boot_cpuid_phys: u32,
    pub size_dt_strings: u32,
    pub size_dt_struct: u32,
}

/// One property on a device-tree node.
#[derive(Debug, Clone)]
pub struct FdtProperty {
    pub name: String,
    pub value: Vec<u8>,
}

/// One node in the parsed tree.
#[derive(Debug, Clone)]
pub struct FdtNode {
    pub name: String,
    pub properties: BTreeMap<String, FdtProperty>,
    pub children: Vec<FdtNode>,
}

/// Fully parsed device tree.
#[derive(Debug, Clone)]
pub struct DeviceTree {
    pub header: FdtHeader,
    pub root: Option<FdtNode>,
    pub mem_reserve: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdtError {
    BadMagic,
    Truncated,
    BadStructure,
    BadOffset,
}

impl DeviceTree {
    /// Parse a DTB blob at `ptr` (must remain valid for the duration of parsing).
    pub unsafe fn from_blob(ptr: *const u8) -> Result<Self, FdtError> {
        if ptr.is_null() {
            return Err(FdtError::Truncated);
        }
        let header = read_header(ptr)?;
        if header.magic != FDT_MAGIC {
            return Err(FdtError::BadMagic);
        }
        if header.totalsize as usize > 64 * 1024 * 1024 {
            return Err(FdtError::Truncated);
        }

        let mem_reserve = parse_mem_reserve(ptr, header.off_mem_rsvmap as usize)?;
        let strings_base = header.off_dt_strings as usize;
        let struct_base = header.off_dt_struct as usize;
        let struct_end = struct_base + header.size_dt_struct as usize;
        if struct_end > header.totalsize as usize {
            return Err(FdtError::Truncated);
        }

        let (root, _) = parse_nodes(ptr, struct_base, struct_end, strings_base)?;
        Ok(DeviceTree {
            header,
            root,
            mem_reserve,
        })
    }

    /// Lookup a property on the first node matching `path` (`"/soc/uart@0"`).
    pub fn property(&self, path: &str, prop: &str) -> Option<&[u8]> {
        let node = self.find_node(path)?;
        node.properties.get(prop).map(|p| p.value.as_slice())
    }

    pub fn find_node(&self, path: &str) -> Option<&FdtNode> {
        let root = self.root.as_ref()?;
        if path == "/" || path.is_empty() {
            return Some(root);
        }
        let trimmed = path.trim_start_matches('/');
        let mut current = root;
        for component in trimmed.split('/') {
            if component.is_empty() {
                continue;
            }
            current = current.children.iter().find(|c| c.name == component)?;
        }
        Some(current)
    }

    /// Return `#address-cells` / `#size-cells` for a node (defaults 2/1).
    pub fn address_cells(&self, path: &str) -> u32 {
        self.property(path, "#address-cells")
            .and_then(|v| read_be_u32(v))
            .unwrap_or(2)
    }

    pub fn size_cells(&self, path: &str) -> u32 {
        self.property(path, "#size-cells")
            .and_then(|v| read_be_u32(v))
            .unwrap_or(1)
    }
}

fn read_header(ptr: *const u8) -> Result<FdtHeader, FdtError> {
    if ptr.is_null() {
        return Err(FdtError::Truncated);
    }
    Ok(FdtHeader {
        magic: read_be_u32_at(ptr, 0).ok_or(FdtError::Truncated)?,
        totalsize: read_be_u32_at(ptr, 4).ok_or(FdtError::Truncated)?,
        off_dt_struct: read_be_u32_at(ptr, 8).ok_or(FdtError::Truncated)?,
        off_dt_strings: read_be_u32_at(ptr, 12).ok_or(FdtError::Truncated)?,
        off_mem_rsvmap: read_be_u32_at(ptr, 16).ok_or(FdtError::Truncated)?,
        version: read_be_u32_at(ptr, 20).ok_or(FdtError::Truncated)?,
        last_comp_version: read_be_u32_at(ptr, 24).ok_or(FdtError::Truncated)?,
        boot_cpuid_phys: read_be_u32_at(ptr, 28).ok_or(FdtError::Truncated)?,
        size_dt_strings: read_be_u32_at(ptr, 32).ok_or(FdtError::Truncated)?,
        size_dt_struct: read_be_u32_at(ptr, 36).ok_or(FdtError::Truncated)?,
    })
}

fn parse_mem_reserve(ptr: *const u8, offset: usize) -> Result<Vec<(u64, u64)>, FdtError> {
    let mut out = Vec::new();
    let mut off = offset;
    loop {
        let addr = read_be_u64_at(ptr, off).ok_or(FdtError::Truncated)?;
        let size = read_be_u64_at(ptr, off + 8).ok_or(FdtError::Truncated)?;
        off += 16;
        if addr == 0 && size == 0 {
            break;
        }
        out.push((addr, size));
    }
    Ok(out)
}

fn parse_nodes(
    ptr: *const u8,
    mut offset: usize,
    end: usize,
    strings_base: usize,
) -> Result<(Option<FdtNode>, usize), FdtError> {
    let mut stack: Vec<FdtNode> = Vec::new();

    while offset + 4 <= end {
        let token = read_be_u32_at(ptr, offset).ok_or(FdtError::BadStructure)?;
        offset += 4;

        match token {
            FDT_BEGIN_NODE => {
                let name = read_cstring(ptr, offset, end)?;
                offset = align4(offset + name.len() + 1);
                stack.push(FdtNode {
                    name,
                    properties: BTreeMap::new(),
                    children: Vec::new(),
                });
            }
            FDT_END_NODE => {
                let node = stack.pop().ok_or(FdtError::BadStructure)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    return Ok((Some(node), offset));
                }
            }
            FDT_PROP => {
                let len = read_be_u32_at(ptr, offset).ok_or(FdtError::BadStructure)? as usize;
                offset += 4;
                let nameoff = read_be_u32_at(ptr, offset).ok_or(FdtError::BadStructure)? as usize;
                offset += 4;
                let value = read_bytes(ptr, offset, len).ok_or(FdtError::Truncated)?;
                offset = align4(offset + len);
                let prop_name = read_string_at(ptr, strings_base + nameoff, end)?;
                if let Some(node) = stack.last_mut() {
                    node.properties.insert(
                        prop_name.clone(),
                        FdtProperty {
                            name: prop_name,
                            value,
                        },
                    );
                }
            }
            FDT_NOP => {}
            FDT_END => break,
            _ => return Err(FdtError::BadStructure),
        }
    }
    Ok((None, offset))
}

fn read_cstring(ptr: *const u8, offset: usize, end: usize) -> Result<String, FdtError> {
    let mut bytes = Vec::new();
    let mut off = offset;
    while off < end {
        let b = unsafe { *ptr.add(off) };
        off += 1;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_string_at(ptr: *const u8, offset: usize, end: usize) -> Result<String, FdtError> {
    if offset >= end {
        return Err(FdtError::BadOffset);
    }
    read_cstring(ptr, offset, end)
}

fn read_bytes(ptr: *const u8, offset: usize, len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(unsafe { *ptr.add(offset + i) });
    }
    Some(out)
}

fn read_be_u32(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_be_u32_at(ptr: *const u8, offset: usize) -> Option<u32> {
    if offset + 4 > isize::MAX as usize {
        return None;
    }
    Some(u32::from_be_bytes(unsafe {
        [
            *ptr.add(offset),
            *ptr.add(offset + 1),
            *ptr.add(offset + 2),
            *ptr.add(offset + 3),
        ]
    }))
}

fn read_be_u64_at(ptr: *const u8, offset: usize) -> Option<u64> {
    let hi = read_be_u32_at(ptr, offset)? as u64;
    let lo = read_be_u32_at(ptr, offset + 4)? as u64;
    Some((hi << 32) | lo)
}

fn align4(offset: usize) -> usize {
    (offset + 3) & !3
}

static PARSED: spin::RwLock<Option<DeviceTree>> = spin::RwLock::new(None);

/// Parse and cache a DTB pointer (no-op when `ptr` is null).
/// # Safety
/// The caller must ensure `ptr` is either null or points to a valid
/// device tree blob (DTB) in mapped memory.
pub unsafe fn init_from_dtb(ptr: *const u8) -> bool {
    if ptr.is_null() {
        crate::serial_println!("[of] no device tree provided");
        return false;
    }
    match DeviceTree::from_blob(ptr) {
        Ok(tree) => {
            let node_count = count_nodes(tree.root.as_ref());
            *PARSED.write() = Some(tree);
            crate::serial_println!("[of] device tree parsed ({} nodes)", node_count);
            true
        }
        Err(e) => {
            crate::serial_println!("[of] device tree parse failed: {:?}", e);
            false
        }
    }
}

fn count_nodes(node: Option<&FdtNode>) -> usize {
    node.map(|n| {
        1 + n
            .children
            .iter()
            .map(|c| count_nodes(Some(c)))
            .sum::<usize>()
    })
    .unwrap_or(0)
}

pub fn device_tree() -> Option<DeviceTree> {
    PARSED.read().clone()
}

pub fn init() {
    crate::serial_println!("[of] flattened device tree support ready");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_dtb_roundtrip() {
        // Minimal valid DTB: header + empty root + END.
        let mut blob = vec![0u8; 64];
        write_be32(&mut blob, 0, FDT_MAGIC);
        write_be32(&mut blob, 4, 64);
        write_be32(&mut blob, 8, 40); // off_dt_struct
        write_be32(&mut blob, 12, 56); // off_dt_strings
        write_be32(&mut blob, 16, 28); // off_mem_rsvmap
        write_be32(&mut blob, 20, 17); // version
        write_be32(&mut blob, 24, 16);
        write_be32(&mut blob, 36, 16); // size_dt_struct
        write_be32(&mut blob, 40, FDT_BEGIN_NODE);
        blob[44] = 0; // empty root name
        write_be32(&mut blob, 48, FDT_END_NODE);
        write_be32(&mut blob, 52, FDT_END);

        let tree = unsafe { DeviceTree::from_blob(blob.as_ptr()) }.expect("parse");
        assert!(tree.root.is_some());
        assert_eq!(tree.root.as_ref().unwrap().name, "");
    }

    fn write_be32(buf: &mut [u8], off: usize, val: u32) {
        let bytes = val.to_be_bytes();
        buf[off..off + 4].copy_from_slice(&bytes);
    }
}
