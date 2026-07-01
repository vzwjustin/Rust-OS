//! SMB network filesystem implementation.
//!
//! In-memory VFS implementation of an SMB/CIFS client mount. Real SMB requires
//! network I/O (TCP transport, SMB2/3 negotiate, session setup, tree connect,
//! signing/encryption); this implementation tracks the client-side state
//! (session, tree, file handle table) and services VFS operations against an
//! in-memory cache so the kernel can mount and exercise an SMB filesystem
//! without a live server. The I/O layer would replace the in-memory content
//! store with READ/WRITE/CREATE/QUERY_DIRECTORY commands sent over the wire.

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

/// Maximum number of inodes the in-memory SMB mount will track.
const MAX_SMB_INODES: u64 = 4096;
/// Maximum single-file size supported by the in-memory cache.
const MAX_SMB_FILE_SIZE: u64 = 16 * 1024 * 1024;

/// SMB negotiate dialect state.
///
/// In a real client this is the result of an SMB2 NEGOTIATE exchange. Here we
/// record the negotiated dialect so callers can introspect the mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbDialect {
    /// Negotiation not yet performed.
    NotNegotiated,
    /// SMB 2.0.2 dialect.
    Smb200,
    /// SMB 2.1 dialect.
    Smb210,
    /// SMB 3.0 dialect.
    Smb300,
    /// SMB 3.0.2 dialect.
    Smb302,
    /// SMB 3.1.1 dialect.
    Smb311,
}

/// SMB session setup state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbSessionState {
    /// No session established.
    Anonymous,
    /// Session setup in progress.
    InProgress,
    /// Session established and valid.
    Active,
    /// Session expired / torn down.
    Expired,
}

/// SMB tree connect state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbTreeState {
    /// Not connected to a tree.
    Disconnected,
    /// TREE_CONNECT sent, awaiting reply.
    Connecting,
    /// Connected to a share tree.
    Connected,
    /// Tree disconnected.
    TornDown,
}

/// An open SMB file handle (equivalent to a server-issued FileId).
#[derive(Debug, Clone)]
struct SmbHandle {
    /// Inode number this handle refers to.
    inode: InodeNumber,
    /// Open flags the handle was created with.
    flags: OpenFlags,
    /// Whether the handle is still valid.
    persistent: bool,
}

/// In-memory SMB inode representing a file/directory on the remote share.
#[derive(Debug, Clone)]
struct SmbInode {
    /// VFS metadata.
    metadata: FileMetadata,
    /// Cached file content (regular files only).
    content: Vec<u8>,
    /// Directory entries (directories only).
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target (symlinks only).
    symlink_target: Option<String>,
}

impl SmbInode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: now,
                modified: now,
                accessed: now,
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: now,
                modified: now,
                accessed: now,
                link_count: 2,
                device_id: None,
            },
            content: Vec::new(),
            entries,
            symlink_target: None,
        }
    }

    fn new_symlink(inode: InodeNumber, target: &str, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: target.len() as u64,
                permissions,
                uid: 0,
                gid: 0,
                created: now,
                modified: now,
                accessed: now,
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// SMB client mount state.
///
/// Tracks the negotiated dialect, session state, tree connect state, the
/// server-issued file handle table, and the in-memory inode cache that stands
/// in for the remote share's contents.
#[derive(Debug)]
pub struct SmbFileSystem {
    /// Device/server identifier this mount is bound to.
    device_id: u32,
    /// Negotiated dialect.
    dialect: RwLock<SmbDialect>,
    /// Session setup state.
    session_state: RwLock<SmbSessionState>,
    /// Tree connect state.
    tree_state: RwLock<SmbTreeState>,
    /// Server share name (the tree we connected to).
    share_name: RwLock<String>,
    /// Open file handle table, keyed by a kernel-issued handle id.
    handles: RwLock<BTreeMap<u64, SmbHandle>>,
    /// Next handle id to allocate.
    next_handle: RwLock<u64>,
    /// In-memory inode cache keyed by inode number.
    inodes: RwLock<BTreeMap<InodeNumber, SmbInode>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode number.
    root_inode: InodeNumber,
}

impl SmbFileSystem {
    /// Create a new SMB filesystem instance.
    ///
    /// Performs the in-memory equivalent of NEGOTIATE + TREE_CONNECT: the
    /// dialect is set to SMB 3.1.1, the session is marked active, and a tree
    /// is connected to the default share "IPC$". A real I/O layer would issue
    /// these commands over a TCP transport before returning.
    pub fn new(device_id: u32) -> FsResult<Self> {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();
        let mut root = SmbInode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        inodes.insert(root_inode, root);

        Ok(Self {
            device_id,
            dialect: RwLock::new(SmbDialect::Smb311),
            session_state: RwLock::new(SmbSessionState::Active),
            tree_state: RwLock::new(SmbTreeState::Connected),
            share_name: RwLock::new("IPC$".to_string()),
            handles: RwLock::new(BTreeMap::new()),
            next_handle: RwLock::new(1),
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
        })
    }

    /// The device/server id this mount is bound to.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Currently negotiated SMB dialect.
    pub fn dialect(&self) -> SmbDialect {
        *self.dialect.read()
    }

    /// Current session setup state.
    pub fn session_state(&self) -> SmbSessionState {
        *self.session_state.read()
    }

    /// Current tree connect state.
    pub fn tree_state(&self) -> SmbTreeState {
        *self.tree_state.read()
    }

    /// The connected share (tree) name.
    pub fn share_name(&self) -> String {
        self.share_name.read().clone()
    }

    /// Allocate a new inode number.
    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let n = *next;
        *next += 1;
        n
    }

    /// Allocate a new file handle id and record it in the handle table.
    fn allocate_handle(&self, inode: InodeNumber, flags: OpenFlags) -> u64 {
        let mut next = self.next_handle.write();
        let id = *next;
        *next += 1;
        self.handles.write().insert(
            id,
            SmbHandle {
                inode,
                flags,
                persistent: true,
            },
        );
        id
    }

    /// Split a path into non-empty components.
    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Resolve a path to an inode number using the in-memory cache.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = Self::split_path(path);
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let inode = inodes.get(&current).ok_or(FsError::NotFound)?;
            if inode.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *inode.entries.get(&component).ok_or(FsError::NotFound)?;
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

    /// Whether a directory inode contains only "." and "..".
    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let dir = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.entries.len() <= 2)
    }
}

