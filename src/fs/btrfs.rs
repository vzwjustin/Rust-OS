//! Btrfs read-only detection and superblock parsing.
//!
//! Scans primary superblock mirrors, validates the on-disk magic, and exposes
//! volume metadata through the VFS `FileSystem` trait (root directory only).

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::read_storage_sectors;
use alloc::{
    collections::BTreeMap,
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

/// Parsed Btrfs superblock fields used for read-only mount.
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

#[derive(Debug, Clone)]
struct BtrfsNode {
    inode: InodeNumber,
    rel_path: String,
    is_dir: bool,
    size: u64,
}

/// Read-only Btrfs volume (superblock-validated, metadata-only file access).
#[derive(Debug)]
pub struct BtrfsFileSystem {
    device_id: u32,
    sector_base: u64,
    super_info: BtrfsSuperInfo,
    inodes: RwLock<BTreeMap<InodeNumber, BtrfsNode>>,
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
            },
        );
        Ok(Self {
            device_id,
            sector_base,
            super_info,
            inodes: RwLock::new(inodes),
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
            total_inodes: 1,
            free_inodes: 0,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        if rel.is_empty() {
            return Ok(1);
        }
        Err(FsError::NotFound)
    }

    fn read(&self, inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        Ok(0)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
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
            permissions: FilePermissions::default_directory(),
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
        let node = self.get_node(inode)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        Ok(Vec::new())
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
        Ok(())
    }
}
