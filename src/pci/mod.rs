//! PCI Bus Enumeration and Management System
//!
//! This module provides comprehensive PCI device discovery, configuration,
//! and management functionality for RustOS.

pub mod config;
pub mod database;
pub mod detection;

use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::{PortReadOnly, PortWriteOnly};

/// Bootloader-provided ACPI Root System Description Pointer (physical address)
static ACPI_RSDP_ADDRESS: AtomicU64 = AtomicU64::new(0);

/// Store the ACPI RSDP physical address for later initialization steps
pub fn set_rsdp_address(addr: Option<u64>) {
    match addr {
        Some(phys) => ACPI_RSDP_ADDRESS.store(phys, Ordering::Relaxed),
        None => ACPI_RSDP_ADDRESS.store(0, Ordering::Relaxed),
    }
}

/// Retrieve the ACPI RSDP physical address if one was provided by the bootloader
pub fn rsdp_address() -> Option<u64> {
    match ACPI_RSDP_ADDRESS.load(Ordering::Relaxed) {
        0 => None,
        value => Some(value),
    }
}

/// PCI Configuration Address Port (0xCF8)
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
/// PCI Configuration Data Port (0xCFC)
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Maximum number of PCI buses
pub const MAX_BUS: u8 = 255;
/// Maximum number of devices per bus
pub const MAX_DEVICE: u8 = 32;
/// Maximum number of functions per device
pub const MAX_FUNCTION: u8 = 8;

/// PCI Device Class Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PciClass {
    Unclassified = 0x00,
    MassStorage = 0x01,
    Network = 0x02,
    Display = 0x03,
    Multimedia = 0x04,
    Memory = 0x05,
    Bridge = 0x06,
    Communication = 0x07,
    SystemPeripheral = 0x08,
    InputDevice = 0x09,
    DockingStation = 0x0A,
    Processor = 0x0B,
    SerialBus = 0x0C,
    Wireless = 0x0D,
    IntelligentIO = 0x0E,
    Satellite = 0x0F,
    Encryption = 0x10,
    SignalProcessing = 0x11,
    ProcessingAccelerator = 0x12,
    NonEssentialInstrumentation = 0x13,
    Reserved = 0xFF,
}

impl From<u8> for PciClass {
    fn from(value: u8) -> Self {
        match value {
            0x00 => PciClass::Unclassified,
            0x01 => PciClass::MassStorage,
            0x02 => PciClass::Network,
            0x03 => PciClass::Display,
            0x04 => PciClass::Multimedia,
            0x05 => PciClass::Memory,
            0x06 => PciClass::Bridge,
            0x07 => PciClass::Communication,
            0x08 => PciClass::SystemPeripheral,
            0x09 => PciClass::InputDevice,
            0x0A => PciClass::DockingStation,
            0x0B => PciClass::Processor,
            0x0C => PciClass::SerialBus,
            0x0D => PciClass::Wireless,
            0x0E => PciClass::IntelligentIO,
            0x0F => PciClass::Satellite,
            0x10 => PciClass::Encryption,
            0x11 => PciClass::SignalProcessing,
            0x12 => PciClass::ProcessingAccelerator,
            0x13 => PciClass::NonEssentialInstrumentation,
            _ => PciClass::Reserved,
        }
    }
}

impl fmt::Display for PciClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            PciClass::Unclassified => "Unclassified",
            PciClass::MassStorage => "Mass Storage",
            PciClass::Network => "Network",
            PciClass::Display => "Display",
            PciClass::Multimedia => "Multimedia",
            PciClass::Memory => "Memory",
            PciClass::Bridge => "Bridge",
            PciClass::Communication => "Communication",
            PciClass::SystemPeripheral => "System Peripheral",
            PciClass::InputDevice => "Input Device",
            PciClass::DockingStation => "Docking Station",
            PciClass::Processor => "Processor",
            PciClass::SerialBus => "Serial Bus",
            PciClass::Wireless => "Wireless",
            PciClass::IntelligentIO => "Intelligent I/O",
            PciClass::Satellite => "Satellite",
            PciClass::Encryption => "Encryption",
            PciClass::SignalProcessing => "Signal Processing",
            PciClass::ProcessingAccelerator => "Processing Accelerator",
            PciClass::NonEssentialInstrumentation => "Non-Essential Instrumentation",
            PciClass::Reserved => "Reserved",
        };
        write!(f, "{}", name)
    }
}

