//! GPT/MBR partition table writer for installer layouts.

use crate::drivers::storage::{write_storage_sectors, StorageError};
use alloc::vec;

use super::plan::PartitionLayout;

const SECTOR_SIZE: usize = 512;
const EFI_SIZE_BYTES: u64 = 512 * 1024 * 1024;
const SWAP_SIZE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const GPT_HEADER_LBA: u64 = 1;
const GPT_ENTRIES_LBA: u64 = 2;
const GPT_ENTRY_COUNT: usize = 128;
const GPT_ENTRY_SIZE: usize = 128;
const FIRST_USABLE_LBA: u64 = 34;

/// Create a standard GPT layout: EFI (FAT32) + root (ext4) + optional swap.
pub fn create_gpt_layout(
    device_id: u32,
    erase_disk: bool,
) -> Result<PartitionLayout, StorageError> {
    let total_sectors = device_capacity_sectors(device_id)?;
    if total_sectors < 64 * 1024 {
        return Err(StorageError::InvalidSector);
    }

    if erase_disk {
        wipe_disk_header(device_id)?;
    }

    let efi_sectors = EFI_SIZE_BYTES / 512;
    let efi_start = FIRST_USABLE_LBA;
    let efi_end = efi_start + efi_sectors - 1;

    let swap_sectors = SWAP_SIZE_BYTES / 512;
    let (swap_start, swap_count) = if total_sectors > efi_end + 64 * 1024 + swap_sectors {
        let swap_start = total_sectors.saturating_sub(swap_sectors);
        (Some(swap_start), Some(swap_sectors))
    } else {
        (None, None)
    };

    let root_start = efi_end + 1;
    let root_end = swap_start.unwrap_or(total_sectors).saturating_sub(1);
    if root_end <= root_start {
        return Err(StorageError::InvalidSector);
    }
    let root_sectors = root_end - root_start + 1;

    let mut parts = vec![
        GptPartSpec {
            type_guid: TYPE_EFI,
            start: efi_start,
            end: efi_end,
            name: "EFI System",
        },
        GptPartSpec {
            type_guid: TYPE_LINUX_FS,
            start: root_start,
            end: root_end,
            name: "RustOS Root",
        },
    ];
    if let Some(start) = swap_start {
        parts.push(GptPartSpec {
            type_guid: TYPE_LINUX_SWAP,
            start,
            end: start + swap_sectors - 1,
            name: "Swap",
        });
    }

    write_gpt(device_id, total_sectors, &parts)?;

    Ok(PartitionLayout {
        device_id,
        efi_start_sector: efi_start,
        efi_sector_count: efi_sectors,
        root_start_sector: root_start,
        root_sector_count: root_sectors,
        swap_start_sector: swap_start,
        swap_sector_count: swap_count,
    })
}

struct GptPartSpec {
    type_guid: [u8; 16],
    start: u64,
    end: u64,
    name: &'static str,
}

const TYPE_EFI: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
const TYPE_LINUX_FS: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xF4,
];
const TYPE_LINUX_SWAP: [u8; 16] = [
    0xAF, 0x9B, 0x60, 0xA0, 0xEB, 0x11, 0xD4, 0x11, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xF4,
];

fn device_capacity_sectors(device_id: u32) -> Result<u64, StorageError> {
    let devices = crate::drivers::storage::get_storage_device_list();
    let dev = devices
        .into_iter()
        .find(|d| d.id == device_id)
        .ok_or(StorageError::DeviceNotFound)?;
    let sectors = dev.capabilities.capacity_bytes / 512;
    if sectors == 0 {
        return Err(StorageError::InvalidSector);
    }
    Ok(sectors)
}

fn wipe_disk_header(device_id: u32) -> Result<(), StorageError> {
    let sector = [0u8; SECTOR_SIZE];
    for lba in 0..=GPT_ENTRIES_LBA + 32 {
        write_storage_sectors(device_id, lba, &sector)?;
    }
    Ok(())
}

fn write_gpt(
    device_id: u32,
    total_sectors: u64,
    parts: &[GptPartSpec],
) -> Result<(), StorageError> {
    write_protective_mbr(device_id, total_sectors)?;
    write_gpt_header(device_id, total_sectors, parts.len())?;
    write_gpt_entries(device_id, parts)?;
    Ok(())
}

fn write_protective_mbr(device_id: u32, total_sectors: u64) -> Result<(), StorageError> {
    let mut mbr = [0u8; SECTOR_SIZE];
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    mbr[450] = 0xEE;
    let start = 1u32;
    let size = if total_sectors > u32::MAX as u64 {
        u32::MAX
    } else {
        (total_sectors - 1) as u32
    };
    mbr[454..458].copy_from_slice(&start.to_le_bytes());
    mbr[458..462].copy_from_slice(&size.to_le_bytes());
    write_storage_sectors(device_id, 0, &mbr)?;
    Ok(())
}

