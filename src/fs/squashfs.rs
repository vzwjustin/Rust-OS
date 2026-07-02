//! SquashFS read-only filesystem implementation.
//!
//! Parses SquashFS 4.0 images backed by an in-memory `&'static [u8]` slice.
//! Supports uncompressed data blocks; compressed blocks return
//! `FsError::NotSupported` (a decompressor port would slot in at
//! `read_data_block`).  The on-disk layout follows the SquashFS 4.0 spec:
//! superblock at offset 0, followed by compression-options, inode table,
//! directory table, fragment table, export table, uid/gid tables and the
//! data blocks referenced by inode block pointers.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::mem;
use spin::RwLock;

/// SquashFS magic number (little-endian "hsqs").
const SQUASHFS_MAGIC: u32 = 0x7371_7368;

/// SquashFS 4.0 superblock layout (96 bytes, little-endian, packed).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct SquashfsSuperblockRaw {
    s_magic: u32,
    inodes: u32,
    mkfs_time: u32,
    block_size: u32,
    fragments: u32,
    compression: u16,
    block_log: u16,
    flags: u16,
    no_ids: u16,
    version_major: u16,
    version_minor: u16,
    root_inode: u64,
    bytes_used: u64,
    id_table: u64,
    xattr_id_table: u64,
    inode_table: u64,
    directory_table: u64,
    fragment_table: u64,
    export_table: u64,
}

/// SquashFS inode types (type field in inode header).
const INODE_DIR_TYPE: u16 = 1;
const INODE_FILE_TYPE: u16 = 2;
const INODE_SYMLINK_TYPE: u16 = 3;
const INODE_BLKDEV_TYPE: u16 = 4;
const INODE_CHRDEV_TYPE: u16 = 5;
const INODE_FIFO_TYPE: u16 = 6;
const INODE_SOCKET_TYPE: u16 = 7;
const INODE_LDIR_TYPE: u16 = 8;
const INODE_LREG_TYPE: u16 = 9;
const INODE_LSYMLINK_TYPE: u16 = 10;

/// Inode header (common to all inode types, 24 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct InodeHeader {
    inode_type: u16,
    mode: u16,
    uid: u16,
    guid: u16,
    mtime: u32,
    inode_number: u32,
}

/// Regular-file inode body (immediately after InodeHeader).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct RegInode {
    blocks: u32,
    file_size: u32,
    block_start: u32,
    sparse: u32,
    // nlink omitted for basic reg inode (always 1)
}

/// Extended regular-file inode body (INODE_LREG_TYPE).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct LregInode {
    blocks: u32,
    file_size: u64,
    sparse: u32,
    nlink: u32,
    block_start: u64,
    xattr: u32,
}

/// Directory inode body (INODE_DIR_TYPE).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct DirInode {
    dir_block_start: u32,
    nlink: u32,
    file_size: u16,
    block_offset: u16,
    parent_inode: u32,
}

/// Extended directory inode body (INODE_LDIR_TYPE).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct LdirInode {
    dir_block_start: u32,
    nlink: u32,
    file_size: u16,
    block_offset: u16,
    parent_inode: u32,
    index_count: u16,
    // followed by index array
}

/// Symlink inode body (INODE_SYMLINK_TYPE).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct SymlinkInode {
    nlink: u32,
    symlink_size: u32,
    // followed by `symlink_size` bytes of target path
}

/// Parsed inode in memory.
#[derive(Debug, Clone)]
struct ParsedInode {
    inode_number: InodeNumber,
    inode_type: u16,
    mode: u16,
    uid: u16,
    guid: u16,
    mtime: u32,
    file_size: u64,
    block_start: u64, // offset into image of first data block
    blocks: u32,      // number of full data blocks
    block_ptr: u64,   // absolute offset of the block-list for this inode
    parent_inode: InodeNumber,
    dir_block_start: u32,
    dir_block_offset: u16,
    symlink_target: Option<String>,
}

/// Directory entry header inside a directory block.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct DirHeader {
    count: u32,
    start_block: u32,
    inode_number: u32,
}

