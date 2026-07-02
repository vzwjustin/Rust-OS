//! Automounter filesystem (autofs).
//!
//! Provides a virtual filesystem that mounts other filesystems on demand.
//! In Linux, autofs acts as an interface between the kernel and a userspace
//! automount daemon: when a mount point is accessed and no filesystem is
//! yet mounted there, the kernel notifies the daemon, which performs the
//! real mount and signals completion. This implementation tracks a table
//! of registered mount points and delegates VFS operations to the mounted
//! filesystem when one is present. When no filesystem is mounted and the
//! autofs timeout has not expired, accesses block (here: return
//! `NotSupported`, since there is no userspace daemon in the kernel).
//!
//! See linux-master `fs/autofs/` for the reference implementation.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::RwLock;

/// Root inode number for the autofs filesystem.
const ROOT_INODE: InodeNumber = 1;

/// Default automount expiry timeout in milliseconds (5 minutes), matching
/// Linux's default `AUTOFS_DEFAULT_TIMEOUT`.
const DEFAULT_TIMEOUT_MS: u64 = 5 * 60 * 1000;

/// A registered mount point within the autofs filesystem.
#[derive(Debug)]
pub struct MountEntry {
    /// Canonical mount path (e.g. "/home").
    pub path: String,
    /// The filesystem mounted at this path, if any. `None` means the
    /// mount point is known but not currently mounted (expired or pending).
    pub target_fs: Option<Arc<dyn FileSystem>>,
    /// Mount options string (opaque, passed through from the daemon).
    pub mount_options: String,
    /// Expiry timeout in milliseconds. A mounted filesystem whose last
    /// access is older than this is considered expired and may be
    /// unmounted by the daemon.
    pub timeout: u64,
    /// Timestamp of the last access to this mount point.
    pub last_access: u64,
    /// Synthetic inode number for the mount-point directory.
    pub inode: InodeNumber,
}

impl MountEntry {
    /// Whether this mount entry has expired (no mounted fs or idle longer
    /// than the timeout).
    fn expired(&self) -> bool {
        if self.target_fs.is_none() {
            return true;
        }
        get_current_time().saturating_sub(self.last_access) > self.timeout
    }
}

/// AutoFS filesystem.
///
/// The root directory is virtual: `readdir` on it enumerates the
/// registered mount points. Operations on paths that correspond to a
/// mounted filesystem are delegated to that filesystem; operations on
/// unmounted paths return `NotSupported` (no mount daemon available).
#[derive(Debug)]
pub struct AutoFs {
    /// Registered mount points keyed by canonical path.
    mounts: RwLock<BTreeMap<String, MountEntry>>,
    /// Reverse lookup: inode -> path.
    inodes: RwLock<BTreeMap<InodeNumber, String>>,
    /// Root directory metadata.
    root_metadata: RwLock<FileMetadata>,
    next_inode: RwLock<InodeNumber>,
}

impl AutoFs {
    /// Create a new autofs filesystem with an empty mount table.
    pub fn new() -> FsResult<Self> {
        let root_md = FileMetadata::new(ROOT_INODE, FileType::Directory, 0);
        Ok(Self {
            mounts: RwLock::new(BTreeMap::new()),
            inodes: RwLock::new(BTreeMap::new()),
            root_metadata: RwLock::new(root_md),
            next_inode: RwLock::new(2),
        })
    }

    /// Allocate the next inode number.
    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    /// Normalize a path: ensure it starts with '/' and has no trailing
    /// slash (except for the root "/").
    fn normalize(path: &str) -> String {
        let mut p = path.to_string();
        if !p.starts_with('/') {
            p = format!("/{}", p);
        }
        while p.len() > 1 && p.ends_with('/') {
            p.pop();
        }
        p
    }

