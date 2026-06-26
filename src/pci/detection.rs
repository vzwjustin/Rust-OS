//! PCI Hardware Detection and Management
//!
//! This module provides automatic hardware discovery, device categorization,
//! resource conflict detection, and hot-plug support preparation.

use crate::pci::config::PciConfigManager;
use crate::pci::{get_pci_scanner, PciBusScanner, PciClass, PciDevice};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

/// Hardware detection results
#[derive(Debug, Clone)]
pub struct HardwareDetectionResults {
    pub total_devices: usize,
    pub devices_by_class: BTreeMap<PciClass, Vec<PciDevice>>,
    pub resource_conflicts: Vec<ResourceConflict>,
    pub hot_plug_capable: Vec<PciDevice>,
    pub power_managed: Vec<PciDevice>,
    pub msi_capable: Vec<PciDevice>,
    pub msi_x_capable: Vec<PciDevice>,
    pub critical_devices: Vec<PciDevice>,
}

impl Default for HardwareDetectionResults {
    fn default() -> Self {
        Self {
            total_devices: 0,
            devices_by_class: BTreeMap::new(),
            resource_conflicts: Vec::new(),
            hot_plug_capable: Vec::new(),
            power_managed: Vec::new(),
            msi_capable: Vec::new(),
            msi_x_capable: Vec::new(),
            critical_devices: Vec::new(),
        }
    }
}

/// Resource conflict information
#[derive(Debug, Clone)]
pub struct ResourceConflict {
    pub device1: PciDevice,
    pub device2: PciDevice,
    pub conflict_type: ConflictType,
    pub address_range: (u64, u64),
    pub severity: ConflictSeverity,
}

/// Type of resource conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    MemoryOverlap,
    IoOverlap,
    InterruptConflict,
}

impl fmt::Display for ConflictType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConflictType::MemoryOverlap => write!(f, "Memory Overlap"),
            ConflictType::IoOverlap => write!(f, "I/O Overlap"),
            ConflictType::InterruptConflict => write!(f, "Interrupt Conflict"),
        }
    }
}

/// Severity of resource conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConflictSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for ConflictSeverity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConflictSeverity::Low => write!(f, "Low"),
            ConflictSeverity::Medium => write!(f, "Medium"),
            ConflictSeverity::High => write!(f, "High"),
            ConflictSeverity::Critical => write!(f, "Critical"),
        }
    }
}

/// Device category for driver matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCategory {
    NetworkCard,
    StorageController,
    GraphicsCard,
    AudioDevice,
    UsbController,
    BridgeDevice,
    InputDevice,
    WirelessCard,
    SerialBusController,
    SystemDevice,
    UnknownDevice,
}

impl DeviceCategory {
    /// Get device category from PCI class and subclass
    pub fn from_pci_class(class: PciClass, subclass: u8) -> Self {
        match class {
            PciClass::Network => Self::NetworkCard,
            PciClass::MassStorage => Self::StorageController,
            PciClass::Display => Self::GraphicsCard,
            PciClass::Multimedia => Self::AudioDevice,
            PciClass::Bridge => Self::BridgeDevice,
            PciClass::InputDevice => Self::InputDevice,
            PciClass::Wireless => Self::WirelessCard,
            PciClass::SerialBus => {
                match subclass {
                    0x03 => Self::UsbController, // USB controller
                    _ => Self::SerialBusController,
                }
            }
            PciClass::SystemPeripheral => Self::SystemDevice,
            _ => Self::UnknownDevice,
        }
    }
}

impl fmt::Display for DeviceCategory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeviceCategory::NetworkCard => write!(f, "Network Card"),
            DeviceCategory::StorageController => write!(f, "Storage Controller"),
            DeviceCategory::GraphicsCard => write!(f, "Graphics Card"),
            DeviceCategory::AudioDevice => write!(f, "Audio Device"),
            DeviceCategory::UsbController => write!(f, "USB Controller"),
            DeviceCategory::BridgeDevice => write!(f, "Bridge Device"),
            DeviceCategory::InputDevice => write!(f, "Input Device"),
            DeviceCategory::WirelessCard => write!(f, "Wireless Card"),
            DeviceCategory::SerialBusController => write!(f, "Serial Bus Controller"),
            DeviceCategory::SystemDevice => write!(f, "System Device"),
            DeviceCategory::UnknownDevice => write!(f, "Unknown Device"),
        }
    }
}

