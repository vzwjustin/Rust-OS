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
pub const PCI_BAR0: u8 = 0x10;
pub const PCI_CAPABILITY_LIST: u8 = 0x34;
pub const PCI_INTERRUPT_LINE: u8 = 0x3C;
pub const PCI_INTERRUPT_PIN: u8 = 0x3D;

/// PCI-to-PCI bridge configuration registers.
pub const PCI_BRIDGE_PRIMARY_BUS: u8 = 0x18;
pub const PCI_BRIDGE_SECONDARY_BUS: u8 = 0x19;
pub const PCI_BRIDGE_SUBORDINATE_BUS: u8 = 0x1A;

/// PCI command register bits
pub const PCI_COMMAND_IO: u16 = 0x0001;
pub const PCI_COMMAND_MEMORY: u16 = 0x0002;
pub const PCI_COMMAND_MASTER: u16 = 0x0004;
pub const PCI_COMMAND_INTERRUPT_DISABLE: u16 = 0x0400;

/// PCI status register bits
pub const PCI_STATUS_CAP_LIST: u16 = 0x0010;

/// PCI header types
pub const PCI_HEADER_TYPE_NORMAL: u8 = 0x00;
pub const PCI_HEADER_TYPE_BRIDGE: u8 = 0x01;
pub const PCI_HEADER_TYPE_CARDBUS: u8 = 0x02;

/// PCI capability IDs used during boot/common device setup.
pub const PCI_CAP_ID_MSI: u8 = 0x05;
pub const PCI_CAP_ID_PCIE: u8 = 0x10;
pub const PCI_CAP_ID_MSIX: u8 = 0x11;

pub const PCI_ANY_ID: u16 = 0xFFFF;

const PCI_BAR_IO_MASK: u32 = 0xFFFF_FFFC;
const PCI_BAR_MEM_MASK: u32 = 0xFFFF_FFF0;

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

/// Linux-style PCI match table entry. `PCI_ANY_ID` wildcards id fields; class
/// matching uses `(device_class ^ class) & class_mask == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDeviceId {
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
    pub class: u32,
    pub class_mask: u32,
}

impl PciDeviceId {
    pub const fn new(vendor_id: u16, device_id: u16) -> Self {
        Self {
            vendor_id,
            device_id,
            subsystem_vendor_id: PCI_ANY_ID,
            subsystem_device_id: PCI_ANY_ID,
            class: 0,
            class_mask: 0,
        }
    }

    pub const fn class(class_code: u8, subclass: u8, prog_if: u8, mask: u32) -> Self {
        Self {
            vendor_id: PCI_ANY_ID,
            device_id: PCI_ANY_ID,
            subsystem_vendor_id: PCI_ANY_ID,
            subsystem_device_id: PCI_ANY_ID,
            class: ((class_code as u32) << 16) | ((subclass as u32) << 8) | prog_if as u32,
            class_mask: mask,
        }
    }
}

/// Decoded BAR/resource kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciBarKind {
    Io,
    Memory32,
    Memory64,
    MemoryBelow1M,
}

/// Decoded Base Address Register resource, including firmware-assigned base
/// and hardware-reported aperture size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciResource {
    pub bar_index: u8,
    pub kind: PciBarKind,
    pub base: u64,
    pub size: u64,
    pub prefetchable: bool,
    pub raw: u64,
}

/// Generic capability-list entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciCapability {
    pub id: u8,
    pub offset: u8,
    pub next: u8,
}

/// Decoded MSI capability metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciMsiInfo {
    pub capability_offset: u8,
    pub enabled: bool,
    pub multiple_message_capable: u8,
    pub multiple_message_enabled: u8,
    pub address_64bit: bool,
    pub per_vector_masking: bool,
}

/// Decoded MSI-X capability metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciMsixInfo {
    pub capability_offset: u8,
    pub enabled: bool,
    pub function_masked: bool,
    pub table_size: u16,
    pub table_bir: u8,
    pub table_offset: u32,
    pub pba_bir: u8,
    pub pba_offset: u32,
}

/// IRQ routing metadata collected from legacy INTx and MSI/MSI-X capability
/// state. The kernel can use this to choose legacy interrupts or later enable
/// MSI without re-walking config space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciIrqInfo {
    pub legacy_line: Option<u8>,
    pub legacy_pin: Option<u8>,
    pub legacy_enabled: bool,
    pub msi_capable: bool,
    pub msi_enabled: bool,
    pub msix_capable: bool,
    pub msix_enabled: bool,
}

