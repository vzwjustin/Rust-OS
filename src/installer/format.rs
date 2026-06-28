//! Minimal ext4 and vfat formatters (superblock-level mkfs).

use crate::drivers::storage::{write_storage_sectors, StorageError};
use alloc::vec;
use alloc::vec::Vec;

const SECTOR: usize = 512;

/// Format a partition as FAT32 (VFAT).
pub fn format_vfat(
    device_id: u32,
    start_sector: u64,
    sector_count: u64,
    volume_label: &str,
) -> Result<(), StorageError> {
    if sector_count < 1024 {
        return Err(StorageError::InvalidSector);
    }

    let bytes_per_sector: u16 = 512;
    let sectors_per_cluster: u8 = 8;
    let reserved_sectors: u16 = 32;
    let num_fats: u8 = 2;
    let total_sectors = sector_count as u32;

    let fat_size = estimate_fat32_size(
        total_sectors,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
    );

    let mut boot = [0u8; SECTOR];
    boot[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    boot[3..11].copy_from_slice(b"RUSTOS  ");
    boot[11..13].copy_from_slice(&bytes_per_sector.to_le_bytes());
    boot[13] = sectors_per_cluster;
    boot[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
    boot[16] = num_fats;
    boot[21] = 0xF8;
    boot[24..26].copy_from_slice(&63u16.to_le_bytes());
    boot[26..28].copy_from_slice(&255u16.to_le_bytes());
    boot[32..36].copy_from_slice(&total_sectors.to_le_bytes());
    boot[36..40].copy_from_slice(&fat_size.to_le_bytes());
    boot[44..48].copy_from_slice(&2u32.to_le_bytes());
    boot[48..50].copy_from_slice(&1u16.to_le_bytes());
    boot[50..52].copy_from_slice(&6u16.to_le_bytes());
    boot[64] = 0x80;
    boot[67..71].copy_from_slice(&0x12345678u32.to_le_bytes());
    write_volume_label(&mut boot[71..82], volume_label);
    boot[82..90].copy_from_slice(b"FAT32   ");
    boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

    write_storage_sectors(device_id, start_sector, &boot)?;

    let mut fsinfo = [0u8; SECTOR];
    fsinfo[0..4].copy_from_slice(&0x41615252u32.to_le_bytes());
    fsinfo[484..488].copy_from_slice(&0x61417272u32.to_le_bytes());
    fsinfo[488..492].copy_from_slice(&0x0FFFFFFEu32.to_le_bytes());
    fsinfo[492..496].copy_from_slice(&2u32.to_le_bytes());
    fsinfo[508..512].copy_from_slice(&0xAA550000u32.to_le_bytes());
    write_storage_sectors(device_id, start_sector + 1, &fsinfo)?;

    let mut fat_sector = [0u8; SECTOR];
    fat_sector[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes());
    fat_sector[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    fat_sector[8..12].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes());
    for fat_idx in 0..num_fats {
        let fat_base = reserved_sectors as u64 + (fat_idx as u64) * fat_size as u64;
        write_storage_sectors(device_id, start_sector + fat_base, &fat_sector)?;
    }

    Ok(())
}

fn estimate_fat32_size(total_sectors: u32, spc: u8, reserved: u16, _fats: u8) -> u32 {
    let data_sectors = total_sectors.saturating_sub(reserved as u32);
    let clusters = data_sectors / spc as u32;
    let fat_bytes = (clusters * 4).next_multiple_of(SECTOR as u32);
    (fat_bytes / SECTOR as u32).max(1)
}

fn write_volume_label(dst: &mut [u8], label: &str) {
    for (i, b) in label.bytes().take(11).enumerate() {
        dst[i] = b;
    }
    for i in label.len()..11 {
        dst[i] = b' ';
    }
}

/// Format a partition as ext4; returns a writer handle for populating files.
pub fn format_ext4(
    device_id: u32,
    start_sector: u64,
    sector_count: u64,
    volume_label: &str,
) -> Result<Ext4Volume, StorageError> {
    if sector_count < 8192 {
        return Err(StorageError::InvalidSector);
    }

    let block_size: u32 = 4096;
    let blocks_per_group = 32768u32;
    let total_blocks = ((sector_count * 512) / block_size as u64) as u32;
    let inode_size: u16 = 256;
    let inodes_per_group = 8192u16;

    let mut superblock = [0u8; 1024];
    superblock[0x38..0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());
    superblock[0x18..0x1C].copy_from_slice(&total_blocks.to_le_bytes());
    superblock[0x1C..0x20].copy_from_slice(&blocks_per_group.to_le_bytes());
    superblock[0x28..0x2A].copy_from_slice(&inodes_per_group.to_le_bytes());
    superblock[0x3A..0x3C].copy_from_slice(&1u16.to_le_bytes());
    superblock[0x58] = 12;
    superblock[0x5C..0x5E].copy_from_slice(&inode_size.to_le_bytes());
    superblock[0x60..0x64].copy_from_slice(&volume_label_bytes(volume_label));

    let mut block0 = [0u8; 4096];
    block0[1024..2048].copy_from_slice(&superblock);
    write_partition_blocks(device_id, start_sector, 0, &block0)?;

    let mut bg = [0u8; 32];
    bg[0..4].copy_from_slice(&1u32.to_le_bytes());
    bg[4..8].copy_from_slice(&2u32.to_le_bytes());
    bg[8..12].copy_from_slice(&3u32.to_le_bytes());
    bg[0x0E..0x10].copy_from_slice(&(inodes_per_group - 1).to_le_bytes());
    bg[0x10..0x12].copy_from_slice(&0xEF53u16.to_le_bytes());
    let mut gd_block = [0u8; 4096];
    gd_block[..32].copy_from_slice(&bg);
    write_partition_blocks(device_id, start_sector, 1, &gd_block)?;

    let mut block_bitmap = [0u8; 4096];
    for b in 0..11u32 {
        set_bitmap_bit(&mut block_bitmap, b);
    }
    write_partition_blocks(device_id, start_sector, 1, &block_bitmap)?;

    let mut inode_bitmap = [0u8; 4096];
    set_bitmap_bit(&mut inode_bitmap, 1);
    write_partition_blocks(device_id, start_sector, 2, &inode_bitmap)?;

    let mut root_inode = [0u8; 256];
    root_inode[0..2].copy_from_slice(&0o040755u16.to_le_bytes());
    root_inode[4..8].copy_from_slice(&4096u32.to_le_bytes());
    root_inode[28..32].copy_from_slice(&1u32.to_le_bytes());
    root_inode[32..36].copy_from_slice(&2u32.to_le_bytes());
    root_inode[40..44].copy_from_slice(&10u32.to_le_bytes());
    let mut inode_table = [0u8; 4096];
    inode_table[256..512].copy_from_slice(&root_inode);
    write_partition_blocks(device_id, start_sector, 3, &inode_table)?;

    let mut root_dir = [0u8; 4096];
    write_dir_entry(&mut root_dir[0..], ".", 2, 2);
    write_dir_entry(&mut root_dir[256..], "..", 2, 2);
    write_partition_blocks(device_id, start_sector, 10, &root_dir)?;

    Ok(Ext4Volume {
        device_id,
        start_sector,
        block_size,
        total_blocks,
        next_data_block: 11,
        next_inode: 3,
        dir_blocks: Vec::new(),
    })
}

fn volume_label_bytes(label: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (i, b) in label.bytes().take(4).enumerate() {
        out[i] = b;
    }
    out
}

fn set_bitmap_bit(bitmap: &mut [u8], bit: u32) {
    let byte = (bit / 8) as usize;
    let mask = 1u8 << (bit % 8);
    if byte < bitmap.len() {
        bitmap[byte] |= mask;
    }
}

fn write_dir_entry(dst: &mut [u8], name: &str, inode: u32, file_type: u8) {
    dst[0..4].copy_from_slice(&inode.to_le_bytes());
    dst[4..6].copy_from_slice(&256u16.to_le_bytes());
    dst[6] = name.len() as u8;
    dst[7] = file_type;
    let bytes = name.as_bytes();
    dst[8..8 + bytes.len()].copy_from_slice(bytes);
}

fn write_partition_blocks(
    device_id: u32,
    start_sector: u64,
    block_index: u32,
    data: &[u8],
) -> Result<(), StorageError> {
    let sector_offset = start_sector + (block_index as u64) * 8;
    for (chunk_idx, chunk) in data.chunks(SECTOR).enumerate() {
        let mut sector = [0u8; SECTOR];
        let len = chunk.len().min(SECTOR);
        sector[..len].copy_from_slice(&chunk[..len]);
        write_storage_sectors(device_id, sector_offset + chunk_idx as u64, &sector)?;
    }
    Ok(())
}

/// Writable ext4 volume for installer file copies.
pub struct Ext4Volume {
    pub device_id: u32,
    pub start_sector: u64,
    pub block_size: u32,
    pub total_blocks: u32,
    pub next_data_block: u32,
    pub next_inode: u32,
    dir_blocks: Vec<(u32, u32)>,
}

impl Ext4Volume {
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), StorageError> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(());
        }
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let filename = *parts.last().ok_or(StorageError::InvalidSector)?;
        let parent_inode = if parts.len() == 1 {
            2u32
        } else {
            self.ensure_path(&parts[..parts.len() - 1])?
        };
        self.create_file_in_dir(parent_inode, filename, data)
    }

    fn ensure_path(&mut self, parts: &[&str]) -> Result<u32, StorageError> {
        let mut current_inode = 2u32;
        for &name in parts {
            current_inode = self.create_dir_in_dir(current_inode, name)?;
        }
        Ok(current_inode)
    }

    fn create_dir_in_dir(&mut self, parent_inode: u32, name: &str) -> Result<u32, StorageError> {
        let inode = self.next_inode;
        self.next_inode += 1;
        let data_block = self.next_data_block;
        self.next_data_block += 1;

        let mut dir_block = [0u8; 4096];
        write_dir_entry(&mut dir_block[0..], ".", inode, 2);
        write_dir_entry(&mut dir_block[256..], "..", parent_inode, 2);
        write_partition_blocks(self.device_id, self.start_sector, data_block, &dir_block)?;
        self.dir_blocks.push((inode, data_block));

        self.write_inode(inode, 0o040755, 4096, data_block)?;
        self.append_dir_entry(parent_inode, name, inode, 2)?;
        Ok(inode)
    }

    fn create_file_in_dir(
        &mut self,
        parent_inode: u32,
        name: &str,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let inode = self.next_inode;
        self.next_inode += 1;
        let data_block = self.next_data_block;
        self.next_data_block += 1;

        let mut first_block = data_block;
        let mut blocks_used = 1u32;
        let mut offset = 0usize;
        while offset < data.len() {
            let end = core::cmp::min(offset + 4096, data.len());
            let chunk = &data[offset..end];
            let mut block = [0u8; 4096];
            block[..chunk.len()].copy_from_slice(chunk);
            write_partition_blocks(self.device_id, self.start_sector, first_block, &block)?;
            offset = end;
            if offset < data.len() {
                first_block = self.next_data_block;
                self.next_data_block += 1;
                blocks_used += 1;
            }
        }

        self.write_inode_multi(inode, 0o100644, data.len() as u32, data_block, blocks_used)?;
        self.append_dir_entry(parent_inode, name, inode, 1)?;
        Ok(())
    }

    fn write_inode_multi(
        &self,
        inode_num: u32,
        mode: u16,
        size: u32,
        first_block: u32,
        block_count: u32,
    ) -> Result<(), StorageError> {
        let mut inode = [0u8; 256];
        inode[0..2].copy_from_slice(&mode.to_le_bytes());
        inode[4..8].copy_from_slice(&size.to_le_bytes());
        inode[28..32].copy_from_slice(&1u32.to_le_bytes());
        inode[32..36].copy_from_slice(&inode_num.to_le_bytes());
        inode[40..44].copy_from_slice(&first_block.to_le_bytes());
        for i in 1..block_count.min(12) {
            let off = 40 + (i as usize * 4);
            inode[off..off + 4].copy_from_slice(&(first_block + i).to_le_bytes());
        }
        let table_block = 3 + (inode_num - 1) / 16;
        let offset = ((inode_num - 1) % 16) as usize * 256;
        let mut table = [0u8; 4096];
        table[offset..offset + 256].copy_from_slice(&inode);
        write_partition_blocks(self.device_id, self.start_sector, table_block, &table)?;
        Ok(())
    }

    fn write_inode(
        &self,
        inode_num: u32,
        mode: u16,
        size: u32,
        data_block: u32,
    ) -> Result<(), StorageError> {
        let mut inode = [0u8; 256];
        inode[0..2].copy_from_slice(&mode.to_le_bytes());
        inode[4..8].copy_from_slice(&size.to_le_bytes());
        inode[28..32].copy_from_slice(&1u32.to_le_bytes());
        inode[32..36].copy_from_slice(&inode_num.to_le_bytes());
        inode[40..44].copy_from_slice(&data_block.to_le_bytes());
        let table_block = 3 + (inode_num - 1) / 16;
        let offset = ((inode_num - 1) % 16) as usize * 256;
        let mut table = [0u8; 4096];
        table[offset..offset + 256].copy_from_slice(&inode);
        write_partition_blocks(self.device_id, self.start_sector, table_block, &table)?;
        Ok(())
    }

    fn append_dir_entry(
        &mut self,
        dir_inode: u32,
        name: &str,
        child_inode: u32,
        file_type: u8,
    ) -> Result<(), StorageError> {
        let dir_block_num = if dir_inode == 2 {
            10
        } else {
            self.dir_blocks
                .iter()
                .find(|(ino, _)| *ino == dir_inode)
                .map(|(_, blk)| *blk)
                .unwrap_or(10)
        };
        let mut dir_data = [0u8; 4096];
        let mut offset = 512usize;
        write_dir_entry(&mut dir_data[offset..], name, child_inode, file_type);
        let _ = offset;
        write_partition_blocks(self.device_id, self.start_sector, dir_block_num, &dir_data)?;
        Ok(())
    }
}
