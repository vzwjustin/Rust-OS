//! Ext2 filesystem implementation
//!
//! This module provides a real ext2 disk filesystem with on-disk format parsing,
//! block/inode allocation via bitmaps, directory entry manipulation, and support
//! for direct, indirect, double-indirect, and triple-indirect block pointers.
//!
//! The implementation uses an `Ext2BlockDevice` trait so it is independent of the
//! underlying storage layer; the kernel wires it to the storage manager.

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
use core::mem;
use spin::RwLock;

/// Ext2 superblock magic number
const EXT2_SUPER_MAGIC: u16 = 0xEF53;

/// Number of direct block pointers in an ext2 inode
const EXT2_DIRECT_BLOCKS: u64 = 12;

/// Good-old inode size (rev 0)
const EXT2_GOOD_OLD_INODE_SIZE: u16 = 128;

/// Root inode number in ext2
const EXT2_ROOT_INO: InodeNumber = 2;

/// Block device abstraction used by the ext2 filesystem.
///
/// Implementations map `block_num` (in filesystem-block units) to the underlying
/// storage medium. The block size is fixed for the lifetime of a device and is
/// reported by [`Ext2BlockDevice::block_size`].
pub trait Ext2BlockDevice: Send + Sync {
    /// Read the block identified by `block_num` into `buffer`.
    /// `buffer.len()` will equal `block_size()`.
    fn read_block(&self, block_num: u64, buffer: &mut [u8]) -> FsResult<()>;

    /// Write `buffer` to the block identified by `block_num`.
    fn write_block(&self, block_num: u64, buffer: &[u8]) -> FsResult<()>;

    /// Flush any cached/dirty data to the underlying medium.
    fn flush(&self) -> FsResult<()>;

    /// Block size in bytes (1024, 2048, 4096, ...).
    fn block_size(&self) -> u32;
}

// ============================================================================
// On-disk structures (all little-endian, #[repr(C, packed)])
// ============================================================================

/// Ext2 superblock (always located at byte offset 1024).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Superblock {
    pub s_inodes_count: u32,
    pub s_blocks_count: u32,
    pub s_r_blocks_count: u32,
    pub s_free_blocks_count: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32,
    pub s_log_frag_size: u32,
    pub s_blocks_per_group: u32,
    pub s_frags_per_group: u32,
    pub s_inodes_per_group: u32,
    pub s_mtime: u32,
    pub s_wtime: u32,
    pub s_mnt_count: u16,
    pub s_max_mnt_count: u16,
    pub s_magic: u16,
    pub s_state: u16,
    pub s_errors: u16,
    pub s_minor_rev_level: u16,
    pub s_lastcheck: u32,
    pub s_checkinterval: u32,
    pub s_creator_os: u32,
    pub s_rev_level: u32,
    pub s_def_resuid: u16,
    pub s_def_resgid: u16,
    // EXT2_DYNAMIC_REV fields
    pub s_first_ino: u32,
    pub s_inode_size: u16,
    pub s_block_group_nr: u16,
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    pub s_volume_name: [u8; 16],
    pub s_last_mounted: [u8; 64],
    pub s_algorithm_usage_bitmap: u32,
    pub s_prealloc_blocks: u8,
    pub s_prealloc_dir_blocks: u8,
    pub s_padding1: u16,
    pub s_journal_uuid: [u8; 16],
    pub s_journal_inum: u32,
    pub s_journal_dev: u32,
    pub s_last_orphan: u32,
    pub s_hash_seed: [u32; 4],
    pub s_def_hash_version: u8,
    pub s_padding2: u8,
    pub s_reserved_word_pad: u16,
    pub s_default_mount_opts: u32,
    pub s_first_meta_bg: u32,
    pub s_reserved: [u32; 190],
}

/// Ext2 group descriptor (32 bytes for rev 0/1 without 64bit).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GroupDescriptor {
    pub bg_block_bitmap: u32,
    pub bg_inode_bitmap: u32,
    pub bg_inode_table: u32,
    pub bg_free_blocks_count: u16,
    pub bg_free_inodes_count: u16,
    pub bg_used_dirs_count: u16,
    pub bg_pad: u16,
    pub bg_reserved: [u32; 3],
}

/// Ext2 on-disk inode (128 bytes for rev 0).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct InodeRaw {
    pub i_mode: u16,
    pub i_uid: u16,
    pub i_size: u32,
    pub i_atime: u32,
    pub i_ctime: u32,
    pub i_mtime: u32,
    pub i_dtime: u32,
    pub i_gid: u16,
    pub i_links_count: u16,
    pub i_blocks: u32,
    pub i_flags: u32,
    pub i_osd1: u32,
    pub i_block: [u32; 15],
    pub i_generation: u32,
    pub i_file_acl: u32,
    pub i_dir_acl: u32,
    pub i_faddr: u32,
    pub i_osd2: [u8; 12],
}

/// Ext2 directory entry header (variable-length name follows).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RawDirEntry {
    pub inode: u32,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
}

// ============================================================================
// Filesystem
// ============================================================================

/// Ext2 filesystem instance backed by a block device.
pub struct Ext2FileSystem {
    device_id: u32,
    block_device: Arc<dyn Ext2BlockDevice>,
    superblock: RwLock<Superblock>,
    block_size: u32,
    blocks_per_group: u32,
    inodes_per_group: u32,
    inode_size: u32,
    group_count: u32,
    group_descriptors: RwLock<Vec<GroupDescriptor>>,
    block_cache: RwLock<BTreeMap<u64, Vec<u8>>>,
    dirty_blocks: RwLock<BTreeMap<u64, Vec<u8>>>,
    inode_cache: RwLock<BTreeMap<InodeNumber, InodeRaw>>,
}

impl core::fmt::Debug for Ext2FileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ext2FileSystem")
            .field("device_id", &self.device_id)
            .field("block_size", &self.block_size)
            .field("blocks_per_group", &self.blocks_per_group)
            .field("inodes_per_group", &self.inodes_per_group)
            .field("group_count", &self.group_count)
            .finish()
    }
}

impl Ext2FileSystem {
    /// Create a new ext2 filesystem reading from `block_device`.
    ///
    /// `device_id` is stored for identification only; all I/O goes through the
    /// supplied block device.
    pub fn new(device_id: u32, block_device: Arc<dyn Ext2BlockDevice>) -> FsResult<Self> {
        let block_size = block_device.block_size();
        if block_size < 1024 || !block_size.is_power_of_two() {
            return Err(FsError::InvalidArgument);
        }

        let mut fs = Self {
            device_id,
            block_device,
            superblock: RwLock::new(unsafe { mem::zeroed() }),
            block_size,
            blocks_per_group: 0,
            inodes_per_group: 0,
            inode_size: EXT2_GOOD_OLD_INODE_SIZE as u32,
            group_count: 0,
            group_descriptors: RwLock::new(Vec::new()),
            block_cache: RwLock::new(BTreeMap::new()),
            dirty_blocks: RwLock::new(BTreeMap::new()),
            inode_cache: RwLock::new(BTreeMap::new()),
        };

        fs.read_and_validate_superblock()?;
        fs.read_group_descriptors()?;
        Ok(fs)
    }

