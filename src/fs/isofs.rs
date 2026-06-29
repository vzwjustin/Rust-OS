//! ISO 9660 (CD-ROM) filesystem implementation
//!
//! Read-only access to ISO 9660 images with optional Rock Ridge extensions
//! (long filenames, POSIX modes, symlinks).

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::read_storage_sectors;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

const ISO_SECTOR_SIZE: u32 = 2048;
const PVD_SECTOR: u64 = 16;

/// ISO 9660 directory record (fixed 33-byte header before variable name).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct IsoDirectoryRecord {
    length: u8,
    ext_attr_length: u8,
    extent_lba_le: u32,
    extent_lba_be: u32,
    data_length_le: u32,
    data_length_be: u32,
    recording_time: [u8; 7],
    file_flags: u8,
    file_unit_size: u8,
    interleave_gap: u8,
    volume_seq_le: u16,
    volume_seq_be: u16,
    name_length: u8,
}

/// Parsed Rock Ridge / SUSP metadata attached to a directory record.
#[derive(Debug, Clone, Default)]
struct RrEntry {
    name: String,
    mode: Option<u16>,
    uid: Option<u32>,
    gid: Option<u32>,
    symlink_target: Option<String>,
    is_symlink: bool,
    is_dir: bool,
    extent: u32,
    size: u32,
}

#[derive(Debug, Clone)]
struct IsoNode {
    inode: InodeNumber,
    extent: u32,
    size: u32,
    is_dir: bool,
    is_symlink: bool,
    symlink_target: Option<String>,
    rel_path: String,
    mode: u16,
}

/// ISO 9660 filesystem backed by a block storage device.
#[derive(Debug)]
pub struct Iso9660FileSystem {
    device_id: u32,
    root_extent: u32,
    root_size: u32,
    volume_blocks: u64,
    block_size: u32,
    rock_ridge: bool,
    joliet: bool,
    inodes: RwLock<BTreeMap<InodeNumber, IsoNode>>,
    next_inode: RwLock<u64>,
}

impl Iso9660FileSystem {
    /// Probe and mount an ISO 9660 image on `device_id`.
    pub fn new(device_id: u32) -> Result<Self, FsError> {
        let mut pvd_buf = [0u8; 2048];
        read_storage_sectors(device_id, PVD_SECTOR * 4, &mut pvd_buf)
            .map_err(|_| FsError::IoError)?;

        if &pvd_buf[1..6] != b"CD001" || pvd_buf[0] != 1 {
            return Err(FsError::NotSupported);
        }

        let volume_blocks = u64::from(u32::from_le_bytes(pvd_buf[80..84].try_into().unwrap()));
        let block_size =
            u32::from_le_bytes(pvd_buf[128..130].try_into().unwrap()).max(ISO_SECTOR_SIZE);

        let root_rec_ptr = &pvd_buf[156] as *const u8 as *const IsoDirectoryRecord;
        let root_rec = unsafe { core::ptr::read_unaligned(root_rec_ptr) };
        let root_extent = root_rec.extent_lba_le;
        let root_size = root_rec.data_length_le;

        let rock_ridge = detect_rock_ridge(&pvd_buf[883..]);
        let (joliet, joliet_root_extent, joliet_root_size) = probe_joliet(device_id)?;

        let use_joliet = joliet;
        let (effective_root_extent, effective_root_size) = if use_joliet {
            (joliet_root_extent, joliet_root_size)
        } else {
            (root_extent, root_size)
        };

        let s = Self {
            device_id,
            root_extent: effective_root_extent,
            root_size: effective_root_size,
            volume_blocks,
            block_size,
            rock_ridge: rock_ridge || use_joliet,
            joliet: use_joliet,
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(2),
        };

        s.inodes.write().insert(
            1,
            IsoNode {
                inode: 1,
                extent: effective_root_extent,
                size: effective_root_size,
                is_dir: true,
                is_symlink: false,
                symlink_target: None,
                rel_path: String::new(),
                mode: 0o755,
            },
        );

        Ok(s)
    }