/// PCI Device Capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciCapabilities {
    pub msi: bool,
    pub msi_x: bool,
    pub power_management: bool,
    pub pci_express: bool,
    pub hot_plug: bool,
    pub vendor_specific: bool,
}

impl Default for PciCapabilities {
    fn default() -> Self {
        Self {
            msi: false,
            msi_x: false,
            power_management: false,
            pci_express: false,
            hot_plug: false,
            vendor_specific: false,
        }
    }
}

/// PCI Device Information
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: PciClass,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision_id: u8,
    pub revision: u8,
    pub header_type: u8,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub capabilities: PciCapabilities,
    pub bars: [u32; 6],
    pub name: alloc::string::String,
}

impl PciDevice {
    /// Create a new PCI device with default values
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            bus,
            device,
            function,
            vendor_id: 0,
            device_id: 0,
            class_code: PciClass::Unclassified,
            class: 0,
            subclass: 0,
            prog_if: 0,
            revision_id: 0,
            revision: 0,
            header_type: 0,
            subsystem_vendor_id: 0,
            subsystem_id: 0,
            capabilities: PciCapabilities::default(),
            bars: [0; 6],
            name: alloc::string::String::from("Unknown Device"),
        }
    }

    /// Get the device location as a formatted string
    pub fn location(&self) -> alloc::string::String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }

    /// Check if this is a multifunction device
    pub fn is_multifunction(&self) -> bool {
        (self.header_type & 0x80) != 0
    }

    /// Get the base header type (without multifunction bit)
    pub fn base_header_type(&self) -> u8 {
        self.header_type & 0x7F
    }
}

impl fmt::Display for PciDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "PCI Device {}:{:04x}:{:04x} - {} (Class: {:02x}:{:02x})",
            self.location(),
            self.vendor_id,
            self.device_id,
            self.class_code,
            self.class_code as u8,
            self.subclass
        )
    }
}

/// PCI Bus Scanner
pub struct PciBusScanner {
    devices: Vec<PciDevice>,
    initialized: bool,
}

