//! OverlayFS stackable filesystem implementation
//!
//! Merges a lower read-only directory and an upper read-write directory
//! into a single merged view. Supports copy-up on write and whiteouts on delete.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

fn overlay_quota_mount(path: &str) -> String {
    crate::quota::mount_for_path(path)
}

fn map_quota_err(err: crate::vfs::VfsError) -> FsError {
    match err {
        crate::vfs::VfsError::DiskQuotaExceeded => FsError::NoSpaceLeft,
        _ => FsError::IoError,
    }
}

#[derive(Debug, Clone)]
struct OverlayNode {
    inode: InodeNumber,
    rel_path: String,
    is_dir: bool,
}

/// OverlayFS implementation
#[derive(Debug)]
pub struct OverlayFs {
    lower_dir: String,
    upper_dir: String,
    inodes: RwLock<BTreeMap<InodeNumber, OverlayNode>>,
    next_inode: RwLock<u64>,
}

impl OverlayFs {
    /// Create a new OverlayFS instance.
    ///
    /// * `lower_dir` - Absolute path to the lower (read-only) directory.
    /// * `upper_dir` - Absolute path to the upper (read-write) directory.
    pub fn new(lower_dir: String, upper_dir: String) -> Self {
        let s = Self {
            lower_dir,
            upper_dir,
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(2), // Root is 1
        };

        // Register root node
        s.inodes.write().insert(
            1,
            OverlayNode {
                inode: 1,
                rel_path: "".to_string(),
                is_dir: true,
            },
        );

        s
    }

    fn alloc_inode(&self, rel_path: &str, is_dir: bool) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;

        let mut inodes = self.inodes.write();
        inodes.insert(
            inode,
            OverlayNode {
                inode,
                rel_path: rel_path.to_string(),
                is_dir,
            },
        );

        inode
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<OverlayNode> {
        let inodes = self.inodes.read();
        inodes.get(&inode).cloned().ok_or(FsError::NotFound)
    }

    /// Check if a file is whited out in the upper directory.
    fn is_whited_out(&self, rel_path: &str) -> bool {
        let filename = rel_path.split('/').last().unwrap_or("");
        if filename.is_empty() {
            return false;
        }

        let parent_path = if let Some(idx) = rel_path.rfind('/') {
            &rel_path[..idx]
        } else {
            ""
        };

        let wh_path = if parent_path.is_empty() {
            alloc::format!("{}/.wh.{}", self.upper_dir, filename)
        } else {
            alloc::format!("{}/{}/.wh.{}", self.upper_dir, parent_path, filename)
        };

        // Check if whiteout file exists in upper
        crate::vfs::vfs_lstat(&wh_path).is_ok()
    }

