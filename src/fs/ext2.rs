//! Ext2 filesystem stub scaffold
//!
//! This is a compile-clean stub for the ext2 filesystem.
//! Real implementation would parse ext2 superblocks and inode tables.
//! TODO: port from linux-master fs/ext2/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// Ext2 filesystem stub
#[derive(Debug)]
pub struct Ext2FileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, Ext2Inode>>,
    next_inode: RwLock<InodeNumber>,
}

/// Ext2 inode stub
#[derive(Debug, Clone)]
struct Ext2Inode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    data: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
}

impl Ext2FileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            Ext2Inode {
                inode: 1,
                is_dir: true,
                size: 0,
                permissions: FilePermissions::default_directory(),
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<Ext2Inode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }
}

impl FileSystem for Ext2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ext2
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/ext2/super.c
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ext2/file.c
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ext2/namei.c
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/ext2/file.c
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/ext2/file.c
        Err(FsError::NotSupported)
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
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // TODO: port from linux-master fs/ext2/inode.c
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ext2/namei.c
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ext2/namei.c
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ext2/namei.c
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/ext2/dir.c
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ext2/namei.c
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // TODO: port from linux-master fs/ext2/super.c
        Ok(())
    }
}