/// Hardware detector for automatic device discovery
pub struct HardwareDetector;

impl HardwareDetector {
    /// Create a new hardware detector
    pub fn new() -> Self {
        Self
    }

    /// Perform comprehensive hardware detection
    pub fn detect_hardware(&self) -> Result<HardwareDetectionResults, &'static str> {
        let scanner = get_pci_scanner().lock();

        if !scanner.is_initialized() {
            return Err("PCI scanner not initialized");
        }

        let devices = scanner.get_devices();
        let mut results = HardwareDetectionResults::default();
        results.total_devices = devices.len();

        // Categorize devices by class
        for device in devices {
            let class = device.class_code;
            results
                .devices_by_class
                .entry(class)
                .or_insert_with(Vec::new)
                .push(device.clone());

            // Identify devices with special capabilities
            if device.capabilities.hot_plug {
                results.hot_plug_capable.push(device.clone());
            }

            if device.capabilities.power_management {
                results.power_managed.push(device.clone());
            }

            if device.capabilities.msi {
                results.msi_capable.push(device.clone());
            }

            if device.capabilities.msi_x {
                results.msi_x_capable.push(device.clone());
            }

            // Identify critical system devices
            if self.is_critical_device(device) {
                results.critical_devices.push(device.clone());
            }
        }

        // Detect resource conflicts
        results.resource_conflicts = self.detect_resource_conflicts(&scanner)?;

