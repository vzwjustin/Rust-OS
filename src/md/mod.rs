//! Software RAID (md) — linear and RAID0 metadata parsing + device registration

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::block_io::{self, Bio, BlockDeviceOps};
use crate::drivers::storage::{read_storage_sectors, with_storage_manager};

pub const MD_SB_MAGIC: u32 = 0xA92B4EFC;
pub const MD_LEVEL_LINEAR: i32 = -1;
pub const MD_LEVEL_RAID0: i32 = 0;
pub const MD_SB_1_OFFSET: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdLevel {
    Linear,
    Raid0,
    Unknown(i32),
}

impl From<i32> for MdLevel {
    fn from(level: i32) -> Self {
        match level {
            MD_LEVEL_LINEAR => MdLevel::Linear,
            MD_LEVEL_RAID0 => MdLevel::Raid0,
            other => MdLevel::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MdSuperblock1 {
    pub magic: u32,
    pub major_version: u32,
    pub level: MdLevel,
    pub size_sectors: u64,
    pub chunk_sectors: u32,
    pub uuid: [u8; 16],
}

#[derive(Debug, Clone)]
struct MdMember {
    storage_device_id: u32,
}

#[derive(Debug, Clone)]
struct MdArray {
    id: u32,
    name: String,
    level: MdLevel,
    size_sectors: u64,
    chunk_sectors: u32,
    members: Vec<MdMember>,
}

static NEXT_MD: AtomicU32 = AtomicU32::new(0);
static MD_ARRAYS: RwLock<BTreeMap<u32, MdArray>> = RwLock::new(BTreeMap::new());

fn le32(buf: &[u8]) -> u32 {
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}

fn le64(buf: &[u8]) -> u64 {
    u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ])
}

/// Parse Linux md version-1 superblock from a buffer (>= 128 bytes).
pub fn parse_superblock_1(data: &[u8]) -> Result<MdSuperblock1, &'static str> {
    if data.len() < 128 {
        return Err("superblock too small");
    }

    let magic = le32(&data[0..4]);
    if magic != MD_SB_MAGIC {
        return Err("bad md magic");
    }

    let major_version = le32(&data[4..8]);
    if major_version != 1 {
        return Err("unsupported md major version");
    }

    let level = MdLevel::from(le32(&data[40..44]) as i32);
    let chunk_sectors = le32(&data[44..48]);
    let size_sectors = le64(&data[48..56]);

    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&data[16..32]);

    Ok(MdSuperblock1 {
        magic,
        major_version,
        level,
        size_sectors,
        chunk_sectors,
        uuid,
    })
}

fn read_superblock_from_storage(device_id: u32) -> Result<MdSuperblock1, &'static str> {
    let mut buf = [0u8; 4096];
    read_storage_sectors(device_id, MD_SB_1_OFFSET / 512, &mut buf)
        .map_err(|_| "read superblock failed")?;
    parse_superblock_1(&buf)
}

fn md_read_linear(array: &MdArray, sector: u64, buf: &mut [u8]) -> Result<(), &'static str> {
    let member = array.members.first().ok_or("no md members")?;
    with_storage_manager(|mgr| mgr.read_sectors(member.storage_device_id, sector, buf))
        .ok_or("storage manager unavailable")?
        .map_err(|_| "member read failed")?;
    Ok(())
}

fn md_write_linear(array: &MdArray, sector: u64, buf: &[u8]) -> Result<(), &'static str> {
    let member = array.members.first().ok_or("no md members")?;
    with_storage_manager(|mgr| mgr.write_sectors(member.storage_device_id, sector, buf))
        .ok_or("storage manager unavailable")?
        .map_err(|_| "member write failed")?;
    Ok(())
}

fn md_read_raid0(array: &MdArray, sector: u64, buf: &mut [u8]) -> Result<(), &'static str> {
    let chunk = array.chunk_sectors.max(1) as u64;
    let members = array.members.len().max(1) as u64;
    let stripe = sector / chunk;
    let member_idx = (stripe % members) as usize;
    let chunk_base = (stripe / members) * chunk + (sector % chunk);
    let member = array
        .members
        .get(member_idx)
        .ok_or("raid0 member missing")?;

    with_storage_manager(|mgr| mgr.read_sectors(member.storage_device_id, chunk_base, buf))
        .ok_or("storage manager unavailable")?
        .map_err(|_| "raid0 read failed")?;
    Ok(())
}

