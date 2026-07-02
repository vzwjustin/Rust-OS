//! NTFS3 filesystem implementation.
//!
//! This module implements an NTFS (NT File System) driver that reads and
//! writes the on-disk format used by Windows. It supports:
//!
//! - BPB (BIOS Parameter Block) parsing and validation (NTFS signature
//!   "NTFS    ").
//! - MFT (Master File Table) record reading via the `NtfsBlockDevice` trait.
//! - File record segment parsing with attribute lists.
//! - Resident and non-resident data attribute reading/writing.
//! - Directory enumeration via index attributes ($INDEX_ROOT / $INDEX_ALLOCATION).
//! - File create/read/write, mkdir/rmdir, unlink, rename, symlink/readlink.
//!
//! Block I/O is performed through the [`NtfsBlockDevice`] trait, which
//! abstracts the underlying storage. All multi-byte on-disk fields are
//! little-endian, matching the NTFS specification.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{format, string::{String, ToString}, sync::Arc, vec, vec::Vec};
use spin::RwLock;

// ============================================================================
// Constants
// ============================================================================

const NTFS_SIGNATURE: &[u8; 8] = b"NTFS    ";
const NTFS_ROOT_MFT: u64 = 5;
#[allow(dead_code)]
const NTFS_MFT_MFT: u64 = 0;
#[allow(dead_code)]
const NTFS_DEFAULT_RECORD_SIZE: u32 = 1024;
const MAX_SYMLINK_DEPTH: usize = 8;
const NTFS_NAME_MAX: usize = 255;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_INDEX_ROOT: u32 = 0x90;
const ATTR_INDEX_ALLOCATION: u32 = 0xA0;
#[allow(dead_code)]
const ATTR_LIST: u32 = 0x20;

const FRF_IN_USE: u16 = 0x0001;
const FRF_DIRECTORY: u16 = 0x0002;

const NAMESPACE_POSIX: u8 = 0;
#[allow(dead_code)]
const NAMESPACE_WIN32: u8 = 1;
#[allow(dead_code)]
const NAMESPACE_DOS: u8 = 2;
#[allow(dead_code)]
const NAMESPACE_WIN32_AND_DOS: u8 = 3;

// ============================================================================
// Block device trait
// ============================================================================

pub trait NtfsBlockDevice: Send + Sync {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> FsResult<()>;
    fn write_at(&self, offset: u64, buf: &[u8]) -> FsResult<()>;
    #[allow(dead_code)]
    fn size(&self) -> u64;
    fn flush(&self) -> FsResult<()>;
}

// ============================================================================
// On-disk structures
// ============================================================================

#[derive(Debug, Clone)]
struct NtfsBpb {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    bytes_per_cluster: u32,
    total_sectors: u64,
    mft_logical_cluster_number: u64,
    #[allow(dead_code)]
    mft_mirror_logical_cluster_number: u64,
    clusters_per_mft_record: i8,
    bytes_per_mft_record: u32,
    #[allow(dead_code)]
    volume_serial_number: u64,
}

impl NtfsBpb {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 512 {
            return Err(FsError::IoError);
        }
        if &buf[3..11] != NTFS_SIGNATURE {
            return Err(FsError::IoError);
        }
        let bytes_per_sector = u16::from_le_bytes([buf[11], buf[12]]);
        let sectors_per_cluster = buf[13];
        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(FsError::IoError);
        }
        let bytes_per_cluster = bytes_per_sector as u32 * sectors_per_cluster as u32;
        let total_sectors = u64::from_le_bytes([
            buf[0x28], buf[0x29], buf[0x2A], buf[0x2B],
            buf[0x2C], buf[0x2D], buf[0x2E], buf[0x2F],
        ]);
        let mft_lcn = u64::from_le_bytes([
            buf[0x30], buf[0x31], buf[0x32], buf[0x33],
            buf[0x34], buf[0x35], buf[0x36], buf[0x37],
        ]);
        let mft_mirror_lcn = u64::from_le_bytes([
            buf[0x38], buf[0x39], buf[0x3A], buf[0x3B],
            buf[0x3C], buf[0x3D], buf[0x3E], buf[0x3F],
        ]);
        let clusters_per_mft_record = buf[0x40] as i8;
        let bytes_per_mft_record = if clusters_per_mft_record < 0 {
            1u32 << (-clusters_per_mft_record)
        } else {
            bytes_per_cluster * clusters_per_mft_record as u32
        };
        let serial = u64::from_le_bytes([
            buf[0x48], buf[0x49], buf[0x4A], buf[0x4B],
            buf[0x4C], buf[0x4D], buf[0x4E], buf[0x4F],
        ]);
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            total_sectors,
            mft_logical_cluster_number: mft_lcn,
            mft_mirror_logical_cluster_number: mft_mirror_lcn,
            clusters_per_mft_record,
            bytes_per_mft_record,
            volume_serial_number: serial,
        })
    }
}

#[derive(Debug, Clone)]
struct FileRecordHeader {
    magic: [u8; 4],
    first_attr_offset: u16,
    flags: u16,
    logical_size: u32,
    physical_size: u32,
    base_record: u64,
    sequence_number: u16,
}

impl FileRecordHeader {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 48 {
            return Err(FsError::IoError);
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        if &magic != b"FILE" {
            return Err(FsError::IoError);
        }
        Ok(Self {
            magic,
            first_attr_offset: u16::from_le_bytes([buf[20], buf[21]]),
            flags: u16::from_le_bytes([buf[22], buf[23]]),
            logical_size: u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]),
            physical_size: u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]),
            base_record: u64::from_le_bytes([
                buf[32], buf[33], buf[34], buf[35],
                buf[36], buf[37], buf[38], buf[39],
            ]),
            sequence_number: u16::from_le_bytes([buf[40], buf[41]]),
        })
    }

    fn is_in_use(&self) -> bool {
        (self.flags & FRF_IN_USE) != 0
    }

    fn is_directory(&self) -> bool {
        (self.flags & FRF_DIRECTORY) != 0
    }

    fn write(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.magic);
        buf[20..22].copy_from_slice(&self.first_attr_offset.to_le_bytes());
        buf[22..24].copy_from_slice(&self.flags.to_le_bytes());
        buf[24..28].copy_from_slice(&self.logical_size.to_le_bytes());
        buf[28..32].copy_from_slice(&self.physical_size.to_le_bytes());
        buf[32..40].copy_from_slice(&self.base_record.to_le_bytes());
        buf[40..42].copy_from_slice(&self.sequence_number.to_le_bytes());
    }
}

#[derive(Debug, Clone)]
struct AttributeHeader {
    attr_type: u32,
    length: u32,
    non_resident: bool,
    #[allow(dead_code)]
    name_length: u8,
    #[allow(dead_code)]
    name_offset: u16,
    #[allow(dead_code)]
    flags: u16,
    resident_value_offset: u16,
    resident_value_length: u32,
    non_resident_vcn_start: u64,
    non_resident_vcn_end: u64,
    non_resident_data_runs_offset: u16,
    non_resident_allocated_size: u64,
    non_resident_real_size: u64,
    non_resident_initialized_size: u64,
}

