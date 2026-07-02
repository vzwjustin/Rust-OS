//! CRAMFS (Compressed ROM File System) implementation.
//!
//! Cramfs is a simple, read-only, compressed filesystem designed for embedded
//! systems and bootable media. The entire filesystem image lives in a
//! contiguous byte buffer (typically memory-mapped), and all metadata is
//! packed in a compact on-disk format.
//!
//! On-disk layout (all multi-byte fields are little-endian):
//!
//! ```text
//! Superblock (76 bytes, padded):
//!   +0  | magic       | 0x28cd3d45
//!   +4  | flags       | feature flags (e.g. compressed, hole support)
//!   +8  | future      | reserved
//!  +12  | signature   | 16-byte string "Compressed ROMFS"
//!  +28  | fsid        | {crc, edition, blocks, files, lines, reserved} (24 bytes)
//!  +52  | name        | 16-byte volume name (not null-terminated)
//!
//! Inode (12 bytes header + variable-length name):
//!   +0  | mode        | file mode (type + permissions)
//!   +4  | uid         | owner user id (16 bits)
//!   +6  | size00      | low 16 bits of size
//!   +8  | gid         | owner group id (16 bits)
//!  +10  | size01      | high 16 bits of size
//!  +12  | offset      | bits 0..25 = data offset, bits 26..31 = name length
//!  +16  | name        | null-terminated, padded to 4-byte boundary
//! ```
//!
//! For regular files, the data offset points to a table of block descriptors
//! (one `u32` per block), where each descriptor encodes the compressed size
//! and flags. This implementation supports uncompressed blocks; compressed
//! blocks return `FsError::NotSupported` when no decompressor is available.
//!
//! See: linux-master/fs/cramfs/

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{string::{String, ToString}, vec::Vec};

/// Cramfs magic number.
const CRAMFS_MAGIC: u32 = 0x28cd3d45;

/// Width of the offset field in the inode `offset` word (bits 0..25).
const CRAMFS_OFFSET_WIDTH: u32 = 26;
/// Mask for the data-offset portion of the inode `offset` word.
const CRAMFS_OFFSET_MASK: u32 = (1 << CRAMFS_OFFSET_WIDTH) - 1;
/// Shift to extract the name length from the inode `offset` word.
const CRAMFS_NAMELEN_SHIFT: u32 = 26;

/// Block descriptor flag: block is compressed.
const CRAMFS_BLK_FLAG_COMPRESSED: u32 = 1 << 25;
/// Mask for the block size portion of a block descriptor.
const CRAMFS_BLK_SIZE_MASK: u32 = 0x00ffffff;

/// File type bits in the inode mode.
const S_IFMT: u16 = 0xF000;
const S_IFREG: u16 = 0x8000;
const S_IFDIR: u16 = 0x4000;
const S_IFLNK: u16 = 0xA000;
const S_IFCHR: u16 = 0x2000;
const S_IFBLK: u16 = 0x6000;
const S_IFIFO: u16 = 0x1000;
const S_IFSOCK: u16 = 0xC000;

/// Maximum number of symlink hops during path resolution.
const MAX_SYMLINK_DEPTH: usize = 8;

/// Parsed cramfs superblock.
#[derive(Debug, Clone)]
struct CramfsSuper {
    magic: u32,
    flags: u32,
    edition: u32,
    blocks: u32,
    files: u32,
    #[allow(dead_code)]
    volume_name: String,
}

/// Parsed cramfs inode (the fixed 12-byte header portion).
#[derive(Debug, Clone, Copy)]
struct CramfsInode {
    /// Byte offset of this inode header within the image.
    offset: usize,
    mode: u16,
    uid: u16,
    gid: u16,
    size: u32,
    /// Raw `offset` word: bits 0..25 = data offset, bits 26..31 = name length.
    offset_word: u32,
}

impl CramfsInode {
    /// Name length (in bytes, excluding the null terminator).
    fn name_len(&self) -> usize {
        (self.offset_word >> CRAMFS_NAMELEN_SHIFT) as usize
    }

    /// Byte offset of the file data (or first child inode for directories).
    fn data_offset(&self) -> usize {
        (self.offset_word & CRAMFS_OFFSET_MASK) as usize
    }