impl FileSystem for SmbFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Smb
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let block_size = 4096u32;
        let used_blocks = inodes
            .values()
            .map(|i| (i.content.len() as u64 + block_size as u64 - 1) / block_size as u64)
            .sum();
        let total_blocks = (MAX_SMB_FILE_SIZE * MAX_SMB_INODES) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_SMB_INODES,
            free_inodes: MAX_SMB_INODES.saturating_sub(used),
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_SMB_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let file = SmbInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, file);
        Ok(new_inode)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        let inode = self.resolve_path(path)?;
        // Record an open handle in the handle table. A real client would issue
        // SMB2 CREATE here and store the server's FileId; we store the inode.
        self.allocate_handle(inode, flags);
        Ok(inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        file.metadata.accessed = get_current_time();
        let len = file.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), file.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&file.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > MAX_SMB_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }
        let required = new_size as usize;
        if file.content.len() < required {
            file.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        file.content[start..end].copy_from_slice(buffer);
        file.metadata.size = file.content.len() as u64;
        file.metadata.modified = get_current_time();
        file.metadata.accessed = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.uid = metadata.uid;
        node.metadata.gid = metadata.gid;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_SMB_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = SmbInode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        inodes.insert(new_inode, dir);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        if !self.is_directory_empty(dir_inode)? {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        inodes.remove(&dir_inode);
        // Close any handles referring to the removed inode.
        let mut handles = self.handles.write();
        let dead: Vec<u64> = handles
            .iter()
            .filter(|(_, h)| h.inode == dir_inode)
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            handles.remove(&id);
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let file = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.metadata.modified = get_current_time();
        inodes.remove(&file_inode);
        let mut handles = self.handles.write();
        let dead: Vec<u64> = handles
            .iter()
            .filter(|(_, h)| h.inode == file_inode)
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            handles.remove(&id);
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        let entry_list: Vec<(String, InodeNumber)> = dir
            .entries
            .iter()
            .map(|(name, &ino)| (name.clone(), ino))
            .collect();
        let mut entries = Vec::new();
        for (name, ino) in entry_list {
            if let Some(node) = inodes.get(&ino) {
                entries.push(DirectoryEntry {
                    name,
                    inode: ino,
                    file_type: node.metadata.file_type,
                });
            }
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent, old_name) = self.resolve_parent(old_path)?;
        let (new_parent, new_name) = self.resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let new_parent_node = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
        if new_parent_node.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent_node = inodes.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        old_parent_node.entries.remove(&old_name);
        old_parent_node.metadata.modified = get_current_time();
        let new_parent_node = inodes.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_parent_node.entries.insert(new_name, old_inode);
        new_parent_node.metadata.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_SMB_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let link = SmbInode::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, link);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let link = inodes.get(&link_inode).ok_or(FsError::NotFound)?;
        if link.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        link.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory cache is always consistent. A real client would issue
        // SMB2 FLUSH for each persistent handle in the handle table here.
        Ok(())
    }
}
