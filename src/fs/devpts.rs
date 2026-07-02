//! devpts pseudoterminal filesystem implementation
//!
//! This module implements the Linux devpts virtual filesystem, which
//! provides access to pseudo-terminal slave device files. PTY slaves are
//! registered dynamically via `add_pty` / `remove_pty`; each active PTY
//! appears as a character device node named by its numeric id (e.g. `/0`).
//!
//! Inode scheme: root directory = 1, PTY slave `id` = `2 + id`.
//! Device metadata uses major 136 (the Linux pty slave major) and
//! mode 0o620, matching `devpts` defaults.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use spin::RwLock;

/// Major device number for PTY slaves (matches Linux `UNIX98_PTY_SLAVE_MAJOR`).
const PTY_SLAVE_MAJOR: u32 = 136;

/// Root directory inode.
const ROOT_INODE: InodeNumber = 1;

/// Metadata for a registered PTY slave.
#[derive(Debug, Clone)]
pub struct PtyEntry {
    /// PTY identifier (the minor number).
    pub id: u32,
}

/// devpts filesystem instance.
#[derive(Debug)]
pub struct DevPtsFs {
    /// Registry of active PTY slaves keyed by id.
    ptys: RwLock<BTreeMap<u32, PtyEntry>>,
}

impl DevPtsFs {
    /// Create a new, empty devpts filesystem.
    pub fn new() -> Self {
        Self {
            ptys: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register a new PTY slave with the given id.
    ///
    /// Returns `AlreadyExists` if a PTY with that id is already registered.
    pub fn add_pty(&self, id: u32) -> FsResult<()> {
        let mut ptys = self.ptys.write();
        if ptys.contains_key(&id) {
            return Err(FsError::AlreadyExists);
        }
        ptys.insert(id, PtyEntry { id });
        Ok(())
    }

    /// Remove a previously registered PTY slave.
    pub fn remove_pty(&self, id: u32) -> FsResult<()> {
        let mut ptys = self.ptys.write();
        if ptys.remove(&id).is_none() {
            return Err(FsError::NotFound);
        }
        Ok(())
    }

    /// Convert a PTY id to its synthetic inode number.
    fn inode_for(id: u32) -> InodeNumber {
        2 + id as InodeNumber
    }

    /// Convert an inode number back to a PTY id, if valid.
    fn id_for_inode(inode: InodeNumber) -> Option<u32> {
        if inode < 2 {
            return None;
        }
        let id = (inode - 2) as u32;
        Some(id)
    }

    /// Build char-device metadata for a PTY slave.
    fn pty_metadata(id: u32) -> FileMetadata {
        FileMetadata {
            inode: Self::inode_for(id),
            file_type: FileType::CharacterDevice,
            size: 0,
            permissions: FilePermissions::from_octal(0o620),
            uid: 0,
            gid: 5, // tty group, matching Linux defaults
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: Some((PTY_SLAVE_MAJOR << 8) | id),
        }
    }

    /// Resolve a path like `/0` or `/` to an inode number.
    fn resolve(&self, path: &str) -> FsResult<InodeNumber> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Ok(ROOT_INODE);
        }
        if parts.len() != 1 {
            return Err(FsError::NotFound);
        }
        let id = parts[0].parse::<u32>().map_err(|_| FsError::NotFound)?;
        let ptys = self.ptys.read();
        if !ptys.contains_key(&id) {
            return Err(FsError::NotFound);
        }
        Ok(Self::inode_for(id))
    }

    /// Map a `LinuxError` from the pty layer to an `FsError`.
    fn map_pty_err(e: crate::linux_compat::LinuxError) -> FsError {
        use crate::linux_compat::LinuxError::*;
        match e {
            EAGAIN => FsError::IoError, // non-blocking; caller retries
            ENOTTY | ENOENT => FsError::NotFound,
            EIO => FsError::IoError,
            _ => FsError::IoError,
        }
    }
}

impl fmt::Display for DevPtsFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "devpts")
    }
}

impl FileSystem for DevPtsFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::DevPts
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let ptys = self.ptys.read();
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: ptys.len() as u64 + 1,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // devpts directory structure is managed via add_pty/remove_pty only.
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve(path)
    }

    fn read(&self, inode: InodeNumber, _offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let id = Self::id_for_inode(inode).ok_or(FsError::NotFound)?;
        // Confirm the PTY is registered.
        {
            let ptys = self.ptys.read();
            if !ptys.contains_key(&id) {
                return Err(FsError::NotFound);
            }
        }
        match crate::drivers::tty::pty::read_slave(id, buffer) {
            Ok(n) => {
                // EAGAIN surfaces as a 0-length read in some callers; treat
                // a positive count as bytes read, otherwise return 0.
                if n < 0 {
                    Ok(0)
                } else {
                    Ok(n as usize)
                }
            }
            Err(crate::linux_compat::LinuxError::EAGAIN) => Ok(0),
            Err(e) => Err(Self::map_pty_err(e)),
        }
    }

    fn write(&self, inode: InodeNumber, _offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let id = Self::id_for_inode(inode).ok_or(FsError::NotFound)?;
        {
            let ptys = self.ptys.read();
            if !ptys.contains_key(&id) {
                return Err(FsError::NotFound);
            }
        }
        match crate::drivers::tty::pty::write_slave(id, buffer) {
            Ok(n) => {
                if n < 0 {
                    Ok(0)
                } else {
                    Ok(n as usize)
                }
            }
            Err(e) => Err(Self::map_pty_err(e)),
        }
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == ROOT_INODE {
            return Ok(FileMetadata {
                inode: ROOT_INODE,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::default_directory(),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 2,
                device_id: None,
            });
        }
        let id = Self::id_for_inode(inode).ok_or(FsError::NotFound)?;
        let ptys = self.ptys.read();
        if !ptys.contains_key(&id) {
            return Err(FsError::NotFound);
        }
        Ok(Self::pty_metadata(id))
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // devpts does not support attribute changes via the VFS.
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
        if inode != ROOT_INODE {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        entries.push(DirectoryEntry {
            name: ".".to_string(),
            inode: ROOT_INODE,
            file_type: FileType::Directory,
        });
        entries.push(DirectoryEntry {
            name: "..".to_string(),
            inode: ROOT_INODE,
            file_type: FileType::Directory,
        });
        let ptys = self.ptys.read();
        for (&id, _entry) in ptys.iter() {
            entries.push(DirectoryEntry {
                name: format!("{}", id),
                inode: Self::inode_for(id),
                file_type: FileType::CharacterDevice,
            });
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
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
