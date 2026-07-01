//! DevPTS pseudo-filesystem for pseudo-terminal slaves
//!
//! In-memory VFS implementation of the Linux devpts filesystem. devpts is a
//! pseudo-filesystem that presents pseudo-terminal (pty) device nodes under
//! /dev/pts. This implementation allocates pty indices, tracks the active pty
//! entries, and services VFS operations against the in-memory entry table. A
//! real implementation would wire `read`/`write` to the pty driver; here those
//! operate on an in-memory buffer so the filesystem is exercisable without a
//! TTY subsystem.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::cmp;
use spin::RwLock;

/// Maximum number of pty entries devpts will hand out.
const DEVPTS_MAX_ENTRIES: u64 = 4096;
/// Maximum buffer size for an in-memory pty slot.
const DEVPTS_MAX_BUFFER: u64 = 64 * 1024;

/// A single devpts pty entry (one /dev/pts/N node).
#[derive(Debug, Clone)]
struct PtyEntry {
    /// VFS metadata.
    metadata: FileMetadata,
    /// The pty index (the N in /dev/pts/N).
    index: u32,
    /// In-memory ring buffer standing in for the pty driver.
    buffer: Vec<u8>,
}

impl PtyEntry {
    fn new(inode: InodeNumber, index: u32) -> Self {
        let now = get_current_time();
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::CharacterDevice,
                size: 0,
                permissions: FilePermissions::from_octal(0o620),
                uid: 0,
                gid: 5, // tty group
                created: now,
                modified: now,
                accessed: now,
                link_count: 1,
                device_id: Some((5 << 8) | (index as u32 & 0xff)),
            },
            index,
            buffer: Vec::new(),
        }
    }
}

/// DevPTS filesystem instance.
#[derive(Debug)]
pub struct DevPtsFs {
    /// Root directory inode number.
    root_inode: InodeNumber,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// All inodes: the root directory plus one entry per allocated pty.
    inodes: RwLock<BTreeMap<InodeNumber, DevPtsNode>>,
    /// Allocated pty indices -> inode number, used for index allocation.
    pty_indices: RwLock<BTreeMap<u32, InodeNumber>>,
    /// Next candidate pty index to allocate.
    next_pty_index: RwLock<u32>,
}

/// A devpts inode: either the root directory or a pty device node.
#[derive(Debug, Clone)]
enum DevPtsNode {
    /// The root /dev/pts directory.
    Directory {
        /// Metadata.
        metadata: FileMetadata,
        /// Child entries: name -> inode number.
        entries: BTreeMap<String, InodeNumber>,
    },
    /// A /dev/pts/N pty device node.
    Pty(PtyEntry),
}

impl DevPtsFs {
    /// Create a new devpts filesystem with an empty root directory.
    pub fn new() -> Self {
        let root_inode = 1;
        let now = get_current_time();
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), root_inode);
        entries.insert("..".to_string(), root_inode);
        let root = DevPtsNode::Directory {
            metadata: FileMetadata {
                inode: root_inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::from_octal(0o755),
                uid: 0,
                gid: 0,
                created: now,
                modified: now,
                accessed: now,
                link_count: 2,
                device_id: None,
            },
            entries,
        };
        let mut inodes = BTreeMap::new();
        inodes.insert(root_inode, root);

        Self {
            root_inode,
            next_inode: RwLock::new(2),
            inodes: RwLock::new(inodes),
            pty_indices: RwLock::new(BTreeMap::new()),
            next_pty_index: RwLock::new(0),
        }
    }

    /// Allocate a fresh pty index, creating the /dev/pts/N node and returning
    /// its inode number. This is the devpts equivalent of `devpts_pty_new`.
    pub fn alloc_pty(&self) -> FsResult<InodeNumber> {
        let mut inodes = self.inodes.write();
        if inodes.len() >= DEVPTS_MAX_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let mut indices = self.pty_indices.write();
        let mut next_idx = self.next_pty_index.write();
        // Find the lowest free index.
        let mut idx = *next_idx;
        loop {
            if !indices.contains_key(&idx) {
                break;
            }
            idx = idx.checked_add(1).ok_or(FsError::NoSpaceLeft)?;
        }
        let inode = {
            let mut next = self.next_inode.write();
            let n = *next;
            *next += 1;
            n
        };
        let pty = PtyEntry::new(inode, idx);
        let name = format!("{}", idx);
        // Insert into the root directory.
        match inodes.get_mut(&self.root_inode) {
            Some(DevPtsNode::Directory { metadata, entries }) => {
                entries.insert(name, inode);
                metadata.modified = get_current_time();
                metadata.link_count += 1;
            }
            _ => return Err(FsError::IoError),
        }
        inodes.insert(inode, DevPtsNode::Pty(pty));
        indices.insert(idx, inode);
        *next_idx = idx + 1;
        Ok(inode)
    }

    /// Release a pty index, removing its /dev/pts/N node.
    pub fn release_pty(&self, inode: InodeNumber) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let mut indices = self.pty_indices.write();
        let node = inodes.remove(&inode).ok_or(FsError::NotFound)?;
        if let DevPtsNode::Pty(pty) = &node {
            let name = format!("{}", pty.index);
            indices.remove(&pty.index);
            if let Some(DevPtsNode::Directory { metadata, entries }) =
                inodes.get_mut(&self.root_inode)
            {
                entries.remove(&name);
                metadata.modified = get_current_time();
                metadata.link_count = metadata.link_count.saturating_sub(1);
            }
        }
        Ok(())
    }

    /// Allocate the next inode number.
    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let n = *next;
        *next += 1;
        n
    }

    /// Split a path into non-empty components.
    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Resolve a path to an inode number.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = Self::split_path(path);
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            match node {
                DevPtsNode::Directory { entries, .. } => {
                    current = *entries.get(&component).ok_or(FsError::NotFound)?;
                }
                DevPtsNode::Pty(_) => return Err(FsError::NotADirectory),
            }
        }
        Ok(current)
    }

    /// Resolve the parent directory inode and final path component.
    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        if path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let components = Self::split_path(path);
        if components.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let filename = components.last().unwrap().clone();
        if components.len() == 1 {
            Ok((self.root_inode, filename))
        } else {
            let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
            let parent_inode = self.resolve_path(&parent_path)?;
            Ok((parent_inode, filename))
        }
    }
}