    fn read_sectors(&self, lba: u32, count: u32) -> FsResult<Vec<u8>> {
        let bytes = (count as u64) * (self.block_size as u64);
        let mut data = Vec::with_capacity(bytes as usize);
        data.resize(bytes as usize, 0);
        let sectors512 = lba as u64 * (self.block_size as u64 / 512);
        read_storage_sectors(self.device_id, sectors512, &mut data)
            .map_err(|_| FsError::IoError)?;
        Ok(data)
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<IsoNode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    fn normalize_iso_name(&self, raw: &[u8]) -> String {
        if raw == [0] {
            return ".".to_string();
        }
        if raw == [1] {
            return "..".to_string();
        }
        let mut name = String::from_utf8_lossy(raw).into_owned();
        if let Some(idx) = name.rfind(';') {
            name.truncate(idx);
        }
        if name.ends_with('.') {
            name.pop();
        }
        if !self.rock_ridge {
            name = name.to_lowercase();
        }
        name
    }

    fn parse_directory_record(&self, data: &[u8], offset: usize) -> Option<(RrEntry, usize)> {
        if offset >= data.len() {
            return None;
        }
        let record_len = data[offset] as usize;
        if record_len == 0 || offset + record_len > data.len() || record_len < 33 {
            return None;
        }

        let rec_ptr = &data[offset] as *const u8 as *const IsoDirectoryRecord;
        let rec = unsafe { core::ptr::read_unaligned(rec_ptr) };
        let name_len = rec.name_length as usize;
        if offset + 33 + name_len > data.len() {
            return None;
        }

        let name_bytes = &data[offset + 33..offset + 33 + name_len];
        let mut entry = RrEntry {
            name: self.normalize_iso_name(name_bytes),
            is_dir: (rec.file_flags & 0x02) != 0,
            is_symlink: (rec.file_flags & 0x04) != 0,
            extent: rec.extent_lba_le,
            size: rec.data_length_le,
            ..Default::default()
        };

        let sua_start = offset + 33 + name_len + (name_len & 1);
        if sua_start < offset + record_len {
            parse_susp(
                &data[sua_start..offset + record_len],
                &mut entry,
                Some(self),
                rec.extent_lba_le,
            );
        }

        Some((entry, record_len))
    }

    fn read_dir_entries(&self, extent: u32, size: u32) -> FsResult<Vec<RrEntry>> {
        let sector_count = (size + self.block_size - 1) / self.block_size;
        let data = self.read_sectors(extent, sector_count)?;
        let mut entries = Vec::new();
        let mut offset = 0usize;
        let limit = size as usize;

        while offset < limit && offset < data.len() {
            let record_len = data[offset] as usize;
            if record_len == 0 {
                let sector_offset = offset % self.block_size as usize;
                if sector_offset == 0 {
                    break;
                }
                offset += self.block_size as usize - sector_offset;
                continue;
            }

            if let Some((entry, len)) = self.parse_directory_record(&data, offset) {
                if entry.name != "." && entry.name != ".." {
                    entries.push(entry);
                }
                offset += len;
            } else {
                break;
            }
        }

        Ok(entries)
    }

    fn names_match(&self, a: &str, b: &str) -> bool {
        if self.rock_ridge {
            a == b
        } else {
            a.eq_ignore_ascii_case(b)
        }
    }

    fn resolve_path(&self, rel_path: &str) -> FsResult<RrEntry> {
        if rel_path.is_empty() {
            return Ok(RrEntry {
                name: String::new(),
                is_dir: true,
                extent: self.root_extent,
                size: self.root_size,
                mode: Some(0o755),
                ..Default::default()
            });
        }

        let parts: Vec<&str> = rel_path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_extent = self.root_extent;
        let mut current_size = self.root_size;
        let mut current_is_dir = true;

        for (idx, part) in parts.iter().enumerate() {
            if !current_is_dir {
                return Err(FsError::NotFound);
            }

            let dir_entries = self.read_dir_entries(current_extent, current_size)?;
            let mut found = None;
            for entry in dir_entries {
                if self.names_match(&entry.name, part) {
                    found = Some(entry);
                    break;
                }
            }

            let entry = found.ok_or(FsError::NotFound)?;
            current_extent = entry.extent;
            current_size = entry.size;
            current_is_dir = entry.is_dir;

            if idx + 1 == parts.len() {
                return Ok(entry);
            }
        }

        Err(FsError::NotFound)
    }

    fn alloc_or_find_inode(&self, entry: &RrEntry, rel_path: &str) -> InodeNumber {
        {
            let inodes = self.inodes.read();
            for (&ino, node) in inodes.iter() {
                if node.extent == entry.extent && node.is_dir == entry.is_dir {
                    return ino;
                }
            }
        }

        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;

        self.inodes.write().insert(
            inode,
            IsoNode {
                inode,
                extent: entry.extent,
                size: entry.size,
                is_dir: entry.is_dir,
                is_symlink: entry.is_symlink,
                symlink_target: entry.symlink_target.clone(),
                rel_path: rel_path.to_string(),
                mode: entry
                    .mode
                    .unwrap_or(if entry.is_dir { 0o755 } else { 0o444 }),
            },
        );

        inode
    }
}

fn detect_rock_ridge(pvd_sua: &[u8]) -> bool {
    let mut offset = 0usize;
    while offset + 4 <= pvd_sua.len() {
        let sig = &pvd_sua[offset..offset + 2];
        let len = pvd_sua[offset + 2] as usize;
        if len < 4 || offset + len > pvd_sua.len() {
            break;
        }
        if sig == b"SP" || sig == b"RR" {
            return true;
        }
        offset += len;
    }
    false
}

fn probe_joliet(device_id: u32) -> Result<(bool, u32, u32), FsError> {
    for sector in 16..32 {
        let mut buf = [0u8; 2048];
        read_storage_sectors(device_id, sector * 4, &mut buf).map_err(|_| FsError::IoError)?;
        if &buf[1..6] != b"CD001" {
            continue;
        }
        if buf[0] != 2 {
            continue;
        }
        if buf[88] != b'%' || buf[89] != b'/' {
            continue;
        }
        // UCS-2 level indicator at byte 90: @=level1, C=level2, E=level3
        let level = buf[90];
        if level != b'@' && level != b'C' && level != b'E' {
            continue;
        }
        let rec_ptr = &buf[156] as *const u8 as *const IsoDirectoryRecord;
        let rec = unsafe { core::ptr::read_unaligned(rec_ptr) };
        return Ok((true, rec.extent_lba_le, rec.data_length_le));
    }
    Ok((false, 0, 0))
}

fn parse_susp(data: &[u8], entry: &mut RrEntry, fs: Option<&Iso9660FileSystem>, extent: u32) {
    let mut offset = 0usize;
    while offset + 4 <= data.len() {
        let sig = &data[offset..offset + 2];
        let len = data[offset + 2] as usize;
        if len < 4 || offset + len > data.len() {
            break;
        }
        let body = &data[offset + 4..offset + len];
        match sig {
            b"SP" if len >= 7 => {
                if body.len() >= 3 && body[0] == 0xBE && body[1] == 0xEF {
                    // Rock Ridge sharing protocol present.
                }
            }
            b"RR" if !body.is_empty() => {
                // RR flags present — treat as Rock Ridge directory.
            }
            b"NM" => {
                if let Some(flags) = body.first() {
                    let continue_name = (*flags & 0x04) != 0;
                    let name_bytes = if body.len() > 1 { &body[1..] } else { &[] };
                    let piece = String::from_utf8_lossy(name_bytes);
                    if continue_name || !entry.name.is_empty() {
                        entry.name.push_str(&piece);
                    } else {
                        entry.name = piece.into_owned();
                    }
                }
            }
            b"PX" if body.len() >= 8 => {
                entry.mode = Some(u16::from_le_bytes(body[0..2].try_into().unwrap()) & 0o7777);
                entry.uid = Some(u32::from_le_bytes(body[4..8].try_into().unwrap()));
                if body.len() >= 12 {
                    entry.gid = Some(u32::from_le_bytes(body[8..12].try_into().unwrap()));
                }
            }
            b"SL" => {
                entry.is_symlink = true;
                entry.symlink_target = Some(parse_symlink_components(body));
            }
            b"CE" if body.len() >= 24 => {
                let lba = u32::from_le_bytes(body[0..4].try_into().unwrap());
                let pos = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
                let total = u32::from_le_bytes(body[8..12].try_into().unwrap()) as usize;
                if let Some(fs) = fs {
                    if let Ok(sectors) = fs.read_sectors(lba, 1) {
                        let end = core::cmp::min(pos + total, sectors.len());
                        if pos < end {
                            parse_susp(&sectors[pos..end], entry, Some(fs), extent);
                        }
                    }
                }
            }
            b"ST" => break,
            _ => {}
        }
        offset += len;
    }
}

fn parse_symlink_components(body: &[u8]) -> String {
    let mut target = String::new();
    let mut idx = 1usize; // skip flags byte
    while idx < body.len() {
        let comp_flags = body[idx];
        idx += 1;
        if idx >= body.len() {
            break;
        }
        let comp_len = body[idx] as usize;
        idx += 1;
        if idx + comp_len > body.len() {
            break;
        }
        let comp = String::from_utf8_lossy(&body[idx..idx + comp_len]);
        idx += comp_len;
        if (comp_flags & 0x08) != 0 {
            target.push('/');
        }
        if (comp_flags & 0x04) != 0 {
            target.push_str("..");
        } else if (comp_flags & 0x02) != 0 {
            target.push('.');
        } else {
            target.push_str(&comp);
        }
    }
    target
}

impl FileSystem for Iso9660FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Iso9660
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.volume_blocks,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: self.inodes.read().len() as u64,
            free_inodes: 0,
            block_size: self.block_size,
            max_filename_length: if self.rock_ridge { 255 } else { 37 },
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        if rel_path.is_empty() {
            return Ok(1);
        }

