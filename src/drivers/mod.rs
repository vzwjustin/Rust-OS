//! # RustOS Hardware Drivers Module
//!
//! This module provides a unified interface for all hardware drivers in RustOS,
//! including graphics, input, network, and storage drivers with hot-plug support.

pub mod display;
pub mod hotplug;
pub mod input_manager;
pub mod network;
pub mod pci;
pub mod ps2_controller;
pub mod ps2_mouse;
pub mod storage;
pub mod vbe;
pub mod vbe_io;
pub mod virtio;

// Removed unused imports
use alloc::string::String;
use core::fmt;

// Re-export VBE driver functionality

// Re-export display driver functionality

// Re-export PCI functionality
pub use pci::{
    get_pci_stats, init as init_pci,
};

// Re-export hot-plug functionality
pub use hotplug::{
    get_hotplug_stats, init as init_hotplug,
    process_events as process_hotplug_events,
};

// Re-export input functionality
pub use input_manager::{
    get_cursor_position, get_event as get_input_event, set_cursor_bounds, InputEvent,
    MouseButton,
};

// Re-export storage functionality
pub use storage::StorageDriver;

/// Driver types supported by RustOS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    Graphics,
    Network,
    Storage,
    Input,
    Audio,
    USB,
    PCI,
    System,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverType::Graphics => write!(f, "Graphics"),
            DriverType::Network => write!(f, "Network"),
            DriverType::Storage => write!(f, "Storage"),
            DriverType::Input => write!(f, "Input"),
            DriverType::Audio => write!(f, "Audio"),
            DriverType::USB => write!(f, "USB"),
            DriverType::PCI => write!(f, "PCI"),
            DriverType::System => write!(f, "System"),
        }
    }
}

/// Driver status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStatus {
    Uninitialized,
    Initializing,
    Ready,
    Error,
    Disabled,
}

impl fmt::Display for DriverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverStatus::Uninitialized => write!(f, "Uninitialized"),
            DriverStatus::Initializing => write!(f, "Initializing"),
            DriverStatus::Ready => write!(f, "Ready"),
            DriverStatus::Error => write!(f, "Error"),
            DriverStatus::Disabled => write!(f, "Disabled"),
        }
    }
}

/// Generic driver information
#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub driver_type: DriverType,
    pub status: DriverStatus,
    pub vendor: String,
    pub device_id: Option<u32>,
    pub description: String,
}

impl DriverInfo {
    /// Create new driver info
    pub fn new(
        name: String,
        version: String,
        driver_type: DriverType,
        vendor: String,
        description: String,
    ) -> Self {
        Self {
            name,
            version,
            driver_type,
            status: DriverStatus::Uninitialized,
            vendor,
            device_id: None,
            description,
        }
    }

    /// Set device ID
    pub fn with_device_id(mut self, device_id: u32) -> Self {
        self.device_id = Some(device_id);
        self
    }

    /// Set status
    pub fn with_status(mut self, status: DriverStatus) -> Self {
        self.status = status;
        self
    }
}

/// Hardware device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub name: String,
    pub driver_loaded: bool,
}

impl DeviceInfo {
    /// Create new device info
    pub fn new(
        vendor_id: u16,
        device_id: u16,
        class_code: u8,
        subclass: u8,
        prog_if: u8,
        revision: u8,
        bus: u8,
        device: u8,
        function: u8,
        name: String,
    ) -> Self {
        Self {
            vendor_id,
            device_id,
            class_code,
            subclass,
            prog_if,
            revision,
            bus,
            device,
            function,
            name,
            driver_loaded: false,
        }
    }

    /// Get device type based on class code
    pub fn get_device_type(&self) -> DriverType {
        match self.class_code {
            0x00 => DriverType::System,   // Unclassified
            0x01 => DriverType::Storage,  // Mass Storage Controller
            0x02 => DriverType::Network,  // Network Controller
            0x03 => DriverType::Graphics, // Display Controller
            0x04 => DriverType::Audio,    // Multimedia Controller
            0x06 => DriverType::System,   // Bridge Device
            0x0C => DriverType::USB,      // Serial Bus Controller
            _ => DriverType::System,      // Other/Unknown
        }
    }

    /// Check if this is a graphics device
    pub fn is_graphics_device(&self) -> bool {
        self.class_code == 0x03 || (self.class_code == 0x00 && self.subclass == 0x01)
        // VGA-compatible
    }

    /// Get vendor name
    pub fn get_vendor_name(&self) -> &'static str {
        match self.vendor_id {
            0x8086 => "Intel",
            0x10DE => "NVIDIA",
            0x1002 => "AMD",
            0x1234 => "QEMU",
            0x80EE => "VirtualBox",
            0x15AD => "VMware",
            0x1AF4 => "Virtio",
            _ => "Unknown",
        }
    }
}