impl Default for DevPtsFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for DevPtsFs {
    fn fs_type(&self) -> FileSystemType {
        // devpts is a pseudo-device filesystem; the kernel's FileSystemType
        // enum does not yet carry a dedicated DevPts variant, so we report
        // DevFs which is the closest existing semantic match.
        FileSystemType::DevFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: DEVPTS_MAX_ENTRIES,
            free_inodes: DEVPTS_MAX_ENTRIES.saturating_sub(used),
            block_size: 1024,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // devpts does not allow arbitrary file creation; only pty nodes are
        // created via alloc_pty. Reject explicit creates.
        let _ = self.resolve_parent(path)?;
        Err(FsError::PermissionDenied)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        let pty = match node {
            DevPtsNode::Pty(p) => p,
            DevPtsNode::Directory { .. } => return Err(FsError::IsADirectory),
        };
        pty.metadata.accessed = get_current_time();
        let len = pty.buffer.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), pty.buffer.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&pty.buffer[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        let pty = match node {
            DevPtsNode::Pty(p) => p,
            DevPtsNode::Directory { .. } => return Err(FsError::IsADirectory),
        };
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > DEVPTS_MAX_BUFFER {
            return Err(FsError::NoSpaceLeft);
        }
        if pty.buffer.len() < new_size as usize {
            pty.buffer.resize(new_size as usize, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        pty.buffer[start..end].copy_from_slice(buffer);
        pty.metadata.size = pty.buffer.len() as u64;
        pty.metadata.modified = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(match node {
            DevPtsNode::Directory { metadata, .. } => metadata.clone(),
            DevPtsNode::Pty(p) => p.metadata.clone(),
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        match node {
            DevPtsNode::Directory { metadata: m, .. } => {
                m.permissions = metadata.permissions;
                m.uid = metadata.uid;
                m.gid = metadata.gid;
                m.modified = get_current_time();
            }
            DevPtsNode::Pty(p) => {
                p.metadata.permissions = metadata.permissions;
                p.metadata.uid = metadata.uid;
                p.metadata.gid = metadata.gid;
                p.metadata.modified = get_current_time();
            }
        }
        Ok(())
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // devpts only contains pty nodes; subdirectory creation is not allowed.
        Err(FsError::PermissionDenied)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::PermissionDenied)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        // Allow unlinking a pty node by path, which releases the pty index.
        let inode = self.resolve_path(path)?;
        self.release_pty(inode)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        let entries = match node {
            DevPtsNode::Directory { metadata, entries } => {
                metadata.accessed = get_current_time();
                entries
            }
            DevPtsNode::Pty(_) => return Err(FsError::NotADirectory),
        };
        let entry_list: Vec<(String, InodeNumber)> =
            entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut result = Vec::new();
        for (name, ino) in entry_list {
            if let Some(child) = inodes.get(&ino) {
                let ft = match child {
                    DevPtsNode::Directory { metadata, .. } => metadata.file_type,
                    DevPtsNode::Pty(p) => p.metadata.file_type,
                };
                result.push(DirectoryEntry {
                    name,
                    inode: ino,
                    file_type: ft,
                });
            }
        }
        Ok(result)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // devpts node names are fixed to their pty index; renames are refused.
        Err(FsError::PermissionDenied)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::PermissionDenied)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