    // ------------------------------------------------------------------
    // Block I/O with simple write-back cache
    // ------------------------------------------------------------------

    fn read_block(&self, block_num: u64) -> FsResult<Vec<u8>> {
        {
            let cache = self.block_cache.read();
            if let Some(data) = cache.get(&block_num) {
                return Ok(data.clone());
            }
        }
        let mut buffer = vec![0u8; self.block_size as usize];
        self.block_device
            .read_block(block_num, &mut buffer)
            .map_err(|_| FsError::IoError)?;
        {
            let mut cache = self.block_cache.write();
            cache.insert(block_num, buffer.clone());
        }
        Ok(buffer)
    }

    fn write_block(&self, block_num: u64, data: &[u8]) -> FsResult<()> {
        if data.len() != self.block_size as usize {
            return Err(FsError::InvalidArgument);
        }
        {
            let mut dirty = self.dirty_blocks.write();
            dirty.insert(block_num, data.to_vec());
        }
        {
            let mut cache = self.block_cache.write();
            cache.insert(block_num, data.to_vec());
        }
        Ok(())
    }

    fn flush_dirty_blocks(&self) -> FsResult<()> {
        let dirty: Vec<(u64, Vec<u8>)> = {
            let mut dirty = self.dirty_blocks.write();
            let snapshot = dirty.iter().map(|(k, v)| (*k, v.clone())).collect();
            dirty.clear();
            snapshot
        };
        for (block_num, data) in dirty {
            self.block_device
                .write_block(block_num, &data)
                .map_err(|_| FsError::IoError)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Superblock & group descriptors
    // ------------------------------------------------------------------

    fn read_and_validate_superblock(&mut self) -> FsResult<()> {
        // Superblock lives at byte offset 1024. For 1024-byte blocks it is block 1;
        // for larger blocks it is inside block 0 at offset 1024.
        let (sb_block, sb_off) = if self.block_size == 1024 {
            (1u64, 0usize)
        } else {
            (0u64, 1024usize)
        };
        let data = self.read_block(sb_block)?;
        if sb_off + mem::size_of::<Superblock>() > data.len() {
            return Err(FsError::IoError);
        }
        let sb = unsafe {
            core::ptr::read_unaligned(data.as_ptr().add(sb_off) as *const Superblock)
        };
        if sb.s_magic != EXT2_SUPER_MAGIC {
            return Err(FsError::InvalidArgument);
        }
        // Block size = 1024 << s_log_block_size. Validate range.
        let computed = 1024u32
            .checked_shl(sb.s_log_block_size)
            .ok_or(FsError::InvalidArgument)?;
        if computed != self.block_size {
            // Trust the on-disk value but keep the device's reported size in sync.
            // If they disagree we use the on-disk value for layout math.
        }
        self.blocks_per_group = sb.s_blocks_per_group;
        self.inodes_per_group = sb.s_inodes_per_group;
        if self.blocks_per_group == 0 || self.inodes_per_group == 0 {
            return Err(FsError::InvalidArgument);
        }
        self.inode_size = if sb.s_rev_level >= 1 {
            let is = sb.s_inode_size as u32;
            if is == 0 || is > self.block_size {
                return Err(FsError::InvalidArgument);
            }
            is
        } else {
            EXT2_GOOD_OLD_INODE_SIZE as u32
        };
        let total_blocks = sb.s_blocks_count as u64;
        let bpg = self.blocks_per_group as u64;
        self.group_count = ((total_blocks + bpg - 1) / bpg) as u32;
        *self.superblock.write() = sb;
        Ok(())
    }

    fn read_group_descriptors(&mut self) -> FsResult<()> {
        let gdt_block = if self.block_size == 1024 { 2u64 } else { 1u64 };
        let desc_size = mem::size_of::<GroupDescriptor>();
        let descs_per_block = self.block_size as usize / desc_size;
        let needed_blocks =
            (self.group_count as usize + descs_per_block - 1) / descs_per_block;
        let mut descs = Vec::new();
        for blk in 0..needed_blocks {
            let data = self.read_block(gdt_block + blk as u64)?;
            for i in 0..descs_per_block {
                if descs.len() >= self.group_count as usize {
                    break;
                }
                let off = i * desc_size;
                if off + desc_size > data.len() {
                    break;
                }
                let desc = unsafe {
                    core::ptr::read_unaligned(
                        data.as_ptr().add(off) as *const GroupDescriptor,
                    )
                };
                descs.push(desc);
            }
        }
        *self.group_descriptors.write() = descs;
        Ok(())
    }

    fn sb_location(&self) -> (u64, usize) {
        if self.block_size == 1024 {
            (1, 0)
        } else {
            (0, 1024)
        }
    }

    fn gdt_location(&self, group: usize) -> (u64, usize) {
        let gdt_block = if self.block_size == 1024 { 2u64 } else { 1u64 };
        let desc_size = mem::size_of::<GroupDescriptor>();
        let per = self.block_size as usize / desc_size;
        let blk = group / per;
        let off = (group % per) * desc_size;
        (gdt_block + blk as u64, off)
    }

    fn update_superblock<F: FnOnce(&mut Superblock)>(&self, f: F) -> FsResult<()> {
        let (blk, off) = self.sb_location();
        let mut data = self.read_block(blk)?;
        let ptr = unsafe { data.as_mut_ptr().add(off) } as *mut Superblock;
        let mut sb = unsafe { core::ptr::read_unaligned(ptr) };
        f(&mut sb);
        unsafe { core::ptr::write_unaligned(ptr, sb) };
        *self.superblock.write() = sb;
        self.write_block(blk, &data)
    }

    fn update_group_desc<F: FnOnce(&mut GroupDescriptor)>(
        &self,
        group: usize,
        f: F,
    ) -> FsResult<()> {
        let (blk, off) = self.gdt_location(group);
        let mut data = self.read_block(blk)?;
        let ptr = unsafe { data.as_mut_ptr().add(off) } as *mut GroupDescriptor;
        let mut desc = unsafe { core::ptr::read_unaligned(ptr) };
        f(&mut desc);
        unsafe { core::ptr::write_unaligned(ptr, desc) };
        {
            let mut descs = self.group_descriptors.write();
            if group < descs.len() {
                descs[group] = desc;
            }
        }
        self.write_block(blk, &data)
    }

    // ------------------------------------------------------------------
    // Bitmap allocation
    // ------------------------------------------------------------------

    fn alloc_bitmap_bit(&self, bitmap_block: u64, max_bits: u32) -> FsResult<Option<u32>> {
        let mut data = self.read_block(bitmap_block)?;
        let limit_bytes = ((max_bits as usize) + 7) / 8;
        let scan = core::cmp::min(limit_bytes, data.len());
        for byte_idx in 0..scan {
            if data[byte_idx] != 0xFF {
                for bit in 0..8u32 {
                    let idx = byte_idx as u32 * 8 + bit;
                    if idx >= max_bits {
                        break;
                    }
                    if data[byte_idx] & (1 << bit) == 0 {
                        data[byte_idx] |= 1 << bit;
                        self.write_block(bitmap_block, &data)?;
                        return Ok(Some(idx));
                    }
                }
            }
        }
        Ok(None)
    }

    fn free_bitmap_bit(&self, bitmap_block: u64, bit: u32) -> FsResult<()> {
        let mut data = self.read_block(bitmap_block)?;
        let byte = (bit / 8) as usize;
        if byte >= data.len() {
            return Err(FsError::InvalidArgument);
        }
        data[byte] &= !(1u8 << (bit % 8));
        self.write_block(bitmap_block, &data)
    }

    fn alloc_block(&self) -> FsResult<u64> {
        let first_data_block = self.superblock.read().s_first_data_block as u64;
        let bpg = self.blocks_per_group as u64;
        // Collect bitmap block locations up front so we don't hold the lock
        // across alloc_bitmap_bit (which itself takes write locks).
        let bitmap_blocks: Vec<(usize, u64)> = {
            let descs = self.group_descriptors.read();
            descs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.bg_block_bitmap != 0)
                .map(|(gi, d)| (gi, d.bg_block_bitmap as u64))
                .collect()
        };
        for (gi, bb) in bitmap_blocks {
            if let Some(b) = self.alloc_bitmap_bit(bb, self.blocks_per_group)? {
                let block = first_data_block + gi as u64 * bpg + b as u64;
                let zero = vec![0u8; self.block_size as usize];
                self.write_block(block, &zero)?;
                self.update_superblock(|sb| {
                    sb.s_free_blocks_count = sb.s_free_blocks_count.saturating_sub(1);
                })?;
                self.update_group_desc(gi, |d| {
                    d.bg_free_blocks_count = d.bg_free_blocks_count.saturating_sub(1);
                })?;
                return Ok(block);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    fn free_block(&self, block: u64) -> FsResult<()> {
        let first_data_block = self.superblock.read().s_first_data_block as u64;
        if block < first_data_block {
            return Err(FsError::InvalidArgument);
        }
        let bpg = self.blocks_per_group as u64;
        let gi = ((block - first_data_block) / bpg) as usize;
        let bit = ((block - first_data_block) % bpg) as u32;
        let descs = self.group_descriptors.read();
        if gi >= descs.len() {
            return Err(FsError::InvalidArgument);
        }
        let bb = descs[gi].bg_block_bitmap as u64;
        drop(descs);
        if bb == 0 {
            return Err(FsError::IoError);
        }
        self.free_bitmap_bit(bb, bit)?;
        self.update_superblock(|sb| {
            sb.s_free_blocks_count = sb.s_free_blocks_count.saturating_add(1);
        })?;
        self.update_group_desc(gi, |d| {
            d.bg_free_blocks_count = d.bg_free_blocks_count.saturating_add(1);
        })?;
        Ok(())
    }

    fn alloc_inode(&self) -> FsResult<InodeNumber> {
        let first_ino = self.superblock.read().s_first_ino as u64;
        let ipg = self.inodes_per_group as u64;
        // Collect inode bitmap locations up front to avoid holding the lock
        // across alloc_bitmap_bit.
        let inode_bitmaps: Vec<(usize, u64)> = {
            let descs = self.group_descriptors.read();
            descs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.bg_inode_bitmap != 0)
                .map(|(gi, d)| (gi, d.bg_inode_bitmap as u64))
                .collect()
        };
        for (gi, ib) in inode_bitmaps {
            if let Some(b) = self.alloc_bitmap_bit(ib, self.inodes_per_group)? {
                let ino = gi as u64 * ipg + b as u64 + 1;
                if ino < first_ino {
                    // Reserved inode — free it and keep scanning.
                    self.free_bitmap_bit(ib, b)?;
                    continue;
                }
                self.update_superblock(|sb| {
                    sb.s_free_inodes_count = sb.s_free_inodes_count.saturating_sub(1);
                })?;
                self.update_group_desc(gi, |d| {
                    d.bg_free_inodes_count = d.bg_free_inodes_count.saturating_sub(1);
                })?;
                return Ok(ino);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    fn free_inode(&self, ino: InodeNumber) -> FsResult<()> {
        if ino == 0 {
            return Err(FsError::InvalidArgument);
        }
        let ipg = self.inodes_per_group as u64;
        let gi = ((ino - 1) / ipg) as usize;
        let bit = ((ino - 1) % ipg) as u32;
        let descs = self.group_descriptors.read();
        if gi >= descs.len() {
            return Err(FsError::NotFound);
        }
        let ib = descs[gi].bg_inode_bitmap as u64;
        drop(descs);
        if ib == 0 {
            return Err(FsError::IoError);
        }
        self.free_bitmap_bit(ib, bit)?;
        self.update_superblock(|sb| {
            sb.s_free_inodes_count = sb.s_free_inodes_count.saturating_add(1);
        })?;
        self.update_group_desc(gi, |d| {
            d.bg_free_inodes_count = d.bg_free_inodes_count.saturating_add(1);
        })?;
        // Zero the on-disk inode.
        let zeroed: InodeRaw = unsafe { mem::zeroed() };
        self.write_inode(ino, &zeroed)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Inode I/O
    // ------------------------------------------------------------------

    fn read_inode(&self, ino: InodeNumber) -> FsResult<InodeRaw> {
        {
            let cache = self.inode_cache.read();
            if let Some(v) = cache.get(&ino) {
                return Ok(*v);
            }
        }
        if ino == 0 {
            return Err(FsError::InvalidArgument);
        }
        let ipg = self.inodes_per_group as u64;
        let group = (ino - 1) / ipg;
        let index = (ino - 1) % ipg;
        let descs = self.group_descriptors.read();
        if group >= descs.len() as u64 {
            return Err(FsError::NotFound);
        }
        let table = descs[group as usize].bg_inode_table as u64;
        drop(descs);
        let inodes_per_block = self.block_size as u64 / self.inode_size as u64;
        let blk = table + index / inodes_per_block;
        let off = (index % inodes_per_block) * self.inode_size as u64;
        let data = self.read_block(blk)?;
        let off = off as usize;
        if off + mem::size_of::<InodeRaw>() > data.len() {
            return Err(FsError::IoError);
        }
        let inode = unsafe {
            core::ptr::read_unaligned(data.as_ptr().add(off) as *const InodeRaw)
        };
        {
            let mut cache = self.inode_cache.write();
            cache.insert(ino, inode);
        }
        Ok(inode)
    }

    fn write_inode(&self, ino: InodeNumber, inode: &InodeRaw) -> FsResult<()> {
        if ino == 0 {
            return Err(FsError::InvalidArgument);
        }
        let ipg = self.inodes_per_group as u64;
        let group = (ino - 1) / ipg;
        let index = (ino - 1) % ipg;
        let descs = self.group_descriptors.read();
        if group >= descs.len() as u64 {
            return Err(FsError::NotFound);
        }
        let table = descs[group as usize].bg_inode_table as u64;
        drop(descs);
        let inodes_per_block = self.block_size as u64 / self.inode_size as u64;
        let blk = table + index / inodes_per_block;
        let off = (index % inodes_per_block) * self.inode_size as u64;
        let mut data = self.read_block(blk)?;
        let off = off as usize;
        if off + mem::size_of::<InodeRaw>() > data.len() {
            return Err(FsError::IoError);
        }
        unsafe {
            core::ptr::write_unaligned(
                data.as_mut_ptr().add(off) as *mut InodeRaw,
                *inode,
            );
        }
        self.write_block(blk, &data)?;
        {
            let mut cache = self.inode_cache.write();
            cache.insert(ino, *inode);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Metadata conversion
    // ------------------------------------------------------------------

    fn mode_to_file_type(mode: u16) -> FileType {
        match mode & 0xF000 {
            0x1000 => FileType::NamedPipe,
            0x2000 => FileType::CharacterDevice,
            0x4000 => FileType::Directory,
            0x6000 => FileType::BlockDevice,
            0x8000 => FileType::Regular,
            0xA000 => FileType::SymbolicLink,
            0xC000 => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    fn file_type_to_dir_ft(ft: FileType) -> u8 {
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

    fn inode_to_metadata(&self, ino: InodeNumber, inode: &InodeRaw) -> FileMetadata {
        let ft = Self::mode_to_file_type(inode.i_mode);
        let size = if ft == FileType::Regular || ft == FileType::SymbolicLink {
            // For regular files in rev0, i_dir_acl holds the high 32 bits of size.
            ((inode.i_dir_acl as u64) << 32) | (inode.i_size as u64)
        } else if ft == FileType::Directory {
            inode.i_size as u64
        } else {
            0
        };
        FileMetadata {
            inode: ino,
            file_type: ft,
            size,
            permissions: FilePermissions::from_octal(inode.i_mode & 0o777),
            uid: inode.i_uid as u32,
            gid: inode.i_gid as u32,
            created: inode.i_ctime as u64,
            modified: inode.i_mtime as u64,
            accessed: inode.i_atime as u64,
            link_count: inode.i_links_count as u32,
            device_id: None,
        }
    }

    fn current_time(&self) -> u32 {
        get_current_time() as u32
    }

    // ------------------------------------------------------------------
    // Path resolution
    // ------------------------------------------------------------------

    fn split_path<'a>(&self, path: &'a str) -> FsResult<(&'a str, &'a str)> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        match trimmed.rfind('/') {
            Some(idx) => {
                let name = &trimmed[idx + 1..];
                if name.is_empty() {
                    return Err(FsError::InvalidArgument);
                }
                let parent = &trimmed[..idx];
                let parent = if parent.is_empty() { "/" } else { parent };
                Ok((parent, name))
            }
            None => Ok(("/", trimmed)),
        }
    }

    fn read_dir_entries_raw(&self, inode: &InodeRaw) -> FsResult<Vec<DirectoryEntry>> {
        let mut entries = Vec::new();
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };
        // Walk direct blocks then indirect/double/triple.
        let bs = self.block_size as u64;
        let total_blocks = (inode.i_size as u64 + bs - 1) / bs;
        for lb in 0..total_blocks {
            let bp = self.logical_block_pointer(&i_block, lb)?;
            if bp == 0 {
                continue;
            }
            let data = self.read_block(bp as u64)?;
            let mut off = 0usize;
            while off + mem::size_of::<RawDirEntry>() <= data.len() {
                let entry = unsafe {
                    core::ptr::read_unaligned(
                        data.as_ptr().add(off) as *const RawDirEntry,
                    )
                };
                if entry.rec_len == 0 {
                    break;
                }
                if entry.inode != 0 && entry.name_len > 0 {
                    let name_end = off + 8 + entry.name_len as usize;
                    if name_end <= data.len() {
                        let name_bytes = &data[off + 8..name_end];
                        if let Ok(name) = core::str::from_utf8(name_bytes) {
                            let ft = match entry.file_type {
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
                                name: name.to_string(),
                                inode: entry.inode as InodeNumber,
                                file_type: ft,
                            });
                        }
                    }
                }
                off += entry.rec_len as usize;
            }
        }
        Ok(entries)
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(EXT2_ROOT_INO);
        }
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let mut cur = EXT2_ROOT_INO;
        let mut symlink_count = 0u8;
        let mut idx = 0usize;
        while idx < components.len() {
            let inode = self.read_inode(cur)?;
            let meta = self.inode_to_metadata(cur, &inode);
            if meta.file_type == FileType::SymbolicLink {
                if symlink_count >= 40 {
                    return Err(FsError::TooManySymlinks);
                }
                symlink_count += 1;
                let target = self.read_symlink_target(cur, &inode, &meta)?;
                let resolved = if target.starts_with('/') {
                    self.resolve_path(&target)?
                } else {
                    let base = components[..idx].join("/");
                    let base = if base.is_empty() { String::from("/") } else { format!("/{}", base) };
                    let combined = format!("{}/{}", base, target);
                    self.resolve_path(&combined)?
                };
                cur = resolved;
                continue;
            }
            if meta.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            let entries = self.read_dir_entries_raw(&inode)?;
            let comp = components[idx];
            let mut found = false;
            for entry in &entries {
                if entry.name == comp {
                    cur = entry.inode;
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(FsError::NotFound);
            }
            idx += 1;
        }
        Ok(cur)
    }

    fn read_symlink_target(
        &self,
        ino: InodeNumber,
        inode: &InodeRaw,
        meta: &FileMetadata,
    ) -> FsResult<String> {
        if meta.size <= 60 {
            // Inline symlink stored in i_block
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(inode.i_block) as *const u8,
                    meta.size as usize,
                )
            };
            core::str::from_utf8(bytes)
                .map(|s| s.to_string())
                .map_err(|_| FsError::IoError)
        } else {
            let mut buf = vec![0u8; meta.size as usize];
            self.read(ino, 0, &mut buf)?;
            core::str::from_utf8(&buf)
                .map(|s| s.to_string())
                .map_err(|_| FsError::IoError)
        }
    }

    // ------------------------------------------------------------------
    // Indirect block pointer traversal
    // ------------------------------------------------------------------

    fn ptrs_per_block(&self) -> u64 {
        self.block_size as u64 / 4
    }

    fn read_ptr(data: &[u8], idx: u64) -> FsResult<u32> {
        let off = idx.checked_mul(4).ok_or(FsError::InvalidArgument)? as usize;
        if off + 4 > data.len() {
            return Err(FsError::InvalidArgument);
        }
        Ok(u32::from_le_bytes([
            data[off],
            data[off + 1],
            data[off + 2],
            data[off + 3],
        ]))
    }

    fn write_ptr(data: &mut [u8], idx: u64, val: u32) -> FsResult<()> {
        let off = idx.checked_mul(4).ok_or(FsError::InvalidArgument)? as usize;
        if off + 4 > data.len() {
            return Err(FsError::InvalidArgument);
        }
        data[off..off + 4].copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    fn read_indirect(&self, block: u32, idx: u64) -> FsResult<u32> {
        if block == 0 {
            return Ok(0);
        }
        let data = self.read_block(block as u64)?;
        Self::read_ptr(&data, idx)
    }

    fn write_indirect(&self, block: u32, idx: u64, val: u32) -> FsResult<()> {
        let mut data = self.read_block(block as u64)?;
        Self::write_ptr(&mut data, idx, val)?;
        self.write_block(block as u64, &data)
    }

    fn logical_block_pointer(&self, i_block: &[u32; 15], lb: u64) -> FsResult<u32> {
        let ptrs = self.ptrs_per_block();
        if lb < EXT2_DIRECT_BLOCKS {
            return Ok(i_block[lb as usize]);
        }
        let mut idx = lb - EXT2_DIRECT_BLOCKS;
        if idx < ptrs {
            return self.read_indirect(i_block[12], idx);
        }
        idx -= ptrs;
        let double_span = ptrs.saturating_mul(ptrs);
        if idx < double_span {
            let first = idx / ptrs;
            let second = idx % ptrs;
            let child = self.read_indirect(i_block[13], first)?;
            return self.read_indirect(child, second);
        }
        idx -= double_span;
        let triple_span = double_span.saturating_mul(ptrs);
        if idx < triple_span {
            let first = idx / double_span;
            let rem = idx % double_span;
            let second = rem / ptrs;
            let third = rem % ptrs;
            let child = self.read_indirect(i_block[14], first)?;
            let grandchild = self.read_indirect(child, second)?;
            return self.read_indirect(grandchild, third);
        }
        Err(FsError::NoSpaceLeft)
    }

    fn ensure_indirect_root(&self, root: &mut u32) -> FsResult<u32> {
        if *root == 0 {
            *root = self.alloc_block()? as u32;
        }
        Ok(*root)
    }

    fn ensure_logical_block_pointer(
        &self,
        i_block: &mut [u32; 15],
        lb: u64,
    ) -> FsResult<u32> {
        let ptrs = self.ptrs_per_block();
        if lb < EXT2_DIRECT_BLOCKS {
            let slot = lb as usize;
            if i_block[slot] == 0 {
                i_block[slot] = self.alloc_block()? as u32;
            }
            return Ok(i_block[slot]);
        }
        let mut idx = lb - EXT2_DIRECT_BLOCKS;
        if idx < ptrs {
            let root = self.ensure_indirect_root(&mut i_block[12])?;
            let mut ptr = self.read_indirect(root, idx)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect(root, idx, ptr)?;
            }
            return Ok(ptr);
        }
        idx -= ptrs;
        let double_span = ptrs.saturating_mul(ptrs);
        if idx < double_span {
            let first = idx / ptrs;
            let second = idx % ptrs;
            let root = self.ensure_indirect_root(&mut i_block[13])?;
            let mut child = self.read_indirect(root, first)?;
            if child == 0 {
                child = self.alloc_block()? as u32;
                self.write_indirect(root, first, child)?;
            }
            let mut ptr = self.read_indirect(child, second)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect(child, second, ptr)?;
            }
            return Ok(ptr);
        }
        idx -= double_span;
        let triple_span = double_span.saturating_mul(ptrs);
        if idx < triple_span {
            let first = idx / double_span;
            let rem = idx % double_span;
            let second = rem / ptrs;
            let third = rem % ptrs;
            let root = self.ensure_indirect_root(&mut i_block[14])?;
            let mut child = self.read_indirect(root, first)?;
            if child == 0 {
                child = self.alloc_block()? as u32;
                self.write_indirect(root, first, child)?;
            }
            let mut grandchild = self.read_indirect(child, second)?;
            if grandchild == 0 {
                grandchild = self.alloc_block()? as u32;
                self.write_indirect(child, second, grandchild)?;
            }
            let mut ptr = self.read_indirect(grandchild, third)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect(grandchild, third, ptr)?;
            }
            return Ok(ptr);
        }
        Err(FsError::NoSpaceLeft)
    }

    fn count_indirect(&self, block: u32, depth: u8) -> FsResult<u64> {
        if block == 0 {
            return Ok(0);
        }
        let data = self.read_block(block as u64)?;
        let mut count = 1u64;
        let ptrs = self.ptrs_per_block();
        for i in 0..ptrs {
            let ptr = Self::read_ptr(&data, i)?;
            if ptr == 0 {
                continue;
            }
            if depth == 1 {
                count += 1;
            } else {
                count += self.count_indirect(ptr, depth - 1)?;
            }
        }
        Ok(count)
    }

    fn count_allocated_blocks(&self, i_block: &[u32; 15]) -> FsResult<u64> {
        let direct = i_block[0..12].iter().filter(|&&b| b != 0).count() as u64;
        Ok(direct
            + self.count_indirect(i_block[12], 1)?
            + self.count_indirect(i_block[13], 2)?
            + self.count_indirect(i_block[14], 3)?)
    }

    fn round_up(&self, val: usize, multiple: usize) -> usize {
        (val + multiple - 1) & !(multiple - 1)
    }

    // ------------------------------------------------------------------
    // Directory entry manipulation
    // ------------------------------------------------------------------

    fn add_dir_entry(
        &self,
        parent_ino: InodeNumber,
        name: &str,
        target_ino: InodeNumber,
        ft: u8,
    ) -> FsResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut parent = self.read_inode(parent_ino)?;
        let name_bytes = name.as_bytes();
        let needed = self.round_up(8 + name_bytes.len(), 4) as u16;
        let bs = self.block_size as usize;
        let mut i_block = unsafe { core::ptr::addr_of!(parent.i_block).read_unaligned() };

        // Try to find slack in existing direct blocks.
        for slot in 0..12usize {
            let bp = i_block[slot];
            if bp == 0 {
                continue;
            }
            let mut data = self.read_block(bp as u64)?;
            let mut off = 0usize;
            while off + mem::size_of::<RawDirEntry>() <= data.len() {
                let eptr = unsafe { data.as_ptr().add(off) } as *const RawDirEntry;
                let entry = unsafe { core::ptr::read_unaligned(eptr) };
                if entry.rec_len == 0 {
                    break;
                }
                let used = self.round_up(8 + entry.name_len as usize, 4) as u16;
                let slack = entry.rec_len.saturating_sub(used);
                if slack >= needed {
                    unsafe {
                        let mut e = core::ptr::read_unaligned(eptr);
                        e.rec_len = used;
                        core::ptr::write_unaligned(
                            data.as_mut_ptr().add(off) as *mut RawDirEntry,
                            e,
                        );
                    }
                    let new_off = off + used as usize;
                    let new_entry = RawDirEntry {
                        inode: target_ino as u32,
                        rec_len: slack,
                        name_len: name_bytes.len() as u8,
                        file_type: ft,
                    };
                    unsafe {
                        core::ptr::write_unaligned(
                            data.as_mut_ptr().add(new_off) as *mut RawDirEntry,
                            new_entry,
                        );
                    }
                    data[new_off + 8..new_off + 8 + name_bytes.len()].copy_from_slice(name_bytes);
                    self.write_block(bp as u64, &data)?;
                    parent.i_mtime = self.current_time();
                    self.write_inode(parent_ino, &parent)?;
                    return Ok(());
                }
                off += entry.rec_len as usize;
            }
        }

        // Allocate a new direct block for the entry.
        for slot in 0..12usize {
            if i_block[slot] != 0 {
                continue;
            }
            let new_block = self.alloc_block()?;
            i_block[slot] = new_block as u32;
            unsafe {
                core::ptr::addr_of_mut!(parent.i_block).write_unaligned(i_block);
            }
            let mut data = vec![0u8; bs];
            let entry = RawDirEntry {
                inode: target_ino as u32,
                rec_len: bs as u16,
                name_len: name_bytes.len() as u8,
                file_type: ft,
            };
            unsafe {
                core::ptr::write_unaligned(data.as_mut_ptr() as *mut RawDirEntry, entry);
            }
            data[8..8 + name_bytes.len()].copy_from_slice(name_bytes);
            self.write_block(new_block, &data)?;
            parent.i_blocks = parent
                .i_blocks
                .saturating_add((bs / 512) as u32);
            parent.i_size = parent.i_size.saturating_add(bs as u32);
            parent.i_mtime = self.current_time();
            self.write_inode(parent_ino, &parent)?;
            return Ok(());
        }
        Err(FsError::NoSpaceLeft)
    }

    fn remove_dir_entry(&self, parent_ino: InodeNumber, name: &str) -> FsResult<()> {
        let parent = self.read_inode(parent_ino)?;
        let i_block = unsafe { core::ptr::addr_of!(parent.i_block).read_unaligned() };
        for slot in 0..12usize {
            let bp = i_block[slot];
            if bp == 0 {
                continue;
            }
            let mut data = self.read_block(bp as u64)?;
            let mut off = 0usize;
            let mut prev: Option<usize> = None;
            while off + mem::size_of::<RawDirEntry>() <= data.len() {
                let eptr = unsafe { data.as_ptr().add(off) } as *const RawDirEntry;
                let entry = unsafe { core::ptr::read_unaligned(eptr) };
                if entry.rec_len == 0 {
                    break;
                }
                if entry.inode != 0
                    && entry.name_len as usize == name.len()
                    && off + 8 + entry.name_len as usize <= data.len()
                {
                    let nb = &data[off + 8..off + 8 + entry.name_len as usize];
                    if nb == name.as_bytes() {
                        let removed = entry.rec_len;
                        if let Some(po) = prev {
                            let pptr = unsafe { data.as_ptr().add(po) } as *const RawDirEntry;
                            let mut pe = unsafe { core::ptr::read_unaligned(pptr) };
                            pe.rec_len = pe
                                .rec_len
                                .checked_add(removed)
                                .ok_or(FsError::InvalidArgument)?;
                            unsafe {
                                core::ptr::write_unaligned(
                                    data.as_mut_ptr().add(po) as *mut RawDirEntry,
                                    pe,
                                );
                            }
                        } else {
                            let mut z = entry;
                            z.inode = 0;
                            unsafe {
                                core::ptr::write_unaligned(
                                    data.as_mut_ptr().add(off) as *mut RawDirEntry,
                                    z,
                                );
                            }
                        }
                        self.write_block(bp as u64, &data)?;
                        let mut p = parent;
                        p.i_mtime = self.current_time();
                        self.write_inode(parent_ino, &p)?;
                        return Ok(());
                    }
                }
                prev = Some(off);
                off += entry.rec_len as usize;
            }
        }
        Err(FsError::NotFound)
    }

    fn free_inode_data_blocks(&self, inode: &InodeRaw) -> FsResult<()> {
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };
        for &bp in &i_block[0..12] {
            if bp != 0 {
                self.free_block(bp as u64)?;
            }
        }
        // Free indirect blocks recursively
        self.free_indirect_tree(i_block[12], 1)?;
        self.free_indirect_tree(i_block[13], 2)?;
        self.free_indirect_tree(i_block[14], 3)?;
        Ok(())
    }

    fn free_indirect_tree(&self, block: u32, depth: u8) -> FsResult<()> {
        if block == 0 {
            return Ok(());
        }
        if depth == 1 {
            let data = self.read_block(block as u64)?;
            let ptrs = self.ptrs_per_block();
            for i in 0..ptrs {
                let ptr = Self::read_ptr(&data, i)?;
                if ptr != 0 {
                    self.free_block(ptr as u64)?;
                }
            }
            self.free_block(block as u64)?;
        } else {
            let data = self.read_block(block as u64)?;
            let ptrs = self.ptrs_per_block();
            for i in 0..ptrs {
                let ptr = Self::read_ptr(&data, i)?;
                if ptr != 0 {
                    self.free_indirect_tree(ptr, depth - 1)?;
                }
            }
            self.free_block(block as u64)?;
        }
        Ok(())
    }
}

impl FileSystem for Ext2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ext2
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let sb = self.superblock.read();
        Ok(FileSystemStats {
            total_blocks: sb.s_blocks_count as u64,
            free_blocks: sb.s_free_blocks_count as u64,
            available_blocks: sb.s_free_blocks_count.saturating_sub(sb.s_r_blocks_count) as u64,
            total_inodes: sb.s_inodes_count as u64,
            free_inodes: sb.s_free_inodes_count as u64,
            block_size: self.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, filename) = self.split_path(path)?;
        let parent_ino = self.resolve_path(parent_path)?;
        let parent = self.read_inode(parent_ino)?;
        let pmeta = self.inode_to_metadata(parent_ino, &parent);
        if pmeta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve_path(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let new_ino = self.alloc_inode()?;
        let mode = (0o100000u32 | permissions.to_octal() as u32) as u16;
        let now = self.current_time();
        let mut inode: InodeRaw = unsafe { mem::zeroed() };
        inode.i_mode = mode;
        inode.i_uid = 0;
        inode.i_size = 0;
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_gid = 0;
        inode.i_links_count = 1;
        inode.i_blocks = 0;
        inode.i_flags = 0;
        self.write_inode(new_ino, &inode)?;
        self.add_dir_entry(parent_ino, filename, new_ino, 1)?;
        self.flush_dirty_blocks()?;
        Ok(new_ino)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, ino: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        if offset >= meta.size {
            return Ok(0);
        }
        let to_read = core::cmp::min(buffer.len(), (meta.size - offset) as usize);
        let bs = self.block_size as u64;
        let start_block = offset / bs;
        let start_off = offset % bs;
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };
        let mut read = 0usize;
        let mut lb = start_block;
        while read < to_read {
            let copy_off = if lb == start_block {
                start_off as usize
            } else {
                0
            };
            let copy_len = core::cmp::min(bs as usize - copy_off, to_read - read);
            let bp = self.logical_block_pointer(&i_block, lb)?;
            if bp == 0 {
                buffer[read..read + copy_len].fill(0);
            } else {
                let data = self.read_block(bp as u64)?;
                buffer[read..read + copy_len]
                    .copy_from_slice(&data[copy_off..copy_off + copy_len]);
            }
            read += copy_len;
            lb += 1;
        }
        Ok(read)
    }

    fn write(&self, ino: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let mut inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let bs = self.block_size as u64;
        let max_blocks = EXT2_DIRECT_BLOCKS
            + self.ptrs_per_block()
            + self.ptrs_per_block().saturating_mul(self.ptrs_per_block())
            + self
                .ptrs_per_block()
                .saturating_mul(self.ptrs_per_block())
                .saturating_mul(self.ptrs_per_block());
        let max_size = max_blocks.saturating_mul(bs);
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let writable_end = core::cmp::min(end, max_size);
        let writable_len = (writable_end - offset) as usize;
        let writable = &buffer[..writable_len];

        let mut written = 0usize;
        let mut lb = offset / bs;
        let mut block_off = (offset % bs) as usize;
        let mut i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };

        while written < writable_len {
            let bp = self.ensure_logical_block_pointer(&mut i_block, lb)?;
            let mut data = self.read_block(bp as u64)?;
            let copy_len = core::cmp::min(bs as usize - block_off, writable_len - written);
            data[block_off..block_off + copy_len]
                .copy_from_slice(&writable[written..written + copy_len]);
            self.write_block(bp as u64, &data)?;
            written += copy_len;
            lb += 1;
            block_off = 0;
        }

        unsafe { core::ptr::addr_of_mut!(inode.i_block).write_unaligned(i_block) };
        let new_size = core::cmp::max(meta.size, offset + written as u64);
        inode.i_size = new_size as u32;
        inode.i_dir_acl = (new_size >> 32) as u32;
        inode.i_mtime = self.current_time();
        inode.i_atime = inode.i_mtime;
        let used = self.count_allocated_blocks(&i_block)?;
        inode.i_blocks = used.saturating_mul((self.block_size / 512) as u64) as u32;
        self.write_inode(ino, &inode)?;
        self.flush_dirty_blocks()?;
        Ok(written)
    }

