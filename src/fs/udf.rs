//! UDF (Universal Disk Format) optical media filesystem implementation.
//!
//! UDF is the ECMA-167-based filesystem used on DVDs, Blu-ray discs and some
//! USB sticks. This is a read-only driver that parses the on-disk descriptor
//! hierarchy and exposes file/directory/symlink access through the VFS trait.
//!
//! Descriptor chain parsed at mount time:
//!
//! ```text
//! Anchor VDP (sector 256)
//!   └─► Volume Descriptor Sequence
//!         ├─ Primary Volume Descriptor      (volume id)
//!         ├─ Partition Descriptor           (partition start/length/number)
//!         ├─ Logical Volume Descriptor      (block size, partition maps,
//!         │                                 file-set descriptor location)
//!         └─ Terminating Descriptor
//! File Set Descriptor
//!   └─► root directory ICB (Information Control Block)
//!         └─► File Identifier Descriptors  (directory entries)
//!               └─► child ICBs (File Entry / Extended File Entry)
//!                     └─► allocation descriptors (short/long/extended/inline)
//! ```
//!
//! All multi-byte fields on disk are little-endian (ECMA-167). The in-memory
//! image is treated as a flat byte buffer indexed by logical block; the inode
//! number encodes the ICB location as `(partition_ref << 32) | logical_block`
//! so that `open`/`read`/`metadata`/`readdir` can re-locate any entry without
//! an external inode table.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Logical block size used for the anchor / VDS region. UDF on optical media
/// always uses 2048-byte sectors; this is also the value stored in the Logical
/// Volume Descriptor for the vast majority of images.
const DESC_BLOCK_SIZE: usize = 2048;
/// Logical block number of the Anchor Volume Descriptor Pointer (ECMA-167).
const ANCHOR_BLOCK: u64 = 256;
/// Maximum number of allocation-extent hops when gathering file extents.
const MAX_EXTENT_DEPTH: usize = 8;
/// Maximum number of symlink hops during path resolution.
const MAX_SYMLINK_DEPTH: usize = 8;

// Descriptor tag identifiers (ECMA-167 3/7.2.1 and 4/7.2.1).
const TAG_PVD: u16 = 1; // Primary Volume Descriptor
const TAG_AVDP: u16 = 2; // Anchor Volume Descriptor Pointer
const TAG_PD: u16 = 4; // Partition Descriptor
const TAG_LVD: u16 = 5; // Logical Volume Descriptor
const TAG_TD: u16 = 7; // Terminating Descriptor
const TAG_FSD: u16 = 256; // File Set Descriptor
const TAG_FID: u16 = 257; // File Identifier Descriptor
const TAG_AED: u16 = 258; // Allocation Extent Descriptor
const TAG_FE: u16 = 261; // File Entry
const TAG_EFE: u16 = 266; // Extended File Entry

// ICB file types (ECMA-167 4/14.6.6).
const ICB_FILE_TYPE_DIRECTORY: u8 = 4;
const ICB_FILE_TYPE_REGULAR: u8 = 5;
const ICB_FILE_TYPE_BLOCK: u8 = 6;
const ICB_FILE_TYPE_CHAR: u8 = 7;
const ICB_FILE_TYPE_FIFO: u8 = 9;
const ICB_FILE_TYPE_SOCKET: u8 = 10;
const ICB_FILE_TYPE_SYMLINK: u8 = 12;
const ICB_FILE_TYPE_STREAMDIR: u8 = 13;

// ICB flags: allocation descriptor type (low 3 bits, ECMA-167 4/14.6.8).
const ICB_AD_SHORT: u16 = 0;
const ICB_AD_LONG: u16 = 1;
const ICB_AD_EXTENDED: u16 = 2;
const ICB_AD_INLINE: u16 = 3;
const ICB_AD_TYPE_MASK: u16 = 0x07;

// File Identifier Descriptor characteristics (ECMA-167 4/14.4.8).
const FID_PARENT: u8 = 0x02;
const FID_DIRECTORY: u8 = 0x04;
const FID_DELETED: u8 = 0x08;

// Allocation descriptor extent type (top 2 bits of the length field).
const AD_TYPE_MASK: u32 = 0xC000_0000;
const AD_LEN_MASK: u32 = 0x3FFF_FFFF;
const AD_RECORDED: u32 = 0x0000_0000;
const AD_NOT_RECORDED_ALLOC: u32 = 0x4000_0000;
const AD_NOT_RECORDED_NOALLOC: u32 = 0x8000_0000;
const AD_NEXT_EXTENT: u32 = 0xC000_0000;

// ---------------------------------------------------------------------------
// Little-endian primitive readers (bounds-checked, no unsafe).
// ---------------------------------------------------------------------------

fn le16(data: &[u8], off: usize) -> Option<u16> {
    if off.checked_add(2)? > data.len() {
        return None;
    }
    Some(u16::from_le_bytes([data[off], data[off + 1]]))
}

fn le32(data: &[u8], off: usize) -> Option<u32> {
    if off.checked_add(4)? > data.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        data[off],
        data[off + 1],
        data[off + 2],
        data[off + 3],
    ]))
}

fn le64(data: &[u8], off: usize) -> Option<u64> {
    if off.checked_add(8)? > data.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        data[off],
        data[off + 1],
        data[off + 2],
        data[off + 3],
        data[off + 4],
        data[off + 5],
        data[off + 6],
        data[off + 7],
    ]))
}

fn u8_at(data: &[u8], off: usize) -> Option<u8> {
    data.get(off).copied()
}

/// Round `value` up to the next multiple of `align` (align must be a power of two).
fn align_up(value: usize, align: usize) -> usize {
    (value + (align - 1)) & !(align - 1)
}

// ---------------------------------------------------------------------------
// On-disk structure parsers
// ---------------------------------------------------------------------------

