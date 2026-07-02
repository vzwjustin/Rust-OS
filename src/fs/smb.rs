//! SMB (Server Message Block) network filesystem implementation.
//!
//! This module implements an SMB filesystem client that operates against an
//! in-memory backing store. The data structures and protocol logic mirror the
//! SMB2/SMB3 protocol flow (negotiate, session setup, tree connect, create,
//! read, write, close) so that a real network transport can be plugged in by
//! replacing the [`SmbTransport`] trait implementation.
//!
//! ## Architecture
//!
//! - [`SmbTransport`] abstracts the network I/O layer. The default
//!   [`MemoryTransport`] stores all share data in memory, allowing the
//!   filesystem to be exercised in tests and during early boot.
//! - [`SmbFileSystem`] holds connection state (server address, share name,
//!   credentials) and an open-file table indexed by file ID (persistent IDs
//!   in SMB3 parlance).
//! - A path→inode (file ID) map is maintained under a `spin::RwLock` using
//!   `BTreeMap` for deterministic iteration.
//!
//! All multi-byte protocol fields are little-endian, matching SMB2 wire format.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use spin::RwLock;

// ============================================================================
// Constants
// ============================================================================

/// SMB2 magic: 0xFE followed by "SMB".
#[allow(dead_code)]
const SMB2_MAGIC: [u8; 4] = [0xFE, b'S', b'M', b'B'];

/// Maximum number of symlink hops during path resolution.
const MAX_SYMLINK_DEPTH: usize = 8;

/// Maximum file name length supported by this implementation.
const SMB_NAME_MAX: usize = 255;

/// Root directory inode number (file ID 1 is reserved for the root).
const SMB_ROOT_INODE: InodeNumber = 1;

/// First dynamically-allocated inode number.
const SMB_FIRST_INODE: InodeNumber = 2;

// ============================================================================
// Transport trait
// ============================================================================

/// Abstract network transport for SMB protocol exchanges.
///
/// In a real deployment this would wrap a TCP connection to an SMB server.
/// The default in-memory implementation ([`MemoryTransport`]) stores all
/// share data in a `BTreeMap` keyed by path, enabling full read/write
/// filesystem semantics without network I/O.
pub trait SmbTransport: Send + Sync {
    /// Send a CREATE request and receive a response containing the file ID
    /// and file metadata. Returns `NotFound` if the path does not exist.
    fn create_request(
        &self,
        share: &str,
        path: &str,
        disposition: CreateDisposition,
    ) -> FsResult<TransportFileInfo>;

    /// Send a READ request for `file_id` at `offset`, filling `buffer`.
    /// Returns the number of bytes actually read.
    fn read_request(
        &self,
        share: &str,
        file_id: InodeNumber,
        offset: u64,
        buffer: &mut [u8],
    ) -> FsResult<usize>;

    /// Send a WRITE request for `file_id` at `offset` with `data`.
    /// Returns the number of bytes actually written.
    fn write_request(
        &self,
        share: &str,
        file_id: InodeNumber,
        offset: u64,
        data: &[u8],
    ) -> FsResult<usize>;

    /// Send a CLOSE request for `file_id`.
    fn close_request(&self, share: &str, file_id: InodeNumber) -> FsResult<()>;

    /// Send a QUERY_INFO request for `file_id`, returning file metadata.
    fn query_info(&self, share: &str, file_id: InodeNumber) -> FsResult<TransportFileInfo>;

    /// Send a SET_INFO request to update file metadata.
    fn set_info(&self, share: &str, file_id: InodeNumber, info: &TransportFileInfo) -> FsResult<()>;

    /// Send a QUERY_DIRECTORY request, returning the list of entries in the
    /// directory identified by `file_id`.
    fn query_directory(
        &self,
        share: &str,
        file_id: InodeNumber,
    ) -> FsResult<Vec<TransportDirEntry>>;

    /// Send a DELETE request (file or directory). For directories, the
    /// server validates emptiness.
    fn delete_request(&self, share: &str, file_id: InodeNumber, is_dir: bool) -> FsResult<()>;

    /// Send a RENAME request.
    fn rename_request(&self, share: &str, file_id: InodeNumber, new_path: &str) -> FsResult<()>;