    /// Create a whiteout for a file in the upper directory.
    fn create_whiteout(&self, rel_path: &str) -> FsResult<()> {
        let filename = rel_path.split('/').last().ok_or(FsError::InvalidArgument)?;
        let parent_path = if let Some(idx) = rel_path.rfind('/') {
            &rel_path[..idx]
        } else {
            ""
        };

        let wh_path = if parent_path.is_empty() {
            alloc::format!("{}/.wh.{}", self.upper_dir, filename)
        } else {
            // Ensure parent directory exists in upper
            self.ensure_upper_dir(parent_path)?;
            alloc::format!("{}/{}/.wh.{}", self.upper_dir, parent_path, filename)
        };

        // Create empty whiteout file
        let flags = crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::WRONLY;
        let fd = crate::vfs::vfs_open(&wh_path, flags, 0o644).map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd);
        Ok(())
    }

    /// Remove a whiteout file if it exists.
    fn remove_whiteout(&self, rel_path: &str) -> FsResult<()> {
        let filename = rel_path.split('/').last().ok_or(FsError::InvalidArgument)?;
        let parent_path = if let Some(idx) = rel_path.rfind('/') {
            &rel_path[..idx]
        } else {
            ""
        };

        let wh_path = if parent_path.is_empty() {
            alloc::format!("{}/.wh.{}", self.upper_dir, filename)
        } else {
            alloc::format!("{}/{}/.wh.{}", self.upper_dir, parent_path, filename)
        };

        if crate::vfs::vfs_lstat(&wh_path).is_ok() {
            let _ = crate::vfs::vfs_unlink(&wh_path);
        }
        Ok(())
    }

    /// Ensure that a directory exists in the upper layer (for copy-up).
    fn ensure_upper_dir(&self, rel_path: &str) -> FsResult<()> {
        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            return Ok(());
        }

        // Recurse to parent
        if let Some(idx) = rel_path.rfind('/') {
            self.ensure_upper_dir(&rel_path[..idx])?;
        }

        // Create directory in upper
        crate::vfs::vfs_mkdir(&upper_path, 0o755).map_err(|_| FsError::IoError)?;
        Ok(())
    }

    /// Copy a file from the lower layer to the upper layer.
    fn copy_up(&self, rel_path: &str) -> FsResult<()> {
        let lower_path = alloc::format!("{}/{}", self.lower_dir, rel_path);
        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);

        if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            return Ok(()); // Already copied up
        }

        // Ensure parent directory exists in upper
        if let Some(idx) = rel_path.rfind('/') {
            self.ensure_upper_dir(&rel_path[..idx])?;
        }

        // Read from lower
        let stat = crate::vfs::vfs_lstat(&lower_path).map_err(|_| FsError::NotFound)?;
        let mut content = Vec::with_capacity(stat.size as usize);
        content.resize(stat.size as usize, 0);

        let fd_in = crate::vfs::vfs_open(&lower_path, crate::vfs::OpenFlags::RDONLY, 0)
            .map_err(|_| FsError::IoError)?;
        let read_bytes = crate::vfs::vfs_read(fd_in, &mut content).map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd_in);

        // Write to upper
        let flags = crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::WRONLY;
        let fd_out =
            crate::vfs::vfs_open(&upper_path, flags, 0o644).map_err(|_| FsError::IoError)?;
        let _ =
            crate::vfs::vfs_write(fd_out, &content[..read_bytes]).map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd_out);

        if let Ok(stat) = crate::vfs::vfs_lstat(&upper_path) {
            crate::quota::account_create(&stat, &overlay_quota_mount(&upper_path))
                .map_err(map_quota_err)?;
        }

        Ok(())
    }

    fn translate_flags(&self, flags: OpenFlags) -> u32 {
        let mut bits = 0;
        if flags.read && flags.write {
            bits |= crate::vfs::OpenFlags::RDWR;
        } else if flags.write {
            bits |= crate::vfs::OpenFlags::WRONLY;
        } else {
            bits |= crate::vfs::OpenFlags::RDONLY;
        }
        if flags.create {
            bits |= crate::vfs::OpenFlags::CREAT;
        }
        if flags.truncate {
            bits |= crate::vfs::OpenFlags::TRUNC;
        }
        if flags.append {
            bits |= crate::vfs::OpenFlags::APPEND;
        }
        if flags.exclusive {
            bits |= crate::vfs::OpenFlags::EXCL;
        }
        bits
    }
}

