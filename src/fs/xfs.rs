//! XFS read-only superblock parsing framework.
//!
//! Validates the primary superblock at sector 0 and exposes volume geometry
//! through the VFS `FileSystem` trait (root directory metadata only).

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::read_storage_sectors;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

const XFS_SB_MAGIC: u32 = 0x5846_5342; // "XFSB"
const XFS_SB_SIZE: usize = 512;

/// Parsed XFS superblock fields.
#[derive(Debug, Clone)]
struct XfsSuperInfo {
    blocksize: u32,
    dblocks: u64,
    rblocks: u64,
    rextents: u64,
    rootino: u64,
    inodes: u64,
    imax_pct: u32,
    logblocks: u32,
    uuid: [u8; 16],
}

#[derive(Debug, Clone)]
struct XfsNode {
    inode: InodeNumber,
    rel_path: String,
    is_dir: bool,
    size: u64,
}

/// Read-only XFS volume (superblock-validated).
#[derive(Debug)]
pub struct XfsFileSystem {
    device_id: u32,
    sector_base: u64,
    super_info: XfsSuperInfo,
    inodes: RwLock<BTreeMap<InodeNumber, XfsNode>>,
}

impl XfsFileSystem {
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let super_info = Self::read_superblock(device_id, sector_base)?;
        let mut nodes = BTreeMap::new();
        nodes.insert(
            1,
            XfsNode {
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
            inodes: RwLock::new(nodes),
        })
    }

    fn read_superblock(device_id: u32, sector_base: u64) -> FsResult<XfsSuperInfo> {
        let mut buf = [0u8; XFS_SB_SIZE];
        read_storage_sectors(device_id, sector_base, &mut buf).map_err(|_| FsError::IoError)?;

        let magic = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        if magic != XFS_SB_MAGIC {
            return Err(FsError::NotSupported);
        }

        let blocksize = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        if blocksize == 0 || !blocksize.is_power_of_two() {
            return Err(FsError::InvalidArgument);
        }

        Ok(XfsSuperInfo {
            blocksize,
            dblocks: u64::from_be_bytes(buf[8..16].try_into().unwrap()),
            rblocks: u64::from_be_bytes(buf[16..24].try_into().unwrap()),
            rextents: u64::from_be_bytes(buf[24..32].try_into().unwrap()),
            rootino: u64::from_be_bytes(buf[56..64].try_into().unwrap()),
            inodes: u64::from_be_bytes(buf[88..96].try_into().unwrap()),
            imax_pct: u32::from_be_bytes(buf[96..100].try_into().unwrap()),
            logblocks: u32::from_be_bytes(buf[104..108].try_into().unwrap()),
            uuid: buf[32..48].try_into().unwrap(),
        })
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<XfsNode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }
}

impl FileSystem for XfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Xfs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.super_info.dblocks,
            free_blocks: self.super_info.rblocks,
            available_blocks: self.super_info.rblocks,
            total_inodes: self.super_info.inodes,
            free_inodes: 0,
            block_size: self.super_info.blocksize,
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
