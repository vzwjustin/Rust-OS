//! Btrfs detection, superblock parsing, and in-memory write support.
//!
//! Scans primary superblock mirrors, validates the on-disk magic, and exposes
//! volume metadata and in-memory write operations through the VFS `FileSystem`
//! trait.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::read_storage_sectors;
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use spin::RwLock;

const BTRFS_MAGIC: &[u8; 8] = b"_BHRfS_M";
const BTRFS_SUPER_INFO_SIZE: usize = 4096;

/// Superblock mirror offsets (bytes from device start).
const BTRFS_SB_MIRRORS: [u64; 4] = [
    0x10_000,
    0x40_000_000,
    0x40_000_000_000,
    0x40_000_000_000_000,
];

/// Parsed Btrfs superblock fields.
#[derive(Debug, Clone)]
struct BtrfsSuperInfo {
    bytenr: u64,
    flags: u64,
    generation: u64,
    root: u64,
    chunk_root: u64,
    total_bytes: u64,
    bytes_used: u64,
    num_devices: u64,
    sectorsize: u32,
    nodesize: u32,
    leafsize: u32,
}

/// In-memory Btrfs inode (leaf node in the B-tree).
#[derive(Debug, Clone)]
struct BtrfsNode {
    inode: InodeNumber,
    rel_path: String,
    is_dir: bool,
    size: u64,
    link_count: u32,
    permissions: FilePermissions,
    /// File data (regular files only).
    data: Vec<u8>,
    /// Directory entries: name -> child inode (directories only).
    entries: BTreeMap<String, InodeNumber>,
}

/// Btrfs volume with in-memory write support (B-tree backed by BTreeMap).
#[derive(Debug)]
pub struct BtrfsFileSystem {
    device_id: u32,
    sector_base: u64,
    super_info: BtrfsSuperInfo,
    inodes: RwLock<BTreeMap<InodeNumber, BtrfsNode>>,
    next_inode: RwLock<InodeNumber>,
}

