//! GObject introspection typelib matching `girepository/gitypelib.h` /
//! `girepository/gitypelib-internal.h`.
//!
//! Provides the binary typelib header layout, directory parsing, and
//! in-memory typelibs for tests and repository integration.
//!
//! Ref counting uses `Arc<T>` (same pattern as `gdbusintrospection.rs`).

use crate::gibaseinfo::{BaseInfo, InfoType};
use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Magic bytes at the start of every typelib (`GI_IR_MAGIC`).
pub const GI_IR_MAGIC: &[u8] = b"GOBJ\nMETADATA\r\n\x1a";

/// Size of the on-disk `Header` struct in typelib format version 4.
pub const TYPELIB_HEADER_SIZE: usize = 104;

/// Size of each `DirEntry` in the typelib directory.
pub const TYPELIB_DIR_ENTRY_SIZE: usize = 12;

static NEXT_TYPELIB_ID: AtomicUsize = AtomicUsize::new(1);

/// Error domain for typelib parsing (`GI_TYPELIB_ERROR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypelibError {
    /// The typelib is invalid.
    Invalid,
    /// The typelib header is invalid.
    InvalidHeader,
    /// The typelib directory is invalid.
    InvalidDirectory,
    /// A typelib entry is invalid.
    InvalidEntry,
    /// A typelib blob is invalid.
    InvalidBlob,
}

impl TypelibError {
    /// Human-readable message for this error.
    pub fn message(self) -> &'static str {
        match self {
            Self::Invalid => "invalid typelib",
            Self::InvalidHeader => "invalid typelib header",
            Self::InvalidDirectory => "invalid typelib directory",
            Self::InvalidEntry => "invalid typelib entry",
            Self::InvalidBlob => "invalid typelib blob",
        }
    }
}

/// Parsed typelib header (`Header` in `gitypelib-internal.h`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeLibHeader {
    /// Major format version.
    pub major_version: u8,
    /// Minor format version.
    pub minor_version: u8,
    /// Number of directory entries.
    pub n_entries: u16,
    /// Number of local (resolved) entries.
    pub n_local_entries: u16,
    /// Byte offset of the directory table.
    pub directory: u32,
    /// Declared typelib size in bytes.
    pub size: u32,
}

/// A loaded introspection typelib (`GITypelib`).
#[derive(Clone, Debug)]
pub struct Typelib {
    id: usize,
    header: TypeLibHeader,
    namespace: String,
    version: String,
    entries: BTreeMap<String, Arc<BaseInfo>>,
    raw: Option<Arc<[u8]>>,
}

impl Typelib {
    /// Build an in-memory typelib for tests and early repository stubs.
    pub fn new_in_memory(
        namespace: impl Into<String>,
        version: impl Into<String>,
        entries: BTreeMap<String, Arc<BaseInfo>>,
    ) -> Arc<Self> {
        let n_entries = entries.len().min(u16::MAX as usize) as u16;
        Arc::new(Self {
            id: NEXT_TYPELIB_ID.fetch_add(1, Ordering::Relaxed),
            header: TypeLibHeader {
                major_version: 1,
                minor_version: 0,
                n_entries,
                n_local_entries: n_entries,
                directory: TYPELIB_HEADER_SIZE as u32,
                size: TYPELIB_HEADER_SIZE as u32,
            },
            namespace: namespace.into(),
            version: version.into(),
            entries,
            raw: None,
        })
    }

