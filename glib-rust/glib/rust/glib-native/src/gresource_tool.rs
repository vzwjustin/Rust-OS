//! gresource-tool matching `gio/gresource-tool.c`.
//!
//! List and extract resources from compiled GResource bundles.

use crate::gresource::{Resource, ResourceLookupFlags};
use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Load a resource bundle from raw bytes.
///
/// Parses the GVDB (GVariant Database) binary format used by GResource bundles.
/// The format consists of:
/// - Header: signature[2], version, options, root pointer
/// - Hash table: bloom filter, buckets, hash items
/// - Key strings and values stored in the data region
///
/// Each hash item has a type byte: 'v' for value (GVariant), 'L' for container.
/// Values are stored as GVariant tuples of (size: u32 LE, flags: u32 LE, data: [u8]).
pub fn load_resource(data: &[u8]) -> Result<Resource, String> {
    if data.is_empty() {
        return Err("empty resource data".into());
    }

    if data.len() < 24 {
        return Err("resource data too small for GVDB header".into());
    }

    let entries = parse_gvdb(data)?;
    Ok(Resource::from_data(entries))
}

/// GVDB file header (24 bytes).
struct GvdbHeader {
    signature: [u32; 2],
    version: u32,
    options: u32,
    root_start: u32,
    root_end: u32,
}

/// GVDB hash item (20 bytes).
struct GvdbHashItem {
    hash_value: u32,
    parent: u32,
    key_start: u32,
    key_size: u16,
    item_type: u8,
    value_start: u32,
    value_end: u32,
}

const GVDB_SIGNATURE0: u32 = 1918981703;
const GVDB_SIGNATURE1: u32 = 1953390953;

fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, String> {
    if offset + 4 > data.len() {
        return Err("unexpected end of data".into());
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, String> {
    if offset + 2 > data.len() {
        return Err("unexpected end of data".into());
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn parse_gvdb_header(data: &[u8]) -> Result<GvdbHeader, String> {
    let sig0 = read_u32_le(data, 0)?;
    let sig1 = read_u32_le(data, 4)?;
    if sig0 != GVDB_SIGNATURE0 && sig0 != u32::swap_bytes(GVDB_SIGNATURE0) {
        return Err(format!("invalid GVDB signature0: {:#x}", sig0));
    }
    if sig1 != GVDB_SIGNATURE1 && sig1 != u32::swap_bytes(GVDB_SIGNATURE1) {
        return Err(format!("invalid GVDB signature1: {:#x}", sig1));
    }

    let version = read_u32_le(data, 8)?;
    let options = read_u32_le(data, 12)?;
    let root_start = read_u32_le(data, 16)?;
    let root_end = read_u32_le(data, 20)?;

    Ok(GvdbHeader {
        signature: [sig0, sig1],
        version,
        options,
        root_start,
        root_end,
    })
}

fn parse_gvdb_hash_item(data: &[u8], offset: usize) -> Result<GvdbHashItem, String> {
    if offset + 20 > data.len() {
        return Err("hash item extends beyond data".into());
    }

    let hash_value = read_u32_le(data, offset)?;
    let parent = read_u32_le(data, offset + 4)?;
    let key_start = read_u32_le(data, offset + 8)?;
    let key_size = read_u16_le(data, offset + 12)?;
    let item_type = data[offset + 14];

    let value_start = read_u32_le(data, offset + 16)?;
    let value_end = read_u32_le(data, offset + 20 - 4)?;

    Ok(GvdbHashItem {
        hash_value,
        parent,
        key_start,
        key_size,
        item_type,
        value_start,
        value_end,
    })
}

fn parse_gvdb(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let header = parse_gvdb_header(data)?;

    let root_start = header.root_start as usize;
    let root_end = header.root_end as usize;

    if root_start >= data.len() || root_end > data.len() || root_end < root_start {
        return Err("invalid root pointer range".into());
    }

    let root_size = root_end - root_start;
    if root_size < 8 {
        return Err("root hash table too small".into());
    }

    let n_bloom_words = read_u32_le(data, root_start)?;
    let n_buckets = read_u32_le(data, root_start + 4)?;

    let bloom_size = n_bloom_words as usize * 4;
    let buckets_size = n_buckets as usize * 4;

    let hash_items_start = root_start + 8 + bloom_size + buckets_size;
    let hash_items_end = root_end;
    if hash_items_start > data.len() || hash_items_end > data.len() {
        return Err("hash items region out of bounds".into());
    }

    let hash_items_size = hash_items_end - hash_items_start;
    let n_items = hash_items_size / 20;

    let mut entries = BTreeMap::new();

    for i in 0..n_items {
        let item_offset = hash_items_start + i * 20;
        let item = match parse_gvdb_hash_item(data, item_offset) {
            Ok(it) => it,
            Err(_) => continue,
        };

        if item.key_size == 0 {
            continue;
        }

        let key_end = item.key_start as usize + item.key_size as usize;
        if key_end > data.len() {
            continue;
        }

        let key_bytes = &data[item.key_start as usize..key_end];
        let key = match core::str::from_utf8(key_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };

        if item.item_type == b'v' {
            if let Ok((resource_data, _flags)) = parse_gresource_value(data, item.value_start as usize, item.value_end as usize) {
                entries.insert(key, resource_data);
            }
        } else if item.item_type == b'L' {
            // Container: recurse into sub-table
            if let Ok(sub_entries) = parse_gvdb_subtable(data, item.value_start as usize, item.value_end as usize) {
                for (sub_path, sub_data) in sub_entries {
                    let full_path = if key.ends_with('/') {
                        format!("{}{}", key, sub_path.trim_start_matches('/'))
                    } else {
                        format!("{}/{}", key, sub_path.trim_start_matches('/'))
                    };
                    entries.insert(full_path, sub_data);
                }
            }
        }
    }

    if entries.is_empty() {
        return Err("no valid entries found in GVDB file".into());
    }

    Ok(entries)
}

fn parse_gvdb_subtable(data: &[u8], start: usize, end: usize) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if start >= data.len() || end > data.len() || end < start {
        return Err("invalid subtable range".into());
    }

    let table_size = end - start;
    if table_size < 8 {
        return Err("subtable too small".into());
    }

    let n_bloom_words = read_u32_le(data, start)?;
    let n_buckets = read_u32_le(data, start + 4)?;

    let bloom_size = n_bloom_words as usize * 4;
    let buckets_size = n_buckets as usize * 4;

    let hash_items_start = start + 8 + bloom_size + buckets_size;
    let hash_items_end = end;

    if hash_items_start > data.len() || hash_items_end > data.len() {
        return Err("subtable hash items out of bounds".into());
    }

    let hash_items_size = hash_items_end - hash_items_start;
    let n_items = hash_items_size / 20;

    let mut entries = BTreeMap::new();

    for i in 0..n_items {
        let item_offset = hash_items_start + i * 20;
        let item = match parse_gvdb_hash_item(data, item_offset) {
            Ok(it) => it,
            Err(_) => continue,
        };

        if item.key_size == 0 {
            continue;
        }

        let key_end = item.key_start as usize + item.key_size as usize;
        if key_end > data.len() {
            continue;
        }

        let key_bytes = &data[item.key_start as usize..key_end];
        let key = match core::str::from_utf8(key_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };

        if item.item_type == b'v' {
            if let Ok((resource_data, _flags)) = parse_gresource_value(data, item.value_start as usize, item.value_end as usize) {
                entries.insert(key, resource_data);
            }
        }
    }

    Ok(entries)
}

/// Parse a GResource value (GVariant tuple: (uu@ay) = size, flags, byte array).
///
/// The value data is a GVariant containing a tuple of:
/// - u32 LE: uncompressed size
/// - u32 LE: flags (0 = uncompressed)
/// - byte array: the actual resource data
fn parse_gresource_value(data: &[u8], start: usize, end: usize) -> Result<(Vec<u8>, u32), String> {
    if start >= data.len() || end > data.len() || end < start {
        return Err("invalid value range".into());
    }

    let value_data = &data[start..end];
    if value_data.len() < 8 {
        return Err("value too small for GResource tuple".into());
    }

    let size = u32::from_le_bytes([value_data[0], value_data[1], value_data[2], value_data[3]]);
    let flags = u32::from_le_bytes([value_data[4], value_data[5], value_data[6], value_data[7]]);

    // The byte array payload starts after the two u32s.
    // In GVariant format, the array data is the remaining bytes (minus the trailing NUL).
    let payload_start = 8;
    let payload_end = value_data.len();
    if payload_end > payload_start {
        let payload = &value_data[payload_start..payload_end];
        // Strip trailing NUL byte that GResource adds for non-compressed files
        let actual_payload = if flags == 0 && payload.last() == Some(&0) {
            &payload[..payload.len() - 1]
        } else {
            payload
        };
        Ok((actual_payload.to_vec(), flags))
    } else {
        Ok((Vec::new(), flags))
    }
}

/// List resource paths under `prefix`.
pub fn list_resources(resource: &Resource, prefix: &str, details: bool) -> Vec<String> {
    let mut lines = Vec::new();
    match resource.enumerate_children(prefix, ResourceLookupFlags::None) {
        Ok(children) => {
            for child in children {
                let path = format!("{prefix}{child}");
                if details {
                    if let Ok((size, flags)) = resource.get_info(&path, ResourceLookupFlags::None) {
                        lines.push(format!("{path}\tsize={size}\tflags={flags}"));
                    } else {
                        lines.push(path);
                    }
                } else {
                    lines.push(path);
                }
            }
        }
        Err(_) => {}
    }
    lines.sort();
    lines
}

/// Extract resource data at `path`.
pub fn extract_resource(resource: &Resource, path: &str) -> Result<Vec<u8>, String> {
    resource
        .lookup_data(path, ResourceLookupFlags::None)
        .map(|b| b.data().to_vec())
        .map_err(|e| e.message().to_owned())
}

/// Entry point for `gresource`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args[0] == "help" || args.contains(&"--help") {
        gwarn!("Usage: gresource {{list,extract}} FILE [PATH]");
        return if args.is_empty() { 1 } else { 0 };
    }
    match args[0] {
        "list" => {
            if args.len() < 2 {
                return 1;
            }
            let details = args.contains(&"--details");
            let resource = match load_resource(args[1].as_bytes()) {
                Ok(r) => r,
                Err(msg) => {
                    gwarn!("{msg}");
                    return 1;
                }
            };
            let prefix = args.get(2).copied().unwrap_or("/");
            for line in list_resources(&resource, prefix, details) {
                gwarn!("{line}");
            }
            0
        }
        "extract" => {
            if args.len() < 3 {
                return 1;
            }
            let resource = match load_resource(args[1].as_bytes()) {
                Ok(r) => r,
                Err(msg) => {
                    gwarn!("{msg}");
                    return 1;
                }
            };
            match extract_resource(&resource, args[2]) {
                Ok(data) => {
                    gwarn!("extracted {} bytes", data.len());
                    0
                }
                Err(msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal GVDB file with one entry for testing.
    fn build_test_gvdb() -> Vec<u8> {
        let key = "/test/resource.txt";
        let payload = b"hello world";

        // GResource value: size (u32 LE) + flags (u32 LE) + data + NUL
        let value_data: Vec<u8> = {
            let mut v = Vec::new();
            v.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // size
            v.extend_from_slice(&0u32.to_le_bytes()); // flags
            v.extend_from_slice(payload);
            v.push(0); // trailing NUL
            v
        };

        let value_start = 24; // header(24) + hash_header(8) + hash_item(20) = 52, but value after
        let header_size = 24; // signature(8) + version(4) + options(4) + root pointer(8)
        let hash_header_size = 8;
        let hash_item_size = 20;
        let root_start = header_size;
        let root_end = root_start + hash_header_size + hash_item_size;
        let value_offset = root_end;
        let key_offset = value_offset + value_data.len();

        let mut data = Vec::new();

        // Header
        data.extend_from_slice(&GVDB_SIGNATURE0.to_le_bytes());
        data.extend_from_slice(&GVDB_SIGNATURE1.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // version
        data.extend_from_slice(&0u32.to_le_bytes()); // options
        data.extend_from_slice(&(root_start as u32).to_le_bytes()); // root start
        data.extend_from_slice(&(root_end as u32).to_le_bytes()); // root end

        // Hash header
        data.extend_from_slice(&0u32.to_le_bytes()); // n_bloom_words
        data.extend_from_slice(&0u32.to_le_bytes()); // n_buckets

        // Hash item
        data.extend_from_slice(&0u32.to_le_bytes()); // hash_value
        data.extend_from_slice(&0u32.to_le_bytes()); // parent
        data.extend_from_slice(&(key_offset as u32).to_le_bytes()); // key_start
        data.extend_from_slice(&(key.len() as u16).to_le_bytes()); // key_size
        data.push(b'v'); // type
        data.push(0); // unused
        data.extend_from_slice(&(value_offset as u32).to_le_bytes()); // value_start
        data.extend_from_slice(&((value_offset + value_data.len()) as u32).to_le_bytes()); // value_end

        // Value data
        data.extend_from_slice(&value_data);

        // Key
        data.extend_from_slice(key.as_bytes());

        data
    }

    #[test]
    fn load_and_extract_gvdb() {
        let gvdb = build_test_gvdb();
        let r = load_resource(&gvdb).unwrap();
        let data = extract_resource(&r, "/test/resource.txt").unwrap();
        assert_eq!(data, b"hello world");
    }

    #[test]
    fn load_invalid_data_fails() {
        assert!(load_resource(b"not a gvdb file").is_err());
    }

    #[test]
    fn empty_load_fails() {
        assert!(load_resource(&[]).is_err());
    }

    #[test]
    fn small_data_fails() {
        assert!(load_resource(&[0u8; 10]).is_err());
    }

    #[test]
    fn run_list_invalid_file() {
        assert_eq!(run(&["list", "x"]), 1);
    }

    #[test]
    fn run_help_ok() {
        assert_eq!(run(&["--help"]), 0);
    }
}