        let entry = self.resolve_path(rel_path)?;
        let inode = self.alloc_or_find_inode(&entry, rel_path);
        Ok(inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_node(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        if node.is_symlink {
            return Err(FsError::InvalidArgument);
        }

        if offset >= node.size as u64 {
            return Ok(0);
        }

        let start = offset as usize;
        let end = core::cmp::min(node.size as usize, start + buffer.len());
        let to_read = end - start;

        let block = self.block_size as usize;
        let start_lba = node.extent + (start / block) as u32;
        let end_lba = node.extent + ((end + block - 1) / block) as u32;
        let sector_count = end_lba - start_lba;

        let data = self.read_sectors(start_lba, sector_count)?;
        let data_offset = start % block;
        buffer[..to_read].copy_from_slice(&data[data_offset..data_offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        let file_type = if node.is_symlink {
            FileType::SymbolicLink
        } else if node.is_dir {
            FileType::Directory
        } else {
            FileType::Regular
        };

        Ok(FileMetadata {
            inode,
            file_type,
            size: node.size as u64,
            permissions: FilePermissions::from_octal(node.mode),
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
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

        let raw_entries = self.read_dir_entries(node.extent, node.size)?;
        let mut entries = Vec::with_capacity(raw_entries.len() + 2);

        entries.push(DirectoryEntry {
            name: ".".to_string(),
            inode,
            file_type: FileType::Directory,
        });
        entries.push(DirectoryEntry {
            name: "..".to_string(),
            inode: 1,
            file_type: FileType::Directory,
        });

        for entry in raw_entries {
            let rel = if node.rel_path.is_empty() {
                entry.name.clone()
            } else {
                alloc::format!("{}/{}", node.rel_path, entry.name)
            };
            let child_inode = self.alloc_or_find_inode(&entry, &rel);
            let file_type = if entry.is_symlink {
                FileType::SymbolicLink
            } else if entry.is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            };
            entries.push(DirectoryEntry {
                name: entry.name,
                inode: child_inode,
                file_type,
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
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        let entry = self.resolve_path(rel_path)?;
        if entry.is_symlink {
            entry.symlink_target.ok_or(FsError::NotSupported)
        } else {
            let node = self.get_node(self.open(path, OpenFlags::read_only())?)?;
            node.symlink_target.ok_or(FsError::NotSupported)
        }
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
