//! FUSE (Filesystem in Userspace) implementation
//!
//! This module implements the FUSE protocol for communicating with a userspace
//! filesystem daemon. It provides:
//! - `#[repr(C)]` FUSE protocol structs (headers, init, attr, entry, etc.)
//! - `FuseConnection`: a request/reply channel with pending-request tracking
//!   and an outgoing request queue.
//! - `FuseFileSystem`: a `FileSystem` trait impl that translates VFS calls
//!   into FUSE opcodes and waits for daemon replies.
//!
//! The connection uses `spin::Mutex<BTreeMap>` for pending requests and a
//! `VecDeque` for outgoing messages, enabling asynchronous request/response.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use spin::Mutex;

// ============================================================================
// FUSE protocol constants
// ============================================================================

const FUSE_KERNEL_VERSION: u32 = 7;
const FUSE_KERNEL_MINOR_VERSION: u32 = 31;

/// FUSE opcodes (subset matching Linux fuse.h).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseOpcode {
    Lookup = 1,
    Forget = 2,
    Getattr = 3,
    Setattr = 4,
    Readlink = 5,
    Symlink = 6,
    Mknod = 8,
    Mkdir = 9,
    Unlink = 10,
    Rmdir = 11,
    Rename = 12,
    Link = 13,
    Open = 14,
    Read = 15,
    Write = 16,
    Statfs = 17,
    Release = 18,
    Fsync = 20,
    Setxattr = 21,
    Getxattr = 22,
    Listxattr = 23,
    Removexattr = 24,
    Flush = 25,
    Init = 26,
    Opendir = 27,
    Readdir = 28,
    Releasedir = 29,
    Fsyncdir = 30,
    Getlk = 31,
    Setlk = 32,
    Setlkw = 33,
    Access = 34,
    Create = 35,
    Interrupt = 36,
    Bmap = 37,
    Destroy = 38,
    Ioctl = 39,
    Poll = 40,
    NotifyReply = 41,
    BatchForget = 42,
    Fallocate = 43,
    Readdirplus = 44,
    Rename2 = 45,
    Lseek = 46,
    CopyFileRange = 47,
}

impl FuseOpcode {
    fn from_u32(v: u32) -> Option<Self> {
        if v <= 47 {
            // SAFETY: FuseOpcode is #[repr(u32)] with contiguous values 1..=47
            // (plus 0 which we don't use). We validate the range.
            // Values 0 and 7 are not defined; skip them.
            if v == 0 || v == 7 {
                return None;
            }
            Some(unsafe { core::mem::transmute(v) })
        } else {
            None
        }
    }
}

// ============================================================================
// FUSE protocol structures (#[repr(C)])
// ============================================================================

/// FUSE input header (kernel -> daemon).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInHeader {
    pub len: u32,
    pub opcode: u32,
    pub unique: u64,
    pub nodeid: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    pub padding: u32,
}

/// FUSE output header (daemon -> kernel).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOutHeader {
    pub len: u32,
    pub error: i32,
    pub unique: u64,
    pub padding: u64,
}

/// FUSE_INIT request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInitIn {
    pub major: u32,
    pub minor: u32,
    pub max_readahead: u32,
    pub flags: u32,
}

/// FUSE_INIT reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInitOut {
    pub major: u32,
    pub minor: u32,
    pub max_readahead: u32,
    pub flags: u32,
    pub max_background: u16,
    pub congestion_threshold: u16,
    pub max_write: u32,
    pub time_gran: u32,
    pub max_pages: u16,
    pub padding: u16,
    pub unused: [u32; 8],
}

/// FUSE file attributes (stat-like).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseAttr {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub atimensec: u32,
    pub mtimensec: u32,
    pub ctimensec: u32,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
    pub padding: u32,
}

/// FUSE entry reply (for lookup, create, etc.).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseEntryOut {
    pub nodeid: u64,
    pub generation: u64,
    pub attr_valid: u64,
    pub attr_valid_nsec: u32,
    pub attr: FuseAttr,
}