/// Driver manager for handling all system drivers
pub struct DriverManager {
    driver_count: usize,
    device_count: usize,
    graphics_initialized: bool,
    input_initialized: bool,
}

impl DriverManager {
    /// Create a new driver manager
    pub const fn new() -> Self {
        Self {
            driver_count: 0,
            device_count: 0,
            graphics_initialized: false,
            input_initialized: false,
        }
    }

    /// Initialize all drivers by scanning PCI for real hardware
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Detect hardware devices via PCI
        self.detect_devices()?;

        // Initialize graphics drivers
        self.init_graphics_drivers()?;

        // Initialize input drivers
        self.init_input_drivers()?;

        // Initialize other drivers as needed
        self.init_system_drivers()?;

        Ok(())
    }

    /// Detect hardware devices by scanning PCI bus
    fn detect_devices(&mut self) -> Result<(), &'static str> {
        use crate::pci::list_devices;

        let devices = list_devices();
        self.device_count = devices.len();

        for dev in devices.iter() {
            crate::serial_println!(
                "pci: {:02x}:{:02x}.{} {:04X}:{:04X} class={:?} ({})",
                dev.bus,
                dev.device,
                dev.function,
                dev.vendor_id,
                dev.device_id,
                dev.class_code,
                dev.name
            );
        }

        Ok(())
    }

    /// Initialize graphics drivers based on PCI detection
    fn init_graphics_drivers(&mut self) -> Result<(), &'static str> {
        use crate::pci::{get_devices_by_class, PciClass};

        let gpu_count = get_devices_by_class(PciClass::Display).len();
        self.driver_count += gpu_count;
        self.graphics_initialized = gpu_count > 0 || display::is_ready();

        Ok(())
    }

    /// Initialize input drivers (PS/2 is already initialized in boot path)
    fn init_input_drivers(&mut self) -> Result<(), &'static str> {
        self.driver_count += 2;
        self.input_initialized = true;
        Ok(())
    }

    /// Initialize system drivers (PCI, hotplug already initialized)
    fn init_system_drivers(&mut self) -> Result<(), &'static str> {
        use crate::pci::{get_devices_by_class, PciClass};

        let net_count = get_devices_by_class(PciClass::Network).len();
        let storage_count = get_devices_by_class(PciClass::MassStorage).len();
        self.driver_count += net_count + storage_count;

        Ok(())
    }

    /// Get driver count by type
    pub fn get_drivers_by_type_count(&self, driver_type: DriverType) -> usize {
        use crate::pci::{get_devices_by_class, PciClass};
        match driver_type {
            DriverType::Graphics => {
                if self.graphics_initialized {
                    1
                } else {
                    0
                }
            }
            DriverType::Network => get_devices_by_class(PciClass::Network).len(),
            DriverType::Storage => get_devices_by_class(PciClass::MassStorage).len(),
            DriverType::Input => {
                if self.input_initialized {
                    1
                } else {
                    0
                }
            }
            DriverType::Audio => 0,
            DriverType::USB => 0,
            DriverType::PCI => 0,
            DriverType::System => 0,
        }
    }

    /// Get driver count
    pub fn driver_count(&self) -> usize {
        self.driver_count
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.device_count
    }

    /// Get ready driver count (graphics + input + PCI devices)
    pub fn ready_driver_count(&self) -> usize {
        use crate::pci::{get_devices_by_class, PciClass};
        let mut count = 0;
        if self.graphics_initialized {
            count += 1;
        }
        if self.input_initialized {
            count += 1;
        }
        count += get_devices_by_class(PciClass::Network).len();
        count += get_devices_by_class(PciClass::MassStorage).len();
        count
    }

    /// Check if graphics is initialized
    pub fn is_graphics_initialized(&self) -> bool {
        self.graphics_initialized
    }

    /// Check if input is initialized
    pub fn is_input_initialized(&self) -> bool {
        self.input_initialized
    }

    /// Get system status
    pub fn get_system_status(&self) -> DriverSystemStatus {
        DriverSystemStatus {
            total_drivers: self.driver_count(),
            ready_drivers: self.ready_driver_count(),
            total_devices: self.device_count(),
            graphics_ready: self.graphics_initialized,
            input_ready: self.input_initialized,
        }
    }
}

/// System-wide driver status
#[derive(Debug, Clone, Copy)]
pub struct DriverSystemStatus {
    pub total_drivers: usize,
    pub ready_drivers: usize,
    pub total_devices: usize,
    pub graphics_ready: bool,
    pub input_ready: bool,
}

/// Global driver manager state (simplified)
static mut DRIVER_MANAGER_INITIALIZED: bool = false;
static mut GRAPHICS_INITIALIZED: bool = false;

