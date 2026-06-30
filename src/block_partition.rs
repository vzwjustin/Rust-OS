//! Partition table parsing — MBR (DOS) and GPT.
//!
//! Ported from Linux block/partitions/msdos.c and block/partitions/efi.c
//! concepts. Pure parsing logic: callers supply a sector-read closure and
//! get back a list of discovered partitions (start LBA + size + type).
//! This module has no dependency on `block_io` so it can be unit-friendly
//! and reused by anything that can hand it raw sectors.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::block_io::SECTOR_SIZE;

/// A single discovered partition, independent of table format.
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    /// 1-based partition number (matches Linux's sdaN / nvme0n1pN numbering).
    pub number: u32,
    pub start_lba: u64,
    pub size_sectors: u64,
    /// Human-readable type (e.g. "Linux", "EFI System", "GPT-<guid>").
    pub type_name: String,
    pub bootable: bool,
}

const MBR_SIGNATURE: u16 = 0xAA55;
const MBR_PARTITION_TABLE_OFFSET: usize = 0x1BE;
const MBR_ENTRY_SIZE: usize = 16;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// Result of probing a device's first sectors for a partition table.
pub enum TableKind {
    None,
    Mbr,
    /// Protective MBR present, real table is GPT.
    Gpt,
}

/// Inspect sector 0 to decide what table format (if any) is present.
pub fn detect_table_kind(sector0: &[u8]) -> TableKind {
    if sector0.len() < 512 {
        return TableKind::None;
    }
    let sig = u16::from_le_bytes([sector0[510], sector0[511]]);
    if sig != MBR_SIGNATURE {
        return TableKind::None;
    }
    // A GPT disk has a single "protective" MBR entry of type 0xEE covering
    // the whole disk (or as much as a 32-bit LBA can express).
    let ptype = sector0[MBR_PARTITION_TABLE_OFFSET + 4];
    if ptype == 0xEE {
        TableKind::Gpt
    } else {
        TableKind::Mbr
    }
}

/// Translate an MBR partition type byte to a human-readable name.
fn mbr_type_name(ptype: u8) -> String {
    match ptype {
        0x00 => String::from("Empty"),
        0x07 => String::from("NTFS/exFAT"),
        0x0B | 0x0C => String::from("FAT32"),
        0x82 => String::from("Linux swap"),
        0x83 => String::from("Linux"),
        0x8E => String::from("Linux LVM"),
        0xEE => String::from("GPT protective"),
        0xFD => String::from("Linux RAID"),
        other => format!("0x{:02X}", other),
    }
}

/// Parse a classic DOS/MBR partition table out of sector 0.
/// Only primary partitions are parsed (no extended/logical chain), which
/// covers the overwhelming majority of disks RustOS will see.
pub fn parse_mbr(sector0: &[u8]) -> Vec<PartitionInfo> {
    let mut out = Vec::new();
    if sector0.len() < 512 {
        return out;
    }
    if u16::from_le_bytes([sector0[510], sector0[511]]) != MBR_SIGNATURE {
        return out;
    }

    for i in 0..4 {
        let off = MBR_PARTITION_TABLE_OFFSET + i * MBR_ENTRY_SIZE;
        let entry = &sector0[off..off + MBR_ENTRY_SIZE];
        let status = entry[0];
        let ptype = entry[4];
        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
        let size_sectors =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;

        if ptype == 0x00 || size_sectors == 0 {
            continue;
        }

        out.push(PartitionInfo {
            number: (i + 1) as u32,
            start_lba,
            size_sectors,
            type_name: mbr_type_name(ptype),
            bootable: status == 0x80,
        });
    }

    out
}

/// Minimal GPT header, just enough to locate and walk the entry array.
struct GptHeader {
    partition_entry_lba: u64,
    num_partition_entries: u32,
    size_of_partition_entry: u32,
}

fn parse_gpt_header(sector1: &[u8]) -> Option<GptHeader> {
    if sector1.len() < 92 {
        return None;
    }
    if &sector1[0..8] != GPT_SIGNATURE {
        return None;
    }
    let partition_entry_lba = u64::from_le_bytes(sector1[72..80].try_into().ok()?);
    let num_partition_entries = u32::from_le_bytes(sector1[80..84].try_into().ok()?);
    let size_of_partition_entry = u32::from_le_bytes(sector1[84..88].try_into().ok()?);
    Some(GptHeader {
        partition_entry_lba,
        num_partition_entries,
        size_of_partition_entry,
    })
}

/// Parse a GPT partition entry array given the raw bytes of the entry
/// table (caller is responsible for reading `num_entries * entry_size`
/// bytes starting at `partition_entry_lba`, which `gpt_entry_table_span`
/// helps compute).
fn parse_gpt_entries(header: &GptHeader, table: &[u8]) -> Vec<PartitionInfo> {
    let mut out = Vec::new();
    let entry_size = header.size_of_partition_entry as usize;
    if entry_size < 128 {
        return out;
    }

    for i in 0..header.num_partition_entries as usize {
        let off = i * entry_size;
        if off + 128 > table.len() {
            break;
        }
        let entry = &table[off..off + entry_size.min(table.len() - off)];

        // All-zero type GUID means the slot is unused.
        if entry[0..16].iter().all(|&b| b == 0) {
            continue;
        }

        let first_lba = u64::from_le_bytes(entry[32..40].try_into().unwrap());
        let last_lba = u64::from_le_bytes(entry[40..48].try_into().unwrap());
        if last_lba < first_lba {
            continue;
        }

        // Partition name is a UTF-16LE string in bytes [56..128).
        let name_bytes = &entry[56..128.min(entry.len())];
        let mut name = String::new();
        for chunk in name_bytes.chunks_exact(2) {
            let cu = u16::from_le_bytes([chunk[0], chunk[1]]);
            if cu == 0 {
                break;
            }
            if let Some(c) = char::from_u32(cu as u32) {
                name.push(c);
            }
        }
        let type_name = if name.is_empty() {
            String::from("GPT partition")
        } else {
            name
        };

        out.push(PartitionInfo {
            number: (i + 1) as u32,
            start_lba: first_lba,
            size_sectors: last_lba - first_lba + 1,
            type_name,
            bootable: false,
        });
    }

    out
}

/// Number of sectors that must be read, starting at the GPT header's
/// `partition_entry_lba`, to cover the full partition entry array.
pub fn gpt_entry_table_span(header_sector: &[u8]) -> Option<(u64, usize)> {
    let header = parse_gpt_header(header_sector)?;
    let bytes = header.num_partition_entries as usize * header.size_of_partition_entry as usize;
    let sectors = bytes.div_ceil(SECTOR_SIZE);
    Some((header.partition_entry_lba, sectors.max(1) * SECTOR_SIZE))
}

/// Parse a full GPT table given the LBA1 header sector and the raw bytes
/// of the partition entry array (as sized by `gpt_entry_table_span`).
pub fn parse_gpt(header_sector: &[u8], entry_table: &[u8]) -> Vec<PartitionInfo> {
    match parse_gpt_header(header_sector) {
        Some(header) => parse_gpt_entries(&header, entry_table),
        None => Vec::new(),
    }
}