impl AttributeHeader {
    fn parse(buf: &[u8], offset: usize) -> FsResult<Self> {
        if offset + 16 > buf.len() {
            return Err(FsError::IoError);
        }
        let attr_type = u32::from_le_bytes([
            buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3],
        ]);
        if attr_type == 0xFFFFFFFF {
            return Err(FsError::NotFound);
        }
        let length = u32::from_le_bytes([
            buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7],
        ]);
        let non_resident = buf[offset + 8] != 0;
        let name_length = buf[offset + 9];
        let name_offset = u16::from_le_bytes([buf[offset + 10], buf[offset + 11]]);
        let flags = u16::from_le_bytes([buf[offset + 12], buf[offset + 13]]);

        if !non_resident {
            let value_offset = u16::from_le_bytes([buf[offset + 16], buf[offset + 17]]);
            let value_length = u32::from_le_bytes([
                buf[offset + 20], buf[offset + 21], buf[offset + 22], buf[offset + 23],
            ]);
            Ok(Self {
                attr_type, length, non_resident: false, name_length, name_offset, flags,
                resident_value_offset: value_offset, resident_value_length: value_length,
                non_resident_vcn_start: 0, non_resident_vcn_end: 0,
                non_resident_data_runs_offset: 0,
                non_resident_allocated_size: 0, non_resident_real_size: 0,
                non_resident_initialized_size: 0,
            })
        } else {
            if offset + 64 > buf.len() {
                return Err(FsError::IoError);
            }
            Ok(Self {
                attr_type, length, non_resident: true, name_length, name_offset, flags,
                resident_value_offset: 0, resident_value_length: 0,
                non_resident_vcn_start: u64::from_le_bytes([
                    buf[offset + 16], buf[offset + 17], buf[offset + 18], buf[offset + 19],
                    buf[offset + 20], buf[offset + 21], buf[offset + 22], buf[offset + 23],
                ]),
                non_resident_vcn_end: u64::from_le_bytes([
                    buf[offset + 24], buf[offset + 25], buf[offset + 26], buf[offset + 27],
                    buf[offset + 28], buf[offset + 29], buf[offset + 30], buf[offset + 31],
                ]),
                non_resident_data_runs_offset: u16::from_le_bytes([buf[offset + 32], buf[offset + 33]]),
                non_resident_allocated_size: u64::from_le_bytes([
                    buf[offset + 40], buf[offset + 41], buf[offset + 42], buf[offset + 43],
                    buf[offset + 44], buf[offset + 45], buf[offset + 46], buf[offset + 47],
                ]),
                non_resident_real_size: u64::from_le_bytes([
                    buf[offset + 48], buf[offset + 49], buf[offset + 50], buf[offset + 51],
                    buf[offset + 52], buf[offset + 53], buf[offset + 54], buf[offset + 55],
                ]),
                non_resident_initialized_size: u64::from_le_bytes([
                    buf[offset + 56], buf[offset + 57], buf[offset + 58], buf[offset + 59],
                    buf[offset + 60], buf[offset + 61], buf[offset + 62], buf[offset + 63],
                ]),
            })
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DataRun {
    length: u64,
    start_cluster: Option<u64>,
}

fn parse_data_runs(buf: &[u8], offset: usize) -> FsResult<Vec<DataRun>> {
    let mut runs = Vec::new();
    let mut pos = offset;
    let mut prev_cluster: i64 = 0;
    loop {
        if pos >= buf.len() { break; }
        let header = buf[pos];
        if header == 0 { break; }
        let len_size = (header & 0x0F) as usize;
        let off_size = ((header >> 4) & 0x0F) as usize;
        pos += 1;
        if pos + len_size > buf.len() { return Err(FsError::IoError); }
        let mut length: u64 = 0;
        for i in 0..len_size { length |= (buf[pos + i] as u64) << (8 * i); }
        pos += len_size;
        if off_size == 0 {
            runs.push(DataRun { length, start_cluster: None });
        } else {
            if pos + off_size > buf.len() { return Err(FsError::IoError); }
            let mut offset_val: i64 = 0;
            for i in 0..off_size { offset_val |= (buf[pos + i] as i64) << (8 * i); }
            if off_size < 8 && (buf[pos + off_size - 1] & 0x80) != 0 {
                offset_val |= -1i64 << (8 * off_size);
            }
            pos += off_size;
            prev_cluster += offset_val;
            runs.push(DataRun { length, start_cluster: Some(prev_cluster as u64) });
        }
    }
    Ok(runs)
}

fn encode_data_runs(runs: &[DataRun]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev_cluster: i64 = 0;
    for run in runs {
        let len = run.length;
        let len_size = if len == 0 { 1 } else { bytes_needed_u64(len) };
        let (off_size, off_val) = if let Some(sc) = run.start_cluster {
            let delta = sc as i64 - prev_cluster;
            prev_cluster = sc as i64;
            (bytes_needed_i64(delta), delta)
        } else { (0, 0) };
        out.push((len_size as u8) | ((off_size as u8) << 4));
        for i in 0..len_size { out.push((len >> (8 * i)) as u8); }
        for i in 0..off_size { out.push((off_val >> (8 * i)) as u8); }
    }
    out.push(0);
    out
}

fn bytes_needed_u64(v: u64) -> usize {
    let mut n = 1;
    let mut tmp = v;
    while tmp > 0xFF { tmp >>= 8; n += 1; }
    n
}

fn bytes_needed_i64(v: i64) -> usize {
    if v == 0 { return 1; }
    let mut n = 1;
    let mut tmp = v.unsigned_abs();
    while tmp > 0x7F { tmp >>= 8; n += 1; }
    n
}

#[derive(Debug, Clone)]
struct FileNameAttr {
    parent_directory: u64,
    #[allow(dead_code)]
    file_creation_time: u64,
    #[allow(dead_code)]
    file_modification_time: u64,
    #[allow(dead_code)]
    file_change_time: u64,
    #[allow(dead_code)]
    file_read_time: u64,
    #[allow(dead_code)]
    allocated_size: u64,
    #[allow(dead_code)]
    real_size: u64,
    flags: u32,
    reparse: u32,
    name_length: u8,
    #[allow(dead_code)]
    namespace: u8,
    name: Vec<u16>,
}

impl FileNameAttr {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 66 { return Err(FsError::IoError); }
        let parent = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let creation = u64::from_le_bytes([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]]);
        let modification = u64::from_le_bytes([buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23]]);
        let change = u64::from_le_bytes([buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31]]);
        let read = u64::from_le_bytes([buf[32], buf[33], buf[34], buf[35], buf[36], buf[37], buf[38], buf[39]]);
        let alloc = u64::from_le_bytes([buf[40], buf[41], buf[42], buf[43], buf[44], buf[45], buf[46], buf[47]]);
        let real = u64::from_le_bytes([buf[48], buf[49], buf[50], buf[51], buf[52], buf[53], buf[54], buf[55]]);
        let flags = u32::from_le_bytes([buf[56], buf[57], buf[58], buf[59]]);
        let reparse = u32::from_le_bytes([buf[60], buf[61], buf[62], buf[63]]);
        let name_length = buf[64];
        let namespace = buf[65];
        let name_bytes = &buf[66..];
        let mut name = Vec::new();
        for i in 0..name_length as usize {
            let off = i * 2;
            if off + 2 > name_bytes.len() { break; }
            name.push(u16::from_le_bytes([name_bytes[off], name_bytes[off + 1]]));
        }
        Ok(Self {
            parent_directory: parent & 0x0000_FFFF_FFFF_FFFF,
            file_creation_time: creation, file_modification_time: modification,
            file_change_time: change, file_read_time: read,
            allocated_size: alloc, real_size: real, flags, reparse,
            name_length, namespace, name,
        })
    }

    fn name_string(&self) -> String {
        let mut s = String::new();
        for &cu in &self.name {
            if cu < 128 { s.push(char::from(cu as u8)); }
            else { s.push(char::from_u32(cu as u32).unwrap_or('?')); }
        }
        s
    }

    fn parent_mft(&self) -> u64 {
        self.parent_directory & 0x0000_FFFF_FFFF_FFFF
    }
}

