//! exFAT filesystem implementation.
//!
//! In-memory VFS implementation of an exFAT mount. Real exFAT requires a block
//! device and parses the FAT chain, the allocation bitmap, the up-case table,
//! and directory entries (with a different timestamp format than FAT32); this
//! implementation tracks the equivalent state (FAT chain, directory entries,
//! cluster allocation bitmap) in memory and services VFS operations against an
//! in-memory content store. The I/O layer would replace the in-memory FAT with
//! on-disk FAT/cluster reads.

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

/// Number of clusters the in-memory exFAT volume manages.
const EXFAT_TOTAL_CLUSTERS: u64 = 65536;
/// Cluster size in bytes (32 KiB, typical exFAT default).
const EXFAT_CLUSTER_SIZE: u32 = 32 * 1024;
/// Maximum number of directory entries / inodes.
const EXFAT_MAX_ENTRIES: u64 = 4096;
/// Maximum single-file size in the in-memory cache.
const EXFAT_MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

/// FAT chain entry values. exFAT uses 32-bit FAT entries where 0xFFFFFFFF is
/// the end-of-chain marker and 0xFFFFFFF7 is a bad-cluster marker.
const FAT_FREE: u32 = 0;
const FAT_EOC: u32 = 0xFFFF_FFFF;
const FAT_BAD: u32 = 0xFFFF_FFF7;

/// exFAT file attributes (subset of the on-disk attribute byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExfatAttributes {
    /// Read-only flag.
    pub read_only: bool,
    /// Hidden flag.
    pub hidden: bool,
    /// System flag.
    pub system: bool,
    /// Directory flag.
    pub directory: bool,
    /// Archive flag.
    pub archive: bool,
}

impl ExfatAttributes {
    /// Default attributes for a regular file.
    fn file() -> Self {
        Self {
            read_only: false,
            hidden: false,
            system: false,
            directory: false,
            archive: true,
        }
    }

    /// Default attributes for a directory.
    fn directory() -> Self {
        Self {
            read_only: false,
            hidden: false,
            system: false,
            directory: true,
            archive: false,
        }
    }
}

/// In-memory exFAT directory entry / inode.
#[derive(Debug, Clone)]
struct ExfatEntry {
    /// VFS metadata.
    metadata: FileMetadata,
    /// exFAT attribute flags.
    attributes: ExfatAttributes,
    /// First cluster of the FAT chain (0 if resident/empty).
    first_cluster: u32,
    /// Cached file content (regular files only).
    content: Vec<u8>,
    /// Directory child index: name -> inode number.
    children: BTreeMap<String, InodeNumber>,
    /// Symbolic link target (symlinks only).
    symlink_target: Option<String>,
}

impl ExfatEntry {
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
            attributes: ExfatAttributes::file(),
            first_cluster: 0,
            content: Vec::new(),
            children: BTreeMap::new(),
            symlink_target: None,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        let mut children = BTreeMap::new();
        children.insert(".".to_string(), inode);
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
            attributes: ExfatAttributes::directory(),
            first_cluster: 0,
            content: Vec::new(),
            children,
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
            attributes: ExfatAttributes::file(),
            first_cluster: 0,
            content: Vec::new(),
            children: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// exFAT filesystem instance.
#[derive(Debug)]
pub struct ExfatFileSystem {
    /// Block device id this volume is backed by.
    device_id: u32,
    /// In-memory directory entries keyed by inode number.
    entries: RwLock<BTreeMap<InodeNumber, ExfatEntry>>,
    /// Next inode number to allocate.
    next_entry: RwLock<InodeNumber>,
    /// In-memory FAT chain: cluster index -> next cluster (or FAT_EOC).
    fat: RwLock<BTreeMap<u32, u32>>,
    /// Cluster allocation bitmap: set bit = cluster in use.
    alloc_bitmap: RwLock<Vec<bool>>,
    /// Number of clusters currently allocated.
    used_clusters: RwLock<u64>,
    /// Next free cluster index to hand out.
    next_cluster: RwLock<u32>,
    /// Root directory inode number.
    root_inode: InodeNumber,
}

impl ExfatFileSystem {
    /// Create a new exFAT filesystem instance.
    ///
    /// Initializes the root directory entry and an empty FAT / allocation
    /// bitmap. A real I/O layer would read the boot sector, FAT, and
    /// allocation bitmap from the block device instead.
    pub fn new(device_id: u32) -> FsResult<Self> {
        let root_inode = 1;
        let mut entries = BTreeMap::new();
        let mut root = ExfatEntry::new_directory(root_inode, FilePermissions::default_directory());
        root.children.insert("..".to_string(), root_inode);
        entries.insert(root_inode, root);

        Ok(Self {
            device_id,
            entries: RwLock::new(entries),
            next_entry: RwLock::new(2),
            fat: RwLock::new(BTreeMap::new()),
            alloc_bitmap: RwLock::new(vec![false; EXFAT_TOTAL_CLUSTERS as usize]),
            used_clusters: RwLock::new(0),
            next_cluster: RwLock::new(2), // clusters 0/1 reserved
            root_inode,
        })
    }