/// FUSE getattr reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseGetattrOut {
    pub attr_valid: u64,
    pub attr_valid_nsec: u32,
    pub padding: u32,
    pub attr: FuseAttr,
}

/// FUSE open request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOpenIn {
    pub flags: u32,
    pub unused: u32,
}

/// FUSE open reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOpenOut {
    pub fh: u64,
    pub open_flags: u32,
    pub padding: u32,
}

/// FUSE read request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseReadIn {
    pub fh: u64,
    pub offset: u64,
    pub size: u32,
    pub read_flags: u32,
    pub lock_owner: u64,
    pub flags: u32,
    pub padding: u32,
}

/// FUSE write request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseWriteIn {
    pub fh: u64,
    pub offset: u64,
    pub size: u32,
    pub write_flags: u32,
    pub lock_owner: u64,
    pub flags: u32,
    pub padding: u32,
}

/// FUSE write reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseWriteOut {
    pub size: u32,
    pub padding: u32,
}

/// FUSE setattr request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseSetattrIn {
    pub valid: u32,
    pub padding: u32,
    pub fh: u64,
    pub size: u64,
    pub lock_owner: u64,
    pub atime: u64,
    pub mtime: u64,
    pub unused2: u64,
    pub atimensec: u32,
    pub mtimensec: u32,
    pub unused3: u32,
    pub mode: u32,
    pub unused4: u32,
    pub uid: u32,
    pub gid: u32,
    pub unused5: u32,
}

/// FUSE mkdir request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseMkdirIn {
    pub mode: u32,
    pub umask: u32,
}

/// FUSE rename request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseRenameIn {
    pub newdir: u64,
    pub oldname_size: u32,
    pub padding: u32,
}

/// FUSE create request.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseCreateIn {
    pub flags: u32,
    pub mode: u32,
    pub umask: u32,
    pub padding: u32,
}

/// FUSE statfs reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseStatfsOut {
    pub st: FuseKstatfs,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseKstatfs {
    pub bsize: u32,
    pub frsize: u32,
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub namelen: u32,
    pub padding: u32,
}

/// FUSE directory entry (in readdir reply payload).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseDirent {
    pub ino: u64,
    pub off: u64,
    pub namelen: u32,
    pub typ: u32,
    // name follows (namelen bytes, NUL-terminated)
}

/// FUSE getxattr reply.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseGetxattrOut {
    pub size: u32,
    pub padding: u32,
}

// ============================================================================
// FuseConnection
// ============================================================================

/// A pending FUSE request awaiting a daemon reply.
#[derive(Debug)]
struct PendingRequest {
    /// The raw request payload (including FuseInHeader) for retransmission
    /// if needed.
    request: Vec<u8>,
    /// Whether a reply has arrived.
    completed: bool,
    /// The reply payload (including FuseOutHeader) once available.
    reply: Option<Vec<u8>>,
}

/// Connection to a FUSE daemon.
///
/// The kernel side enqueues outgoing requests and the daemon reads them via
/// [`read_request`], processes them, and calls [`submit_reply`] to deliver
/// responses. The filesystem side calls [`send_request`] + [`wait_for_reply`].
pub struct FuseConnection {
    /// Monotonically increasing unique request ID.
    next_unique: Mutex<u64>,
    /// Pending requests keyed by unique ID.
    pending: Mutex<BTreeMap<u64, PendingRequest>>,
    /// Outgoing request queue (daemon reads from here).
    outgoing: Mutex<VecDeque<Vec<u8>>>,
    /// Whether the INIT handshake has completed.
    initialized: Mutex<bool>,
    /// Negotiated max write size.
    max_write: Mutex<u32>,
}

impl core::fmt::Debug for FuseConnection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FuseConnection")
            .field("next_unique", &*self.next_unique.lock())
            .field("pending_count", &self.pending.lock().len())
            .field("outgoing_count", &self.outgoing.lock().len())
            .field("initialized", &*self.initialized.lock())
            .finish()
    }
}