#[derive(Debug, Clone)]
struct StandardInfoAttr {
    creation_time: u64,
    modification_time: u64,
    #[allow(dead_code)]
    change_time: u64,
    read_time: u64,
    #[allow(dead_code)]
    flags: u32,
}

impl StandardInfoAttr {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 48 { return Err(FsError::IoError); }
        Ok(Self {
            creation_time: u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]),
            modification_time: u64::from_le_bytes([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]]),
            change_time: u64::from_le_bytes([buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23]]),
            read_time: u64::from_le_bytes([buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31]]),
            flags: u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]),
        })
    }
}

#[derive(Debug, Clone)]
struct IndexEntry {
    file_reference: u64,
    entry_size: u16,
    key_offset: u16,
    flags: u32,
}

impl IndexEntry {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 16 { return Err(FsError::IoError); }
        Ok(Self {
            file_reference: u64::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]]),
            entry_size: u16::from_le_bytes([buf[8], buf[9]]),
            key_offset: u16::from_le_bytes([buf[10], buf[11]]),
            flags: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        })
    }

    fn is_last(&self) -> bool { (self.flags & 0x02) != 0 }
    fn child_mft(&self) -> u64 { self.file_reference & 0x0000_FFFF_FFFF_FFFF }
}

// ============================================================================
// Filesystem
// ============================================================================

pub struct Ntfs3FileSystem {
    device_id: u32,
    device: Arc<dyn NtfsBlockDevice>,
    bpb: RwLock<NtfsBpb>,
    bytes_per_cluster: u32,
    bytes_per_mft_record: u32,
    mft_offset: u64,
    total_clusters: u64,
    dirty: RwLock<bool>,
    free_clusters: RwLock<u64>,
}

impl core::fmt::Debug for Ntfs3FileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("Ntfs3FileSystem")
            .field("device_id", &self.device_id)
            .field("bytes_per_cluster", &self.bytes_per_cluster)
            .field("bytes_per_mft_record", &self.bytes_per_mft_record)
            .field("mft_offset", &self.mft_offset)
            .field("total_clusters", &self.total_clusters)
            .finish()
    }
}

impl Ntfs3FileSystem {
    pub fn new(device_id: u32, device: Arc<dyn NtfsBlockDevice>) -> FsResult<Self> {
        let mut boot = vec![0u8; 512];
        device.read_at(0, &mut boot)?;
        let bpb = NtfsBpb::parse(&boot)?;
        let bytes_per_cluster = bpb.bytes_per_cluster;
        let bytes_per_mft_record = bpb.bytes_per_mft_record;
        let mft_offset = bpb.mft_logical_cluster_number * bytes_per_cluster as u64;
        let total_clusters = bpb.total_sectors / bpb.sectors_per_cluster as u64;
        let free_clusters = total_clusters / 2;
        Ok(Self {
            device_id, device, bpb: RwLock::new(bpb),
            bytes_per_cluster, bytes_per_mft_record, mft_offset, total_clusters,
            dirty: RwLock::new(false), free_clusters: RwLock::new(free_clusters),
        })
    }

    fn read_mft_record(&self, record_num: u64) -> FsResult<Vec<u8>> {
        let rec_size = self.bytes_per_mft_record as usize;
        let offset = self.mft_offset + record_num * self.bytes_per_mft_record as u64;
        let mut buf = vec![0u8; rec_size];
        self.device.read_at(offset, &mut buf)?;
        Ok(buf)
    }

    fn write_mft_record(&self, record_num: u64, buf: &[u8]) -> FsResult<()> {
        let offset = self.mft_offset + record_num * self.bytes_per_mft_record as u64;
        self.device.write_at(offset, buf)?;
        Ok(())
    }

    fn find_attribute(&self, record: &[u8], attr_type: u32) -> FsResult<(AttributeHeader, usize)> {
        let header = FileRecordHeader::parse(record)?;
        let mut offset = header.first_attr_offset as usize;
        loop {
            if offset + 16 > record.len() { break; }
            let attr = AttributeHeader::parse(record, offset)?;
            if attr.attr_type == 0xFFFFFFFF { break; }
            if attr.attr_type == attr_type { return Ok((attr, offset)); }
            if attr.length == 0 { break; }
            offset += attr.length as usize;
        }
        Err(FsError::NotFound)
    }

    fn find_attributes(&self, record: &[u8], attr_type: u32) -> FsResult<Vec<(AttributeHeader, usize)>> {
        let header = FileRecordHeader::parse(record)?;
        let mut offset = header.first_attr_offset as usize;
        let mut result = Vec::new();
        loop {
            if offset + 16 > record.len() { break; }
            let attr = match AttributeHeader::parse(record, offset) { Ok(a) => a, Err(_) => break };
            if attr.attr_type == 0xFFFFFFFF { break; }
            let attr_length = attr.length;
            if attr.attr_type == attr_type { result.push((attr, offset)); }
            if attr_length == 0 { break; }
            offset += attr_length as usize;
        }
        Ok(result)
    }

    fn read_resident_value(&self, record: &[u8], attr: &AttributeHeader, attr_offset: usize) -> FsResult<Vec<u8>> {
        let val_off = attr_offset + attr.resident_value_offset as usize;
        let val_len = attr.resident_value_length as usize;
        if val_off + val_len > record.len() { return Err(FsError::IoError); }
        Ok(record[val_off..val_off + val_len].to_vec())
    }

    fn read_non_resident(&self, record: &[u8], attr: &AttributeHeader, attr_offset: usize, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let real_size = attr.non_resident_real_size;
        if offset >= real_size { return Ok(0); }
        let remaining = real_size - offset;
        let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;
        let runs_offset = attr_offset + attr.non_resident_data_runs_offset as usize;
        let runs = parse_data_runs(record, runs_offset)?;
        let cluster_size = self.bytes_per_cluster as u64;
        let mut read = 0usize;
        let mut cur_vcn = 0u64;
        for run in &runs {
            let run_start_vcn = cur_vcn;
            let run_end_vcn = cur_vcn + run.length;
            let run_byte_start = run_start_vcn * cluster_size;
            let run_byte_end = run_end_vcn * cluster_size;
            if run_byte_end <= offset { cur_vcn = run_end_vcn; continue; }
            let overlap_start = if offset > run_byte_start { offset - run_byte_start } else { 0 };
            let overlap_end = core::cmp::min(run_byte_end, offset + to_read as u64);
            let overlap_len = (overlap_end - run_byte_start - overlap_start) as usize;
            if let Some(start_cluster) = run.start_cluster {
                let cluster_offset = start_cluster * cluster_size + overlap_start;
                let mut chunk = vec![0u8; overlap_len];
                self.device.read_at(cluster_offset, &mut chunk)?;
                let dest_end = read + overlap_len;
                if dest_end <= buffer.len() {
                    buffer[read..dest_end].copy_from_slice(&chunk);
                }
            } else {
                let dest_end = core::cmp::min(read + overlap_len, buffer.len());
                for b in &mut buffer[read..dest_end] { *b = 0; }
            }
            read += overlap_len;
            cur_vcn = run_end_vcn;
            if read >= to_read { break; }
        }
        Ok(core::cmp::min(read, to_read))
    }