    /// File type from the mode field.
    fn file_type(&self) -> FileType {
        match self.mode & S_IFMT {
            S_IFDIR => FileType::Directory,
            S_IFREG => FileType::Regular,
            S_IFLNK => FileType::SymbolicLink,
            S_IFCHR => FileType::CharacterDevice,
            S_IFBLK => FileType::BlockDevice,
            S_IFIFO => FileType::NamedPipe,
            S_IFSOCK => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Whether this inode represents a directory.
    fn is_dir(&self) -> bool {
        (self.mode & S_IFMT) == S_IFDIR
    }

    /// Whether this inode represents a symlink.
    fn is_symlink(&self) -> bool {
        (self.mode & S_IFMT) == S_IFLNK
    }

    /// Permissions extracted from the mode.
    fn permissions(&self) -> FilePermissions {
        FilePermissions::from_octal(self.mode & 0o7777)
    }

    /// The effective inode number (byte offset of the header in the image).
    fn inode_number(&self) -> InodeNumber {
        self.offset as InodeNumber
    }
}

/// Cramfs read-only filesystem backed by an in-memory image.
///
/// The backing slice is typically a memory-mapped cramfs image. Because
/// cramfs is immutable, no internal locking is required.
#[derive(Debug)]
pub struct CramfsFileSystem {
    /// Device id reported via metadata.
    #[allow(dead_code)]
    device_id: u32,
    /// The full cramfs image bytes.
    image: &'static [u8],
    /// Parsed superblock.
    superblock: CramfsSuper,
    /// Byte offset of the root inode within the image.
    root_offset: usize,
    /// Block size used for statfs (cramfs uses PAGE_SIZE = 4096).
    block_size: u32,
}

impl CramfsFileSystem {
    /// Create a new cramfs filesystem instance backed by a memory-mapped image.
    ///
    /// `image` must point to a valid cramfs superblock beginning with the
    /// magic `0x28cd3d45`. The slice is expected to live for the static
    /// lifetime of the kernel.
    pub fn new(device_id: u32, image: &'static [u8]) -> FsResult<Self> {
        if image.len() < 76 {
            return Err(FsError::IoError);
        }

        let magic = read_le32(image, 0).ok_or(FsError::IoError)?;
        if magic != CRAMFS_MAGIC {
            return Err(FsError::IoError);
        }

        let flags = read_le32(image, 4).unwrap_or(0);
        let edition = read_le32(image, 32).unwrap_or(0);
        let blocks = read_le32(image, 36).unwrap_or(0);
        let files = read_le32(image, 40).unwrap_or(0);

        // Volume name: 16 bytes at offset 52, not null-terminated.
        let name_bytes = &image[52..68];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        let volume_name = core::str::from_utf8(&name_bytes[..name_end])
            .map(|s| s.to_string())
            .unwrap_or_else(|_| String::new());

        // The root inode is always the first inode, located right after the
        // superblock. The superblock is 76 bytes, padded to a 4-byte boundary.
        let root_offset = 76usize;
        if root_offset + 12 > image.len() {
            return Err(FsError::IoError);
        }

        let superblock = CramfsSuper {
            magic,
            flags,
            edition,
            blocks,
            files,
            volume_name,
        };

        Ok(Self {
            device_id,
            image,
            superblock,
            root_offset,
            block_size: 4096,
        })
    }

    /// Parse the inode header at `offset` within the image.
    fn inode_at(&self, offset: usize) -> Option<CramfsInode> {
        let data = self.image;
        if offset + 16 > data.len() {
            return None;
        }
        let mode = read_le16(data, offset)?;
        let uid = read_le16(data, offset + 4)?;
        let size00 = read_le16(data, offset + 6)? as u32;
        let gid = read_le16(data, offset + 8)?;
        let size01 = read_le16(data, offset + 10)? as u32;
        let offset_word = read_le32(data, offset + 12)?;
        let size = size00 | (size01 << 16);

        Some(CramfsInode {
            offset,
            mode,
            uid,
            gid,
            size,
            offset_word,
        })
    }

    /// Read the name of the inode at `offset`. The name starts at offset+16
    /// and is `name_len` bytes long (null-terminated, padded to 4 bytes).
    fn name_at(&self, offset: usize) -> Option<String> {
        let inode = self.inode_at(offset)?;
        let name_len = inode.name_len();
        if name_len == 0 {
            return Some(String::new());
        }
        let name_start = offset + 16;
        let name_end = name_start + name_len;
        if name_end > self.image.len() {
            return None;
        }
        core::str::from_utf8(&self.image[name_start..name_end])
            .ok()
            .map(|s| s.to_string())
    }

    /// Compute the total size of an inode entry (header + name, padded to 4).
    fn inode_entry_size(&self, offset: usize) -> Option<usize> {
        let inode = self.inode_at(offset)?;
        let name_len = inode.name_len();
        // Header (12 bytes) + name (name_len bytes, including null) + padding to 4.
        let total = 12 + name_len + 1;
        let padded = (total + 3) & !3;
        Some(padded)
    }

    /// Resolve a single path component within a directory.
    ///
    /// `dir_offset` is the byte offset of a directory inode. Returns the byte
    /// offset of the child whose name matches `component`.
    fn lookup_in_dir(&self, dir_offset: usize, component: &str) -> Option<usize> {
        let dir = self.inode_at(dir_offset)?;
        if !dir.is_dir() {
            return None;
        }
        let mut child_off = dir.data_offset();
        let dir_end = child_off + dir.size as usize;
        while child_off + 16 <= dir_end && child_off + 16 <= self.image.len() {
            if let Some(name) = self.name_at(child_off) {
                if name == component {
                    return Some(child_off);
                }
            }
            let entry_size = self.inode_entry_size(child_off)?;
            child_off += entry_size;
        }
        None
    }

    /// Walk a path from the root, returning the byte offset of the final entry.
    /// Symlinks are resolved up to `MAX_SYMLINK_DEPTH` times.
    fn walk(&self, path: &str, depth: usize) -> FsResult<usize> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::TooManySymlinks);
        }
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(self.root_offset);
        }

        let mut current = self.root_offset;
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            // Follow symlinks in intermediate path components.
            let entry = self.inode_at(current).ok_or(FsError::NotFound)?;
            if entry.is_symlink() {
                let target = self.readlink_data(&entry)?;
                let resolved = self.walk(&target, depth + 1)?;
                let resolved_entry = self.inode_at(resolved).ok_or(FsError::NotFound)?;
                if !resolved_entry.is_dir() {
                    return Err(FsError::NotADirectory);
                }
                current = resolved;
            }
            let entry = self.inode_at(current).ok_or(FsError::NotFound)?;
            if !entry.is_dir() {
                return Err(FsError::NotADirectory);
            }
            current = self
                .lookup_in_dir(current, component)
                .ok_or(FsError::NotFound)?;
        }

        // Follow a trailing symlink.
        let entry = self.inode_at(current).ok_or(FsError::NotFound)?;
        if entry.is_symlink() && depth < MAX_SYMLINK_DEPTH {
            let target = self.readlink_data(&entry)?;
            return self.walk(&target, depth + 1);
        }

        Ok(current)
    }

    /// Read the symlink target string from the inode's data region.
    fn readlink_data(&self, inode: &CramfsInode) -> FsResult<String> {
        let data_off = inode.data_offset();
        let size = inode.size as usize;
        let end = data_off.checked_add(size).ok_or(FsError::IoError)?;
        if end > self.image.len() {
            return Err(FsError::IoError);
        }
        core::str::from_utf8(&self.image[data_off..end])
            .map(|s| s.to_string())
            .map_err(|_| FsError::IoError)
    }

    /// Build `FileMetadata` from a parsed inode.
    fn metadata_for(&self, inode: &CramfsInode) -> FileMetadata {
        let file_type = inode.file_type();
        let size = inode.size as u64;

        FileMetadata {
            inode: inode.inode_number(),
            file_type,
            size,
            permissions: inode.permissions(),
            uid: inode.uid as u32,
            gid: inode.gid as u32,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: None,
        }
    }

    /// Read file data from a regular file inode. For each block, the block
    /// descriptor table (at `data_offset`) contains one `u32` per block
    /// encoding the block size and compression flag. Uncompressed blocks are
    /// copied directly; compressed blocks return `NotSupported`.
    fn read_file_data(&self, inode: &CramfsInode, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let file_size = inode.size as u64;
        if offset >= file_size {
            return Ok(0);
        }

        let remaining = file_size - offset;
        let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;

        let data_off = inode.data_offset();
        let block_size = self.block_size as u64;

        // First block to read from.
        let first_block = offset / block_size;
        let off_in_first = (offset % block_size) as usize;

        let mut read = 0usize;
        let mut block_idx = first_block;
        let mut pos_in_block = off_in_first;

        while read < to_read {
            // Read the block descriptor (u32 at data_off + block_idx * 4).
            let desc_off = data_off + (block_idx as usize) * 4;
            if desc_off + 4 > self.image.len() {
                return Err(FsError::IoError);
            }
            let desc = read_le32(self.image, desc_off).ok_or(FsError::IoError)?;
            let blk_size = (desc & CRAMFS_BLK_SIZE_MASK) as usize;
            let is_compressed = (desc & CRAMFS_BLK_FLAG_COMPRESSED) != 0;

            // The block data starts after the entire block descriptor table.
            let num_blocks = (file_size + block_size - 1) / block_size;
            let block_data_start = data_off + (num_blocks as usize) * 4;
            let block_data_off = block_data_start + (block_idx as usize) * blk_size;

            if is_compressed {
                return Err(FsError::NotSupported);
            }

            if block_data_off + blk_size > self.image.len() {
                return Err(FsError::IoError);
            }

            let chunk = core::cmp::min(blk_size - pos_in_block, to_read - read);
            buffer[read..read + chunk].copy_from_slice(
                &self.image[block_data_off + pos_in_block..block_data_off + pos_in_block + chunk],
            );
            read += chunk;
            block_idx += 1;
            pos_in_block = 0;
        }

        Ok(read)
    }
}

