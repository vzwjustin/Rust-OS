//! ROMFS read-only filesystem implementation.
//!
//! In-memory VFS implementation of ROMFS mount state. It models the ROMFS
//! file header and data layout as in-RAM structures so that read-only
//! traversal works without a block device. ROMFS is a simple, read-only
//! filesystem commonly used in embedded systems and initramfs.

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

/// ROMFS magic string ("-rom1fs-").
const ROMFS_MAGIC: &[u8] = b"-rom1fs-";
/// ROMFS block size (every header is 16-byte aligned).
const ROMFS_BLOCK_SIZE: u32 = 16;

/// In-memory ROMFS file header.
#[derive(Debug, Clone)]
struct RomfsHeader {
    /// Offset of the header within the image (simulated).
    offset: u32,
    /// Next-file offset (0 means last in this directory).
    next: u32,
    /// File type/spec bits (mode).
    spec: u32,
    /// Size of the file data in bytes.
    size: u32,
    /// Checksum of the header.
    checksum: u32,
}

/// In-memory ROMFS inode.
#[derive(Debug, Clone)]
struct RomfsInode {
    metadata: FileMetadata,
    /// Parsed header.
    header: RomfsHeader,
    /// File data payload (regular files).
    content: Vec<u8>,
    /// Directory entries (directories only).
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target.
    symlink_target: Option<String>,
}

impl RomfsInode {
    fn new_file(inode: InodeNumber, offset: u32, content: Vec<u8>) -> Self {
        let size = content.len() as u32;
        let next = offset + 16 + ((size + 15) & !15);
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: size as u64,
                permissions: FilePermissions::from_octal(0o444),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            },
            header: RomfsHeader {
                offset,
                next,
                spec: 0o100_000, // regular file
                size,
                checksum: 0,
            },
            content,
            entries: BTreeMap::new(),
            symlink_target: None,
        }
    }

    fn new_directory(inode: InodeNumber, offset: u32) -> Self {
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
            header: RomfsHeader {
                offset,
                next: 0,
                spec: 0o040_000, // directory
                size: 0,
                checksum: 0,
            },
            content: Vec::new(),
            entries,
            symlink_target: None,
        }
    }

    fn new_symlink(inode: InodeNumber, offset: u32, target: &str) -> Self {
        let size = target.len() as u32;
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: size as u64,
                permissions: FilePermissions::from_octal(0o777),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            },
            header: RomfsHeader {
                offset,
                next: offset + 16 + ((size + 15) & !15),
                spec: 0o120_000, // symlink
                size,
                checksum: 0,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// ROMFS filesystem — in-memory, read-only.
#[derive(Debug)]
pub struct RomfsFileSystem {
    /// Magic bytes tracked for validation.
    magic: [u8; 8],
    /// Total simulated image size in bytes.
    image_size: RwLock<u32>,
    /// All inodes keyed by inode number.
    inodes: RwLock<BTreeMap<InodeNumber, RomfsInode>>,
    /// Path -> inode cache.
    path_map: RwLock<BTreeMap<String, InodeNumber>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Next free offset within the simulated image.
    next_offset: RwLock<u32>,
    /// Root directory inode.
    root_inode: InodeNumber,
}

impl RomfsFileSystem {
    /// Create a new, empty ROMFS filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();
        let mut path_map = BTreeMap::new();
        // The root header sits at offset 16 (after the 16-byte superblock).
        let root = RomfsInode::new_directory(root_inode, 16);
        inodes.insert(root_inode, root);
        path_map.insert("/".to_string(), root_inode);

        let mut magic = [0u8; 8];
        magic.copy_from_slice(ROMFS_MAGIC);

        Ok(Self {
            magic,
            image_size: RwLock::new(16),
            inodes: RwLock::new(inodes),
            path_map: RwLock::new(path_map),
            next_inode: RwLock::new(2),
            next_offset: RwLock::new(32),
            root_inode,
        })
    }

    /// Validate the tracked magic bytes.
    pub fn validate_magic(&self) -> bool {
        &self.magic == ROMFS_MAGIC
    }

    /// Insert a file at an absolute path with the given content.
    pub fn insert_file(&self, path: &str, content: Vec<u8>) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let offset = {
            let mut off = self.next_offset.write();
            let cur = *off;
            *off = cur + 16 + ((content.len() as u32 + 15) & !15);
            cur
        };
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let entry = RomfsInode::new_file(new_inode, offset, content);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, entry);
        self.path_map.write().insert(path.to_string(), new_inode);
        let mut img = self.image_size.write();
        *img = (*img).max(offset + 16);
        Ok(new_inode)
    }

    /// Insert a directory at an absolute path.
    pub fn insert_directory(&self, path: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') || path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let offset = {
            let mut off = self.next_offset.write();
            let cur = *off;
            *off = cur + 16;
            cur
        };
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = RomfsInode::new_directory(new_inode, offset);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        inodes.insert(new_inode, dir);
        self.path_map.write().insert(path.to_string(), new_inode);
        let mut img = self.image_size.write();
        *img = (*img).max(offset + 16);
        Ok(new_inode)
    }

    /// Insert a symbolic link.
    pub fn insert_symlink(&self, path: &str, target: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let offset = {
            let mut off = self.next_offset.write();
            let cur = *off;
            *off = cur + 16 + ((target.len() as u32 + 15) & !15);
            cur
        };
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let sym = RomfsInode::new_symlink(new_inode, offset, target);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, sym);
        self.path_map.write().insert(path.to_string(), new_inode);
        let mut img = self.image_size.write();
        *img = (*img).max(offset + 16);
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
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let entry = inodes.get(&current).ok_or(FsError::NotFound)?;
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

impl FileSystem for RomfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RomFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let img = *self.image_size.read() as u64;
        let total_blocks = (img + ROMFS_BLOCK_SIZE as u64 - 1) / ROMFS_BLOCK_SIZE as u64;
        Ok(FileSystemStats {
            total_blocks: total_blocks.max(used),
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: used,
            free_inodes: 0,
            block_size: ROMFS_BLOCK_SIZE,
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
        let entry = inodes.get(&inode).ok_or(FsError::NotFound)?;
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
        let inodes = self.inodes.read();
        let entry = inodes.get(&inode).ok_or(FsError::NotFound)?;
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
        let inodes = self.inodes.read();
        let dir = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        for (name, &child_inode) in dir.entries.iter() {
            if let Some(child) = inodes.get(&child_inode) {
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
        let inodes = self.inodes.read();
        let entry = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        entry.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