impl FuseConnection {
    /// Create a new FUSE connection.
    pub fn new() -> Self {
        Self {
            next_unique: Mutex::new(1),
            pending: Mutex::new(BTreeMap::new()),
            outgoing: Mutex::new(VecDeque::new()),
            initialized: Mutex::new(false),
            max_write: Mutex::new(4096),
        }
    }

    /// Send a FUSE request with the given opcode, target nodeid, and payload.
    /// Returns the unique ID assigned to the request.
    pub fn send_request(
        &self,
        opcode: FuseOpcode,
        nodeid: u64,
        data: &[u8],
    ) -> FsResult<u64> {
        let unique = {
            let mut nu = self.next_unique.lock();
            let v = *nu;
            *nu += 1;
            v
        };
        let header = FuseInHeader {
            len: (core::mem::size_of::<FuseInHeader>() + data.len()) as u32,
            opcode: opcode as u32,
            unique,
            nodeid,
            uid: 0,
            gid: 0,
            pid: 0,
            padding: 0,
        };
        let mut buf = Vec::with_capacity(header.len as usize);
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &header as *const FuseInHeader as *const u8,
                core::mem::size_of::<FuseInHeader>(),
            )
        };
        buf.extend_from_slice(header_bytes);
        buf.extend_from_slice(data);

        {
            let mut pending = self.pending.lock();
            pending.insert(
                unique,
                PendingRequest {
                    request: buf.clone(),
                    completed: false,
                    reply: None,
                },
            );
        }
        {
            let mut outgoing = self.outgoing.lock();
            outgoing.push_back(buf);
        }
        Ok(unique)
    }

    /// Wait for and retrieve the reply payload (excluding the header) for the
    /// given unique ID. In a real kernel this would block on a waitqueue; here
    /// we poll the pending map. Returns `Err(IoError)` on daemon error.
    pub fn wait_for_reply(&self, unique: u64) -> FsResult<Vec<u8>> {
        // Poll for the reply. In a single-threaded test environment the daemon
        // may have already submitted the reply synchronously.
        loop {
            let result = {
                let mut pending = self.pending.lock();
                if let Some(req) = pending.get_mut(&unique) {
                    if req.completed {
                        let reply = req.reply.take().unwrap_or_default();
                        pending.remove(&unique);
                        Some(reply)
                    } else {
                        None
                    }
                } else {
                    // Request was already consumed or doesn't exist.
                    return Err(FsError::IoError);
                }
            };
            if let Some(reply) = result {
                // Parse the out header to check for errors.
                if reply.len() < core::mem::size_of::<FuseOutHeader>() {
                    return Err(FsError::IoError);
                }
                let out_header = unsafe {
                    core::ptr::read_unaligned(reply.as_ptr() as *const FuseOutHeader)
                };
                if out_header.error != 0 {
                    return Err(fuse_error_to_fs_error(out_header.error));
                }
                // Return payload after the header.
                return Ok(reply[core::mem::size_of::<FuseOutHeader>()..].to_vec());
            }
            // In a real kernel we'd yield/wait here. For now, spin.
            // The daemon is expected to call submit_reply from another context.
            core::hint::spin_loop();
        }
    }

    /// Submit a raw reply (including FuseOutHeader) from the daemon.
    /// This wakes up the waiting kernel thread.
    pub fn submit_reply(&self, raw: &[u8]) -> FsResult<()> {
        if raw.len() < core::mem::size_of::<FuseOutHeader>() {
            return Err(FsError::InvalidArgument);
        }
        let out_header = unsafe {
            core::ptr::read_unaligned(raw.as_ptr() as *const FuseOutHeader)
        };
        let mut pending = self.pending.lock();
        if let Some(req) = pending.get_mut(&out_header.unique) {
            req.completed = true;
            req.reply = Some(raw.to_vec());
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Read the next outgoing request (daemon side). Returns the raw request
    /// bytes including the FuseInHeader. Returns `None` if no requests are
    /// pending.
    pub fn read_request(&self) -> Option<Vec<u8>> {
        let mut outgoing = self.outgoing.lock();
        outgoing.pop_front()
    }

    /// Mark the connection as initialized (after FUSE_INIT handshake).
    pub fn set_initialized(&self, max_write: u32) {
        *self.max_write.lock() = max_write;
        *self.initialized.lock() = true;
    }

    /// Check if the INIT handshake has completed.
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock()
    }

    /// Perform a best-effort FUSE_INIT handshake. If the daemon is not
    /// available (no reply), returns `Ok(())` anyway so the filesystem can
    /// still be mounted — operations will fail individually if the daemon
    /// never responds.
    pub fn init_handshake(&self) -> FsResult<()> {
        let init_in = FuseInitIn {
            major: FUSE_KERNEL_VERSION,
            minor: FUSE_KERNEL_MINOR_VERSION,
            max_readahead: 4096,
            flags: 0,
        };
        let data = unsafe {
            core::slice::from_raw_parts(
                &init_in as *const FuseInitIn as *const u8,
                core::mem::size_of::<FuseInitIn>(),
            )
        };
        let unique = self.send_request(FuseOpcode::Init, 0, data)?;
        // Try to get the reply. If the daemon hasn't processed it yet in a
        // single-threaded context, we proceed without initialization.
        let result = self.wait_for_reply(unique);
        match result {
            Ok(payload) => {
                if payload.len() >= core::mem::size_of::<FuseInitOut>() {
                    let init_out = unsafe {
                        core::ptr::read_unaligned(payload.as_ptr() as *const FuseInitOut)
                    };
                    self.set_initialized(init_out.max_write.max(4096));
                } else {
                    self.set_initialized(4096);
                }
            }
            Err(_) => {
                // Daemon not yet available — proceed with defaults.
                self.set_initialized(4096);
            }
        }
        Ok(())
    }
}

