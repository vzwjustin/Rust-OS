//! Persistent storage filesystem (pstore).
//!
//! Provides a virtual filesystem that exposes persistent kernel crash logs
//! and diagnostic messages. In Linux, pstore is backed by ramoops, NVRAM,
//! UEFI variables, or other backend storage drivers. This implementation
//! keeps entries in an in-memory `BTreeMap` so that crash logs recorded
//! during the current boot are visible under the pstore mount and survive
//! `sync`/`flush` calls. A real backend driver would plug in by loading
//! entries into the store at init time.
//!
//! See linux-master `fs/pstore/` for the reference implementation.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::cmp;
use spin::RwLock;

/// Root inode number for the pstore filesystem.
const ROOT_INODE: InodeNumber = 1;

/// First inode number allocated for pstore entries.
const FIRST_ENTRY_INODE: InodeNumber = 2;

/// Maximum number of pstore entries before older ones are pruned.
const MAX_ENTRIES: usize = 256;

/// The category of a persistent store entry, mirroring Linux's
/// `enum pstore_type_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PStoreType {
    /// Kernel dmesg / oops / panic log.
    Dmesg,
    /// Console output captured across reboots.
    Console,
    /// Function tracing output.
    Ftrace,
    /// Arbitrary userspace-defined record.
    Pmsg,
    /// Other / unspecified backend record.
    Other,
}

impl PStoreType {
    /// Filename prefix used for each record type, matching Linux's
    /// `pstore_type_to_name` convention.
    fn prefix(self) -> &'static str {
        match self {
            PStoreType::Dmesg => "dmesg",
            PStoreType::Console => "console",
            PStoreType::Ftrace => "ftrace",
            PStoreType::Pmsg => "pmsg",
            PStoreType::Other => "pstore",
        }
    }
}

/// A single persistent store record.
#[derive(Debug, Clone)]
pub struct PStoreEntry {
    /// Entry name (filename visible under the pstore mount).
    pub name: String,
    /// Raw record payload.
    pub data: Vec<u8>,
    /// Creation timestamp (ms since boot).
    pub timestamp: u64,
    /// Record category.
    pub record_type: PStoreType,
    /// Synthetic inode number.
    pub inode: InodeNumber,
}

/// Persistent store filesystem.
///
/// All entries live in a single flat directory (the pstore root). The
/// filesystem is read-mostly: new crash logs are appended via `write`
/// or `create`, and `unlink` removes a record. Directory operations
/// (`mkdir`, `rmdir`, `rename`, `symlink`, `readlink`) are not supported,
/// matching the real pstore filesystem semantics.
#[derive(Debug)]
pub struct PStoreFs {
    entries: RwLock<BTreeMap<String, PStoreEntry>>,
    /// Reverse lookup: inode -> name, kept in sync with `entries`.
    inodes: RwLock<BTreeMap<InodeNumber, String>>,
    next_inode: RwLock<InodeNumber>,
}

impl PStoreFs {
    /// Create a new empty pstore filesystem.
    ///
    /// A real backend would call [`Self::load_entry`] for each persisted
    /// record discovered during driver probe; here we start empty.
    pub fn new() -> FsResult<Self> {
        Ok(Self {
            entries: RwLock::new(BTreeMap::new()),
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(FIRST_ENTRY_INODE),
        })
    }

    /// Allocate the next inode number.
    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    /// Insert a pstore record, pruning the oldest entry if the store is full.
    fn insert_entry(&self, name: String, data: Vec<u8>, record_type: PStoreType) -> InodeNumber {
        let mut entries = self.entries.write();
        let mut inodes = self.inodes.write();

        // Prune oldest entry if at capacity. BTreeMap iteration is ordered
        // by key (name), but we want oldest-by-timestamp; collect and sort.
        if entries.len() >= MAX_ENTRIES {
            // Find the entry with the smallest timestamp.
            let oldest = entries
                .iter()
                .min_by_key(|(_, e)| e.timestamp)
                .map(|(k, _)| k.clone());
            if let Some(oldest_name) = oldest {
                if let Some(removed) = entries.remove(&oldest_name) {
                    inodes.remove(&removed.inode);
                }
            }
        }

        let inode = self.allocate_inode();
        let now = get_current_time();
        let entry = PStoreEntry {
            name: name.clone(),
            data,
            timestamp: now,
            record_type,
            inode,
        };
        inodes.insert(inode, name.clone());
        entries.insert(name, entry);
        inode
    }

