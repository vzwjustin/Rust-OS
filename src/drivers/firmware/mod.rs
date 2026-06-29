//! Firmware loading subsystem
//!
//! Provides firmware request/load lifecycle for device drivers, with
//! built-in firmware storage for embedded firmware blobs. Mirrors Linux's
//! `drivers/base/firmware_class.c` (firmware_class / firmware_loader).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Firmware loading status (Linux `enum fw_load_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareStatus {
    NotRequested,
    Loading,
    Loaded,
    NotFound,
    Error,
}

/// A loaded firmware blob (Linux `struct firmware`).
pub struct Firmware {
    pub id: u32,
    pub name: String,
    pub data: Vec<u8>,
    pub status: FirmwareStatus,
    pub size: usize,
}

/// Built-in firmware entry (Linux `CONFIG_FW_LOADER_BUILTIN`).
struct BuiltinFirmware {
    name: String,
    data: &'static [u8],
}

// ── Registry ────────────────────────────────────────────────────────────

static FIRMWARE_CACHE: RwLock<BTreeMap<u32, Firmware>> = RwLock::new(BTreeMap::new());
static BUILTIN_FIRMWARE: RwLock<Vec<BuiltinFirmware>> = RwLock::new(Vec::new());
static NEXT_FW_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register built-in firmware (Linux `DECLARE_FW_LOADER_BUILTIN`).
pub fn register_builtin(name: &str, data: &'static [u8]) {
    BUILTIN_FIRMWARE.write().push(BuiltinFirmware {
        name: String::from(name),
        data,
    });
}

/// Request firmware by name (Linux `request_firmware`).
/// Returns a firmware ID that can be used with `get_data`.
pub fn request_firmware(name: &str) -> Result<u32, &'static str> {
    let id = NEXT_FW_ID.fetch_add(1, Ordering::SeqCst);

    // Mark as loading.
    FIRMWARE_CACHE.write().insert(
        id,
        Firmware {
            id,
            name: String::from(name),
            data: Vec::new(),
            status: FirmwareStatus::Loading,
            size: 0,
        },
    );

    // Search built-in firmware.
    let builtin_data = {
        let builtins = BUILTIN_FIRMWARE.read();
        builtins.iter().find(|fw| fw.name == name).map(|fw| fw.data)
    };

    if let Some(data) = builtin_data {
        let mut cache = FIRMWARE_CACHE.write();
        let fw = cache.get_mut(&id).ok_or("Firmware entry vanished")?;
        fw.data = data.to_vec();
        fw.size = data.len();
        fw.status = FirmwareStatus::Loaded;
        return Ok(id);
    }

    // Search initramfs filesystem for firmware files.
    if let Ok(data) = load_from_initramfs(name) {
        let mut cache = FIRMWARE_CACHE.write();
        let fw = cache.get_mut(&id).ok_or("Firmware entry vanished")?;
        fw.data = data;
        fw.size = fw.data.len();
        fw.status = FirmwareStatus::Loaded;
        return Ok(id);
    }

    // Not found.
    let mut cache = FIRMWARE_CACHE.write();
    let fw = cache.get_mut(&id).ok_or("Firmware entry vanished")?;
    fw.status = FirmwareStatus::NotFound;
    Err("Firmware not found")
}

/// Request firmware without blocking (Linux `request_firmware_nowait`).
/// For now this is synchronous since we have no async workqueue integration.
pub fn request_firmware_nowait(name: &str) -> Result<u32, &'static str> {
    request_firmware(name)
}

/// Get firmware data (Linux `firmware->data`).
pub fn get_data(fw_id: u32) -> Result<Vec<u8>, &'static str> {
    let cache = FIRMWARE_CACHE.read();
    let fw = cache.get(&fw_id).ok_or("Firmware handle not found")?;
    if fw.status != FirmwareStatus::Loaded {
        return Err("Firmware not loaded");
    }
    Ok(fw.data.clone())
}

/// Get firmware size (Linux `firmware->size`).
pub fn get_size(fw_id: u32) -> Result<usize, &'static str> {
    let cache = FIRMWARE_CACHE.read();
    let fw = cache.get(&fw_id).ok_or("Firmware handle not found")?;
    Ok(fw.size)
}

/// Get firmware status.
pub fn get_status(fw_id: u32) -> Result<FirmwareStatus, &'static str> {
    let cache = FIRMWARE_CACHE.read();
    let fw = cache.get(&fw_id).ok_or("Firmware handle not found")?;
    Ok(fw.status)
}

/// Release firmware (Linux `release_firmware`).
pub fn release_firmware(fw_id: u32) -> Result<(), &'static str> {
    FIRMWARE_CACHE
        .write()
        .remove(&fw_id)
        .ok_or("Firmware handle not found")?;
    Ok(())
}

/// Load firmware from initramfs path `/lib/firmware/<name>`.
fn load_from_initramfs(name: &str) -> Result<Vec<u8>, &'static str> {
    let path = alloc::format!("/lib/firmware/{}", name);
    // Open the file read-only (O_RDONLY = 0).
    let fd = crate::vfs::vfs_open(&path, 0, 0).map_err(|_| "Firmware file not found in VFS")?;
    // Read in chunks.
    let mut data = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = crate::vfs::vfs_read(fd, &mut buf).map_err(|_| "Firmware read error")?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let _ = crate::vfs::vfs_close(fd);
    Ok(data)
}

/// Number of currently loaded firmware blobs.
pub fn loaded_count() -> usize {
    FIRMWARE_CACHE
        .read()
        .values()
        .filter(|fw| fw.status == FirmwareStatus::Loaded)
        .count()
}

/// Number of registered built-in firmware entries.
pub fn builtin_count() -> usize {
    BUILTIN_FIRMWARE.read().len()
}

/// Initialize firmware loader subsystem.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("firmware: loader ready ({} built-in)", builtin_count());
    Ok(())
}