/// Descriptor tag (ECMA-167 3/7.2). Only the tag identifier is used here.
#[derive(Debug, Clone, Copy)]
struct DescriptorTag {
    tag_identifier: u16,
    #[allow(dead_code)]
    descriptor_version: u16,
    tag_location: u32,
}

impl DescriptorTag {
    fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            tag_identifier: le16(data, 0)?,
            descriptor_version: le16(data, 2)?,
            tag_location: le32(data, 12)?,
        })
    }
}

/// Anchor Volume Descriptor Pointer (ECMA-167 3/10.2, tag 2).
#[derive(Debug, Clone, Copy)]
struct AnchorVdp {
    /// Main Volume Descriptor Sequence extent (length, location).
    main_length: u32,
    main_location: u32,
    /// Reserve VDS extent (used if the main one is corrupt).
    #[allow(dead_code)]
    reserve_length: u32,
    #[allow(dead_code)]
    reserve_location: u32,
}

impl AnchorVdp {
    /// Extent descriptor = { u32 length; u32 location; } (8 bytes).
    fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            main_length: le32(data, 16)?,
            main_location: le32(data, 20)?,
            reserve_length: le32(data, 24)?,
            reserve_location: le32(data, 28)?,
        })
    }
}

/// Primary Volume Descriptor (ECMA-167 3/10.1, tag 1).
#[derive(Debug, Clone)]
struct PrimaryVolDesc {
    volume_identifier: String,
    #[allow(dead_code)]
    vol_desc_seq_number: u32,
}

impl PrimaryVolDesc {
    fn parse(data: &[u8]) -> Option<Self> {
        let vol_desc_seq_number = le32(data, 16)?;
        // Volume Identifier is a 32-byte d-string at offset 24.
        let volume_identifier = decode_dstring(&data[24..56]);
        Some(Self {
            volume_identifier,
            vol_desc_seq_number,
        })
    }
}

/// Partition Descriptor (ECMA-167 3/10.5, tag 4).
#[derive(Debug, Clone, Copy)]
struct PartitionDesc {
    partition_number: u16,
    partition_start: u32,
    partition_length: u32,
}

impl PartitionDesc {
    fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            partition_number: le16(data, 22)?,
            // ContentsUse is 128 bytes (offsets 56..184); access type at 184.
            partition_start: le32(data, 188)?,
            partition_length: le32(data, 192)?,
        })
    }
}

/// Logical Volume Descriptor (ECMA-167 3/10.6, tag 5).
#[derive(Debug, Clone)]
struct LogicalVolDesc {
    logical_block_size: u32,
    /// File Set Descriptor location (long_ad in `logicalVolContentsUse`).
    fsd_length: u32,
    fsd_lba: u32,
    fsd_part: u16,
    map_table_length: u32,
    num_partition_maps: u32,
}

impl LogicalVolDesc {
    fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            logical_block_size: le32(data, 212)?,
            // logicalVolContentsUse is a 16-byte long_ad at offset 248.
            fsd_length: le32(data, 248)?,
            fsd_lba: le32(data, 252)?,
            fsd_part: le16(data, 256)?,
            map_table_length: le32(data, 264)?,
            num_partition_maps: le32(data, 268)?,
        })
    }
}

/// File Set Descriptor (ECMA-167 4/14.1, tag 256).
#[derive(Debug, Clone, Copy)]
struct FileSetDesc {
    /// Root directory ICB location (long_ad at offset 528).
    root_length: u32,
    root_lba: u32,
    root_part: u16,
}

impl FileSetDesc {
    fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            root_length: le32(data, 528)?,
            root_lba: le32(data, 532)?,
            root_part: le16(data, 536)?,
        })
    }
}

/// ICB tag (ECMA-167 4/14.6), embedded at the start of a File/Extended File Entry.
#[derive(Debug, Clone, Copy)]
struct IcbTag {
    file_type: u8,
    /// Allocation descriptor type (low 3 bits of the flags field).
    ad_type: u16,
    #[allow(dead_code)]
    link_count: u16,
}

impl IcbTag {
    /// Parse the ICB tag which begins at `base` within `data`.
    fn parse(data: &[u8], base: usize) -> Option<Self> {
        Some(Self {
            // fileType is at ICBTag offset 11.
            file_type: u8_at(data, base + 11)?,
            // flags at ICBTag offset 18.
            ad_type: le16(data, base + 18)? & ICB_AD_TYPE_MASK,
            link_count: le16(data, base + 18).unwrap_or(1),
        })
    }
}

/// Parsed File Entry / Extended File Entry (the ICB record).
#[derive(Debug, Clone)]
struct IcbEntry {
    /// Location of this ICB (used as the inode identity).
    loc: IcbLoc,
    file_type: u8,
    ad_type: u16,
    info_len: u64,
    uid: u32,
    gid: u32,
    perm: u32,
    link_count: u16,
    /// Raw block bytes holding this ICB (needed for inline data & AD parsing).
    block: Vec<u8>,
    /// Byte offset within `block` where the allocation descriptor area starts.
    ad_offset: usize,
    /// Length in bytes of the allocation descriptor area.
    ad_len: u32,
    /// Length in bytes of the extended attribute area (precedes the AD area).
    #[allow(dead_code)]
    ea_len: u32,
    /// Whether this is an Extended File Entry (tag 266) vs File Entry (261).
    is_extended: bool,
}

impl IcbEntry {
    fn file_type_vfs(&self) -> FileType {
        match self.file_type {
            ICB_FILE_TYPE_DIRECTORY => FileType::Directory,
            ICB_FILE_TYPE_REGULAR => FileType::Regular,
            ICB_FILE_TYPE_SYMLINK => FileType::SymbolicLink,
            ICB_FILE_TYPE_BLOCK => FileType::BlockDevice,
            ICB_FILE_TYPE_CHAR => FileType::CharacterDevice,
            ICB_FILE_TYPE_FIFO => FileType::NamedPipe,
            ICB_FILE_TYPE_SOCKET => FileType::Socket,
            _ => FileType::Regular,
        }
    }