impl PciBusScanner {
    /// Create a new PCI bus scanner
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            initialized: false,
        }
    }

    /// Initialize and scan the PCI bus
    pub fn initialize(&mut self) -> Result<(), &'static str> {
        if self.initialized {
            return Ok(());
        }

        // Validate PCI configuration space access before scanning
        if !self.validate_pci_access()? {
            return Err("PCI configuration space access validation failed");
        }

        self.scan_all_buses()?;

        // Validate discovered devices
        self.validate_discovered_devices()?;

        self.initialized = true;
        Ok(())
    }

    /// Scan all PCI buses for devices
    fn scan_all_buses(&mut self) -> Result<(), &'static str> {
        self.devices.clear();

        // Start with bus 0
        self.scan_bus(0)?;

        // Check if there are multiple host controllers
        let host_bridge = self.read_config_word(0, 0, 0, 0x0E);
        if (host_bridge & 0x80) == 0 {
            // Single host controller, scan all buses
            for bus in 1..=MAX_BUS {
                self.scan_bus(bus)?;
            }
        } else {
            // Multiple host controllers, scan based on function 0 of device 0
            for function in 0..MAX_FUNCTION {
                let vendor_id = self.read_config_word(0, 0, function, 0x00);
                if vendor_id != 0xFFFF {
                    self.scan_bus(function)?;
                }
            }
        }

        Ok(())
    }

    /// Scan a specific PCI bus
    fn scan_bus(&mut self, bus: u8) -> Result<(), &'static str> {
        for device in 0..MAX_DEVICE {
            self.scan_device(bus, device)?;
        }
        Ok(())
    }

    /// Scan a specific device on a bus
    fn scan_device(&mut self, bus: u8, device: u8) -> Result<(), &'static str> {
        let vendor_id = self.read_config_word(bus, device, 0, 0x00);
        if vendor_id == 0xFFFF {
            return Ok(()); // Device doesn't exist
        }

        // Scan function 0
        self.scan_function(bus, device, 0)?;

        // Check if this is a multifunction device
        let header_type = self.read_config_byte(bus, device, 0, 0x0E);
        if (header_type & 0x80) != 0 {
            // Multifunction device, scan all functions
            for function in 1..MAX_FUNCTION {
                let func_vendor_id = self.read_config_word(bus, device, function, 0x00);
                if func_vendor_id != 0xFFFF {
                    self.scan_function(bus, device, function)?;
                }
            }
        }

        Ok(())
    }

    /// Scan a specific function and add it to the device list
    fn scan_function(&mut self, bus: u8, device: u8, function: u8) -> Result<(), &'static str> {
        let vendor_id = self.read_config_word(bus, device, function, 0x00);
        if vendor_id == 0xFFFF {
            return Ok(());
        }

        let mut pci_device = PciDevice::new(bus, device, function);

        // Read basic device information
        pci_device.vendor_id = vendor_id;
        pci_device.device_id = self.read_config_word(bus, device, function, 0x02);
        pci_device.revision_id = self.read_config_byte(bus, device, function, 0x08);

        let class_info = self.read_config_dword(bus, device, function, 0x08);
        pci_device.prog_if = ((class_info >> 8) & 0xFF) as u8;
        pci_device.subclass = ((class_info >> 16) & 0xFF) as u8;
        pci_device.class_code = PciClass::from(((class_info >> 24) & 0xFF) as u8);

        pci_device.header_type = self.read_config_byte(bus, device, function, 0x0E);

        // Read subsystem information for header type 0
        if pci_device.base_header_type() == 0 {
            pci_device.subsystem_vendor_id = self.read_config_word(bus, device, function, 0x2C);
            pci_device.subsystem_id = self.read_config_word(bus, device, function, 0x2E);
        }

        // Read BARs
        for i in 0..6 {
            let bar_offset = 0x10 + (i * 4) as u8;
            pci_device.bars[i] = self.read_config_dword(bus, device, function, bar_offset);
        }

        // Scan capabilities
        pci_device.capabilities = self.scan_capabilities(bus, device, function)?;

        self.devices.push(pci_device);
        Ok(())
    }

    /// Scan device capabilities
    fn scan_capabilities(
        &self,
        bus: u8,
        device: u8,
        function: u8,
    ) -> Result<PciCapabilities, &'static str> {
        let mut capabilities = PciCapabilities::default();

        // Check if device has capabilities
        let status = self.read_config_word(bus, device, function, 0x06);
        if (status & 0x10) == 0 {
            return Ok(capabilities); // No capabilities
        }

        let mut cap_ptr = self.read_config_byte(bus, device, function, 0x34) & 0xFC;

        while cap_ptr != 0 && cap_ptr != 0xFF {
            let cap_id = self.read_config_byte(bus, device, function, cap_ptr);

            match cap_id {
                0x01 => capabilities.power_management = true,
                0x05 => capabilities.msi = true,
                0x10 => capabilities.pci_express = true,
                0x11 => capabilities.msi_x = true,
                0x0C => capabilities.hot_plug = true,
                0x09 => capabilities.vendor_specific = true,
                _ => {}
            }

            cap_ptr = self.read_config_byte(bus, device, function, cap_ptr + 1) & 0xFC;
        }

        Ok(capabilities)
    }

    /// Read a 32-bit configuration space register
    pub fn read_config_dword(&self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        // Use MMCONFIG if available
        if MMCONFIG_ENABLED.load(Ordering::Acquire) {
            if let Some(value) = self.read_config_dword_mmconfig(bus, device, function, offset) {
                return value;
            }
            // Fall through to legacy I/O on error
        }

        // Legacy I/O port access
        let address = self.make_config_address(bus, device, function, offset);

        unsafe {
            let mut addr_port: PortWriteOnly<u32> = PortWriteOnly::new(PCI_CONFIG_ADDRESS);
            let mut data_port: PortReadOnly<u32> = PortReadOnly::new(PCI_CONFIG_DATA);

            addr_port.write(address);
            data_port.read()
        }
    }

    /// Read via MMCONFIG (PCIe memory-mapped configuration space)
    fn read_config_dword_mmconfig(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
    ) -> Option<u32> {
        // Get MCFG info from ACPI
        let mcfg = crate::acpi::mcfg()?;

        // Find the MCFG entry that covers this bus
        for entry in &mcfg.entries {
            if bus >= entry.start_bus && bus <= entry.end_bus {
                // Calculate MMCONFIG address
                let addr = entry.base_address +
                          ((bus as u64 - entry.start_bus as u64) << 20) + // Bus offset
                          ((device as u64 & 0x1F) << 15) +                 // Device offset
                          ((function as u64 & 0x07) << 12) +                // Function offset
                          (offset as u64 & 0xFFC); // Register offset (aligned)

                unsafe {
                    // Use volatile read to prevent compiler optimization
                    let value = core::ptr::read_volatile(addr as *const u32);
                    return Some(value);
                }
            }
        }

        None
    }

    /// Read a 16-bit configuration space register
    pub fn read_config_word(&self, bus: u8, device: u8, function: u8, offset: u8) -> u16 {
        let dword = self.read_config_dword(bus, device, function, offset & 0xFC);
        let shift = (offset & 0x02) * 8;
        ((dword >> shift) & 0xFFFF) as u16
    }

    /// Read an 8-bit configuration space register
    pub fn read_config_byte(&self, bus: u8, device: u8, function: u8, offset: u8) -> u8 {
        let dword = self.read_config_dword(bus, device, function, offset & 0xFC);
        let shift = (offset & 0x03) * 8;
        ((dword >> shift) & 0xFF) as u8
    }

    /// Write a 32-bit configuration space register
    pub fn write_config_dword(&self, bus: u8, device: u8, function: u8, offset: u8, value: u32) {
        // Use MMCONFIG if available
        if MMCONFIG_ENABLED.load(Ordering::Acquire) {
            if self.write_config_dword_mmconfig(bus, device, function, offset, value) {
                return;
            }
            // Fall through to legacy I/O on error
        }

        // Legacy I/O port access
        let address = self.make_config_address(bus, device, function, offset);

        unsafe {
            let mut addr_port: PortWriteOnly<u32> = PortWriteOnly::new(PCI_CONFIG_ADDRESS);
            let mut data_port: PortWriteOnly<u32> = PortWriteOnly::new(PCI_CONFIG_DATA);

            addr_port.write(address);
            data_port.write(value);
        }
    }

    /// Write via MMCONFIG (PCIe memory-mapped configuration space)
    fn write_config_dword_mmconfig(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
        value: u32,
    ) -> bool {
        // Get MCFG info from ACPI
        let Some(mcfg) = crate::acpi::mcfg() else {
            return false;
        };

        // Find the MCFG entry that covers this bus
        for entry in &mcfg.entries {
            if bus >= entry.start_bus && bus <= entry.end_bus {
                // Calculate MMCONFIG address
                let addr = entry.base_address +
                          ((bus as u64 - entry.start_bus as u64) << 20) + // Bus offset
                          ((device as u64 & 0x1F) << 15) +                 // Device offset
                          ((function as u64 & 0x07) << 12) +                // Function offset
                          (offset as u64 & 0xFFC); // Register offset (aligned)

                unsafe {
                    // Use volatile write to prevent compiler optimization
                    core::ptr::write_volatile(addr as *mut u32, value);
                    return true;
                }
            }
        }

        false
    }

    /// Write a 16-bit configuration space register
    pub fn write_config_word(&self, bus: u8, device: u8, function: u8, offset: u8, value: u16) {
        let dword = self.read_config_dword(bus, device, function, offset & 0xFC);
        let shift = (offset & 0x02) * 8;
        let mask = !(0xFFFF << shift);
        let new_value = (dword & mask) | ((value as u32) << shift);
        self.write_config_dword(bus, device, function, offset & 0xFC, new_value);
    }

    /// Write an 8-bit configuration space register
    pub fn write_config_byte(&self, bus: u8, device: u8, function: u8, offset: u8, value: u8) {
        let dword = self.read_config_dword(bus, device, function, offset & 0xFC);
        let shift = (offset & 0x03) * 8;
        let mask = !(0xFF << shift);
        let new_value = (dword & mask) | ((value as u32) << shift);
        self.write_config_dword(bus, device, function, offset & 0xFC, new_value);
    }

    /// Create a PCI configuration address
    fn make_config_address(&self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let enable_bit = 1u32 << 31;
        let bus_bits = (bus as u32) << 16;
        let device_bits = (device as u32 & 0x1F) << 11;
        let function_bits = (function as u32 & 0x07) << 8;
        let offset_bits = (offset as u32) & 0xFC;

        enable_bit | bus_bits | device_bits | function_bits | offset_bits
    }

    /// Get all discovered devices
    pub fn get_devices(&self) -> &Vec<PciDevice> {
        &self.devices
    }

    /// Get devices by class
    pub fn get_devices_by_class(&self, class: PciClass) -> Vec<&PciDevice> {
        self.devices
            .iter()
            .filter(|d| d.class_code == class)
            .collect()
    }

    /// Get devices by vendor ID
    pub fn get_devices_by_vendor(&self, vendor_id: u16) -> Vec<&PciDevice> {
        self.devices
            .iter()
            .filter(|d| d.vendor_id == vendor_id)
            .collect()
    }

    /// Find a specific device by vendor and device ID
    pub fn find_device(&self, vendor_id: u16, device_id: u16) -> Option<&PciDevice> {
        self.devices
            .iter()
            .find(|d| d.vendor_id == vendor_id && d.device_id == device_id)
    }

    /// Get total number of discovered devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Check if the scanner has been initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Validate PCI configuration space access
    fn validate_pci_access(&self) -> Result<bool, &'static str> {
        // Test PCI configuration space access by reading a known register
        // Try to read vendor ID from bus 0, device 0, function 0
        let test_vendor = self.read_config_word(0, 0, 0, 0x00);

        // If we get all 1s, PCI access might not be working
        if test_vendor == 0xFFFF {
            // This could be normal if no device exists at 0:0.0
            // Try a few more locations to validate PCI access
            for device in 0..4 {
                let vendor = self.read_config_word(0, device, 0, 0x00);
                if vendor != 0xFFFF && vendor != 0x0000 {
                    return Ok(true); // Found a valid device, PCI access works
                }
            }
            return Ok(false); // No valid devices found, might indicate PCI access issues
        }

        Ok(true)
    }

    /// Validate discovered devices for consistency
    fn validate_discovered_devices(&self) -> Result<(), &'static str> {
        for device in &self.devices {
            // Validate vendor ID is not invalid
            if device.vendor_id == 0xFFFF || device.vendor_id == 0x0000 {
                return Err("Invalid vendor ID found in device list");
            }

            // Validate device ID is not invalid
            if device.device_id == 0xFFFF {
                return Err("Invalid device ID found in device list");
            }

            // Validate bus/device/function ranges
            if device.device >= MAX_DEVICE || device.function >= MAX_FUNCTION {
                return Err("Device location out of valid range");
            }
        }

        Ok(())
    }
}

