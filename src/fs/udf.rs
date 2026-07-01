//! UDF (Universal Disk Format) optical media filesystem implementation.
//!
//! In-memory VFS implementation of UDF mount state. It models the on-disc
//! structures (Volume Descriptor Sequence, partition, file entries) as in-RAM
//! tables so that read-only traversal works without a block device. UDF is
//! commonly used on DVDs, Blu-rays, and other optical media and is treated as
//! read-only here.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::cmp;
use spin::RwLock;

/// UDF block size (sectors are 2048 bytes on optical media).
const UDF_BLOCK_SIZE: u32 = 2048;

/// A parsed Volume Descriptor Sequence entry.
#[derive(Debug, Clone)]
struct VolumeDescriptor {
    /// Sector address of the descriptor within the VDS.
    sector: u32,
    /// Tag identifier (e.g. PVD=1, PVDP=2, IUVD=3, TD=8).
    tag_id: u16,
    /// Raw descriptor payload.
    data: Vec<u8>,
}

/// A UDF partition descriptor.
#[derive(Debug, Clone)]
struct PartitionDescriptor {
    /// Partition number.
    number: u16,
    /// Starting sector of the partition.
    start: u32,
    /// Length of the partition in sectors.
    length: u32,
}

/// In-memory UDF file entry (inode).
#[derive(Debug, Clone)]
struct UdfEntry {
    metadata: FileMetadata,
    /// File data (regular files) or empty (directories/symlinks).
    content: Vec<u8>,
    /// Directory entries (directories only).
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target.
    symlink_target: Option<String>,
    /// Allocation extent: (partition, start_sector, length_bytes).
    extent: Option<(u16, u32, u64)>,
}

impl UdfEntry {
    fn new_file(inode: InodeNumber) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: 0,
                permissions: FilePermissions::from_octal(0o444),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
            extent: None,
        }
    }

    fn new_directory(inode: InodeNumber) -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::from_octal(0o555),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 2,
                device_id: None,
            },
            content: Vec::new(),
            entries,
            symlink_target: None,
            extent: None,
        }
    }

    fn new_symlink(inode: InodeNumber, target: &str) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: target.len() as u64,
                permissions: FilePermissions::from_octal(0o777),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
            extent: None,
        }
    }
}

/// UDF filesystem — in-memory, read-only.
#[derive(Debug)]
pub struct UdfFileSystem {
    /// Volume Descriptor Sequence entries.
    vds: RwLock<Vec<VolumeDescriptor>>,
    /// Partition descriptors keyed by partition number.
    partitions: RwLock<BTreeMap<u16, PartitionDescriptor>>,
    /// All file entries keyed by inode number.
    entries: RwLock<BTreeMap<InodeNumber, UdfEntry>>,
    /// Path -> inode cache for fast lookup.
    path_map: RwLock<BTreeMap<String, InodeNumber>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode.
    root_inode: InodeNumber,
    /// Total volume size in blocks.
    volume_blocks: RwLock<u64>,
}

impl UdfFileSystem {
    /// Create a new, empty UDF filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        let root_inode = 1;
        let mut entries = BTreeMap::new();
        let mut path_map = BTreeMap::new();
        let mut root = UdfEntry::new_directory(root_inode);
        root.entries.insert("..".to_string(), root_inode);
        entries.insert(root_inode, root);
        path_map.insert("/".to_string(), root_inode);

        Ok(Self {
            vds: RwLock::new(Vec::new()),
            partitions: RwLock::new(BTreeMap::new()),
            entries: RwLock::new(entries),
            path_map: RwLock::new(path_map),
            next_inode: RwLock::new(2),
            root_inode,
            volume_blocks: RwLock::new(0),
        })
    }

    /// Register a volume descriptor in the VDS.
    pub fn add_volume_descriptor(&self, sector: u32, tag_id: u16, data: Vec<u8>) {
        self.vds.write().push(VolumeDescriptor {
            sector,
            tag_id,
            data,
        });
    }

    /// Register a partition descriptor.
    pub fn add_partition(&self, number: u16, start: u32, length: u32) {
        self.partitions.write().insert(
            number,
            PartitionDescriptor {
                number,
                start,
                length,
            },
        );
        let total: u32 = self.partitions.read().values().map(|p| p.length).sum();
        let mut vol = self.volume_blocks.write();
        *vol = (*vol).max(total as u64);
    }

    /// Insert a file entry at an absolute path. Used to populate the in-memory
    /// image. Returns the allocated inode number.
    pub fn insert_file(&self, path: &str, content: Vec<u8>) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut entries = self.entries.write();
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut entry = UdfEntry::new_file(new_inode);
        entry.metadata.size = content.len() as u64;
        entry.content = content;
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        entries.insert(new_inode, entry);
        self.path_map.write().insert(path.to_string(), new_inode);
        Ok(new_inode)
    }

    /// Insert a directory entry at an absolute path.
    pub fn insert_directory(&self, path: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') || path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut entries = self.entries.write();
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = UdfEntry::new_directory(new_inode);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        entries.insert(new_inode, dir);
        self.path_map.write().insert(path.to_string(), new_inode);
        Ok(new_inode)
    }

    /// Insert a symbolic link.
    pub fn insert_symlink(&self, path: &str, target: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut entries = self.entries.write();
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let sym = UdfEntry::new_symlink(new_inode, target);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        entries.insert(new_inode, sym);
        self.path_map.write().insert(path.to_string(), new_inode);
        Ok(new_inode)
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if let Some(&ino) = self.path_map.read().get(path) {
            return Ok(ino);
        }
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let entries = self.entries.read();
        let mut current = self.root_inode;
        for component in components {
            let entry = entries.get(&current).ok_or(FsError::NotFound)?;
            if entry.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *entry.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }
}

fn split_parent(path: &str) -> FsResult<(String, String)> {
    let trimmed = path.trim_end_matches('/');
    let idx = trimmed.rfind('/').ok_or(FsError::InvalidArgument)?;
    let parent = if idx == 0 {
        "/".to_string()
    } else {
        trimmed[..idx].to_string()
    };
    let name = trimmed[idx + 1..].to_string();
    if name.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    Ok((parent, name))
}

impl FileSystem for UdfFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Udf
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let entries = self.entries.read();
        let used = entries.len() as u64;
        let vol = *self.volume_blocks.read();
        let total_blocks = vol.max(used);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: used,
            free_inodes: 0,
            block_size: UDF_BLOCK_SIZE,
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
        let entries = self.entries.read();
        let entry = entries.get(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let len = entry.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), entry.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&entry.content[start..end]);
        Ok(n)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let entries = self.entries.read();
        let entry = entries.get(&inode).ok_or(FsError::NotFound)?;
        Ok(entry.metadata.clone())
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
        let entries = self.entries.read();
        let dir = entries.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        for (name, &child_inode) in dir.entries.iter() {
            if let Some(child) = entries.get(&child_inode) {
                out.push(DirectoryEntry {
                    name: name.clone(),
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

    fn readlink(&self, path: &str) -> FsResult<String> {
        let inode = self.resolve_path(path)?;
        let entries = self.entries.read();
        let entry = entries.get(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        entry.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