    /// Modification timestamp offset within the block (12-byte ECMA-167 timestamp).
    fn mtime_offset(&self) -> usize {
        if self.is_extended {
            // EFE: atime@80, mtime@92
            92
        } else {
            // FE: atime@72, mtime@84
            84
        }
    }
}

/// File Identifier Descriptor (ECMA-167 4/14.4, tag 257).
#[derive(Debug, Clone)]
struct FileIdentDesc {
    characteristics: u8,
    name_len: u8,
    /// Child ICB location (long_ad at offset 20).
    icb_lba: u32,
    icb_part: u16,
    #[allow(dead_code)]
    icb_length: u32,
    /// Decoded file name.
    name: String,
    /// Total byte length of this FID record (aligned to 4).
    record_len: usize,
}

impl FileIdentDesc {
    /// Parse a FID at `offset` within a directory data buffer.
    fn parse(data: &[u8], offset: usize) -> Option<Self> {
        if offset + 38 > data.len() {
            return None;
        }
        let characteristics = data[offset + 18];
        let name_len = data[offset + 19];
        let icb_length = le32(data, offset + 20)?;
        let icb_lba = le32(data, offset + 24)?;
        let icb_part = le16(data, offset + 28)?;
        let impl_use_len = le16(data, offset + 36)? as usize;

        let name_start = offset + 38 + impl_use_len;
        let name_end = name_start + name_len as usize;
        if name_end > data.len() {
            return None;
        }
        let name = decode_dstring(&data[name_start..name_end]);

        let record_len = align_up(38 + impl_use_len + name_len as usize, 4);
        Some(Self {
            characteristics,
            name_len,
            icb_lba,
            icb_part,
            icb_length,
            name,
            record_len,
        })
    }

    fn is_deleted(&self) -> bool {
        (self.characteristics & FID_DELETED) != 0
    }

    fn is_parent(&self) -> bool {
        (self.characteristics & FID_PARENT) != 0
    }

    fn is_directory(&self) -> bool {
        (self.characteristics & FID_DIRECTORY) != 0
    }
}

// ---------------------------------------------------------------------------
// Location helpers
// ---------------------------------------------------------------------------

/// An ICB location: partition reference + logical block within that partition.
/// Encoded as the VFS inode number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IcbLoc {
    part: u16,
    lba: u32,
}

impl IcbLoc {
    fn to_inode(self) -> InodeNumber {
        ((self.part as u64) << 32) | (self.lba as u64)
    }

    fn from_inode(ino: InodeNumber) -> Self {
        Self {
            part: (ino >> 32) as u16,
            lba: ino as u32,
        }
    }
}

/// A resolved data extent within a file.
#[derive(Debug, Clone, Copy)]
struct Extent {
    /// Logical byte offset within the file.
    logical_off: u64,
    /// Physical byte offset within the image (for out-of-line extents).
    phys_byte: u64,
    /// Length in bytes.
    len: u32,
    /// Whether this extent is a hole (reads as zero).
    is_hole: bool,
    /// Whether the data lives inside the ICB block (inline allocation).
    is_inline: bool,
}

// ---------------------------------------------------------------------------
// d-string decoding
// ---------------------------------------------------------------------------

/// Decode a UDF d-string (ECMA-167 1/7.2.12) into a Rust `String`.
///
/// The first byte is the compression identifier:
/// - `0x08` / `0xFE`: UTF-8 (OSTA compressed Unicode / OSD UTF-8).
/// - `0x10`: UTF-16 big-endian.
/// - `0xFF`: empty / unspecified.
fn decode_dstring(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let comp_id = bytes[0];
    let body = &bytes[1..];
    match comp_id {
        0x08 | 0xFE => String::from_utf8_lossy(body).into_owned(),
        0x10 => {
            let mut s = String::new();
            let mut i = 0;
            while i + 1 < body.len() {
                let code = u16::from_be_bytes([body[i], body[i + 1]]);
                if let Some(ch) = char::from_u32(code as u32) {
                    s.push(ch);
                }
                i += 2;
            }
            s
        }
        // Unknown compression id: best-effort UTF-8 lossy decode of the body.
        _ => String::from_utf8_lossy(body).into_owned(),
    }
}

// ---------------------------------------------------------------------------
// Timestamp conversion
// ---------------------------------------------------------------------------

/// Days from the Unix epoch (1970-01-01) to the given civil date.
/// Algorithm by Howard Hinnant (`days_from_civil`).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe as i64 - 719468
}

/// Convert a 12-byte ECMA-167 timestamp at `off` within `data` to Unix seconds.
fn timestamp_to_unix(data: &[u8], off: usize) -> u64 {
    if off + 8 > data.len() {
        return 0;
    }
    let year = le16(data, off + 1).unwrap_or(0) as i32;
    let month = data[off + 3] as u32;
    let day = data[off + 4] as u32;
    let hour = data[off + 5] as u32;
    let minute = data[off + 6] as u32;
    let second = data[off + 7] as u32;
    if year < 1970 || month == 0 || month > 12 || day == 0 {
        return 0;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return 0;
    }
    days as u64 * 86400 + hour as u64 * 3600 + minute as u64 * 60 + second as u64
}

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------

/// UDF read-only filesystem backed by an in-memory image slice.
#[derive(Debug)]
pub struct UdfFileSystem {
    device_id: u32,
    image: &'static [u8],
    /// Logical block size (from the Logical Volume Descriptor).
    block_size: u32,
    /// `partition_start[part_ref]` = physical start block of that partition.
    part_start: Vec<u32>,
    /// Root directory ICB location.
    root_icb: IcbLoc,
    /// Total filesystem size in logical blocks (for `statfs`).
    volume_blocks: u64,
    /// Volume identifier (from the Primary Volume Descriptor).
    volume_name: String,
}

