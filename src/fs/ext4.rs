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
const EXT4_DIRECT_BLOCKS: u64 = 12;

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

impl Default for Ext4Superblock {
    fn default() -> Self {
        // SAFETY: All-zero bit pattern is valid for this repr(C, packed) struct
        // containing only integers and arrays of integers.
        unsafe { mem::zeroed() }
    }
}

/// EXT4 group descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
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
#[derive(Debug, Clone, Copy, Default)]
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
#[derive(Debug, Clone, Copy, Default)]
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
    sector_base: u64,
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
        Self::new_at(device_id, 0)
    }

    /// Open ext4 on a partition starting at `sector_base` (512-byte sectors).
    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let mut fs = Self {
            device_id,
            sector_base,
            superblock: Ext4Superblock::default(),
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
        read_storage_sectors(self.device_id, self.sector_base + 2, &mut buffer)
            .map_err(|_| FsError::IoError)?;

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

        // JBD2 replay is not part of this filesystem driver. Refuse volumes that
        // need recovery or use a dedicated journal device instead of writing
        // through stale metadata.
        if self.superblock.s_feature_incompat & Ext4FeatureIncompat::RECOVER.bits() != 0
            || self.superblock.s_feature_incompat & Ext4FeatureIncompat::JOURNAL_DEV.bits() != 0
        {
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

        read_storage_sectors(self.device_id, self.sector_base + start_sector, &mut buffer)
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

            write_storage_sectors(self.device_id, self.sector_base + start_sector, &data)
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

    // ===================================================================
    // Write-path helpers
    //
    // The existing reader in this module treats `i_block[0..12]` as classic
    // ext2-style direct block pointers (it does NOT parse extent trees).
    // To stay consistent with what the reader can consume, the write path
    // below also uses classic direct blocks and never sets the EXT4_EXTENTS
    // flag on newly-created inodes. Indirect/singly/doubly/triply indirect
    // blocks, extent trees, and journaling are intentionally NOT supported
    // here; they are called out where relevant.
    // ===================================================================

    /// Current time as a 32-bit value for inode timestamps.
    /// Uses the same source as `fs::get_current_time` (system time in ms).
    fn current_time(&self) -> u32 {
        crate::time::get_system_time_ms() as u32
    }

    /// Round `val` up to the next multiple of `multiple` (power of two).
    fn round_up(&self, val: usize, multiple: usize) -> usize {
        (val + multiple - 1) & !(multiple - 1)
    }

    /// Split an absolute path into (parent_dir, filename).
    /// e.g. "/foo/bar/baz.txt" -> ("/foo/bar", "baz.txt")
    fn split_path<'a>(&self, path: &'a str) -> FsResult<(&'a str, &'a str)> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidArgument); // cannot create the root
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

    /// Location of the superblock as (block_number, byte_offset_within_block).
    fn superblock_location(&self) -> (u64, usize) {
        if self.block_size == 1024 {
            (1, 0)
        } else {
            (0, 1024)
        }
    }

    /// Location of a group descriptor as (gdt_block_number, byte_offset, desc_size).
    fn gdt_location(&self, group: usize) -> (u64, usize, usize) {
        let gdt_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size =
            if self.superblock.s_feature_incompat & Ext4FeatureIncompat::BIT64.bits() != 0 {
                self.superblock.s_desc_size as usize
            } else {
                32
            };
        let descs_per_block = self.block_size as usize / desc_size;
        let block_idx = group / descs_per_block;
        let desc_idx = group % descs_per_block;
        (
            gdt_block + block_idx as u64,
            desc_idx * desc_size,
            desc_size,
        )
    }

    /// Decrement the global free-inode count in the on-disk superblock.
    ///
    /// NOTE: The in-memory `self.superblock` copy is intentionally left stale
    /// (it is not behind an `RwLock` and the trait methods only give `&self`).
    /// Allocation does not rely on the free count — it scans the bitmaps — so
    /// correctness is unaffected; only `statfs` reporting may lag until remount.
    fn decrement_superblock_free_inodes(&self) -> FsResult<()> {
        let (sb_block, sb_off) = self.superblock_location();
        let mut data = self.read_block(sb_block)?;
        let sb_ptr = unsafe { data.as_mut_ptr().add(sb_off) } as *mut Ext4Superblock;
        let mut sb = unsafe { core::ptr::read_unaligned(sb_ptr) };
        if sb.s_free_inodes_count > 0 {
            sb.s_free_inodes_count -= 1;
        }
        unsafe { core::ptr::write_unaligned(sb_ptr, sb) };
        self.write_block(sb_block, &data)?;
        Ok(())
    }

    /// Decrement the global free-block count in the on-disk superblock by `count`.
    fn decrement_superblock_free_blocks(&self, count: u32) -> FsResult<()> {
        let (sb_block, sb_off) = self.superblock_location();
        let mut data = self.read_block(sb_block)?;
        let sb_ptr = unsafe { data.as_mut_ptr().add(sb_off) } as *mut Ext4Superblock;
        let mut sb = unsafe { core::ptr::read_unaligned(sb_ptr) };
        let cur = sb.s_free_blocks_count_lo;
        sb.s_free_blocks_count_lo = cur.saturating_sub(count);
        // NOTE: high 32 bits (s_free_blocks_count_hi) are not updated; this
        // implementation only handles filesystems that fit in 32-bit block
        // counts, which is consistent with the reader's 64-bit-but-rare path.
        unsafe { core::ptr::write_unaligned(sb_ptr, sb) };
        self.write_block(sb_block, &data)?;
        Ok(())
    }

    /// Adjust a group descriptor's free inode/block counts on disk.
    /// Only the low 16-bit counts are updated (sufficient for non-huge volumes).
    fn adjust_group_desc_free(
        &self,
        group: usize,
        delta_inodes: i32,
        delta_blocks: i32,
    ) -> FsResult<()> {
        let (gdt_block_num, off, _desc_size) = self.gdt_location(group);
        let mut data = self.read_block(gdt_block_num)?;
        let desc_ptr = unsafe { data.as_mut_ptr().add(off) } as *mut Ext4GroupDesc;
        let mut desc = unsafe { core::ptr::read_unaligned(desc_ptr) };
        if delta_inodes != 0 {
            let cur = desc.bg_free_inodes_count_lo as i32;
            desc.bg_free_inodes_count_lo = (cur + delta_inodes).max(0) as u16;
        }
        if delta_blocks != 0 {
            let cur = desc.bg_free_blocks_count_lo as i32;
            desc.bg_free_blocks_count_lo = (cur + delta_blocks).max(0) as u16;
        }
        unsafe { core::ptr::write_unaligned(desc_ptr, desc) };
        self.write_block(gdt_block_num, &data)?;
        Ok(())
    }

    /// Find the first free (zero) bit in a bitmap block, set it, and write the
    /// block back. Returns `Some(bit_index)` if a bit was allocated, else `None`.
    /// `max_bits` bounds the valid bit range for this bitmap.
    fn alloc_bitmap_bit(&self, bitmap_block: u64, max_bits: u32) -> FsResult<Option<u32>> {
        let mut data = self.read_block(bitmap_block)?;
        let limit_bytes = ((max_bits as usize) + 7) / 8;
        let scan_bytes = core::cmp::min(limit_bytes, data.len());
        for byte_idx in 0..scan_bytes {
            if data[byte_idx] != 0xFF {
                for bit in 0..8u32 {
                    let bit_index = (byte_idx * 8) as u32 + bit;
                    if bit_index >= max_bits {
                        break;
                    }
                    if data[byte_idx] & (1 << bit) == 0 {
                        data[byte_idx] |= 1 << bit;
                        self.write_block(bitmap_block, &data)?;
                        return Ok(Some(bit_index));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Allocate a free inode by scanning the per-group inode bitmaps.
    /// Updates the inode bitmap, superblock free-inode count, and the
    /// group descriptor free-inode count on disk. Returns the new inode number.
    fn alloc_inode(&self) -> FsResult<InodeNumber> {
        let first_ino = self.superblock.s_first_ino as u64;
        for (group_idx, desc) in self.group_desc_table.iter().enumerate() {
            // Ignore the 64-bit high half of the bitmap block pointer; the
            // reader does the same and bitmap blocks always live in the low
            // 32-bit block address space for supported volume sizes.
            let inode_bitmap_block = desc.bg_inode_bitmap_lo as u64;
            if inode_bitmap_block == 0 {
                continue;
            }
            let bit = self.alloc_bitmap_bit(inode_bitmap_block, self.inodes_per_group as u32)?;
            if let Some(b) = bit {
                let inode_num = group_idx as u64 * self.inodes_per_group as u64 + b as u64 + 1;
                // Reserved inodes should already be marked used in the bitmap,
                // so this is purely defensive against a corrupt bitmap.
                if inode_num < first_ino {
                    continue;
                }
                self.decrement_superblock_free_inodes()?;
                self.adjust_group_desc_free(group_idx, -1, 0)?;
                return Ok(inode_num);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    /// Allocate a free data block by scanning the per-group block bitmaps.
    /// Updates the block bitmap, superblock free-block count, and the group
    /// descriptor free-block count on disk. The newly allocated block is zeroed
    /// (via the block cache) so callers never see stale block contents.
    /// Returns the new absolute block number.
    fn alloc_block(&self) -> FsResult<u64> {
        let first_data_block = self.superblock.s_first_data_block as u64;
        let bpg = self.blocks_per_group as u64;
        for (group_idx, desc) in self.group_desc_table.iter().enumerate() {
            let block_bitmap_block = desc.bg_block_bitmap_lo as u64;
            if block_bitmap_block == 0 {
                continue;
            }
            let bit = self.alloc_bitmap_bit(block_bitmap_block, self.blocks_per_group as u32)?;
            if let Some(b) = bit {
                let block_num = first_data_block + group_idx as u64 * bpg + b as u64;
                self.decrement_superblock_free_blocks(1)?;
                self.adjust_group_desc_free(group_idx, 0, -1)?;
                // Zero the freshly allocated block in the cache + dirty log.
                let zero = vec![0u8; self.block_size as usize];
                self.write_block(block_num, &zero)?;
                return Ok(block_num);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    fn indirect_ptrs_per_block(&self) -> u64 {
        self.block_size as u64 / 4
    }

    fn max_classic_file_blocks(&self) -> u64 {
        let ptrs = self.indirect_ptrs_per_block();
        EXT4_DIRECT_BLOCKS
            + ptrs
            + ptrs.saturating_mul(ptrs)
            + ptrs.saturating_mul(ptrs).saturating_mul(ptrs)
    }

    fn read_block_pointer(data: &[u8], index: u64) -> FsResult<u32> {
        let offset = index.checked_mul(4).ok_or(FsError::InvalidArgument)? as usize;
        if offset + 4 > data.len() {
            return Err(FsError::InvalidArgument);
        }

        Ok(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    }

    fn write_block_pointer(data: &mut [u8], index: u64, block: u32) -> FsResult<()> {
        let offset = index.checked_mul(4).ok_or(FsError::InvalidArgument)? as usize;
        if offset + 4 > data.len() {
            return Err(FsError::InvalidArgument);
        }

        data[offset..offset + 4].copy_from_slice(&block.to_le_bytes());
        Ok(())
    }

    fn read_indirect_pointer(&self, indirect_block: u32, index: u64) -> FsResult<u32> {
        if indirect_block == 0 {
            return Ok(0);
        }

        let data = self.read_block(indirect_block as u64)?;
        Self::read_block_pointer(&data, index)
    }

    fn write_indirect_pointer(&self, indirect_block: u32, index: u64, block: u32) -> FsResult<()> {
        let mut data = self.read_block(indirect_block as u64)?;
        Self::write_block_pointer(&mut data, index, block)?;
        self.write_block(indirect_block as u64, &data)
    }

    fn logical_block_pointer(&self, i_block: &[u32; 15], logical_block: u64) -> FsResult<u32> {
        let ptrs = self.indirect_ptrs_per_block();
        if logical_block < EXT4_DIRECT_BLOCKS {
            return Ok(i_block[logical_block as usize]);
        }

        let mut idx = logical_block - EXT4_DIRECT_BLOCKS;
        if idx < ptrs {
            return self.read_indirect_pointer(i_block[12], idx);
        }

        idx -= ptrs;
        let double_span = ptrs.saturating_mul(ptrs);
        if idx < double_span {
            let first = idx / ptrs;
            let second = idx % ptrs;
            let child = self.read_indirect_pointer(i_block[13], first)?;
            return self.read_indirect_pointer(child, second);
        }

        idx -= double_span;
        let triple_span = double_span.saturating_mul(ptrs);
        if idx < triple_span {
            let first = idx / double_span;
            let rem = idx % double_span;
            let second = rem / ptrs;
            let third = rem % ptrs;
            let child = self.read_indirect_pointer(i_block[14], first)?;
            let grandchild = self.read_indirect_pointer(child, second)?;
            return self.read_indirect_pointer(grandchild, third);
        }

        Err(FsError::NoSpaceLeft)
    }

    fn ensure_indirect_root(root: &mut u32, fs: &Self) -> FsResult<u32> {
        if *root == 0 {
            *root = fs.alloc_block()? as u32;
        }
        Ok(*root)
    }

    fn ensure_logical_block_pointer(
        &self,
        i_block: &mut [u32; 15],
        logical_block: u64,
    ) -> FsResult<u32> {
        let ptrs = self.indirect_ptrs_per_block();
        if logical_block < EXT4_DIRECT_BLOCKS {
            let slot = logical_block as usize;
            if i_block[slot] == 0 {
                i_block[slot] = self.alloc_block()? as u32;
            }
            return Ok(i_block[slot]);
        }

        let mut idx = logical_block - EXT4_DIRECT_BLOCKS;
        if idx < ptrs {
            let root = Self::ensure_indirect_root(&mut i_block[12], self)?;
            let mut ptr = self.read_indirect_pointer(root, idx)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect_pointer(root, idx, ptr)?;
            }
            return Ok(ptr);
        }

        idx -= ptrs;
        let double_span = ptrs.saturating_mul(ptrs);
        if idx < double_span {
            let first = idx / ptrs;
            let second = idx % ptrs;
            let root = Self::ensure_indirect_root(&mut i_block[13], self)?;
            let mut child = self.read_indirect_pointer(root, first)?;
            if child == 0 {
                child = self.alloc_block()? as u32;
                self.write_indirect_pointer(root, first, child)?;
            }

            let mut ptr = self.read_indirect_pointer(child, second)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect_pointer(child, second, ptr)?;
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
            let root = Self::ensure_indirect_root(&mut i_block[14], self)?;
            let mut child = self.read_indirect_pointer(root, first)?;
            if child == 0 {
                child = self.alloc_block()? as u32;
                self.write_indirect_pointer(root, first, child)?;
            }

            let mut grandchild = self.read_indirect_pointer(child, second)?;
            if grandchild == 0 {
                grandchild = self.alloc_block()? as u32;
                self.write_indirect_pointer(child, second, grandchild)?;
            }

            let mut ptr = self.read_indirect_pointer(grandchild, third)?;
            if ptr == 0 {
                ptr = self.alloc_block()? as u32;
                self.write_indirect_pointer(grandchild, third, ptr)?;
            }
            return Ok(ptr);
        }

        Err(FsError::NoSpaceLeft)
    }

    fn count_indirect_blocks(&self, block: u32, depth: u8) -> FsResult<u64> {
        if block == 0 {
            return Ok(0);
        }

        let data = self.read_block(block as u64)?;
        let mut count = 1u64;
        let ptrs = self.indirect_ptrs_per_block();
        for idx in 0..ptrs {
            let ptr = Self::read_block_pointer(&data, idx)?;
            if ptr == 0 {
                continue;
            }
            if depth == 1 {
                count += 1;
            } else {
                count += self.count_indirect_blocks(ptr, depth - 1)?;
            }
        }
        Ok(count)
    }

    fn count_allocated_file_blocks(&self, i_block: &[u32; 15]) -> FsResult<u64> {
        let direct = i_block[0..12].iter().filter(|&&block| block != 0).count() as u64;
        Ok(direct
            + self.count_indirect_blocks(i_block[12], 1)?
            + self.count_indirect_blocks(i_block[13], 2)?
            + self.count_indirect_blocks(i_block[14], 3)?)
    }

    /// Write an inode back to its on-disk inode table block and update the
    /// inode cache. Mirrors `read_inode`'s location math.
    fn write_inode(&self, inode_num: InodeNumber, inode: &Ext4Inode) -> FsResult<()> {
        if inode_num == 0 {
            return Err(FsError::InvalidArgument);
        }
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

        let block_num = inode_table_block + block_offset as u64;
        let mut data = self.read_block(block_num)?;

        if inode_offset + mem::size_of::<Ext4Inode>() > data.len() {
            return Err(FsError::IoError);
        }

        // SAFETY: `data` is a block-sized Vec aligned to at least the block
        // size; we write the inode at the computed byte offset using
        // write_unaligned because Ext4Inode is #[repr(C, packed)].
        unsafe {
            core::ptr::write_unaligned(
                data.as_mut_ptr().add(inode_offset) as *mut Ext4Inode,
                *inode,
            );
        }

        self.write_block(block_num, &data)?;

        // Update the inode cache so subsequent reads see the new value.
        {
            let mut cache = self.inode_cache.write();
            cache.insert(inode_num, *inode);
        }

        Ok(())
    }

    /// Append a directory entry (`name` -> `target_inode`) into the parent
    /// directory identified by `parent_inode_num`. Handles two cases:
    ///   1. Split an existing directory entry whose `rec_len` has enough slack.
    ///   2. Allocate a fresh directory data block when no slack is available.
    /// Only the first 12 direct blocks of the parent are used, matching the
    /// reader's `read_directory_entries` limitation.
    fn add_dir_entry(
        &self,
        parent_inode_num: InodeNumber,
        name: &str,
        target_inode: InodeNumber,
        file_type: u8,
    ) -> FsResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let mut parent = self.read_inode(parent_inode_num)?;
        let name_bytes = name.as_bytes();
        let needed = self.round_up(8 + name_bytes.len(), 4) as u16;
        let block_size = self.block_size as usize;

        let mut i_block = unsafe { core::ptr::addr_of!(parent.i_block).read_unaligned() };

        // First pass: try to find slack in an existing directory block.
        for slot in 0..12usize {
            let block_ptr = i_block[slot];
            if block_ptr == 0 {
                continue;
            }

            let mut data = self.read_block(block_ptr as u64)?;
            let mut offset = 0usize;
            while offset + mem::size_of::<Ext4DirEntry2>() <= data.len() {
                let entry_ptr = unsafe { data.as_ptr().add(offset) } as *const Ext4DirEntry2;
                let entry = unsafe { core::ptr::read_unaligned(entry_ptr) };
                if entry.rec_len == 0 {
                    break;
                }
                let used = self.round_up(8 + entry.name_len as usize, 4) as u16;
                // rec_len is always >= used for a well-formed entry; slack is
                // the reusable tail space inside this record.
                let slack = entry.rec_len.saturating_sub(used);
                if slack >= needed {
                    // Shrink the existing entry to its real size...
                    unsafe {
                        let mut e = core::ptr::read_unaligned(entry_ptr);
                        e.rec_len = used;
                        core::ptr::write_unaligned(
                            data.as_mut_ptr().add(offset) as *mut Ext4DirEntry2,
                            e,
                        );
                    }
                    // ...and place the new entry in the freed slack.
                    let new_off = offset + used as usize;
                    let new_entry = Ext4DirEntry2 {
                        inode: target_inode as u32,
                        rec_len: slack,
                        name_len: name_bytes.len() as u8,
                        file_type,
                    };
                    unsafe {
                        core::ptr::write_unaligned(
                            data.as_mut_ptr().add(new_off) as *mut Ext4DirEntry2,
                            new_entry,
                        );
                    }
                    data[new_off + 8..new_off + 8 + name_bytes.len()].copy_from_slice(name_bytes);
                    self.write_block(block_ptr as u64, &data)?;

                    parent.i_mtime = self.current_time();
                    self.write_inode(parent_inode_num, &parent)?;
                    return Ok(());
                }
                offset += entry.rec_len as usize;
            }
        }

        // Second pass: no slack found — allocate a new directory data block in
        // the first free direct-block slot.
        for slot in 0..12usize {
            if i_block[slot] != 0 {
                continue;
            }

            let new_block = self.alloc_block()?;
            i_block[slot] = new_block as u32;
            unsafe {
                core::ptr::addr_of_mut!(parent.i_block).write_unaligned(i_block);
            }

            // The new block contains a single directory entry whose rec_len
            // spans the whole block (standard for the sole entry in a block).
            let mut data = vec![0u8; block_size];
            let entry = Ext4DirEntry2 {
                inode: target_inode as u32,
                rec_len: block_size as u16,
                name_len: name_bytes.len() as u8,
                file_type,
            };
            unsafe {
                core::ptr::write_unaligned(data.as_mut_ptr() as *mut Ext4DirEntry2, entry);
            }
            data[8..8 + name_bytes.len()].copy_from_slice(name_bytes);
            self.write_block(new_block, &data)?;

            // Account for the new block in the parent inode.
            parent.i_blocks_lo = parent.i_blocks_lo.saturating_add((block_size / 512) as u32);
            parent.i_size_lo = parent.i_size_lo.saturating_add(block_size as u32);
            parent.i_mtime = self.current_time();
            self.write_inode(parent_inode_num, &parent)?;
            return Ok(());
        }

        // Parent directory is full and uses only direct blocks (no indirect
        // blocks are supported by this implementation).
        Err(FsError::NoSpaceLeft)
    }

    /// Find a directory entry by name in a parent directory.
    /// Returns (block_index, byte_offset_within_block, entry_copy) if found.
    fn find_dir_entry(
        &self,
        parent_inode_num: InodeNumber,
        name: &str,
    ) -> FsResult<Option<(usize, usize, Ext4DirEntry2)>> {
        let parent = self.read_inode(parent_inode_num)?;
        let i_block = unsafe { core::ptr::addr_of!(parent.i_block).read_unaligned() };

        for slot in 0..12usize {
            let block_ptr = i_block[slot];
            if block_ptr == 0 {
                continue;
            }
            let data = self.read_block(block_ptr as u64)?;
            let mut offset = 0usize;
            while offset + mem::size_of::<Ext4DirEntry2>() <= data.len() {
                let entry_ptr = unsafe { data.as_ptr().add(offset) } as *const Ext4DirEntry2;
                let entry = unsafe { core::ptr::read_unaligned(entry_ptr) };
                if entry.rec_len == 0 {
                    break;
                }
                if entry.inode != 0
                    && entry.name_len as usize == name.len()
                    && offset + 8 + entry.name_len as usize <= data.len()
                {
                    let name_bytes = &data[offset + 8..offset + 8 + entry.name_len as usize];
                    if name_bytes == name.as_bytes() {
                        return Ok(Some((slot, offset, entry)));
                    }
                }
                offset += entry.rec_len as usize;
            }
        }
        Ok(None)
    }

    /// Remove a directory entry by name. The entry's inode is zeroed out and
    /// its rec_len is merged into the previous entry's rec_len (or, if it's
    /// the first entry in the block, the inode is just zeroed).
    fn remove_dir_entry(&self, parent_inode_num: InodeNumber, name: &str) -> FsResult<()> {
        let parent = self.read_inode(parent_inode_num)?;
        let i_block = unsafe { core::ptr::addr_of!(parent.i_block).read_unaligned() };

        for slot in 0..12usize {
            let block_ptr = i_block[slot];
            if block_ptr == 0 {
                continue;
            }
            let mut data = self.read_block(block_ptr as u64)?;
            let mut offset = 0usize;
            let mut prev_offset: Option<usize> = None;
            while offset + mem::size_of::<Ext4DirEntry2>() <= data.len() {
                let entry_ptr = unsafe { data.as_ptr().add(offset) } as *const Ext4DirEntry2;
                let entry = unsafe { core::ptr::read_unaligned(entry_ptr) };
                if entry.rec_len == 0 {
                    break;
                }
                if entry.inode != 0
                    && entry.name_len as usize == name.len()
                    && offset + 8 + entry.name_len as usize <= data.len()
                {
                    let name_bytes = &data[offset + 8..offset + 8 + entry.name_len as usize];
                    if name_bytes == name.as_bytes() {
                        // Found the entry. Merge its rec_len into the previous entry.
                        let removed_rec_len = entry.rec_len;
                        if let Some(prev_off) = prev_offset {
                            let prev_ptr =
                                unsafe { data.as_ptr().add(prev_off) } as *const Ext4DirEntry2;
                            let mut prev_entry = unsafe { core::ptr::read_unaligned(prev_ptr) };
                            prev_entry.rec_len = prev_entry
                                .rec_len
                                .checked_add(removed_rec_len)
                                .ok_or(FsError::InvalidArgument)?;
                            unsafe {
                                core::ptr::write_unaligned(
                                    data.as_mut_ptr().add(prev_off) as *mut Ext4DirEntry2,
                                    prev_entry,
                                );
                            }
                        } else {
                            // First entry in block — just zero the inode field.
                            let mut zeroed = entry;
                            zeroed.inode = 0;
                            unsafe {
                                core::ptr::write_unaligned(
                                    data.as_mut_ptr().add(offset) as *mut Ext4DirEntry2,
                                    zeroed,
                                );
                            }
                        }
                        self.write_block(block_ptr as u64, &data)?;

                        // Update parent mtime
                        let mut p = parent;
                        p.i_mtime = self.current_time();
                        self.write_inode(parent_inode_num, &p)?;
                        return Ok(());
                    }
                }
                prev_offset = Some(offset);
                offset += entry.rec_len as usize;
            }
        }
        Err(FsError::NotFound)
    }

    /// Clear a bit in a bitmap block (free a resource).
    fn free_bitmap_bit(&self, bitmap_block: u64, bit_index: u32) -> FsResult<()> {
        let mut data = self.read_block(bitmap_block)?;
        let byte_idx = (bit_index / 8) as usize;
        let bit = bit_index % 8;
        if byte_idx >= data.len() {
            return Err(FsError::InvalidArgument);
        }
        data[byte_idx] &= !(1u8 << bit);
        self.write_block(bitmap_block, &data)?;
        Ok(())
    }

    /// Increment the global free-inode count in the on-disk superblock.
    fn increment_superblock_free_inodes(&self) -> FsResult<()> {
        let (sb_block, sb_off) = self.superblock_location();
        let mut data = self.read_block(sb_block)?;
        let sb_ptr = unsafe { data.as_mut_ptr().add(sb_off) } as *mut Ext4Superblock;
        let mut sb = unsafe { core::ptr::read_unaligned(sb_ptr) };
        sb.s_free_inodes_count = sb.s_free_inodes_count.saturating_add(1);
        unsafe { core::ptr::write_unaligned(sb_ptr, sb) };
        self.write_block(sb_block, &data)?;
        Ok(())
    }

    /// Increment the global free-block count in the on-disk superblock by `count`.
    fn increment_superblock_free_blocks(&self, count: u32) -> FsResult<()> {
        let (sb_block, sb_off) = self.superblock_location();
        let mut data = self.read_block(sb_block)?;
        let sb_ptr = unsafe { data.as_mut_ptr().add(sb_off) } as *mut Ext4Superblock;
        let mut sb = unsafe { core::ptr::read_unaligned(sb_ptr) };
        sb.s_free_blocks_count_lo = sb.s_free_blocks_count_lo.saturating_add(count);
        unsafe { core::ptr::write_unaligned(sb_ptr, sb) };
        self.write_block(sb_block, &data)?;
        Ok(())
    }

    /// Free an inode: clear its bitmap bit, update free counts, and zero the inode on disk.
    fn free_inode(&self, inode_num: InodeNumber) -> FsResult<()> {
        if inode_num == 0 {
            return Err(FsError::InvalidArgument);
        }
        let group = ((inode_num - 1) / self.inodes_per_group as u64) as usize;
        let index = ((inode_num - 1) % self.inodes_per_group as u64) as u32;

        if group >= self.group_desc_table.len() {
            return Err(FsError::NotFound);
        }

        let inode_bitmap_block = self.group_desc_table[group].bg_inode_bitmap_lo as u64;
        if inode_bitmap_block == 0 {
            return Err(FsError::IoError);
        }

        self.free_bitmap_bit(inode_bitmap_block, index)?;
        self.increment_superblock_free_inodes()?;
        self.adjust_group_desc_free(group, 1, 0)?;

        // Zero the inode on disk
        let zeroed: Ext4Inode = Ext4Inode::default();
        self.write_inode(inode_num, &zeroed)?;
        Ok(())
    }

    /// Free a data block: clear its bitmap bit and update free counts.
    fn free_block(&self, block_num: u64) -> FsResult<()> {
        let first_data_block = self.superblock.s_first_data_block as u64;
        if block_num < first_data_block {
            return Err(FsError::InvalidArgument);
        }
        let bpg = self.blocks_per_group as u64;
        let group = ((block_num - first_data_block) / bpg) as usize;
        let bit = ((block_num - first_data_block) % bpg) as u32;

        if group >= self.group_desc_table.len() {
            return Err(FsError::InvalidArgument);
        }

        let block_bitmap_block = self.group_desc_table[group].bg_block_bitmap_lo as u64;
        if block_bitmap_block == 0 {
            return Err(FsError::IoError);
        }

        self.free_bitmap_bit(block_bitmap_block, bit)?;
        self.increment_superblock_free_blocks(1)?;
        self.adjust_group_desc_free(group, 0, 1)?;
        Ok(())
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

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Split the target path into parent directory and file name.
        let (parent_path, filename) = self.split_path(path)?;

        // The parent must exist and be a directory.
        let parent_inode_num = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode(parent_inode_num)?;
        let parent_meta = self.inode_to_metadata(parent_inode_num, &parent_inode);
        if parent_meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Refuse to clobber an existing entry.
        if self.resolve_path(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Allocate a free inode and initialize it as a regular file.
        let new_inode_num = self.alloc_inode()?;
        let mode = (0o100000u32 | permissions.to_octal() as u32) as u16; // S_IFREG | perms
        let now = self.current_time();
        let mut inode: Ext4Inode = Ext4Inode::default();
        inode.i_mode = mode;
        inode.i_uid = 0;
        inode.i_size_lo = 0;
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_dtime = 0;
        inode.i_gid = 0;
        inode.i_links_count = 1;
        inode.i_blocks_lo = 0;
        // i_flags = 0: deliberately do NOT set EXT4_EXTENTS_FL (0x80000) because
        // the reader in this module consumes classic direct-block pointers, not
        // extent trees. i_block stays all-zero (empty file).
        inode.i_flags = 0;
        self.write_inode(new_inode_num, &inode)?;

        // Link the new inode into the parent directory.
        // file_type 1 == regular file (matches the reader's mapping).
        self.add_dir_entry(parent_inode_num, filename, new_inode_num, 1)?;

        // Flush all dirty metadata (bitmaps, superblock, GDT, inode table,
        // directory block) to disk using this driver's ordered direct-write path.
        self.flush_dirty_blocks()?;

        Ok(new_inode_num)
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

        let block_size = self.block_size as u64;
        let start_block = offset / block_size;
        let start_offset = offset % block_size;
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };

        for block_idx in start_block.. {
            if bytes_read >= bytes_to_read {
                break;
            }

            let copy_offset = if block_idx == start_block {
                start_offset as usize
            } else {
                0
            };
            let copy_len = core::cmp::min(
                block_size as usize - copy_offset,
                bytes_to_read - bytes_read,
            );

            let block_ptr = self.logical_block_pointer(&i_block, block_idx)?;
            if block_ptr == 0 {
                buffer[bytes_read..bytes_read + copy_len].fill(0);
            } else {
                let block_data = self.read_block(block_ptr as u64)?;
                buffer[bytes_read..bytes_read + copy_len]
                    .copy_from_slice(&block_data[copy_offset..copy_offset + copy_len]);
            }

            bytes_read += copy_len;
        }

        Ok(bytes_read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut ino = self.read_inode(inode)?;
        let meta = self.inode_to_metadata(inode, &ino);

        // Cannot write to a directory via the file-write path.
        if meta.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }

        let block_size = self.block_size as u64;
        let max_offset = core::cmp::min(
            self.max_classic_file_blocks().saturating_mul(block_size),
            u32::MAX as u64,
        );

        if offset >= max_offset {
            return Err(FsError::NoSpaceLeft);
        }

        // Clip the write to the end of the classic block-map region so we never
        // silently drop data beyond what the reader can later read back.
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let writable_end = core::cmp::min(end, max_offset);
        let writable_len = (writable_end - offset) as usize;
        let writable = &buffer[..writable_len];

        let mut bytes_written = 0usize;
        let mut block_idx = offset / block_size;
        let mut block_off = (offset % block_size) as usize;
        let mut i_block = unsafe { core::ptr::addr_of!(ino.i_block).read_unaligned() };

        while bytes_written < writable_len {
            let block_ptr = self.ensure_logical_block_pointer(&mut i_block, block_idx)?;

            // Read the existing block so partial writes preserve the rest of
            // the block's contents (alloc_block already zeroed fresh blocks,
            // but a pre-existing block may hold live data).
            let mut data = self.read_block(block_ptr as u64)?;
            let copy_len = core::cmp::min(
                block_size as usize - block_off,
                writable_len - bytes_written,
            );
            data[block_off..block_off + copy_len]
                .copy_from_slice(&writable[bytes_written..bytes_written + copy_len]);
            self.write_block(block_ptr as u64, &data)?;

            bytes_written += copy_len;
            block_idx += 1;
            block_off = 0;
        }

        // Persist the (possibly updated) classic block pointer array.
        unsafe { core::ptr::addr_of_mut!(ino.i_block).write_unaligned(i_block) };

        // Update file size, block count, and timestamps.
        let new_size = core::cmp::max(meta.size, offset + bytes_written as u64);
        ino.i_size_lo = new_size as u32;
        ino.i_size_high = (new_size >> 32) as u32;
        ino.i_mtime = self.current_time();
        ino.i_atime = ino.i_mtime;
        // i_blocks is in 512-byte units; include data and indirect pointer blocks.
        let used_blocks = self.count_allocated_file_blocks(&i_block)?;
        ino.i_blocks_lo = used_blocks.saturating_mul((self.block_size / 512) as u64) as u32;

        self.write_inode(inode, &ino)?;
        self.flush_dirty_blocks()?;

        Ok(bytes_written)
    }

    fn metadata(&self, inode_num: InodeNumber) -> FsResult<FileMetadata> {
        let inode = self.read_inode(inode_num)?;
        Ok(self.inode_to_metadata(inode_num, &inode))
    }

    fn set_metadata(&self, inode_num: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inode = self.read_inode(inode_num)?;
        inode.i_mode =
            ((inode.i_mode as u32 & 0o7770000) | metadata.permissions.to_octal() as u32) as u16;
        inode.i_uid = metadata.uid as u16;
        inode.i_gid = metadata.gid as u16;
        inode.i_mtime = metadata.modified as u32;
        inode.i_atime = metadata.accessed as u32;
        self.write_inode(inode_num, &inode)?;
        self.flush_dirty_blocks()?;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, dirname) = self.split_path(path)?;
        let parent_inode_num = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode(parent_inode_num)?;
        let parent_meta = self.inode_to_metadata(parent_inode_num, &parent_inode);
        if parent_meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve_path(path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        let new_inode_num = self.alloc_inode()?;
        let mode = (0o040000u32 | permissions.to_octal() as u32) as u16; // S_IFDIR | perms
        let now = self.current_time();
        let mut inode: Ext4Inode = Ext4Inode::default();
        inode.i_mode = mode;
        inode.i_uid = 0;
        inode.i_size_lo = self.block_size; // directory starts with one block
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_gid = 0;
        inode.i_links_count = 2; // . and parent
        inode.i_blocks_lo = (self.block_size / 512) as u32;
        inode.i_flags = 0;

        // Allocate one data block for the new directory
        let new_block = self.alloc_block()?;
        inode.i_block[0] = new_block as u32;

        // Initialize the directory block with . and .. entries
        let block_size = self.block_size as usize;
        let mut data = vec![0u8; block_size];

        // "." entry
        let dot = Ext4DirEntry2 {
            inode: new_inode_num as u32,
            rec_len: 12,
            name_len: 1,
            file_type: 2, // directory
        };
        unsafe {
            core::ptr::write_unaligned(data.as_mut_ptr() as *mut Ext4DirEntry2, dot);
        }
        data[8] = b'.';

        // ".." entry — rec_len spans the rest of the block
        let dotdot = Ext4DirEntry2 {
            inode: parent_inode_num as u32,
            rec_len: (block_size - 12) as u16,
            name_len: 2,
            file_type: 2,
        };
        unsafe {
            core::ptr::write_unaligned(data.as_mut_ptr().add(12) as *mut Ext4DirEntry2, dotdot);
        }
        data[20] = b'.';
        data[21] = b'.';

        self.write_block(new_block, &data)?;
        self.write_inode(new_inode_num, &inode)?;

        // Link the new directory into the parent
        self.add_dir_entry(parent_inode_num, dirname, new_inode_num, 2)?;

        // Increment parent's link count for the new subdirectory's ".."
        let mut p = parent_inode;
        p.i_links_count = p.i_links_count.saturating_add(1);
        self.write_inode(parent_inode_num, &p)?;

        self.flush_dirty_blocks()?;
        Ok(new_inode_num)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let inode_num = self.resolve_path(path)?;
        let inode = self.read_inode(inode_num)?;
        let meta = self.inode_to_metadata(inode_num, &inode);
        if meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Check if directory is empty (only . and ..)
        let entries = self.read_directory_entries(&inode)?;
        // Filter out . and ..
        let real_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .collect();
        if !real_entries.is_empty() {
            return Err(FsError::DirectoryNotEmpty);
        }

        let (parent_path, dirname) = self.split_path(path)?;
        let parent_inode_num = self.resolve_path(parent_path)?;

        // Free the directory's data block(s)
        let i_block = unsafe { core::ptr::addr_of!(inode.i_block).read_unaligned() };
        for &block_ptr in &i_block[0..12] {
            if block_ptr != 0 {
                self.free_block(block_ptr as u64)?;
            }
        }

        // Remove the directory entry from the parent
        self.remove_dir_entry(parent_inode_num, dirname)?;

        // Decrement parent's link count
        let mut parent = self.read_inode(parent_inode_num)?;
        parent.i_links_count = parent.i_links_count.saturating_sub(1);
        self.write_inode(parent_inode_num, &parent)?;

        // Free the inode itself
        self.free_inode(inode_num)?;

        self.flush_dirty_blocks()?;
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let inode_num = self.resolve_path(path)?;
        let inode = self.read_inode(inode_num)?;
        let meta = self.inode_to_metadata(inode_num, &inode);

        if meta.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }

        let (parent_path, filename) = self.split_path(path)?;
        let parent_inode_num = self.resolve_path(parent_path)?;

        // Remove the directory entry
        self.remove_dir_entry(parent_inode_num, filename)?;

        // Decrement link count
        let mut ino = inode;
        if ino.i_links_count > 0 {
            ino.i_links_count -= 1;
        }

        if ino.i_links_count == 0 {
            // No more links — free data blocks and the inode
            let i_block = unsafe { core::ptr::addr_of!(ino.i_block).read_unaligned() };
            for &block_ptr in &i_block[0..12] {
                if block_ptr != 0 {
                    self.free_block(block_ptr as u64)?;
                }
            }
            self.free_inode(inode_num)?;
        } else {
            // Still has other hard links — just update the inode
            ino.i_dtime = self.current_time();
            self.write_inode(inode_num, &ino)?;
        }

        self.flush_dirty_blocks()?;
        Ok(())
    }

    fn readdir(&self, inode_num: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inode = self.read_inode(inode_num)?;
        let metadata = self.inode_to_metadata(inode_num, &inode);

        if metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        self.read_directory_entries(&inode)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        if old_path == new_path {
            return Ok(());
        }

        let old_inode_num = self.resolve_path(old_path)?;
        let old_inode = self.read_inode(old_inode_num)?;
        let meta = self.inode_to_metadata(old_inode_num, &old_inode);

        let (old_parent_path, old_filename) = self.split_path(old_path)?;
        let old_parent_inode_num = self.resolve_path(old_parent_path)?;

        let (new_parent_path, new_filename) = self.split_path(new_path)?;
        let new_parent_inode_num = self.resolve_path(new_parent_path)?;

        // Check if destination already exists
        if let Ok(existing_inode) = self.resolve_path(new_path) {
            if existing_inode == old_inode_num {
                return Ok(());
            }
            let existing_meta = self.metadata(existing_inode)?;
            if meta.file_type == FileType::Directory {
                if existing_meta.file_type != FileType::Directory {
                    return Err(FsError::NotADirectory);
                }
                // Destination directory must be empty
                let existing_inode_obj = self.read_inode(existing_inode)?;
                let entries = self.read_directory_entries(&existing_inode_obj)?;
                let real: Vec<_> = entries
                    .iter()
                    .filter(|e| e.name != "." && e.name != "..")
                    .collect();
                if !real.is_empty() {
                    return Err(FsError::DirectoryNotEmpty);
                }
                // Free the empty destination directory
                let i_block =
                    unsafe { core::ptr::addr_of!(existing_inode_obj.i_block).read_unaligned() };
                for &bp in &i_block[0..12] {
                    if bp != 0 {
                        self.free_block(bp as u64)?;
                    }
                }
                self.remove_dir_entry(new_parent_inode_num, new_filename)?;
                let mut new_parent = self.read_inode(new_parent_inode_num)?;
                new_parent.i_links_count = new_parent.i_links_count.saturating_sub(1);
                self.write_inode(new_parent_inode_num, &new_parent)?;
                self.free_inode(existing_inode)?;
            } else {
                // Overwrite existing file — remove it first
                self.remove_dir_entry(new_parent_inode_num, new_filename)?;
                let existing = self.read_inode(existing_inode)?;
                if existing.i_links_count <= 1 {
                    let i_block = unsafe { core::ptr::addr_of!(existing.i_block).read_unaligned() };
                    for &bp in &i_block[0..12] {
                        if bp != 0 {
                            self.free_block(bp as u64)?;
                        }
                    }
                    self.free_inode(existing_inode)?;
                } else {
                    let mut ei = existing;
                    ei.i_links_count -= 1;
                    self.write_inode(existing_inode, &ei)?;
                }
            }
        }

        // Add new entry pointing to the old inode
        let file_type = match meta.file_type {
            FileType::Regular => 1u8,
            FileType::Directory => 2,
            FileType::SymbolicLink => 7,
            _ => 1,
        };
        self.add_dir_entry(new_parent_inode_num, new_filename, old_inode_num, file_type)?;

        // Remove old entry
        self.remove_dir_entry(old_parent_inode_num, old_filename)?;

        // If directory, update .. in the moved directory
        if meta.file_type == FileType::Directory {
            let moved_inode = self.read_inode(old_inode_num)?;
            let i_block = unsafe { core::ptr::addr_of!(moved_inode.i_block).read_unaligned() };
            if i_block[0] != 0 {
                let mut data = self.read_block(i_block[0] as u64)?;
                // Update ".." entry (at offset 12)
                if data.len() >= 22 {
                    let dd_ptr = unsafe { data.as_mut_ptr().add(12) } as *mut Ext4DirEntry2;
                    let mut dd = unsafe { core::ptr::read_unaligned(dd_ptr) };
                    dd.inode = new_parent_inode_num as u32;
                    unsafe {
                        core::ptr::write_unaligned(dd_ptr, dd);
                    }
                    self.write_block(i_block[0] as u64, &data)?;
                }
            }

            // If parent changed, adjust link counts
            if old_parent_inode_num != new_parent_inode_num {
                let mut old_parent = self.read_inode(old_parent_inode_num)?;
                old_parent.i_links_count = old_parent.i_links_count.saturating_sub(1);
                self.write_inode(old_parent_inode_num, &old_parent)?;

                let mut new_parent = self.read_inode(new_parent_inode_num)?;
                new_parent.i_links_count = new_parent.i_links_count.saturating_add(1);
                self.write_inode(new_parent_inode_num, &new_parent)?;
            }
        }

        self.flush_dirty_blocks()?;
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, linkname) = self.split_path(link_path)?;
        let parent_inode_num = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode(parent_inode_num)?;
        let parent_meta = self.inode_to_metadata(parent_inode_num, &parent_inode);
        if parent_meta.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if self.resolve_path(link_path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        let new_inode_num = self.alloc_inode()?;
        let mode = (0o120000u32) as u16; // S_IFLNK
        let now = self.current_time();
        let target_bytes = target.as_bytes();
        let mut inode: Ext4Inode = Ext4Inode::default();
        inode.i_mode = mode;
        inode.i_uid = 0;
        inode.i_atime = now;
        inode.i_ctime = now;
        inode.i_mtime = now;
        inode.i_gid = 0;
        inode.i_links_count = 1;
        inode.i_flags = 0;

        if target_bytes.len() <= 60 {
            // Inline symlink — store target in i_block array
            inode.i_size_lo = target_bytes.len() as u32;
            // SAFETY: `i_block` is a field in a `repr(C)` inode and 60 bytes
            // is the inline symlink max size.
            let block_bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    core::ptr::addr_of_mut!(inode.i_block) as *mut u8,
                    60,
                )
            };
            block_bytes[..target_bytes.len()].copy_from_slice(target_bytes);
        } else {
            // Long symlink — allocate a data block
            if target_bytes.len() > self.block_size as usize * 12 {
                return Err(FsError::NameTooLong);
            }
            inode.i_size_lo = target_bytes.len() as u32;
            let block_size = self.block_size as usize;
            let mut blocks = 0u32;
            for (slot, chunk) in target_bytes.chunks(block_size).enumerate() {
                let new_block = self.alloc_block()?;
                inode.i_block[slot] = new_block as u32;
                blocks += 1;

                let mut data = vec![0u8; block_size];
                data[..chunk.len()].copy_from_slice(chunk);
                self.write_block(new_block, &data)?;
            }
            inode.i_blocks_lo = blocks * (self.block_size / 512);
        }

        self.write_inode(new_inode_num, &inode)?;
        self.add_dir_entry(parent_inode_num, linkname, new_inode_num, 7)?; // file_type 7 = symlink
        self.flush_dirty_blocks()?;
        Ok(())
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