/// Inclusive bridge window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciBridgeWindow {
    pub base: u64,
    pub limit: u64,
}

/// Decoded PCI-to-PCI bridge bus numbers and resource windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciBridgeInfo {
    pub primary_bus: u8,
    pub secondary_bus: u8,
    pub subordinate_bus: u8,
    pub io_window: Option<PciBridgeWindow>,
    pub memory_window: Option<PciBridgeWindow>,
    pub prefetchable_memory_window: Option<PciBridgeWindow>,
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
    pub resources: [Option<PciResource>; 6],
    pub subsystem_vendor_id: Option<u16>,
    pub subsystem_device_id: Option<u16>,
    pub interrupt_line: Option<u8>,
    pub interrupt_pin: Option<u8>,
    pub irq: PciIrqInfo,
    pub capabilities: Vec<PciCapability>,
    pub msi: Option<PciMsiInfo>,
    pub msix: Option<PciMsixInfo>,
    pub bridge: Option<PciBridgeInfo>,
}

impl PciDevice {
    /// Packed class/subclass/programming-interface value used by Linux match
    /// tables.
    pub fn class_triplet(&self) -> u32 {
        ((self.class_code as u32) << 16) | ((self.subclass as u32) << 8) | self.prog_if as u32
    }

    /// Linux-style modalias string for PCI driver matching.
    pub fn modalias(&self) -> alloc::string::String {
        format!(
            "pci:v{:04X}d{:04X}sv{:04X}sd{:04X}bc{:02X}sc{:02X}i{:02X}",
            self.vendor_id,
            self.device_id,
            self.subsystem_vendor_id.unwrap_or(PCI_ANY_ID),
            self.subsystem_device_id.unwrap_or(PCI_ANY_ID),
            self.class_code,
            self.subclass,
            self.prog_if
        )
    }

    /// Match this device against a Linux-style PCI device-id table entry.
    pub fn matches_id(&self, id: &PciDeviceId) -> bool {
        if id.vendor_id != PCI_ANY_ID && id.vendor_id != self.vendor_id {
            return false;
        }
        if id.device_id != PCI_ANY_ID && id.device_id != self.device_id {
            return false;
        }
        if id.subsystem_vendor_id != PCI_ANY_ID
            && Some(id.subsystem_vendor_id) != self.subsystem_vendor_id
        {
            return false;
        }
        if id.subsystem_device_id != PCI_ANY_ID
            && Some(id.subsystem_device_id) != self.subsystem_device_id
        {
            return false;
        }
        id.class_mask == 0 || ((self.class_triplet() ^ id.class) & id.class_mask) == 0
    }

    /// Match by base class/subclass/programming-interface; `None` wildcards a
    /// level, matching the common Linux helper use cases.
    pub fn matches_class(&self, class: u8, subclass: Option<u8>, prog_if: Option<u8>) -> bool {
        self.class_code == class
            && subclass.map_or(true, |s| self.subclass == s)
            && prog_if.map_or(true, |p| self.prog_if == p)
    }

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

    fn bar_count_for_header(header_type: u8) -> usize {
        match header_type & 0x7F {
            PCI_HEADER_TYPE_NORMAL => 6,
            PCI_HEADER_TYPE_BRIDGE => 2,
            _ => 0,
        }
    }