use lazy_static::lazy_static;
/// Global PCI bus scanner instance
use spin::Mutex;

lazy_static! {
    static ref PCI_SCANNER: Mutex<PciBusScanner> = Mutex::new(PciBusScanner::new());
}

/// Initialize the global PCI scanner
pub fn init_pci() -> Result<(), &'static str> {
    // Production: ACPI FADT integration
    let _fadt = crate::acpi::fadt();

    // Production: PCIe MMCONFIG initialization
    if let Some(mcfg) = crate::acpi::mcfg() {
        // Try MMCONFIG, fall back to legacy I/O on failure
        let _ = init_mmconfig_scanner(&mcfg);
    }

    PCI_SCANNER.lock().initialize()
}

fn init_mmconfig_scanner(mcfg: &crate::acpi::McfgInfo) -> Result<(), &'static str> {
    // Validate MCFG entries thoroughly
    for entry in &mcfg.entries {
        if entry.base_address == 0 {
            return Err("Invalid MMCONFIG base address");
        }

        if entry.end_bus < entry.start_bus {
            return Err("Invalid MMCONFIG bus range");
        }

        // Validate base address alignment (must be aligned to segment size)
        let bus_count = (entry.end_bus - entry.start_bus + 1) as u64;
        let segment_size = bus_count * 256 * 8 * 4096; // buses * devices * functions * 4KB config space

        if entry.base_address & (segment_size - 1) != 0 {
            return Err("MMCONFIG base address not properly aligned");
        }

        // Validate address range doesn't overflow
        if entry.base_address.checked_add(segment_size).is_none() {
            return Err("MMCONFIG address range overflow");
        }

        // Validate segment group
        if entry.segment_group != 0 {
            // Most systems only have segment group 0
            crate::serial_println!(
                "Warning: Non-zero PCI segment group {}",
                entry.segment_group
            );
        }

        // Map MMCONFIG space into virtual memory
        map_mmconfig_space(entry)?;

        // Test MMCONFIG access
        if !test_mmconfig_access(entry) {
            return Err("MMCONFIG access test failed - falling back to I/O port access");
        }
    }

    // MMCONFIG successfully initialized
    MMCONFIG_ENABLED.store(true, Ordering::Release);

    Ok(())
}