impl FileSystem for OverlayFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::OverlayFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // Return stats of the upper directory if possible
        if let Ok(stat) = crate::vfs::vfs_statfs(&self.upper_dir) {
            Ok(FileSystemStats {
                total_blocks: stat.total_blocks,
                free_blocks: stat.free_blocks,
                available_blocks: stat.avail_blocks,
                total_inodes: stat.total_inodes,
                free_inodes: stat.free_inodes,
                block_size: stat.block_size as u32,
                max_filename_length: stat.max_name_len as u32,
            })
        } else {
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
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);

        // Remove whiteout if it existed
        self.remove_whiteout(rel_path)?;

        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);

        // Ensure parent directory exists in upper
        if let Some(idx) = rel_path.rfind('/') {
            self.ensure_upper_dir(&rel_path[..idx])?;
        }

        // Create in upper
        let flags = crate::vfs::OpenFlags::CREAT | crate::vfs::OpenFlags::WRONLY;
        let fd = crate::vfs::vfs_open(&upper_path, flags, permissions.to_octal() as u32)
            .map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd);

        if let Ok(stat) = crate::vfs::vfs_lstat(&upper_path) {
            crate::quota::account_create(&stat, &overlay_quota_mount(&upper_path))
                .map_err(map_quota_err)?;
        }

        let inode = self.alloc_inode(rel_path, false);
        Ok(inode)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);

        if self.is_whited_out(rel_path) {
            return Err(FsError::NotFound);
        }

        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, rel_path);

        let exists_in_upper = crate::vfs::vfs_lstat(&upper_path).is_ok();
        let exists_in_lower = crate::vfs::vfs_lstat(&lower_path).is_ok();

        if !exists_in_upper && !exists_in_lower {
            return Err(FsError::NotFound);
        }

        let is_dir = if exists_in_upper {
            let stat = crate::vfs::vfs_lstat(&upper_path).map_err(|_| FsError::IoError)?;
            stat.inode_type == crate::vfs::InodeType::Directory
        } else {
            let stat = crate::vfs::vfs_lstat(&lower_path).map_err(|_| FsError::IoError)?;
            stat.inode_type == crate::vfs::InodeType::Directory
        };

        // If opened for write and only exists in lower, copy up
        if (flags.write || flags.append || flags.truncate) && !exists_in_upper && !is_dir {
            self.copy_up(rel_path)?;
        }

        let inode = self.alloc_inode(rel_path, is_dir);
        Ok(inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        let upper_path = alloc::format!("{}/{}", self.upper_dir, node.rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, node.rel_path);

        let path = if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            upper_path
        } else {
            lower_path
        };

        let fd = crate::vfs::vfs_open(&path, crate::vfs::OpenFlags::RDONLY, 0)
            .map_err(|_| FsError::IoError)?;
        if offset > 0 {
            let _ = crate::vfs::vfs_seek(fd, crate::vfs::SeekFrom::Start(offset));
        }
        let bytes = crate::vfs::vfs_read(fd, buffer).map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd);

        Ok(bytes)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }

        // Ensure file is copied up
        self.copy_up(&node.rel_path)?;

        let upper_path = alloc::format!("{}/{}", self.upper_dir, node.rel_path);
        let old_stat = crate::vfs::vfs_lstat(&upper_path).map_err(|_| FsError::IoError)?;
        let mount = overlay_quota_mount(&upper_path);
        let new_size = core::cmp::max(old_stat.size, offset + buffer.len() as u64);
        crate::quota::account_write(&old_stat, &mount, old_stat.size, new_size)
            .map_err(map_quota_err)?;

        let fd = crate::vfs::vfs_open(&upper_path, crate::vfs::OpenFlags::WRONLY, 0)
            .map_err(|_| FsError::IoError)?;
        if offset > 0 {
            let _ = crate::vfs::vfs_seek(fd, crate::vfs::SeekFrom::Start(offset));
        }
        let bytes = crate::vfs::vfs_write(fd, buffer).map_err(|_| FsError::IoError)?;
        let _ = crate::vfs::vfs_close(fd);

        Ok(bytes)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        let upper_path = alloc::format!("{}/{}", self.upper_dir, node.rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, node.rel_path);

        let path = if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            upper_path
        } else {
            lower_path
        };

        let stat = crate::vfs::vfs_lstat(&path).map_err(|_| FsError::NotFound)?;
        let file_type = match stat.inode_type {
            crate::vfs::InodeType::Directory => FileType::Directory,
            crate::vfs::InodeType::Symlink => FileType::SymbolicLink,
            _ => FileType::Regular,
        };

        Ok(FileMetadata {
            inode,
            file_type,
            size: stat.size,
            permissions: FilePermissions::from_octal(stat.mode as u16),
            uid: stat.uid,
            gid: stat.gid,
            created: stat.ctime,
            modified: stat.mtime,
            accessed: stat.atime,
            link_count: stat.nlink,
            device_id: if stat.rdev != 0 {
                Some(stat.rdev as u32)
            } else {
                None
            },
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let node = self.get_node(inode)?;
        self.copy_up(&node.rel_path)?;

        let upper_path = alloc::format!("{}/{}", self.upper_dir, node.rel_path);
        // Set metadata on upper file (permissions, etc.) using VFS
        let mode = metadata.permissions.to_octal();
        crate::vfs::vfs_chmod(&upper_path, mode as u32).map_err(|_| FsError::IoError)?;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        self.remove_whiteout(rel_path)?;

        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        if let Some(idx) = rel_path.rfind('/') {
            self.ensure_upper_dir(&rel_path[..idx])?;
        }

        crate::vfs::vfs_mkdir(&upper_path, permissions.to_octal() as u32)
            .map_err(|_| FsError::IoError)?;
        if let Ok(stat) = crate::vfs::vfs_lstat(&upper_path) {
            crate::quota::account_create(&stat, &overlay_quota_mount(&upper_path))
                .map_err(map_quota_err)?;
        }
        let inode = self.alloc_inode(rel_path, true);
        Ok(inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, rel_path);

        let exists_in_upper = crate::vfs::vfs_lstat(&upper_path).is_ok();
        let exists_in_lower = crate::vfs::vfs_lstat(&lower_path).is_ok();

        if exists_in_upper {
            if let Ok(stat) = crate::vfs::vfs_lstat(&upper_path) {
                crate::quota::account_unlink(&stat, &overlay_quota_mount(&upper_path))
                    .map_err(map_quota_err)?;
            }
            crate::vfs::vfs_rmdir(&upper_path).map_err(|_| FsError::IoError)?;
        }
        if exists_in_lower {
            self.create_whiteout(rel_path)?;
        }

        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, rel_path);

        let exists_in_upper = crate::vfs::vfs_lstat(&upper_path).is_ok();
        let exists_in_lower = crate::vfs::vfs_lstat(&lower_path).is_ok();

        if exists_in_upper {
            if let Ok(stat) = crate::vfs::vfs_lstat(&upper_path) {
                crate::quota::account_unlink(&stat, &overlay_quota_mount(&upper_path))
                    .map_err(map_quota_err)?;
            }
            crate::vfs::vfs_unlink(&upper_path).map_err(|_| FsError::IoError)?;
        }
        if exists_in_lower {
            self.create_whiteout(rel_path)?;
        }

        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let node = self.get_node(inode)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }

        let upper_path = alloc::format!("{}/{}", self.upper_dir, node.rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, node.rel_path);

        let mut merged_entries = BTreeMap::new();

        // Read lower directory entries
        if crate::vfs::vfs_lstat(&lower_path).is_ok() {
            if let Ok(entries) = crate::vfs::vfs_readdir(&lower_path) {
                for entry in entries {
                    if entry.name.starts_with(".wh.") {
                        continue; // Skip whiteout markers
                    }
                    merged_entries.insert(entry.name.clone(), entry);
                }
            }
        }

        // Read upper directory entries (merging and applying whiteouts)
        if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            if let Ok(entries) = crate::vfs::vfs_readdir(&upper_path) {
                for entry in entries {
                    if entry.name.starts_with(".wh.") {
                        // This is a whiteout! Mask the lower entry.
                        let target_name = entry.name.strip_prefix(".wh.").unwrap_or("");
                        merged_entries.remove(target_name);
                        continue;
                    }
                    merged_entries.insert(entry.name.clone(), entry);
                }
            }
        }

        // Convert merged map to Vec<DirectoryEntry>
        let mut result = Vec::new();
        for (_, entry) in merged_entries {
            let file_type = match entry.inode_type {
                crate::vfs::InodeType::Directory => FileType::Directory,
                crate::vfs::InodeType::Symlink => FileType::SymbolicLink,
                _ => FileType::Regular,
            };
            let name = entry.name;
            let child_inode = self.alloc_inode(
                &alloc::format!("{}/{}", node.rel_path, name),
                file_type == FileType::Directory,
            );
            result.push(DirectoryEntry {
                name,
                inode: child_inode,
                file_type,
            });
        }

        Ok(result)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let rel_old = old_path.strip_prefix('/').unwrap_or(old_path);
        let rel_new = new_path.strip_prefix('/').unwrap_or(new_path);

        // Copy up the source if it is only in lower
        self.copy_up(rel_old)?;

        let upper_old = alloc::format!("{}/{}", self.upper_dir, rel_old);
        let upper_new = alloc::format!("{}/{}", self.upper_dir, rel_new);

        if let Some(idx) = rel_new.rfind('/') {
            self.ensure_upper_dir(&rel_new[..idx])?;
        }

        crate::vfs::vfs_rename(&upper_old, &upper_new).map_err(|_| FsError::IoError)?;

        // If the source was in lower, create a whiteout at the old location
        let lower_old = alloc::format!("{}/{}", self.lower_dir, rel_old);
        if crate::vfs::vfs_lstat(&lower_old).is_ok() {
            self.create_whiteout(rel_old)?;
        }

        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let rel_link = link_path.strip_prefix('/').unwrap_or(link_path);
        let upper_link = alloc::format!("{}/{}", self.upper_dir, rel_link);

        if let Some(idx) = rel_link.rfind('/') {
            self.ensure_upper_dir(&rel_link[..idx])?;
        }

        crate::vfs::vfs_symlink(target, &upper_link).map_err(|_| FsError::IoError)?;
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        let upper_path = alloc::format!("{}/{}", self.upper_dir, rel_path);
        let lower_path = alloc::format!("{}/{}", self.lower_dir, rel_path);

        let target_path = if crate::vfs::vfs_lstat(&upper_path).is_ok() {
            upper_path
        } else {
            lower_path
        };

        crate::vfs::vfs_readlink(&target_path).map_err(|_| FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