    /// Send a SET_SYMLINK request to create a symbolic link.
    fn symlink_request(&self, share: &str, link_path: &str, target: &str) -> FsResult<()>;

    /// Send a GET_SYMLINK request to read the target of a symlink.
    fn readlink_request(&self, share: &str, file_id: InodeNumber) -> FsResult<String>;

    /// Create a directory at `path` within `share`. The file at `path` must
    /// not already exist. Returns the file ID of the new directory.
    fn mkdir_request(
        &self,
        share: &str,
        path: &str,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber>;

    /// Query filesystem statistics (size info).
    fn query_fs_info(&self, share: &str) -> FsResult<TransportFsInfo>;

    /// Flush all pending writes for this share.
    fn flush(&self, share: &str) -> FsResult<()>;
}

/// Create disposition values (SMB2_CREATE_DISPOSITION).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateDisposition {
    /// Open only if the file exists; fail otherwise.
    Open,
    /// Create only if the file does not exist; fail if it exists.
    Create,
    /// Open if exists, create if not.
    #[allow(dead_code)]
    OpenIf,
    /// Overwrite if exists, create if not.
    #[allow(dead_code)]
    OverwriteIf,
    /// Overwrite only if exists; fail otherwise.
    Overwrite,
}

/// File type as reported by the transport layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFileType {
    File,
    Directory,
    Symlink,
}

impl TransportFileType {
    fn to_vfs(self) -> FileType {
        match self {
            TransportFileType::File => FileType::Regular,
            TransportFileType::Directory => FileType::Directory,
            TransportFileType::Symlink => FileType::SymbolicLink,
        }
    }
}

/// File information returned by the transport layer.
#[derive(Debug, Clone)]
pub struct TransportFileInfo {
    pub file_id: InodeNumber,
    pub file_type: TransportFileType,
    pub size: u64,
    pub attributes: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub permissions: FilePermissions,
}

/// Directory entry returned by the transport layer.
#[derive(Debug, Clone)]
pub struct TransportDirEntry {
    pub name: String,
    pub file_id: InodeNumber,
    pub file_type: TransportFileType,
}

/// Filesystem statistics from the transport layer.
#[derive(Debug, Clone, Copy)]
pub struct TransportFsInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
}

// ============================================================================
// In-memory transport implementation
// ============================================================================

/// A single in-memory file node.
#[derive(Debug, Clone)]
struct MemNode {
    file_type: TransportFileType,
    data: Vec<u8>,
    children: BTreeMap<String, InodeNumber>,
    attributes: u32,
    created: u64,
    modified: u64,
    accessed: u64,
    permissions: FilePermissions,
    #[allow(dead_code)]
    link_count: u32,
}

impl MemNode {
    fn new_dir(permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            file_type: TransportFileType::Directory,
            data: Vec::new(),
            children: BTreeMap::new(),
            attributes: 0x10,
            created: now,
            modified: now,
            accessed: now,
            permissions,
            link_count: 2,
        }
    }

    fn new_file(permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            file_type: TransportFileType::File,
            data: Vec::new(),
            children: BTreeMap::new(),
            attributes: 0x80,
            created: now,
            modified: now,
            accessed: now,
            permissions,
            link_count: 1,
        }
    }

    fn new_symlink() -> Self {
        let now = get_current_time();
        Self {
            file_type: TransportFileType::Symlink,
            data: Vec::new(),
            children: BTreeMap::new(),
            attributes: 0x400,
            created: now,
            modified: now,
            accessed: now,
            permissions: FilePermissions::from_octal(0o777),
            link_count: 1,
        }
    }

    fn size(&self) -> u64 {
        if self.file_type == TransportFileType::Directory {
            self.children.len() as u64 * 32
        } else {
            self.data.len() as u64
        }
    }

    fn to_info(&self, file_id: InodeNumber) -> TransportFileInfo {
        TransportFileInfo {
            file_id,
            file_type: self.file_type,
            size: self.size(),
            attributes: self.attributes,
            created: self.created,
            modified: self.modified,
            accessed: self.accessed,
            permissions: self.permissions,
        }
    }
}

/// In-memory SMB transport. All share data is stored under a `RwLock`.
pub struct MemoryTransport {
    shares: RwLock<BTreeMap<String, MemoryShare>>,
}