fn write_gpt_header(
    device_id: u32,
    total_sectors: u64,
    part_count: usize,
) -> Result<(), StorageError> {
    let mut hdr = [0u8; SECTOR_SIZE];
    hdr[0..8].copy_from_slice(b"EFI PART");
    hdr[8..12].copy_from_slice(&[0x00, 0x00, 0x01, 0x00]);
    hdr[12..16].copy_from_slice(&92u32.to_le_bytes());
    hdr[24..32].copy_from_slice(&0u64.to_le_bytes());
    hdr[32..40].copy_from_slice(&GPT_HEADER_LBA.to_le_bytes());
    hdr[40..48].copy_from_slice(&hdr_backup_lba(total_sectors).to_le_bytes());
    hdr[48..56].copy_from_slice(&FIRST_USABLE_LBA.to_le_bytes());
    hdr[56..64].copy_from_slice(&last_usable_lba(total_sectors).to_le_bytes());
    let disk_guid: [u8; 16] = [
        0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07, 0x18, 0x29, 0x3A, 0x4B, 0x5C, 0x6D, 0x7E, 0x8F,
        0x90,
    ];
    hdr[56..72].copy_from_slice(&disk_guid);
    hdr[72..80].copy_from_slice(&GPT_ENTRIES_LBA.to_le_bytes());
    hdr[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    hdr[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    hdr[88..92].copy_from_slice(&(part_count as u32).to_le_bytes());
    write_storage_sectors(device_id, GPT_HEADER_LBA, &hdr)?;
    Ok(())
}

fn hdr_backup_lba(total_sectors: u64) -> u64 {
    total_sectors.saturating_sub(1)
}

fn last_usable_lba(total_sectors: u64) -> u64 {
    total_sectors.saturating_sub(34)
}

fn write_gpt_entries(device_id: u32, parts: &[GptPartSpec]) -> Result<(), StorageError> {
    let mut sector = [0u8; SECTOR_SIZE];
    let mut entry_index = 0usize;

    for part in parts {
        let base = (entry_index % 4) * GPT_ENTRY_SIZE;
        if entry_index > 0 && entry_index % 4 == 0 {
            let lba = GPT_ENTRIES_LBA + (entry_index / 4) as u64 - 1;
            write_storage_sectors(device_id, lba, &sector)?;
            sector = [0u8; SECTOR_SIZE];
        }

        sector[base..base + 16].copy_from_slice(&part.type_guid);
        let unique: [u8; 16] = [
            0x10,
            0x20,
            0x30,
            0x40,
            0x50,
            0x60,
            0x70,
            0x80,
            (entry_index as u8).wrapping_add(1),
            0xAA,
            0xBB,
            0xCC,
            0xDD,
            0xEE,
            0xFF,
            0x00,
        ];
        sector[base + 16..base + 32].copy_from_slice(&unique);
        sector[base + 32..base + 40].copy_from_slice(&part.start.to_le_bytes());
        sector[base + 40..base + 48].copy_from_slice(&part.end.to_le_bytes());
        write_utf16_name(&mut sector[base + 56..base + 128], part.name);
        entry_index += 1;
    }

    let lba = GPT_ENTRIES_LBA + (entry_index.saturating_sub(1) / 4) as u64;
    write_storage_sectors(device_id, lba, &sector)?;
    Ok(())
}

fn write_utf16_name(dst: &mut [u8], name: &str) {
    let mut i = 0usize;
    for ch in name.encode_utf16() {
        if i + 2 > dst.len() {
            break;
        }
        dst[i..i + 2].copy_from_slice(&ch.to_le_bytes());
        i += 2;
    }
}

/// Write a legacy MBR with EFI + root + swap primaries.
pub fn create_mbr_layout(
    device_id: u32,
    erase_disk: bool,
) -> Result<PartitionLayout, StorageError> {
    let total_sectors = device_capacity_sectors(device_id)?;
    if erase_disk {
        wipe_disk_header(device_id)?;
    }

    let efi_sectors = EFI_SIZE_BYTES / 512;
    let efi_start = 2048u64;
    let root_start = efi_start + efi_sectors;
    let swap_sectors = SWAP_SIZE_BYTES / 512;
    let swap_start = total_sectors.saturating_sub(swap_sectors);
    let root_sectors = swap_start.saturating_sub(root_start);
    if root_sectors < 64 * 1024 {
        return Err(StorageError::InvalidSector);
    }

    let mut mbr = [0u8; SECTOR_SIZE];
    write_mbr_entry(
        &mut mbr[446..462],
        0x80,
        0xEF,
        efi_start as u32,
        efi_sectors as u32,
    );
    write_mbr_entry(
        &mut mbr[462..478],
        0x00,
        0x83,
        root_start as u32,
        root_sectors as u32,
    );
    write_mbr_entry(
        &mut mbr[478..494],
        0x00,
        0x82,
        swap_start as u32,
        swap_sectors as u32,
    );
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    write_storage_sectors(device_id, 0, &mbr)?;

    Ok(PartitionLayout {
        device_id,
        efi_start_sector: efi_start,
        efi_sector_count: efi_sectors,
        root_start_sector: root_start,
        root_sector_count: root_sectors,
        swap_start_sector: Some(swap_start),
        swap_sector_count: Some(swap_sectors),
    })
}

fn write_mbr_entry(slot: &mut [u8], boot: u8, ptype: u8, start: u32, sectors: u32) {
    slot[0] = boot;
    slot[4] = ptype;
    slot[8..12].copy_from_slice(&start.to_le_bytes());
    slot[12..16].copy_from_slice(&sectors.to_le_bytes());
}

/// Prefer GPT; fall back to MBR on failure.
pub fn create_partition_layout(
    device_id: u32,
    erase_disk: bool,
) -> Result<PartitionLayout, StorageError> {
    match create_gpt_layout(device_id, erase_disk) {
        Ok(layout) => Ok(layout),
        Err(_) => create_mbr_layout(device_id, erase_disk),
    }
}