impl UdfFileSystem {
    /// Create a new UDF filesystem instance backed by a memory-mapped image.
    ///
    /// `image` must point to a valid UDF image (typically a DVD/Blu-ray ISO
    /// held in memory) that lives for the static lifetime of the kernel.
    pub fn new(device_id: u32, image: &'static [u8]) -> FsResult<Self> {
        // --- Anchor Volume Descriptor Pointer at sector 256 ---------------
        let anchor_off = (ANCHOR_BLOCK * DESC_BLOCK_SIZE as u64) as usize;
        if anchor_off + DESC_BLOCK_SIZE > image.len() {
            return Err(FsError::IoError);
        }
        let anchor_buf = &image[anchor_off..anchor_off + DESC_BLOCK_SIZE];
        let anchor_tag = DescriptorTag::parse(anchor_buf).ok_or(FsError::IoError)?;
        if anchor_tag.tag_identifier != TAG_AVDP {
            return Err(FsError::NotSupported);
        }
        let anchor = AnchorVdp::parse(anchor_buf).ok_or(FsError::IoError)?;

        // --- Volume Descriptor Sequence ----------------------------------
        let mut partitions: BTreeMap<u16, (u32, u32)> = BTreeMap::new();
        let mut lvd: Option<LogicalVolDesc> = None;
        let mut volume_name = String::new();

        Self::parse_vds(image, anchor.main_location, anchor.main_length, &mut partitions, &mut lvd, &mut volume_name)
            .or_else(|_| {
                // Fall back to the reserve VDS extent if the main one is bad.
                Self::parse_vds(image, anchor.reserve_location, anchor.reserve_length, &mut partitions, &mut lvd, &mut volume_name)
            })?;

        let lvd = lvd.ok_or(FsError::IoError)?;
        let block_size = if lvd.logical_block_size != 0 {
            lvd.logical_block_size
        } else {
            DESC_BLOCK_SIZE as u32
        };

        // --- Build partition reference -> physical start mapping ----------
        let part_start = Self::build_partition_starts(&lvd, &partitions, image, block_size)?;

        // --- File Set Descriptor -> root directory ICB --------------------
        let fsd_phys = Self::resolve_phys(&part_start, lvd.fsd_part, lvd.fsd_lba, block_size)?;
        let fsd_buf = Self::slice_block(image, fsd_phys, block_size)?;
        let fsd_tag = DescriptorTag::parse(fsd_buf).ok_or(FsError::IoError)?;
        if fsd_tag.tag_identifier != TAG_FSD {
            return Err(FsError::IoError);
        }
        let fsd = FileSetDesc::parse(fsd_buf).ok_or(FsError::IoError)?;
        let root_icb = IcbLoc {
            part: fsd.root_part,
            lba: fsd.root_lba,
        };

        // --- Volume size (sum of partition lengths) -----------------------
        let volume_blocks = partitions.values().map(|(_, len)| *len as u64).sum();

        Ok(Self {
            device_id,
            image,
            block_size,
            part_start,
            root_icb,
            volume_blocks,
            volume_name,
        })
    }

    /// Walk one Volume Descriptor Sequence, populating `partitions`, `lvd` and
    /// `volume_name`. Descriptors are laid out one per block starting at
    /// `start_block` for `length` bytes.
    fn parse_vds(
        image: &[u8],
        start_block: u32,
        length: u32,
        partitions: &mut BTreeMap<u16, (u32, u32)>,
        lvd: &mut Option<LogicalVolDesc>,
        volume_name: &mut String,
    ) -> FsResult<()> {
        if start_block == 0 && length == 0 {
            return Err(FsError::IoError);
        }
        let block_count = (length as usize + DESC_BLOCK_SIZE - 1) / DESC_BLOCK_SIZE;
        for i in 0..block_count {
            let block = start_block as usize + i;
            let off = block * DESC_BLOCK_SIZE;
            if off + DESC_BLOCK_SIZE > image.len() {
                break;
            }
            let buf = &image[off..off + DESC_BLOCK_SIZE];
            let tag = match DescriptorTag::parse(buf) {
                Some(t) => t,
                None => continue,
            };
            match tag.tag_identifier {
                TAG_PVD => {
                    if let Some(pvd) = PrimaryVolDesc::parse(buf) {
                        if volume_name.is_empty() {
                            *volume_name = pvd.volume_identifier;
                        }
                    }
                }
                TAG_PD => {
                    if let Some(pd) = PartitionDesc::parse(buf) {
                        partitions.insert(pd.partition_number, (pd.partition_start, pd.partition_length));
                    }
                }
                TAG_LVD => {
                    if lvd.is_none() {
                        *lvd = LogicalVolDesc::parse(buf);
                    }
                }
                TAG_TD => break,
                _ => {}
            }
        }
        Ok(())
    }

