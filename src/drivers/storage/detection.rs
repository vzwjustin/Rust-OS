//! Storage Device Detection and Initialization
//!
//! This module provides comprehensive storage device detection,
//! initialization, and management for the RustOS kernel.

use super::ahci::{AhciDriver, AHCI_DEVICE_IDS};
use super::ide::{create_ide_drivers, IdeDriver};
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

        // Get BAR5 (AHCI base address)
        let base_addr = device.bar5 as u64;
        if base_addr == 0 {
            return Err(StorageError::HardwareError);
        }

        // BAR5 MMIO is not mapped by the bootloader. Skip AHCI init to avoid page faults.
        Err(StorageError::NotSupported)
    }

    /// Detect NVMe controller
    fn detect_nvme_controller(&mut self, device: &PciDevice) -> Result<(), StorageError> {
        // BAR0 MMIO is not mapped by the bootloader. Skip NVMe init to avoid page faults.
        let _ = device;
        Err(StorageError::NotSupported)
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
                let device_id = self.manager.register_device(
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
                let device_id = self.manager.register_device(
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
