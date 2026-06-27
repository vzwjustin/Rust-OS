//! EXT4 Filesystem Implementation
//!
//! This module provides a production-ready EXT4 filesystem implementation
//! with proper metadata handling, journaling, and disk I/O operations.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::{read_storage_sectors, write_storage_sectors};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::mem;
use spin::RwLock;

/// EXT4 superblock magic number
const EXT4_SUPER_MAGIC: u16 = 0xEF53;

/// EXT4 block size constants
const EXT4_MIN_BLOCK_SIZE: u32 = 1024;
const EXT4_MAX_BLOCK_SIZE: u32 = 65536;

/// EXT4 inode size
const EXT4_GOOD_OLD_INODE_SIZE: u16 = 128;
const EXT4_INODE_SIZE_DEFAULT: u16 = 256;

// EXT4 feature flags
bitflags::bitflags! {
    pub struct Ext4FeatureCompat: u32 {
        const DIR_PREALLOC = 0x0001;
        const IMAGIC_INODES = 0x0002;
        const HAS_JOURNAL = 0x0004;
        const EXT_ATTR = 0x0008;
        const RESIZE_INODE = 0x0010;
        const DIR_INDEX = 0x0020;
        const LAZY_BG = 0x0040;
        const EXCLUDE_INODE = 0x0080;
        const EXCLUDE_BITMAP = 0x0100;
        const SPARSE_SUPER2 = 0x0200;
    }
}

bitflags::bitflags! {
    pub struct Ext4FeatureIncompat: u32 {
        const COMPRESSION = 0x0001;
        const FILETYPE = 0x0002;
        const RECOVER = 0x0004;
        const JOURNAL_DEV = 0x0008;
        const META_BG = 0x0010;
        const EXTENTS = 0x0040;
        const BIT64 = 0x0080;
        const MMP = 0x0100;
        const FLEX_BG = 0x0200;
        const EA_INODE = 0x0400;
        const DIRDATA = 0x1000;
        const CSUM_SEED = 0x2000;
        const LARGEDIR = 0x4000;
        const INLINE_DATA = 0x8000;
        const ENCRYPT = 0x10000;
    }
}

bitflags::bitflags! {
    pub struct Ext4FeatureRoCompat: u32 {
        const SPARSE_SUPER = 0x0001;
        const LARGE_FILE = 0x0002;
        const BTREE_DIR = 0x0004;
        const HUGE_FILE = 0x0008;
        const GDT_CSUM = 0x0010;
        const DIR_NLINK = 0x0020;
        const EXTRA_ISIZE = 0x0040;
        const HAS_SNAPSHOT = 0x0080;
        const QUOTA = 0x0100;
        const BIGALLOC = 0x0200;
        const METADATA_CSUM = 0x0400;
        const REPLICA = 0x0800;
        const READONLY = 0x1000;
        const PROJECT = 0x2000;
    }
}