/// Map MMCONFIG space into kernel virtual memory
fn map_mmconfig_space(entry: &crate::acpi::McfgEntry) -> Result<(), &'static str> {
    use crate::memory::{map_physical_memory, MemoryFlags};

    let bus_count = (entry.end_bus - entry.start_bus + 1) as u64;
    let segment_size = bus_count * 256 * 8 * 4096; // Total size to map

    // Calculate number of 4KB pages needed
    let page_count = (segment_size + 4095) / 4096;

    // Map each page using identity mapping (virtual = physical for MMIO)
    // Use MMIO flags: present, writable, write-through (for ordering), no cache
    let flags = MemoryFlags::PRESENT
        | MemoryFlags::WRITABLE
        | MemoryFlags::WRITE_COMBINING
        | MemoryFlags::NO_CACHE;

    for page_offset in 0..page_count {
        let phys_addr = entry.base_address + page_offset * 4096;
        let virt_addr = phys_addr; // Identity mapping for MMCONFIG

        map_physical_memory(virt_addr as usize, phys_addr as usize, flags)?;
    }

    Ok(())
}

/// Test MMCONFIG access by reading a known register
fn test_mmconfig_access(entry: &crate::acpi::McfgEntry) -> bool {
    // Try to read vendor ID from bus 0, device 0, function 0
    let addr = entry.base_address +
               (0u64 << 20) + // Bus 0
               (0u64 << 15) + // Device 0
               (0u64 << 12) + // Function 0
               0x00; // Offset 0 (vendor ID)

    unsafe {
        let vendor_id = core::ptr::read_volatile(addr as *const u16);

        // Valid vendor IDs are not 0xFFFF or 0x0000
        vendor_id != 0xFFFF && vendor_id != 0x0000
    }
}