        Ok(results)
    }

    /// Detect resource conflicts between devices
    fn detect_resource_conflicts(
        &self,
        scanner: &PciBusScanner,
    ) -> Result<Vec<ResourceConflict>, &'static str> {
        let mut conflicts = Vec::new();
        let devices = scanner.get_devices();
        let config_manager = PciConfigManager::new(scanner);

        // Build resource map
        let mut memory_resources: Vec<(PciDevice, u64, u64)> = Vec::new();
        let mut io_resources: Vec<(PciDevice, u64, u64)> = Vec::new();
        let mut interrupt_map: BTreeMap<u8, Vec<PciDevice>> = BTreeMap::new();

        for device in devices {
            // Check BARs for overlaps
            let bars = config_manager.read_bars(device);
            for bar in &bars {
                if bar.is_active() && bar.size > 0 {
                    let end_addr = bar.base_address + bar.size - 1;

                    if bar.is_memory() {
                        memory_resources.push((device.clone(), bar.base_address, end_addr));
                    } else if bar.is_io() {
                        io_resources.push((device.clone(), bar.base_address, end_addr));
                    }
                }
            }

            // Track interrupt usage
            let irq_line = config_manager.get_interrupt_line(device);
            if irq_line != 0 && irq_line != 0xFF {
                interrupt_map
                    .entry(irq_line)
                    .or_insert_with(Vec::new)
                    .push(device.clone());
            }
        }

        // Check for memory overlaps
        for i in 0..memory_resources.len() {
            for j in (i + 1)..memory_resources.len() {
                let (ref dev1, start1, end1) = memory_resources[i];
                let (ref dev2, start2, end2) = memory_resources[j];

                if self.ranges_overlap(start1, end1, start2, end2) {
                    let severity = self.determine_conflict_severity(dev1, dev2);
                    conflicts.push(ResourceConflict {
                        device1: dev1.clone(),
                        device2: dev2.clone(),
                        conflict_type: ConflictType::MemoryOverlap,
                        address_range: (start1.max(start2), end1.min(end2)),
                        severity,
                    });
                }
            }
        }

        // Check for I/O overlaps
        for i in 0..io_resources.len() {
            for j in (i + 1)..io_resources.len() {
                let (ref dev1, start1, end1) = io_resources[i];
                let (ref dev2, start2, end2) = io_resources[j];

                if self.ranges_overlap(start1, end1, start2, end2) {
                    let severity = self.determine_conflict_severity(dev1, dev2);
                    conflicts.push(ResourceConflict {
                        device1: dev1.clone(),
                        device2: dev2.clone(),
                        conflict_type: ConflictType::IoOverlap,
                        address_range: (start1.max(start2), end1.min(end2)),
                        severity,
                    });
                }
            }
        }

        // Check for interrupt conflicts
        for (irq, devices_list) in &interrupt_map {
            if devices_list.len() > 1 {
                // Multiple devices sharing the same IRQ line
                for i in 0..devices_list.len() {
                    for j in (i + 1)..devices_list.len() {
                        let dev1 = &devices_list[i];
                        let dev2 = &devices_list[j];

                        // Sharing interrupts is sometimes OK (e.g., PCI interrupt sharing)
                        // but can be problematic for certain device types
                        let severity = if self.is_interrupt_sharing_problematic(dev1, dev2) {
                            ConflictSeverity::High
                        } else {
                            ConflictSeverity::Low
                        };

                        conflicts.push(ResourceConflict {
                            device1: dev1.clone(),
                            device2: dev2.clone(),
                            conflict_type: ConflictType::InterruptConflict,
                            address_range: (*irq as u64, *irq as u64),
                            severity,
                        });
                    }
                }
            }
        }

        Ok(conflicts)
    }

    /// Check if two address ranges overlap
    fn ranges_overlap(&self, start1: u64, end1: u64, start2: u64, end2: u64) -> bool {
        start1 <= end2 && start2 <= end1
    }

    /// Determine conflict severity based on device types
    fn determine_conflict_severity(&self, dev1: &PciDevice, dev2: &PciDevice) -> ConflictSeverity {
        // Critical if either device is a system device
        if self.is_critical_device(dev1) || self.is_critical_device(dev2) {
            return ConflictSeverity::Critical;
        }

        // High severity for storage or network conflicts
        if matches!(dev1.class_code, PciClass::MassStorage | PciClass::Network)
            || matches!(dev2.class_code, PciClass::MassStorage | PciClass::Network)
        {
            return ConflictSeverity::High;
        }

        // Medium severity for multimedia or graphics
        if matches!(dev1.class_code, PciClass::Multimedia | PciClass::Display)
            || matches!(dev2.class_code, PciClass::Multimedia | PciClass::Display)
        {
            return ConflictSeverity::Medium;
        }

        ConflictSeverity::Low
    }

    /// Check if interrupt sharing is problematic for specific device types
    fn is_interrupt_sharing_problematic(&self, dev1: &PciDevice, dev2: &PciDevice) -> bool {
        // Real-time or high-performance devices shouldn't share interrupts
        matches!(dev1.class_code, PciClass::MassStorage | PciClass::Network)
            && matches!(dev2.class_code, PciClass::MassStorage | PciClass::Network)
    }

    /// Determine if a device is critical for system operation
    fn is_critical_device(&self, device: &PciDevice) -> bool {
        match device.class_code {
            PciClass::Bridge => true,           // Bridge devices are critical
            PciClass::SystemPeripheral => true, // System devices
            PciClass::MassStorage => {
                // Storage controllers are critical if they're likely the boot device
                match device.subclass {
                    0x01 | 0x06 => true, // IDE or SATA controllers
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Get device category for driver matching
    pub fn get_device_category(&self, device: &PciDevice) -> DeviceCategory {
        DeviceCategory::from_pci_class(device.class_code, device.subclass)
    }

    /// Get recommended driver for device
    pub fn get_recommended_driver(&self, device: &PciDevice) -> &'static str {
        match self.get_device_category(device) {
            DeviceCategory::NetworkCard => {
                match device.vendor_id {
                    0x8086 => "e1000", // Intel
                    0x10EC => "r8169", // Realtek
                    0x14E4 => "tg3",   // Broadcom
                    _ => "generic_net",
                }
            }
            DeviceCategory::StorageController => match device.subclass {
                0x01 => "ide",
                0x06 => "ahci",
                0x07 => "nvme",
                _ => "generic_storage",
            },
            DeviceCategory::GraphicsCard => {
                match device.vendor_id {
                    0x8086 => "i915",    // Intel
                    0x1002 => "amdgpu",  // AMD/ATI
                    0x10DE => "nouveau", // NVIDIA
                    _ => "generic_gpu",
                }
            }
            DeviceCategory::AudioDevice => "hda",
            DeviceCategory::UsbController => match device.subclass {
                0x00 => "uhci",
                0x10 => "ohci",
                0x20 => "ehci",
                0x30 => "xhci",
                _ => "generic_usb",
            },
            DeviceCategory::WirelessCard => {
                match device.vendor_id {
                    0x8086 => "iwlwifi",   // Intel
                    0x168C => "ath9k",     // Atheros
                    0x14E4 => "brcm80211", // Broadcom
                    _ => "generic_wifi",
                }
            }
            _ => "generic",
        }
    }

    /// Check if device supports hot-plug
    pub fn supports_hot_plug(&self, device: &PciDevice) -> bool {
        device.capabilities.hot_plug
    }

    /// Get device power capabilities
    pub fn get_power_capabilities(&self, device: &PciDevice) -> Vec<&'static str> {
        let mut capabilities = Vec::new();

        if device.capabilities.power_management {
            capabilities.push("Power Management");
        }

        if device.capabilities.msi {
            capabilities.push("MSI");
        }

        if device.capabilities.msi_x {
            capabilities.push("MSI-X");
        }

        if device.capabilities.pci_express {
            capabilities.push("PCI Express");
        }

        capabilities
    }

    /// Production hardware detection report - only critical issues
    pub fn print_detection_report(&self, results: &HardwareDetectionResults) {
        // Production: only report critical information and errors

        // Report only critical resource conflicts
        if !results.resource_conflicts.is_empty() {
            for conflict in &results.resource_conflicts {
                if conflict.severity == ConflictSeverity::Critical {
                    crate::println!(
                        "Critical PCI conflict: {} and {}",
                        conflict.device1.location(),
                        conflict.device2.location()
                    );
                }
            }
        }

        // Only report if no devices found (system problem)
        if results.total_devices == 0 {
            crate::println!("Warning: No PCI devices detected");
        }
    }
}

/// Initialize hardware detection and return results
pub fn detect_and_report_hardware() -> Result<HardwareDetectionResults, &'static str> {
    let detector = HardwareDetector::new();
    let results = detector.detect_hardware()?;
    detector.print_detection_report(&results);
    Ok(results)
}

/// Get device by location string (e.g., "00:1f.2")
pub fn find_device_by_location(location: &str) -> Option<PciDevice> {
    let scanner = get_pci_scanner().lock();

    // Parse location string
    let parts: Vec<&str> = location.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let bus = u8::from_str_radix(parts[0], 16).ok()?;
    let dev_func: Vec<&str> = parts[1].split('.').collect();
    if dev_func.len() != 2 {
        return None;
    }

    let device = u8::from_str_radix(dev_func[0], 16).ok()?;
    let function = u8::from_str_radix(dev_func[1], 10).ok()?;

    scanner
        .get_devices()
        .iter()
        .find(|d| d.bus == bus && d.device == device && d.function == function)
        .cloned()
}

/// Get all devices of a specific category
pub fn get_devices_by_category(category: DeviceCategory) -> Vec<PciDevice> {
    let scanner = get_pci_scanner().lock();
    let detector = HardwareDetector::new();

    scanner
        .get_devices()
        .iter()
        .filter(|device| detector.get_device_category(device) == category)
        .cloned()
        .collect()
}