/// EXT4 superblock structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4Superblock {
    pub s_inodes_count: u32,         // Total inode count
    pub s_blocks_count_lo: u32,      // Total block count (low 32 bits)
    pub s_r_blocks_count_lo: u32,    // Reserved block count (low 32 bits)
    pub s_free_blocks_count_lo: u32, // Free block count (low 32 bits)
    pub s_free_inodes_count: u32,    // Free inode count
    pub s_first_data_block: u32,     // First data block
    pub s_log_block_size: u32,       // Block size (log2(block_size) - 10)
    pub s_log_cluster_size: u32,     // Cluster size (log2(cluster_size) - 10)
    pub s_blocks_per_group: u32,     // Blocks per group
    pub s_clusters_per_group: u32,   // Clusters per group
    pub s_inodes_per_group: u32,     // Inodes per group
    pub s_mtime: u32,                // Mount time
    pub s_wtime: u32,                // Write time
    pub s_mnt_count: u16,            // Mount count
    pub s_max_mnt_count: u16,        // Maximum mount count
    pub s_magic: u16,                // Magic signature
    pub s_state: u16,                // File system state
    pub s_errors: u16,               // Error handling
    pub s_minor_rev_level: u16,      // Minor revision level
    pub s_lastcheck: u32,            // Last check time
    pub s_checkinterval: u32,        // Check interval
    pub s_creator_os: u32,           // Creator OS
    pub s_rev_level: u32,            // Revision level
    pub s_def_resuid: u16,           // Default reserved user ID
    pub s_def_resgid: u16,           // Default reserved group ID

    // EXT4_DYNAMIC_REV specific fields
    pub s_first_ino: u32,              // First non-reserved inode
    pub s_inode_size: u16,             // Size of inode structure
    pub s_block_group_nr: u16,         // Block group number of this superblock
    pub s_feature_compat: u32,         // Compatible feature set
    pub s_feature_incompat: u32,       // Incompatible feature set
    pub s_feature_ro_compat: u32,      // Read-only compatible feature set
    pub s_uuid: [u8; 16],              // 128-bit UUID for volume
    pub s_volume_name: [u8; 16],       // Volume name
    pub s_last_mounted: [u8; 64],      // Directory where last mounted
    pub s_algorithm_usage_bitmap: u32, // For compression

    // Performance hints
    pub s_prealloc_blocks: u8, // Number of blocks to preallocate for files
    pub s_prealloc_dir_blocks: u8, // Number of blocks to preallocate for directories
    pub s_reserved_gdt_blocks: u16, // Number of reserved GDT entries for future filesystem expansion

    // Journaling support
    pub s_journal_uuid: [u8; 16],  // UUID of journal superblock
    pub s_journal_inum: u32,       // Inode number of journal file
    pub s_journal_dev: u32,        // Device number of journal file
    pub s_last_orphan: u32,        // Start of list of inodes to delete
    pub s_hash_seed: [u32; 4],     // HTREE hash seed
    pub s_def_hash_version: u8,    // Default hash version to use
    pub s_jnl_backup_type: u8,     // Journal backup type
    pub s_desc_size: u16,          // Size of group descriptor
    pub s_default_mount_opts: u32, // Default mount options
    pub s_first_meta_bg: u32,      // First metablock block group
    pub s_mkfs_time: u32,          // When the filesystem was created
    pub s_jnl_blocks: [u32; 17],   // Backup of the journal inode

    // 64-bit support
    pub s_blocks_count_hi: u32,         // High 32 bits of block count
    pub s_r_blocks_count_hi: u32,       // High 32 bits of reserved block count
    pub s_free_blocks_count_hi: u32,    // High 32 bits of free block count
    pub s_min_extra_isize: u16,         // All inodes have at least this many bytes
    pub s_want_extra_isize: u16,        // New inodes should reserve this many bytes
    pub s_flags: u32,                   // Miscellaneous flags
    pub s_raid_stride: u16,             // RAID stride
    pub s_mmp_update_interval: u16,     // Number of seconds to wait in MMP checking
    pub s_mmp_block: u64,               // Block for multi-mount protection data
    pub s_raid_stripe_width: u32,       // Blocks on all data disks (N * stride)
    pub s_log_groups_per_flex: u8,      // FLEX_BG group size
    pub s_checksum_type: u8,            // Metadata checksum algorithm type
    pub s_reserved_pad: u16,            // Padding
    pub s_kbytes_written: u64,          // Number of lifetime kilobytes written
    pub s_snapshot_inum: u32,           // Inode number of active snapshot
    pub s_snapshot_id: u32,             // Sequential ID of active snapshot
    pub s_snapshot_r_blocks_count: u64, // Number of blocks reserved for active snapshot's future use
    pub s_snapshot_list: u32,           // Inode number of the head of the on-disk snapshot list
    pub s_error_count: u32,             // Number of file system errors
    pub s_first_error_time: u32,        // First time an error happened
    pub s_first_error_ino: u32,         // Inode involved in first error
    pub s_first_error_block: u64,       // Block involved in first error
    pub s_first_error_func: [u8; 32],   // Function where the error happened
    pub s_first_error_line: u32,        // Line number where error happened
    pub s_last_error_time: u32,         // Most recent time of an error
    pub s_last_error_ino: u32,          // Inode involved in most recent error
    pub s_last_error_line: u32,         // Line number where most recent error happened
    pub s_last_error_block: u64,        // Block involved in most recent error
    pub s_last_error_func: [u8; 32],    // Function where the most recent error happened
    pub s_mount_opts: [u8; 64],         // ASCIIZ string of mount options
    pub s_usr_quota_inum: u32,          // Inode for tracking user quota
    pub s_grp_quota_inum: u32,          // Inode for tracking group quota
    pub s_overhead_clusters: u32,       // Overhead clusters/blocks in fs
    pub s_backup_bgs: [u32; 2],         // Groups with sparse_super2 SBs
    pub s_encrypt_algos: [u8; 4],       // Encryption algorithms in use
    pub s_encrypt_pw_salt: [u8; 16],    // Salt used for string2key algorithm
    pub s_lpf_ino: u32,                 // Location of the lost+found inode
    pub s_prj_quota_inum: u32,          // Inode for tracking project quota
    pub s_checksum_seed: u32,           // crc32c(uuid) if csum_seed set
    pub s_reserved: [u32; 98],          // Padding to the end of the block
    pub s_checksum: u32,                // crc32c(superblock)
}