    fn decode_bar_resource(
        bar_index: u8,
        raw_low: u32,
        raw_high: u32,
        mask_low: u32,
        mask_high: u32,
    ) -> Option<PciResource> {
        if raw_low == 0xFFFF_FFFF {
            return None;
        }

        if (raw_low & 0x1) != 0 {
            let mask = (mask_low & PCI_BAR_IO_MASK) as u64;
            if mask == 0 {
                return None;
            }
            let size = ((!mask) & 0xFFFF_FFFF).wrapping_add(1);
            return Some(PciResource {
                bar_index,
                kind: PciBarKind::Io,
                base: (raw_low & PCI_BAR_IO_MASK) as u64,
                size,
                prefetchable: false,
                raw: raw_low as u64,
            });
        }

        let mem_type = (raw_low >> 1) & 0x3;
        let prefetchable = (raw_low & 0x8) != 0;
        let kind = match mem_type {
            0x1 => PciBarKind::MemoryBelow1M,
            0x2 => PciBarKind::Memory64,
            _ => PciBarKind::Memory32,
        };
        let raw = if kind == PciBarKind::Memory64 {
            ((raw_high as u64) << 32) | raw_low as u64
        } else {
            raw_low as u64
        };
        let base = if kind == PciBarKind::Memory64 {
            (((raw_high as u64) << 32) | ((raw_low & PCI_BAR_MEM_MASK) as u64))
                & 0xFFFF_FFFF_FFFF_FFF0
        } else {
            (raw_low & PCI_BAR_MEM_MASK) as u64
        };
        let mask = if kind == PciBarKind::Memory64 {
            ((mask_high as u64) << 32) | ((mask_low & PCI_BAR_MEM_MASK) as u64)
        } else {
            (mask_low & PCI_BAR_MEM_MASK) as u64
        };
        let width_mask = if kind == PciBarKind::Memory64 {
            u64::MAX
        } else {
            0xFFFF_FFFF
        };
        if mask == 0 {
            return None;
        }
        let size = ((!mask) & width_mask).wrapping_add(1);
        Some(PciResource {
            bar_index,
            kind,
            base,
            size,
            prefetchable,
            raw,
        })
    }