    /// Parse a typelib from raw bytes (`gi_typelib_new_from_bytes`).
    pub fn from_bytes(data: &[u8]) -> Result<Arc<Self>, TypelibError> {
        let header = parse_full_header(data)?;
        let namespace_off =
            read_u32_le(data, header_offset_namespace()).ok_or(TypelibError::InvalidHeader)?;
        let version_off =
            read_u32_le(data, header_offset_nsversion()).ok_or(TypelibError::InvalidHeader)?;
        let namespace = read_typelib_string(data, namespace_off)?;
        let version = read_typelib_string(data, version_off)?;
        let entry_specs = parse_directory(data, &header)?;

        let raw: Arc<[u8]> = Arc::from(data.to_vec().into_boxed_slice());
        let tl = Arc::new_cyclic(|weak| {
            let mut entries = BTreeMap::new();
            for (name, info_type) in entry_specs {
                let info = BaseInfo::new(name.clone(), &namespace, info_type, None, weak.clone());
                entries.insert(name, info);
            }
            Self {
                id: NEXT_TYPELIB_ID.fetch_add(1, Ordering::Relaxed),
                header,
                namespace,
                version,
                entries,
                raw: Some(raw.clone()),
            }
        });
        Ok(tl)
    }

    /// Returns the typelib namespace (`gi_typelib_get_namespace`).
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the typelib version string.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the parsed header.
    pub fn header(&self) -> &TypeLibHeader {
        &self.header
    }

    /// Raw typelib bytes when loaded from a binary blob.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.raw.as_deref()
    }

    /// Look up a [class@GIRepository.BaseInfo] by name within this typelib.
    pub fn find_by_name(&self, name: &str) -> Option<Arc<BaseInfo>> {
        self.entries.get(name).map(Arc::clone)
    }

    /// Number of registered entries (mirrors `header.n_entries` for in-memory libs).
    pub fn n_entries(&self) -> u16 {
        self.header.n_entries
    }

    /// Bump the ref count (`gi_typelib_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

fn header_offset_namespace() -> usize {
    44
}