#[derive(Debug)]
struct MemoryShare {
    nodes: BTreeMap<InodeNumber, MemNode>,
    root_id: InodeNumber,
    next_id: InodeNumber,
    total_bytes: u64,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self {
            shares: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn add_share(&self, share: &str, total_bytes: u64) {
        let mut shares = self.shares.write();
        let mut nodes = BTreeMap::new();
        let root = MemNode::new_dir(FilePermissions::default_directory());
        nodes.insert(SMB_ROOT_INODE, root);
        shares.insert(
            share.to_string(),
            MemoryShare {
                nodes,
                root_id: SMB_ROOT_INODE,
                next_id: SMB_FIRST_INODE,
                total_bytes,
            },
        );
    }

    fn resolve_path(&self, share: &str, path: &str) -> FsResult<InodeNumber> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(sh.root_id);
        }
        let mut current = sh.root_id;
        for component in path.split('/').filter(|c| !c.is_empty()) {
            let node = sh.nodes.get(&current).ok_or(FsError::NotFound)?;
            if node.file_type != TransportFileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *node.children.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    fn resolve_parent(&self, share: &str, path: &str) -> FsResult<(InodeNumber, String)> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        if let Some(idx) = path.rfind('/') {
            let parent = path[..idx].trim_start_matches('/');
            let name = &path[idx + 1..];
            let parent_id = if parent.is_empty() {
                let shares = self.shares.read();
                shares.get(share).ok_or(FsError::NotFound)?.root_id
            } else {
                self.resolve_path(share, parent)?
            };
            Ok((parent_id, name.to_string()))
        } else {
            let shares = self.shares.read();
            let sh = shares.get(share).ok_or(FsError::NotFound)?;
            Ok((sh.root_id, path.to_string()))
        }
    }

    fn alloc_id(&self, share: &str) -> FsResult<InodeNumber> {
        let mut shares = self.shares.write();
        let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
        let id = sh.next_id;
        sh.next_id += 1;
        Ok(id)
    }

    fn id_to_path(&self, share: &str, file_id: InodeNumber) -> FsResult<String> {
        if file_id == SMB_ROOT_INODE {
            return Ok(String::from("/"));
        }
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let mut stack: Vec<(InodeNumber, String)> = vec![(sh.root_id, String::from(""))];
        while let Some((id, path)) = stack.pop() {
            let node = sh.nodes.get(&id).ok_or(FsError::NotFound)?;
            if id == file_id {
                return Ok(if path.is_empty() {
                    String::from("/")
                } else {
                    path
                });
            }
            if node.file_type == TransportFileType::Directory {
                for (name, &child_id) in &node.children {
                    let child_path = if path.is_empty() {
                        format!("/{}", name)
                    } else {
                        format!("{}/{}", path, name)
                    };
                    stack.push((child_id, child_path));
                }
            }
        }
        Err(FsError::NotFound)
    }

    fn used_bytes(&self, share: &str) -> u64 {
        let shares = self.shares.read();
        if let Some(sh) = shares.get(share) {
            sh.nodes.values().map(|n| n.size()).sum()
        } else {
            0
        }
    }
}

