//! Disk enumeration via storage subsystem APIs.

use crate::drivers::storage::{
    get_storage_device_list, read_mbr_partitions, StorageDeviceInfo, StorageDeviceState,
};
use alloc::string::String;
use alloc::vec::Vec;

/// Partition summary for installer UI.
#[derive(Debug, Clone)]
pub struct DiskPartitionInfo {
    pub number: u32,
    pub start_sector: u64,
    pub sector_count: u64,
    pub size_bytes: u64,
    pub type_name: String,
    pub bootable: bool,
}

/// Disk visible to the installer wizard.
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub id: u32,
    pub name: String,
    pub size_bytes: u64,
    pub model: String,
    pub ready: bool,
    pub partitions: Vec<DiskPartitionInfo>,
}

impl DiskInfo {
    pub fn display_label(&self) -> String {
        let gb = self.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        if self.model.is_empty() {
            alloc::format!("{} ({:.1} GB)", self.name, gb)
        } else {
            alloc::format!("{} — {} ({:.1} GB)", self.name, self.model, gb)
        }
    }
}

fn partition_type_name(pt: &crate::drivers::storage::PartitionType) -> String {
    use crate::drivers::storage::PartitionType;
    let s = match pt {
        PartitionType::Empty => "empty",
        PartitionType::Fat12 => "FAT12",
        PartitionType::Fat16 => "FAT16",
        PartitionType::Fat32 => "FAT32",
        PartitionType::ExtendedBoot => "extended",
        PartitionType::Ntfs => "NTFS",
        PartitionType::LinuxSwap => "swap",
        PartitionType::LinuxNative => "Linux",
        PartitionType::LinuxLvm => "LVM",
        PartitionType::Efi => "EFI",
        PartitionType::Unknown(b) => return alloc::format!("type 0x{:02x}", b),
    };
    String::from(s)
}

/// Enumerate block devices suitable for installation.
pub fn enumerate_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    for dev in get_storage_device_list() {
        if let Some(info) = disk_from_device(&dev) {
            disks.push(info);
        }
    }
    disks
}

fn disk_from_device(dev: &StorageDeviceInfo) -> Option<DiskInfo> {
    if dev.state != StorageDeviceState::Ready && dev.state != StorageDeviceState::Standby {
        return None;
    }
    let size_bytes = dev.capabilities.capacity_bytes;
    if size_bytes == 0 {
        return None;
    }

    let mut partitions = Vec::new();
    if let Ok(parts) = read_mbr_partitions(dev.id) {
        for p in parts {
            partitions.push(DiskPartitionInfo {
                number: p.partition_number,
                start_sector: p.start_sector,
                sector_count: p.sector_count,
                size_bytes: p.sector_count.saturating_mul(512),
                type_name: partition_type_name(&p.partition_type),
                bootable: p.bootable,
            });
        }
    }

    Some(DiskInfo {
        id: dev.id,
        name: dev.name.clone(),
        size_bytes,
        model: dev.model.clone(),
        ready: dev.state == StorageDeviceState::Ready,
        partitions,
    })
}

/// Pick the first disk with enough space (>= 8 GiB).
pub fn default_target_disk(disks: &[DiskInfo]) -> Option<u32> {
    const MIN_BYTES: u64 = 8 * 1024 * 1024 * 1024;
    disks
        .iter()
        .find(|d| d.ready && d.size_bytes >= MIN_BYTES)
        .map(|d| d.id)
        .or_else(|| disks.first().map(|d| d.id))
}
