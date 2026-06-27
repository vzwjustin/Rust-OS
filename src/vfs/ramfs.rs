//! RAM Filesystem (RamFs)
//!
//! A simple in-memory filesystem implementation that serves as the default
//! filesystem for RustOS. All data is stored in RAM and lost on shutdown.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use super::{DirEntry, InodeOps, InodeType, Stat, StatFs, SuperblockOps, VfsError, VfsResult};

/// RAM filesystem inode data
enum RamFsInodeData {
    /// File data (content)
    File(RwLock<Vec<u8>>),
    /// Directory entries (name -> inode)
    Directory(RwLock<BTreeMap<String, Arc<dyn InodeOps>>>),
    /// Symbolic link target path
    Symlink(RwLock<String>),
}

/// RAM filesystem inode
pub struct RamFsInode {
    /// Inode number
    ino: u64,
    /// Inode type
    inode_type: InodeType,
    /// Access mode
    mode: AtomicU32,
    /// Owner user ID
    uid: AtomicU32,
    /// Owner group ID
    gid: AtomicU32,
    /// Number of hard links
    nlink: RwLock<u32>,
    /// Access time
    atime: RwLock<u64>,
    /// Modification time
    mtime: RwLock<u64>,
    /// Change time
    ctime: RwLock<u64>,
    /// Inode data
    data: RamFsInodeData,
}

impl RamFsInode {
    /// Create a new file inode
    pub fn new_file(ino: u64, mode: u32) -> Arc<Self> {
        let now = get_time();
        Arc::new(Self {
            ino,
            inode_type: InodeType::File,
            mode: AtomicU32::new(mode),
            uid: AtomicU32::new(0),
            gid: AtomicU32::new(0),
            nlink: RwLock::new(1),
            atime: RwLock::new(now),
            mtime: RwLock::new(now),
            ctime: RwLock::new(now),
            data: RamFsInodeData::File(RwLock::new(Vec::new())),
        })
    }

    /// Create a new directory inode
    pub fn new_directory(ino: u64, mode: u32) -> Arc<Self> {
        let now = get_time();
        Arc::new(Self {
            ino,
            inode_type: InodeType::Directory,
            mode: AtomicU32::new(mode | 0o111),
            uid: AtomicU32::new(0),
            gid: AtomicU32::new(0),
            nlink: RwLock::new(2), // . and ..
            atime: RwLock::new(now),
            mtime: RwLock::new(now),
            ctime: RwLock::new(now),
            data: RamFsInodeData::Directory(RwLock::new(BTreeMap::new())),
        })
    }

    /// Create a new symlink inode
    pub fn new_symlink(ino: u64, target: &str) -> Arc<Self> {
        let now = get_time();
        Arc::new(Self {
            ino,
            inode_type: InodeType::Symlink,
            mode: AtomicU32::new(0o777),
            uid: AtomicU32::new(0),
            gid: AtomicU32::new(0),
            nlink: RwLock::new(1),
            atime: RwLock::new(now),
            mtime: RwLock::new(now),
            ctime: RwLock::new(now),
            data: RamFsInodeData::Symlink(RwLock::new(String::from(target))),
        })
    }

    /// Update modification time
    fn update_mtime(&self) {
        let now = get_time();
        *self.mtime.write() = now;
        *self.ctime.write() = now;
    }

    /// Update access time
    fn update_atime(&self) {
        *self.atime.write() = get_time();
    }
}