impl BtrfsFileSystem {
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let super_info = Self::probe_superblock(device_id, sector_base)?;
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            BtrfsNode {
                inode: 1,
                rel_path: String::new(),
                is_dir: true,
                size: 0,
                link_count: 2,
                permissions: FilePermissions::default_directory(),
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        Ok(Self {
            device_id,
            sector_base,
            super_info,
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
    }

    fn probe_superblock(device_id: u32, sector_base: u64) -> FsResult<BtrfsSuperInfo> {
        for &mirror in &BTRFS_SB_MIRRORS {
            if let Ok(info) = Self::read_superblock_at(device_id, sector_base, mirror) {
                return Ok(info);
            }
        }
        Err(FsError::NotSupported)
    }

    fn read_superblock_at(
        device_id: u32,
        sector_base: u64,
        byte_offset: u64,
    ) -> FsResult<BtrfsSuperInfo> {
        let mut buf = vec![0u8; BTRFS_SUPER_INFO_SIZE];
        let sector = sector_base + byte_offset / 512;
        read_storage_sectors(device_id, sector, &mut buf).map_err(|_| FsError::IoError)?;

        if buf.len() < 0x48 || &buf[0x40..0x48] != BTRFS_MAGIC {
            return Err(FsError::NotSupported);
        }

        Ok(BtrfsSuperInfo {
            bytenr: u64::from_le_bytes(buf[0x20..0x28].try_into().unwrap()),
            flags: u64::from_le_bytes(buf[0x28..0x30].try_into().unwrap()),
            generation: u64::from_le_bytes(buf[0x48..0x50].try_into().unwrap()),
            root: u64::from_le_bytes(buf[0x50..0x58].try_into().unwrap()),
            chunk_root: u64::from_le_bytes(buf[0x58..0x60].try_into().unwrap()),
            total_bytes: u64::from_le_bytes(buf[0x60..0x68].try_into().unwrap()),
            bytes_used: u64::from_le_bytes(buf[0x68..0x70].try_into().unwrap()),
            num_devices: u64::from_le_bytes(buf[0x70..0x78].try_into().unwrap()),
            sectorsize: u32::from_le_bytes(buf[0x78..0x7c].try_into().unwrap()),
            nodesize: u32::from_le_bytes(buf[0x7c..0x80].try_into().unwrap()),
            leafsize: u32::from_le_bytes(buf[0x80..0x84].try_into().unwrap()),
        })
    }

    /// Returns true if the device appears to contain a Btrfs volume.
    pub fn detect(device_id: u32, sector_base: u64) -> bool {
        Self::probe_superblock(device_id, sector_base).is_ok()
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<BtrfsNode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut n = self.next_inode.write();
        let id = *n;
        *n += 1;
        id
    }

    /// Resolve an absolute path to an inode using the in-memory B-tree.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        if rel.is_empty() {
            return Ok(1);
        }
        let inodes = self.inodes.read();
        let mut current: InodeNumber = 1;
        for component in rel.split('/').filter(|c| !c.is_empty()) {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Return (parent_inode, filename) for a path.
    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        if rel.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let parts: Vec<&str> = rel.split('/').filter(|c| !c.is_empty()).collect();
        if parts.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let filename = parts.last().unwrap().to_string();
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = if parts.len() == 1 {
            1
        } else {
            let parent_path = format!("/{}", parts[..parts.len() - 1].join("/"));
            self.resolve_path(&parent_path)?
        };
        Ok((parent_ino, filename))
    }

    /// Create a new inode (file or directory) and link it into the parent.
    fn create_node(
        &self,
        path: &str,
        is_dir: bool,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let (parent_ino, filename) = self.resolve_parent(path)?;
        let new_ino = self.alloc_inode();
        let mut inodes = self.inodes.write();

        let parent = inodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        parent.entries.insert(filename.clone(), new_ino);

        let parent_rel = parent.rel_path.clone();
        let rel_path = if parent_rel.is_empty() {
            filename
        } else {
            format!("{}/{}", parent_rel, filename)
        };

        inodes.insert(
            new_ino,
            BtrfsNode {
                inode: new_ino,
                rel_path,
                is_dir,
                size: 0,
                link_count: if is_dir { 2 } else { 1 },
                permissions,
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        Ok(new_ino)
    }
}

impl FileSystem for BtrfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Btrfs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let block_size = self.super_info.sectorsize.max(4096);
        let total_blocks = self.super_info.total_bytes / block_size as u64;
        let used_blocks = self.super_info.bytes_used / block_size as u64;
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: total_blocks.saturating_sub(used_blocks),
            available_blocks: total_blocks.saturating_sub(used_blocks),
            total_inodes: self.inodes.read().len() as u64,
            free_inodes: 0,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.create_node(path, false, permissions)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let off = offset as usize;
        if off >= node.data.len() {
            return Ok(0);
        }
        let avail = node.data.len() - off;
        let to_copy = core::cmp::min(buffer.len(), avail);
        buffer[..to_copy].copy_from_slice(&node.data[off..off + to_copy]);
        Ok(to_copy)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let off = offset as usize;
        let end = off + buffer.len();
        if end > node.data.len() {
            node.data.resize(end, 0);
        }
        node.data[off..end].copy_from_slice(buffer);
        node.size = node.data.len() as u64;
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        let now = get_current_time();
        Ok(FileMetadata {
            inode,
            file_type: if node.is_dir {
                FileType::Directory
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
            link_count: node.link_count,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.size = metadata.size;
        node.permissions = metadata.permissions;
        node.link_count = metadata.link_count;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.create_node(path, true, permissions)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let (parent_ino, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        {
            let node = inodes.get(&ino).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            if !node.entries.is_empty() {
                return Err(FsError::DirectoryNotEmpty);
            }
        }
        inodes.remove(&ino);
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            parent.entries.remove(&filename);
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let (parent_ino, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        {
            let node = inodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.is_dir {
                return Err(FsError::IsADirectory);
            }
        }
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            parent.entries.remove(&filename);
        }
        // Decrement link count; free inode when it hits 0
        let remove = {
            let node = inodes.get_mut(&ino).ok_or(FsError::NotFound)?;
            node.link_count = node.link_count.saturating_sub(1);
            node.link_count == 0
        };
        if remove {
            inodes.remove(&ino);
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let node = self.get_node(inode)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        let inodes = self.inodes.read();
        let mut out = Vec::new();
        for (name, &child_ino) in &node.entries {
            if let Some(child) = inodes.get(&child_ino) {
                out.push(DirectoryEntry {
                    name: name.clone(),
                    inode: child_ino,
                    file_type: if child.is_dir {
                        FileType::Directory
                    } else {
                        FileType::Regular
                    },
                });
            }
        }
        Ok(out)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let ino = self.resolve_path(old_path)?;
        let (old_parent_ino, old_name) = self.resolve_parent(old_path)?;
        let (new_parent_ino, new_name) = self.resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        // Remove from old parent
        if let Some(old_parent) = inodes.get_mut(&old_parent_ino) {
            old_parent.entries.remove(&old_name);
        }
        // Evict any existing destination entry
        let victim_ino = inodes
            .get(&new_parent_ino)
            .and_then(|p| p.entries.get(&new_name))
            .copied();
        if let Some(v_ino) = victim_ino {
            let remove = if let Some(victim) = inodes.get_mut(&v_ino) {
                victim.link_count = victim.link_count.saturating_sub(1);
                victim.link_count == 0
            } else {
                false
            };
            if remove {
                inodes.remove(&v_ino);
            }
        }
        // Insert into new parent
        if let Some(new_parent) = inodes.get_mut(&new_parent_ino) {
            new_parent.entries.insert(new_name.clone(), ino);
        }
        // Update the node's rel_path
        let new_parent_rel = inodes
            .get(&new_parent_ino)
            .map(|p| p.rel_path.clone())
            .unwrap_or_default();
        if let Some(node) = inodes.get_mut(&ino) {
            node.rel_path = if new_parent_rel.is_empty() {
                new_name
            } else {
                format!("{}/{}", new_parent_rel, new_name)
            };
        }
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