    /// Block device id backing this volume.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Allocate the next inode number.
    fn allocate_entry(&self) -> InodeNumber {
        let mut next = self.next_entry.write();
        let n = *next;
        *next += 1;
        n
    }

    /// Allocate a FAT chain of `count` clusters and return the first cluster.
    /// Returns 0 when no clusters are needed (count == 0).
    fn allocate_chain(&self, count: u64) -> FsResult<u32> {
        if count == 0 {
            return Ok(0);
        }
        let mut bitmap = self.alloc_bitmap.write();
        let mut fat = self.fat.write();
        let mut used = self.used_clusters.write();
        let mut next = self.next_cluster.write();
        let mut first: Option<u32> = None;
        let mut prev: Option<u32> = None;
        let mut allocated = 0u64;
        while allocated < count {
            // Find a free cluster starting from *next.
            let mut found: Option<u32> = None;
            let start = *next;
            let mut idx = start;
            while idx < bitmap.len() as u32 {
                if !bitmap[idx as usize] {
                    found = Some(idx);
                    break;
                }
                idx += 1;
            }
            let cluster = match found {
                Some(c) => c,
                None => return Err(FsError::NoSpaceLeft),
            };
            bitmap[cluster as usize] = true;
            fat.insert(cluster, FAT_EOC);
            *used += 1;
            if let Some(p) = prev {
                fat.insert(p, cluster);
            } else {
                first = Some(cluster);
            }
            prev = Some(cluster);
            *next = cluster + 1;
            allocated += 1;
        }
        Ok(first.unwrap_or(0))
    }

    /// Free every cluster in the FAT chain starting at `first`.
    fn free_chain(&self, first: u32) {
        if first == 0 {
            return;
        }
        let mut bitmap = self.alloc_bitmap.write();
        let mut fat = self.fat.write();
        let mut used = self.used_clusters.write();
        let mut current = first;
        while current != FAT_EOC && current != FAT_FREE && current != FAT_BAD {
            if (current as usize) < bitmap.len() && bitmap[current as usize] {
                bitmap[current as usize] = false;
                *used = used.saturating_sub(1);
            }
            let nxt = fat.remove(&current).unwrap_or(FAT_EOC);
            current = nxt;
        }
    }

    /// Split a path into non-empty components.
    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Resolve a path to an inode number via the directory child indices.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = Self::split_path(path);
        let entries = self.entries.read();
        let mut current = self.root_inode;
        for component in components {
            let entry = entries.get(&current).ok_or(FsError::NotFound)?;
            if entry.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *entry.children.get(&component).ok_or(FsError::NotFound)?;
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

    /// Whether a directory holds only "." and "..".
    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let entries = self.entries.read();
        let dir = entries.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.children.len() <= 2)
    }
}