    fn dir_lookup(&self, dir_record_num: u64, name: &str) -> FsResult<u64> {
        let record = self.read_mft_record(dir_record_num)?;
        let header = FileRecordHeader::parse(&record)?;
        if !header.is_directory() { return Err(FsError::NotADirectory); }

        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_INDEX_ROOT) {
            let index_data = self.read_resident_value(&record, &attr, attr_off)?;
            if let Some(child) = self.search_index_root(&index_data, name) { return Ok(child); }
        }
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_INDEX_ALLOCATION) {
            let mut buf = vec![0u8; attr.non_resident_real_size as usize];
            let n = self.read_non_resident(&record, &attr, attr_off, 0, &mut buf)?;
            if let Some(child) = self.search_index_allocation(&buf[..n], name) { return Ok(child); }
        }

        let file_name_attrs = self.find_attributes(&record, ATTR_FILE_NAME)?;
        for (attr, attr_off) in file_name_attrs {
            let value = self.read_resident_value(&record, &attr, attr_off)?;
            if let Ok(fn_attr) = FileNameAttr::parse(&value) {
                if fn_attr.name_string().eq_ignore_ascii_case(name) {
                    return Ok(fn_attr.parent_mft());
                }
            }
        }
        Err(FsError::NotFound)
    }

    fn search_index_root(&self, data: &[u8], name: &str) -> Option<u64> {
        if data.len() < 32 { return None; }
        self.search_index_entries(&data[32..], name)
    }

    fn search_index_entries(&self, data: &[u8], name: &str) -> Option<u64> {
        let mut pos = 0usize;
        while pos + 16 <= data.len() {
            let entry = match IndexEntry::parse(&data[pos..]) { Ok(e) => e, Err(_) => break };
            if entry.entry_size == 0 { break; }
            let key_off = pos + entry.key_offset as usize;
            if key_off < data.len() && key_off + 24 <= data.len() {
                let value_offset = u16::from_le_bytes([data[key_off + 16], data[key_off + 17]]) as usize;
                let value_start = key_off + value_offset;
                if value_start < data.len() {
                    if let Ok(fn_attr) = FileNameAttr::parse(&data[value_start..]) {
                        if fn_attr.name_string().eq_ignore_ascii_case(name) {
                            return Some(entry.child_mft());
                        }
                    }
                }
            }
            if entry.is_last() { break; }
            pos += entry.entry_size as usize;
        }
        None
    }

    fn search_index_allocation(&self, data: &[u8], name: &str) -> Option<u64> {
        let cluster_size = self.bytes_per_cluster as usize;
        let mut offset = 0usize;
        while offset < data.len() {
            let end = core::cmp::min(offset + cluster_size, data.len());
            let block = &data[offset..end];
            if block.len() >= 4 && &block[0..4] == b"INDX" {
                let entries_start = 24 + 16;
                if block.len() > entries_start {
                    if let Some(child) = self.search_index_entries(&block[entries_start..], name) {
                        return Some(child);
                    }
                }
            }
            offset += cluster_size;
        }
        None
    }

    fn dir_entries(&self, dir_record_num: u64) -> FsResult<Vec<(String, u64, FileType)>> {
        let record = self.read_mft_record(dir_record_num)?;
        let header = FileRecordHeader::parse(&record)?;
        if !header.is_directory() { return Err(FsError::NotADirectory); }
        let mut entries = Vec::new();
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_INDEX_ROOT) {
            let index_data = self.read_resident_value(&record, &attr, attr_off)?;
            self.collect_index_entries(&index_data, &mut entries);
        }
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_INDEX_ALLOCATION) {
            let mut buf = vec![0u8; attr.non_resident_real_size as usize];
            let n = self.read_non_resident(&record, &attr, attr_off, 0, &mut buf)?;
            self.collect_index_allocation_entries(&buf[..n], &mut entries);
        }
        if entries.is_empty() {
            let file_name_attrs = self.find_attributes(&record, ATTR_FILE_NAME)?;
            for (attr, attr_off) in file_name_attrs {
                let value = self.read_resident_value(&record, &attr, attr_off)?;
                if let Ok(fn_attr) = FileNameAttr::parse(&value) {
                    let name = fn_attr.name_string();
                    if !name.is_empty() && name != "." && name != ".." {
                        let child_mft = fn_attr.parent_mft();
                        let ft = self.file_type_of(child_mft);
                        entries.push((name, child_mft, ft));
                    }
                }
            }
        }
        Ok(entries)
    }

    fn collect_index_entries(&self, data: &[u8], out: &mut Vec<(String, u64, FileType)>) {
        if data.len() < 32 { return; }
        self.collect_entries_from(&data[32..], out);
    }

    fn collect_index_allocation_entries(&self, data: &[u8], out: &mut Vec<(String, u64, FileType)>) {
        let cluster_size = self.bytes_per_cluster as usize;
        let mut offset = 0usize;
        while offset < data.len() {
            let end = core::cmp::min(offset + cluster_size, data.len());
            let block = &data[offset..end];
            if block.len() >= 4 && &block[0..4] == b"INDX" {
                let entries_start = 24 + 16;
                if block.len() > entries_start {
                    self.collect_entries_from(&block[entries_start..], out);
                }
            }
            offset += cluster_size;
        }
    }

    fn collect_entries_from(&self, data: &[u8], out: &mut Vec<(String, u64, FileType)>) {
        let mut pos = 0usize;
        while pos + 16 <= data.len() {
            let entry = match IndexEntry::parse(&data[pos..]) { Ok(e) => e, Err(_) => break };
            if entry.entry_size == 0 { break; }
            let key_off = pos + entry.key_offset as usize;
            if key_off < data.len() && key_off + 24 <= data.len() {
                let value_offset = u16::from_le_bytes([data[key_off + 16], data[key_off + 17]]) as usize;
                let value_start = key_off + value_offset;
                if value_start < data.len() {
                    if let Ok(fn_attr) = FileNameAttr::parse(&data[value_start..]) {
                        let name = fn_attr.name_string();
                        if !name.is_empty() && name != "." && name != ".." {
                            let child_mft = entry.child_mft();
                            let ft = self.file_type_of(child_mft);
                            out.push((name, child_mft, ft));
                        }
                    }
                }
            }
            if entry.is_last() { break; }
            pos += entry.entry_size as usize;
        }
    }

    fn file_type_of(&self, record_num: u64) -> FileType {
        if let Ok(record) = self.read_mft_record(record_num) {
            if let Ok(header) = FileRecordHeader::parse(&record) {
                if header.is_directory() { return FileType::Directory; }
                if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_FILE_NAME) {
                    if let Ok(value) = self.read_resident_value(&record, &attr, attr_off) {
                        if let Ok(fn_attr) = FileNameAttr::parse(&value) {
                            if (fn_attr.reparse & 0xA000FFFF) == 0xA0000000 {
                                return FileType::SymbolicLink;
                            }
                        }
                    }
                }
                return FileType::Regular;
            }
        }
        FileType::Regular
    }

    fn walk(&self, path: &str, follow_symlink: bool, depth: usize) -> FsResult<u64> {
        if depth > MAX_SYMLINK_DEPTH { return Err(FsError::TooManySymlinks); }
        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        if components.is_empty() { return Ok(NTFS_ROOT_MFT); }
        let mut current = NTFS_ROOT_MFT;
        let last = components.len() - 1;
        for (i, comp) in components.iter().enumerate() {
            let child = self.dir_lookup(current, comp)?;
            if i < last || follow_symlink {
                let record = self.read_mft_record(child)?;
                if self.is_symlink_record(&record)? {
                    let target = self.read_symlink_target(&record, child)?;
                    let resolved = self.walk(&target, true, depth + 1)?;
                    current = resolved;
                    continue;
                }
            }
            current = child;
        }
        Ok(current)
    }

    fn is_symlink_record(&self, record: &[u8]) -> FsResult<bool> {
        if let Ok((attr, attr_off)) = self.find_attribute(record, ATTR_FILE_NAME) {
            if let Ok(value) = self.read_resident_value(record, &attr, attr_off) {
                if let Ok(fn_attr) = FileNameAttr::parse(&value) {
                    if fn_attr.reparse == 0xA000000C { return Ok(true); }
                }
            }
        }
        Ok(false)
    }

    fn read_symlink_target(&self, record: &[u8], _record_num: u64) -> FsResult<String> {
        if let Ok((attr, attr_off)) = self.find_attribute(record, ATTR_DATA) {
            if attr.non_resident {
                let mut buf = vec![0u8; attr.non_resident_real_size as usize];
                let n = self.read_non_resident(record, &attr, attr_off, 0, &mut buf)?;
                if n > 12 {
                    return Ok(utf16_bytes_to_string(&buf[12..n]));
                }
            } else {
                let data = self.read_resident_value(record, &attr, attr_off)?;
                if data.len() > 12 {
                    return Ok(utf16_bytes_to_string(&data[12..]));
                }
            }
        }
        Err(FsError::IoError)
    }

    fn split_path(path: &str) -> (String, String) {
        let path = path.trim_start_matches('/');
        if let Some(idx) = path.rfind('/') {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            let parent = parent.trim_start_matches('/');
            let parent = if parent.is_empty() { "/".to_string() } else { format!("/{}", parent) };
            (parent, name.to_string())
        } else {
            ("/".to_string(), path.to_string())
        }
    }

    fn metadata_from_record(&self, record_num: u64, record: &[u8]) -> FsResult<FileMetadata> {
        let header = FileRecordHeader::parse(record)?;
        let file_type = if header.is_directory() { FileType::Directory }
            else if self.is_symlink_record(record)? { FileType::SymbolicLink }
            else { FileType::Regular };
        let (created, modified, accessed, size) =
            if let Ok((attr, attr_off)) = self.find_attribute(record, ATTR_STANDARD_INFORMATION) {
                let value = self.read_resident_value(record, &attr, attr_off)?;
                let si = StandardInfoAttr::parse(&value)?;
                let data_size = if let Ok((dattr, _)) = self.find_attribute(record, ATTR_DATA) {
                    if dattr.non_resident { dattr.non_resident_real_size } else { dattr.resident_value_length as u64 }
                } else { 0 };
                (si.creation_time, si.modification_time, si.read_time, data_size)
            } else { (0, 0, 0, 0) };
        let ntfs_to_unix_ms = |nt: u64| -> u64 {
            if nt == 0 { return 0; }
            let unix_100ns = nt.saturating_sub(116444736000000000);
            unix_100ns / 10000
        };
        Ok(FileMetadata {
            inode: record_num, file_type, size,
            permissions: if file_type == FileType::Directory { FilePermissions::default_directory() } else { FilePermissions::default_file() },
            uid: 0, gid: 0,
            created: ntfs_to_unix_ms(created), modified: ntfs_to_unix_ms(modified), accessed: ntfs_to_unix_ms(accessed),
            link_count: 1, device_id: Some(self.device_id),
        })
    }

    fn alloc_mft_record(&self) -> FsResult<u64> {
        let mut record_num = 24u64;
        loop {
            let record = self.read_mft_record(record_num)?;
            if let Ok(header) = FileRecordHeader::parse(&record) {
                if !header.is_in_use() { return Ok(record_num); }
            } else { return Ok(record_num); }
            record_num += 1;
            if record_num > 100000 { return Err(FsError::NoSpaceLeft); }
        }
    }

    fn alloc_cluster(&self) -> FsResult<u64> {
        let bpb = self.bpb.read();
        let scan_start = bpb.mft_logical_cluster_number + 100;
        drop(bpb);
        for cluster in scan_start..self.total_clusters {
            let data = self.read_cluster_vec(cluster)?;
            if data.iter().all(|&b| b == 0) {
                let marker = [0xFFu8; 8];
                let offset = cluster * self.bytes_per_cluster as u64;
                self.device.write_at(offset, &marker)?;
                { let mut fc = self.free_clusters.write(); *fc = fc.saturating_sub(1); }
                return Ok(cluster);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    fn read_cluster_vec(&self, cluster: u64) -> FsResult<Vec<u8>> {
        let bpb = self.bpb.read();
        let offset = (bpb.mft_logical_cluster_number * 0 + cluster) * self.bytes_per_cluster as u64;
        drop(bpb);
        let mut buf = vec![0u8; self.bytes_per_cluster as usize];
        self.device.read_at(offset, &mut buf)?;
        Ok(buf)
    }

    fn create_mft_record(&self, is_dir: bool, _permissions: FilePermissions) -> FsResult<u64> {
        let record_num = self.alloc_mft_record()?;
        let rec_size = self.bytes_per_mft_record as usize;
        let mut record = vec![0u8; rec_size];
        let header = FileRecordHeader {
            magic: *b"FILE", first_attr_offset: 42,
            flags: FRF_IN_USE | if is_dir { FRF_DIRECTORY } else { 0 },
            logical_size: rec_size as u32, physical_size: rec_size as u32,
            base_record: 0, sequence_number: 1,
        };
        header.write(&mut record);
        let mut offset = header.first_attr_offset as usize;
        let now_ntfs = unix_to_ntfs_time(get_current_time());
        let si_attr_len = 88u32;
        record[offset..offset + 4].copy_from_slice(&ATTR_STANDARD_INFORMATION.to_le_bytes());
        record[offset + 4..offset + 8].copy_from_slice(&si_attr_len.to_le_bytes());
        record[offset + 8] = 0;
        record[offset + 16..offset + 18].copy_from_slice(&24u16.to_le_bytes());
        record[offset + 20..offset + 24].copy_from_slice(&72u32.to_le_bytes());
        let val_off = offset + 24;
        record[val_off..val_off + 8].copy_from_slice(&now_ntfs.to_le_bytes());
        record[val_off + 8..val_off + 16].copy_from_slice(&now_ntfs.to_le_bytes());
        record[val_off + 16..val_off + 24].copy_from_slice(&now_ntfs.to_le_bytes());
        record[val_off + 24..val_off + 32].copy_from_slice(&now_ntfs.to_le_bytes());
        let flags = if is_dir { 0x10000000u32 } else { 0x00000020u32 };
        record[val_off + 32..val_off + 36].copy_from_slice(&flags.to_le_bytes());
        offset += si_attr_len as usize;
        let data_attr_len = 24u32;
        record[offset..offset + 4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        record[offset + 4..offset + 8].copy_from_slice(&data_attr_len.to_le_bytes());
        record[offset + 8] = 0;
        record[offset + 16..offset + 18].copy_from_slice(&24u16.to_le_bytes());
        record[offset + 20..offset + 24].copy_from_slice(&0u32.to_le_bytes());
        offset += data_attr_len as usize;
        if offset + 4 <= rec_size { record[offset..offset + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); }
        if rec_size >= 512 { record[0x1FE] = 0; record[0x1FF] = 0; }
        self.write_mft_record(record_num, &record)?;
        Ok(record_num)
    }

    fn dir_add_entry(&self, parent_mft: u64, name: &str, child_mft: u64, is_dir: bool) -> FsResult<()> {
        let mut record = self.read_mft_record(parent_mft)?;
        let header = FileRecordHeader::parse(&record)?;
        if !header.is_directory() { return Err(FsError::NotADirectory); }
        let mut offset = header.first_attr_offset as usize;
        let mut last_attr_end = offset;
        loop {
            if offset + 16 > record.len() { break; }
            let attr = match AttributeHeader::parse(&record, offset) { Ok(a) => a, Err(_) => break };
            if attr.attr_type == 0xFFFFFFFF { break; }
            if attr.length == 0 { break; }
            offset += attr.length as usize;
            last_attr_end = offset;
        }
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes: Vec<u8> = name_utf16.iter().flat_map(|cu| cu.to_le_bytes()).collect();
        let fn_value_len = 66 + name_bytes.len();
        let fn_value_padded = (fn_value_len + 7) & !7;
        let fn_attr_len = (16 + fn_value_padded) as u32;
        if last_attr_end + fn_attr_len as usize + 4 > record.len() { return Err(FsError::NoSpaceLeft); }
        let off = last_attr_end;
        record[off..off + 4].copy_from_slice(&ATTR_FILE_NAME.to_le_bytes());
        record[off + 4..off + 8].copy_from_slice(&fn_attr_len.to_le_bytes());
        record[off + 8] = 0;
        record[off + 16..off + 18].copy_from_slice(&24u16.to_le_bytes());
        record[off + 20..off + 24].copy_from_slice(&(fn_value_len as u32).to_le_bytes());
        let val_off = off + 24;
        let parent_ref = parent_mft & 0x0000_FFFF_FFFF_FFFF;
        record[val_off..val_off + 8].copy_from_slice(&parent_ref.to_le_bytes());
        let now = unix_to_ntfs_time(get_current_time());
        record[val_off + 8..val_off + 16].copy_from_slice(&now.to_le_bytes());
        record[val_off + 16..val_off + 24].copy_from_slice(&now.to_le_bytes());
        record[val_off + 24..val_off + 32].copy_from_slice(&now.to_le_bytes());
        record[val_off + 32..val_off + 40].copy_from_slice(&now.to_le_bytes());
        record[val_off + 40..val_off + 48].copy_from_slice(&0u64.to_le_bytes());
        record[val_off + 48..val_off + 56].copy_from_slice(&0u64.to_le_bytes());
        let flags = if is_dir { 0x10000000u32 } else { 0x00000020u32 };
        record[val_off + 56..val_off + 60].copy_from_slice(&flags.to_le_bytes());
        record[val_off + 60..val_off + 64].copy_from_slice(&0u32.to_le_bytes());
        record[val_off + 64] = name_utf16.len() as u8;
        record[val_off + 65] = NAMESPACE_POSIX;
        record[val_off + 66..val_off + 66 + name_bytes.len()].copy_from_slice(&name_bytes);
        let end = off + fn_attr_len as usize;
        if end + 4 <= record.len() { record[end..end + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); }
        let new_logical = (end + 4) as u32;
        let mut new_header = header;
        new_header.logical_size = core::cmp::max(new_header.logical_size, new_logical);
        new_header.write(&mut record);
        self.write_mft_record(parent_mft, &record)?;
        Ok(())
    }

    fn dir_remove_entry(&self, parent_mft: u64, name: &str) -> FsResult<u64> {
        let mut record = self.read_mft_record(parent_mft)?;
        let header = FileRecordHeader::parse(&record)?;
        if !header.is_directory() { return Err(FsError::NotADirectory); }
        let mut offset = header.first_attr_offset as usize;
        loop {
            if offset + 16 > record.len() { break; }
            let attr = match AttributeHeader::parse(&record, offset) { Ok(a) => a, Err(_) => break };
            if attr.attr_type == 0xFFFFFFFF { break; }
            if attr.length == 0 { break; }
            if attr.attr_type == ATTR_FILE_NAME {
                let value = self.read_resident_value(&record, &attr, offset)?;
                if let Ok(fn_attr) = FileNameAttr::parse(&value) {
                    if fn_attr.name_string().eq_ignore_ascii_case(name) {
                        let child_mft = fn_attr.parent_mft();
                        record[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
                        self.write_mft_record(parent_mft, &record)?;
                        return Ok(child_mft);
                    }
                }
            }
            offset += attr.length as usize;
        }
        Ok(0)
    }

    fn truncate_file(&self, inode: InodeNumber, new_size: u64) -> FsResult<()> {
        let record = self.read_mft_record(inode)?;
        let (attr, attr_off) = self.find_attribute(&record, ATTR_DATA)?;
        if !attr.non_resident {
            let mut data = self.read_resident_value(&record, &attr, attr_off)?;
            if (new_size as usize) < data.len() { data.truncate(new_size as usize); }
            else { data.resize(new_size as usize, 0); }
            self.write_resident_data(inode, &record, &attr, attr_off, &data)?;
        } else {
            let mut new_record = record.clone();
            let real_size_off = attr_off + 48;
            new_record[real_size_off..real_size_off + 8].copy_from_slice(&new_size.to_le_bytes());
            self.write_mft_record(inode, &new_record)?;
        }
        Ok(())
    }

    fn write_resident_data(&self, inode: InodeNumber, record: &[u8], attr: &AttributeHeader, attr_offset: usize, data: &[u8]) -> FsResult<()> {
        let mut new_record = record.to_vec();
        let len_off = attr_offset + 20;
        new_record[len_off..len_off + 4].copy_from_slice(&(data.len() as u32).to_le_bytes());
        let val_off = attr_offset + attr.resident_value_offset as usize;
        if val_off + data.len() > new_record.len() { return Err(FsError::NoSpaceLeft); }
        new_record[val_off..val_off + data.len()].copy_from_slice(data);
        self.write_mft_record(inode, &new_record)?;
        Ok(())
    }

    fn convert_to_non_resident(&self, inode: InodeNumber, record: &[u8], data: &[u8]) -> FsResult<()> {
        let cluster_size = self.bytes_per_cluster as usize;
        let num_clusters = (data.len() + cluster_size - 1) / cluster_size;
        let mut clusters = Vec::new();
        for _ in 0..num_clusters { clusters.push(self.alloc_cluster()?); }
        for (i, &cluster) in clusters.iter().enumerate() {
            let start = i * cluster_size;
            let end = core::cmp::min(start + cluster_size, data.len());
            let mut buf = vec![0u8; cluster_size];
            buf[..end - start].copy_from_slice(&data[start..end]);
            self.device.write_at(cluster * cluster_size as u64, &buf)?;
        }
        let runs: Vec<DataRun> = clusters.iter().map(|&c| DataRun { length: 1, start_cluster: Some(c) }).collect();
        let encoded_runs = encode_data_runs(&runs);
        let mut new_record = record.to_vec();
        let (attr, attr_off) = self.find_attribute(record, ATTR_DATA)?;
        let new_attr_len = (64 + encoded_runs.len() + 7) & !7;
        if attr_off + new_attr_len > new_record.len() { return Err(FsError::NoSpaceLeft); }
        new_record[attr_off..attr_off + 4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        new_record[attr_off + 4..attr_off + 8].copy_from_slice(&(new_attr_len as u32).to_le_bytes());
        new_record[attr_off + 8] = 1;
        new_record[attr_off + 16..attr_off + 24].copy_from_slice(&0u64.to_le_bytes());
        new_record[attr_off + 24..attr_off + 32].copy_from_slice(&((num_clusters - 1) as u64).to_le_bytes());
        new_record[attr_off + 32..attr_off + 34].copy_from_slice(&64u16.to_le_bytes());
        new_record[attr_off + 40..attr_off + 48].copy_from_slice(&((num_clusters * cluster_size) as u64).to_le_bytes());
        new_record[attr_off + 48..attr_off + 56].copy_from_slice(&(data.len() as u64).to_le_bytes());
        new_record[attr_off + 56..attr_off + 64].copy_from_slice(&(data.len() as u64).to_le_bytes());
        new_record[attr_off + 64..attr_off + 64 + encoded_runs.len()].copy_from_slice(&encoded_runs);
        for i in (attr_off + 64 + encoded_runs.len())..(attr_off + new_attr_len) {
            if i < new_record.len() { new_record[i] = 0; }
        }
        self.write_mft_record(inode, &new_record)?;
        Ok(())
    }

    fn write_non_resident_data(&self, inode: InodeNumber, record: &[u8], attr: &AttributeHeader, attr_offset: usize, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let cluster_size = self.bytes_per_cluster as u64;
        let runs_buf_offset = attr_offset + attr.non_resident_data_runs_offset as usize;
        let mut runs = parse_data_runs(record, runs_buf_offset)?;
        let end = offset + buffer.len() as u64;
        let clusters_needed = (end + cluster_size - 1) / cluster_size;
        let mut current_vcn = 0u64;
        for run in &runs { current_vcn += run.length; }
        while current_vcn < clusters_needed {
            let new_cluster = self.alloc_cluster()?;
            runs.push(DataRun { length: 1, start_cluster: Some(new_cluster) });
            current_vcn += 1;
        }
        let mut written = 0usize;
        let mut cur_vcn = 0u64;
        for run in &runs {
            let run_start_vcn = cur_vcn;
            let run_end_vcn = cur_vcn + run.length;
            let run_byte_start = run_start_vcn * cluster_size;
            let run_byte_end = run_end_vcn * cluster_size;
            if run_byte_end <= offset { cur_vcn = run_end_vcn; continue; }
            let overlap_start = if offset > run_byte_start { offset - run_byte_start } else { 0 };
            let overlap_end = core::cmp::min(run_byte_end, end);
            let overlap_len = (overlap_end - run_byte_start - overlap_start) as usize;
            if let Some(start_cluster) = run.start_cluster {
                let cluster_byte = start_cluster * cluster_size + overlap_start;
                let mut chunk = vec![0u8; cluster_size as usize];
                self.device.read_at(cluster_byte, &mut chunk)?;
                let buf_start = written;
                let buf_end = written + overlap_len;
                if buf_end <= buffer.len() {
                    chunk[..overlap_len].copy_from_slice(&buffer[buf_start..buf_end]);
                    self.device.write_at(cluster_byte, &chunk[..overlap_len.min(chunk.len())])?;
                }
            }
            written += overlap_len;
            cur_vcn = run_end_vcn;
            if written >= buffer.len() { break; }
        }
        let encoded = encode_data_runs(&runs);
        let mut new_record = record.to_vec();
        let new_runs_off = attr_offset + attr.non_resident_data_runs_offset as usize;
        if new_runs_off + encoded.len() <= new_record.len() {
            new_record[new_runs_off..new_runs_off + encoded.len()].copy_from_slice(&encoded);
        }
        if end > attr.non_resident_real_size {
            let real_size_off = attr_offset + 48;
            new_record[real_size_off..real_size_off + 8].copy_from_slice(&end.to_le_bytes());
            let init_size_off = attr_offset + 56;
            new_record[init_size_off..init_size_off + 8].copy_from_slice(&end.to_le_bytes());
        }
        self.write_mft_record(inode, &new_record)?;
        Ok(written)
    }

    fn free_clusters_range(&self, start: u64, count: u64) -> FsResult<()> {
        let cluster_size = self.bytes_per_cluster as usize;
        let zero = vec![0u8; cluster_size];
        for i in 0..count {
            let offset = (start + i) * cluster_size as u64;
            self.device.write_at(offset, &zero)?;
        }
        { let mut fc = self.free_clusters.write(); *fc += count; }
        Ok(())
    }

    fn free_mft_record(&self, record_num: u64) -> FsResult<()> {
        let mut record = self.read_mft_record(record_num)?;
        let mut header = FileRecordHeader::parse(&record)?;
        header.flags &= !FRF_IN_USE;
        header.sequence_number += 1;
        header.write(&mut record);
        self.write_mft_record(record_num, &record)?;
        Ok(())
    }
}

fn utf16_bytes_to_string(bytes: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let cu = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        if cu == 0 { break; }
        s.push(char::from_u32(cu as u32).unwrap_or('?'));
        i += 2;
    }
    s
}

fn unix_to_ntfs_time(unix_ms: u64) -> u64 {
    (unix_ms * 10000) + 116444736000000000
}

impl FileSystem for Ntfs3FileSystem {
    fn fs_type(&self) -> FileSystemType { FileSystemType::Ntfs3 }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let bpb = self.bpb.read();
        let free = *self.free_clusters.read();
        Ok(FileSystemStats {
            total_blocks: self.total_clusters, free_blocks: free, available_blocks: free,
            total_inodes: 0, free_inodes: 0,
            block_size: bpb.bytes_per_cluster, max_filename_length: NTFS_NAME_MAX as u32,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." { return Err(FsError::InvalidArgument); }
        let parent_mft = self.walk(&parent_path, true, 0)?;
        if self.dir_lookup(parent_mft, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let record_num = self.create_mft_record(false, permissions)?;
        self.dir_add_entry(parent_mft, &name, record_num, false)?;
        Ok(record_num)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        if flags.create {
            match self.walk(path, true, 0) {
                Ok(mft) => {
                    if flags.exclusive { return Err(FsError::AlreadyExists); }
                    if flags.truncate {
                        let record = self.read_mft_record(mft)?;
                        let header = FileRecordHeader::parse(&record)?;
                        if header.is_directory() { return Err(FsError::IsADirectory); }
                        self.truncate_file(mft, 0)?;
                    }
                    return Ok(mft);
                }
                Err(FsError::NotFound) => { return self.create(path, FilePermissions::default_file()); }
                Err(e) => return Err(e),
            }
        }
        let mft = self.walk(path, true, 0)?;
        if flags.truncate {
            let record = self.read_mft_record(mft)?;
            let header = FileRecordHeader::parse(&record)?;
            if header.is_directory() { return Err(FsError::IsADirectory); }
            self.truncate_file(mft, 0)?;
        }
        Ok(mft)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let record = self.read_mft_record(inode)?;
        let header = FileRecordHeader::parse(&record)?;
        if header.is_directory() { return Err(FsError::IsADirectory); }
        let (attr, attr_off) = self.find_attribute(&record, ATTR_DATA)?;
        if attr.non_resident {
            self.read_non_resident(&record, &attr, attr_off, offset, buffer)
        } else {
            let data = self.read_resident_value(&record, &attr, attr_off)?;
            if offset >= data.len() as u64 { return Ok(0); }
            let start = offset as usize;
            let end = core::cmp::min(start + buffer.len(), data.len());
            let n = end - start;
            buffer[..n].copy_from_slice(&data[start..end]);
            Ok(n)
        }
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let record = self.read_mft_record(inode)?;
        let header = FileRecordHeader::parse(&record)?;
        if header.is_directory() { return Err(FsError::IsADirectory); }
        let (attr, attr_off) = self.find_attribute(&record, ATTR_DATA)?;
        if !attr.non_resident {
            let current_data = self.read_resident_value(&record, &attr, attr_off)?;
            let new_end = (offset + buffer.len() as u64) as usize;
            let max_resident = self.bytes_per_mft_record as usize / 2;
            if new_end <= max_resident {
                let mut new_data = current_data.clone();
                if new_data.len() < new_end { new_data.resize(new_end, 0); }
                new_data[offset as usize..new_end].copy_from_slice(buffer);
                self.write_resident_data(inode, &record, &attr, attr_off, &new_data)?;
                return Ok(buffer.len());
            }
            let mut full_data = current_data.clone();
            if full_data.len() < new_end { full_data.resize(new_end, 0); }
            full_data[offset as usize..new_end].copy_from_slice(buffer);
            self.convert_to_non_resident(inode, &record, &full_data)?;
            return Ok(buffer.len());
        }
        self.write_non_resident_data(inode, &record, &attr, attr_off, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let record = self.read_mft_record(inode)?;
        self.metadata_from_record(inode, &record)
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut record = self.read_mft_record(inode)?;
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_STANDARD_INFORMATION) {
            let value = self.read_resident_value(&record, &attr, attr_off)?;
            let si = StandardInfoAttr::parse(&value)?;
            let val_off = attr_off + attr.resident_value_offset as usize;
            record[val_off..val_off + 8].copy_from_slice(&unix_to_ntfs_time(metadata.created).to_le_bytes());
            record[val_off + 8..val_off + 16].copy_from_slice(&unix_to_ntfs_time(metadata.modified).to_le_bytes());
            record[val_off + 24..val_off + 32].copy_from_slice(&unix_to_ntfs_time(metadata.accessed).to_le_bytes());
            self.write_mft_record(inode, &record)?;
        }
        if metadata.size != self.metadata(inode)?.size {
            self.truncate_file(inode, metadata.size)?;
        }
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." { return Err(FsError::InvalidArgument); }
        let parent_mft = self.walk(&parent_path, true, 0)?;
        if self.dir_lookup(parent_mft, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let record_num = self.create_mft_record(true, permissions)?;
        self.dir_add_entry(parent_mft, &name, record_num, true)?;
        Ok(record_num)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let mft = self.walk(path, true, 0)?;
        let record = self.read_mft_record(mft)?;
        let header = FileRecordHeader::parse(&record)?;
        if !header.is_directory() { return Err(FsError::NotADirectory); }
        let entries = self.dir_entries(mft)?;
        if !entries.is_empty() { return Err(FsError::DirectoryNotEmpty); }
        let (parent_path, name) = Self::split_path(path);
        let parent_mft = self.walk(&parent_path, true, 0)?;
        let removed = self.dir_remove_entry(parent_mft, &name)?;
        if removed == 0 { return Err(FsError::NotFound); }
        self.free_mft_record(mft)?;
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(path);
        let parent_mft = self.walk(&parent_path, true, 0)?;
        let removed = self.dir_remove_entry(parent_mft, &name)?;
        if removed == 0 { return Err(FsError::NotFound); }
        let record = self.read_mft_record(removed)?;
        let header = FileRecordHeader::parse(&record)?;
        if header.is_directory() {
            self.dir_add_entry(parent_mft, &name, removed, true)?;
            return Err(FsError::IsADirectory);
        }
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_DATA) {
            if attr.non_resident {
                let runs_offset = attr_off + attr.non_resident_data_runs_offset as usize;
                if let Ok(runs) = parse_data_runs(&record, runs_offset) {
                    for run in &runs {
                        if let Some(cluster) = run.start_cluster {
                            self.free_clusters_range(cluster, run.length)?;
                        }
                    }
                }
            }
        }
        self.free_mft_record(removed)?;
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let entries = self.dir_entries(inode)?;
        Ok(entries.into_iter().map(|(name, mft, ft)| DirectoryEntry { name, inode: mft, file_type: ft }).collect())
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent, old_name) = Self::split_path(old_path);
        let (new_parent, new_name) = Self::split_path(new_path);
        let old_parent_mft = self.walk(&old_parent, true, 0)?;
        let new_parent_mft = self.walk(&new_parent, true, 0)?;
        let src_mft = self.dir_lookup(old_parent_mft, &old_name)?;
        if let Ok(dst_mft) = self.dir_lookup(new_parent_mft, &new_name) {
            let dst_record = self.read_mft_record(dst_mft)?;
            let dst_header = FileRecordHeader::parse(&dst_record)?;
            if dst_header.is_directory() {
                let dst_entries = self.dir_entries(dst_mft)?;
                if !dst_entries.is_empty() { return Err(FsError::DirectoryNotEmpty); }
                self.dir_remove_entry(new_parent_mft, &new_name)?;
                self.free_mft_record(dst_mft)?;
            } else {
                self.dir_remove_entry(new_parent_mft, &new_name)?;
                self.free_mft_record(dst_mft)?;
            }
        }
        let src_record = self.read_mft_record(src_mft)?;
        let src_header = FileRecordHeader::parse(&src_record)?;
        self.dir_add_entry(new_parent_mft, &new_name, src_mft, src_header.is_directory())?;
        self.dir_remove_entry(old_parent_mft, &old_name)?;
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(link_path);
        if name.is_empty() { return Err(FsError::InvalidArgument); }
        let parent_mft = self.walk(&parent_path, true, 0)?;
        if self.dir_lookup(parent_mft, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let record_num = self.create_mft_record(false, FilePermissions::from_octal(0o777))?;
        let target_utf16: Vec<u16> = target.encode_utf16().collect();
        let target_bytes: Vec<u8> = target_utf16.iter().flat_map(|cu| cu.to_le_bytes()).collect();
        let mut reparse_data = vec![0u8; 12];
        reparse_data.extend_from_slice(&target_bytes);
        reparse_data.extend_from_slice(&[0, 0]);
        let record = self.read_mft_record(record_num)?;
        if let Ok((attr, attr_off)) = self.find_attribute(&record, ATTR_DATA) {
            self.write_resident_data(record_num, &record, &attr, attr_off, &reparse_data)?;
        }
        self.dir_add_entry(parent_mft, &name, record_num, false)?;
        let mut record = self.read_mft_record(record_num)?;
        let mut offset = FileRecordHeader::parse(&record)?.first_attr_offset as usize;
        loop {
            if offset + 16 > record.len() { break; }
            let attr = match AttributeHeader::parse(&record, offset) { Ok(a) => a, Err(_) => break };
            if attr.attr_type == 0xFFFFFFFF || attr.length == 0 { break; }
            if attr.attr_type == ATTR_FILE_NAME {
                let val_off = offset + attr.resident_value_offset as usize;
                if val_off + 64 <= record.len() {
                    record[val_off + 60..val_off + 64].copy_from_slice(&0xA000000Cu32.to_le_bytes());
                }
            }
            offset += attr.length as usize;
        }
        self.write_mft_record(record_num, &record)?;
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let mft = self.walk(path, false, 0)?;
        let record = self.read_mft_record(mft)?;
        if !self.is_symlink_record(&record)? { return Err(FsError::InvalidArgument); }
        self.read_symlink_target(&record, mft)
    }

    fn sync(&self) -> FsResult<()> {
        if *self.dirty.read() { *self.dirty.write() = false; }
        self.device.flush()
    }
}
