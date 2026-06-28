//! Storage Device Detection and Initialization
//!
//! This module provides comprehensive storage device detection,
//! initialization, and management for the RustOS kernel.

use super::ahci::{AhciDriver, AHCI_DEVICE_IDS};
use super::ide::create_ide_drivers;
use super::nvme::NvmeDriver;
use super::pci_scan::{scan_pci_devices, PciDevice};
use super::{StorageDeviceType, StorageDriver, StorageDriverManager, StorageError};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

/// PCI class codes for storage controllers
const PCI_CLASS_STORAGE: u8 = 0x01;
const PCI_SUBCLASS_IDE: u8 = 0x01;
const PCI_SUBCLASS_SATA: u8 = 0x06;
const PCI_SUBCLASS_NVME: u8 = 0x08;

/// Storage device detection results
#[derive(Debug)]
pub struct DetectionResults {
    /// Number of AHCI controllers found
    pub ahci_controllers: usize,
    /// Number of NVMe controllers found
    pub nvme_controllers: usize,
    /// Number of IDE controllers found
    pub ide_controllers: usize,
    /// Total storage devices detected
    pub total_devices: usize,
    /// Detection errors encountered
    pub errors: Vec<String>,
}

/// Storage device detector
pub struct StorageDetector {
    manager: StorageDriverManager,
    detection_results: DetectionResults,
}

impl StorageDetector {
    /// Create new storage detector
    pub fn new() -> Self {
        Self {
            manager: StorageDriverManager::new(),
            detection_results: DetectionResults {
                ahci_controllers: 0,
                nvme_controllers: 0,
                ide_controllers: 0,
                total_devices: 0,
                errors: Vec::new(),
            },
        }
    }

    /// Detect and initialize all storage devices
    pub fn detect_and_initialize(&mut self) -> Result<DetectionResults, StorageError> {
        // Reset detection results
        self.detection_results = DetectionResults {
            ahci_controllers: 0,
            nvme_controllers: 0,
            ide_controllers: 0,
            total_devices: 0,
            errors: Vec::new(),
        };

        // Scan PCI bus for storage controllers
        self.scan_pci_storage_devices()?;

        // Detect legacy IDE controllers
        self.detect_ide_controllers()?;

        // Initialize all detected devices
        self.initialize_all_devices()?;

        Ok(self.detection_results.clone())
    }