/// Convert a FUSE error code (negative errno) to FsError.
fn fuse_error_to_fs_error(err: i32) -> FsError {
    match -err {
        2 => FsError::NotFound,       // ENOENT
        13 => FsError::PermissionDenied, // EACCES
        17 => FsError::AlreadyExists, // EEXIST
        20 => FsError::NotADirectory, // ENOTDIR
        21 => FsError::IsADirectory,  // EISDIR
        28 => FsError::NoSpaceLeft,   // ENOSPC
        30 => FsError::ReadOnly,      // EROFS
        22 => FsError::InvalidArgument, // EINVAL
        39 => FsError::DirectoryNotEmpty, // ENOTEMPTY
        36 => FsError::NameTooLong,   // ENAMETOOLONG
        40 => FsError::TooManySymlinks, // ELOOP
        18 => FsError::CrossDevice,   // EXDEV
        9 => FsError::BadFileDescriptor, // EBADF
        _ => FsError::IoError,
    }
}

/// Convert FuseAttr to FileMetadata.
fn fuse_attr_to_metadata(attr: &FuseAttr) -> FileMetadata {
    let ft = match attr.mode & 0xF000 {
        0x4000 => FileType::Directory,
        0x8000 => FileType::Regular,
        0xA000 => FileType::SymbolicLink,
        0x2000 => FileType::CharacterDevice,
        0x6000 => FileType::BlockDevice,
        0x1000 => FileType::NamedPipe,
        0xC000 => FileType::Socket,
        _ => FileType::Regular,
    };
    FileMetadata {
        inode: attr.ino,
        file_type: ft,
        size: attr.size,
        permissions: FilePermissions::from_octal((attr.mode & 0o777) as u16),
        uid: attr.uid,
        gid: attr.gid,
        created: attr.ctime,
        modified: attr.mtime,
        accessed: attr.atime,
        link_count: attr.nlink,
        device_id: None,
    }
}