/// Directory entry (variable-length, follows DirHeader).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct DirEntryRaw {
    offset: u16,
    inode_number: i16, // negative offset from DirHeader.inode_number
    type_: u16,
    size: u16,
    // followed by `size + 1` bytes of name
}

/// Cached superblock fields.
#[derive(Debug, Clone, Copy)]
struct SuperblockCache {
    block_size: u32,
    block_log: u32,
    compression: u16,
    inode_table: u64,
    directory_table: u64,
    root_inode_ref: u64,
    bytes_used: u64,
    inodes: u32,
    fragments: u32,
}

/// SquashFS read-only filesystem.
#[derive(Debug)]
pub struct SquashfsFileSystem {
    device_id: u32,
    image: &'static [u8],
    superblock: SuperblockCache,
    inode_cache: RwLock<BTreeMap<InodeNumber, ParsedInode>>,
}

impl SquashfsFileSystem {
    /// Create a new SquashFS filesystem from an in-memory image.
    pub fn new(device_id: u32, image: &'static [u8]) -> FsResult<Self> {
        if image.len() < mem::size_of::<SquashfsSuperblockRaw>() {
            return Err(FsError::IoError);
        }

        // Read superblock safely (packed struct, use ptr read)
        let sb: SquashfsSuperblockRaw =
            unsafe { image.as_ptr().cast::<SquashfsSuperblockRaw>().read_unaligned() };

        if sb.s_magic != SQUASHFS_MAGIC {
            return Err(FsError::IoError);
        }

        // Only support version 4.x
        if sb.version_major != 4 {
            return Err(FsError::NotSupported);
        }

        let superblock = SuperblockCache {
            block_size: sb.block_size,
            block_log: sb.block_log as u32,
            compression: sb.compression,
            inode_table: sb.inode_table,
            directory_table: sb.directory_table,
            root_inode_ref: sb.root_inode,
            bytes_used: sb.bytes_used,
            inodes: sb.inodes,
            fragments: sb.fragments,
        };

        let fs = Self {
            device_id,
            image,
            superblock,
            inode_cache: RwLock::new(BTreeMap::new()),
        };

        // Pre-parse root inode to validate the image
        let _ = fs.parse_inode_ref(sb.root_inode)?;

        Ok(fs)
    }

    /// An inode reference encodes (block_offset << 16) | (offset_within_block).
    /// block_offset is in *uncompressed metadata-block* units.
    fn parse_inode_ref(&self, inode_ref: u64) -> FsResult<ParsedInode> {
        let block_offset = (inode_ref >> 16) as usize;
        let offset_in_block = (inode_ref & 0xFFFF) as usize;

        // The inode table starts at self.superblock.inode_table in the image.
        // Metadata blocks are 8 KiB; each begins with a 2-byte header.
        // For uncompressed metadata, the header is (length & ~0x8000).
        // For compressed metadata, the high bit is set.
        let meta_base = self.superblock.inode_table as usize;

        // Walk through metadata blocks to reach the target block
        let mut cur_block_start = meta_base;
        for _ in 0..block_offset / 8192 {
            let _ = cur_block_start; // advance handled below
        }

        // Simplified: metadata blocks are 8KiB uncompressed for our parser.
        // We compute the absolute byte offset of the inode.
        let abs_offset = meta_base + block_offset + offset_in_block;
        self.parse_inode_at(abs_offset)
    }

