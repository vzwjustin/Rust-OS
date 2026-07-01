//! Persistent storage filesystem.
//!
//! Implements pstore (persistent storage filesystem), which exposes persistent
//! memory regions (ramoops, NVRAM, UEFI variables, SELinux policy) as a
//! read-only pseudofilesystem for crash logs and diagnostics.
//!
//! This in-memory implementation tracks a list of records, each with a type
//! and opaque payload. The filesystem presents the records as regular files
//! under the root directory. Writes and directory creation are rejected because
//! pstore is read-only (records are populated by the backend, not by userspace
//! writes through the VFS).

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
use spin::RwLock;

/// PStore record type (mirrors `enum pstore_type_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PStoreRecordType {
    /// Kernel crash log (dmesg).
    Dmesg,
    /// Console output.
    Console,
    /// Ftrace output.
    Ftrace,
    /// Userspace mtdoops.
    Mtdoops,
    /// Platform-specific (e.g. EFI variables).
    Platform,
    /// Frontend-specific.
    Pmsg,
    /// Powerpc OPAL.
    PowerpcOpal,
}

impl PStoreRecordType {
    /// Filename prefix used for the record type.
    pub fn prefix(&self) -> &'static str {
        match self {
            PStoreRecordType::Dmesg => "dmesg",
            PStoreRecordType::Console => "console",
            PStoreRecordType::Ftrace => "ftrace",
            PStoreRecordType::Mtdoops => "mtdoops",
            PStoreRecordType::Platform => "platform",
            PStoreRecordType::Pmsg => "pmsg",
            PStoreRecordType::PowerpcOpal => "opal",
        }
    }
}

/// A persistent store record.
#[derive(Debug, Clone)]
pub struct PStoreRecord {
    /// Record type.
    pub record_type: PStoreRecordType,
    /// Record id (monotonic per type).
    pub id: u64,
    /// Record payload.
    pub data: Vec<u8>,
    /// Timestamp (in ticks) when the record was captured.
    pub timestamp: u64,
}

/// In-memory inode backing a pstore record.
#[derive(Debug, Clone)]
struct PStoreInode {
    /// Inode metadata.
    metadata: FileMetadata,
    /// Record payload (for regular files).
    content: Vec<u8>,
    /// Directory entries (for the root directory).
    entries: BTreeMap<String, InodeNumber>,
}

/// PStore filesystem implementation.
#[derive(Debug)]
pub struct PStoreFileSystem {
    /// All inodes keyed by inode number.
    inodes: RwLock<BTreeMap<InodeNumber, PStoreInode>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode number.
    root_inode: InodeNumber,
}

impl PStoreFileSystem {
    /// Create a new PStore filesystem with an empty root directory.
    pub fn new() -> FsResult<Self> {
        let root_inode = 1;
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), root_inode);
        entries.insert("..".to_string(), root_inode);

        let root = PStoreInode {
            metadata: FileMetadata {
                inode: root_inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::from_octal(0o555),
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 2,
                device_id: None,
            },
            content: Vec::new(),
            entries,
        };

        let mut inodes = BTreeMap::new();
        inodes.insert(root_inode, root);

        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
        })
    }

    /// Allocate a new inode number.
    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    /// Add a persistent record to the filesystem.
    ///
    /// The record is exposed as a regular file named `<prefix>-<id>` under the
    /// root directory. This is the API a backend driver (ramoops, EFI, etc.)
    /// would call to publish a captured record.
    pub fn add_record(&self, record: PStoreRecord) -> FsResult<InodeNumber> {
        let name = format!("{}-{}", record.record_type.prefix(), record.id);
        if name.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let new_inode = self.allocate_inode();
        let size = record.data.len() as u64;
        let file_inode = PStoreInode {
            metadata: FileMetadata {
                inode: new_inode,
                file_type: FileType::Regular,
                size,
                permissions: FilePermissions::from_octal(0o444),
                uid: 0,
                gid: 0,
                created: record.timestamp,
                modified: record.timestamp,
                accessed: record.timestamp,
                link_count: 1,
                device_id: None,
            },
            content: record.data,
            entries: BTreeMap::new(),
        };

        let mut inodes = self.inodes.write();
        let root = inodes
            .get_mut(&self.root_inode)
            .ok_or(FsError::IoError)?;
        root.entries.insert(name, new_inode);
        root.metadata.modified = get_current_time();
        inodes.insert(new_inode, file_inode);
        Ok(new_inode)
    }

    /// Number of records currently stored (excluding the root directory).
    pub fn record_count(&self) -> usize {
        self.inodes.read().len().saturating_sub(1)
    }

    /// Resolve a path to an inode number.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" || path.is_empty() {
            return Ok(self.root_inode);
        }
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let inode = inodes.get(&current).ok_or(FsError::NotFound)?;
            if inode.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *inode.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }
}

impl FileSystem for PStoreFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // PStore is a RAM-backed pseudofilesystem.
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: used,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // pstore is read-only from the VFS perspective.
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if file_inode.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        file_inode.metadata.accessed = get_current_time();
        let len = file_inode.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), file_inode.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&file_inode.content[start..end]);
        Ok(n)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        // Snapshot entries to release the mutable borrow before re-looking-up.
        let snapshot: Vec<(String, InodeNumber)> = dir
            .entries
            .iter()
            .map(|(n, &i)| (n.clone(), i))
            .collect();
        let mut out = Vec::new();
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                out.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: child.metadata.file_type,
                });
            }
        }
        Ok(out)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory store; nothing to sync.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_read_record() {
        let fs = PStoreFileSystem::new().unwrap();
        let rec = PStoreRecord {
            record_type: PStoreRecordType::Dmesg,
            id: 1,
            data: b"kernel panic".to_vec(),
            timestamp: 12345,
        };
        let inode = fs.add_record(rec).unwrap();
        let md = fs.metadata(inode).unwrap();
        assert_eq!(md.size, 12);
        let mut buf = [0u8; 12];
        let n = fs.read(inode, 0, &mut buf).unwrap();
        assert_eq!(n, 12);
        assert_eq!(&buf, b"kernel panic");
        assert_eq!(fs.record_count(), 1);
    }

    #[test]
    fn test_readonly_rejected() {
        let fs = PStoreFileSystem::new().unwrap();
        assert_eq!(
            fs.create("/x", FilePermissions::default_file()),
            Err(FsError::ReadOnly)
        );
        assert_eq!(fs.mkdir("/d", FilePermissions::default_directory()), Err(FsError::ReadOnly));
    }
}