    /// Size BARs using the standard all-ones probe while temporarily disabling
    /// I/O and memory decode, matching the common Linux PCI resource flow.
    fn read_bar_resources(
        &self,
        address: PciAddress,
        header_type: u8,
    ) -> PciResult<[Option<PciResource>; 6]> {
        let saved_command = self.read_config_word(address, PCI_COMMAND)?;
        self.write_config_word(
            address,
            PCI_COMMAND,
            saved_command & !(PCI_COMMAND_IO | PCI_COMMAND_MEMORY),
        )?;

        let result: PciResult<[Option<PciResource>; 6]> = (|| {
            let mut resources = [None; 6];
            let bar_count = Self::bar_count_for_header(header_type);
            let mut i = 0usize;
            while i < bar_count {
                let offset = PCI_BAR0 + (i as u8 * 4);
                let raw_low = self.read_config_dword(address, offset)?;
                if raw_low == 0xFFFF_FFFF {
                    i += 1;
                    continue;
                }

                let raw_is_mem64 = (raw_low & 0x1) == 0 && ((raw_low >> 1) & 0x3) == 0x2;
                if raw_is_mem64 && i + 1 >= bar_count {
                    i += 1;
                    continue;
                }
                let is_mem64 = raw_is_mem64;
                let raw_high = if is_mem64 && i + 1 < bar_count {
                    self.read_config_dword(address, offset + 4)?
                } else {
                    0
                };

                self.write_config_dword(address, offset, 0xFFFF_FFFF)?;
                if is_mem64 && i + 1 < bar_count {
                    self.write_config_dword(address, offset + 4, 0xFFFF_FFFF)?;
                }
                let mask_low = self.read_config_dword(address, offset)?;
                let mask_high = if is_mem64 && i + 1 < bar_count {
                    self.read_config_dword(address, offset + 4)?
                } else {
                    0
                };
                if is_mem64 && i + 1 < bar_count {
                    self.write_config_dword(address, offset + 4, raw_high)?;
                }
                self.write_config_dword(address, offset, raw_low)?;

                resources[i] =
                    Self::decode_bar_resource(i as u8, raw_low, raw_high, mask_low, mask_high);
                i += if is_mem64 { 2 } else { 1 };
            }
            Ok(resources)
        })();

        let restore = self.write_config_word(address, PCI_COMMAND, saved_command);
        match (result, restore) {
            (Ok(resources), Ok(())) => Ok(resources),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn read_capabilities(
        &self,
        address: PciAddress,
        header_type: u8,
        status: u16,
    ) -> PciResult<Vec<PciCapability>> {
        let mut capabilities = Vec::new();
        if (status & PCI_STATUS_CAP_LIST) == 0 {
            return Ok(capabilities);
        }

        let cap_ptr_reg = match header_type & 0x7F {
            PCI_HEADER_TYPE_CARDBUS => 0x14,
            PCI_HEADER_TYPE_NORMAL | PCI_HEADER_TYPE_BRIDGE => PCI_CAPABILITY_LIST,
            _ => return Ok(capabilities),
        };
        let mut ptr = self.read_config_byte(address, cap_ptr_reg)? & !0x03;
        let mut seen = [false; 256];
        for _ in 0..48 {
            if ptr < 0x40 || seen[ptr as usize] {
                break;
            }
            seen[ptr as usize] = true;
            let id = self.read_config_byte(address, ptr)?;
            let next = self.read_config_byte(address, ptr.wrapping_add(1))? & !0x03;
            if id != 0 && id != 0xFF {
                capabilities.push(PciCapability {
                    id,
                    offset: ptr,
                    next,
                });
            }
            if next < 0x40 || next == ptr {
                break;
            }
            ptr = next;
        }
        Ok(capabilities)
    }

    fn parse_msi_capabilities(
        &self,
        address: PciAddress,
        capabilities: &[PciCapability],
    ) -> PciResult<(Option<PciMsiInfo>, Option<PciMsixInfo>)> {
        let mut msi = None;
        let mut msix = None;
        for cap in capabilities {
            match cap.id {
                PCI_CAP_ID_MSI => {
                    let Some(control_reg) = cap.offset.checked_add(2) else {
                        continue;
                    };
                    let control = self.read_config_word(address, control_reg)?;
                    msi = Some(PciMsiInfo {
                        capability_offset: cap.offset,
                        enabled: (control & 0x0001) != 0,
                        multiple_message_capable: 1u8 << (((control >> 1) & 0x7) as u32),
                        multiple_message_enabled: 1u8 << (((control >> 4) & 0x7) as u32),
                        address_64bit: (control & 0x0080) != 0,
                        per_vector_masking: (control & 0x0100) != 0,
                    });
                }
                PCI_CAP_ID_MSIX => {
                    let (Some(control_reg), Some(table_reg), Some(pba_reg)) = (
                        cap.offset.checked_add(2),
                        cap.offset.checked_add(4),
                        cap.offset.checked_add(8),
                    ) else {
                        continue;
                    };
                    let control = self.read_config_word(address, control_reg)?;
                    let table = self.read_config_dword(address, table_reg)?;
                    let pba = self.read_config_dword(address, pba_reg)?;
                    msix = Some(PciMsixInfo {
                        capability_offset: cap.offset,
                        enabled: (control & 0x8000) != 0,
                        function_masked: (control & 0x4000) != 0,
                        table_size: (control & 0x07FF) + 1,
                        table_bir: (table & 0x7) as u8,
                        table_offset: table & !0x7,
                        pba_bir: (pba & 0x7) as u8,
                        pba_offset: pba & !0x7,
                    });
                }
                _ => {}
            }
        }
        Ok((msi, msix))
    }

    fn read_bridge_info(
        &self,
        address: PciAddress,
        header_type: u8,
    ) -> PciResult<Option<PciBridgeInfo>> {
        if (header_type & 0x7F) != PCI_HEADER_TYPE_BRIDGE {
            return Ok(None);
        }

        let primary_bus = self.read_config_byte(address, PCI_BRIDGE_PRIMARY_BUS)?;
        let secondary_bus = self.read_config_byte(address, PCI_BRIDGE_SECONDARY_BUS)?;
        let subordinate_bus = self.read_config_byte(address, PCI_BRIDGE_SUBORDINATE_BUS)?;

        let io_base_low = self.read_config_byte(address, 0x1C)?;
        let io_limit_low = self.read_config_byte(address, 0x1D)?;
        let io_base_upper = if (io_base_low & 0x0F) == 0x01 {
            self.read_config_word(address, 0x30)? as u64
        } else {
            0
        };
        let io_limit_upper = if (io_limit_low & 0x0F) == 0x01 {
            self.read_config_word(address, 0x32)? as u64
        } else {
            0
        };
        let io_base = (io_base_upper << 16) | (((io_base_low & 0xF0) as u64) << 8);
        let io_limit = (io_limit_upper << 16) | (((io_limit_low & 0xF0) as u64) << 8) | 0x0FFF;
        let io_window = if io_base <= io_limit {
            Some(PciBridgeWindow {
                base: io_base,
                limit: io_limit,
            })
        } else {
            None
        };

        let mem_base_reg = self.read_config_word(address, 0x20)?;
        let mem_limit_reg = self.read_config_word(address, 0x22)?;
        let mem_base = ((mem_base_reg & 0xFFF0) as u64) << 16;
        let mem_limit = (((mem_limit_reg & 0xFFF0) as u64) << 16) | 0x000F_FFFF;
        let memory_window = if mem_base <= mem_limit {
            Some(PciBridgeWindow {
                base: mem_base,
                limit: mem_limit,
            })
        } else {
            None
        };

        let pref_base_reg = self.read_config_word(address, 0x24)?;
        let pref_limit_reg = self.read_config_word(address, 0x26)?;
        let pref_is_64 = (pref_base_reg & 0x000F) == 0x0001;
        let pref_base_upper = if pref_is_64 {
            self.read_config_dword(address, 0x28)? as u64
        } else {
            0
        };
        let pref_limit_upper = if pref_is_64 {
            self.read_config_dword(address, 0x2C)? as u64
        } else {
            0
        };
        let pref_base = (pref_base_upper << 32) | (((pref_base_reg & 0xFFF0) as u64) << 16);
        let pref_limit =
            (pref_limit_upper << 32) | (((pref_limit_reg & 0xFFF0) as u64) << 16) | 0x000F_FFFF;
        let prefetchable_memory_window = if pref_base <= pref_limit {
            Some(PciBridgeWindow {
                base: pref_base,
                limit: pref_limit,
            })
        } else {
            None
        };

        Ok(Some(PciBridgeInfo {
            primary_bus,
            secondary_bus,
            subordinate_bus,
            io_window,
            memory_window,
            prefetchable_memory_window,
        }))
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
        for i in 0..Self::bar_count_for_header(header_type) {
            bars[i] = self.read_config_dword(address, PCI_BAR0 + (i as u8 * 4))?;
        }
        let resources = self.read_bar_resources(address, header_type)?;

        // Read subsystem information (for header type 0)
        let (subsystem_vendor_id, subsystem_device_id) =
            if (header_type & 0x7F) == PCI_HEADER_TYPE_NORMAL {
                let sub_vendor = self.read_config_word(address, 0x2C).ok();
                let sub_device = self.read_config_word(address, 0x2E).ok();
                (sub_vendor, sub_device)
            } else {
                (None, None)
            };

        // Read interrupt and capability information
        let interrupt_line = self.read_config_byte(address, PCI_INTERRUPT_LINE).ok();
        let interrupt_pin = self.read_config_byte(address, PCI_INTERRUPT_PIN).ok();
        let capabilities = self.read_capabilities(address, header_type, status)?;
        let (msi, msix) = self.parse_msi_capabilities(address, &capabilities)?;
        let irq = PciIrqInfo {
            legacy_line: interrupt_line.filter(|line| *line != 0xFF),
            legacy_pin: interrupt_pin.filter(|pin| *pin != 0),
            legacy_enabled: interrupt_line.map_or(false, |line| line != 0xFF)
                && interrupt_pin.map_or(false, |pin| pin != 0)
                && (command & PCI_COMMAND_INTERRUPT_DISABLE) == 0,
            msi_capable: msi.is_some(),
            msi_enabled: msi.map_or(false, |info| info.enabled),
            msix_capable: msix.is_some(),
            msix_enabled: msix.map_or(false, |info| info.enabled),
        };
        let bridge = self.read_bridge_info(address, header_type)?;

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
            resources,
            subsystem_vendor_id,
            subsystem_device_id,
            interrupt_line,
            interrupt_pin,
            irq,
            capabilities,
            msi,
            msix,
            bridge,
        })
    }