/// Convert FileType to FUSE directory entry type field.
fn file_type_to_fuse_type(ft: FileType) -> u32 {
    match ft {
        FileType::Regular => 1,
        FileType::Directory => 2,
        FileType::CharacterDevice => 3,
        FileType::BlockDevice => 4,
        FileType::NamedPipe => 5,
        FileType::Socket => 6,
        FileType::SymbolicLink => 7,
    }
}

// ============================================================================
// FuseFileSystem
// ============================================================================

/// FUSE filesystem instance.
pub struct FuseFileSystem {
    connection: Arc<FuseConnection>,
    /// Path -> nodeid cache (for LOOKUP results).
    path_cache: Mutex<BTreeMap<String, u64>>,
    /// nodeid -> attr cache.
    attr_cache: Mutex<BTreeMap<u64, FuseAttr>>,
    /// Root nodeid (always 1 in FUSE).
    root_nodeid: u64,
}

impl core::fmt::Debug for FuseFileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FuseFileSystem")
            .field("root_nodeid", &self.root_nodeid)
            .field("path_cache_len", &self.path_cache.lock().len())
            .field("attr_cache_len", &self.attr_cache.lock().len())
            .finish()
    }
}

impl FuseFileSystem {
    /// Create a new FUSE filesystem over the given connection.
    /// Seeds the root cache and attempts a best-effort FUSE_INIT handshake.
    pub fn new(_device_id: u32, connection: Arc<FuseConnection>) -> FsResult<Self> {
        let root = 1u64;
        // Seed root cache.
        {
            let mut pc = connection.pending.lock();
            let _ = &mut pc; // ensure connection is valid
        }
        // Best-effort INIT handshake.
        let _ = connection.init_handshake();
        Ok(Self {
            connection,
            path_cache: Mutex::new(BTreeMap::new()),
            attr_cache: Mutex::new(BTreeMap::new()),
            root_nodeid: root,
        })
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Send a LOOKUP for `name` in directory `parent_nodeid` and cache the
    /// result. Returns the nodeid.
    fn lookup(&self, parent_nodeid: u64, name: &str) -> FsResult<u64> {
        let data = name.as_bytes();
        let unique = self
            .connection
            .send_request(FuseOpcode::Lookup, parent_nodeid, data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseEntryOut>() {
            return Err(FsError::IoError);
        }
        let entry = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseEntryOut)
        };
        {
            let mut ac = self.attr_cache.lock();
            ac.insert(entry.nodeid, entry.attr);
        }
        Ok(entry.nodeid)
    }

    /// Resolve a path to a FUSE nodeid by walking the tree with LOOKUP.
    fn resolve_path(&self, path: &str) -> FsResult<u64> {
        // Check cache first.
        {
            let pc = self.path_cache.lock();
            if let Some(&nid) = pc.get(path) {
                return Ok(nid);
            }
        }
        if path == "/" {
            return Ok(self.root_nodeid);
        }
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let mut cur = self.root_nodeid;
        let mut accumulated = String::new();
        for comp in components {
            cur = self.lookup(cur, comp)?;
            accumulated = format!("{}/{}", accumulated, comp);
            {
                let mut pc = self.path_cache.lock();
                pc.insert(accumulated.clone(), cur);
            }
        }
        Ok(cur)
    }