impl Default for MemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SmbTransport for MemoryTransport {
    fn create_request(
        &self,
        share: &str,
        path: &str,
        disposition: CreateDisposition,
    ) -> FsResult<TransportFileInfo> {
        let (parent_id, name) = self.resolve_parent(share, path)?;
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidArgument);
        }
        if name.len() > SMB_NAME_MAX {
            return Err(FsError::NameTooLong);
        }

        let exists = {
            let shares = self.shares.read();
            let sh = shares.get(share).ok_or(FsError::NotFound)?;
            let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
            parent.children.contains_key(&name)
        };

        match disposition {
            CreateDisposition::Open => {
                if !exists {
                    return Err(FsError::NotFound);
                }
            }
            CreateDisposition::Create => {
                if exists {
                    return Err(FsError::AlreadyExists);
                }
            }
            CreateDisposition::Overwrite | CreateDisposition::OverwriteIf => {
                if exists {
                    let mut shares = self.shares.write();
                    let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
                    let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
                    let &child_id = parent.children.get(&name).ok_or(FsError::NotFound)?;
                    let child = sh.nodes.get_mut(&child_id).ok_or(FsError::NotFound)?;
                    if child.file_type == TransportFileType::Directory {
                        return Err(FsError::IsADirectory);
                    }
                    child.data.clear();
                    child.modified = get_current_time();
                    return Ok(child.to_info(child_id));
                }
                if disposition == CreateDisposition::Overwrite {
                    return Err(FsError::NotFound);
                }
            }
            _ => {}
        }

        if exists {
            let shares = self.shares.read();
            let sh = shares.get(share).ok_or(FsError::NotFound)?;
            let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
            let &child_id = parent.children.get(&name).ok_or(FsError::NotFound)?;
            let child = sh.nodes.get(&child_id).ok_or(FsError::NotFound)?;
            return Ok(child.to_info(child_id));
        }

        let new_id = self.alloc_id(share)?;
        let now = get_current_time();
        let node = MemNode::new_file(FilePermissions::default_file());
        {
            let mut shares = self.shares.write();
            let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
            sh.nodes.insert(new_id, node);
            let parent = sh.nodes.get_mut(&parent_id).ok_or(FsError::NotFound)?;
            parent.children.insert(name.clone(), new_id);
            parent.modified = now;
        }
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get(&new_id).ok_or(FsError::NotFound)?;
        Ok(node.to_info(new_id))
    }

    fn read_request(
        &self,
        share: &str,
        file_id: InodeNumber,
        offset: u64,
        buffer: &mut [u8],
    ) -> FsResult<usize> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get(&file_id).ok_or(FsError::NotFound)?;
        if node.file_type == TransportFileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let data = &node.data;
        if offset >= data.len() as u64 {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), data.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&data[start..end]);
        Ok(n)
    }

    fn write_request(
        &self,
        share: &str,
        file_id: InodeNumber,
        offset: u64,
        data: &[u8],
    ) -> FsResult<usize> {
        let used = self.used_bytes(share);
        let total = {
            let shares = self.shares.read();
            shares.get(share).map(|sh| sh.total_bytes).unwrap_or(0)
        };
        let mut shares = self.shares.write();
        let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get_mut(&file_id).ok_or(FsError::NotFound)?;
        if node.file_type == TransportFileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let required_len = (offset + data.len() as u64) as usize;
        if required_len > node.data.len() {
            let growth = (required_len - node.data.len()) as u64;
            if used + growth > total {
                return Err(FsError::NoSpaceLeft);
            }
            node.data.resize(required_len, 0);
        }
        let start = offset as usize;
        let end = start + data.len();
        node.data[start..end].copy_from_slice(data);
        node.modified = get_current_time();
        Ok(data.len())
    }

    fn close_request(&self, _share: &str, _file_id: InodeNumber) -> FsResult<()> {
        Ok(())
    }

    fn query_info(&self, share: &str, file_id: InodeNumber) -> FsResult<TransportFileInfo> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get(&file_id).ok_or(FsError::NotFound)?;
        Ok(node.to_info(file_id))
    }

    fn set_info(&self, share: &str, file_id: InodeNumber, info: &TransportFileInfo) -> FsResult<()> {
        let mut shares = self.shares.write();
        let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get_mut(&file_id).ok_or(FsError::NotFound)?;
        node.attributes = info.attributes;
        node.permissions = info.permissions;
        node.created = info.created;
        node.modified = info.modified;
        node.accessed = info.accessed;
        if node.file_type == TransportFileType::File {
            if info.size < node.data.len() as u64 {
                node.data.truncate(info.size as usize);
            } else if info.size > node.data.len() as u64 {
                node.data.resize(info.size as usize, 0);
            }
        }
        Ok(())
    }

    fn query_directory(&self, share: &str, file_id: InodeNumber) -> FsResult<Vec<TransportDirEntry>> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get(&file_id).ok_or(FsError::NotFound)?;
        if node.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        for (name, &child_id) in &node.children {
            let child = sh.nodes.get(&child_id).ok_or(FsError::IoError)?;
            out.push(TransportDirEntry {
                name: name.clone(),
                file_id: child_id,
                file_type: child.file_type,
            });
        }
        Ok(out)
    }

    fn delete_request(&self, share: &str, file_id: InodeNumber, is_dir: bool) -> FsResult<()> {
        let path = self.id_to_path(share, file_id)?;
        let (parent_id, name) = self.resolve_parent(share, &path)?;
        let mut shares = self.shares.write();
        let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
        let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
        let &child_id = parent.children.get(&name).ok_or(FsError::NotFound)?;
        let child = sh.nodes.get(&child_id).ok_or(FsError::NotFound)?;
        if is_dir {
            if child.file_type != TransportFileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if !child.children.is_empty() {
                return Err(FsError::DirectoryNotEmpty);
            }
        } else {
            if child.file_type == TransportFileType::Directory {
                return Err(FsError::IsADirectory);
            }
        }
        let parent = sh.nodes.get_mut(&parent_id).ok_or(FsError::NotFound)?;
        parent.children.remove(&name);
        parent.modified = get_current_time();
        sh.nodes.remove(&child_id);
        Ok(())
    }

    fn rename_request(&self, share: &str, file_id: InodeNumber, new_path: &str) -> FsResult<()> {
        let old_path = self.id_to_path(share, file_id)?;
        let (old_parent_id, old_name) = self.resolve_parent(share, &old_path)?;
        let (new_parent_id, new_name) = self.resolve_parent(share, new_path)?;
        if new_name.is_empty() || new_name == "." || new_name == ".." {
            return Err(FsError::InvalidArgument);
        }
        let mut shares = self.shares.write();
        let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
        let new_parent = sh.nodes.get(&new_parent_id).ok_or(FsError::NotFound)?;
        if new_parent.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if let Some(&existing_id) = new_parent.children.get(&new_name) {
            let existing = sh.nodes.get(&existing_id).ok_or(FsError::IoError)?;
            if existing.file_type == TransportFileType::Directory && !existing.children.is_empty() {
                return Err(FsError::DirectoryNotEmpty);
            }
            sh.nodes.remove(&existing_id);
        }
        let old_parent = sh.nodes.get_mut(&old_parent_id).ok_or(FsError::NotFound)?;
        let child_id = *old_parent.children.get(&old_name).ok_or(FsError::NotFound)?;
        old_parent.children.remove(&old_name);
        let new_parent = sh.nodes.get_mut(&new_parent_id).ok_or(FsError::NotFound)?;
        new_parent.children.insert(new_name.clone(), child_id);
        new_parent.modified = get_current_time();
        Ok(())
    }

    fn symlink_request(&self, share: &str, link_path: &str, target: &str) -> FsResult<()> {
        let (parent_id, name) = self.resolve_parent(share, link_path)?;
        if name.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        {
            let shares = self.shares.read();
            let sh = shares.get(share).ok_or(FsError::NotFound)?;
            let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
            if parent.children.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        let new_id = self.alloc_id(share)?;
        let mut node = MemNode::new_symlink();
        node.data = target.as_bytes().to_vec();
        {
            let mut shares = self.shares.write();
            let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
            sh.nodes.insert(new_id, node);
            let parent = sh.nodes.get_mut(&parent_id).ok_or(FsError::NotFound)?;
            parent.children.insert(name, new_id);
            parent.modified = get_current_time();
        }
        Ok(())
    }

    fn readlink_request(&self, share: &str, file_id: InodeNumber) -> FsResult<String> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let node = sh.nodes.get(&file_id).ok_or(FsError::NotFound)?;
        if node.file_type != TransportFileType::Symlink {
            return Err(FsError::InvalidArgument);
        }
        core::str::from_utf8(&node.data)
            .map(|s| s.to_string())
            .map_err(|_| FsError::IoError)
    }

    fn mkdir_request(
        &self,
        share: &str,
        path: &str,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let (parent_id, name) = self.resolve_parent(share, path)?;
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidArgument);
        }
        {
            let shares = self.shares.read();
            let sh = shares.get(share).ok_or(FsError::NotFound)?;
            let parent = sh.nodes.get(&parent_id).ok_or(FsError::NotFound)?;
            if parent.children.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        let new_id = self.alloc_id(share)?;
        let node = MemNode::new_dir(permissions);
        {
            let mut shares = self.shares.write();
            let sh = shares.get_mut(share).ok_or(FsError::NotFound)?;
            sh.nodes.insert(new_id, node);
            let parent = sh.nodes.get_mut(&parent_id).ok_or(FsError::NotFound)?;
            parent.children.insert(name, new_id);
            parent.modified = get_current_time();
        }
        Ok(new_id)
    }

    fn query_fs_info(&self, share: &str) -> FsResult<TransportFsInfo> {
        let shares = self.shares.read();
        let sh = shares.get(share).ok_or(FsError::NotFound)?;
        let used: u64 = sh.nodes.values().map(|n| n.size()).sum();
        let total = sh.total_bytes;
        let inode_count = sh.nodes.len() as u64;
        Ok(TransportFsInfo {
            total_bytes: total,
            free_bytes: total.saturating_sub(used),
            total_inodes: 1_000_000,
            free_inodes: 1_000_000_u64.saturating_sub(inode_count),
        })
    }

    fn flush(&self, _share: &str) -> FsResult<()> {
        Ok(())
    }
}