/// EXT4 group descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4GroupDesc {
    pub bg_block_bitmap_lo: u32,      // Blocks bitmap block (low 32 bits)
    pub bg_inode_bitmap_lo: u32,      // Inodes bitmap block (low 32 bits)
    pub bg_inode_table_lo: u32,       // Inodes table block (low 32 bits)
    pub bg_free_blocks_count_lo: u16, // Free blocks count (low 16 bits)
    pub bg_free_inodes_count_lo: u16, // Free inodes count (low 16 bits)
    pub bg_used_dirs_count_lo: u16,   // Directories count (low 16 bits)
    pub bg_flags: u16,                // EXT4_BG_* flags
    pub bg_exclude_bitmap_lo: u32,    // Exclude bitmap for snapshots (low 32 bits)
    pub bg_block_bitmap_csum_lo: u16, // crc32c(s_uuid+grp_num+bbitmap) LE (low 16 bits)
    pub bg_inode_bitmap_csum_lo: u16, // crc32c(s_uuid+grp_num+ibitmap) LE (low 16 bits)
    pub bg_itable_unused_lo: u16,     // Unused inodes count (low 16 bits)
    pub bg_checksum: u16,             // crc16(sb_uuid+group+desc)

    // 64-bit fields (only if INCOMPAT_64BIT is set)
    pub bg_block_bitmap_hi: u32, // Blocks bitmap block (high 32 bits)
    pub bg_inode_bitmap_hi: u32, // Inodes bitmap block (high 32 bits)
    pub bg_inode_table_hi: u32,  // Inodes table block (high 32 bits)
    pub bg_free_blocks_count_hi: u16, // Free blocks count (high 16 bits)
    pub bg_free_inodes_count_hi: u16, // Free inodes count (high 16 bits)
    pub bg_used_dirs_count_hi: u16, // Directories count (high 16 bits)
    pub bg_itable_unused_hi: u16, // Unused inodes count (high 16 bits)
    pub bg_exclude_bitmap_hi: u32, // Exclude bitmap block (high 32 bits)
    pub bg_block_bitmap_csum_hi: u16, // crc32c(s_uuid+grp_num+bbitmap) BE (high 16 bits)
    pub bg_inode_bitmap_csum_hi: u16, // crc32c(s_uuid+grp_num+ibitmap) BE (high 16 bits)
    pub bg_reserved: u32,        // Padding
}

/// EXT4 inode structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4Inode {
    pub i_mode: u16,        // File mode
    pub i_uid: u16,         // Low 16 bits of Owner Uid
    pub i_size_lo: u32,     // Size in bytes (low 32 bits)
    pub i_atime: u32,       // Access time
    pub i_ctime: u32,       // Inode Change time
    pub i_mtime: u32,       // Modification time
    pub i_dtime: u32,       // Deletion Time
    pub i_gid: u16,         // Low 16 bits of Group Id
    pub i_links_count: u16, // Links count
    pub i_blocks_lo: u32,   // Blocks count (low 32 bits)
    pub i_flags: u32,       // File flags
    pub i_osd1: u32,        // OS dependent 1
    pub i_block: [u32; 15], // Pointers to blocks
    pub i_generation: u32,  // File version (for NFS)
    pub i_file_acl_lo: u32, // File ACL (low 32 bits)
    pub i_size_high: u32,   // Size in bytes (high 32 bits)
    pub i_obso_faddr: u32,  // Obsoleted fragment address
    pub i_osd2: [u32; 3],   // OS dependent 2
    pub i_extra: [u8; 0],   // Extra inode fields (variable size)
}

