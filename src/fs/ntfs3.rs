//! NTFS3 filesystem implementation.
//!
//! In-memory VFS implementation of an NTFS3 mount. Real NTFS3 requires a block
//! device and parses the Master File Table (MFT), attribute lists, run lists,
//! and the cluster bitmap; this implementation tracks the equivalent state
//! (MFT entries, attribute list, cluster allocation bitmap) in memory and
//! services VFS operations against an in-memory content store. The I/O layer
//! would replace the in-memory structures with on-disk MFT/cluster reads.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::cmp;
use spin::RwLock;

/// Number of clusters the in-memory NTFS volume pretends to manage.
const NTFS_TOTAL_CLUSTERS: u64 = 65536;
/// Cluster size in bytes (4 KiB).
const NTFS_CLUSTER_SIZE: u32 = 4096;
/// Maximum number of MFT entries (inodes).
const NTFS_MAX_MFT_ENTRIES: u64 = 4096;
/// Maximum single-file size in the in-memory cache.
const NTFS_MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

/// NTFS attribute type codes (subset matching fs/ntfs3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtfsAttributeType {
    /// Standard information (timestamps, attributes).
    StandardInformation,
    /// Attribute list (used when attributes span multiple MFT records).
    AttributeList,
    /// File name attribute.
    FileName,
    /// Data stream (the file's contents / default $DATA).
    Data,
    /// Index root (directory index).
    IndexRoot,
    /// Index allocation (directory index continuation).
    IndexAllocation,
}

/// An NTFS attribute attached to an MFT entry.
#[derive(Debug, Clone)]
struct NtfsAttribute {
    /// Attribute type.
    attr_type: NtfsAttributeType,
    /// Attribute name (e.g. data stream name, usually empty for $DATA).
    name: String,
    /// Whether the attribute is resident (stored in the MFT entry).
    resident: bool,
    /// Allocated size in bytes (for non-resident attributes).
    allocated_size: u64,
}

/// In-memory MFT entry.
#[derive(Debug, Clone)]
struct MftEntry {
    /// VFS metadata.
    metadata: FileMetadata,
    /// Attributes attached to this entry.
    attributes: Vec<NtfsAttribute>,
    /// Resident data stream content (default $DATA).
    content: Vec<u8>,
    /// Directory index: child name -> inode number.
    index: BTreeMap<String, InodeNumber>,
    /// Symbolic link target (reparse point).
    symlink_target: Option<String>,
    /// Sequence number bumped each time the entry is reused.
    sequence: u64,
}