impl InodeOps for RamFsInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        match &self.data {
            RamFsInodeData::File(content) => {
                self.update_atime();

                let content = content.read();
                let start = offset as usize;

                if start >= content.len() {
                    return Ok(0);
                }

                let end = core::cmp::min(start + buf.len(), content.len());
                let bytes_to_copy = end - start;

                buf[..bytes_to_copy].copy_from_slice(&content[start..end]);
                Ok(bytes_to_copy)
            }
            RamFsInodeData::Directory(_) => Err(VfsError::IsDirectory),
            RamFsInodeData::Symlink(target) => {
                self.update_atime();
                let target = target.read();
                let bytes = target.as_bytes();
                let start = offset as usize;
                if start >= bytes.len() {
                    return Ok(0);
                }
                let end = core::cmp::min(start + buf.len(), bytes.len());
                let n = end - start;
                buf[..n].copy_from_slice(&bytes[start..end]);
                Ok(n)
            }
        }
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        match &self.data {
            RamFsInodeData::File(content) => {
                self.update_mtime();

                let mut content = content.write();
                let start = offset as usize;
                let end = start + buf.len();

                // Extend file if necessary
                if end > content.len() {
                    content.resize(end, 0);
                }

                content[start..end].copy_from_slice(buf);
                Ok(buf.len())
            }
            RamFsInodeData::Directory(_) => Err(VfsError::IsDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotSupported),
        }
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = match &self.data {
            RamFsInodeData::File(content) => content.read().len() as u64,
            RamFsInodeData::Directory(entries) => entries.read().len() as u64,
            RamFsInodeData::Symlink(target) => target.read().len() as u64,
        };

        let blocks = (size + 511) / 512;

        Ok(Stat {
            ino: self.ino,
            inode_type: self.inode_type,
            size,
            blksize: 4096,
            blocks,
            mode: self.mode.load(Ordering::Relaxed),
            nlink: *self.nlink.read(),
            uid: self.uid.load(Ordering::Relaxed),
            gid: self.gid.load(Ordering::Relaxed),
            rdev: 0,
            atime: *self.atime.read(),
            mtime: *self.mtime.read(),
            ctime: *self.ctime.read(),
        })
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        match &self.data {
            RamFsInodeData::File(content) => {
                self.update_mtime();
                content.write().resize(size as usize, 0);
                Ok(())
            }
            RamFsInodeData::Directory(_) => Err(VfsError::IsDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotSupported),
        }
    }

    fn sync(&self) -> VfsResult<()> {
        // RAM filesystem has no backing store, so sync is a no-op
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                self.update_atime();

                let entries = entries.read();
                entries
                    .get(name)
                    .cloned()
                    .ok_or(VfsError::NotFound)
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn create(&self, name: &str, inode_type: InodeType, mode: u32) -> VfsResult<Arc<dyn InodeOps>> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                if name.len() > 255 {
                    return Err(VfsError::NameTooLong);
                }

                if name.contains('/') || name == "." || name == ".." {
                    return Err(VfsError::InvalidArgument);
                }

                self.update_mtime();

                let mut entries = entries.write();

                // Check if entry already exists
                if entries.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }

                // Allocate new inode number
                let ino = super::get_vfs().alloc_ino();

                // Create new inode
                let new_inode: Arc<dyn InodeOps> = match inode_type {
                    InodeType::File => RamFsInode::new_file(ino, mode),
                    InodeType::Directory => RamFsInode::new_directory(ino, mode),
                    InodeType::Symlink => RamFsInode::new_symlink(ino, ""),
                    _ => return Err(VfsError::NotSupported),
                };

                entries.insert(String::from(name), Arc::clone(&new_inode));

                // Increment link count
                *self.nlink.write() += 1;

                Ok(new_inode)
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                self.update_mtime();

                let mut entries = entries.write();
                let _inode = entries.remove(name).ok_or(VfsError::NotFound)?;
                *self.nlink.write() -= 1;

                Ok(())
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn link(&self, name: &str, target: Arc<dyn InodeOps>) -> VfsResult<()> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                if name.len() > 255 {
                    return Err(VfsError::NameTooLong);
                }

                if name.contains('/') || name == "." || name == ".." {
                    return Err(VfsError::InvalidArgument);
                }

                self.update_mtime();

                let mut entries = entries.write();

                // Check if entry already exists
                if entries.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }

                entries.insert(String::from(name), Arc::clone(&target));
                *self.nlink.write() += 1;

                Ok(())
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn rename(&self, old_name: &str, new_dir: Arc<dyn InodeOps>, new_name: &str) -> VfsResult<()> {
        // Remove from source directory
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                let inode = {
                    let mut entries = entries.write();
                    entries.remove(old_name).ok_or(VfsError::NotFound)?
                };

                // Add to destination directory
                new_dir.link(new_name, inode)?;

                self.update_mtime();
                Ok(())
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                self.update_atime();

                let entries = entries.read();
                let mut result = Vec::with_capacity(entries.len());

                for (name, inode) in entries.iter() {
                    let stat = inode.stat()?;
                    result.push(DirEntry {
                        ino: stat.ino,
                        name: name.clone(),
                        inode_type: inode.inode_type(),
                    });
                }

                Ok(result)
            }
            RamFsInodeData::File(_) => Err(VfsError::NotDirectory),
            RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn inode_type(&self) -> InodeType {
        self.inode_type
    }

    fn read_symlink_target(&self) -> VfsResult<alloc::string::String> {
        match &self.data {
            RamFsInodeData::Symlink(target) => Ok(target.read().clone()),
            _ => Err(VfsError::NotSupported),
        }
    }

    fn write_symlink_target(&self, target: &str) -> VfsResult<()> {
        match &self.data {
            RamFsInodeData::Symlink(t) => {
                *t.write() = String::from(target);
                self.update_mtime();
                Ok(())
            }
            _ => Err(VfsError::NotSupported),
        }
    }

    fn attach_child(&self, name: &str, child: Arc<dyn InodeOps>) -> VfsResult<()> {
        match &self.data {
            RamFsInodeData::Directory(entries) => {
                if name.len() > 255 {
                    return Err(VfsError::NameTooLong);
                }
                if name.contains('/') || name == "." || name == ".." {
                    return Err(VfsError::InvalidArgument);
                }
                let mut entries = entries.write();
                if entries.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }
                entries.insert(String::from(name), child);
                *self.nlink.write() += 1;
                Ok(())
            }
            RamFsInodeData::File(_) | RamFsInodeData::Symlink(_) => Err(VfsError::NotDirectory),
        }
    }

    fn set_mode(&self, mode: u32) -> VfsResult<()> {
        self.mode.store(mode, Ordering::Relaxed);
        *self.ctime.write() = get_time();
        Ok(())
    }

    fn set_owner(&self, uid: u32, gid: u32) -> VfsResult<()> {
        self.uid.store(uid, Ordering::Relaxed);
        self.gid.store(gid, Ordering::Relaxed);
        *self.ctime.write() = get_time();
        Ok(())
    }
}

/// RAM filesystem superblock
pub struct RamFs {
    /// Root inode
    root: Arc<RamFsInode>,
}

impl RamFs {
    /// Create a new RAM filesystem
    pub fn new() -> Self {
        let root = RamFsInode::new_directory(1, 0o755);
        Self { root }
    }
}

impl SuperblockOps for RamFs {
    fn root(&self) -> Arc<dyn InodeOps> {
        Arc::clone(&self.root) as Arc<dyn InodeOps>
    }

    fn sync_fs(&self) -> VfsResult<()> {
        // RAM filesystem has no backing store
        Ok(())
    }

    fn statfs(&self) -> VfsResult<StatFs> {
        // Return dummy statistics
        Ok(StatFs {
            fs_type: 0x858458f6, // RAMFS_MAGIC
            block_size: 4096,
            total_blocks: 0,
            free_blocks: 0,
            avail_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            max_name_len: 255,
        })
    }
}

/// Get current time (stub implementation)
fn get_time() -> u64 {
    // TODO: Integrate with kernel time system
    0
}