// ============================================================================
// Credentials
// ============================================================================

/// SMB authentication credentials.
#[derive(Debug, Clone)]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
    pub domain: String,
}

impl SmbCredentials {
    pub fn new(username: &str, password: &str, domain: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
            domain: domain.to_string(),
        }
    }

    pub fn guest() -> Self {
        Self {
            username: String::from("guest"),
            password: String::new(),
            domain: String::new(),
        }
    }
}

// ============================================================================
// Open file table entry
// ============================================================================

#[derive(Debug, Clone)]
struct OpenFileEntry {
    file_id: InodeNumber,
    path: String,
    flags: OpenFlags,
    #[allow(dead_code)]
    position: u64,
}

// ============================================================================
// SMB Filesystem
// ============================================================================

pub struct SmbFileSystem {
    device_id: u32,
    server: String,
    share: String,
    #[allow(dead_code)]
    credentials: SmbCredentials,
    transport: Arc<dyn SmbTransport>,
    open_files: RwLock<BTreeMap<InodeNumber, OpenFileEntry>>,
    path_cache: RwLock<BTreeMap<String, InodeNumber>>,
    next_handle: RwLock<InodeNumber>,
}

impl core::fmt::Debug for SmbFileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("SmbFileSystem")
            .field("device_id", &self.device_id)
            .field("server", &self.server)
            .field("share", &self.share)
            .finish()
    }
}

