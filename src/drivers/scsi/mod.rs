//! SCSI mid-layer — host/device registry and command dispatch
//!
//! Connects AHCI and NVMe storage drivers as SCSI hosts and exposes a unified
//! request path for upper layers (USB MSC, md, block).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use super::storage::{with_storage_manager, StorageDeviceState, StorageError};

// ── SCSI constants ──────────────────────────────────────────────────────

pub const SCSI_OPCODE_INQUIRY: u8 = 0x12;
pub const SCSI_OPCODE_READ_CAPACITY10: u8 = 0x25;
pub const SCSI_OPCODE_SYNCHRONIZE_CACHE10: u8 = 0x35;
pub const SCSI_OPCODE_READ10: u8 = 0x28;
pub const SCSI_OPCODE_WRITE10: u8 = 0x2A;
pub const SCSI_OPCODE_TEST_UNIT_READY: u8 = 0x00;
pub const SCSI_OPCODE_REQUEST_SENSE: u8 = 0x03;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScsiHostType {
    Ahci,
    Nvme,
    Usb,
    Virtual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScsiDeviceType {
    Disk,
    Optical,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ScsiHost {
    pub id: u32,
    pub host_type: ScsiHostType,
    pub name: String,
    pub storage_device_id: u32,
}

#[derive(Debug, Clone)]
pub struct ScsiDevice {
    pub id: u32,
    pub host_id: u32,
    pub lun: u8,
    pub device_type: ScsiDeviceType,
    pub vendor: String,
    pub model: String,
    pub block_size: u32,
    pub sector_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ScsiCommand {
    pub opcode: u8,
    pub lba: u64,
    pub transfer_blocks: u32,
    pub data_dir_in: bool,
}

fn validate_rw(dev: &ScsiDevice, cmd: ScsiCommand, buffer: &[u8]) -> Result<usize, StorageError> {
    if cmd.transfer_blocks == 0 {
        return Ok(0);
    }
    if dev.block_size == 0 {
        return Err(StorageError::HardwareError);
    }
    let bytes = (cmd.transfer_blocks as u64)
        .checked_mul(dev.block_size as u64)
        .ok_or(StorageError::TransferTooLarge)?;
    if bytes > usize::MAX as u64 || buffer.len() < bytes as usize {
        return Err(StorageError::BufferTooSmall);
    }
    let end = cmd
        .lba
        .checked_add(cmd.transfer_blocks as u64)
        .ok_or(StorageError::InvalidSector)?;
    if end > dev.sector_count {
        return Err(StorageError::InvalidSector);
    }
    Ok(bytes as usize)
}

static NEXT_HOST_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(1);

static SCSI_HOSTS: RwLock<BTreeMap<u32, ScsiHost>> = RwLock::new(BTreeMap::new());
static SCSI_DEVICES: RwLock<BTreeMap<u32, ScsiDevice>> = RwLock::new(BTreeMap::new());

fn host_type_for_storage(device_type: super::storage::StorageDeviceType) -> ScsiHostType {
    use super::storage::StorageDeviceType;
    match device_type {
        StorageDeviceType::SataHdd | StorageDeviceType::SataSsd => ScsiHostType::Ahci,
        StorageDeviceType::NvmeSsd => ScsiHostType::Nvme,
        StorageDeviceType::UsbMassStorage => ScsiHostType::Usb,
        _ => ScsiHostType::Virtual,
    }
}

/// Register a storage-backed SCSI host and LUN 0 device.
pub fn register_storage_host(
    storage_device_id: u32,
    host_type: ScsiHostType,
    name: String,
) -> Result<(u32, u32), StorageError> {
    let (model, block_size, sector_count, device_type) = with_storage_manager(|mgr| {
        let dev = mgr
            .get_device(storage_device_id)
            .ok_or(StorageError::DeviceNotFound)?;
        let caps = dev.driver.capabilities();
        Ok((
            dev.model.clone(),
            caps.sector_size,
            caps.capacity_bytes / caps.sector_size as u64,
            dev.driver.device_type(),
        ))
    })
    .ok_or(StorageError::DeviceNotFound)??;

    let host_id = NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst);
    let dev_id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);

    SCSI_HOSTS.write().insert(
        host_id,
        ScsiHost {
            id: host_id,
            host_type,
            name,
            storage_device_id,
        },
    );

    SCSI_DEVICES.write().insert(
        dev_id,
        ScsiDevice {
            id: dev_id,
            host_id,
            lun: 0,
            device_type: ScsiDeviceType::Disk,
            vendor: String::from("RustOS"),
            model,
            block_size,
            sector_count,
        },
    );

    let _ = device_type;
    Ok((host_id, dev_id))
}

/// Scan registered storage devices and attach them as SCSI hosts.
pub fn scan_hosts() -> ScsiScanResult {
    let mut result = ScsiScanResult::default();
    let devices = super::storage::get_storage_device_list();

    for info in devices {
        if info.state != StorageDeviceState::Ready {
            continue;
        }

        let host_type = host_type_for_storage(info.device_type);
        let name = format!("scsi-host-{}", info.id);
        match register_storage_host(info.id, host_type, name) {
            Ok((host_id, dev_id)) => {
                crate::serial_println!(
                    "scsi: host {} device {} -> storage {}",
                    host_id,
                    dev_id,
                    info.id
                );
                result.hosts_registered += 1;
                result.devices_registered += 1;
            }
            Err(e) => {
                result.errors.push(format!("storage {}: {:?}", info.id, e));
            }
        }
    }

    result
}

/// Dispatch a SCSI command to the backing storage device.
pub fn queue_command(
    device_id: u32,
    cmd: ScsiCommand,
    buffer: &mut [u8],
) -> Result<usize, StorageError> {
    let devices = SCSI_DEVICES.read();
    let dev = devices
        .get(&device_id)
        .ok_or(StorageError::DeviceNotFound)?;
    let hosts = SCSI_HOSTS.read();
    let host = hosts
        .get(&dev.host_id)
        .ok_or(StorageError::DeviceNotFound)?;
    let storage_id = host.storage_device_id;

    match cmd.opcode {
        SCSI_OPCODE_TEST_UNIT_READY => Ok(0),
        SCSI_OPCODE_REQUEST_SENSE => {
            if !cmd.data_dir_in || buffer.len() < 18 {
                return Err(StorageError::BufferTooSmall);
            }
            buffer[..18].fill(0);
            buffer[0] = 0x70; // current fixed sense data
            buffer[7] = 10; // additional sense length
            Ok(18)
        }
        SCSI_OPCODE_INQUIRY => {
            if !cmd.data_dir_in || buffer.len() < 36 {
                return Err(StorageError::BufferTooSmall);
            }
            buffer[..36].fill(0);
            buffer[0] = match dev.device_type {
                ScsiDeviceType::Disk => 0x00,
                ScsiDeviceType::Optical => 0x05,
                ScsiDeviceType::Unknown => 0x1f,
            };
            buffer[2] = 0x06; // SPC-4 compatible response
            buffer[3] = 0x02;
            buffer[4] = 31;
            let vendor = dev.vendor.as_bytes();
            let model = dev.model.as_bytes();
            let vlen = core::cmp::min(vendor.len(), 8);
            let mlen = core::cmp::min(model.len(), 16);
            buffer[8..16].fill(b' ');
            buffer[16..32].fill(b' ');
            buffer[8..8 + vlen].copy_from_slice(&vendor[..vlen]);
            buffer[16..16 + mlen].copy_from_slice(&model[..mlen]);
            Ok(36)
        }
        SCSI_OPCODE_READ_CAPACITY10 => {
            if !cmd.data_dir_in || buffer.len() < 8 {
                return Err(StorageError::BufferTooSmall);
            }
            let last_lba = if dev.sector_count.saturating_sub(1) > u32::MAX as u64 {
                u32::MAX
            } else {
                dev.sector_count.saturating_sub(1) as u32
            };
            buffer[0..4].copy_from_slice(&last_lba.to_be_bytes());
            buffer[4..8].copy_from_slice(&dev.block_size.to_be_bytes());
            Ok(8)
        }
        SCSI_OPCODE_SYNCHRONIZE_CACHE10 => {
            with_storage_manager(|mgr| {
                let dev = mgr
                    .get_device_mut(storage_id)
                    .ok_or(StorageError::DeviceNotFound)?;
                dev.driver.flush()
            })
            .ok_or(StorageError::DeviceNotFound)??;
            Ok(0)
        }
        SCSI_OPCODE_READ10 => {
            if cmd.data_dir_in {
                let bytes = validate_rw(dev, cmd, buffer)?;
                with_storage_manager(|mgr| {
                    mgr.read_sectors(storage_id, cmd.lba, &mut buffer[..bytes])
                })
                .ok_or(StorageError::DeviceNotFound)?
            } else {
                Err(StorageError::HardwareError)
            }
        }
        SCSI_OPCODE_WRITE10 => {
            if cmd.data_dir_in {
                return Err(StorageError::HardwareError);
            }
            let bytes = validate_rw(dev, cmd, buffer)?;
            with_storage_manager(|mgr| mgr.write_sectors(storage_id, cmd.lba, &buffer[..bytes]))
                .ok_or(StorageError::DeviceNotFound)?
        }
        _ => Err(StorageError::NotSupported),
    }
}

pub fn init() -> ScsiScanResult {
    let result = scan_hosts();
    result
}

/// Register a representative SCSI device into the unified `base` device model
/// (additive; tolerant of a missing bus and never fatal to SCSI init).
fn publish_to_base() {
    use crate::drivers::base;
    if base::device_exists("scsi-disk0") {
        return;
    }
    if let Ok(id) = base::register_device_simple("scsi", "scsi-disk0", "scsi,disk") {
        let _ = base::set_property(id, "vendor", "RustOS");
        let _ = base::set_property(id, "type", "disk");
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScsiScanResult {
    pub hosts_registered: usize,
    pub devices_registered: usize,
    pub errors: Vec<String>,
}

pub fn host_count() -> usize {
    SCSI_HOSTS.read().len()
}

pub fn device_count() -> usize {
    SCSI_DEVICES.read().len()
}

pub fn list_devices() -> Vec<ScsiDevice> {
    SCSI_DEVICES.read().values().cloned().collect()
}