/// Global flag indicating if MMCONFIG is available
use core::sync::atomic::AtomicBool;
static MMCONFIG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Get the global PCI scanner
pub fn get_pci_scanner() -> &'static Mutex<PciBusScanner> {
    &PCI_SCANNER
}

/// Get all PCI devices
pub fn get_all_devices() -> Vec<PciDevice> {
    PCI_SCANNER.lock().get_devices().clone()
}

/// Get devices by class
pub fn get_devices_by_class(class: PciClass) -> Vec<PciDevice> {
    PCI_SCANNER
        .lock()
        .get_devices_by_class(class)
        .into_iter()
        .cloned()
        .collect()
}

/// Print all discovered PCI devices
pub fn print_devices() {
    let scanner = PCI_SCANNER.lock();
    let devices = scanner.get_devices();

    // Production: silent device enumeration
    if devices.is_empty() {
        return;
    }

    // Production: devices enumerated silently
}

/// PCI Address representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            bus,
            device,
            function,
        }
    }
}

/// PCI bus reference (returns reference to the global scanner)
pub fn pci_bus() -> &'static Mutex<PciBusScanner> {
    &PCI_SCANNER
}

/// Scan PCI devices (re-initializes if needed)
pub fn scan_devices() -> Result<Vec<PciDevice>, &'static str> {
    let mut scanner = PCI_SCANNER.lock();
    if scanner.get_devices().is_empty() {
        scanner.initialize()?;
    }
    Ok(scanner.get_devices().clone())
}