    /// Parse the partition maps in the Logical Volume Descriptor and build the
    /// `part_ref -> physical start block` table.
    fn build_partition_starts(
        lvd: &LogicalVolDesc,
        partitions: &BTreeMap<u16, (u32, u32)>,
        image: &[u8],
        block_size: u32,
    ) -> FsResult<Vec<u32>> {
        // Re-read the LVD block to access the partition map area. The LVD was
        // located in the VDS; we find it again by scanning the VDS is avoided
        // by re-parsing from the image using the descriptor tag location field.
        // Instead, locate the LVD via a fresh scan of the main VDS extent.
        let lvd_buf = Self::find_lvd(image)?;
        let num_maps = lvd.num_partition_maps as usize;
        let map_table_len = lvd.map_table_length as usize;
        let maps_start = 440usize;
        if maps_start + map_table_len > lvd_buf.len() {
            return Err(FsError::IoError);
        }

        let mut part_start: Vec<u32> = Vec::new();
        part_start.resize(num_maps, 0);
        let mut off = maps_start;
        for i in 0..num_maps {
            if off + 2 > lvd_buf.len() {
                break;
            }
            let map_type = lvd_buf[off];
            let map_len = lvd_buf[off + 1] as usize;
            if map_len == 0 || off + map_len > lvd_buf.len() {
                break;
            }
            if map_type == 1 {
                // Type 1 partition map: type(1) + length(1) + volSeqNum(2) + partitionNum(2).
                let part_number = le16(lvd_buf, off + 4).unwrap_or(0);
                if let Some(&(start, _)) = partitions.get(&part_number) {
                    if i < part_start.len() {
                        part_start[i] = start;
                    }
                }
            }
            // Type 2 maps (metadata, virtual) are skipped; their part_refs will
            // read as start 0 and fail gracefully if accessed.
            off += map_len;
        }

        // If no maps were parsed, fall back to a single partition (part_ref 0).
        if part_start.is_empty() {
            if let Some(&(start, _)) = partitions.values().next() {
                part_start.push(start);
            } else {
                return Err(FsError::IoError);
            }
        }
        let _ = block_size; // block size not needed for the start mapping itself.
        Ok(part_start)
    }

    /// Re-locate the Logical Volume Descriptor block by scanning the VDS.
    fn find_lvd(image: &[u8]) -> FsResult<&[u8]> {
        let anchor_off = (ANCHOR_BLOCK * DESC_BLOCK_SIZE as u64) as usize;
        if anchor_off + DESC_BLOCK_SIZE > image.len() {
            return Err(FsError::IoError);
        }
        let anchor = AnchorVdp::parse(&image[anchor_off..anchor_off + DESC_BLOCK_SIZE])
            .ok_or(FsError::IoError)?;
        for &start in [anchor.main_location, anchor.reserve_location].iter() {
            let block_count = (anchor.main_length as usize + DESC_BLOCK_SIZE - 1) / DESC_BLOCK_SIZE;
            for i in 0..block_count {
                let off = (start as usize + i) * DESC_BLOCK_SIZE;
                if off + DESC_BLOCK_SIZE > image.len() {
                    break;
                }
                let buf = &image[off..off + DESC_BLOCK_SIZE];
                if let Some(tag) = DescriptorTag::parse(buf) {
                    if tag.tag_identifier == TAG_LVD {
                        return Ok(buf);
                    }
                }
            }
        }
        Err(FsError::IoError)
    }

    /// Translate a (partition_ref, logical_block) pair to a physical block.
    fn resolve_phys(part_start: &[u32], part: u16, lba: u32, _block_size: u32) -> FsResult<u64> {
        let start = *part_start
            .get(part as usize)
            .ok_or(FsError::NotFound)?;
        Ok(start as u64 + lba as u64)
    }

    /// Return a slice covering one logical block at `physical_block`.
    fn slice_block(image: &[u8], physical_block: u64, block_size: u32) -> FsResult<&[u8]> {
        let start = (physical_block as usize)
            .checked_mul(block_size as usize)
            .ok_or(FsError::IoError)?;
        let end = start.checked_add(block_size as usize).ok_or(FsError::IoError)?;
        if end > image.len() {
            return Err(FsError::IoError);
        }
        Ok(&image[start..end])
    }

    /// Physical block for an ICB location.
    fn phys_for(&self, loc: IcbLoc) -> FsResult<u64> {
        Self::resolve_phys(&self.part_start, loc.part, loc.lba, self.block_size)
    }

    /// Slice one block from the backing image at `physical_block`.
    fn block(&self, physical_block: u64) -> FsResult<&[u8]> {
        Self::slice_block(self.image, physical_block, self.block_size)
    }

    /// Read and parse the ICB (File Entry / Extended File Entry) at `loc`.
    fn read_icb(&self, loc: IcbLoc) -> FsResult<IcbEntry> {
        let phys = self.phys_for(loc)?;
        let buf = self.block(phys)?;
        let tag = DescriptorTag::parse(buf).ok_or(FsError::IoError)?;
        let (is_extended, ad_offset, ea_len, ad_len) = match tag.tag_identifier {
            TAG_FE => (
                false,
                176usize,
                le32(buf, 168).ok_or(FsError::IoError)?,
                le32(buf, 172).ok_or(FsError::IoError)?,
            ),
            TAG_EFE => (
                true,
                216usize,
                le32(buf, 208).ok_or(FsError::IoError)?,
                le32(buf, 212).ok_or(FsError::IoError)?,
            ),
            _ => return Err(FsError::IoError),
        };

        let icb_tag = IcbTag::parse(buf, 16).ok_or(FsError::IoError)?;
        let info_len = le64(buf, 56).ok_or(FsError::IoError)?;
        let uid = le32(buf, 36).unwrap_or(0);
        let gid = le32(buf, 40).unwrap_or(0);
        let perm = le32(buf, 44).unwrap_or(0);
        let link_count = le16(buf, 48).unwrap_or(1);

        // The allocation descriptor area follows the extended attribute area.
        let ad_area = ad_offset + ea_len as usize;
        if ad_area + ad_len as usize > buf.len() {
            return Err(FsError::IoError);
        }

        Ok(IcbEntry {
            loc,
            file_type: icb_tag.file_type,
            ad_type: icb_tag.ad_type,
            info_len,
            uid,
            gid,
            perm,
            link_count,
            block: buf.to_vec(),
            ad_offset: ad_area,
            ad_len,
            ea_len,
            is_extended,
        })
    }

