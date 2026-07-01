//! JFS (Journaled File System) in-memory implementation.
//!
//! JFS is a high-performance journaled filesystem with extent-based
//! allocation. This implementation provides a fully functional in-memory VFS
//! with a journal transaction log. On-disk superblock parsing, B+ tree extent
//! allocation, and journal recovery are out of scope for the in-memory model;
//! instead, a journal records metadata transactions in memory so that the
//! filesystem can replay them and expose journal state to callers.

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
use spin::RwLock;

/// Maximum file size in the in-memory JFS filesystem (16 MiB).
const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;
/// Maximum number of inodes.
const MAX_INODES: u64 = 4096;

/// Journal transaction type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalTxType {
    /// Create a file.
    Create,
    /// Create a directory.
    Mkdir,
    /// Remove a file.
    Unlink,
    /// Remove a directory.
    Rmdir,
    /// Write data.
    Write,
    /// Rename a path.
    Rename,
    /// Symlink creation.
    Symlink,
    /// Metadata update.
    SetMetadata,
}

/// A journal transaction record.
#[derive(Debug, Clone)]
pub struct JournalTx {
    /// Transaction sequence number (monotonic).
    pub seq: u64,
    /// Transaction type.
    pub tx_type: JournalTxType,
    /// Affected path (or primary path for rename).
    pub path: String,
    /// Secondary path (for rename: the new path).
    pub path2: Option<String>,
    /// Affected inode (if known).
    pub inode: Option<InodeNumber>,
    /// Timestamp the transaction was committed.
    pub timestamp: u64,
}

/// Journal state.
#[derive(Debug)]
struct Journal {
    /// Committed transactions in sequence order.
    log: Vec<JournalTx>,
    /// Next sequence number.
    next_seq: u64,
    /// Whether the journal is currently in a committed (clean) state.
    clean: bool,
}

impl Journal {
    fn new() -> Self {
        Self {
            log: Vec::new(),
            next_seq: 1,
            clean: true,
        }
    }

    fn record(
        &mut self,
        tx_type: JournalTxType,
        path: &str,
        path2: Option<&str>,
        inode: Option<InodeNumber>,
    ) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.log.push(JournalTx {
            seq,
            tx_type,
            path: path.to_string(),
            path2: path2.map(|s| s.to_string()),
            inode,
            timestamp: get_current_time(),
        });
        // An in-memory journal is always clean after a synchronous record.
        self.clean = true;
    }
}

/// In-memory inode.
#[derive(Debug, Clone)]
struct JfsInode {
    metadata: FileMetadata,
    content: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
    symlink_target: Option<String>,
}

impl JfsInode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
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
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 2,
                device_id: None,
            },
            content: Vec::new(),
            entries,
            symlink_target: None,
        }
    }

    fn new_symlink(inode: InodeNumber, target: &str) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: target.len() as u64,
                permissions: FilePermissions::from_octal(0o777),
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// JFS filesystem.
#[derive(Debug)]
pub struct JfsFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, JfsInode>>,
    next_inode: RwLock<InodeNumber>,
    root_inode: InodeNumber,
    /// Metadata journal.
    journal: RwLock<Journal>,
}

impl JfsFileSystem {
    /// Create a new JFS filesystem instance.
    ///
    /// Initializes the root directory and an empty journal.
    pub fn new() -> FsResult<Self> {
        let root_inode = 1;
        let mut root = JfsInode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        let mut inodes = BTreeMap::new();
        inodes.insert(root_inode, root);

        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
            journal: RwLock::new(Journal::new()),
        })
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

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
            return Ok((self.root_inode, filename));
        }
        let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
        let parent_inode = self.resolve_path(&parent_path)?;
        Ok((parent_inode, filename))
    }

    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let dir = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.entries.len() <= 2)
    }

    // ── Journal API ────────────────────────────────────────────────────────

    /// Number of transactions recorded in the journal.
    pub fn journal_len(&self) -> usize {
        self.journal.read().log.len()
    }

    /// Whether the journal is in a clean (committed) state.
    pub fn journal_clean(&self) -> bool {
        self.journal.read().clean
    }

    /// Get a snapshot of the last `n` journal transactions (most recent last).
    pub fn journal_recent(&self, n: usize) -> Vec<JournalTx> {
        let journal = self.journal.read();
        let log = &journal.log;
        let start = log.len().saturating_sub(n);
        log[start..].to_vec()
    }

    /// Clear the journal log (e.g. after a checkpoint).
    pub fn journal_checkpoint(&self) {
        self.journal.write().log.clear();
    }
}