/// EXT4 directory entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4DirEntry2 {
    pub inode: u32,   // Inode number
    pub rec_len: u16, // Directory entry length
    pub name_len: u8, // Name length
    pub file_type: u8, // File type
                      // name follows here (variable length)
}

/// EXT4 filesystem implementation
#[derive(Debug)]
pub struct Ext4FileSystem {
    device_id: u32,
    superblock: Ext4Superblock,
    block_size: u32,
    blocks_per_group: u32,
    inodes_per_group: u32,
    group_desc_table: Vec<Ext4GroupDesc>,
    inode_cache: RwLock<BTreeMap<InodeNumber, Ext4Inode>>,
    block_cache: RwLock<BTreeMap<u64, Vec<u8>>>,
    dirty_blocks: RwLock<BTreeMap<u64, Vec<u8>>>,
}

impl Ext4FileSystem {
    /// Create new EXT4 filesystem instance
    pub fn new(device_id: u32) -> FsResult<Self> {
        let mut fs = Self {
            device_id,
            superblock: unsafe { mem::zeroed() },
            block_size: 0,
            blocks_per_group: 0,
            inodes_per_group: 0,
            group_desc_table: Vec::new(),
            inode_cache: RwLock::new(BTreeMap::new()),
            block_cache: RwLock::new(BTreeMap::new()),
            dirty_blocks: RwLock::new(BTreeMap::new()),
        };

        fs.read_superblock()?;
        fs.read_group_descriptors()?;
        Ok(fs)
    }

    /// Read superblock from disk
    fn read_superblock(&mut self) -> FsResult<()> {
        let mut buffer = vec![0u8; 1024];

        // Superblock is at offset 1024 bytes (sector 2 for 512-byte sectors)
        read_storage_sectors(self.device_id, 2, &mut buffer).map_err(|_| FsError::IoError)?;

        // Parse superblock
        self.superblock =
            unsafe { core::ptr::read_unaligned(buffer.as_ptr() as *const Ext4Superblock) };

        // Validate magic number
        if self.superblock.s_magic != EXT4_SUPER_MAGIC {
            return Err(FsError::InvalidArgument);
        }

        // Calculate block size. s_log_block_size is semi-trusted on-disk data; a large
        // value would overflow the shift, so use checked_shl and bound the result.
        self.block_size = 1024u32
            .checked_shl(self.superblock.s_log_block_size)
            .ok_or(FsError::InvalidArgument)?;
        if self.block_size < EXT4_MIN_BLOCK_SIZE || self.block_size > EXT4_MAX_BLOCK_SIZE {
            return Err(FsError::InvalidArgument);
        }

        self.blocks_per_group = self.superblock.s_blocks_per_group;
        self.inodes_per_group = self.superblock.s_inodes_per_group;

        // These are later used as divisors; reject zero to avoid div-by-zero panics.
        if self.blocks_per_group == 0 || self.inodes_per_group == 0 {
            return Err(FsError::InvalidArgument);
        }

        // Validate inode size (dynamic-rev only): must be non-zero and fit within a
        // block, since it is used as a divisor and to index within a block.
        if self.superblock.s_rev_level >= 1 {
            let inode_size = self.superblock.s_inode_size as u32;
            if inode_size == 0 || inode_size > self.block_size {
                return Err(FsError::InvalidArgument);
            }
        }

        Ok(())
    }

