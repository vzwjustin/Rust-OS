//! PCI (Peripheral Component Interconnect) driver for RustOS
//!
//! This module provides PCI bus enumeration, device detection,
//! and configuration space access for hot-plug support.

use super::hotplug::add_device;
use super::DeviceInfo;
use alloc::{collections::BTreeMap, format, vec::Vec};
use core::fmt;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

/// PCI configuration space registers
pub const PCI_VENDOR_ID: u8 = 0x00;
pub const PCI_DEVICE_ID: u8 = 0x02;
pub const PCI_COMMAND: u8 = 0x04;
pub const PCI_STATUS: u8 = 0x06;
pub const PCI_REVISION_ID: u8 = 0x08;
pub const PCI_PROG_IF: u8 = 0x09;
pub const PCI_SUBCLASS: u8 = 0x0A;
pub const PCI_CLASS_CODE: u8 = 0x0B;
pub const PCI_HEADER_TYPE: u8 = 0x0E;

/// PCI command register bits
pub const PCI_COMMAND_IO: u16 = 0x0001;
pub const PCI_COMMAND_MEMORY: u16 = 0x0002;
pub const PCI_COMMAND_MASTER: u16 = 0x0004;
pub const PCI_COMMAND_INTERRUPT_DISABLE: u16 = 0x0400;

/// PCI header types
pub const PCI_HEADER_TYPE_NORMAL: u8 = 0x00;
pub const PCI_HEADER_TYPE_BRIDGE: u8 = 0x01;
pub const PCI_HEADER_TYPE_CARDBUS: u8 = 0x02;

/// PCI device address
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub slot: u8,
}

impl PciAddress {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            bus,
            device,
            function,
            slot: device,
        }
    }

    /// Convert to configuration address format
    pub fn config_address(&self, register: u8) -> u32 {
        0x80000000
            | ((self.bus as u32) << 16)
            | ((self.device as u32) << 11)
            | ((self.function as u32) << 8)
            | ((register & 0xFC) as u32)
    }
}

impl fmt::Display for PciAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// PCI device configuration
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub address: PciAddress,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub command: u16,
    pub status: u16,
    pub bars: [u32; 6],
    pub subsystem_vendor_id: Option<u16>,
    pub subsystem_device_id: Option<u16>,
    pub interrupt_line: Option<u8>,
    pub interrupt_pin: Option<u8>,
}

impl PciDevice {
    /// Create device info from PCI device
    pub fn to_device_info(&self) -> DeviceInfo {
        let name = format!(
            "{} PCI Device {:04x}:{:04x}",
            self.get_vendor_name(),
            self.vendor_id,
            self.device_id
        );

        DeviceInfo::new(
            self.vendor_id,
            self.device_id,
            self.class_code,
            self.subclass,
            self.prog_if,
            self.revision,
            self.address.bus,
            self.address.device,
            self.address.function,
            name,
        )
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
            0x1013 => "Cirrus Logic",
            0x5333 => "S3 Graphics",
            0x1106 => "VIA Technologies",
            0x10EC => "Realtek",
            _ => "Unknown",
        }
    }

    /// Get device class name
    pub fn get_class_name(&self) -> &'static str {
        match self.class_code {
            0x00 => "Unclassified",
            0x01 => "Mass Storage Controller",
            0x02 => "Network Controller",
            0x03 => "Display Controller",
            0x04 => "Multimedia Controller",
            0x05 => "Memory Controller",
            0x06 => "Bridge Device",
            0x07 => "Simple Communication Controller",
            0x08 => "Base System Peripheral",
            0x09 => "Input Device Controller",
            0x0A => "Docking Station",
            0x0B => "Processor",
            0x0C => "Serial Bus Controller",
            0x0D => "Wireless Controller",
            0x0E => "Intelligent Controller",
            0x0F => "Satellite Communication Controller",
            0x10 => "Encryption Controller",
            0x11 => "Signal Processing Controller",
            _ => "Unknown",
        }
    }

    /// Check if device is a bridge
    pub fn is_bridge(&self) -> bool {
        (self.header_type & 0x7F) == PCI_HEADER_TYPE_BRIDGE
    }

    /// Check if device is multifunction
    pub fn is_multifunction(&self) -> bool {
        (self.header_type & 0x80) != 0
    }

    /// Enable device
    pub fn enable(&mut self) -> Result<(), PciError> {
        self.command |= PCI_COMMAND_IO | PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER;
        PCI_BUS.write_config_word(self.address, PCI_COMMAND, self.command)?;
        Ok(())
    }

    /// Disable device
    pub fn disable(&mut self) -> Result<(), PciError> {
        self.command &= !(PCI_COMMAND_IO | PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER);
        PCI_BUS.write_config_word(self.address, PCI_COMMAND, self.command)?;
        Ok(())
    }
}