    /// Register a filesystem to be mounted at `path`.
    ///
    /// If a mount entry already exists at `path` its target filesystem is
    /// replaced; otherwise a new entry is created.
    pub fn register_mount(&self, path: &str, fs: Arc<dyn FileSystem>) -> FsResult<()> {
        let path = Self::normalize(path);
        if path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let mut mounts = self.mounts.write();
        let mut inodes = self.inodes.write();
        let now = get_current_time();
        if let Some(entry) = mounts.get_mut(&path) {
            entry.target_fs = Some(fs);
            entry.last_access = now;
            return Ok(());
        }
        let inode = self.allocate_inode();
        inodes.insert(inode, path.clone());
        mounts.insert(
            path.clone(),
            MountEntry {
                path: path.clone(),
                target_fs: Some(fs),
                mount_options: String::new(),
                timeout: DEFAULT_TIMEOUT_MS,
                last_access: now,
                inode,
            },
        );
        Ok(())
    }

    /// Remove a registered mount point, unmounting its filesystem.
    pub fn unregister_mount(&self, path: &str) -> FsResult<()> {
        let path = Self::normalize(path);
        let mut mounts = self.mounts.write();
        let removed = mounts.remove(&path).ok_or(FsError::NotFound)?;
        let mut inodes = self.inodes.write();
        inodes.remove(&removed.inode);
        Ok(())
    }

    /// Look up the mount entry for `path`, returning a cloned `Arc` to the
    /// mounted filesystem if one is present and not expired.
    fn resolve_mount(&self, path: &str) -> Option<Arc<dyn FileSystem>> {
        let path = Self::normalize(path);
        let mut mounts = self.mounts.write();
        let entry = mounts.get_mut(&path)?;
        if entry.expired() {
            // Expire: drop the target fs. The daemon would re-mount on
            // next access, but without a daemon we leave it unmounted.
            entry.target_fs = None;
            return None;
        }
        entry.last_access = get_current_time();
        entry.target_fs.clone()
    }

    /// Resolve `path` to a mounted filesystem and the path relative to
    /// that mount. The relative path is `path` with the mount-point prefix
    /// stripped. If `path` is exactly the mount point, the relative path
    /// is "/".
    fn resolve(&self, path: &str) -> Result<(Arc<dyn FileSystem>, String), FsError> {
        let norm = Self::normalize(path);
        // Try exact match first.
        if let Some(fs) = self.resolve_mount(&norm) {
            return Ok((fs, "/".to_string()));
        }
        // Try longest-prefix mount point. Collect the best match as an
        // owned String so the read borrow on `mounts` is released before
        // we call `resolve_mount` (which takes a write lock).
        let best_mp: Option<String> = {
            let mounts = self.mounts.read();
            let mut best: Option<String> = None;
            for (mp, _entry) in mounts.iter() {
                if norm == *mp {
                    continue; // handled above
                }
                if norm.starts_with(&format!("{}/", mp)) || (mp == "/" && norm != "/") {
                    match &best {
                        None => best = Some(mp.clone()),
                        Some(cur) if mp.len() > cur.len() => best = Some(mp.clone()),
                        _ => {}
                    }
                }
            }
            best
        };
        if let Some(mp) = best_mp {
            if let Some(fs) = self.resolve_mount(&mp) {
                let rel = if mp == "/" {
                    norm.clone()
                } else {
                    norm[mp.len()..].to_string()
                };
                let rel = if rel.is_empty() || !rel.starts_with('/') {
                    format!("/{}", rel)
                } else {
                    rel
                };
                return Ok((fs, rel));
            }
        }
        Err(FsError::NotFound)
    }
}