    /// Gather all data extents for an ICB, following allocation extent
    /// descriptors (next-extent chaining) up to `MAX_EXTENT_DEPTH` hops.
    fn collect_extents(&self, icb: &IcbEntry) -> FsResult<Vec<Extent>> {
        let mut extents = Vec::new();
        let part = icb.loc.part;
        self.collect_extents_recursive(
            &icb.block,
            icb.ad_offset,
            icb.ad_len,
            icb.ad_type,
            part,
            &mut extents,
            0,
        )?;
        Ok(extents)
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_extents_recursive(
        &self,
        block: &[u8],
        ad_offset: usize,
        ad_len: u32,
        ad_type: u16,
        part: u16,
        out: &mut Vec<Extent>,
        depth: usize,
    ) -> FsResult<()> {
        if depth > MAX_EXTENT_DEPTH {
            return Err(FsError::IoError);
        }
        let mut logical_off: u64 = out
            .last()
            .map(|e| e.logical_off + e.len as u64)
            .unwrap_or(0);
        let end = ad_offset + ad_len as usize;
        match ad_type {
            ICB_AD_INLINE => {
                // Inline data lives directly in the ICB's AD area.
                out.push(Extent {
                    logical_off,
                    phys_byte: ad_offset as u64,
                    len: ad_len,
                    is_hole: false,
                    is_inline: true,
                });
            }
            ICB_AD_SHORT => {
                let mut off = ad_offset;
                while off + 8 <= end {
                    let length = le32(block, off).ok_or(FsError::IoError)?;
                    let position = le32(block, off + 4).ok_or(FsError::IoError)?;
                    off += 8;
                    let ext_type = length & AD_TYPE_MASK;
                    let real_len = (length & AD_LEN_MASK) as u32;
                    if real_len == 0 {
                        break;
                    }
                    if ext_type == AD_NEXT_EXTENT {
                        // `position` is a logical block within `part` holding an
                        // Allocation Extent Descriptor with more short ADs.
                        self.follow_aed(part, position, ad_type, out, &mut logical_off, depth)?;
                    } else {
                        let phys = Self::resolve_phys(&self.part_start, part, position, self.block_size)?;
                        out.push(Extent {
                            logical_off,
                            phys_byte: phys * self.block_size as u64,
                            len: real_len,
                            is_hole: ext_type != AD_RECORDED,
                            is_inline: false,
                        });
                        logical_off += real_len as u64;
                    }
                }
            }
            ICB_AD_LONG => {
                let mut off = ad_offset;
                while off + 16 <= end {
                    let length = le32(block, off).ok_or(FsError::IoError)?;
                    let lba = le32(block, off + 4).ok_or(FsError::IoError)?;
                    let ext_part = le16(block, off + 8).ok_or(FsError::IoError)?;
                    off += 16;
                    let ext_type = length & AD_TYPE_MASK;
                    let real_len = (length & AD_LEN_MASK) as u32;
                    if real_len == 0 {
                        break;
                    }
                    if ext_type == AD_NEXT_EXTENT {
                        self.follow_aed(ext_part, lba, ad_type, out, &mut logical_off, depth)?;
                    } else {
                        let phys = Self::resolve_phys(&self.part_start, ext_part, lba, self.block_size)?;
                        out.push(Extent {
                            logical_off,
                            phys_byte: phys * self.block_size as u64,
                            len: real_len,
                            is_hole: ext_type != AD_RECORDED,
                            is_inline: false,
                        });
                        logical_off += real_len as u64;
                    }
                }
            }
            ICB_AD_EXTENDED => {
                let mut off = ad_offset;
                while off + 20 <= end {
                    let ex_len = le32(block, off).ok_or(FsError::IoError)?;
                    // rec_len at off+4, inf_len at off+8.
                    let lba = le32(block, off + 12).ok_or(FsError::IoError)?;
                    let ext_part = le16(block, off + 16).ok_or(FsError::IoError)?;
                    off += 20;
                    let ext_type = ex_len & AD_TYPE_MASK;
                    let real_len = (ex_len & AD_LEN_MASK) as u32;
                    if real_len == 0 {
                        break;
                    }
                    if ext_type == AD_NEXT_EXTENT {
                        self.follow_aed(ext_part, lba, ad_type, out, &mut logical_off, depth)?;
                    } else {
                        let phys = Self::resolve_phys(&self.part_start, ext_part, lba, self.block_size)?;
                        out.push(Extent {
                            logical_off,
                            phys_byte: phys * self.block_size as u64,
                            len: real_len,
                            is_hole: ext_type != AD_RECORDED,
                            is_inline: false,
                        });
                        logical_off += real_len as u64;
                    }
                }
            }
            _ => return Err(FsError::NotSupported),
        }
        Ok(())
    }

    /// Follow an Allocation Extent Descriptor (tag 258) which holds a
    /// continuation of allocation descriptors of the same type.
    fn follow_aed(
        &self,
        part: u16,
        lba: u32,
        ad_type: u16,
        out: &mut Vec<Extent>,
        logical_off: &mut u64,
        depth: usize,
    ) -> FsResult<()> {
        let phys = Self::resolve_phys(&self.part_start, part, lba, self.block_size)?;
        let buf = self.block(phys)?;
        let tag = DescriptorTag::parse(buf).ok_or(FsError::IoError)?;
        if tag.tag_identifier != TAG_AED {
            return Err(FsError::IoError);
        }
        // AED: tag(16) + previousAllocExtLocation(4) + lengthAllocDescs(4) + ads.
        let cont_len = le32(buf, 20).ok_or(FsError::IoError)?;
        // Preserve the running logical offset across the recursion by recording
        // the current extent count.
        let before = out.len();
        self.collect_extents_recursive(buf, 24, cont_len, ad_type, part, out, depth + 1)?;
        // `logical_off` is recomputed inside the recursion from `out.last()`,
        // so nothing more is needed here.
        let _ = (logical_off, before);
        Ok(())
    }

    /// Read the entire data stream of an ICB into a `Vec<u8>`.
    fn read_icb_all(&self, icb: &IcbEntry) -> FsResult<Vec<u8>> {
        let extents = self.collect_extents(icb)?;
        let total = extents
            .iter()
            .map(|e| e.len as u64)
            .sum::<u64>()
            .min(icb.info_len);
        let mut out: Vec<u8> = Vec::new();
        out.resize(total as usize, 0);
        for ext in &extents {
            let take = (ext.len as u64).min(total.saturating_sub(ext.logical_off)) as usize;
            if take == 0 {
                continue;
            }
            if ext.is_hole {
                // Holes read as zeroes (already zero-initialised).
                continue;
            }
            let dst_off = ext.logical_off as usize;
            if ext.is_inline {
                let src_off = ext.phys_byte as usize;
                if src_off + take > icb.block.len() {
                    return Err(FsError::IoError);
                }
                out[dst_off..dst_off + take]
                    .copy_from_slice(&icb.block[src_off..src_off + take]);
            } else {
                let src_off = ext.phys_byte as usize;
                if src_off + take > self.image.len() {
                    return Err(FsError::IoError);
                }
                out[dst_off..dst_off + take]
                    .copy_from_slice(&self.image[src_off..src_off + take]);
            }
        }
        Ok(out)
    }

    /// Enumerate the File Identifier Descriptors of a directory ICB.
    fn enumerate_fids(&self, icb: &IcbEntry) -> FsResult<Vec<FileIdentDesc>> {
        if icb.file_type != ICB_FILE_TYPE_DIRECTORY && icb.file_type != ICB_FILE_TYPE_STREAMDIR {
            return Err(FsError::NotADirectory);
        }
        let data = self.read_icb_all(icb)?;
        let mut fids = Vec::new();
        let mut offset = 0usize;
        while offset + 16 <= data.len() {
            // Validate the descriptor tag where present.
            let tag_id = le16(&data, offset).unwrap_or(0);
            if tag_id != TAG_FID {
                // FIDs may be padded to block boundaries with zero bytes.
                if tag_id == 0 {
                    // Skip to the next block boundary within the directory.
                    let bs = self.block_size as usize;
                    let in_block = offset % bs;
                    if in_block == 0 {
                        break;
                    }
                    offset += bs - in_block;
                    continue;
                }
                break;
            }
            match FileIdentDesc::parse(&data, offset) {
                Some(fid) => {
                    let next = offset + fid.record_len;
                    fids.push(fid);
                    if next <= offset {
                        break;
                    }
                    offset = next;
                }
                None => break,
            }
        }
        Ok(fids)
    }

    /// Look up a single named component within a directory ICB.
    fn lookup_in(&self, dir: IcbLoc, component: &str) -> FsResult<IcbLoc> {
        let icb = self.read_icb(dir)?;
        if icb.file_type != ICB_FILE_TYPE_DIRECTORY && icb.file_type != ICB_FILE_TYPE_STREAMDIR {
            return Err(FsError::NotADirectory);
        }
        for fid in self.enumerate_fids(&icb)? {
            if fid.is_deleted() || fid.is_parent() {
                continue;
            }
            if names_equal(&fid.name, component) {
                if fid.icb_lba == 0 && fid.icb_part == 0 {
                    return Err(FsError::NotFound);
                }
                return Ok(IcbLoc {
                    part: fid.icb_part,
                    lba: fid.icb_lba,
                });
            }
        }
        Err(FsError::NotFound)
    }

    /// Walk a path from the root, resolving symlinks, returning the final ICB.
    fn walk(&self, path: &str, depth: usize) -> FsResult<IcbLoc> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::TooManySymlinks);
        }
        let path = path.trim_start_matches('/');
        let mut current = self.root_icb;
        if path.is_empty() {
            return Ok(current);
        }
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            // The current node must be a directory to descend.
            let cur_icb = self.read_icb(current)?;
            if cur_icb.file_type == ICB_FILE_TYPE_SYMLINK {
                let target = self.read_symlink_target(&cur_icb)?;
                let resolved = self.walk(&target, depth + 1)?;
                let r_icb = self.read_icb(resolved)?;
                if r_icb.file_type != ICB_FILE_TYPE_DIRECTORY
                    && r_icb.file_type != ICB_FILE_TYPE_STREAMDIR
                {
                    return Err(FsError::NotADirectory);
                }
                current = resolved;
            }
            let cur_icb = self.read_icb(current)?;
            if cur_icb.file_type != ICB_FILE_TYPE_DIRECTORY
                && cur_icb.file_type != ICB_FILE_TYPE_STREAMDIR
            {
                return Err(FsError::NotADirectory);
            }
            current = self.lookup_in(current, component)?;
        }
        // Follow a trailing symlink so callers always receive the final target.
        let final_icb = self.read_icb(current)?;
        if final_icb.file_type == ICB_FILE_TYPE_SYMLINK && depth < MAX_SYMLINK_DEPTH {
            let target = self.read_symlink_target(&final_icb)?;
            return self.walk(&target, depth + 1);
        }
        Ok(current)
    }

    /// Read the target string of a symlink ICB (ECMA-167 4/14.16 path components).
    fn read_symlink_target(&self, icb: &IcbEntry) -> FsResult<String> {
        if icb.file_type != ICB_FILE_TYPE_SYMLINK {
            return Err(FsError::InvalidArgument);
        }
        let data = self.read_icb_all(icb)?;
        let mut target = String::new();
        let mut offset = 0usize;
        while offset + 2 <= data.len() {
            let comp_type = data[offset];
            let comp_len = data[offset + 1] as usize;
            if offset + 2 + comp_len > data.len() {
                break;
            }
            let ident = &data[offset + 2..offset + 2 + comp_len];
            match comp_type {
                1 => {
                    // Root path component -> absolute path.
                    target.clear();
                    target.push('/');
                }
                3 => {
                    // Parent directory.
                    if !target.is_empty() && !target.ends_with('/') {
                        target.push('/');
                    }
                    target.push_str("..");
                }
                4 | 2 => {
                    // Normal / environment component.
                    if !target.is_empty() && !target.ends_with('/') {
                        target.push('/');
                    }
                    target.push_str(&String::from_utf8_lossy(ident));
                }
                5 => {
                    // Delimiter / separator.
                    if !target.ends_with('/') {
                        target.push('/');
                    }
                }
                0 => break, // continuation / end
                _ => break,
            }
            // Path components are packed (2 + comp_len), no inter-padding.
            offset += 2 + comp_len;
        }
        Ok(target)
    }

    /// Build `FileMetadata` from a parsed ICB.
    fn metadata_for(&self, icb: &IcbEntry) -> FsResult<FileMetadata> {
        let file_type = icb.file_type_vfs();
        let size = if icb.file_type == ICB_FILE_TYPE_DIRECTORY {
            // Directories report their info length (the FID stream size).
            icb.info_len
        } else {
            icb.info_len
        };

        let mode = (icb.perm & 0o777) as u16;
        let permissions = if mode == 0 {
            match file_type {
                FileType::Directory => FilePermissions::default_directory(),
                _ => FilePermissions::default_file(),
            }
        } else {
            FilePermissions::from_octal(mode)
        };

        let modified = timestamp_to_unix(&icb.block, icb.mtime_offset());

        Ok(FileMetadata {
            inode: icb.loc.to_inode(),
            file_type,
            size,
            permissions,
            uid: icb.uid,
            gid: icb.gid,
            created: modified,
            modified,
            accessed: modified,
            link_count: icb.link_count as u32,
            device_id: None,
        })
    }
}