/// Initialize the global driver manager (simplified)
pub fn init_drivers() -> Result<(), &'static str> {
    // Initialize PCI subsystem
    if let Err(_e) = init_pci() {
        return Err("PCI initialization failed");
    }

    // Initialize hot-plug subsystem
    if let Err(_e) = init_hotplug() {
        return Err("Hot-plug initialization failed");
    }

    // Process any initial hot-plug events
    let _ = process_hotplug_events();

    // Initialize VirtIO devices (virtio-net, virtio-blk)
    if let Err(e) = virtio::init() {
        crate::serial_println!("virtio: init failed: {}", e);
    }

    unsafe {
        DRIVER_MANAGER_INITIALIZED = true;
        GRAPHICS_INITIALIZED = true;
    }

    // Display driver statistics
    let _pci_stats = get_pci_stats();
    let _hotplug_stats = get_hotplug_stats();

    // Production: drivers initialized silently

    Ok(())
}

/// Check if driver manager is initialized
pub fn is_driver_manager_initialized() -> bool {
    unsafe { DRIVER_MANAGER_INITIALIZED }
}

/// Get driver system status (simplified)
pub fn get_driver_system_status() -> Option<DriverSystemStatus> {
    use crate::pci::{get_devices_by_class, list_devices, PciClass};

    unsafe {
        if DRIVER_MANAGER_INITIALIZED {
            let total_devices = list_devices().len();
            let gpu_count = get_devices_by_class(PciClass::Display).len();
            let net_count = get_devices_by_class(PciClass::Network).len();
            let storage_count = get_devices_by_class(PciClass::MassStorage).len();
            let total_drivers = gpu_count + net_count + storage_count + 2; // +2 for input
            Some(DriverSystemStatus {
                total_drivers,
                ready_drivers: total_drivers,
                total_devices,
                graphics_ready: GRAPHICS_INITIALIZED,
                input_ready: true,
            })
        } else {
            None
        }
    }
}

/// Check if graphics drivers are ready
pub fn is_graphics_ready() -> bool {
    unsafe { GRAPHICS_INITIALIZED }
}

/// Check if input drivers are ready
pub fn is_input_ready() -> bool {
    unsafe { DRIVER_MANAGER_INITIALIZED }
}

/// Print driver information (simplified)
pub fn print_driver_info() {
    unsafe {
        if DRIVER_MANAGER_INITIALIZED {
            // Driver system initialized
            // Total Drivers: 4
            // Ready Drivers: 4
            // Total Devices: 4
            // Graphics Ready
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{serial_print, serial_println, ToString};
    use alloc::format;

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_info_creation() {
        serial_print!("test_driver_info_creation... ");
        let driver = DriverInfo::new(
            "Test Driver".to_string(),
            "1.0.0".to_string(),
            DriverType::Graphics,
            "Test Vendor".to_string(),
            "Test Description".to_string(),
        );

        assert_eq!(driver.name, "Test Driver");
        assert_eq!(driver.version, "1.0.0");
        assert_eq!(driver.driver_type, DriverType::Graphics);
        assert_eq!(driver.status, DriverStatus::Uninitialized);
        assert_eq!(driver.vendor, "Test Vendor");
        assert!(driver.device_id.is_none());
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_device_info_creation() {
        serial_print!("test_device_info_creation... ");
        let device = DeviceInfo::new(
            0x8086,
            0x1234,
            0x03,
            0x00,
            0x00,
            0x01,
            0x00,
            0x02,
            0x00,
            "Test Graphics Card".to_string(),
        );

        assert_eq!(device.vendor_id, 0x8086);
        assert_eq!(device.device_id, 0x1234);
        assert_eq!(device.get_vendor_name(), "Intel");
        assert_eq!(device.get_device_type(), DriverType::Graphics);
        assert!(device.is_graphics_device());
        assert!(!device.driver_loaded);
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_manager_creation() {
        serial_print!("test_driver_manager_creation... ");
        let manager = DriverManager::new();
        assert_eq!(manager.driver_count(), 0);
        assert_eq!(manager.device_count(), 0);
        assert!(!manager.is_graphics_initialized());
        assert!(!manager.is_input_initialized());
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_types_display() {
        serial_print!("test_driver_types_display... ");
        assert_eq!(format!("{}", DriverType::Graphics), "Graphics");
        assert_eq!(format!("{}", DriverType::Network), "Network");
        assert_eq!(format!("{}", DriverType::Storage), "Storage");
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_status_display() {
        serial_print!("test_driver_status_display... ");
        assert_eq!(format!("{}", DriverStatus::Ready), "Ready");
        assert_eq!(format!("{}", DriverStatus::Error), "Error");
        assert_eq!(format!("{}", DriverStatus::Uninitialized), "Uninitialized");
        serial_println!("[ok]");
    }
}