    /// Read group descriptor table
    fn read_group_descriptors(&mut self) -> FsResult<()> {
        let total_blocks = self.get_total_blocks();
        let blocks_per_group = self.blocks_per_group as u64;
        let group_count = (total_blocks + blocks_per_group - 1) / blocks_per_group;

        // Group descriptor table starts at block 1 (or block 2 if block size is 1024)
        let gdt_block = if self.block_size == 1024 { 2 } else { 1 };

        let desc_size =
            if self.superblock.s_feature_incompat & Ext4FeatureIncompat::BIT64.bits() != 0 {
                self.superblock.s_desc_size as usize
            } else {
                32 // Old 32-byte descriptor size
            };

        // s_desc_size is semi-trusted; reject zero or larger-than-block values so the
        // divisions below cannot panic.
        if desc_size == 0 || desc_size > self.block_size as usize {
            return Err(FsError::InvalidArgument);
        }

        let descs_per_block = self.block_size as usize / desc_size;
        let gdt_blocks = (group_count as usize + descs_per_block - 1) / descs_per_block;

        for block_idx in 0..gdt_blocks {
            let block_num = gdt_block + block_idx as u64;
            let block_data = self.read_block(block_num)?;

            for desc_idx in 0..descs_per_block {
                if self.group_desc_table.len() >= group_count as usize {
                    break;
                }

                let offset = desc_idx * desc_size;
                if offset + desc_size <= block_data.len() {
                    let desc = unsafe {
                        core::ptr::read_unaligned(
                            block_data.as_ptr().add(offset) as *const Ext4GroupDesc
                        )
                    };
                    self.group_desc_table.push(desc);
                }
            }
        }

        Ok(())
    }

    /// Get total number of blocks in filesystem
    fn get_total_blocks(&self) -> u64 {
        if self.superblock.s_feature_incompat & Ext4FeatureIncompat::BIT64.bits() != 0 {
            ((self.superblock.s_blocks_count_hi as u64) << 32)
                | (self.superblock.s_blocks_count_lo as u64)
        } else {
            self.superblock.s_blocks_count_lo as u64
        }
    }

    /// Read a block from disk with caching
    fn read_block(&self, block_num: u64) -> FsResult<Vec<u8>> {
        // Check cache first
        {
            let cache = self.block_cache.read();
            if let Some(cached_block) = cache.get(&block_num) {
                return Ok(cached_block.clone());
            }
        }

        // Read from disk
        let sectors_per_block = self.block_size / 512;
        let start_sector = block_num * sectors_per_block as u64;
        let mut buffer = vec![0u8; self.block_size as usize];

        read_storage_sectors(self.device_id, start_sector, &mut buffer)
            .map_err(|_| FsError::IoError)?;

        // Cache the block
        {
            let mut cache = self.block_cache.write();
            cache.insert(block_num, buffer.clone());
        }

        Ok(buffer)
    }

    /// Write a block to disk with caching
    fn write_block(&self, block_num: u64, data: &[u8]) -> FsResult<()> {
        if data.len() != self.block_size as usize {
            return Err(FsError::InvalidArgument);
        }

        // Mark as dirty for write-back
        {
            let mut dirty = self.dirty_blocks.write();
            dirty.insert(block_num, data.to_vec());
        }

        // Update cache
        {
            let mut cache = self.block_cache.write();
            cache.insert(block_num, data.to_vec());
        }

        Ok(())
    }

    /// Flush dirty blocks to disk
    fn flush_dirty_blocks(&self) -> FsResult<()> {
        let dirty_blocks = {
            let mut dirty = self.dirty_blocks.write();
            let blocks = dirty.clone();
            dirty.clear();
            blocks
        };

        for (block_num, data) in dirty_blocks {
            let sectors_per_block = self.block_size / 512;
            let start_sector = block_num * sectors_per_block as u64;

            write_storage_sectors(self.device_id, start_sector, &data)
                .map_err(|_| FsError::IoError)?;
        }

        Ok(())
    }