/// Compare two UDF file names. UDF names are case-sensitive but many images
/// use uppercase; fall back to a case-insensitive comparison for robustness.
fn names_equal(a: &str, b: &str) -> bool {
    a == b || a.eq_ignore_ascii_case(b)
}

impl FileSystem for UdfFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Udf
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.volume_blocks,
            // UDF is mounted read-only here.
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            block_size: self.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        // UDF is read-only: reject any write/create/truncate intent.
        if flags.write || flags.create || flags.truncate || flags.append || flags.exclusive {
            return Err(FsError::ReadOnly);
        }
        let loc = self.walk(path, 0)?;
        Ok(loc.to_inode())
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let loc = IcbLoc::from_inode(inode);
        let icb = self.read_icb(loc)?;

        // Directories are not readable as a byte stream.
        if icb.file_type == ICB_FILE_TYPE_DIRECTORY || icb.file_type == ICB_FILE_TYPE_STREAMDIR {
            return Err(FsError::IsADirectory);
        }

        let size = icb.info_len;
        if offset >= size {
            return Ok(0);
        }
        let remaining = size - offset;
        let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;

        let extents = self.collect_extents(&icb)?;
        let mut copied = 0usize;
        for ext in &extents {
            if copied >= to_read {
                break;
            }
            let ext_end = ext.logical_off + ext.len as u64;
            // Skip extents entirely before the requested offset.
            if ext_end <= offset {
                continue;
            }
            // Skip extents entirely after the requested range.
            if ext.logical_off >= offset + to_read as u64 {
                break;
            }
            let data_start = core::cmp::max(offset, ext.logical_off);
            let data_end = core::cmp::min(offset + to_read as u64, ext_end);
            let n = (data_end - data_start) as usize;
            if n == 0 {
                continue;
            }
            let within = (data_start - ext.logical_off) as usize;
            let dst_off = (data_start - offset) as usize;

            if ext.is_hole {
                // Holes read as zeroes.
                for b in &mut buffer[dst_off..dst_off + n] {
                    *b = 0;
                }
            } else if ext.is_inline {
                let src = ext.phys_byte as usize + within;
                if src + n > icb.block.len() {
                    return Err(FsError::IoError);
                }
                buffer[dst_off..dst_off + n].copy_from_slice(&icb.block[src..src + n]);
            } else {
                let src = ext.phys_byte as usize + within;
                if src + n > self.image.len() {
                    return Err(FsError::IoError);
                }
                buffer[dst_off..dst_off + n].copy_from_slice(&self.image[src..src + n]);
            }
            copied += n;
        }
        Ok(copied)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let loc = IcbLoc::from_inode(inode);
        let icb = self.read_icb(loc)?;
        self.metadata_for(&icb)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let loc = IcbLoc::from_inode(inode);
        let icb = self.read_icb(loc)?;
        if icb.file_type != ICB_FILE_TYPE_DIRECTORY && icb.file_type != ICB_FILE_TYPE_STREAMDIR {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        // Synthesize "." and ".." entries (UDF does not store them as FIDs).
        entries.push(DirectoryEntry {
            name: ".".to_string(),
            inode,
            file_type: FileType::Directory,
        });
        entries.push(DirectoryEntry {
            name: "..".to_string(),
            inode: self.root_icb.to_inode(),
            file_type: FileType::Directory,
        });

        for fid in self.enumerate_fids(&icb)? {
            if fid.is_deleted() || fid.is_parent() {
                continue;
            }
            let child_loc = IcbLoc {
                part: fid.icb_part,
                lba: fid.icb_lba,
            };
            // Determine the file type from the child ICB; fall back to the FID
            // directory characteristic if the ICB cannot be read.
            let file_type = match self.read_icb(child_loc) {
                Ok(child) => child.file_type_vfs(),
                Err(_) => {
                    if fid.is_directory() {
                        FileType::Directory
                    } else {
                        FileType::Regular
                    }
                }
            };
            entries.push(DirectoryEntry {
                name: fid.name,
                inode: child_loc.to_inode(),
                file_type,
            });
        }

        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only.
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<alloc::string::String> {
        let loc = self.walk(path, 0)?;
        let icb = self.read_icb(loc)?;
        if icb.file_type != ICB_FILE_TYPE_SYMLINK {
            return Err(FsError::InvalidArgument);
        }
        self.read_symlink_target(&icb)
    }

    fn sync(&self) -> FsResult<()> {
        // Read-only mount: nothing to flush.
        Ok(())
    }
}