    /// Load a pre-existing record into the store (used by backend drivers).
    pub fn load_entry(&self, name: &str, data: Vec<u8>, record_type: PStoreType) -> InodeNumber {
        self.insert_entry(name.to_string(), data, record_type)
    }

    /// Record a new crash log entry, generating a unique filename from the
    /// record type and current timestamp.
    pub fn record(&self, record_type: PStoreType, data: Vec<u8>) -> InodeNumber {
        let now = get_current_time();
        let name = format!("{}-{}", record_type.prefix(), now);
        self.insert_entry(name, data, record_type)
    }

    /// Resolve a path to an entry name, stripping leading slashes.
    fn path_to_name(&self, path: &str) -> String {
        path.trim_start_matches('/').to_string()
    }

    /// Look up an entry by inode number.
    fn entry_by_inode(&self, inode: InodeNumber) -> FsResult<String> {
        let inodes = self.inodes.read();
        inodes
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }
}

impl FileSystem for PStoreFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::PStore
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let entries = self.entries.read();
        let count = entries.len() as u64;
        let used_bytes: u64 = entries.values().map(|e| e.data.len() as u64).sum();
        let block_size = 4096u32;
        // PStore is virtual: report a small fixed inode pool and a 1 MiB
        // logical block pool so callers see sensible free-space numbers.
        let total_blocks: u64 = 256;
        let used_blocks = (used_bytes + block_size as u64 - 1) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_ENTRIES as u64,
            free_inodes: MAX_ENTRIES as u64 - count,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let name = self.path_to_name(path);
        if name.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        if name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut entries = self.entries.write();
        if entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        drop(entries);
        // Infer the record type from the filename prefix.
        let record_type = if name.starts_with("dmesg") {
            PStoreType::Dmesg
        } else if name.starts_with("console") {
            PStoreType::Console
        } else if name.starts_with("ftrace") {
            PStoreType::Ftrace
        } else if name.starts_with("pmsg") {
            PStoreType::Pmsg
        } else {
            PStoreType::Other
        };
        Ok(self.insert_entry(name, Vec::new(), record_type))
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let name = self.path_to_name(path);
        if name.is_empty() {
            // Root directory.
            return Ok(ROOT_INODE);
        }
        let entries = self.entries.read();
        let entry = entries.get(&name).ok_or(FsError::NotFound)?;
        Ok(entry.inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }
        let name = self.entry_by_inode(inode)?;
        let entries = self.entries.read();
        let entry = entries.get(&name).ok_or(FsError::NotFound)?;
        let len = entry.data.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), entry.data.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&entry.data[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }
        let name = self.entry_by_inode(inode)?;
        let mut entries = self.entries.write();
        let entry = entries.get_mut(&name).ok_or(FsError::NotFound)?;
        let required = (offset as usize).saturating_add(buffer.len());
        if entry.data.len() < required {
            entry.data.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        entry.data[start..end].copy_from_slice(buffer);
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == ROOT_INODE {
            let mut md = FileMetadata::new(ROOT_INODE, FileType::Directory, 0);
            md.permissions = FilePermissions::default_directory();
            return Ok(md);
        }
        let name = self.entry_by_inode(inode)?;
        let entries = self.entries.read();
        let entry = entries.get(&name).ok_or(FsError::NotFound)?;
        let mut md = FileMetadata::new(inode, FileType::Regular, entry.data.len() as u64);
        md.created = entry.timestamp;
        md.modified = entry.timestamp;
        md.accessed = get_current_time();
        Ok(md)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // PStore entries are immutable in metadata; accept silently.
        Ok(())
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let name = self.path_to_name(path);
        if name.is_empty() {
            return Err(FsError::IsADirectory);
        }
        let mut entries = self.entries.write();
        let removed = entries.remove(&name).ok_or(FsError::NotFound)?;
        let mut inodes = self.inodes.write();
        inodes.remove(&removed.inode);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        if inode != ROOT_INODE {
            return Err(FsError::NotADirectory);
        }
        let entries = self.entries.read();
        let mut result = Vec::new();
        for entry in entries.values() {
            result.push(DirectoryEntry {
                name: entry.name.clone(),
                inode: entry.inode,
                file_type: FileType::Regular,
            });
        }
        Ok(result)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory store is always consistent; a real backend would flush
        // to NVRAM/UEFI variables here.
        Ok(())
    }
}