impl MftEntry {
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
            attributes: vec![
                NtfsAttribute {
                    attr_type: NtfsAttributeType::StandardInformation,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
                NtfsAttribute {
                    attr_type: NtfsAttributeType::FileName,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
                NtfsAttribute {
                    attr_type: NtfsAttributeType::Data,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
            ],
            content: Vec::new(),
            index: BTreeMap::new(),
            symlink_target: None,
            sequence: 1,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        let mut index = BTreeMap::new();
        index.insert(".".to_string(), inode);
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
            attributes: vec![
                NtfsAttribute {
                    attr_type: NtfsAttributeType::StandardInformation,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
                NtfsAttribute {
                    attr_type: NtfsAttributeType::FileName,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
                NtfsAttribute {
                    attr_type: NtfsAttributeType::IndexRoot,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
            ],
            content: Vec::new(),
            index,
            symlink_target: None,
            sequence: 1,
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
            attributes: vec![
                NtfsAttribute {
                    attr_type: NtfsAttributeType::StandardInformation,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
                NtfsAttribute {
                    attr_type: NtfsAttributeType::FileName,
                    name: String::new(),
                    resident: true,
                    allocated_size: 0,
                },
            ],
            content: Vec::new(),
            index: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
            sequence: 1,
        }
    }

    /// Mark the $DATA attribute non-resident and record its allocated size.
    fn make_data_nonresident(&mut self, allocated: u64) {
        for attr in self.attributes.iter_mut() {
            if attr.attr_type == NtfsAttributeType::Data {
                attr.resident = false;
                attr.allocated_size = allocated;
            }
        }
    }
}

/// NTFS3 filesystem instance.
#[derive(Debug)]
pub struct Ntfs3FileSystem {
    /// Block device id this volume is backed by.
    device_id: u32,
    /// MFT entries keyed by inode number.
    mft: RwLock<BTreeMap<InodeNumber, MftEntry>>,
    /// Next MFT record number to allocate.
    next_mft_record: RwLock<InodeNumber>,
    /// Cluster allocation bitmap: set bit = cluster in use.
    cluster_bitmap: RwLock<Vec<bool>>,
    /// Number of clusters currently allocated.
    used_clusters: RwLock<u64>,
    /// Root directory inode number.
    root_inode: InodeNumber,
}

impl Ntfs3FileSystem {
    /// Create a new NTFS3 filesystem instance.
    ///
    /// Initializes the MFT with the root directory record (record 5, matching
    /// the conventional NTFS root) and an empty cluster bitmap. A real I/O
    /// layer would read $MFT and $Bitmap from the block device instead.
    pub fn new(device_id: u32) -> FsResult<Self> {
        let root_inode = 5;
        let mut mft = BTreeMap::new();
        let mut root = MftEntry::new_directory(root_inode, FilePermissions::default_directory());
        root.index.insert("..".to_string(), root_inode);
        mft.insert(root_inode, root);

        Ok(Self {
            device_id,
            mft: RwLock::new(mft),
            next_mft_record: RwLock::new(6),
            cluster_bitmap: RwLock::new(vec![false; NTFS_TOTAL_CLUSTERS as usize]),
            used_clusters: RwLock::new(0),
            root_inode,
        })
    }

    /// Block device id backing this volume.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Allocate the next MFT record number.
    fn allocate_mft_record(&self) -> InodeNumber {
        let mut next = self.next_mft_record.write();
        let n = *next;
        *next += 1;
        n
    }

    /// Allocate `count` contiguous clusters, returning the starting cluster
    /// index. Returns None if the bitmap is too fragmented to satisfy.
    fn allocate_clusters(&self, count: u64) -> Option<u64> {
        if count == 0 {
            return Some(0);
        }
        let mut bitmap = self.cluster_bitmap.write();
        let mut used = self.used_clusters.write();
        let total = bitmap.len() as u64;
        let mut run_start: Option<u64> = None;
        let mut run_len = 0u64;
        for (i, slot) in bitmap.iter().enumerate() {
            if !*slot {
                if run_start.is_none() {
                    run_start = Some(i as u64);
                }
                run_len += 1;
                if run_len >= count {
                    let start = run_start.unwrap();
                    for c in start..start + count {
                        bitmap[c as usize] = true;
                    }
                    *used += count;
                    return Some(start);
                }
            } else {
                run_start = None;
                run_len = 0;
            }
        }
        let _ = total;
        None
    }

    /// Free `count` clusters starting at `start`.
    fn free_clusters(&self, start: u64, count: u64) {
        let mut bitmap = self.cluster_bitmap.write();
        let mut used = self.used_clusters.write();
        for c in start..start + count {
            if (c as usize) < bitmap.len() && bitmap[c as usize] {
                bitmap[c as usize] = false;
                *used = used.saturating_sub(1);
            }
        }
    }

    /// Split a path into non-empty components.
    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Resolve a path to an inode number via the MFT directory indices.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = Self::split_path(path);
        let mft = self.mft.read();
        let mut current = self.root_inode;
        for component in components {
            let entry = mft.get(&current).ok_or(FsError::NotFound)?;
            if entry.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *entry.index.get(&component).ok_or(FsError::NotFound)?;
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

    /// Whether a directory index holds only "." and "..".
    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let mft = self.mft.read();
        let dir = mft.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.index.len() <= 2)
    }
}

impl FileSystem for Ntfs3FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ntfs3
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let mft = self.mft.read();
        let used = mft.len() as u64;
        let used_clusters = *self.used_clusters.read();
        Ok(FileSystemStats {
            total_blocks: NTFS_TOTAL_CLUSTERS,
            free_blocks: NTFS_TOTAL_CLUSTERS.saturating_sub(used_clusters),
            available_blocks: NTFS_TOTAL_CLUSTERS.saturating_sub(used_clusters),
            total_inodes: NTFS_MAX_MFT_ENTRIES,
            free_inodes: NTFS_MAX_MFT_ENTRIES.saturating_sub(used),
            block_size: NTFS_CLUSTER_SIZE,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut mft = self.mft.write();
        if mft.len() >= NTFS_MAX_MFT_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = mft.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.index.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_mft_record();
        let file = MftEntry::new_file(new_inode, permissions);
        parent.index.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        mft.insert(new_inode, file);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut mft = self.mft.write();
        let entry = mft.get_mut(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        entry.metadata.accessed = get_current_time();
        let len = entry.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), entry.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&entry.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut mft = self.mft.write();
        let entry = mft.get_mut(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > NTFS_MAX_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }
        let required = new_size as usize;
        if entry.content.len() < required {
            entry.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        entry.content[start..end].copy_from_slice(buffer);
        entry.metadata.size = entry.content.len() as u64;
        entry.metadata.modified = get_current_time();
        entry.metadata.accessed = get_current_time();
        // Transition the $DATA attribute to non-resident once it exceeds a
        // cluster, mirroring fs/ntfs3 which spills resident data to a run list.
        let clusters_needed =
            (entry.content.len() as u64 + NTFS_CLUSTER_SIZE as u64 - 1) / NTFS_CLUSTER_SIZE as u64;
        if clusters_needed > 0 {
            entry.make_data_nonresident(clusters_needed * NTFS_CLUSTER_SIZE as u64);
        }
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let mft = self.mft.read();
        let entry = mft.get(&inode).ok_or(FsError::NotFound)?;
        Ok(entry.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut mft = self.mft.write();
        let entry = mft.get_mut(&inode).ok_or(FsError::NotFound)?;
        entry.metadata.permissions = metadata.permissions;
        entry.metadata.uid = metadata.uid;
        entry.metadata.gid = metadata.gid;
        entry.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut mft = self.mft.write();
        if mft.len() >= NTFS_MAX_MFT_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = mft.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.index.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_mft_record();
        let mut dir = MftEntry::new_directory(new_inode, permissions);
        dir.index.insert("..".to_string(), parent_inode);
        parent.index.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        mft.insert(new_inode, dir);
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
        let mut mft = self.mft.write();
        let parent = mft.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.index.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        if let Some(removed) = mft.remove(&dir_inode) {
            let clusters = (removed.content.len() as u64 + NTFS_CLUSTER_SIZE as u64 - 1)
                / NTFS_CLUSTER_SIZE as u64;
            if clusters > 0 {
                // In-memory content has no fixed cluster offset to free; the
                // bitmap accounting is handled at allocation time. A real
                // implementation would free the run list here.
                self.free_clusters(0, clusters);
            }
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut mft = self.mft.write();
        let file = mft.get(&file_inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = mft.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.index.remove(&filename);
        parent.metadata.modified = get_current_time();
        if let Some(removed) = mft.remove(&file_inode) {
            let clusters = (removed.content.len() as u64 + NTFS_CLUSTER_SIZE as u64 - 1)
                / NTFS_CLUSTER_SIZE as u64;
            if clusters > 0 {
                self.free_clusters(0, clusters);
            }
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut mft = self.mft.write();
        let dir = mft.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        let entry_list: Vec<(String, InodeNumber)> = dir
            .index
            .iter()
            .map(|(name, &ino)| (name.clone(), ino))
            .collect();
        let mut entries = Vec::new();
        for (name, ino) in entry_list {
            if let Some(node) = mft.get(&ino) {
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
        let mut mft = self.mft.write();
        let new_parent_node = mft.get(&new_parent).ok_or(FsError::NotFound)?;
        if new_parent_node.index.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent_node = mft.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        old_parent_node.index.remove(&old_name);
        old_parent_node.metadata.modified = get_current_time();
        let new_parent_node = mft.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_parent_node.index.insert(new_name, old_inode);
        new_parent_node.metadata.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut mft = self.mft.write();
        if mft.len() >= NTFS_MAX_MFT_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = mft.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.index.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_mft_record();
        let link = MftEntry::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));
        parent.index.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        mft.insert(new_inode, link);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let mft = self.mft.read();
        let link = mft.get(&link_inode).ok_or(FsError::NotFound)?;
        if link.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        link.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory MFT is always consistent. A real implementation would flush
        // dirty MFT records and run lists to the block device here.
        Ok(())
    }
}