fn header_offset_nsversion() -> usize {
    48
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let chunk = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([chunk[0], chunk[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let chunk = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
}

fn read_typelib_string(bytes: &[u8], offset: u32) -> Result<String, TypelibError> {
    let start = offset as usize;
    if start >= bytes.len() {
        return Err(TypelibError::InvalidHeader);
    }
    let end = bytes[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|pos| start + pos)
        .unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[start..end])
        .map(|s| s.to_owned())
        .map_err(|_| TypelibError::InvalidHeader)
}

fn blob_type_to_info_type(blob_type: u16) -> Option<InfoType> {
    match blob_type {
        1 => Some(InfoType::Function),
        2 => Some(InfoType::Callback),
        3 => Some(InfoType::Struct),
        5 => Some(InfoType::Enum),
        6 => Some(InfoType::Flags),
        7 => Some(InfoType::Object),
        8 => Some(InfoType::Interface),
        9 => Some(InfoType::Constant),
        11 => Some(InfoType::Union),
        _ => None,
    }
}

/// Parse the typelib header from raw bytes.
///
/// Only the fixed-size header fields needed for validation and entry counts
/// are decoded from the first 24 bytes (legacy helper).
pub fn parse_header(bytes: &[u8]) -> Result<TypeLibHeader, TypelibError> {
    parse_full_header(bytes)
}

/// Parse the full typelib header (`Header` in `gitypelib-internal.h`).
pub fn parse_full_header(bytes: &[u8]) -> Result<TypeLibHeader, TypelibError> {
    if bytes.len() < TYPELIB_HEADER_SIZE {
        return Err(TypelibError::InvalidHeader);
    }

    if &bytes[..16] != GI_IR_MAGIC {
        return Err(TypelibError::InvalidHeader);
    }

    let major_version = bytes[16];
    let minor_version = bytes[17];
    let n_entries = read_u16_le(bytes, 20).ok_or(TypelibError::InvalidHeader)?;
    let n_local_entries = read_u16_le(bytes, 22).ok_or(TypelibError::InvalidHeader)?;
    let directory = read_u32_le(bytes, 24).ok_or(TypelibError::InvalidHeader)?;
    let size = read_u32_le(bytes, 40).ok_or(TypelibError::InvalidHeader)?;

    if n_entries < n_local_entries {
        return Err(TypelibError::InvalidHeader);
    }

    if size as usize > bytes.len() {
        return Err(TypelibError::InvalidHeader);
    }

    let dir_start = directory as usize;
    let dir_end = dir_start
        .checked_add(usize::from(n_entries) * TYPELIB_DIR_ENTRY_SIZE)
        .ok_or(TypelibError::InvalidDirectory)?;
    if dir_end > bytes.len() {
        return Err(TypelibError::InvalidDirectory);
    }

    Ok(TypeLibHeader {
        major_version,
        minor_version,
        n_entries,
        n_local_entries,
        directory,
        size,
    })
}

fn parse_directory(
    bytes: &[u8],
    header: &TypeLibHeader,
) -> Result<Vec<(String, InfoType)>, TypelibError> {
    let dir_start = header.directory as usize;
    let mut specs = Vec::new();

    for index in 0..header.n_local_entries {
        let off = dir_start + usize::from(index) * TYPELIB_DIR_ENTRY_SIZE;
        let blob_type = read_u16_le(bytes, off).ok_or(TypelibError::InvalidDirectory)?;
        let flags = read_u16_le(bytes, off + 2).ok_or(TypelibError::InvalidDirectory)?;
        let local = flags & 1 != 0;
        let name_off = read_u32_le(bytes, off + 4).ok_or(TypelibError::InvalidDirectory)?;

        if !local {
            continue;
        }

        let info_type = blob_type_to_info_type(blob_type).ok_or(TypelibError::InvalidEntry)?;
        let name = read_typelib_string(bytes, name_off)?;
        if name.is_empty() {
            return Err(TypelibError::InvalidEntry);
        }
        specs.push((name, info_type));
    }

    Ok(specs)
}

/// Build a minimal valid typelib byte buffer with the given header fields.
///
/// Used by unit tests; includes namespace/version strings and optional entries.
pub fn build_test_typelib_bytes(header: &TypeLibHeader) -> Vec<u8> {
    build_test_typelib_bytes_with_entries(header, "Test", "1.0", &[])
}

/// Build a typelib byte buffer with namespace, version, and named entries.
pub fn build_test_typelib_bytes_with_entries(
    header: &TypeLibHeader,
    namespace: &str,
    version: &str,
    entry_names: &[(&str, u16)],
) -> Vec<u8> {
    let n_entries = if entry_names.is_empty() {
        header.n_entries
    } else {
        entry_names.len().min(u16::MAX as usize) as u16
    };
    let n_local_entries = if entry_names.is_empty() {
        header.n_local_entries
    } else {
        n_entries
    };
    let mut header = header.clone();
    header.n_entries = n_entries;
    header.n_local_entries = n_local_entries;
    header.directory = TYPELIB_HEADER_SIZE as u32;
    header.size = 0;

    let dir_size = usize::from(n_entries) * TYPELIB_DIR_ENTRY_SIZE;
    let mut string_blob = Vec::new();
    let mut string_offsets = Vec::new();

    for &(name, _) in entry_names {
        let off = (TYPELIB_HEADER_SIZE + dir_size + string_blob.len()) as u32;
        string_offsets.push(off);
        string_blob.extend_from_slice(name.as_bytes());
        string_blob.push(0);
    }

    let namespace_off = (TYPELIB_HEADER_SIZE + dir_size + string_blob.len()) as u32;
    string_blob.extend_from_slice(namespace.as_bytes());
    string_blob.push(0);

    let version_off = (TYPELIB_HEADER_SIZE + dir_size + string_blob.len()) as u32;
    string_blob.extend_from_slice(version.as_bytes());
    string_blob.push(0);

    let total_size = (TYPELIB_HEADER_SIZE + dir_size + string_blob.len()) as u32;

    let mut bytes = vec![0u8; total_size as usize];
    bytes[..16].copy_from_slice(GI_IR_MAGIC);
    bytes[16] = header.major_version;
    bytes[17] = header.minor_version;
    bytes[20..22].copy_from_slice(&header.n_entries.to_le_bytes());
    bytes[22..24].copy_from_slice(&header.n_local_entries.to_le_bytes());
    bytes[24..28].copy_from_slice(&header.directory.to_le_bytes());
    bytes[40..44].copy_from_slice(&total_size.to_le_bytes());
    bytes[44..48].copy_from_slice(&namespace_off.to_le_bytes());
    bytes[48..52].copy_from_slice(&version_off.to_le_bytes());

    for (index, &(_, blob_type)) in entry_names.iter().enumerate() {
        let off = TYPELIB_HEADER_SIZE + index * TYPELIB_DIR_ENTRY_SIZE;
        bytes[off..off + 2].copy_from_slice(&blob_type.to_le_bytes());
        bytes[off + 2..off + 4].copy_from_slice(&1u16.to_le_bytes()); // local=1
        bytes[off + 4..off + 8].copy_from_slice(&string_offsets[index].to_le_bytes());
        bytes[off + 8..off + 12].copy_from_slice(&0u32.to_le_bytes());
    }

    bytes[TYPELIB_HEADER_SIZE + dir_size..].copy_from_slice(&string_blob);
    bytes
}

/// Register a named info entry on an in-memory typelib builder map.
pub fn register_entry(entries: &mut BTreeMap<String, Arc<BaseInfo>>, info: Arc<BaseInfo>) {
    entries.insert(info.name().to_owned(), info);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gienuminfo::EnumInfo;

    #[test]
    fn parse_header_accepts_magic_and_counts() {
        let header = TypeLibHeader {
            major_version: 4,
            minor_version: 0,
            n_entries: 0,
            n_local_entries: 0,
            directory: TYPELIB_HEADER_SIZE as u32,
            size: TYPELIB_HEADER_SIZE as u32,
        };
        let bytes = build_test_typelib_bytes(&header);
        let parsed = parse_header(&bytes).expect("header should parse");
        assert_eq!(parsed.major_version, 4);
        assert_eq!(parsed.n_entries, 0);
        assert_eq!(parsed.n_local_entries, 0);
    }

    #[test]
    fn parse_header_rejects_bad_magic() {
        let mut bytes = build_test_typelib_bytes(&TypeLibHeader {
            major_version: 4,
            minor_version: 0,
            n_entries: 1,
            n_local_entries: 1,
            directory: TYPELIB_HEADER_SIZE as u32,
            size: TYPELIB_HEADER_SIZE as u32,
        });
        bytes[0] = b'X';
        assert_eq!(parse_header(&bytes), Err(TypelibError::InvalidHeader));
    }

    #[test]
    fn from_bytes_resolves_namespace_and_entries() {
        let header = TypeLibHeader {
            major_version: 4,
            minor_version: 0,
            n_entries: 1,
            n_local_entries: 1,
            directory: TYPELIB_HEADER_SIZE as u32,
            size: 0,
        };
        let bytes =
            build_test_typelib_bytes_with_entries(&header, "Sample", "2.0", &[("SampleEnum", 5)]);
        let tl = Typelib::from_bytes(&bytes).expect("typelib");
        assert_eq!(tl.namespace(), "Sample");
        assert_eq!(tl.version(), "2.0");
        let found = tl.find_by_name("SampleEnum").expect("entry");
        assert_eq!(found.info_type(), InfoType::Enum);
    }

    #[test]
    fn in_memory_typelib_lookup() {
        let mut entries = BTreeMap::new();
        let info = EnumInfo::new("TestEnum", "Test", &[]);
        register_entry(&mut entries, info.base().ref_());
        let tl = Typelib::new_in_memory("Test", "1.0", entries);
        assert_eq!(tl.namespace(), "Test");
        assert_eq!(tl.n_entries(), 1);
        let found = tl.find_by_name("TestEnum").expect("entry");
        assert_eq!(found.info_type(), InfoType::Enum);
    }
}