impl FileSystem for AutoFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::AutoFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let mounts = self.mounts.read();
        let count = mounts.len() as u64;
        // AutoFs is a virtual passthrough; report a small fixed pool.
        Ok(FileSystemStats {
            total_blocks: 1024,
            free_blocks: 1024,
            available_blocks: 1024,
            total_inodes: 1024,
            free_inodes: 1024u64.saturating_sub(count),
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        match self.resolve(path) {
            Ok((fs, rel)) => fs.create(&rel, permissions),
            Err(FsError::NotFound) => {
                // Not a known mount point and nothing mounted above it.
                // Without a mount daemon we cannot create real files here.
                Err(FsError::NotSupported)
            }
            Err(e) => Err(e),
        }
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        let norm = Self::normalize(path);
        if norm == "/" {
            return Ok(ROOT_INODE);
        }
        // If this path is a registered mount point but not currently
        // mounted, we would normally trigger the automount daemon. With no
        // daemon available, return NotSupported so callers know the mount
        // is pending.
        {
            let mounts = self.mounts.read();
            if let Some(entry) = mounts.get(&norm) {
                if entry.target_fs.is_none() {
                    return Err(FsError::NotSupported);
                }
            }
        }
        match self.resolve(path) {
            Ok((fs, rel)) => fs.open(&rel, flags),
            Err(FsError::NotFound) => Err(FsError::NotFound),
            Err(e) => Err(e),
        }
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }
        // Map the inode back to a path, then delegate.
        let path = {
            let inodes = self.inodes.read();
            inodes.get(&inode).cloned().ok_or(FsError::NotFound)?
        };
        let (fs, rel) = self.resolve(&path)?;
        // Use the mounted fs's own inode scheme by opening the relative path.
        let child_inode = fs.open(&rel, OpenFlags::read_only())?;
        fs.read(child_inode, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }
        let path = {
            let inodes = self.inodes.read();
            inodes.get(&inode).cloned().ok_or(FsError::NotFound)?
        };
        let (fs, rel) = self.resolve(&path)?;
        let child_inode = fs.open(&rel, OpenFlags::read_write())?;
        fs.write(child_inode, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == ROOT_INODE {
            return Ok(self.root_metadata.read().clone());
        }
        let path = {
            let inodes = self.inodes.read();
            inodes.get(&inode).cloned().ok_or(FsError::NotFound)?
        };
        let (fs, rel) = self.resolve(&path)?;
        let child_inode = fs.open(&rel, OpenFlags::read_only())?;
        fs.metadata(child_inode)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // AutoFs does not propagate metadata changes to mounted filesystems.
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        match self.resolve(path) {
            Ok((fs, rel)) => fs.mkdir(&rel, permissions),
            Err(FsError::NotFound) => Err(FsError::NotSupported),
            Err(e) => Err(e),
        }
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        match self.resolve(path) {
            Ok((fs, rel)) => fs.rmdir(&rel),
            Err(FsError::NotFound) => Err(FsError::NotSupported),
            Err(e) => Err(e),
        }
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        match self.resolve(path) {
            Ok((fs, rel)) => fs.unlink(&rel),
            Err(FsError::NotFound) => Err(FsError::NotSupported),
            Err(e) => Err(e),
        }
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        if inode == ROOT_INODE {
            // Enumerate registered mount points as directory entries.
            let mounts = self.mounts.read();
            let mut entries = Vec::new();
            for entry in mounts.values() {
                let name = entry
                    .path
                    .trim_start_matches('/')
                    .to_string();
                entries.push(DirectoryEntry {
                    name,
                    inode: entry.inode,
                    file_type: FileType::Directory,
                });
            }
            return Ok(entries);
        }
        // Delegate to the mounted filesystem.
        let path = {
            let inodes = self.inodes.read();
            inodes.get(&inode).cloned().ok_or(FsError::NotFound)?
        };
        let (fs, rel) = self.resolve(&path)?;
        let child_inode = fs.open(&rel, OpenFlags::read_only())?;
        fs.readdir(child_inode)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        // Rename across mount points is not supported.
        let (fs, rel_old) = self.resolve(old_path)?;
        let (_, rel_new) = self.resolve(new_path).or_else(|_| {
            // If new_path isn't mounted, try resolving against the same fs
            // by stripping the old mount prefix and using new_path relative.
            Err(FsError::NotSupported)
        })?;
        fs.rename(&rel_old, &rel_new)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // Sync all mounted filesystems.
        let mounts = self.mounts.read();
        for entry in mounts.values() {
            if let Some(fs) = &entry.target_fs {
                fs.sync()?;
            }
        }
        Ok(())
    }
}
