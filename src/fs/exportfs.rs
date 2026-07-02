//! ExportFS utilities for NFS file handle encoding/decoding
//!
//! Wraps a backing `FileSystem` and adds the ability to encode/decode
//! file handles for NFS exports.  All `FileSystem` trait methods delegate
//! to the backing filesystem; the export operations (`encode_fh`,
//! `decode_fh`, `get_parent`, `get_name`) provide the NFS export helper
//! layer.

use alloc::vec;
use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::VecDeque,
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::fmt;
use spin::RwLock;

/// File handle type indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileHandleType {
    /// Handle contains just the inode number (64-bit).
    InodeOnly,
    /// Handle contains inode + parent inode (128-bit).
    WithParent,
    /// Handle contains inode + parent + generation (full).
    Full,
}

/// An NFS file handle that uniquely identifies a file across reboots.
#[derive(Debug, Clone)]
pub struct FileHandle {
    /// Handle type.
    pub handle_type: FileHandleType,
    /// Inode number of the file.
    pub inode: InodeNumber,
    /// Inode number of the parent directory.
    pub parent_inode: InodeNumber,
    /// Generation number (derived from metadata to detect reuse).
    pub generation: u64,
}

impl FileHandle {
    /// Encode the handle into a byte vector.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.handle_type as u8);
        buf.extend_from_slice(&self.inode.to_le_bytes());
        buf.extend_from_slice(&self.parent_inode.to_le_bytes());
        buf.extend_from_slice(&self.generation.to_le_bytes());
        buf
    }

    /// Decode a handle from a byte slice.
    pub fn decode(data: &[u8]) -> FsResult<Self> {
        if data.len() < 1 + 8 + 8 + 8 {
            return Err(FsError::InvalidArgument);
        }
        let handle_type = match data[0] {
            0 => FileHandleType::InodeOnly,
            1 => FileHandleType::WithParent,
            2 => FileHandleType::Full,
            _ => return Err(FsError::InvalidArgument),
        };
        let inode = InodeNumber::from_le_bytes(data[1..9].try_into().unwrap());
        let parent_inode = InodeNumber::from_le_bytes(data[9..17].try_into().unwrap());
        let generation = u64::from_le_bytes(data[17..25].try_into().unwrap());
        Ok(Self {
            handle_type,
            inode,
            parent_inode,
            generation,
        })
    }
}

/// Trait for filesystems that support NFS export operations.
pub trait ExportOperations {
    /// Encode a file handle for the given inode.
    fn encode_fh(&self, inode: InodeNumber) -> FsResult<FileHandle>;

    /// Decode a file handle back to an inode number.
    fn decode_fh(&self, handle: &FileHandle) -> FsResult<InodeNumber>;

    /// Get the parent directory inode for a given inode.
    fn get_parent(&self, inode: InodeNumber) -> FsResult<InodeNumber>;

    /// Get the name of a child inode within its parent directory.
    fn get_name(&self, parent: InodeNumber, child: InodeNumber) -> FsResult<String>;
}

/// ExportFS wrapper around a backing filesystem.
#[derive(Debug)]
pub struct ExportFs {
    backing: Arc<dyn FileSystem>,
    root_inode: RwLock<InodeNumber>,
}

impl ExportFs {
    /// Create a new ExportFs wrapping a backing filesystem.
    pub fn new(backing: Arc<dyn FileSystem>) -> Self {
        // The root inode is typically 1 for most filesystems
        Self {
            backing,
            root_inode: RwLock::new(1),
        }
    }

    /// Set the root inode (for filesystems where root != 1).
    pub fn set_root_inode(&self, inode: InodeNumber) {
        *self.root_inode.write() = inode;
    }
}

impl fmt::Display for ExportFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "exportfs({})", self.backing.fs_type())
    }
}

impl FileSystem for ExportFs {
    fn fs_type(&self) -> FileSystemType {
        self.backing.fs_type()
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        self.backing.statfs()
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.backing.create(path, permissions)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        self.backing.open(path, flags)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        self.backing.read(inode, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        self.backing.write(inode, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        self.backing.metadata(inode)
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        self.backing.set_metadata(inode, metadata)
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.backing.mkdir(path, permissions)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        self.backing.rmdir(path)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        self.backing.unlink(path)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        self.backing.readdir(inode)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        self.backing.rename(old_path, new_path)
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        self.backing.symlink(target, link_path)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        self.backing.readlink(path)
    }

    fn sync(&self) -> FsResult<()> {
        self.backing.sync()
    }
}

impl ExportOperations for ExportFs {
    /// Encode a file handle for the given inode.
    /// Validates the inode exists and derives a generation from metadata.
    fn encode_fh(&self, inode: InodeNumber) -> FsResult<FileHandle> {
        // Validate the inode exists
        let metadata = self.backing.metadata(inode)?;

        // Derive generation from metadata: combine created time and inode
        // to produce a stable generation number
        let generation = metadata.created ^ (inode << 32) ^ metadata.link_count as u64;

        // Determine parent inode
        let parent_inode = self.get_parent(inode).unwrap_or(0);

        let handle_type = if parent_inode == 0 {
            FileHandleType::InodeOnly
        } else {
            FileHandleType::Full
        };

        Ok(FileHandle {
            handle_type,
            inode,
            parent_inode,
            generation,
        })
    }

    /// Decode a file handle back to an inode number.
    /// Validates the inode exists and the generation matches.
    fn decode_fh(&self, handle: &FileHandle) -> FsResult<InodeNumber> {
        // Validate the inode exists
        let metadata = self.backing.metadata(handle.inode)?;

        // Recompute generation and verify
        let expected_gen = metadata.created ^ (handle.inode << 32) ^ metadata.link_count as u64;
        if expected_gen != handle.generation {
            return Err(FsError::NotFound);
        }

        Ok(handle.inode)
    }

    /// Get the parent directory inode for a given inode.
    /// Uses BFS from the root directory via readdir.
    fn get_parent(&self, inode: InodeNumber) -> FsResult<InodeNumber> {
        let root = *self.root_inode.read();

        // Root's parent is itself
        if inode == root {
            return Ok(root);
        }

        // BFS from root
        let mut queue: VecDeque<InodeNumber> = VecDeque::new();
        queue.push_back(root);
        let mut visited: Vec<InodeNumber> = vec![root];

        while let Some(current) = queue.pop_front() {
            let entries = self.backing.readdir(current)?;
            for entry in entries {
                // Check if this entry is the target inode
                if entry.inode == inode {
                    return Ok(current);
                }

                // If it's a directory, add to BFS queue
                if entry.file_type == FileType::Directory
                    && !visited.contains(&entry.inode)
                {
                    visited.push(entry.inode);
                    queue.push_back(entry.inode);
                }
            }
        }

        Err(FsError::NotFound)
    }

    /// Get the name of a child inode within its parent directory.
    /// Scans readdir(parent) for an entry matching the child inode.
    fn get_name(&self, parent: InodeNumber, child: InodeNumber) -> FsResult<String> {
        let entries = self.backing.readdir(parent)?;
        for entry in entries {
            if entry.inode == child {
                return Ok(entry.name);
            }
        }
        Err(FsError::NotFound)
    }
}
