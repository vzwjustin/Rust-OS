//! FUSE (Filesystem in Userspace) in-memory filesystem
//!
//! Real FUSE forwards VFS operations to a userspace daemon through `/dev/fuse`
//! and waits for the reply. Without a userspace daemon and a block-device
//! channel, this implementation keeps the in-kernel state that a FUSE
//! connection would track: the list of mounted sessions, per-session
//! connection state, and a pending-request queue. The VFS data path
//! (inodes/entries/file bytes) is served from memory so the kernel can still
//! expose a FUSE mount before a daemon is attached.
//!
//! A real implementation would, on each VFS op, enqueue a request on the
//! session's request queue, notify the daemon, and block on the reply; the
//! data path here is the cache the daemon would populate.

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

/// Maximum number of FUSE inodes per session.
const MAX_INODES: u64 = 32768;
/// Block size reported via statfs.
const BLOCK_SIZE: u32 = 4096;

/// State of a FUSE connection to a userspace daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseConnectionState {
    /// Session created but no daemon has attached yet.
    Initialized,
    /// Daemon has completed the FUSE_INIT handshake.
    Connected,
    /// Daemon has unmounted or crashed; ops fail with IoError.
    Disconnected,
}

/// A pending FUSE request awaiting a daemon reply.
#[derive(Debug, Clone)]
pub struct FuseRequest {
    /// Monotonic request id.
    pub id: u64,
    /// Opcode the kernel sent (FUSE_LOOKUP, FUSE_READ, etc.).
    pub opcode: u32,
    /// Inode the request targets.
    pub inode: InodeNumber,
    /// Payload bytes (e.g. write data or lookup name).
    pub payload: Vec<u8>,
}

/// A FUSE mount session tracking daemon connection state and pending requests.
#[derive(Debug)]
pub struct FuseSession {
    /// Unique session id.
    pub id: u64,
    /// Current connection state.
    pub state: FuseConnectionState,
    /// Pending requests awaiting daemon replies.
    pub pending_requests: Vec<FuseRequest>,
    /// Next request id to allocate.
    pub next_request_id: u64,
}

impl FuseSession {
    fn new(id: u64) -> Self {
        Self {
            id,
            state: FuseConnectionState::Initialized,
            pending_requests: Vec::new(),
            next_request_id: 1,
        }
    }

    /// Enqueue a request on this session and return its id.
    pub fn enqueue(&mut self, opcode: u32, inode: InodeNumber, payload: Vec<u8>) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.pending_requests.push(FuseRequest {
            id,
            opcode,
            inode,
            payload,
        });
        id
    }

    /// Pop a request by id once the daemon has replied.
    pub fn complete(&mut self, id: u64) -> Option<FuseRequest> {
        let idx = self.pending_requests.iter().position(|r| r.id == id)?;
        Some(self.pending_requests.swap_remove(idx))
    }
}

/// In-memory FUSE filesystem
#[derive(Debug)]
pub struct FuseFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, FuseInode>>,
    next_inode: RwLock<InodeNumber>,
    /// Active mount sessions keyed by session id.
    sessions: RwLock<BTreeMap<u64, FuseSession>>,
    /// Next session id to allocate.
    next_session_id: RwLock<u64>,
}

/// In-memory FUSE inode
#[derive(Debug, Clone)]
struct FuseInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    /// Cached file content for regular files; empty for directories.
    data: Vec<u8>,
    /// Directory entries mapping name -> child inode number.
    entries: BTreeMap<String, InodeNumber>,
    /// Creation time (Unix timestamp).
    created: u64,
    /// Last modification time.
    modified: u64,
    /// Last access time.
    accessed: u64,
    /// Owner user ID.
    uid: u32,
    /// Owner group ID.
    gid: u32,
}

impl FuseInode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: false,
            size: 0,
            permissions,
            data: Vec::new(),
            entries: BTreeMap::new(),
            created: now,
            modified: now,
            accessed: now,
            uid: 0,
            gid: 0,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            inode,
            is_dir: true,
            size: 0,
            permissions,
            data: Vec::new(),
            entries,
            created: now,
            modified: now,
            accessed: now,
            uid: 0,
            gid: 0,
        }
    }
}

