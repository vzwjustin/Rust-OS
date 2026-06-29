//! Bridge `crate::fs` block filesystems into the kernel VFS (`SuperblockOps`).

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::fs::{
    btrfs, ext4, f2fs, fat32, xfs, DirectoryEntry, FileMetadata, FileSystem, FileSystemType,
    FileType, FsError, InodeNumber, OpenFlags,
};

use super::{InodeOps, InodeType, Stat, StatFs, SuperblockOps, VfsError, VfsResult};

static NEXT_LEGACY_INO: AtomicU64 = AtomicU64::new(50_000);

fn alloc_ino() -> u64 {
    NEXT_LEGACY_INO.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
enum BackingFs {
    Ext4(Arc<ext4::Ext4FileSystem>),
    Fat32(Arc<fat32::Fat32FileSystem>),
    Iso9660(Arc<crate::fs::isofs::Iso9660FileSystem>),
    F2fs(Arc<f2fs::F2fsFileSystem>),
    Btrfs(Arc<btrfs::BtrfsFileSystem>),
    Xfs(Arc<xfs::XfsFileSystem>),
}

impl BackingFs {
    fn open_path(&self, path: &str) -> Result<(InodeNumber, FileMetadata), FsError> {
        let flags = OpenFlags {
            read: true,
            write: false,
            create: false,
            truncate: false,
            append: false,
            exclusive: false,
        };
        let ino = match self {
            BackingFs::Ext4(fs) => fs.open(path, flags)?,
            BackingFs::Fat32(fs) => fs.open(path, flags)?,
            BackingFs::Iso9660(fs) => fs.open(path, flags)?,
            BackingFs::F2fs(fs) => fs.open(path, flags)?,
            BackingFs::Btrfs(fs) => fs.open(path, flags)?,
            BackingFs::Xfs(fs) => fs.open(path, flags)?,
        };
        let meta = self.metadata(ino)?;
        Ok((ino, meta))
    }

    fn metadata(&self, inode: InodeNumber) -> Result<FileMetadata, FsError> {
        match self {
            BackingFs::Ext4(fs) => fs.metadata(inode),
            BackingFs::Fat32(fs) => fs.metadata(inode),
            BackingFs::Iso9660(fs) => fs.metadata(inode),
            BackingFs::F2fs(fs) => fs.metadata(inode),
            BackingFs::Btrfs(fs) => fs.metadata(inode),
            BackingFs::Xfs(fs) => fs.metadata(inode),
        }
    }

    fn read(&self, inode: InodeNumber, offset: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        match self {
            BackingFs::Ext4(fs) => fs.read(inode, offset, buf),
            BackingFs::Fat32(fs) => fs.read(inode, offset, buf),
            BackingFs::Iso9660(fs) => fs.read(inode, offset, buf),
            BackingFs::F2fs(fs) => fs.read(inode, offset, buf),
            BackingFs::Btrfs(fs) => fs.read(inode, offset, buf),
            BackingFs::Xfs(fs) => fs.read(inode, offset, buf),
        }
    }

    fn readdir(&self, inode: InodeNumber) -> Result<Vec<DirectoryEntry>, FsError> {
        match self {
            BackingFs::Ext4(fs) => fs.readdir(inode),
            BackingFs::Fat32(fs) => fs.readdir(inode),
            BackingFs::Iso9660(fs) => fs.readdir(inode),
            BackingFs::F2fs(fs) => fs.readdir(inode),
            BackingFs::Btrfs(fs) => fs.readdir(inode),
            BackingFs::Xfs(fs) => fs.readdir(inode),
        }
    }

    fn readlink(&self, path: &str) -> Result<String, FsError> {
        match self {
            BackingFs::Ext4(fs) => fs.readlink(path),
            BackingFs::Fat32(fs) => fs.readlink(path),
            BackingFs::Iso9660(fs) => fs.readlink(path),
            BackingFs::F2fs(fs) => fs.readlink(path),
            BackingFs::Btrfs(fs) => fs.readlink(path),
            BackingFs::Xfs(fs) => fs.readlink(path),
        }
    }

    fn statfs(&self) -> Result<crate::fs::FileSystemStats, FsError> {
        match self {
            BackingFs::Ext4(fs) => fs.statfs(),
            BackingFs::Fat32(fs) => fs.statfs(),
            BackingFs::Iso9660(fs) => fs.statfs(),
            BackingFs::F2fs(fs) => fs.statfs(),
            BackingFs::Btrfs(fs) => fs.statfs(),
            BackingFs::Xfs(fs) => fs.statfs(),
        }
    }

    fn fs_type(&self) -> FileSystemType {
        match self {
            BackingFs::Ext4(fs) => fs.fs_type(),
            BackingFs::Fat32(fs) => fs.fs_type(),
            BackingFs::Iso9660(fs) => fs.fs_type(),
            BackingFs::F2fs(fs) => fs.fs_type(),
            BackingFs::Btrfs(fs) => fs.fs_type(),
            BackingFs::Xfs(fs) => fs.fs_type(),
        }
    }
}

fn join_path(base: &str, name: &str) -> String {
    if base == "/" {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn map_file_type(ft: FileType) -> InodeType {
    match ft {
        FileType::Directory => InodeType::Directory,
        FileType::SymbolicLink => InodeType::Symlink,
        FileType::CharacterDevice => InodeType::CharDevice,
        FileType::BlockDevice => InodeType::BlockDevice,
        FileType::NamedPipe => InodeType::Fifo,
        FileType::Socket => InodeType::Socket,
        FileType::Regular => InodeType::File,
    }
}

struct LegacyInode {
    backing: Arc<BackingFs>,
    path: String,
    inode: InodeNumber,
    file_type: FileType,
    ino: u64,
    mode: u32,
    size: u64,
}

impl LegacyInode {
    fn from_open(
        backing: Arc<BackingFs>,
        path: String,
        inode: InodeNumber,
        meta: FileMetadata,
    ) -> Arc<Self> {
        Arc::new(Self {
            mode: meta.permissions.to_octal() as u32
                | match meta.file_type {
                    FileType::Directory => 0o040000,
                    FileType::SymbolicLink => 0o120000,
                    FileType::CharacterDevice => 0o020000,
                    FileType::BlockDevice => 0o060000,
                    FileType::NamedPipe => 0o010000,
                    FileType::Socket => 0o140000,
                    FileType::Regular => 0o100000,
                },
            size: meta.size,
            file_type: meta.file_type,
            backing,
            path,
            inode,
            ino: alloc_ino(),
        })
    }
}

impl InodeOps for LegacyInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if self.file_type == FileType::Directory {
            return Err(VfsError::IsDirectory);
        }
        self.backing
            .read(self.inode, offset, buf)
            .map_err(|_| VfsError::IoError)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: map_file_type(self.file_type),
            size: self.size,
            blksize: 4096,
            blocks: (self.size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        if self.file_type != FileType::Directory {
            return Err(VfsError::NotDirectory);
        }
        if name == "." {
            return Ok(LegacyInode::from_open(
                Arc::clone(&self.backing),
                self.path.clone(),
                self.inode,
                self.backing
                    .metadata(self.inode)
                    .map_err(|_| VfsError::IoError)?,
            ));
        }
        if name == ".." {
            let parent = if self.path == "/" {
                "/".to_string()
            } else {
                self.path
                    .rsplit('/')
                    .nth(1)
                    .map(|p| {
                        if p.is_empty() {
                            "/".to_string()
                        } else {
                            format!("/{p}")
                        }
                    })
                    .unwrap_or_else(|| "/".to_string())
            };
            let (ino, meta) = self
                .backing
                .open_path(&parent)
                .map_err(|_| VfsError::NotFound)?;
            return Ok(LegacyInode::from_open(
                Arc::clone(&self.backing),
                parent,
                ino,
                meta,
            ));
        }
        let child_path = join_path(&self.path, name);
        let (ino, meta) = self
            .backing
            .open_path(&child_path)
            .map_err(|_| VfsError::NotFound)?;
        Ok(LegacyInode::from_open(
            Arc::clone(&self.backing),
            child_path,
            ino,
            meta,
        ))
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<super::DirEntry>> {
        if self.file_type != FileType::Directory {
            return Err(VfsError::NotDirectory);
        }
        let entries = self
            .backing
            .readdir(self.inode)
            .map_err(|_| VfsError::IoError)?;
        Ok(entries
            .into_iter()
            .map(|e| super::DirEntry {
                ino: e.inode as u64,
                name: e.name,
                inode_type: map_file_type(e.file_type),
            })
            .collect())
    }

    fn inode_type(&self) -> InodeType {
        map_file_type(self.file_type)
    }

    fn read_symlink_target(&self) -> VfsResult<String> {
        if self.file_type != FileType::SymbolicLink {
            return Err(VfsError::NotSupported);
        }
        self.backing
            .readlink(&self.path)
            .map_err(|_| VfsError::IoError)
    }
}

pub struct LegacyMount {
    backing: Arc<BackingFs>,
    root: Arc<dyn InodeOps>,
}

impl LegacyMount {
    pub fn from_ext4(device_id: u32, sector_base: u64) -> Result<Self, FsError> {
        let fs = if sector_base == 0 {
            ext4::Ext4FileSystem::new(device_id)?
        } else {
            ext4::Ext4FileSystem::new_at(device_id, sector_base)?
        };
        let backing = Arc::new(BackingFs::Ext4(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }

    pub fn from_fat32(device_id: u32, sector_base: u64) -> Result<Self, FsError> {
        let fs = if sector_base == 0 {
            fat32::Fat32FileSystem::new(device_id)?
        } else {
            fat32::Fat32FileSystem::new_at(device_id, sector_base)?
        };
        let backing = Arc::new(BackingFs::Fat32(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }

    pub fn from_iso9660(device_id: u32) -> Result<Self, FsError> {
        let fs = crate::fs::isofs::Iso9660FileSystem::new(device_id)?;
        let backing = Arc::new(BackingFs::Iso9660(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }

    pub fn from_f2fs(device_id: u32, sector_base: u64) -> Result<Self, FsError> {
        let fs = if sector_base == 0 {
            f2fs::F2fsFileSystem::new(device_id)?
        } else {
            f2fs::F2fsFileSystem::new_at(device_id, sector_base)?
        };
        let backing = Arc::new(BackingFs::F2fs(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }

    pub fn from_btrfs(device_id: u32, sector_base: u64) -> Result<Self, FsError> {
        let fs = if sector_base == 0 {
            btrfs::BtrfsFileSystem::new(device_id)?
        } else {
            btrfs::BtrfsFileSystem::new_at(device_id, sector_base)?
        };
        let backing = Arc::new(BackingFs::Btrfs(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }

    pub fn from_xfs(device_id: u32, sector_base: u64) -> Result<Self, FsError> {
        let fs = if sector_base == 0 {
            xfs::XfsFileSystem::new(device_id)?
        } else {
            xfs::XfsFileSystem::new_at(device_id, sector_base)?
        };
        let backing = Arc::new(BackingFs::Xfs(Arc::new(fs)));
        let (ino, meta) = backing.open_path("/")?;
        let root = LegacyInode::from_open(backing.clone(), String::from("/"), ino, meta);
        Ok(Self { backing, root })
    }
}

impl SuperblockOps for LegacyMount {
    fn root(&self) -> Arc<dyn InodeOps> {
        Arc::clone(&self.root)
    }

    fn sync_fs(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<StatFs> {
        let stats = self.backing.statfs().map_err(|_| VfsError::IoError)?;
        let fs_type = match self.backing.fs_type() {
            FileSystemType::Ext2 => 0xEF53,
            FileSystemType::Fat32 => 0x4d44,
            FileSystemType::Iso9660 => 0x9660,
            FileSystemType::F2fs => 0xF2F5_2010,
            FileSystemType::Btrfs => 0x9123_683E,
            FileSystemType::Xfs => 0x5846_5342,
            _ => 0,
        };
        Ok(StatFs {
            fs_type,
            block_size: stats.block_size as u64,
            total_blocks: stats.total_blocks,
            free_blocks: stats.free_blocks,
            avail_blocks: stats.available_blocks,
            total_inodes: stats.total_inodes,
            free_inodes: stats.free_inodes,
            max_name_len: stats.max_filename_length as u64,
        })
    }
}

/// Mount a block device (by path) at `target` using the requested fstype hint.
pub fn mount_block_device(source: &str, target: &str, fstype: &str) -> VfsResult<()> {
    let spec = super::devfs::block_device_spec(source).ok_or(VfsError::NotFound)?;
    let sb: Arc<dyn SuperblockOps> = match fstype {
        "vfat" | "fat" | "msdos" => Arc::new(
            LegacyMount::from_fat32(spec.device_id, spec.start_sector)
                .map_err(|_| VfsError::IoError)?,
        ),
        "iso9660" => {
            Arc::new(LegacyMount::from_iso9660(spec.device_id).map_err(|_| VfsError::IoError)?)
        }
        "f2fs" => Arc::new(
            LegacyMount::from_f2fs(spec.device_id, spec.start_sector)
                .map_err(|_| VfsError::IoError)?,
        ),
        "btrfs" => Arc::new(
            LegacyMount::from_btrfs(spec.device_id, spec.start_sector)
                .map_err(|_| VfsError::IoError)?,
        ),
        "xfs" => Arc::new(
            LegacyMount::from_xfs(spec.device_id, spec.start_sector)
                .map_err(|_| VfsError::IoError)?,
        ),
        _ => {
            if let Ok(m) = LegacyMount::from_ext4(spec.device_id, spec.start_sector) {
                Arc::new(m)
            } else if let Ok(m) = LegacyMount::from_f2fs(spec.device_id, spec.start_sector) {
                Arc::new(m)
            } else if let Ok(m) = LegacyMount::from_btrfs(spec.device_id, spec.start_sector) {
                Arc::new(m)
            } else if let Ok(m) = LegacyMount::from_xfs(spec.device_id, spec.start_sector) {
                Arc::new(m)
            } else {
                Arc::new(
                    LegacyMount::from_fat32(spec.device_id, spec.start_sector)
                        .map_err(|_| VfsError::IoError)?,
                )
            }
        }
    };
    super::get_vfs().mount(target, sb)
}