impl FileSystem for CramfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RomFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.superblock.blocks as u64,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: self.superblock.files as u64,
            free_inodes: 0,
            block_size: self.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        if flags.write || flags.create || flags.truncate || flags.append || flags.exclusive {
            return Err(FsError::ReadOnly);
        }
        let offset = self.walk(path, 0)?;
        let inode = self.inode_at(offset).ok_or(FsError::NotFound)?;
        Ok(inode.inode_number())
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let header_off = inode as usize;
        let entry = self.inode_at(header_off).ok_or(FsError::NotFound)?;

        if entry.is_dir() {
            return Err(FsError::IsADirectory);
        }

        if entry.is_symlink() {
            let data_off = entry.data_offset();
            let size = entry.size as u64;
            if offset >= size {
                return Ok(0);
            }
            let remaining = size - offset;
            let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;
            let start = data_off + offset as usize;
            let end = start + to_read;
            if end > self.image.len() {
                return Err(FsError::IoError);
            }
            buffer[..to_read].copy_from_slice(&self.image[start..end]);
            return Ok(to_read);
        }

        self.read_file_data(&entry, offset, buffer)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let header_off = inode as usize;
        let entry = self.inode_at(header_off).ok_or(FsError::NotFound)?;
        Ok(self.metadata_for(&entry))
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
        let header_off = inode as usize;
        let dir = self.inode_at(header_off).ok_or(FsError::NotFound)?;
        if !dir.is_dir() {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        let mut child_off = dir.data_offset();
        let dir_end = child_off + dir.size as usize;

        while child_off + 16 <= dir_end && child_off + 16 <= self.image.len() {
            let child = self.inode_at(child_off).ok_or(FsError::IoError)?;
            if let Some(name) = self.name_at(child_off) {
                if !name.is_empty() && name != "." && name != ".." {
                    entries.push(DirectoryEntry {
                        name,
                        inode: child.inode_number(),
                        file_type: child.file_type(),
                    });
                }
            }
            let entry_size = self.inode_entry_size(child_off).ok_or(FsError::IoError)?;
            child_off += entry_size;
        }

        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let offset = self.walk(path, 0)?;
        let inode = self.inode_at(offset).ok_or(FsError::NotFound)?;
        if !inode.is_symlink() {
            return Err(FsError::InvalidArgument);
        }
        self.readlink_data(&inode)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}

// ============================================================================
// Little-endian helpers
// ============================================================================

fn read_le16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let bytes = data.get(offset..end)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes = data.get(offset..end)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