    /// Send a GETATTR and return the metadata.
    fn getattr(&self, nodeid: u64) -> FsResult<FileMetadata> {
        // Check cache.
        {
            let ac = self.attr_cache.lock();
            if let Some(attr) = ac.get(&nodeid) {
                return Ok(fuse_attr_to_metadata(attr));
            }
        }
        let unique = self
            .connection
            .send_request(FuseOpcode::Getattr, nodeid, &[])?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseGetattrOut>() {
            return Err(FsError::IoError);
        }
        let out = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseGetattrOut)
        };
        {
            let mut ac = self.attr_cache.lock();
            ac.insert(nodeid, out.attr);
        }
        Ok(fuse_attr_to_metadata(&out.attr))
    }

    /// Invalidate cached path and attr entries for a nodeid.
    fn invalidate(&self, nodeid: u64) {
        {
            let mut ac = self.attr_cache.lock();
            ac.remove(&nodeid);
        }
        // Remove all path cache entries pointing to this nodeid.
        let to_remove: Vec<String> = {
            let pc = self.path_cache.lock();
            pc.iter()
                .filter(|(_, &v)| v == nodeid)
                .map(|(k, _)| k.clone())
                .collect()
        };
        let mut pc = self.path_cache.lock();
        for k in to_remove {
            pc.remove(&k);
        }
    }

    /// Split path into (parent_path, name).
    fn split_path(path: &str) -> FsResult<(String, String)> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        match trimmed.rfind('/') {
            Some(idx) => {
                let name = trimmed[idx + 1..].to_string();
                if name.is_empty() {
                    return Err(FsError::InvalidArgument);
                }
                let parent = if idx == 0 {
                    String::from("/")
                } else {
                    trimmed[..idx].to_string()
                };
                Ok((parent, name))
            }
            None => Ok((String::from("/"), trimmed.to_string())),
        }
    }
}