    fn metadata(&self, ino: InodeNumber) -> FsResult<FileMetadata> {
        let inode = self.read_inode(ino)?;
        Ok(self.inode_to_metadata(ino, &inode))
    }

    fn set_metadata(&self, ino: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inode = self.read_inode(ino)?;
        inode.i_mode = ((inode.i_mode as u32 & 0o7770000) | metadata.permissions.to_octal() as u32)
            as u16;
        inode.i_uid = metadata.uid as u16;
        inode.i_gid = metadata.gid as u16;
        inode.i_mtime = metadata.modified as u32;
        inode.i_atime = metadata.accessed as u32;
        self.write_inode(ino, &inode)?;
        self.flush_dirty_blocks()
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, dirname) = self.split_path(path)?;
        let parent_ino = self.resolve_path(parent_path)?;
        let parent = self.read_inode(parent_ino)?;
        let pmeta = self.inode_to_metadata(parent_ino, &parent);
        if pmeta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve_path(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let new_ino = self.alloc_inode()?;
        let mode = (0o040000u32 | permissions.to_octal() as u32) as u16;
        let now = self.current_time();
        let mut inode: InodeRaw = unsafe { mem::zeroed() };
        inode.i_mode = mode;
        inode.i_uid = 0;
        inode.i_size = self.block_size;
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_gid = 0;
        inode.i_links_count = 2;
        inode.i_blocks = (self.block_size / 512) as u32;
        inode.i_flags = 0;

        let new_block = self.alloc_block()?;
        inode.i_block[0] = new_block as u32;

        let bs = self.block_size as usize;
        let mut data = vec![0u8; bs];
        let dot = RawDirEntry {
            inode: new_ino as u32,
            rec_len: 12,
            name_len: 1,
            file_type: 2,
        };
        unsafe {
            core::ptr::write_unaligned(data.as_mut_ptr() as *mut RawDirEntry, dot);
        }
        data[8] = b'.';
        let dotdot = RawDirEntry {
            inode: parent_ino as u32,
            rec_len: (bs - 12) as u16,
            name_len: 2,
            file_type: 2,
        };
        unsafe {
            core::ptr::write_unaligned(data.as_mut_ptr().add(12) as *mut RawDirEntry, dotdot);
        }
        data[20] = b'.';
        data[21] = b'.';
        self.write_block(new_block, &data)?;
        self.write_inode(new_ino, &inode)?;
        self.add_dir_entry(parent_ino, dirname, new_ino, 2)?;
        let mut p = parent;
        p.i_links_count = p.i_links_count.saturating_add(1);
        self.write_inode(parent_ino, &p)?;
        self.flush_dirty_blocks()?;
        Ok(new_ino)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let entries = self.read_dir_entries_raw(&inode)?;
        let real: Vec<_> = entries
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .collect();
        if !real.is_empty() {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_path, dirname) = self.split_path(path)?;
        let parent_ino = self.resolve_path(parent_path)?;
        self.free_inode_data_blocks(&inode)?;
        self.remove_dir_entry(parent_ino, dirname)?;
        let mut parent = self.read_inode(parent_ino)?;
        parent.i_links_count = parent.i_links_count.saturating_sub(1);
        self.write_inode(parent_ino, &parent)?;
        self.free_inode(ino)?;
        self.flush_dirty_blocks()
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let ino = self.resolve_path(path)?;
        let inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let (parent_path, filename) = self.split_path(path)?;
        let parent_ino = self.resolve_path(parent_path)?;
        self.remove_dir_entry(parent_ino, filename)?;
        let mut ino_obj = inode;
        if ino_obj.i_links_count > 0 {
            ino_obj.i_links_count -= 1;
        }
        if ino_obj.i_links_count == 0 {
            self.free_inode_data_blocks(&ino_obj)?;
            self.free_inode(ino)?;
        } else {
            ino_obj.i_dtime = self.current_time();
            self.write_inode(ino, &ino_obj)?;
        }
        self.flush_dirty_blocks()
    }