impl FuseFileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        let mut root = FuseInode::new_directory(1, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), 1);
        inodes.insert(1, root);
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            sessions: RwLock::new(BTreeMap::new()),
            next_session_id: RwLock::new(1),
        })
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let n = *next;
        *next += 1;
        n
    }

    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(1);
        }
        let components = Self::split_path(path);
        let inodes = self.inodes.read();
        let mut current = 1u64;
        for component in components {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(&component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

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
            Ok((1, filename))
        } else {
            let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
            let parent_inode = self.resolve_path(&parent_path)?;
            Ok((parent_inode, filename))
        }
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<FuseInode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    fn is_directory_empty(inode: &FuseInode) -> bool {
        inode.entries.len() <= 2
    }

    // ---- Session management API ----

    /// Create a new FUSE mount session and return its id.
    pub fn create_session(&self) -> u64 {
        let mut next = self.next_session_id.write();
        let id = *next;
        *next += 1;
        let session = FuseSession::new(id);
        self.sessions.write().insert(id, session);
        id
    }

    /// Mark a session as connected after the daemon completes FUSE_INIT.
    pub fn connect_session(&self, id: u64) -> FsResult<()> {
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&id).ok_or(FsError::NotFound)?;
        session.state = FuseConnectionState::Connected;
        Ok(())
    }

    /// Mark a session as disconnected (daemon unmount/crash).
    pub fn disconnect_session(&self, id: u64) -> FsResult<()> {
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&id).ok_or(FsError::NotFound)?;
        session.state = FuseConnectionState::Disconnected;
        session.pending_requests.clear();
        Ok(())
    }

    /// Destroy a session, removing it from the session list.
    pub fn destroy_session(&self, id: u64) -> FsResult<()> {
        let mut sessions = self.sessions.write();
        sessions.remove(&id).map(|_| ()).ok_or(FsError::NotFound)
    }

    /// Enqueue a request on a session (kernel -> daemon direction).
    pub fn enqueue_request(
        &self,
        session_id: u64,
        opcode: u32,
        inode: InodeNumber,
        payload: Vec<u8>,
    ) -> FsResult<u64> {
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&session_id).ok_or(FsError::NotFound)?;
        if session.state == FuseConnectionState::Disconnected {
            return Err(FsError::IoError);
        }
        Ok(session.enqueue(opcode, inode, payload))
    }

    /// Complete a pending request once the daemon has replied.
    pub fn complete_request(&self, session_id: u64, request_id: u64) -> FsResult<FuseRequest> {
        let mut sessions = self.sessions.write();
        let session = sessions.get_mut(&session_id).ok_or(FsError::NotFound)?;
        session.complete(request_id).ok_or(FsError::NotFound)
    }

    /// Snapshot of the active session ids.
    pub fn session_ids(&self) -> Vec<u64> {
        self.sessions.read().keys().copied().collect()
    }

    /// Number of pending requests across all sessions.
    pub fn pending_request_count(&self) -> usize {
        self.sessions
            .read()
            .values()
            .map(|s| s.pending_requests.len())
            .sum()
    }
}

impl FileSystem for FuseFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // No dedicated Fuse variant in FileSystemType; report RamFs since the
        // in-memory data path behaves like a RAM-backed mount.
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let used_blocks: u64 = inodes
            .values()
            .map(|n| {
                let len = n.data.len() as u64;
                (len + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64
            })
            .sum();
        let free_blocks = MAX_INODES.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks: MAX_INODES,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_INODES,
            free_inodes: MAX_INODES.saturating_sub(used),
            block_size: BLOCK_SIZE,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let node = FuseInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, node);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        node.accessed = get_current_time();
        let len = node.data.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), node.data.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&node.data[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let required = new_size as usize;
        if node.data.len() < required {
            node.data.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        node.data[start..end].copy_from_slice(buffer);
        node.size = node.data.len() as u64;
        node.modified = get_current_time();
        node.accessed = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        Ok(FileMetadata {
            inode,
            file_type: if node.is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: node.size,
            permissions: node.permissions,
            uid: node.uid,
            gid: node.gid,
            created: node.created,
            modified: node.modified,
            accessed: node.accessed,
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.uid = metadata.uid;
        node.gid = metadata.gid;
        if !node.is_dir {
            node.size = metadata.size;
            if node.data.len() < metadata.size as usize {
                node.data.resize(metadata.size as usize, 0);
            } else if node.data.len() > metadata.size as usize {
                node.data.truncate(metadata.size as usize);
            }
        }
        node.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = FuseInode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, dir);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&dir_inode).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        if !Self::is_directory_empty(node) {
            return Err(FsError::DirectoryNotEmpty);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.modified = get_current_time();
        inodes.remove(&dir_inode);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.modified = get_current_time();
        inodes.remove(&file_inode);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if !dir.is_dir {
            return Err(FsError::NotADirectory);
        }
        dir.accessed = get_current_time();
        let snapshot: Vec<(String, InodeNumber)> =
            dir.entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut out = Vec::with_capacity(snapshot.len());
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                out.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: if child.is_dir {
                        FileType::Directory
                    } else {
                        FileType::Regular
                    },
                });
            }
        }
        Ok(out)
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
        if !new_parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }
        if new_parent_node.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent_node = inodes.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        if !old_parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }
        old_parent_node.entries.remove(&old_name);
        old_parent_node.modified = get_current_time();
        let new_parent_node = inodes.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_parent_node.entries.insert(new_name, old_inode);
        new_parent_node.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // Symlink support would require a daemon round-trip (FUSE_SYMLINK);
        // deferred until the daemon channel is wired up.
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory cache only. A real FUSE implementation would issue
        // FUSE_FSYNC requests to the daemon for dirty inodes.
        Ok(())
    }
}
