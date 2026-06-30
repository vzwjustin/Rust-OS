//! F2FS mount framework with in-memory write support.
//!
//! Parses the on-disk superblock, resolves inodes via the NAT, and supports
//! root directory listing and file reads through direct data blocks.
//! Write operations use an in-memory overlay that takes precedence over
//! on-disk data, mirroring F2FS log-structured semantics in RAM.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::read_storage_sectors;
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use spin::RwLock;

const F2FS_SUPER_MAGIC: u32 = 0xF2F5_2010;
const F2FS_SB_OFFSET: u64 = 1024;
const NR_DENTRY_IN_BLOCK: usize = 214;
const F2FS_NAME_LEN: usize = 8;

/// On-disk F2FS superblock (subset of fields used for read-only mount).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct F2fsSuperBlock {
    magic: u32,
    major_ver: u16,
    minor_ver: u16,
    log_sectorsize: u32,
    log_sectors_per_block: u32,
    log_blocksize: u32,
    log_blocks_per_seg: u32,
    segs_per_sec: u32,
    secs_per_zone: u32,
    checksum_offset: u32,
    block_count: u64,
    section_count: u32,
    segment_count: u32,
    segment_count_ckpt: u32,
    segment_count_sit: u32,
    segment_count_nat: u32,
    segment_count_ssa: u32,
    segment_count_main: u32,
    segment0_blkaddr: u32,
    cp_blkaddr: u32,
    sit_blkaddr: u32,
    nat_blkaddr: u32,
    ssa_blkaddr: u32,
    main_blkaddr: u32,
    root_ino: u32,
    node_ino: u32,
    meta_ino: u32,
}

/// On-disk F2FS inode (direct fields only).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct F2fsInode {
    i_mode: u16,
    i_advise: u8,
    i_inline: u8,
    i_uid: u32,
    i_gid: u32,
    i_links: u32,
    i_size: u32,
    i_blocks: u64,
    i_atime: u64,
    i_ctime: u64,
    i_mtime: u64,
    i_atime_nsec: u32,
    i_ctime_nsec: u32,
    i_mtime_nsec: u32,
    i_generation: u32,
    i_current_depth: u32,
    i_xattr_nid: u32,
    i_flags: u32,
    i_pino: u32,
    i_namelen: u32,
    i_name: [u8; 255],
    i_addr: [u32; 923],
}

#[derive(Debug, Clone)]
struct F2fsNode {
    inode: InodeNumber,
    nid: u32,
    mode: u16,
    size: u64,
    uid: u32,
    gid: u32,
    links: u32,
    is_dir: bool,
    /// On-disk block addresses (used by the disk read path).
    data_blocks: Vec<u32>,
    rel_path: String,
    /// In-memory overlay data (None = read from disk, Some = use this).
    mem_data: Option<Vec<u8>>,
    /// In-memory directory entries for nodes not yet on disk.
    mem_entries: Option<BTreeMap<String, InodeNumber>>,
    permissions: FilePermissions,
}

/// F2FS filesystem backed by a block device with in-memory write overlay.
#[derive(Debug)]
pub struct F2fsFileSystem {
    device_id: u32,
    sector_base: u64,
    block_size: u32,
    nat_blkaddr: u32,
    root_ino: u32,
    block_count: u64,
    inodes: RwLock<BTreeMap<InodeNumber, F2fsNode>>,
    next_inode: RwLock<u64>,
}

impl F2fsFileSystem {
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let sb = Self::read_superblock(device_id, sector_base)?;
        let block_size = 1u32
            .checked_shl(sb.log_blocksize + 12)
            .ok_or(FsError::InvalidArgument)?;
        if block_size < 1024 || block_size > 65536 {
            return Err(FsError::InvalidArgument);
        }

        let fs = Self {
            device_id,
            sector_base,
            block_size,
            nat_blkaddr: sb.nat_blkaddr,
            root_ino: sb.root_ino,
            block_count: sb.block_count,
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(2),
        };

        let root = fs.read_inode_node(sb.root_ino)?;
        let perm = FilePermissions::from_octal((root.mode & 0o7777) as u16);
        fs.inodes.write().insert(
            1,
            F2fsNode {
                inode: 1,
                nid: sb.root_ino,
                mode: root.mode,
                size: root.size,
                uid: root.uid,
                gid: root.gid,
                links: root.links,
                is_dir: root.is_dir,
                data_blocks: root.data_blocks,
                rel_path: String::new(),
                mem_data: None,
                mem_entries: None,
                permissions: perm,
            },
        );

