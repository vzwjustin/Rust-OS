//! SquashFS read-only filesystem implementation.
//!
//! In-memory VFS implementation of SquashFS mount state. It models the
//! superblock, inode table, and fragment table as in-RAM structures so that
//! read-only traversal and decompression-state tracking work without a block
//! device. SquashFS is a compressed, read-only filesystem.

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

/// SquashFS block size (default 128 KiB).
const SQUASHFS_BLOCK_SIZE: u32 = 131_072;

/// Compression codec tracked for the mounted image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionCodec {
    None,
    Zlib,
    Lz4,
    Lzo,
    Xz,
    Zstd,
}

/// In-memory SquashFS superblock.
#[derive(Debug, Clone)]
struct SquashfsSuperblock {
    /// Magic number (hsqs).
    magic: u32,
    /// Total number of inodes.
    inode_count: u32,
    /// Last modification time.
    mod_time: u32,
    /// Block size.
    block_size: u32,
    /// Fragment count.
    fragment_count: u32,
    /// Compression codec id.
    compression: CompressionCodec,
    /// Number of block logs (block_size = 1 << block_log).
    block_log: u16,
    /// Flags.
    flags: u32,
}

/// A fragment table entry.
#[derive(Debug, Clone)]
struct FragmentEntry {
    /// Index into the fragment table.
    index: u32,
    /// On-disk offset of the compressed fragment block.
    offset: u64,
    /// Size of the compressed block (MSB set means uncompressed).
    size: u32,
    /// Decompressed payload cached in memory.
    data: Vec<u8>,
}

/// In-memory SquashFS inode.
#[derive(Debug, Clone)]
struct SquashfsInode {
    metadata: FileMetadata,
    /// Decompressed file content (regular files).
    content: Vec<u8>,
    /// Directory entries (directories only).
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target.
    symlink_target: Option<String>,
    /// Block/extent offsets within the image.
    block_start: u64,
    block_count: u32,
    /// Fragment index backing this inode (if tail-packed).
    fragment_index: Option<u32>,
}

impl SquashfsInode {
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
            block_start: 0,
            block_count: 0,
            fragment_index: None,
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
            block_start: 0,
            block_count: 0,
            fragment_index: None,
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
            block_start: 0,
            block_count: 0,
            fragment_index: None,
        }
    }
}

/// SquashFS filesystem — in-memory, read-only.
#[derive(Debug)]
pub struct SquashfsFileSystem {
    /// Parsed superblock.
    superblock: RwLock<SquashfsSuperblock>,
    /// Inode table keyed by inode number.
    inode_table: RwLock<BTreeMap<InodeNumber, SquashfsInode>>,
    /// Fragment table keyed by fragment index.
    fragment_table: RwLock<BTreeMap<u32, FragmentEntry>>,
    /// Path -> inode cache.
    path_map: RwLock<BTreeMap<String, InodeNumber>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode.
    root_inode: InodeNumber,
}

impl SquashfsFileSystem {
    /// Create a new, empty SquashFS filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        let root_inode = 1;
        let mut inode_table = BTreeMap::new();
        let mut path_map = BTreeMap::new();
        let mut root = SquashfsInode::new_directory(root_inode);
        root.entries.insert("..".to_string(), root_inode);
        inode_table.insert(root_inode, root);
        path_map.insert("/".to_string(), root_inode);

        Ok(Self {
            superblock: RwLock::new(SquashfsSuperblock {
                magic: 0x7371_7368, // "hsqs"
                inode_count: 1,
                mod_time: 0,
                block_size: SQUASHFS_BLOCK_SIZE,
                fragment_count: 0,
                compression: CompressionCodec::None,
                block_log: 17,
                flags: 0,
            }),
            inode_table: RwLock::new(inode_table),
            fragment_table: RwLock::new(BTreeMap::new()),
            path_map: RwLock::new(path_map),
            next_inode: RwLock::new(2),
            root_inode,
        })
    }

    /// Set the compression codec tracked by the superblock.
    pub fn set_compression(&self, codec: CompressionCodec) {
        self.superblock.write().compression = codec;
    }

    /// Register a fragment in the fragment table and return its index.
    pub fn add_fragment(&self, offset: u64, size: u32, data: Vec<u8>) -> u32 {
        let mut frag = self.fragment_table.write();
        let index = frag.len() as u32;
        frag.insert(
            index,
            FragmentEntry {
                index,
                offset,
                size,
                data,
            },
        );
        self.superblock.write().fragment_count = frag.len() as u32;
        index
    }

    /// Look up a cached fragment by index.
    pub fn get_fragment(&self, index: u32) -> FsResult<Vec<u8>> {
        self.fragment_table
            .read()
            .get(&index)
            .map(|f| f.data.clone())
            .ok_or(FsError::NotFound)
    }

    /// Insert a file at an absolute path with the given decompressed content.
    pub fn insert_file(&self, path: &str, content: Vec<u8>) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut table = self.inode_table.write();
        let parent = table.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut entry = SquashfsInode::new_file(new_inode);
        entry.metadata.size = content.len() as u64;
        entry.block_count = ((content.len() as u64 + SQUASHFS_BLOCK_SIZE as u64 - 1)
            / SQUASHFS_BLOCK_SIZE as u64) as u32;
        entry.content = content;
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        table.insert(new_inode, entry);
        self.path_map.write().insert(path.to_string(), new_inode);
        self.superblock.write().inode_count = table.len() as u32;
        Ok(new_inode)
    }

    /// Insert a directory at an absolute path.
    pub fn insert_directory(&self, path: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') || path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut table = self.inode_table.write();
        let parent = table.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = SquashfsInode::new_directory(new_inode);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        table.insert(new_inode, dir);
        self.path_map.write().insert(path.to_string(), new_inode);
        self.superblock.write().inode_count = table.len() as u32;
        Ok(new_inode)
    }

    /// Insert a symbolic link.
    pub fn insert_symlink(&self, path: &str, target: &str) -> FsResult<InodeNumber> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        let (parent_path, name) = split_parent(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut table = self.inode_table.write();
        let parent = table.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let sym = SquashfsInode::new_symlink(new_inode, target);
        parent.entries.insert(name.clone(), new_inode);
        parent.metadata.modified = get_current_time();
        table.insert(new_inode, sym);
        self.path_map.write().insert(path.to_string(), new_inode);
        self.superblock.write().inode_count = table.len() as u32;
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
        let table = self.inode_table.read();
        let mut current = self.root_inode;
        for component in components {
            let entry = table.get(&current).ok_or(FsError::NotFound)?;
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

impl FileSystem for SquashfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SquashFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let sb = self.superblock.read();
        let table = self.inode_table.read();
        let used = table.len() as u64;
        Ok(FileSystemStats {
            total_blocks: used,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: sb.inode_count as u64,
            free_inodes: 0,
            block_size: sb.block_size,
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
        let table = self.inode_table.read();
        let entry = table.get(&inode).ok_or(FsError::NotFound)?;
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
        let table = self.inode_table.read();
        let entry = table.get(&inode).ok_or(FsError::NotFound)?;
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
        let table = self.inode_table.read();
        let dir = table.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        for (name, &child_inode) in dir.entries.iter() {
            if let Some(child) = table.get(&child_inode) {
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
        let table = self.inode_table.read();
        let entry = table.get(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        entry.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