    /// Scan PCI bus for storage controllers
    fn scan_pci_storage_devices(&mut self) -> Result<(), StorageError> {
        let pci_devices = scan_pci_devices();

        for device in pci_devices {
            // Check for VirtIO block devices (vendor 0x1AF4, device 0x1001)
            if device.vendor_id == 0x1AF4 && device.device_id == 0x1001 {
                if let Err(e) = self.detect_virtio_blk_device(&device) {
                    self.detection_results.errors.push(format!(
                        "VirtIO-blk detection failed for device {:04x}:{:04x}: {:?}",
                        device.vendor_id, device.device_id, e
                    ));
                }
                continue;
            }

            if device.class_code == PCI_CLASS_STORAGE {
                match device.subclass {
                    PCI_SUBCLASS_SATA => {
                        if let Err(e) = self.detect_ahci_controller(&device) {
                            self.detection_results.errors.push(format!(
                                "AHCI detection failed for device {:04x}:{:04x}: {:?}",
                                device.vendor_id, device.device_id, e
                            ));
                        }
                    }
                    PCI_SUBCLASS_NVME => {
                        if let Err(e) = self.detect_nvme_controller(&device) {
                            self.detection_results.errors.push(format!(
                                "NVMe detection failed for device {:04x}:{:04x}: {:?}",
                                device.vendor_id, device.device_id, e
                            ));
                        }
                    }
                    PCI_SUBCLASS_IDE => {
                        if let Err(e) = self.detect_pci_ide_controller(&device) {
                            self.detection_results.errors.push(format!(
                                "PCI IDE detection failed for device {:04x}:{:04x}: {:?}",
                                device.vendor_id, device.device_id, e
                            ));
                        }
                    }
                    _ => {
                        // Unknown storage subclass
                        self.detection_results.errors.push(format!(
                            "Unknown storage subclass 0x{:02x} for device {:04x}:{:04x}",
                            device.subclass, device.vendor_id, device.device_id
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Detect AHCI controller
    fn detect_ahci_controller(&mut self, device: &PciDevice) -> Result<(), StorageError> {
        // Check if this is a known AHCI device
        let device_info = AHCI_DEVICE_IDS
            .iter()
            .find(|info| info.vendor_id == device.vendor_id && info.device_id == device.device_id);

        if device_info.is_none() {
            return Err(StorageError::NotSupported);
        }

        // BAR5 is a 32-bit memory BAR; low 4 bits are type flags, not address.
        let base_addr = (device.bar5 & !0xF) as u64;
        if base_addr == 0 {
            return Err(StorageError::HardwareError);
        }

        // Map the HBA register space (ABAR) so init can touch MMIO without faulting.
        // 0x2000 covers generic regs + 32 port register sets (0x100 + 32*0x80 = 0x1100).
        crate::memory::map_mmio_region(base_addr as usize, 0x2000)
            .map_err(|_| StorageError::HardwareError)?;

        let mut driver = AhciDriver::new(
            format!("ahci-{:04x}:{:04x}", device.vendor_id, device.device_id),
            device.vendor_id,
            device.device_id,
            base_addr,
        );
        driver.init()?;

        let model = format!(
            "AHCI Controller {:04x}:{:04x}",
            device.vendor_id, device.device_id
        );
        let serial = format!("AHCI-{:04x}-{:04x}", device.vendor_id, device.device_id);
        self.manager.register_device(
            Box::new(driver),
            model,
            serial,
            "1.0".to_string(),
            get_current_time(),
        )?;

        self.detection_results.ahci_controllers += 1;
        self.detection_results.total_devices += 1;
        Ok(())
    }

    /// Detect NVMe controller
    fn detect_nvme_controller(&mut self, device: &PciDevice) -> Result<(), StorageError> {
        // NVMe BAR0 is a 64-bit memory BAR: low 4 bits are flags, high half is bar1.
        let base_addr = ((device.bar1 as u64) << 32) | ((device.bar0 & !0xF) as u64);
        if base_addr == 0 {
            return Err(StorageError::HardwareError);
        }

        // Map controller registers + admin doorbell (doorbells start at 0x1000).
        // ponytail: 0x2000 covers admin + first I/O queue; widen if many queues are used.
        crate::memory::map_mmio_region(base_addr as usize, 0x2000)
            .map_err(|_| StorageError::HardwareError)?;

        let mut driver = NvmeDriver::new(
            format!("nvme-{:04x}:{:04x}", device.vendor_id, device.device_id),
            base_addr,
        );
        driver.init()?;

        let model = format!(
            "NVMe Controller {:04x}:{:04x}",
            device.vendor_id, device.device_id
        );
        let serial = format!("NVME-{:04x}-{:04x}", device.vendor_id, device.device_id);
        self.manager.register_device(
            Box::new(driver),
            model,
            serial,
            "1.0".to_string(),
            get_current_time(),
        )?;

        self.detection_results.nvme_controllers += 1;
        self.detection_results.total_devices += 1;
        Ok(())
    }

    /// Detect PCI IDE controller
    fn detect_pci_ide_controller(&mut self, device: &PciDevice) -> Result<(), StorageError> {
        // PCI IDE controllers can have up to 4 drives (2 channels, 2 drives each)
        let drivers = create_ide_drivers();
        let mut devices_found = 0;

        for mut driver in drivers {
            if let Ok(()) = driver.init() {
                // Get device information
                let model = if let Some(model) = driver.get_model() {
                    model
                } else {
                    format!(
                        "IDE Device on PCI {:04x}:{:04x}",
                        device.vendor_id, device.device_id
                    )
                };

                let serial = if let Some(serial) = driver.get_serial() {
                    serial
                } else {
                    format!(
                        "IDE-{:04x}-{:04x}-{}",
                        device.vendor_id, device.device_id, devices_found
                    )
                };

                let firmware = "1.0".to_string();

                // Register the device
                let _device_id = self.manager.register_device(
                    driver,
                    model,
                    serial,
                    firmware,
                    get_current_time(),
                )?;

                devices_found += 1;
            }
        }

        if devices_found > 0 {
            self.detection_results.ide_controllers += 1;
            self.detection_results.total_devices += devices_found;
        }

        Ok(())
    }

    /// Detect VirtIO block device
    fn detect_virtio_blk_device(&mut self, device: &PciDevice) -> Result<(), StorageError> {
        // VirtIO-blk is already initialized by the virtio subsystem during boot.
        // Here we just register it as a storage device if it's available.
        if !crate::drivers::virtio::blk::is_available() {
            return Ok(());
        }

        let capacity_sectors = crate::drivers::virtio::blk::capacity_sectors().unwrap_or(0);

        let driver: Box<dyn StorageDriver> =
            Box::new(VirtioBlkStorageAdapter::new(capacity_sectors));

        let model = format!("VirtIO Block Disk");
        let serial = format!(
            "virtio-blk-{:04x}-{:04x}",
            device.vendor_id, device.device_id
        );
        let firmware = "1.0".to_string();

        let _device_id =
            self.manager
                .register_device(driver, model, serial, firmware, get_current_time())?;

        self.detection_results.total_devices += 1;
        crate::serial_println!(
            "virtio-blk: registered as storage device ({} sectors)",
            capacity_sectors
        );

        Ok(())
    }

    /// Detect legacy IDE controllers (ISA)
    fn detect_ide_controllers(&mut self) -> Result<(), StorageError> {
        let drivers = create_ide_drivers();
        let mut devices_found = 0;

        for mut driver in drivers {
            if let Ok(()) = driver.init() {
                // Get device information
                let model = if let Some(model) = driver.get_model() {
                    model
                } else {
                    format!("Legacy IDE Device {}", devices_found)
                };

                let serial = if let Some(serial) = driver.get_serial() {
                    serial
                } else {
                    format!("LEGACY-IDE-{}", devices_found)
                };

                let firmware = "1.0".to_string();

                // Register the device
                let _device_id = self.manager.register_device(
                    driver,
                    model,
                    serial,
                    firmware,
                    get_current_time(),
                )?;

                devices_found += 1;
            }
        }

        if devices_found > 0 {
            self.detection_results.ide_controllers += 1;
            self.detection_results.total_devices += devices_found;
        }

        Ok(())
    }

    /// Initialize all detected devices
    fn initialize_all_devices(&mut self) -> Result<(), StorageError> {
        self.manager.init_all_devices()?;
        Ok(())
    }

    /// Get the storage manager
    pub fn get_manager(self) -> StorageDriverManager {
        self.manager
    }

    /// Get detection results
    pub fn get_results(&self) -> &DetectionResults {
        &self.detection_results
    }
}

/// Global storage detection and initialization
pub fn detect_and_initialize_storage() -> Result<DetectionResults, StorageError> {
    let mut detector = StorageDetector::new();
    let results = detector.detect_and_initialize()?;

    // Set the global storage manager
    super::init_storage_manager();
    if let Some(manager) = super::STORAGE_MANAGER.write().as_mut() {
        *manager = detector.get_manager();
    }

    Ok(results)
}

/// Get current time in milliseconds
fn get_current_time() -> u64 {
    // Use system time for storage detection timestamps
    crate::time::get_system_time_ms()
}

impl Clone for DetectionResults {
    fn clone(&self) -> Self {
        Self {
            ahci_controllers: self.ahci_controllers,
            nvme_controllers: self.nvme_controllers,
            ide_controllers: self.ide_controllers,
            total_devices: self.total_devices,
            errors: self.errors.clone(),
        }
    }
}

// Additional methods for IDE driver are already implemented in ide.rs

/// Adapter that wraps the virtio-blk driver to implement the StorageDriver trait
#[derive(Debug)]
struct VirtioBlkStorageAdapter {
    capacity_sectors: u64,
    reads_total: core::sync::atomic::AtomicU64,
    writes_total: core::sync::atomic::AtomicU64,
    bytes_read: core::sync::atomic::AtomicU64,
    bytes_written: core::sync::atomic::AtomicU64,
    read_errors: core::sync::atomic::AtomicU64,
    write_errors: core::sync::atomic::AtomicU64,
}

impl VirtioBlkStorageAdapter {
    fn new(capacity_sectors: u64) -> Self {
        Self {
            capacity_sectors,
            reads_total: core::sync::atomic::AtomicU64::new(0),
            writes_total: core::sync::atomic::AtomicU64::new(0),
            bytes_read: core::sync::atomic::AtomicU64::new(0),
            bytes_written: core::sync::atomic::AtomicU64::new(0),
            read_errors: core::sync::atomic::AtomicU64::new(0),
            write_errors: core::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl StorageDriver for VirtioBlkStorageAdapter {
    fn name(&self) -> &str {
        "VirtIO Block"
    }

    fn device_type(&self) -> StorageDeviceType {
        StorageDeviceType::Unknown
    }

    fn state(&self) -> super::StorageDeviceState {
        if crate::drivers::virtio::blk::is_available() {
            super::StorageDeviceState::Ready
        } else {
            super::StorageDeviceState::Offline
        }
    }

    fn capabilities(&self) -> super::StorageCapabilities {
        super::StorageCapabilities {
            capacity_bytes: self.capacity_sectors * 512,
            sector_size: 512,
            max_transfer_size: 128 * 1024,
            max_queue_depth: 32,
            supports_ncq: false,
            read_speed_mbps: 200,
            write_speed_mbps: 200,
            supports_smart: false,
            supports_trim: false,
            is_removable: false,
        }
    }

    fn init(&mut self) -> Result<(), StorageError> {
        // VirtIO block device is already initialized by the virtio subsystem
        // during PCI enumeration. Nothing additional to do here.
        Ok(())
    }

    fn read_sectors(
        &mut self,
        start_sector: u64,
        buffer: &mut [u8],
    ) -> Result<usize, StorageError> {
        match crate::drivers::virtio::blk::read_sectors(start_sector, buffer) {
            Ok(n) => {
                self.reads_total
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                self.bytes_read
                    .fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
                Ok(n)
            }
            Err(_) => {
                self.read_errors
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                Err(StorageError::HardwareError)
            }
        }
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        match crate::drivers::virtio::blk::write_sectors(start_sector, buffer) {
            Ok(n) => {
                self.writes_total
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                self.bytes_written
                    .fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
                Ok(n)
            }
            Err(_) => {
                self.write_errors
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                Err(StorageError::HardwareError)
            }
        }
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        crate::drivers::virtio::blk::flush().map_err(|_| StorageError::HardwareError)
    }

    fn get_stats(&self) -> super::StorageStats {
        super::StorageStats {
            reads_total: self.reads_total.load(core::sync::atomic::Ordering::Relaxed),
            writes_total: self
                .writes_total
                .load(core::sync::atomic::Ordering::Relaxed),
            bytes_read: self.bytes_read.load(core::sync::atomic::Ordering::Relaxed),
            bytes_written: self
                .bytes_written
                .load(core::sync::atomic::Ordering::Relaxed),
            read_errors: self.read_errors.load(core::sync::atomic::Ordering::Relaxed),
            write_errors: self
                .write_errors
                .load(core::sync::atomic::Ordering::Relaxed),
            avg_read_latency_us: 0,
            avg_write_latency_us: 0,
            uptime_seconds: crate::time::system_time(),
        }
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        // VirtIO block reset requires re-initializing the virtqueue.
        // This is not supported at runtime without a full device re-init.
        Err(StorageError::NotSupported)
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        // VirtIO block does not support power management commands.
        Err(StorageError::NotSupported)
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        // VirtIO block does not support power management commands.
        Err(StorageError::NotSupported)
    }

    fn vendor_command(&mut self, _command: u8, _data: &[u8]) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::NotSupported)
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::NotSupported)
    }

    fn get_model(&self) -> Option<String> {
        Some("VirtIO Block Disk".to_string())
    }
}