        Ok(fs)
    }

    fn read_superblock(device_id: u32, sector_base: u64) -> FsResult<F2fsSuperBlock> {
        let mut buf = [0u8; 512];
        let sb_sector = sector_base + F2FS_SB_OFFSET / 512;
        read_storage_sectors(device_id, sb_sector, &mut buf).map_err(|_| FsError::IoError)?;

        let sb_ptr = buf.as_ptr() as *const F2fsSuperBlock;
        let sb = unsafe { core::ptr::read_unaligned(sb_ptr) };
        if sb.magic != F2FS_SUPER_MAGIC {
            return Err(FsError::NotSupported);
        }
        if sb.nat_blkaddr == 0 || sb.root_ino == 0 {
            return Err(FsError::InvalidArgument);
        }
        Ok(sb)
    }

    fn read_block(&self, block_addr: u32) -> FsResult<Vec<u8>> {
        let mut data = vec![0u8; self.block_size as usize];
        let sectors_per_block = self.block_size as u64 / 512;
        let sector = self.sector_base + (block_addr as u64) * sectors_per_block;
        read_storage_sectors(self.device_id, sector, &mut data).map_err(|_| FsError::IoError)?;
        Ok(data)
    }

    fn nat_entry_per_block(&self) -> u32 {
        self.block_size / 9
    }

    fn lookup_nat(&self, nid: u32) -> FsResult<u32> {
        let entries_per = self.nat_entry_per_block();
        if entries_per == 0 {
            return Err(FsError::IoError);
        }
        let block_idx = nid / entries_per;
        let entry_idx = nid % entries_per;
        let nat_block = self.read_block(self.nat_blkaddr + block_idx)?;
        let offset = (entry_idx * 9) as usize;
        if offset + 9 > nat_block.len() {
            return Err(FsError::IoError);
        }
        let block_addr = u32::from_le_bytes(nat_block[offset + 1..offset + 5].try_into().unwrap());
        if block_addr == 0 {
            return Err(FsError::NotFound);
        }
        Ok(block_addr)
    }

    fn read_inode_node(&self, nid: u32) -> FsResult<F2fsNode> {
        let node_block_addr = self.lookup_nat(nid)?;
        let block = self.read_block(node_block_addr)?;
        if block.len() < core::mem::size_of::<F2fsInode>() {
            return Err(FsError::IoError);
        }
        let raw = unsafe { core::ptr::read_unaligned(block.as_ptr() as *const F2fsInode) };
        let mode = raw.i_mode;
        let is_dir = (mode & 0o170000) == 0o040000;
        let mut data_blocks = Vec::new();
        for i in 0..6 {
            let addr = raw.i_addr[i];
            if addr != 0 {
                data_blocks.push(addr);
            }
        }
        Ok(F2fsNode {
            inode: 0,
            nid,
            mode,
            size: raw.i_size as u64,
            uid: raw.i_uid,
            gid: raw.i_gid,
            links: raw.i_links,
            is_dir,
            data_blocks,
            rel_path: String::new(),
            mem_data: None,
            mem_entries: None,
            permissions: FilePermissions::from_octal((mode & 0o7777) as u16),
        })
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<F2fsNode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    /// Allocate a new inode number and insert the node.
    fn alloc_inode(&self, node: F2fsNode, rel_path: &str) -> InodeNumber {
        let mut next = self.next_inode.write();
        let ino = *next;
        *next += 1;
        let mut stored = node;
        stored.inode = ino;
        stored.rel_path = rel_path.to_string();
        self.inodes.write().insert(ino, stored);
        ino
    }

    fn alloc_inode_num(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let ino = *next;
        *next += 1;
        ino
    }

    /// Resolve a path by calling lookup_in_dir for each component.
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        if rel.is_empty() {
            return Ok(1);
        }
        let mut current: InodeNumber = 1;
        for component in rel.split('/').filter(|c| !c.is_empty()) {
            current = self.lookup_in_dir(current, component)?;
        }
        Ok(current)
    }

    fn resolve_parent_path(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        if rel.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let parts: Vec<&str> = rel.split('/').filter(|c| !c.is_empty()).collect();
        if parts.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let filename = parts.last().unwrap().to_string();
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = if parts.len() == 1 {
            1
        } else {
            let parent_path = format!("/{}", parts[..parts.len() - 1].join("/"));
            self.resolve_path(&parent_path)?
        };
        Ok((parent_ino, filename))
    }

    fn lookup_in_dir(&self, dir_inode: InodeNumber, name: &str) -> FsResult<InodeNumber> {
        let dir = self.get_node(dir_inode)?;
        if !dir.is_dir {
            return Err(FsError::NotADirectory);
        }
        // In-memory entries take priority
        if let Some(ref mem_ents) = dir.mem_entries {
            if let Some(&ino) = mem_ents.get(name) {
                return Ok(ino);
            }
        }
        // On-disk entries
        for entry in self.read_dir_entries(&dir)? {
            if entry.name == name {
                return Ok(entry.inode);
            }
        }
        Err(FsError::NotFound)
    }

    fn read_dir_entries(&self, dir: &F2fsNode) -> FsResult<Vec<DirectoryEntry>> {
        let mut out = Vec::new();
        for &block_addr in &dir.data_blocks {
            let block = self.read_block(block_addr)?;
            for i in 0..NR_DENTRY_IN_BLOCK {
                let dentry_off = i * 4;
                if dentry_off + 4 > block.len() {
                    break;
                }
                let ino = u32::from_le_bytes(block[dentry_off..dentry_off + 4].try_into().unwrap());
                if ino == 0 {
                    continue;
                }
                let name_off = NR_DENTRY_IN_BLOCK * 4 + i * F2FS_NAME_LEN;
                if name_off + F2FS_NAME_LEN > block.len() {
                    break;
                }
                let name_bytes = &block[name_off..name_off + F2FS_NAME_LEN];
                let end = name_bytes
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(F2FS_NAME_LEN);
                let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
                if name.is_empty() {
                    continue;
                }
                let child_path = if dir.rel_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", dir.rel_path, name)
                };
                let child_node = self.read_inode_node(ino)?;
                let file_type = if child_node.is_dir {
                    FileType::Directory
                } else {
                    FileType::Regular
                };
                let child_ino = self.alloc_inode(child_node, &child_path);
                out.push(DirectoryEntry {
                    name,
                    inode: child_ino,
                    file_type,
                });
            }
        }
        Ok(out)
    }

    fn read_file_data(&self, node: &F2fsNode, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        // In-memory data takes priority
        if let Some(ref data) = node.mem_data {
            let off = offset as usize;
            if off >= data.len() {
                return Ok(0);
            }
            let avail = data.len() - off;
            let to_copy = core::cmp::min(buffer.len(), avail);
            buffer[..to_copy].copy_from_slice(&data[off..off + to_copy]);
            return Ok(to_copy);
        }
        // On-disk read
        if offset >= node.size {
            return Ok(0);
        }
        let mut copied = 0usize;
        let mut pos = offset;
        let block_size = self.block_size as u64;
        for &block_addr in &node.data_blocks {
            if pos >= block_size {
                pos -= block_size;
                continue;
            }
            let block = self.read_block(block_addr)?;
            let avail = (block_size - pos) as usize;
            let need = buffer.len() - copied;
            let take = core::cmp::min(avail, need);
            let start = pos as usize;
            buffer[copied..copied + take].copy_from_slice(&block[start..start + take]);
            copied += take;
            pos = 0;
            if copied >= buffer.len() || (offset + copied as u64) >= node.size {
                break;
            }
        }
        let max = (node.size - offset) as usize;
        Ok(core::cmp::min(copied, max))
    }

    fn create_node(
        &self,
        path: &str,
        is_dir: bool,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let (parent_ino, filename) = self.resolve_parent_path(path)?;
        let new_ino = self.alloc_inode_num();
        let mut inodes = self.inodes.write();

        let parent = inodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if let Some(ref mem_ents) = parent.mem_entries {
            if mem_ents.contains_key(&filename) {
                return Err(FsError::AlreadyExists);
            }
        }
        let mem_ents = parent.mem_entries.get_or_insert_with(BTreeMap::new);
        mem_ents.insert(filename.clone(), new_ino);

        let parent_rel = parent.rel_path.clone();
        let rel_path = if parent_rel.is_empty() {
            filename
        } else {
            format!("{}/{}", parent_rel, filename)
        };

        let mode: u16 = if is_dir { 0o040755 } else { 0o100644 };
        inodes.insert(
            new_ino,
            F2fsNode {
                inode: new_ino,
                nid: 0,
                mode,
                size: 0,
                uid: 0,
                gid: 0,
                links: if is_dir { 2 } else { 1 },
                is_dir,
                data_blocks: Vec::new(),
                rel_path,
                mem_data: Some(Vec::new()),
                mem_entries: if is_dir { Some(BTreeMap::new()) } else { None },
                permissions,
            },
        );
        Ok(new_ino)
    }
}

