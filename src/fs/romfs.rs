//! ROMFS read-only filesystem implementation
//!
//! This module implements a real ROMFS format parser. ROMFS is a simple,
//! read-only filesystem commonly used in embedded systems and initramfs.
//!
//! On-disk layout:
//! - Superblock: 8-byte magic `-rom1fs-`, big-endian total size (u32),
//!   checksum (u32), then the volume name (null-terminated, 16-byte aligned).
//! - Each node has a 16-byte-aligned header:
//!   `next` (u32 BE, low 4 bits = type, high 28 bits = next-sibling offset),
//!   `spec.info` (u32 BE, type-specific), `checksum` (u32 BE), then the
//!   null-terminated name padded to a 16-byte boundary.
//! - File data follows the header, 16-byte aligned.
//!
//! Inode numbers are byte offsets into the image, matching Linux ROMFS.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

/// ROMFS magic identifier.
const ROMFS_MAGIC: &[u8] = b"-rom1fs-";

/// ROMFS node type codes (low nibble of the `next` field).
const ROMFH_HRD: u32 = 0; // hard link
const ROMFH_DIR: u32 = 1; // directory
const ROMFH_REG: u32 = 2; // regular file
const ROMFH_LNK: u32 = 3; // symbolic link
const ROMFH_BLK: u32 = 4; // block device
const ROMFH_CHR: u32 = 5; // character device
const ROMFH_SCK: u32 = 6; // socket
const ROMFH_FIF: u32 = 7; // fifo

/// Round `x` up to the next 16-byte boundary.
fn align16(x: usize) -> usize {
    (x + 15) & !15
}

/// ROMFS filesystem instance backed by an in-memory image.
#[derive(Debug)]
pub struct RomfsFileSystem {
    /// Device identifier (for device files).
    device_id: u32,
    /// The full ROMFS image.
    image: &'static [u8],
    /// Total size of the filesystem in bytes (from the superblock).
    total_size: u32,
    /// Inode (byte offset) of the root directory entry.
    root_inode: InodeNumber,
}

/// Parsed node header.
#[derive(Debug, Clone, Copy)]
struct RomNode {
    /// Byte offset of this node in the image.
    offset: usize,
    /// `next` field (raw, including type bits).
    next: u32,
    /// `spec.info` field.
    spec: u32,
    /// File type code (low nibble of `next`).
    node_type: u32,
}

impl RomNode {
    /// Offset of the next sibling (0 means end of chain).
    fn next_offset(&self) -> usize {
        (self.next & !0xf) as usize
    }

    /// True if this is the last entry in its sibling chain.
    fn is_last(&self) -> bool {
        self.next_offset() == 0
    }

    /// File size for regular files / symlinks (stored in `spec.info`).
    fn size(&self) -> u64 {
        self.spec as u64
    }

    /// Offset of the first child for directories (stored in `spec.info`).
    fn child_offset(&self) -> usize {
        self.spec as usize
    }