impl SmbFileSystem {
    pub fn new_with_transport(
        device_id: u32,
        server: &str,
        share: &str,
        credentials: SmbCredentials,
        transport: Arc<dyn SmbTransport>,
    ) -> FsResult<Self> {
        let _info = transport.create_request(share, "/", CreateDisposition::Open)?;
        Ok(Self {
            device_id,
            server: server.to_string(),
            share: share.to_string(),
            credentials,
            transport,
            open_files: RwLock::new(BTreeMap::new()),
            path_cache: RwLock::new(BTreeMap::new()),
            next_handle: RwLock::new(1_000_000),
        })
    }

    pub fn new(
        device_id: u32,
        server: &str,
        share: &str,
        credentials: SmbCredentials,
    ) -> FsResult<Self> {
        let transport = Arc::new(MemoryTransport::new());
        transport.add_share(share, 1024 * 1024 * 1024);
        Self::new_with_transport(device_id, server, share, credentials, transport)
    }

    fn alloc_handle(&self) -> InodeNumber {
        let mut nh = self.next_handle.write();
        let id = *nh;
        *nh += 1;
        id
    }

    fn resolve(&self, path: &str) -> FsResult<InodeNumber> {
        {
            let cache = self.path_cache.read();
            if let Some(&id) = cache.get(path) {
                return Ok(id);
            }
        }
        let info = self
            .transport
            .create_request(&self.share, path, CreateDisposition::Open)?;
        self.path_cache.write().insert(path.to_string(), info.file_id);
        Ok(info.file_id)
    }