impl FileSystem for ExfatFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::ExFat
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let entries = self.entries.read();
        let used_entries = entries.len() as u64;
        let used_clusters = *self.used_clusters.read();
        Ok(FileSystemStats {
            total_blocks: EXFAT_TOTAL_CLUSTERS,
            free_blocks: EXFAT_TOTAL_CLUSTERS.saturating_sub(used_clusters),
            available_blocks: EXFAT_TOTAL_CLUSTERS.saturating_sub(used_clusters),
            total_inodes: EXFAT_MAX_ENTRIES,
            free_inodes: EXFAT_MAX_ENTRIES.saturating_sub(used_entries),
            block_size: EXFAT_CLUSTER_SIZE,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut entries = self.entries.write();
        if entries.len() >= EXFAT_MAX_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.children.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_entry();
        let file = ExfatEntry::new_file(new_inode, permissions);
        parent.children.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        entries.insert(new_inode, file);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(&inode).ok_or(FsError::NotFound)?;
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
        let mut entries = self.entries.write();
        let entry = entries.get_mut(&inode).ok_or(FsError::NotFound)?;
        if entry.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > EXFAT_MAX_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }
        // Grow the FAT chain if the file crosses a new cluster boundary.
        let old_clusters = (entry.content.len() as u64 + EXFAT_CLUSTER_SIZE as u64 - 1)
            / EXFAT_CLUSTER_SIZE as u64;
        let new_clusters = (new_size + EXFAT_CLUSTER_SIZE as u64 - 1) / EXFAT_CLUSTER_SIZE as u64;
        if new_clusters > old_clusters {
            // Free the old chain and allocate a fresh one of the new length.
            // (A real implementation would extend the chain; we rebuild it for
            // the in-memory model.)
            if entry.first_cluster != 0 {
                self.free_chain(entry.first_cluster);
            }
            entry.first_cluster = self.allocate_chain(new_clusters)?;
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
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let entries = self.entries.read();
        let entry = entries.get(&inode).ok_or(FsError::NotFound)?;
        Ok(entry.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(&inode).ok_or(FsError::NotFound)?;
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
        let mut entries = self.entries.write();
        if entries.len() >= EXFAT_MAX_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.children.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_entry();
        let mut dir = ExfatEntry::new_directory(new_inode, permissions);
        dir.children.insert("..".to_string(), parent_inode);
        parent.children.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        entries.insert(new_inode, dir);
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
        let mut entries = self.entries.write();
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.children.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        if let Some(removed) = entries.remove(&dir_inode) {
            if removed.first_cluster != 0 {
                self.free_chain(removed.first_cluster);
            }
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut entries = self.entries.write();
        let file = entries.get(&file_inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.children.remove(&filename);
        parent.metadata.modified = get_current_time();
        if let Some(removed) = entries.remove(&file_inode) {
            if removed.first_cluster != 0 {
                self.free_chain(removed.first_cluster);
            }
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut entries = self.entries.write();
        let dir = entries.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        let entry_list: Vec<(String, InodeNumber)> = dir
            .children
            .iter()
            .map(|(name, &ino)| (name.clone(), ino))
            .collect();
        let mut result = Vec::new();
        for (name, ino) in entry_list {
            if let Some(node) = entries.get(&ino) {
                result.push(DirectoryEntry {
                    name,
                    inode: ino,
                    file_type: node.metadata.file_type,
                });
            }
        }
        Ok(result)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent, old_name) = self.resolve_parent(old_path)?;
        let (new_parent, new_name) = self.resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut entries = self.entries.write();
        let new_parent_node = entries.get(&new_parent).ok_or(FsError::NotFound)?;
        if new_parent_node.children.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent_node = entries.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        old_parent_node.children.remove(&old_name);
        old_parent_node.metadata.modified = get_current_time();
        let new_parent_node = entries.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_parent_node.children.insert(new_name, old_inode);
        new_parent_node.metadata.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut entries = self.entries.write();
        if entries.len() >= EXFAT_MAX_ENTRIES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = entries.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.children.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_entry();
        let link = ExfatEntry::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));
        parent.children.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        entries.insert(new_inode, link);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let entries = self.entries.read();
        let link = entries.get(&link_inode).ok_or(FsError::NotFound)?;
        if link.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        link.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory FAT is always consistent. A real implementation would flush
        // dirty FAT entries and directory entries to the block device here.
        Ok(())
    }
}
