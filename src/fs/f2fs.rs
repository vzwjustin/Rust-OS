//! F2FS read-only mount framework.
//!
//! Parses the on-disk superblock, resolves inodes via the NAT, and supports
//! root directory listing and file reads through direct data blocks.

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
    data_blocks: Vec<u32>,
    rel_path: String,
}

/// Read-only F2FS filesystem backed by a block device.
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
        })
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<F2fsNode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

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

    fn resolve_path(&self, rel_path: &str) -> FsResult<InodeNumber> {
        if rel_path.is_empty() {
            return Ok(1);
        }
        let mut current = 1u64;
        for component in rel_path.split('/').filter(|c| !c.is_empty()) {
            current = self.lookup_in_dir(current, component)?;
        }
        Ok(current)
    }

    fn lookup_in_dir(&self, dir_inode: InodeNumber, name: &str) -> FsResult<InodeNumber> {
        let dir = self.get_node(dir_inode)?;
        if !dir.is_dir {
            return Err(FsError::NotADirectory);
        }
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
            if copied >= buffer.len() || (offset as u64 + copied as u64) >= node.size {
                break;
            }
        }
        let max = (node.size - offset) as usize;
        Ok(core::cmp::min(copied, max))
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

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let rel = path.strip_prefix('/').unwrap_or(path);
        self.resolve_path(rel)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        self.read_file_data(&node, offset, buffer)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
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
            permissions: FilePermissions::from_octal((node.mode & 0o7777) as u16),
            uid: node.uid,
            gid: node.gid,
            created: now,
            modified: now,
            accessed: now,
            link_count: node.links,
            device_id: None,
        })
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
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
        let node = self.get_node(inode)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        self.read_dir_entries(&node)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