fn md_write_raid0(array: &MdArray, sector: u64, buf: &[u8]) -> Result<(), &'static str> {
    let chunk = array.chunk_sectors.max(1) as u64;
    let members = array.members.len().max(1) as u64;
    let stripe = sector / chunk;
    let member_idx = (stripe % members) as usize;
    let chunk_base = (stripe / members) * chunk + (sector % chunk);
    let member = array
        .members
        .get(member_idx)
        .ok_or("raid0 member missing")?;

    with_storage_manager(|mgr| mgr.write_sectors(member.storage_device_id, chunk_base, buf))
        .ok_or("storage manager unavailable")?
        .map_err(|_| "raid0 write failed")?;
    Ok(())
}

fn md_read_for_array(array: &MdArray, sector: u64, buf: &mut [u8]) -> Result<(), &'static str> {
    match array.level {
        MdLevel::Linear => md_read_linear(array, sector, buf),
        MdLevel::Raid0 => md_read_raid0(array, sector, buf),
        MdLevel::Unknown(_) => Err("unsupported md level"),
    }
}

fn md_write_for_array(array: &MdArray, sector: u64, buf: &[u8]) -> Result<(), &'static str> {
    match array.level {
        MdLevel::Linear => md_write_linear(array, sector, buf),
        MdLevel::Raid0 => md_write_raid0(array, sector, buf),
        MdLevel::Unknown(_) => Err("unsupported md level"),
    }
}

fn md_get_capacity(array_id: u32) -> u64 {
    MD_ARRAYS
        .read()
        .get(&array_id)
        .map(|a| a.size_sectors)
        .unwrap_or(0)
}

fn md_get_name(_array_id: u32) -> &'static str {
    "md"
}

fn md_submit_bio(array_id: u32, bio: &mut Bio) -> Result<(), &'static str> {
    let array = MD_ARRAYS
        .read()
        .get(&array_id)
        .cloned()
        .ok_or("md array missing")?;

    match bio.bi_dir {
        block_io::BioDirection::Read => md_read_for_array(&array, bio.bi_sector, &mut bio.bi_data),
        block_io::BioDirection::Write => md_write_for_array(&array, bio.bi_sector, &bio.bi_data),
        block_io::BioDirection::Flush | block_io::BioDirection::Discard => Ok(()),
    }
}

fn register_md_block_device(array: &MdArray) -> Result<(), &'static str> {
    let array_id = array.id;

    let ops = BlockDeviceOps {
        submit_bio: md_submit_bio,
        get_capacity: md_get_capacity,
        get_name: md_get_name,
        driver_data: array_id,
    };

    block_io::register_block_device_major(&array.name, 9, array.id, ops)
}

pub fn scan_and_register() -> MdScanResult {
    let mut result = MdScanResult::default();
    let devices = crate::drivers::storage::get_storage_device_list();

    for info in devices {
        let sb = match read_superblock_from_storage(info.id) {
            Ok(sb) => sb,
            Err(_) => continue,
        };

        if !matches!(sb.level, MdLevel::Linear | MdLevel::Raid0) {
            result.errors.push(format!(
                "md on storage {}: unsupported level {:?}",
                info.id, sb.level
            ));
            continue;
        }

        let id = NEXT_MD.fetch_add(1, Ordering::SeqCst);
        let name = format!("md{}", id);
        let array = MdArray {
            id,
            name: name.clone(),
            level: sb.level,
            size_sectors: sb.size_sectors,
            chunk_sectors: sb.chunk_sectors.max(1),
            members: vec![MdMember {
                storage_device_id: info.id,
            }],
        };

        match register_md_block_device(&array) {
            Ok(()) => {
                MD_ARRAYS.write().insert(id, array);
                crate::serial_println!(
                    "md: registered {} level={:?} size={} sectors (member storage {})",
                    name,
                    sb.level,
                    sb.size_sectors,
                    info.id
                );
                result.arrays_registered += 1;
            }
            Err(e) => result.errors.push(format!("{}: {}", name, e)),
        }
    }

    result
}

pub fn init() -> MdScanResult {
    scan_and_register()
}

#[derive(Debug, Clone, Default)]
pub struct MdScanResult {
    pub arrays_registered: usize,
    pub errors: Vec<String>,
}

pub fn array_count() -> usize {
    MD_ARRAYS.read().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_linear_superblock() {
        let mut data = [0u8; 128];
        data[0..4].copy_from_slice(&MD_SB_MAGIC.to_le_bytes());
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[40..44].copy_from_slice(&MD_LEVEL_LINEAR.to_le_bytes());
        data[44..48].copy_from_slice(&128u32.to_le_bytes());
        data[48..56].copy_from_slice(&2048u64.to_le_bytes());

        let sb = parse_superblock_1(&data).expect("parse");
        assert_eq!(sb.level, MdLevel::Linear);
        assert_eq!(sb.size_sectors, 2048);
    }
}