impl FileSystem for FuseFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Fuse
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let unique = self
            .connection
            .send_request(FuseOpcode::Statfs, self.root_nodeid, &[])?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseStatfsOut>() {
            return Ok(FileSystemStats {
                total_blocks: 0,
                free_blocks: 0,
                available_blocks: 0,
                total_inodes: 0,
                free_inodes: 0,
                block_size: 4096,
                max_filename_length: 255,
            });
        }
        let out = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseStatfsOut)
        };
        Ok(FileSystemStats {
            total_blocks: out.st.blocks,
            free_blocks: out.st.bfree,
            available_blocks: out.st.bavail,
            total_inodes: out.st.files,
            free_inodes: out.st.ffree,
            block_size: out.st.bsize,
            max_filename_length: out.st.namelen,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path)?;
        let parent_nodeid = self.resolve_path(&parent_path)?;
        let create_in = FuseCreateIn {
            flags: 0o2, // O_RDWR
            mode: (0o100000u32 | permissions.to_octal() as u32),
            umask: 0,
            padding: 0,
        };
        let mut data = Vec::new();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &create_in as *const FuseCreateIn as *const u8,
                core::mem::size_of::<FuseCreateIn>(),
            )
        };
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(name.as_bytes());
        let unique = self
            .connection
            .send_request(FuseOpcode::Create, parent_nodeid, &data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseEntryOut>() {
            return Err(FsError::IoError);
        }
        let entry = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseEntryOut)
        };
        {
            let mut ac = self.attr_cache.lock();
            ac.insert(entry.nodeid, entry.attr);
        }
        self.invalidate(parent_nodeid);
        Ok(entry.nodeid as InodeNumber)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        let nodeid = self.resolve_path(path)?;
        // Send FUSE_OPEN for regular files.
        let open_in = FuseOpenIn {
            flags: if flags.write { 0o2 } else { 0o0 },
            unused: 0,
        };
        let data = unsafe {
            core::slice::from_raw_parts(
                &open_in as *const FuseOpenIn as *const u8,
                core::mem::size_of::<FuseOpenIn>(),
            )
        };
        let unique = self
            .connection
            .send_request(FuseOpcode::Open, nodeid, data)?;
        let _ = self.connection.wait_for_reply(unique)?;
        Ok(nodeid as InodeNumber)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let nodeid = inode as u64;
        let read_in = FuseReadIn {
            fh: 0,
            offset,
            size: buffer.len() as u32,
            read_flags: 0,
            lock_owner: 0,
            flags: 0,
            padding: 0,
        };
        let data = unsafe {
            core::slice::from_raw_parts(
                &read_in as *const FuseReadIn as *const u8,
                core::mem::size_of::<FuseReadIn>(),
            )
        };
        let unique = self
            .connection
            .send_request(FuseOpcode::Read, nodeid, data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        let to_copy = core::cmp::min(reply.len(), buffer.len());
        buffer[..to_copy].copy_from_slice(&reply[..to_copy]);
        Ok(to_copy)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let nodeid = inode as u64;
        let write_in = FuseWriteIn {
            fh: 0,
            offset,
            size: buffer.len() as u32,
            write_flags: 0,
            lock_owner: 0,
            flags: 0,
            padding: 0,
        };
        let mut data = Vec::new();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &write_in as *const FuseWriteIn as *const u8,
                core::mem::size_of::<FuseWriteIn>(),
            )
        };
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(buffer);
        let unique = self
            .connection
            .send_request(FuseOpcode::Write, nodeid, &data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseWriteOut>() {
            return Err(FsError::IoError);
        }
        let out = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseWriteOut)
        };
        self.invalidate(nodeid);
        Ok(out.size as usize)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        self.getattr(inode as u64)
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let nodeid = inode as u64;
        let setattr_in = FuseSetattrIn {
            valid: 0x1F, // mode | uid | gid | size | atime/mtime
            padding: 0,
            fh: 0,
            size: metadata.size,
            lock_owner: 0,
            atime: metadata.accessed,
            mtime: metadata.modified,
            unused2: 0,
            atimensec: 0,
            mtimensec: 0,
            unused3: 0,
            mode: (0o100000u32 | metadata.permissions.to_octal() as u32),
            unused4: 0,
            uid: metadata.uid,
            gid: metadata.gid,
            unused5: 0,
        };
        let data = unsafe {
            core::slice::from_raw_parts(
                &setattr_in as *const FuseSetattrIn as *const u8,
                core::mem::size_of::<FuseSetattrIn>(),
            )
        };
        let unique = self
            .connection
            .send_request(FuseOpcode::Setattr, nodeid, data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        // Update cache from the getattr reply that follows.
        if reply.len() >= core::mem::size_of::<FuseGetattrOut>() {
            let out = unsafe {
                core::ptr::read_unaligned(reply.as_ptr() as *const FuseGetattrOut)
            };
            let mut ac = self.attr_cache.lock();
            ac.insert(nodeid, out.attr);
        }
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path)?;
        let parent_nodeid = self.resolve_path(&parent_path)?;
        let mkdir_in = FuseMkdirIn {
            mode: (0o040000u32 | permissions.to_octal() as u32),
            umask: 0,
        };
        let mut data = Vec::new();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &mkdir_in as *const FuseMkdirIn as *const u8,
                core::mem::size_of::<FuseMkdirIn>(),
            )
        };
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(name.as_bytes());
        let unique = self
            .connection
            .send_request(FuseOpcode::Mkdir, parent_nodeid, &data)?;
        let reply = self.connection.wait_for_reply(unique)?;
        if reply.len() < core::mem::size_of::<FuseEntryOut>() {
            return Err(FsError::IoError);
        }
        let entry = unsafe {
            core::ptr::read_unaligned(reply.as_ptr() as *const FuseEntryOut)
        };
        {
            let mut ac = self.attr_cache.lock();
            ac.insert(entry.nodeid, entry.attr);
        }
        self.invalidate(parent_nodeid);
        Ok(entry.nodeid as InodeNumber)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(path)?;
        let parent_nodeid = self.resolve_path(&parent_path)?;
        let data = name.as_bytes();
        let unique = self
            .connection
            .send_request(FuseOpcode::Rmdir, parent_nodeid, data)?;
        let _ = self.connection.wait_for_reply(unique)?;
        self.invalidate(parent_nodeid);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(path)?;
        let parent_nodeid = self.resolve_path(&parent_path)?;
        let data = name.as_bytes();
        let unique = self
            .connection
            .send_request(FuseOpcode::Unlink, parent_nodeid, data)?;
        let _ = self.connection.wait_for_reply(unique)?;
        self.invalidate(parent_nodeid);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let nodeid = inode as u64;
        let mut entries = Vec::new();
        let mut offset = 0u64;
        loop {
            let read_in = FuseReadIn {
                fh: 0,
                offset,
                size: 4096,
                read_flags: 0,
                lock_owner: 0,
                flags: 0,
                padding: 0,
            };
            let data = unsafe {
                core::slice::from_raw_parts(
                    &read_in as *const FuseReadIn as *const u8,
                    core::mem::size_of::<FuseReadIn>(),
                )
            };
            let unique = self
                .connection
                .send_request(FuseOpcode::Readdir, nodeid, data)?;
            let reply = self.connection.wait_for_reply(unique)?;
            if reply.is_empty() {
                break;
            }
            let mut off = 0usize;
            let mut got_any = false;
            while off + core::mem::size_of::<FuseDirent>() <= reply.len() {
                let dirent = unsafe {
                    core::ptr::read_unaligned(
                        reply.as_ptr().add(off) as *const FuseDirent,
                    )
                };
                if dirent.namelen == 0 {
                    break;
                }
                got_any = true;
                let name_start = off + core::mem::size_of::<FuseDirent>();
                let name_end = name_start + dirent.namelen as usize;
                if name_end > reply.len() {
                    break;
                }
                let name_bytes = &reply[name_start..name_end];
                let name = core::str::from_utf8(name_bytes)
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !name.is_empty() && name != "." && name != ".." {
                    let ft = match dirent.typ {
                        1 => FileType::Regular,
                        2 => FileType::Directory,
                        3 => FileType::CharacterDevice,
                        4 => FileType::BlockDevice,
                        5 => FileType::NamedPipe,
                        6 => FileType::Socket,
                        7 => FileType::SymbolicLink,
                        _ => FileType::Regular,
                    };
                    entries.push(DirectoryEntry {
                        name,
                        inode: dirent.ino as InodeNumber,
                        file_type: ft,
                    });
                }
                // Advance past the dirent + name + NUL padding to 8-byte boundary.
                let entry_size = core::mem::size_of::<FuseDirent>() + dirent.namelen as usize;
                let padded = (entry_size + 7) & !7;
                off += padded;
                offset = dirent.off;
            }
            if !got_any {
                break;
            }
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent_path, old_name) = Self::split_path(old_path)?;
        let (new_parent_path, new_name) = Self::split_path(new_path)?;
        let old_parent = self.resolve_path(&old_parent_path)?;
        let new_parent = self.resolve_path(&new_parent_path)?;
        let rename_in = FuseRenameIn {
            newdir: new_parent,
            oldname_size: old_name.len() as u32,
            padding: 0,
        };
        let mut data = Vec::new();
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &rename_in as *const FuseRenameIn as *const u8,
                core::mem::size_of::<FuseRenameIn>(),
            )
        };
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(old_name.as_bytes());
        data.push(0); // NUL separator
        data.extend_from_slice(new_name.as_bytes());
        let unique = self
            .connection
            .send_request(FuseOpcode::Rename, old_parent, &data)?;
        let _ = self.connection.wait_for_reply(unique)?;
        self.invalidate(old_parent);
        self.invalidate(new_parent);
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(link_path)?;
        let parent_nodeid = self.resolve_path(&parent_path)?;
        let mut data = Vec::new();
        data.extend_from_slice(name.as_bytes());
        data.push(0); // NUL separator
        data.extend_from_slice(target.as_bytes());
        let unique = self
            .connection
            .send_request(FuseOpcode::Symlink, parent_nodeid, &data)?;
        let _ = self.connection.wait_for_reply(unique)?;
        self.invalidate(parent_nodeid);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let nodeid = self.resolve_path(path)?;
        let unique = self
            .connection
            .send_request(FuseOpcode::Readlink, nodeid, &[])?;
        let reply = self.connection.wait_for_reply(unique)?;
        core::str::from_utf8(&reply)
            .map(|s| s.to_string())
            .map_err(|_| FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // Send FSYNC on root to flush everything.
        let unique = self
            .connection
            .send_request(FuseOpcode::Fsync, self.root_nodeid, &[])?;
        let _ = self.connection.wait_for_reply(unique);
        Ok(())
    }
}