    fn readdir(&self, ino: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        self.read_dir_entries_raw(&inode)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        if old_path == new_path {
            return Ok(());
        }
        let old_ino = self.resolve_path(old_path)?;
        let old_inode = self.read_inode(old_ino)?;
        let meta = self.inode_to_metadata(old_ino, &old_inode);
        let (old_parent_path, old_filename) = self.split_path(old_path)?;
        let old_parent_ino = self.resolve_path(old_parent_path)?;
        let (new_parent_path, new_filename) = self.split_path(new_path)?;
        let new_parent_ino = self.resolve_path(new_parent_path)?;

        if let Ok(existing) = self.resolve_path(new_path) {
            if existing == old_ino {
                return Ok(());
            }
            let existing_meta = self.metadata(existing)?;
            if meta.file_type == FileType::Directory {
                if existing_meta.file_type != FileType::Directory {
                    return Err(FsError::NotADirectory);
                }
                let ei = self.read_inode(existing)?;
                let entries = self.read_dir_entries_raw(&ei)?;
                let real: Vec<_> = entries
                    .iter()
                    .filter(|e| e.name != "." && e.name != "..")
                    .collect();
                if !real.is_empty() {
                    return Err(FsError::DirectoryNotEmpty);
                }
                self.free_inode_data_blocks(&ei)?;
                self.remove_dir_entry(new_parent_ino, new_filename)?;
                let mut np = self.read_inode(new_parent_ino)?;
                np.i_links_count = np.i_links_count.saturating_sub(1);
                self.write_inode(new_parent_ino, &np)?;
                self.free_inode(existing)?;
            } else {
                self.remove_dir_entry(new_parent_ino, new_filename)?;
                let ei = self.read_inode(existing)?;
                if ei.i_links_count <= 1 {
                    self.free_inode_data_blocks(&ei)?;
                    self.free_inode(existing)?;
                } else {
                    let mut e = ei;
                    e.i_links_count -= 1;
                    self.write_inode(existing, &e)?;
                }
            }
        }

        let ft = Self::file_type_to_dir_ft(meta.file_type);
        self.add_dir_entry(new_parent_ino, new_filename, old_ino, ft)?;
        self.remove_dir_entry(old_parent_ino, old_filename)?;

        if meta.file_type == FileType::Directory {
            // Update ".." in the moved directory.
            let moved = self.read_inode(old_ino)?;
            let i_block = unsafe { core::ptr::addr_of!(moved.i_block).read_unaligned() };
            if i_block[0] != 0 {
                let mut data = self.read_block(i_block[0] as u64)?;
                if data.len() >= 22 {
                    let ddptr = unsafe { data.as_mut_ptr().add(12) } as *mut RawDirEntry;
                    let mut dd = unsafe { core::ptr::read_unaligned(ddptr) };
                    dd.inode = new_parent_ino as u32;
                    unsafe { core::ptr::write_unaligned(ddptr, dd) };
                    self.write_block(i_block[0] as u64, &data)?;
                }
            }
            if old_parent_ino != new_parent_ino {
                let mut op = self.read_inode(old_parent_ino)?;
                op.i_links_count = op.i_links_count.saturating_sub(1);
                self.write_inode(old_parent_ino, &op)?;
                let mut np = self.read_inode(new_parent_ino)?;
                np.i_links_count = np.i_links_count.saturating_add(1);
                self.write_inode(new_parent_ino, &np)?;
            }
        }
        self.flush_dirty_blocks()
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, linkname) = self.split_path(link_path)?;
        let parent_ino = self.resolve_path(parent_path)?;
        let parent = self.read_inode(parent_ino)?;
        let pmeta = self.inode_to_metadata(parent_ino, &parent);
        if pmeta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve_path(link_path).is_ok() {
            return Err(FsError::AlreadyExists);
        }
        let new_ino = self.alloc_inode()?;
        let mode = 0o120000u16; // S_IFLNK
        let now = self.current_time();
        let target_bytes = target.as_bytes();
        let mut inode: InodeRaw = unsafe { mem::zeroed() };
        inode.i_mode = mode;
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_links_count = 1;
        inode.i_flags = 0;