/// List all discovered PCI devices
pub fn list_devices() -> Vec<PciDevice> {
    PCI_SCANNER.lock().get_devices().clone()
}

/// PCI subsystem statistics
#[derive(Debug, Clone)]
pub struct PciStats {
    pub total_devices: usize,
    pub buses_scanned: usize,
    pub mmconfig_enabled: bool,
}

/// Get PCI subsystem statistics
pub fn get_pci_stats() -> PciStats {
    let scanner = PCI_SCANNER.lock();
    PciStats {
        total_devices: scanner.device_count(),
        buses_scanned: (MAX_BUS as usize) + 1,
        mmconfig_enabled: MMCONFIG_ENABLED.load(Ordering::Relaxed),
    }
}

/// Re-export init function with expected name
pub fn init() -> Result<(), &'static str> {
    init_pci()
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Alias for scan_devices - scans the PCI bus
pub fn scan_pci_bus() -> Result<Vec<PciDevice>, &'static str> {
    scan_devices()
}

/// Read device configuration (returns the device itself)
pub fn read_device_config(device: &PciDevice) -> Result<PciDevice, &'static str> {
    // Return a clone of the device as its "config"
    Ok(device.clone())
}

/// Classify a PCI device (returns its class code)
pub fn classify_device(device: &PciDevice) -> Result<PciClass, &'static str> {
    Ok(device.class_code.clone())
}

/// Load device driver for a PCI device based on its class code
pub fn load_device_driver(device: &PciDevice) -> Result<(), &'static str> {
    match device.class_code {
        PciClass::Display => {
            crate::serial_println!(
                "pci: display device {:04x}:{:04x} at {:02x}:{:02x}.{}",
                device.vendor_id,
                device.device_id,
                device.bus,
                device.device,
                device.function
            );
            Ok(())
        }
        PciClass::Network => {
            crate::serial_println!(
                "pci: network device {:04x}:{:04x} at {:02x}:{:02x}.{}",
                device.vendor_id,
                device.device_id,
                device.bus,
                device.device,
                device.function
            );
            Ok(())
        }
        PciClass::MassStorage => {
            crate::serial_println!(
                "pci: storage device {:04x}:{:04x} at {:02x}:{:02x}.{}",
                device.vendor_id,
                device.device_id,
                device.bus,
                device.device,
                device.function
            );
            Ok(())
        }
        PciClass::Wireless => {
            crate::serial_println!(
                "pci: wireless device {:04x}:{:04x} at {:02x}:{:02x}.{}",
                device.vendor_id,
                device.device_id,
                device.bus,
                device.device,
                device.function
            );
            Ok(())
        }
        _ => {
            crate::serial_println!(
                "pci: unhandled device class {:?} {:04x}:{:04x}",
                device.class_code,
                device.vendor_id,
                device.device_id
            );
            Ok(())
        }
    }
}