    fn resolve_follow(&self, path: &str, depth: usize) -> FsResult<InodeNumber> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::TooManySymlinks);
        }
        let file_id = self.resolve(path)?;
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type == TransportFileType::Symlink {
            let target = self.transport.readlink_request(&self.share, file_id)?;
            let resolved_target = if target.starts_with('/') {
                target
            } else {
                let parent = parent_path(path);
                format!("{}/{}", parent, target)
            };
            return self.resolve_follow(&resolved_target, depth + 1);
        }
        Ok(file_id)
    }

    fn split_path(path: &str) -> (String, String) {
        let path = path.trim_start_matches('/');
        if let Some(idx) = path.rfind('/') {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            let parent = parent.trim_start_matches('/');
            let parent = if parent.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", parent)
            };
            (parent, name.to_string())
        } else {
            ("/".to_string(), path.to_string())
        }
    }

    fn invalidate(&self, path: &str) {
        self.path_cache.write().remove(path);
    }
}

fn parent_path(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if let Some(idx) = path.rfind('/') {
        let parent = &path[..idx];
        if parent.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parent)
        }
    } else {
        "/".to_string()
    }
}

impl FileSystem for SmbFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Smb
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let info = self.transport.query_fs_info(&self.share)?;
        let block_size = 4096u32;
        Ok(FileSystemStats {
            total_blocks: info.total_bytes / block_size as u64,
            free_blocks: info.free_bytes / block_size as u64,
            available_blocks: info.free_bytes / block_size as u64,
            total_inodes: info.total_inodes,
            free_inodes: info.free_inodes,
            block_size,
            max_filename_length: SMB_NAME_MAX as u32,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidArgument);
        }
        if name.len() > SMB_NAME_MAX {
            return Err(FsError::NameTooLong);
        }
        let parent_id = self.resolve_follow(&parent_path, 0)?;
        let parent_info = self.transport.query_info(&self.share, parent_id)?;
        if parent_info.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let info = self
            .transport
            .create_request(&self.share, path, CreateDisposition::Create)?;
        let mut new_info = info.clone();
        new_info.permissions = permissions;
        let _ = self.transport.set_info(&self.share, info.file_id, &new_info);
        self.invalidate(path);
        self.invalidate(&parent_path);
        Ok(info.file_id)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        if flags.create {
            match self.resolve_follow(path, 0) {
                Ok(file_id) => {
                    if flags.exclusive {
                        return Err(FsError::AlreadyExists);
                    }
                    if flags.truncate {
                        let info = self.transport.query_info(&self.share, file_id)?;
                        if info.file_type == TransportFileType::Directory {
                            return Err(FsError::IsADirectory);
                        }
                        let mut truncated = info.clone();
                        truncated.size = 0;
                        self.transport.set_info(&self.share, file_id, &truncated)?;
                    }
                    let handle = self.alloc_handle();
                    self.open_files.write().insert(
                        handle,
                        OpenFileEntry {
                            file_id,
                            path: path.to_string(),
                            flags,
                            position: 0,
                        },
                    );
                    return Ok(handle);
                }
                Err(FsError::NotFound) => {
                    let file_id = self.create(path, FilePermissions::default_file())?;
                    let handle = self.alloc_handle();
                    self.open_files.write().insert(
                        handle,
                        OpenFileEntry {
                            file_id,
                            path: path.to_string(),
                            flags,
                            position: 0,
                        },
                    );
                    return Ok(handle);
                }
                Err(e) => return Err(e),
            }
        }

        let file_id = self.resolve_follow(path, 0)?;

        if flags.truncate {
            let info = self.transport.query_info(&self.share, file_id)?;
            if info.file_type == TransportFileType::Directory {
                return Err(FsError::IsADirectory);
            }
            let mut truncated = info.clone();
            truncated.size = 0;
            self.transport.set_info(&self.share, file_id, &truncated)?;
        }

        let handle = self.alloc_handle();
        self.open_files.write().insert(
            handle,
            OpenFileEntry {
                file_id,
                path: path.to_string(),
                flags,
                position: 0,
            },
        );
        Ok(handle)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let file_id = {
            let files = self.open_files.read();
            files.get(&inode).map(|e| e.file_id)
        };
        let file_id = match file_id {
            Some(id) => id,
            None => inode,
        };
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type == TransportFileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let n = self
            .transport
            .read_request(&self.share, file_id, offset, buffer)?;
        let mut updated = info.clone();
        updated.accessed = get_current_time();
        let _ = self.transport.set_info(&self.share, file_id, &updated);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let file_id = {
            let files = self.open_files.read();
            files.get(&inode).map(|e| e.file_id)
        };
        let file_id = match file_id {
            Some(id) => id,
            None => inode,
        };
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type == TransportFileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let write_offset = {
            let files = self.open_files.read();
            if let Some(entry) = files.get(&inode) {
                if entry.flags.append {
                    info.size
                } else {
                    offset
                }
            } else {
                offset
            }
        };
        let n = self
            .transport
            .write_request(&self.share, file_id, write_offset, buffer)?;
        Ok(n)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let file_id = {
            let files = self.open_files.read();
            files.get(&inode).map(|e| e.file_id).unwrap_or(inode)
        };
        let info = self.transport.query_info(&self.share, file_id)?;
        Ok(FileMetadata {
            inode: file_id,
            file_type: info.file_type.to_vfs(),
            size: info.size,
            permissions: info.permissions,
            uid: 0,
            gid: 0,
            created: info.created,
            modified: info.modified,
            accessed: info.accessed,
            link_count: 1,
            device_id: Some(self.device_id),
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let file_id = {
            let files = self.open_files.read();
            files.get(&inode).map(|e| e.file_id).unwrap_or(inode)
        };
        let info = self.transport.query_info(&self.share, file_id)?;
        let mut updated = info.clone();
        updated.permissions = metadata.permissions;
        updated.created = metadata.created;
        updated.modified = metadata.modified;
        updated.accessed = metadata.accessed;
        updated.size = metadata.size;
        self.transport.set_info(&self.share, file_id, &updated)
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidArgument);
        }
        if name.len() > SMB_NAME_MAX {
            return Err(FsError::NameTooLong);
        }
        let parent_id = self.resolve_follow(&parent_path, 0)?;
        let parent_info = self.transport.query_info(&self.share, parent_id)?;
        if parent_info.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let file_id = self
            .transport
            .mkdir_request(&self.share, path, permissions)?;
        self.invalidate(path);
        self.invalidate(&parent_path);
        Ok(file_id)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let file_id = self.resolve_follow(path, 0)?;
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let entries = self.transport.query_directory(&self.share, file_id)?;
        if !entries.is_empty() {
            return Err(FsError::DirectoryNotEmpty);
        }
        self.transport.delete_request(&self.share, file_id, true)?;
        self.invalidate(path);
        let (parent, _) = Self::split_path(path);
        self.invalidate(&parent);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_id = self.resolve_follow(path, 0)?;
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type == TransportFileType::Directory {
            return Err(FsError::IsADirectory);
        }
        self.transport.delete_request(&self.share, file_id, false)?;
        self.invalidate(path);
        let (parent, _) = Self::split_path(path);
        self.invalidate(&parent);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let file_id = {
            let files = self.open_files.read();
            files.get(&inode).map(|e| e.file_id).unwrap_or(inode)
        };
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let entries = self.transport.query_directory(&self.share, file_id)?;
        Ok(entries
            .into_iter()
            .map(|e| DirectoryEntry {
                name: e.name,
                inode: e.file_id,
                file_type: e.file_type.to_vfs(),
            })
            .collect())
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let file_id = self.resolve_follow(old_path, 0)?;
        self.transport.rename_request(&self.share, file_id, new_path)?;
        self.invalidate(old_path);
        self.invalidate(new_path);
        let (old_parent, _) = Self::split_path(old_path);
        let (new_parent, _) = Self::split_path(new_path);
        self.invalidate(&old_parent);
        self.invalidate(&new_parent);
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(link_path);
        if name.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let parent_id = self.resolve_follow(&parent_path, 0)?;
        let parent_info = self.transport.query_info(&self.share, parent_id)?;
        if parent_info.file_type != TransportFileType::Directory {
            return Err(FsError::NotADirectory);
        }
        self.transport.symlink_request(&self.share, link_path, target)?;
        self.invalidate(link_path);
        self.invalidate(&parent_path);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let file_id = self.resolve(path)?;
        let info = self.transport.query_info(&self.share, file_id)?;
        if info.file_type != TransportFileType::Symlink {
            return Err(FsError::InvalidArgument);
        }
        self.transport.readlink_request(&self.share, file_id)
    }

    fn sync(&self) -> FsResult<()> {
        self.transport.flush(&self.share)
    }
}