impl FileSystem for F2fsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::F2fs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.block_count,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: self.inodes.read().len() as u64,
            free_inodes: 0,
            block_size: self.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.create_node(path, false, permissions)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        self.resolve_path(rel)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        self.read_file_data(&node, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Promote disk-backed node to in-memory overlay before taking write lock
        // so we don't need to call read_block while holding the write lock.
        let existing_data: Option<Vec<u8>> = {
            let inodes = self.inodes.read();
            let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
            if node.is_dir {
                return Err(FsError::IsADirectory);
            }
            if node.mem_data.is_none() {
                // Read existing on-disk content into a Vec
                let mut existing = vec![0u8; node.size as usize];
                let block_size = self.block_size as usize;
                let mut dst_off = 0usize;
                for &block_addr in &node.data_blocks {
                    if dst_off >= existing.len() {
                        break;
                    }
                    let sectors_per_block = self.block_size as u64 / 512;
                    let sector = self.sector_base + (block_addr as u64) * sectors_per_block;
                    let mut blk = vec![0u8; block_size];
                    if read_storage_sectors(self.device_id, sector, &mut blk).is_ok() {
                        let take = core::cmp::min(blk.len(), existing.len() - dst_off);
                        existing[dst_off..dst_off + take].copy_from_slice(&blk[..take]);
                        dst_off += take;
                    }
                }
                Some(existing)
            } else {
                None // Already has in-memory data; will update under write lock
            }
        };

        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        // Install promoted data if needed
        if let Some(existing) = existing_data {
            node.mem_data = Some(existing);
        }
        let data = node.mem_data.as_mut().unwrap();
        let off = offset as usize;
        let end = off + buffer.len();
        if end > data.len() {
            data.resize(end, 0);
        }
        data[off..end].copy_from_slice(buffer);
        node.size = data.len() as u64;
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        let now = get_current_time();
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
            created: now,
            modified: now,
            accessed: now,
            link_count: node.links,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.size = metadata.size;
        node.permissions = metadata.permissions;
        node.uid = metadata.uid;
        node.gid = metadata.gid;
        node.links = metadata.link_count;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        self.create_node(path, true, permissions)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let (parent_ino, filename) = self.resolve_parent_path(path)?;
        let mut inodes = self.inodes.write();
        {
            let node = inodes.get(&ino).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            let mem_empty = node
                .mem_entries
                .as_ref()
                .map(|e| e.is_empty())
                .unwrap_or(true);
            let disk_empty = node.data_blocks.is_empty();
            if !mem_empty || !disk_empty {
                return Err(FsError::DirectoryNotEmpty);
            }
        }
        inodes.remove(&ino);
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            if let Some(ref mut mem_ents) = parent.mem_entries {
                mem_ents.remove(&filename);
            }
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let (parent_ino, filename) = self.resolve_parent_path(path)?;
        let mut inodes = self.inodes.write();
        {
            let node = inodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.is_dir {
                return Err(FsError::IsADirectory);
            }
        }
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            if let Some(ref mut mem_ents) = parent.mem_entries {
                mem_ents.remove(&filename);
            }
        }
        let remove = {
            let node = inodes.get_mut(&ino).ok_or(FsError::NotFound)?;
            node.links = node.links.saturating_sub(1);
            node.links == 0
        };
        if remove {
            inodes.remove(&ino);
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let node = self.get_node(inode)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut out = Vec::new();
        // In-memory entries
        if let Some(ref mem_ents) = node.mem_entries {
            let inodes = self.inodes.read();
            for (name, &child_ino) in mem_ents {
                if let Some(child) = inodes.get(&child_ino) {
                    out.push(DirectoryEntry {
                        name: name.clone(),
                        inode: child_ino,
                        file_type: if child.is_dir {
                            FileType::Directory
                        } else {
                            FileType::Regular
                        },
                    });
                }
            }
        }
        // On-disk entries
        let mut disk_entries = self.read_dir_entries(&node)?;
        out.append(&mut disk_entries);
        Ok(out)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let ino = self.resolve_path(old_path)?;
        let (old_parent_ino, old_name) = self.resolve_parent_path(old_path)?;
        let (new_parent_ino, new_name) = self.resolve_parent_path(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        // Remove from old parent
        if let Some(old_parent) = inodes.get_mut(&old_parent_ino) {
            if let Some(ref mut mem_ents) = old_parent.mem_entries {
                mem_ents.remove(&old_name);
            }
        }
        // Evict destination if it exists
        let victim_ino = inodes
            .get(&new_parent_ino)
            .and_then(|p| p.mem_entries.as_ref())
            .and_then(|e| e.get(&new_name))
            .copied();
        if let Some(v_ino) = victim_ino {
            let remove = if let Some(victim) = inodes.get_mut(&v_ino) {
                victim.links = victim.links.saturating_sub(1);
                victim.links == 0
            } else {
                false
            };
            if remove {
                inodes.remove(&v_ino);
            }
        }
        // Insert into new parent
        if let Some(new_parent) = inodes.get_mut(&new_parent_ino) {
            let mem_ents = new_parent.mem_entries.get_or_insert_with(BTreeMap::new);
            mem_ents.insert(new_name.clone(), ino);
        }
        // Update rel_path
        let new_parent_rel = inodes
            .get(&new_parent_ino)
            .map(|p| p.rel_path.clone())
            .unwrap_or_default();
        if let Some(node) = inodes.get_mut(&ino) {
            node.rel_path = if new_parent_rel.is_empty() {
                new_name
            } else {
                format!("{}/{}", new_parent_rel, new_name)
            };
        }
        Ok(())
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