/// PCI error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciError {
    /// Invalid address
    InvalidAddress,
    /// Device not found
    DeviceNotFound,
    /// Configuration access failed
    ConfigAccessFailed,
    /// Invalid register
    InvalidRegister,
    /// Operation not supported
    NotSupported,
}

impl fmt::Display for PciError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PciError::InvalidAddress => write!(f, "Invalid PCI address"),
            PciError::DeviceNotFound => write!(f, "PCI device not found"),
            PciError::ConfigAccessFailed => write!(f, "PCI configuration access failed"),
            PciError::InvalidRegister => write!(f, "Invalid PCI register"),
            PciError::NotSupported => write!(f, "Operation not supported"),
        }
    }
}

/// PCI result type
pub type PciResult<T> = Result<T, PciError>;

/// PCI bus manager
pub struct PciBus {
    /// Discovered devices
    devices: RwLock<BTreeMap<PciAddress, PciDevice>>,
    /// Configuration space access method
    config_method: Mutex<ConfigMethod>,
    /// Scan complete flag
    scan_complete: RwLock<bool>,
}

/// Configuration space access methods
#[derive(Debug, Clone, Copy)]
enum ConfigMethod {
    /// Legacy I/O port method
    IoPort,
    /// Memory-mapped configuration (MMCONFIG)
    MemoryMapped(u64), // Base address
}