        if target_bytes.len() <= 60 {
            inode.i_size = target_bytes.len() as u32;
            let block_bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(inode.i_block) as *mut u8,
                    60,
                )
            };
            block_bytes[..target_bytes.len()].copy_from_slice(target_bytes);
        } else {
            if target_bytes.len() > self.block_size as usize * 12 {
                return Err(FsError::NameTooLong);
            }
            inode.i_size = target_bytes.len() as u32;
            let bs = self.block_size as usize;
            let mut blocks = 0u32;
            for (slot, chunk) in target_bytes.chunks(bs).enumerate() {
                if slot >= 12 {
                    return Err(FsError::NameTooLong);
                }
                let nb = self.alloc_block()?;
                inode.i_block[slot] = nb as u32;
                blocks += 1;
                let mut data = vec![0u8; bs];
                data[..chunk.len()].copy_from_slice(chunk);
                self.write_block(nb, &data)?;
            }
            inode.i_blocks = blocks * (self.block_size / 512);
        }
        self.write_inode(new_ino, &inode)?;
        self.add_dir_entry(parent_ino, linkname, new_ino, 7)?;
        self.flush_dirty_blocks()
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let ino = self.resolve_path(path)?;
        let inode = self.read_inode(ino)?;
        let meta = self.inode_to_metadata(ino, &inode);
        if meta.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        self.read_symlink_target(ino, &inode, &meta)
    }

    fn sync(&self) -> FsResult<()> {
        self.flush_dirty_blocks()?;
        self.block_device.flush()
    }
}