    /// Map the ROMFS type code to a `FileType`.
    fn file_type(&self) -> FileType {
        match self.node_type {
            ROMFH_DIR => FileType::Directory,
            ROMFH_REG => FileType::Regular,
            ROMFH_LNK => FileType::SymbolicLink,
            ROMFH_BLK => FileType::BlockDevice,
            ROMFH_CHR => FileType::CharacterDevice,
            ROMFH_FIF => FileType::NamedPipe,
            ROMFH_SCK => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Byte offset where file data begins (after the aligned name).
    fn data_offset(&self, image: &[u8]) -> usize {
        let name_len = Self::name_len(image, self.offset);
        self.offset + align16(12 + name_len + 1)
    }

    /// Length of the null-terminated name (excluding the null byte).
    fn name_len(image: &[u8], offset: usize) -> usize {
        let mut i = offset + 12;
        while i < image.len() {
            if image[i] == 0 {
                return i - (offset + 12);
            }
            i += 1;
        }
        image.len() - (offset + 12)
    }

    /// Read the null-terminated name as a `String`.
    fn name(&self, image: &[u8]) -> String {
        let start = self.offset + 12;
        let len = Self::name_len(image, self.offset);
        core::str::from_utf8(&image[start..start + len])
            .unwrap_or("")
            .to_string()
    }
}

impl RomfsFileSystem {
    /// Create a new ROMFS filesystem instance from a static image.
    ///
    /// Validates the magic, reads the big-endian total size, and locates
    /// the root directory entry past the volume name.
    pub fn new(device_id: u32, image: &'static [u8]) -> FsResult<Self> {
        if image.len() < 16 + 8 {
            return Err(FsError::InvalidArgument);
        }
        if &image[0..8] != ROMFS_MAGIC {
            return Err(FsError::IoError);
        }

        // Big-endian total size.
        let total_size = Self::read_be_u32(image, 8);

        // Volume name starts at offset 16, null-terminated, 16-byte aligned.
        let volname_start = 16;
        let mut name_end = volname_start;
        while name_end < image.len() && image[name_end] != 0 {
            name_end += 1;
        }
        if name_end >= image.len() {
            return Err(FsError::IoError);
        }
        let volname_field_len = name_end - volname_start + 1; // include null
        let root_offset = volname_start + align16(volname_field_len);

        if root_offset + 16 > image.len() {
            return Err(FsError::IoError);
        }

        Ok(Self {
            device_id,
            image,
            total_size,
            root_inode: root_offset as InodeNumber,
        })
    }

    /// Read a big-endian u32 from the image.
    fn read_be_u32(image: &[u8], offset: usize) -> u32 {
        ((image[offset] as u32) << 24)
            | ((image[offset + 1] as u32) << 16)
            | ((image[offset + 2] as u32) << 8)
            | (image[offset + 3] as u32)
    }

    /// Parse the node header at the given byte offset.
    fn read_node(&self, offset: usize) -> FsResult<RomNode> {
        if offset + 12 > self.image.len() {
            return Err(FsError::NotFound);
        }
        let next = Self::read_be_u32(self.image, offset);
        let spec = Self::read_be_u32(self.image, offset + 4);
        Ok(RomNode {
            offset,
            next,
            spec,
            node_type: next & 0xf,
        })
    }

    /// Iterate over the children of a directory node.
    ///
    /// `dir_offset` is the byte offset of the directory entry. The first
    /// child is at `spec.info`; siblings are chained via the `next` field.
    fn iter_children(&self, dir_offset: usize) -> Vec<RomNode> {
        let mut children = Vec::new();
        let dir = match self.read_node(dir_offset) {
            Ok(d) => d,
            Err(_) => return children,
        };
        let mut cur = dir.child_offset();
        while cur != 0 && cur + 12 <= self.image.len() {
            let node = match self.read_node(cur) {
                Ok(n) => n,
                Err(_) => break,
            };
            children.push(node);
            if node.is_last() {
                break;
            }
            cur = node.next_offset();
        }
        children
    }

    /// Look up a single name within a directory's children.
    fn lookup_in_dir(&self, dir_offset: usize, name: &str) -> FsResult<usize> {
        for child in self.iter_children(dir_offset) {
            if child.name(self.image) == name {
                return Ok(child.offset);
            }
        }
        Err(FsError::NotFound)
    }

    /// Walk a path from the root and return the resulting node offset.
    fn resolve(&self, path: &str) -> FsResult<usize> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = self.root_inode as usize;
        for part in parts {
            let node = self.read_node(current)?;
            if node.node_type != ROMFH_DIR {
                return Err(FsError::NotADirectory);
            }
            current = self.lookup_in_dir(current, part)?;
        }
        Ok(current)
    }
}

impl FileSystem for RomfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RomFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let block_size = 512u32;
        let total_blocks = (self.total_size as u64) / block_size as u64;
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let offset = self.resolve(path)?;
        Ok(offset as InodeNumber)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.read_node(inode as usize)?;
        if node.node_type == ROMFH_DIR {
            return Err(FsError::IsADirectory);
        }
        let data_off = node.data_offset(self.image);
        let size = node.size();
        if offset >= size {
            return Ok(0);
        }
        let remaining = (size - offset) as usize;
        let to_read = core::cmp::min(buffer.len(), remaining);
        let src_start = data_off + offset as usize;
        if src_start + to_read > self.image.len() {
            return Err(FsError::IoError);
        }
        buffer[..to_read].copy_from_slice(&self.image[src_start..src_start + to_read]);
        Ok(to_read)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.read_node(inode as usize)?;
        let file_type = node.file_type();
        let size = if file_type == FileType::Directory {
            0
        } else {
            node.size()
        };
        let permissions = match file_type {
            FileType::Directory => FilePermissions::default_directory(),
            FileType::SymbolicLink => FilePermissions::from_octal(0o777),
            _ => FilePermissions::from_octal(0o444),
        };
        let device_id = match file_type {
            FileType::BlockDevice | FileType::CharacterDevice => {
                Some((node.spec >> 16) as u32)
            }
            _ => None,
        };
        Ok(FileMetadata {
            inode,
            file_type,
            size,
            permissions,
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id,
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
        let node = self.read_node(inode as usize)?;
        if node.node_type != ROMFH_DIR {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for child in self.iter_children(inode as usize) {
            entries.push(DirectoryEntry {
                name: child.name(self.image),
                inode: child.offset as InodeNumber,
                file_type: child.file_type(),
            });
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
        let offset = self.resolve(path)?;
        let node = self.read_node(offset)?;
        if node.node_type != ROMFH_LNK {
            return Err(FsError::InvalidArgument);
        }
        let data_off = node.data_offset(self.image);
        let size = node.size() as usize;
        if data_off + size > self.image.len() {
            return Err(FsError::IoError);
        }
        let bytes = &self.image[data_off..data_off + size];
        core::str::from_utf8(bytes)
            .map(|s| s.to_string())
            .map_err(|_| FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // ROMFS is read-only; nothing to sync.
        Ok(())
    }
}

/// Expose the device id for callers that need it.
impl RomfsFileSystem {
    /// Return the device id this filesystem was mounted with.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }
}