impl FileSystem for JfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let block_size = 4096u32;
        let used_blocks: u64 = inodes
            .values()
            .map(|i| (i.content.len() as u64 + block_size as u64 - 1) / block_size as u64)
            .sum();
        let total_blocks = (MAX_FILE_SIZE * MAX_INODES) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_INODES,
            free_inodes: MAX_INODES.saturating_sub(used),
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
        if inodes.len() >= MAX_INODES as usize {
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
        let file_inode = JfsInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, file_inode);
        drop(inodes);
        self.journal
            .write()
            .record(JournalTxType::Create, path, None, Some(new_inode));
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        node.metadata.accessed = get_current_time();
        let len = node.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), node.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&node.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > MAX_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }
        let required = new_size as usize;
        if node.content.len() < required {
            node.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        node.content[start..end].copy_from_slice(buffer);
        node.metadata.size = node.content.len() as u64;
        node.metadata.modified = get_current_time();
        node.metadata.accessed = get_current_time();
        drop(inodes);
        self.journal.write().record(
            JournalTxType::Write,
            &format!("/inode/{}", inode),
            None,
            Some(inode),
        );
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
        drop(inodes);
        self.journal.write().record(
            JournalTxType::SetMetadata,
            &format!("/inode/{}", inode),
            None,
            Some(inode),
        );
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
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = JfsInode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        inodes.insert(new_inode, dir);
        drop(inodes);
        self.journal
            .write()
            .record(JournalTxType::Mkdir, path, None, Some(new_inode));
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
        drop(inodes);
        self.journal
            .write()
            .record(JournalTxType::Rmdir, path, None, Some(dir_inode));
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
        drop(inodes);
        self.journal
            .write()
            .record(JournalTxType::Unlink, path, None, Some(file_inode));
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        let snapshot: Vec<(String, InodeNumber)> =
            dir.entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut out = Vec::new();
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                out.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: child.metadata.file_type,
                });
            }
        }
        Ok(out)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent_inode, old_filename) = self.resolve_parent(old_path)?;
        let (new_parent_inode, new_filename) = self.resolve_parent(new_path)?;
        if new_filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let new_parent = inodes.get(&new_parent_inode).ok_or(FsError::NotFound)?;
        if new_parent.entries.contains_key(&new_filename) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent = inodes.get_mut(&old_parent_inode).ok_or(FsError::NotFound)?;
        old_parent.entries.remove(&old_filename);
        old_parent.metadata.modified = get_current_time();
        let new_parent = inodes.get_mut(&new_parent_inode).ok_or(FsError::NotFound)?;
        new_parent.entries.insert(new_filename, old_inode);
        new_parent.metadata.modified = get_current_time();
        drop(inodes);
        self.journal.write().record(
            JournalTxType::Rename,
            old_path,
            Some(new_path),
            Some(old_inode),
        );
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        if inodes.len() >= MAX_INODES as usize {
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
        let sym = JfsInode::new_symlink(new_inode, target);
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, sym);
        drop(inodes);
        self.journal.write().record(
            JournalTxType::Symlink,
            link_path,
            Some(target),
            Some(new_inode),
        );
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let sym = inodes.get(&link_inode).ok_or(FsError::NotFound)?;
        if sym.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        sym.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // Flush the journal: in-memory, this just marks it clean.
        self.journal.write().clean = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_records() {
        let fs = JfsFileSystem::new().unwrap();
        assert_eq!(fs.journal_len(), 0);
        let _ = fs.create("/a", FilePermissions::default_file()).unwrap();
        let _ = fs
            .mkdir("/d", FilePermissions::default_directory())
            .unwrap();
        assert_eq!(fs.journal_len(), 2);
        let recent = fs.journal_recent(2);
        assert_eq!(recent[0].tx_type, JournalTxType::Create);
        assert_eq!(recent[1].tx_type, JournalTxType::Mkdir);
        fs.journal_checkpoint();
        assert_eq!(fs.journal_len(), 0);
    }

    #[test]
    fn test_file_ops() {
        let fs = JfsFileSystem::new().unwrap();
        let inode = fs.create("/f", FilePermissions::default_file()).unwrap();
        fs.write(inode, 0, b"data").unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(fs.read(inode, 0, &mut buf).unwrap(), 4);
        assert_eq!(&buf, b"data");
    }
}
