//! CRAMFS (Compressed ROM File System) implementation
//!
//! CRAMFS is a read-only, compressed filesystem optimized for embedded systems and
//! bootable media. This in-memory implementation tracks file entries with
//! compression metadata; a full implementation would decompress blocks via zlib.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// CRAMFS inode entry
#[derive(Debug, Clone)]
struct CramfsInode {
    inode: InodeNumber,
    name: String,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    data: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
    symlink_target: Option<String>,
    compressed: bool,
}

/// CRAMFS filesystem (read-only in-memory)
#[derive(Debug)]
pub struct CramfsFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, CramfsInode>>,
    next_inode: RwLock<InodeNumber>,
}

impl CramfsFileSystem {
    /// Create a new CRAMFS filesystem instance with a root directory.
    /// A full implementation would parse the superblock, verify magic
    /// (`-rom1fs-` equivalent for CRAMFS), and build inode/block mapping
    /// tables from the compressed image.
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            CramfsInode {
                inode: 1,
                name: String::from("/"),
                is_dir: true,
                size: 0,
                permissions: FilePermissions::default_directory(),
                data: Vec::new(),
                entries: BTreeMap::new(),
                symlink_target: None,
                compressed: false,
            },
        );
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(1);
        }
        let inodes = self.inodes.read();
        let mut current = 1u64;
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Insert a file into the CRAMFS image (for populating the filesystem).
    pub fn insert_file(
        &self,
        path: &str,
        data: Vec<u8>,
        compressed: bool,
    ) -> FsResult<InodeNumber> {
        let (parent_path, name) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        };
        let parent_inode = self.resolve_path(parent_path)?;
        let mut next = self.next_inode.write();
        let inode_num = *next;
        *next += 1;
        let mut inodes = self.inodes.write();
        let node = CramfsInode {
            inode: inode_num,
            name: String::from(name),
            is_dir: false,
            size: data.len() as u64,
            permissions: FilePermissions::default_file(),
            data,
            entries: BTreeMap::new(),
            symlink_target: None,
            compressed,
        };
        inodes.insert(inode_num, node);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.insert(String::from(name), inode_num);
        }
        Ok(inode_num)
    }

    /// Insert a directory into the CRAMFS image.
    pub fn insert_directory(&self, path: &str) -> FsResult<InodeNumber> {
        let (parent_path, name) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        };
        let parent_inode = self.resolve_path(parent_path)?;
        let mut next = self.next_inode.write();
        let inode_num = *next;
        *next += 1;
        let mut inodes = self.inodes.write();
        let node = CramfsInode {
            inode: inode_num,
            name: String::from(name),
            is_dir: true,
            size: 0,
            permissions: FilePermissions::default_directory(),
            data: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
            compressed: false,
        };
        inodes.insert(inode_num, node);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.insert(String::from(name), inode_num);
        }
        Ok(inode_num)
    }
}

impl FileSystem for CramfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Cramfs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let total_size: u64 = inodes.values().map(|n| n.size).sum();
        let block_size = 4096u64;
        let total_blocks = (total_size + block_size - 1) / block_size;
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: inodes.len() as u64,
            free_inodes: 0,
            block_size: block_size as u32,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let offset = offset as usize;
        if offset >= node.data.len() {
            return Ok(0);
        }
        let available = node.data.len() - offset;
        let to_read = buffer.len().min(available);
        buffer[..to_read].copy_from_slice(&node.data[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        let now = get_current_time();
        Ok(FileMetadata {
            inode,
            file_type: if node.is_dir {
                FileType::Directory
            } else if node.symlink_target.is_some() {
                FileType::SymbolicLink
            } else {
                FileType::Regular
            },
            size: node.size,
            permissions: node.permissions,
            uid: 0,
            gid: 0,
            created: now,
            modified: now,
            accessed: now,
            link_count: 1,
            device_id: None,
        })
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
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_inode) in &node.entries {
            if let Some(child) = inodes.get(&child_inode) {
                entries.push(DirectoryEntry {
                    name: name.clone(),
                    inode: child_inode,
                    file_type: if child.is_dir {
                        FileType::Directory
                    } else if child.symlink_target.is_some() {
                        FileType::SymbolicLink
                    } else {
                        FileType::Regular
                    },
                });
            }
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<alloc::string::String> {
        let inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        node.symlink_target.clone().ok_or(FsError::NotASymlink)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