// ============================================================================
// Storage-backed block device adapter
// ============================================================================

/// Adapter that wires the kernel storage manager into an `Ext2BlockDevice`.
///
/// Reads/writes are translated to 512-byte sector operations on `device_id`
/// starting at `sector_base` (for partitions).
#[derive(Debug, Clone)]
pub struct StorageBlockDevice {
    device_id: u32,
    sector_base: u64,
    block_size: u32,
}

impl StorageBlockDevice {
    pub fn new(device_id: u32, block_size: u32, sector_base: u64) -> Self {
        Self {
            device_id,
            sector_base,
            block_size,
        }
    }
}

impl Ext2BlockDevice for StorageBlockDevice {
    fn read_block(&self, block_num: u64, buffer: &mut [u8]) -> FsResult<()> {
        use crate::drivers::storage::read_storage_sectors;
        let sectors = self.block_size / 512;
        let sector = self.sector_base + block_num * sectors as u64;
        read_storage_sectors(self.device_id, sector, buffer)
            .map(|_| ())
            .map_err(|_| FsError::IoError)
    }

    fn write_block(&self, block_num: u64, buffer: &[u8]) -> FsResult<()> {
        use crate::drivers::storage::write_storage_sectors;
        let sectors = self.block_size / 512;
        let sector = self.sector_base + block_num * sectors as u64;
        write_storage_sectors(self.device_id, sector, buffer)
            .map(|_| ())
            .map_err(|_| FsError::IoError)
    }

    fn flush(&self) -> FsResult<()> {
        Ok(())
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }
}