impl PciBus {
    /// Create new PCI bus manager
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(BTreeMap::new()),
            config_method: Mutex::new(ConfigMethod::IoPort),
            scan_complete: RwLock::new(false),
        }
    }

    /// Initialize PCI subsystem
    pub fn init(&self) -> PciResult<()> {
        // Detect configuration method
        self.detect_config_method()?;

        // Scan for devices
        self.scan_bus()?;

        // Production: PCI subsystem initialized
        Ok(())
    }

    /// Detect PCI configuration access method
    fn detect_config_method(&self) -> PciResult<()> {
        // Try to detect MMCONFIG from ACPI MCFG table
        #[cfg(not(test))]
        {
            use crate::acpi;

            // Attempt to get MCFG table from ACPI
            if let Ok(mcfg_address) = acpi::get_table_address(b"MCFG") {
                // MCFG table structure:
                // 0x00-0x03: Signature ("MCFG")
                // 0x04-0x07: Length
                // 0x08: Revision
                // 0x09: Checksum
                // 0x0A-0x0F: OEMID
                // 0x10-0x17: OEM Table ID
                // 0x18-0x1B: OEM Revision
                // 0x1C-0x1F: Creator ID
                // 0x20-0x23: Creator Revision
                // 0x24-0x2B: Reserved (8 bytes)
                // 0x2C+: Configuration Space Base Address Allocation Structures

                // Each allocation structure is 16 bytes:
                // 0x00-0x07: Base Address (64-bit)
                // 0x08-0x09: PCI Segment Group Number
                // 0x0A: Start PCI Bus Number
                // 0x0B: End PCI Bus Number
                // 0x0C-0x0F: Reserved

                unsafe {
                    let mcfg_ptr = mcfg_address as *const u8;

                    // Read length at offset 0x04
                    let length = core::ptr::read_volatile(mcfg_ptr.add(0x04) as *const u32);

                    // MCFG header is 44 bytes (0x2C)
                    if length >= 44 + 16 {
                        // Read first allocation structure at offset 0x2C
                        let base_address_ptr = mcfg_ptr.add(0x2C) as *const u64;
                        let base_address = core::ptr::read_volatile(base_address_ptr);

                        if base_address != 0 {
                            // MMCONFIG base address found
                            *self.config_method.lock() = ConfigMethod::MemoryMapped(base_address);
                            // Production: MMCONFIG detected and enabled
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Fallback to I/O port method if MMCONFIG not available
        *self.config_method.lock() = ConfigMethod::IoPort;
        // Production: I/O port configuration method selected
        Ok(())
    }

    /// Read 32-bit value from configuration space
    pub fn read_config_dword(&self, address: PciAddress, register: u8) -> PciResult<u32> {
        if register & 0x03 != 0 {
            return Err(PciError::InvalidRegister);
        }

        let config_method = *self.config_method.lock();
        match config_method {
            ConfigMethod::IoPort => {
                let config_address = address.config_address(register);

                // Write address to CONFIG_ADDRESS (0xCF8)
                unsafe {
                    x86_64::instructions::port::Port::new(0xCF8).write(config_address);
                    // Read data from CONFIG_DATA (0xCFC)
                    Ok(x86_64::instructions::port::Port::new(0xCFC).read())
                }
            }
            ConfigMethod::MemoryMapped(base) => {
                // Calculate MMCONFIG address:
                // Base + (Bus << 20) + (Device << 15) + (Function << 12) + Register
                let offset = ((address.bus as u64) << 20)
                    | ((address.slot as u64) << 15)
                    | ((address.function as u64) << 12)
                    | (register as u64);

                let mmconfig_addr = base + offset;

                unsafe {
                    // Read 32-bit value from memory-mapped configuration space
                    Ok(core::ptr::read_volatile(mmconfig_addr as *const u32))
                }
            }
        }
    }

    /// Write 32-bit value to configuration space
    pub fn write_config_dword(
        &self,
        address: PciAddress,
        register: u8,
        value: u32,
    ) -> PciResult<()> {
        if register & 0x03 != 0 {
            return Err(PciError::InvalidRegister);
        }

        let config_method = *self.config_method.lock();
        match config_method {
            ConfigMethod::IoPort => {
                let config_address = address.config_address(register);

                unsafe {
                    x86_64::instructions::port::Port::new(0xCF8).write(config_address);
                    x86_64::instructions::port::Port::new(0xCFC).write(value);
                }
                Ok(())
            }
            ConfigMethod::MemoryMapped(base) => {
                // Calculate MMCONFIG address:
                // Base + (Bus << 20) + (Device << 15) + (Function << 12) + Register
                let offset = ((address.bus as u64) << 20)
                    | ((address.slot as u64) << 15)
                    | ((address.function as u64) << 12)
                    | (register as u64);

                let mmconfig_addr = base + offset;

                unsafe {
                    // Write 32-bit value to memory-mapped configuration space
                    core::ptr::write_volatile(mmconfig_addr as *mut u32, value);
                }
                Ok(())
            }
        }
    }

    /// Read 16-bit value from configuration space
    pub fn read_config_word(&self, address: PciAddress, register: u8) -> PciResult<u16> {
        let dword = self.read_config_dword(address, register & 0xFC)?;
        let shift = (register & 0x02) * 8;
        Ok((dword >> shift) as u16)
    }

    /// Write 16-bit value to configuration space
    pub fn write_config_word(
        &self,
        address: PciAddress,
        register: u8,
        value: u16,
    ) -> PciResult<()> {
        let aligned_reg = register & 0xFC;
        let shift = (register & 0x02) * 8;

        let dword = self.read_config_dword(address, aligned_reg)?;
        let mask = 0xFFFF << shift;
        let new_dword = (dword & !mask) | ((value as u32) << shift);

        self.write_config_dword(address, aligned_reg, new_dword)
    }

    /// Read 8-bit value from configuration space
    pub fn read_config_byte(&self, address: PciAddress, register: u8) -> PciResult<u8> {
        let dword = self.read_config_dword(address, register & 0xFC)?;
        let shift = (register & 0x03) * 8;
        Ok((dword >> shift) as u8)
    }

    /// Write 8-bit value to configuration space
    pub fn write_config_byte(&self, address: PciAddress, register: u8, value: u8) -> PciResult<()> {
        let aligned_reg = register & 0xFC;
        let shift = (register & 0x03) * 8;

        let dword = self.read_config_dword(address, aligned_reg)?;
        let mask = 0xFF << shift;
        let new_dword = (dword & !mask) | ((value as u32) << shift);

        self.write_config_dword(address, aligned_reg, new_dword)
    }

    /// Check if device exists at address
    pub fn device_exists(&self, address: PciAddress) -> bool {
        if let Ok(vendor_id) = self.read_config_word(address, PCI_VENDOR_ID) {
            vendor_id != 0xFFFF
        } else {
            false
        }
    }

    /// Read device configuration
    fn read_device_config(&self, address: PciAddress) -> PciResult<PciDevice> {
        if !self.device_exists(address) {
            return Err(PciError::DeviceNotFound);
        }

        let vendor_id = self.read_config_word(address, PCI_VENDOR_ID)?;
        let device_id = self.read_config_word(address, PCI_DEVICE_ID)?;
        let command = self.read_config_word(address, PCI_COMMAND)?;
        let status = self.read_config_word(address, PCI_STATUS)?;
        let revision = self.read_config_byte(address, PCI_REVISION_ID)?;
        let prog_if = self.read_config_byte(address, PCI_PROG_IF)?;
        let subclass = self.read_config_byte(address, PCI_SUBCLASS)?;
        let class_code = self.read_config_byte(address, PCI_CLASS_CODE)?;
        let header_type = self.read_config_byte(address, PCI_HEADER_TYPE)?;

        // Read BARs (Base Address Registers)
        let mut bars = [0u32; 6];
        for i in 0..6 {
            bars[i] = self.read_config_dword(address, 0x10 + (i as u8 * 4))?;
        }

        // Read subsystem information (for header type 0)
        let (subsystem_vendor_id, subsystem_device_id) =
            if (header_type & 0x7F) == PCI_HEADER_TYPE_NORMAL {
                let sub_vendor = self.read_config_word(address, 0x2C).ok();
                let sub_device = self.read_config_word(address, 0x2E).ok();
                (sub_vendor, sub_device)
            } else {
                (None, None)
            };

        // Read interrupt information
        let interrupt_line = self.read_config_byte(address, 0x3C).ok();
        let interrupt_pin = self.read_config_byte(address, 0x3D).ok();

        Ok(PciDevice {
            address,
            vendor_id,
            device_id,
            class_code,
            subclass,
            prog_if,
            revision,
            header_type,
            command,
            status,
            bars,
            subsystem_vendor_id,
            subsystem_device_id,
            interrupt_line,
            interrupt_pin,
        })
    }

    /// Scan PCI bus for devices
    pub fn scan_bus(&self) -> PciResult<usize> {
        let mut device_count = 0;
        let mut devices = self.devices.write();

        // Production: PCI bus scan in progress

        // Scan all possible bus/device/function combinations
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    let address = PciAddress::new(bus, device, function);

                    if let Ok(pci_device) = self.read_device_config(address) {
                        // Production: PCI device enumerated silently
                        let _device_id = pci_device.device_id;
                        let _class_name = pci_device.get_class_name();

                        // Add to hot-plug system
                        let device_info = pci_device.to_device_info();
                        if let Err(_e) = add_device(device_info) {
                            // Production: hot-plug registration issue
                        }

                        devices.insert(address, pci_device);
                        device_count += 1;

                        // If this is not a multifunction device and function is 0,
                        // skip other functions
                        if function == 0 && !devices[&address].is_multifunction() {
                            break;
                        }
                    } else if function == 0 {
                        // If function 0 doesn't exist, skip other functions
                        break;
                    }
                }
            }
        }

        *self.scan_complete.write() = true;
        // Production: PCI scan completed
        Ok(device_count)
    }

    /// Get device by address
    pub fn get_device(&self, address: PciAddress) -> Option<PciDevice> {
        let devices = self.devices.read();
        devices.get(&address).cloned()
    }

    /// List all devices
    pub fn list_devices(&self) -> Vec<PciDevice> {
        let devices = self.devices.read();
        devices.values().cloned().collect()
    }

    /// Get devices by class
    pub fn get_devices_by_class(&self, class_code: u8) -> Vec<PciDevice> {
        let devices = self.devices.read();
        devices
            .values()
            .filter(|device| device.class_code == class_code)
            .cloned()
            .collect()
    }

    /// Get devices by vendor
    pub fn get_devices_by_vendor(&self, vendor_id: u16) -> Vec<PciDevice> {
        let devices = self.devices.read();
        devices
            .values()
            .filter(|device| device.vendor_id == vendor_id)
            .cloned()
            .collect()
    }

    /// Enable device
    pub fn enable_device(&self, address: PciAddress) -> PciResult<()> {
        let mut devices = self.devices.write();
        if let Some(device) = devices.get_mut(&address) {
            device.enable()
        } else {
            Err(PciError::DeviceNotFound)
        }
    }

    /// Disable device
    pub fn disable_device(&self, address: PciAddress) -> PciResult<()> {
        let mut devices = self.devices.write();
        if let Some(device) = devices.get_mut(&address) {
            device.disable()
        } else {
            Err(PciError::DeviceNotFound)
        }
    }

    /// Get PCI statistics
    pub fn get_stats(&self) -> PciStats {
        let devices = self.devices.read();
        let scan_complete = *self.scan_complete.read();

        let mut stats = PciStats {
            total_devices: devices.len(),
            scan_complete,
            devices_by_class: [0; 18],
            bridges: 0,
            multifunction_devices: 0,
        };

        for device in devices.values() {
            if device.class_code < 18 {
                stats.devices_by_class[device.class_code as usize] += 1;
            }

            if device.is_bridge() {
                stats.bridges += 1;
            }

            if device.is_multifunction() {
                stats.multifunction_devices += 1;
            }
        }

        stats
    }
}

/// PCI statistics
#[derive(Debug, Clone)]
pub struct PciStats {
    pub total_devices: usize,
    pub scan_complete: bool,
    pub devices_by_class: [usize; 18],
    pub bridges: usize,
    pub multifunction_devices: usize,
}

lazy_static! {
    static ref PCI_BUS: PciBus = PciBus::new();
}

/// Initialize PCI subsystem
pub fn init() -> PciResult<()> {
    PCI_BUS.init()
}

/// Get the global PCI bus
pub fn pci_bus() -> &'static PciBus {
    &PCI_BUS
}

/// Scan for PCI devices
pub fn scan_devices() -> PciResult<usize> {
    PCI_BUS.scan_bus()
}

/// Get PCI device by address
pub fn get_device(bus: u8, device: u8, function: u8) -> Option<PciDevice> {
    let address = PciAddress::new(bus, device, function);
    PCI_BUS.get_device(address)
}

/// List all PCI devices
pub fn list_devices() -> Vec<PciDevice> {
    PCI_BUS.list_devices()
}

/// Get PCI statistics
pub fn get_pci_stats() -> PciStats {
    PCI_BUS.get_stats()
}

/// Find devices by vendor and device ID
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    let devices = PCI_BUS.list_devices();
    devices
        .into_iter()
        .find(|device| device.vendor_id == vendor_id && device.device_id == device_id)
}

/// Enable PCI device
pub fn enable_device(bus: u8, device: u8, function: u8) -> PciResult<()> {
    let address = PciAddress::new(bus, device, function);
    PCI_BUS.enable_device(address)
}

/// Disable PCI device
pub fn disable_device(bus: u8, device: u8, function: u8) -> PciResult<()> {
    let address = PciAddress::new(bus, device, function);
    PCI_BUS.disable_device(address)
}