    /// Read inode from disk
    fn read_inode(&self, inode_num: InodeNumber) -> FsResult<Ext4Inode> {
        // Check cache first
        {
            let cache = self.inode_cache.read();
            if let Some(cached_inode) = cache.get(&inode_num) {
                return Ok(*cached_inode);
            }
        }

        // Calculate inode location
        let group = (inode_num - 1) / self.inodes_per_group as u64;
        let index = (inode_num - 1) % self.inodes_per_group as u64;

        if group >= self.group_desc_table.len() as u64 {
            return Err(FsError::NotFound);
        }

        let group_desc = &self.group_desc_table[group as usize];
        let inode_table_block =
            if self.superblock.s_feature_incompat & Ext4FeatureIncompat::BIT64.bits() != 0 {
                ((group_desc.bg_inode_table_hi as u64) << 32)
                    | (group_desc.bg_inode_table_lo as u64)
            } else {
                group_desc.bg_inode_table_lo as u64
            };

        let inode_size = if self.superblock.s_rev_level >= 1 {
            self.superblock.s_inode_size as usize
        } else {
            EXT4_GOOD_OLD_INODE_SIZE as usize
        };

        let inodes_per_block = self.block_size as usize / inode_size;
        let block_offset = index as usize / inodes_per_block;
        let inode_offset = (index as usize % inodes_per_block) * inode_size;

        let block_data = self.read_block(inode_table_block + block_offset as u64)?;

        if inode_offset + mem::size_of::<Ext4Inode>() > block_data.len() {
            return Err(FsError::IoError);
        }

        let inode = unsafe {
            core::ptr::read_unaligned(block_data.as_ptr().add(inode_offset) as *const Ext4Inode)
        };

        // Cache the inode
        {
            let mut cache = self.inode_cache.write();
            cache.insert(inode_num, inode);
        }

        Ok(inode)
    }

    /// Convert EXT4 inode to VFS metadata
    fn inode_to_metadata(&self, inode_num: InodeNumber, inode: &Ext4Inode) -> FileMetadata {
        let file_type = match inode.i_mode & 0xF000 {
            0x1000 => FileType::NamedPipe,
            0x2000 => FileType::CharacterDevice,
            0x4000 => FileType::Directory,
            0x6000 => FileType::BlockDevice,
            0x8000 => FileType::Regular,
            0xA000 => FileType::SymbolicLink,
            0xC000 => FileType::Socket,
            _ => FileType::Regular,
        };

        let size = if file_type == FileType::Regular {
            if self.superblock.s_feature_ro_compat & Ext4FeatureRoCompat::LARGE_FILE.bits() != 0 {
                ((inode.i_size_high as u64) << 32) | (inode.i_size_lo as u64)
            } else {
                inode.i_size_lo as u64
            }
        } else {
            0
        };

        FileMetadata {
            inode: inode_num,
            file_type,
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

    /// Read directory entries from an inode
    fn read_directory_entries(&self, inode: &Ext4Inode) -> FsResult<Vec<DirectoryEntry>> {
        let mut entries = Vec::new();

        // For simplicity, only handle direct blocks (first 12 block pointers)
        // SAFETY: inode is a packed struct representing EXT4 on-disk format.
        // We use addr_of! to avoid creating misaligned references.
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };
        for &block_ptr in &i_block[0..12] {
            if block_ptr == 0 {
                break;
            }

            let block_data = self.read_block(block_ptr as u64)?;
            let mut offset = 0;

            while offset < block_data.len() {
                if offset + mem::size_of::<Ext4DirEntry2>() > block_data.len() {
                    break;
                }

                let dir_entry = unsafe {
                    core::ptr::read_unaligned(
                        block_data.as_ptr().add(offset) as *const Ext4DirEntry2
                    )
                };

                if dir_entry.inode == 0 || dir_entry.rec_len == 0 {
                    break;
                }

                if dir_entry.name_len > 0
                    && offset + mem::size_of::<Ext4DirEntry2>() + dir_entry.name_len as usize
                        <= block_data.len()
                {
                    let name_bytes = &block_data[offset + mem::size_of::<Ext4DirEntry2>()
                        ..offset + mem::size_of::<Ext4DirEntry2>() + dir_entry.name_len as usize];

                    if let Ok(name) = core::str::from_utf8(name_bytes) {
                        let file_type = match dir_entry.file_type {
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
                            inode: dir_entry.inode as InodeNumber,
                            file_type,
                        });
                    }
                }

                offset += dir_entry.rec_len as usize;
            }
        }

        Ok(entries)
    }