    /// Scan PCI bus for devices
    pub fn scan_bus(&self) -> PciResult<usize> {
        let mut device_count = 0;
        let mut devices = self.devices.write();
        let mut visited_buses = [false; 256];

        // Production: PCI bus scan in progress

        // Prefer Linux-style bridge-guided traversal from bus 0, then fall back
        // to unvisited buses so firmware that left bridge numbers incomplete is
        // still detected during early boot.
        device_count += self.scan_bus_number(0, &mut devices, &mut visited_buses)?;
        for bus in 1..=255u16 {
            let bus = bus as u8;
            if !visited_buses[bus as usize] {
                device_count += self.scan_bus_number(bus, &mut devices, &mut visited_buses)?;
            }
        }

        *self.scan_complete.write() = true;
        // Production: PCI scan completed
        Ok(device_count)
    }

    fn scan_bus_number(
        &self,
        bus: u8,
        devices: &mut BTreeMap<PciAddress, PciDevice>,
        visited_buses: &mut [bool; 256],
    ) -> PciResult<usize> {
        if visited_buses[bus as usize] {
            return Ok(0);
        }
        visited_buses[bus as usize] = true;

        let mut count = 0;
        for device in 0..32u8 {
            let function0 = PciAddress::new(bus, device, 0);
            let Some(pci_function0) = self.scan_function(function0, devices)? else {
                continue;
            };
            count += 1;

            if let Some(bridge) = pci_function0.bridge {
                if bridge.secondary_bus != 0
                    && bridge.secondary_bus != bus
                    && bridge.secondary_bus <= bridge.subordinate_bus
                {
                    count += self.scan_bus_number(bridge.secondary_bus, devices, visited_buses)?;
                }
            }

            if pci_function0.is_multifunction() {
                for function in 1..8u8 {
                    let address = PciAddress::new(bus, device, function);
                    if let Some(pci_device) = self.scan_function(address, devices)? {
                        if let Some(bridge) = pci_device.bridge {
                            if bridge.secondary_bus != 0
                                && bridge.secondary_bus != bus
                                && bridge.secondary_bus <= bridge.subordinate_bus
                            {
                                count += self.scan_bus_number(
                                    bridge.secondary_bus,
                                    devices,
                                    visited_buses,
                                )?;
                            }
                        }
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }

    fn scan_function(
        &self,
        address: PciAddress,
        devices: &mut BTreeMap<PciAddress, PciDevice>,
    ) -> PciResult<Option<PciDevice>> {
        match self.read_device_config(address) {
            Ok(pci_device) => {
                let is_new = !devices.contains_key(&address);
                if is_new {
                    let device_info = pci_device.to_device_info();
                    if let Err(_e) = add_device(device_info) {
                        // Production: hot-plug registration issue
                    }
                }
                devices.insert(address, pci_device.clone());
                Ok(Some(pci_device))
            }
            Err(PciError::DeviceNotFound) => Ok(None),
            Err(err) => Err(err),
        }
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
    PCI_BUS.init()?;

    // Publish enumerated PCI devices into the unified `base` model
    // (additive; tolerant of a missing bus and never fatal to PCI init).
    publish_to_base();

    Ok(())
}

/// Register enumerated PCI devices into the unified `base` device model.
fn publish_to_base() {
    use crate::drivers::base;
    let devices = PCI_BUS.list_devices();

    for d in devices {
        let name = format!(
            "pci-{:02x}:{:02x}.{}",
            d.address.bus, d.address.device, d.address.function
        );
        if base::device_exists(&name) {
            continue;
        }
        if let Ok(id) = base::register_device_simple("pci", &name, &d.modalias()) {
            let _ = base::set_property(id, "vendor_id", &format!("0x{:04x}", d.vendor_id));
            let _ = base::set_property(id, "device_id", &format!("0x{:04x}", d.device_id));
            let _ = base::set_property(id, "class", &format!("0x{:06x}", d.class_triplet()));
            let _ = base::set_property(id, "class_name", d.get_class_name());
            let _ = base::set_property(id, "irq_legacy", &format!("{}", d.irq.legacy_enabled));
            let _ = base::set_property(id, "msi_capable", &format!("{}", d.irq.msi_capable));
            let _ = base::set_property(id, "msix_capable", &format!("{}", d.irq.msix_capable));
            let mut resource_count = 0usize;
            for resource in d.resources.iter().flatten() {
                let key = format!("bar{}_resource", resource.bar_index);
                let value = format!(
                    "{:?}:base=0x{:x},size=0x{:x},prefetchable={}",
                    resource.kind, resource.base, resource.size, resource.prefetchable
                );
                let _ = base::set_property(id, &key, &value);
                resource_count += 1;
            }
            let _ = base::set_property(id, "resource_count", &format!("{}", resource_count));
        }
    }
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

/// Find the first device matching a Linux-style PCI id table entry.
pub fn find_matching_device(id: &PciDeviceId) -> Option<PciDevice> {
    PCI_BUS
        .list_devices()
        .into_iter()
        .find(|device| device.matches_id(id))
}

/// Return all devices matching a Linux-style PCI id table entry.
pub fn get_devices_by_id(id: &PciDeviceId) -> Vec<PciDevice> {
    PCI_BUS
        .list_devices()
        .into_iter()
        .filter(|device| device.matches_id(id))
        .collect()
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