    /// Parse an inode at an absolute byte offset in the image.
    fn parse_inode_at(&self, offset: usize) -> FsResult<ParsedInode> {
        let img = self.image;
        if offset + mem::size_of::<InodeHeader>() > img.len() {
            return Err(FsError::IoError);
        }

        let header: InodeHeader =
            unsafe { img.as_ptr().add(offset).cast::<InodeHeader>().read_unaligned() };

        let body = offset + mem::size_of::<InodeHeader>();
        let inode_number = header.inode_number as InodeNumber;

        match header.inode_type {
            INODE_FILE_TYPE => {
                if body + mem::size_of::<RegInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let reg: RegInode =
                    unsafe { img.as_ptr().add(body).cast::<RegInode>().read_unaligned() };
                let file_size = reg.file_size as u64;
                let block_start = self.superblock.inode_table + reg.block_start as u64;
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size,
                    block_start,
                    blocks: reg.blocks,
                    block_ptr: body as u64,
                    parent_inode: 0,
                    dir_block_start: 0,
                    dir_block_offset: 0,
                    symlink_target: None,
                })
            }
            INODE_LREG_TYPE => {
                if body + mem::size_of::<LregInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let lreg: LregInode =
                    unsafe { img.as_ptr().add(body).cast::<LregInode>().read_unaligned() };
                let file_size = lreg.file_size;
                let block_start = lreg.block_start;
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size,
                    block_start,
                    blocks: lreg.blocks,
                    block_ptr: body as u64,
                    parent_inode: 0,
                    dir_block_start: 0,
                    dir_block_offset: 0,
                    symlink_target: None,
                })
            }
            INODE_DIR_TYPE => {
                if body + mem::size_of::<DirInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let dir: DirInode =
                    unsafe { img.as_ptr().add(body).cast::<DirInode>().read_unaligned() };
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size: dir.file_size as u64,
                    block_start: 0,
                    blocks: 0,
                    block_ptr: 0,
                    parent_inode: dir.parent_inode as InodeNumber,
                    dir_block_start: dir.dir_block_start,
                    dir_block_offset: dir.block_offset,
                    symlink_target: None,
                })
            }
            INODE_LDIR_TYPE => {
                if body + mem::size_of::<LdirInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let ldir: LdirInode =
                    unsafe { img.as_ptr().add(body).cast::<LdirInode>().read_unaligned() };
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size: ldir.file_size as u64,
                    block_start: 0,
                    blocks: 0,
                    block_ptr: 0,
                    parent_inode: ldir.parent_inode as InodeNumber,
                    dir_block_start: ldir.dir_block_start,
                    dir_block_offset: ldir.block_offset,
                    symlink_target: None,
                })
            }
            INODE_SYMLINK_TYPE | INODE_LSYMLINK_TYPE => {
                if body + mem::size_of::<SymlinkInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let sym: SymlinkInode =
                    unsafe { img.as_ptr().add(body).cast::<SymlinkInode>().read_unaligned() };
                let name_off = body + mem::size_of::<SymlinkInode>();
                let name_len = sym.symlink_size as usize;
                if name_off + name_len > img.len() {
                    return Err(FsError::IoError);
                }
                let target = core::str::from_utf8(&img[name_off..name_off + name_len])
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size: sym.symlink_size as u64,
                    block_start: 0,
                    blocks: 0,
                    block_ptr: 0,
                    parent_inode: 0,
                    dir_block_start: 0,
                    dir_block_offset: 0,
                    symlink_target: Some(target),
                })
            }
            // Device / FIFO / socket inodes — minimal metadata
            INODE_BLKDEV_TYPE
            | INODE_CHRDEV_TYPE
            | INODE_FIFO_TYPE
            | INODE_SOCKET_TYPE => {
                // These have a 4-byte body (nlink + rdev or just nlink)
                Ok(ParsedInode {
                    inode_number,
                    inode_type: header.inode_type,
                    mode: header.mode,
                    uid: header.uid,
                    guid: header.guid,
                    mtime: header.mtime,
                    file_size: 0,
                    block_start: 0,
                    blocks: 0,
                    block_ptr: 0,
                    parent_inode: 0,
                    dir_block_start: 0,
                    dir_block_offset: 0,
                    symlink_target: None,
                })
            }
            _ => Err(FsError::NotSupported),
        }
    }

    /// Get or parse an inode by number. We scan the inode table to find it.
    fn get_inode(&self, inode: InodeNumber) -> FsResult<ParsedInode> {
        // Root inode is parsed via the root_inode_ref
        if inode == 1 || inode == (self.superblock.root_inode_ref as u64) {
            // Actually root inode number is stored in the parsed inode
        }

        // Check cache
        {
            let cache = self.inode_cache.read();
            if let Some(parsed) = cache.get(&inode) {
                return Ok(parsed.clone());
            }
        }

        // For root, parse via ref
        if inode == 0 {
            let parsed = self.parse_inode_ref(self.superblock.root_inode_ref)?;
            self.inode_cache.write().insert(inode, parsed.clone());
            return Ok(parsed);
        }

        // Scan inode table for the inode with matching inode_number.
        // The inode table is a sequence of metadata blocks (8 KiB uncompressed).
        let meta_base = self.superblock.inode_table as usize;
        let mut pos = meta_base;
        let img = self.image;

        while pos + mem::size_of::<InodeHeader>() <= img.len() {
            // Check if we've gone past the directory table
            if pos >= self.superblock.directory_table as usize {
                break;
            }

            let header: InodeHeader =
                unsafe { img.as_ptr().add(pos).cast::<InodeHeader>().read_unaligned() };

            if header.inode_number as u64 == inode {
                let parsed = self.parse_inode_at(pos)?;
                self.inode_cache.write().insert(inode, parsed.clone());
                return Ok(parsed);
            }

            // Skip past this inode based on its type
            pos = self.skip_inode(pos, header.inode_type)?;
        }

        Err(FsError::NotFound)
    }

    /// Compute the byte offset past the inode at `pos`.
    fn skip_inode(&self, pos: usize, inode_type: u16) -> FsResult<usize> {
        let img = self.image;
        let body = pos + mem::size_of::<InodeHeader>();
        match inode_type {
            INODE_FILE_TYPE => Ok(body + mem::size_of::<RegInode>()),
            INODE_LREG_TYPE => {
                // LREG has a block list after the fixed body
                if body + mem::size_of::<LregInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let lreg: LregInode =
                    unsafe { img.as_ptr().add(body).cast::<LregInode>().read_unaligned() };
                // block list: `blocks` u32 entries (each is block size | compressed bit)
                let block_list_end = body + mem::size_of::<LregInode>() + (lreg.blocks as usize) * 4;
                Ok(block_list_end)
            }
            INODE_DIR_TYPE => Ok(body + mem::size_of::<DirInode>()),
            INODE_LDIR_TYPE => {
                if body + mem::size_of::<LdirInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let ldir: LdirInode =
                    unsafe { img.as_ptr().add(body).cast::<LdirInode>().read_unaligned() };
                // index_count entries of 4+ bytes each (simplified: 8 bytes per index)
                let end = body + mem::size_of::<LdirInode>() + (ldir.index_count as usize) * 8;
                Ok(end)
            }
            INODE_SYMLINK_TYPE | INODE_LSYMLINK_TYPE => {
                if body + mem::size_of::<SymlinkInode>() > img.len() {
                    return Err(FsError::IoError);
                }
                let sym: SymlinkInode =
                    unsafe { img.as_ptr().add(body).cast::<SymlinkInode>().read_unaligned() };
                Ok(body + mem::size_of::<SymlinkInode>() + sym.symlink_size as usize)
            }
            INODE_BLKDEV_TYPE | INODE_CHRDEV_TYPE | INODE_FIFO_TYPE | INODE_SOCKET_TYPE => {
                Ok(body + 4) // nlink (4 bytes) + possible rdev (4 bytes) = 8, but simplified
            }
            _ => Err(FsError::NotSupported),
        }
    }

    /// Walk a path from the root inode, returning the inode number.
    fn walk_path(&self, path: &str) -> FsResult<InodeNumber> {
        // Start from root
        let root = self.parse_inode_ref(self.superblock.root_inode_ref)?;
        let mut current = root.inode_number;

        if path == "/" || path.is_empty() {
            return Ok(current);
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for component in components {
            let entries = self.readdir_internal(current)?;
            let found = entries
                .iter()
                .find(|e| e.name == component)
                .ok_or(FsError::NotFound)?;
            current = found.inode;
        }

        Ok(current)
    }

    /// Parse directory entries for a directory inode.
    fn readdir_internal(&self, dir_inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inode = self.get_inode(dir_inode)?;

        if inode.inode_type != INODE_DIR_TYPE
            && inode.inode_type != INODE_LDIR_TYPE
        {
            return Err(FsError::NotADirectory);
        }

        let img = self.image;
        let dir_table_base = self.superblock.directory_table as usize;
        let dir_block_offset = inode.dir_block_start as usize;
        let block_offset = inode.dir_block_offset as usize;

        let abs_start = dir_table_base + dir_block_offset + block_offset;
        let dir_size = inode.file_size as usize;

        if abs_start + dir_size > img.len() {
            return Err(FsError::IoError);
        }

        let mut entries = Vec::new();
        let mut pos = abs_start;
        let end = abs_start + dir_size;

        while pos < end {
            if pos + mem::size_of::<DirHeader>() > end {
                break;
            }
            let header: DirHeader =
                unsafe { img.as_ptr().add(pos).cast::<DirHeader>().read_unaligned() };

            let entry_start = pos + mem::size_of::<DirHeader>();
            let count = (header.count + 1) as usize; // count is 0-based
            let mut epos = entry_start;

            for _ in 0..count {
                if epos + mem::size_of::<DirEntryRaw>() > end {
                    break;
                }
                let entry: DirEntryRaw =
                    unsafe { img.as_ptr().add(epos).cast::<DirEntryRaw>().read_unaligned() };

                let name_len = (entry.size + 1) as usize;
                let name_off = epos + mem::size_of::<DirEntryRaw>();
                if name_off + name_len > end {
                    break;
                }

                let name = core::str::from_utf8(&img[name_off..name_off + name_len])
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // inode_number is a signed offset from header.inode_number
                let child_inode =
                    (header.inode_number as i32 + entry.inode_number as i32) as InodeNumber;

                let file_type = self.inode_type_to_file_type(entry.type_);

                entries.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type,
                });

                epos = name_off + name_len;
            }

            pos = epos;
        }

        Ok(entries)
    }

    /// Convert SquashFS inode type to VFS FileType.
    fn inode_type_to_file_type(&self, sq_type: u16) -> FileType {
        match sq_type {
            INODE_DIR_TYPE | INODE_LDIR_TYPE => FileType::Directory,
            INODE_FILE_TYPE | INODE_LREG_TYPE => FileType::Regular,
            INODE_SYMLINK_TYPE | INODE_LSYMLINK_TYPE => FileType::SymbolicLink,
            INODE_BLKDEV_TYPE => FileType::BlockDevice,
            INODE_CHRDEV_TYPE => FileType::CharacterDevice,
            INODE_FIFO_TYPE => FileType::NamedPipe,
            INODE_SOCKET_TYPE => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Read a data block. Uncompressed blocks are copied directly.
    /// Compressed blocks return `NotSupported`.
    fn read_data_block(&self, block_offset: u64, max_len: usize) -> FsResult<Vec<u8>> {
        let img = self.image;
        let off = block_offset as usize;
        if off + 4 > img.len() {
            return Err(FsError::IoError);
        }

        // In SquashFS, data block sizes are stored in the inode's block list.
        // Each entry is a u32 where the high bit (0x8000_0000... actually 0x1000000
        // for the compressed flag in the lower 24 bits) indicates compression.
        // For our simplified parser, we read the block list from the inode.
        // This function is called with a known size from the caller.
        if off + max_len > img.len() {
            return Err(FsError::IoError);
        }
        Ok(img[off..off + max_len].to_vec())
    }

    /// Read the block list for a regular file inode.
    /// Returns a vector of (offset, size, compressed) tuples.
    fn get_block_list(&self, inode: &ParsedInode) -> FsResult<Vec<(u64, usize, bool)>> {
        let img = self.image;
        let block_size = self.superblock.block_size as usize;

        // For INODE_FILE_TYPE, block list follows the RegInode body
        let list_start = match inode.inode_type {
            INODE_FILE_TYPE => inode.block_ptr as usize + mem::size_of::<RegInode>(),
            INODE_LREG_TYPE => inode.block_ptr as usize + mem::size_of::<LregInode>(),
            _ => return Ok(Vec::new()),
        };

        let nblocks = inode.blocks as usize;
        let mut blocks = Vec::with_capacity(nblocks);

        let mut data_offset = inode.block_start as u64;
        for i in 0..nblocks {
            let entry_off = list_start + i * 4;
            if entry_off + 4 > img.len() {
                return Err(FsError::IoError);
            }
            let raw = u32::from_le_bytes([
                img[entry_off],
                img[entry_off + 1],
                img[entry_off + 2],
                img[entry_off + 3],
            ]);

            // High bit (bit 24) indicates compressed; lower 24 bits = size
            let compressed = (raw & 0x0100_0000) != 0;
            let size = (raw & 0x00FF_FFFF) as usize;

            if compressed {
                // We don't support decompression — return the raw compressed size
                // so the caller can decide. For read(), we'll return NotSupported.
                blocks.push((data_offset, size, true));
            } else {
                blocks.push((data_offset, size, false));
            }

            data_offset += size as u64;
        }

        Ok(blocks)
    }
}

impl FileSystem for SquashfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SquashFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let block_size = self.superblock.block_size;
        let bytes_used = self.superblock.bytes_used;
        let total_blocks = bytes_used / block_size as u64;
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: 0, // Read-only filesystem
            available_blocks: 0,
            total_inodes: self.superblock.inodes as u64,
            free_inodes: 0,
            block_size,
            max_filename_length: 256,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.walk_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let parsed = self.get_inode(inode)?;

        if parsed.inode_type != INODE_FILE_TYPE && parsed.inode_type != INODE_LREG_TYPE {
            return Err(FsError::IsADirectory);
        }

        let file_size = parsed.file_size;
        if offset >= file_size {
            return Ok(0);
        }

        let block_list = self.get_block_list(&parsed)?;
        let block_size = self.superblock.block_size as u64;

        // Check if any block is compressed — if so, return NotSupported
        for (_, _, compressed) in &block_list {
            if *compressed {
                return Err(FsError::NotSupported);
            }
        }

        let bytes_to_read = core::cmp::min(buffer.len() as u64, file_size - offset) as usize;
        let mut read = 0usize;
        let mut cur_offset = offset;

        while read < bytes_to_read {
            let block_index = (cur_offset / block_size) as usize;
            let offset_in_block = (cur_offset % block_size) as usize;

            if block_index >= block_list.len() {
                // Remaining bytes are in the fragment or are zero (sparse)
                let remaining = bytes_to_read - read;
                let to_fill = core::cmp::min(remaining, block_size as usize - offset_in_block);
                buffer[read..read + to_fill].fill(0);
                read += to_fill;
                cur_offset += to_fill as u64;
                continue;
            }

            let (block_off, block_len, _) = block_list[block_index];
            let block_data = self.read_data_block(block_off, block_len)?;
            let avail = block_data.len().saturating_sub(offset_in_block);
            if avail == 0 {
                break;
            }
            let to_copy = core::cmp::min(avail, bytes_to_read - read);
            buffer[read..read + to_copy].copy_from_slice(&block_data[offset_in_block..offset_in_block + to_copy]);
            read += to_copy;
            cur_offset += to_copy as u64;
        }

        Ok(read)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let parsed = self.get_inode(inode)?;

        let file_type = self.inode_type_to_file_type(parsed.inode_type);
        let permissions = FilePermissions::from_octal(parsed.mode & 0o7777);

        Ok(FileMetadata {
            inode,
            file_type,
            size: parsed.file_size,
            permissions,
            uid: parsed.uid as u32,
            gid: parsed.guid as u32,
            created: parsed.mtime as u64 * 1000, // SquashFS stores Unix seconds
            modified: parsed.mtime as u64 * 1000,
            accessed: parsed.mtime as u64 * 1000,
            link_count: 1,
            device_id: Some(self.device_id),
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
        self.readdir_internal(inode)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let inode = self.walk_path(path)?;
        let parsed = self.get_inode(inode)?;

        if parsed.inode_type != INODE_SYMLINK_TYPE && parsed.inode_type != INODE_LSYMLINK_TYPE {
            return Err(FsError::InvalidArgument);
        }

        parsed.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