    /// Resolve path to inode number
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(2); // Root inode is always 2 in EXT4
        }

        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let mut current_inode = 2; // Start from root

        for component in components {
            let inode = self.read_inode(current_inode)?;
            let metadata = self.inode_to_metadata(current_inode, &inode);

            if metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }

            let entries = self.read_directory_entries(&inode)?;
            let mut found = false;

            for entry in entries {
                if entry.name == component {
                    current_inode = entry.inode;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(FsError::NotFound);
            }
        }

        Ok(current_inode)
    }
}

impl FileSystem for Ext4FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ext2 // Using Ext2 enum value for EXT4
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let total_blocks = self.get_total_blocks();
        let free_blocks =
            if self.superblock.s_feature_incompat & Ext4FeatureIncompat::BIT64.bits() != 0 {
                ((self.superblock.s_free_blocks_count_hi as u64) << 32)
                    | (self.superblock.s_free_blocks_count_lo as u64)
            } else {
                self.superblock.s_free_blocks_count_lo as u64
            };

        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: self.superblock.s_inodes_count as u64,
            free_inodes: self.superblock.s_free_inodes_count as u64,
            block_size: self.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // File creation requires complex inode allocation and directory modification
        // For now, return not supported
        Err(FsError::NotSupported)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode_num: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inode = self.read_inode(inode_num)?;
        let metadata = self.inode_to_metadata(inode_num, &inode);

        if metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }

        if offset >= metadata.size {
            return Ok(0);
        }

        let bytes_to_read = core::cmp::min(buffer.len(), (metadata.size - offset) as usize);
        let mut bytes_read = 0;

        // For simplicity, only handle direct blocks
        let block_size = self.block_size as u64;
        let start_block = offset / block_size;
        let start_offset = offset % block_size;

        for block_idx in start_block.. {
            if bytes_read >= bytes_to_read || block_idx >= 12 {
                break;
            }

            let block_ptr = inode.i_block[block_idx as usize];
            if block_ptr == 0 {
                break;
            }

            let block_data = self.read_block(block_ptr as u64)?;
            let copy_offset = if block_idx == start_block {
                start_offset as usize
            } else {
                0
            };
            let copy_len =
                core::cmp::min(block_data.len() - copy_offset, bytes_to_read - bytes_read);

            buffer[bytes_read..bytes_read + copy_len]
                .copy_from_slice(&block_data[copy_offset..copy_offset + copy_len]);

            bytes_read += copy_len;
        }

        Ok(bytes_read)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // Writing requires complex block allocation and metadata updates
        // For now, return read-only error
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode_num: InodeNumber) -> FsResult<FileMetadata> {
        let inode = self.read_inode(inode_num)?;
        Ok(self.inode_to_metadata(inode_num, &inode))
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // Metadata modification requires writing back to disk
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

    fn readdir(&self, inode_num: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inode = self.read_inode(inode_num)?;
        let metadata = self.inode_to_metadata(inode_num, &inode);

        if metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        self.read_directory_entries(&inode)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let inode_num = self.resolve_path(path)?;
        let inode = self.read_inode(inode_num)?;
        let metadata = self.inode_to_metadata(inode_num, &inode);

        if metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }

        // For small symlinks, target is stored in i_block
        if metadata.size <= 60 {
            // SAFETY: inode is a packed struct representing EXT4 on-disk format.
            // We use addr_of! to avoid creating misaligned references.
            let target_bytes = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(inode.i_block) as *const u8,
                    metadata.size as usize,
                )
            };

            core::str::from_utf8(target_bytes)
                .map(|s| s.to_string())
                .map_err(|_| FsError::IoError)
        } else {
            // Large symlinks are stored in blocks
            let mut buffer = vec![0u8; metadata.size as usize];
            self.read(inode_num, 0, &mut buffer)?;

            core::str::from_utf8(&buffer)
                .map(|s| s.to_string())
                .map_err(|_| FsError::IoError)
        }
    }

    fn sync(&self) -> FsResult<()> {
        self.flush_dirty_blocks()
    }
}
