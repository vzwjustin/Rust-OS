//! PCI Configuration Space Management
//!
//! This module provides comprehensive PCI configuration space access,
//! BAR management, resource allocation, and power management capabilities.

use crate::pci::{PciBusScanner, PciDevice};
use alloc::vec::Vec;
use core::fmt;

/// PCI Configuration Registers
pub const PCI_VENDOR_ID: u8 = 0x00;
pub const PCI_DEVICE_ID: u8 = 0x02;
pub const PCI_COMMAND: u8 = 0x04;
pub const PCI_STATUS: u8 = 0x06;
pub const PCI_REVISION_ID: u8 = 0x08;
pub const PCI_PROG_IF: u8 = 0x09;
pub const PCI_SUBCLASS: u8 = 0x0A;
pub const PCI_CLASS: u8 = 0x0B;
pub const PCI_CACHE_LINE_SIZE: u8 = 0x0C;
pub const PCI_LATENCY_TIMER: u8 = 0x0D;
pub const PCI_HEADER_TYPE: u8 = 0x0E;
pub const PCI_BIST: u8 = 0x0F;
pub const PCI_BAR0: u8 = 0x10;
pub const PCI_BAR1: u8 = 0x14;
pub const PCI_BAR2: u8 = 0x18;
pub const PCI_BAR3: u8 = 0x1C;
pub const PCI_BAR4: u8 = 0x20;
pub const PCI_BAR5: u8 = 0x24;
pub const PCI_CARDBUS_CIS: u8 = 0x28;
pub const PCI_SUBSYSTEM_VENDOR_ID: u8 = 0x2C;
pub const PCI_SUBSYSTEM_ID: u8 = 0x2E;
pub const PCI_EXPANSION_ROM: u8 = 0x30;
pub const PCI_CAPABILITIES_PTR: u8 = 0x34;
pub const PCI_INTERRUPT_LINE: u8 = 0x3C;
pub const PCI_INTERRUPT_PIN: u8 = 0x3D;
pub const PCI_MIN_GNT: u8 = 0x3E;
pub const PCI_MAX_LAT: u8 = 0x3F;

/// PCI Command Register bits
pub const PCI_COMMAND_IO: u16 = 0x0001;
pub const PCI_COMMAND_MEMORY: u16 = 0x0002;
pub const PCI_COMMAND_MASTER: u16 = 0x0004;
pub const PCI_COMMAND_SPECIAL: u16 = 0x0008;
pub const PCI_COMMAND_INVALIDATE: u16 = 0x0010;
pub const PCI_COMMAND_VGA_PALETTE: u16 = 0x0020;
pub const PCI_COMMAND_PARITY: u16 = 0x0040;
pub const PCI_COMMAND_WAIT: u16 = 0x0080;
pub const PCI_COMMAND_SERR: u16 = 0x0100;
pub const PCI_COMMAND_FAST_BACK: u16 = 0x0200;
pub const PCI_COMMAND_INTX_DISABLE: u16 = 0x0400;

/// PCI Status Register bits
pub const PCI_STATUS_INTERRUPT: u16 = 0x0008;
pub const PCI_STATUS_CAP_LIST: u16 = 0x0010;
pub const PCI_STATUS_66MHZ: u16 = 0x0020;
pub const PCI_STATUS_UDF: u16 = 0x0040;
pub const PCI_STATUS_FAST_BACK: u16 = 0x0080;
pub const PCI_STATUS_PARITY: u16 = 0x0100;

/// Base Address Register (BAR) types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarType {
    Memory32,
    Memory64,
    Io,
    Unused,
}

/// Base Address Register information
#[derive(Debug, Clone)]
pub struct BarInfo {
    pub bar_type: BarType,
    pub base_address: u64,
    pub size: u64,
    pub prefetchable: bool,
    pub raw_value: u32,
}

impl BarInfo {
    /// Create a new BAR info from raw value
    pub fn from_raw(raw_value: u32) -> Self {
        if raw_value == 0 {
            return Self {
                bar_type: BarType::Unused,
                base_address: 0,
                size: 0,
                prefetchable: false,
                raw_value,
            };
        }

        if (raw_value & 1) == 1 {
            // I/O BAR
            Self {
                bar_type: BarType::Io,
                base_address: (raw_value & 0xFFFFFFFC) as u64,
                size: 0, // Size needs to be determined separately
                prefetchable: false,
                raw_value,
            }
        } else {
            // Memory BAR
            let mem_type = (raw_value >> 1) & 0x3;
            let prefetchable = (raw_value & 0x8) != 0;
            let base_address = (raw_value & 0xFFFFFFF0) as u64;

            match mem_type {
                0 => Self {
                    bar_type: BarType::Memory32,
                    base_address,
                    size: 0, // Size needs to be determined separately
                    prefetchable,
                    raw_value,
                },
                2 => Self {
                    bar_type: BarType::Memory64,
                    base_address,
                    size: 0, // Size needs to be determined separately
                    prefetchable,
                    raw_value,
                },
                _ => Self {
                    bar_type: BarType::Unused,
                    base_address: 0,
                    size: 0,
                    prefetchable: false,
                    raw_value,
                },
            }
        }
    }

    /// Check if this BAR is active/valid
    pub fn is_active(&self) -> bool {
        !matches!(self.bar_type, BarType::Unused) && self.base_address != 0
    }

    /// Check if this BAR is for memory-mapped I/O
    pub fn is_memory(&self) -> bool {
        matches!(self.bar_type, BarType::Memory32 | BarType::Memory64)
    }

    /// Check if this BAR is for I/O ports
    pub fn is_io(&self) -> bool {
        matches!(self.bar_type, BarType::Io)
    }
}

impl fmt::Display for BarInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.bar_type {
            BarType::Unused => write!(f, "Unused"),
            BarType::Io => write!(
                f,
                "I/O at 0x{:08x} (size: 0x{:x})",
                self.base_address, self.size
            ),
            BarType::Memory32 => write!(
                f,
                "Memory32 at 0x{:08x} (size: 0x{:x}){}",
                self.base_address,
                self.size,
                if self.prefetchable {
                    " [prefetchable]"
                } else {
                    ""
                }
            ),
            BarType::Memory64 => write!(
                f,
                "Memory64 at 0x{:016x} (size: 0x{:x}){}",
                self.base_address,
                self.size,
                if self.prefetchable {
                    " [prefetchable]"
                } else {
                    ""
                }
            ),
        }
    }
}

/// PCI Device Power States
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerState {
    D0 = 0,     // Fully On
    D1 = 1,     // Low Power
    D2 = 2,     // Lower Power
    D3Hot = 3,  // Off, but PCI config space accessible
    D3Cold = 4, // Off, PCI config space not accessible
}

impl fmt::Display for PowerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            PowerState::D0 => "D0 (Fully On)",
            PowerState::D1 => "D1 (Low Power)",
            PowerState::D2 => "D2 (Lower Power)",
            PowerState::D3Hot => "D3Hot (Off, Config Accessible)",
            PowerState::D3Cold => "D3Cold (Off, Config Not Accessible)",
        };
        write!(f, "{}", name)
    }
}

/// MSI (Message Signaled Interrupt) Configuration
#[derive(Debug, Clone)]
pub struct MsiConfig {
    pub enabled: bool,
    pub multiple_message_capable: u8,
    pub multiple_message_enable: u8,
    pub address: u64,
    pub data: u16,
    pub mask_bits: u32,
    pub pending_bits: u32,
}

impl Default for MsiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            multiple_message_capable: 0,
            multiple_message_enable: 0,
            address: 0,
            data: 0,
            mask_bits: 0,
            pending_bits: 0,
        }
    }
}

/// MSI-X Configuration
#[derive(Debug, Clone)]
pub struct MsiXConfig {
    pub enabled: bool,
    pub function_mask: bool,
    pub table_size: u16,
    pub table_offset: u32,
    pub table_bar: u8,
    pub pending_offset: u32,
    pub pending_bar: u8,
}

impl Default for MsiXConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            function_mask: false,
            table_size: 0,
            table_offset: 0,
            table_bar: 0,
            pending_offset: 0,
            pending_bar: 0,
        }
    }
}

/// PCI Configuration Space Manager
pub struct PciConfigManager {
    scanner: *const PciBusScanner,
}

impl PciConfigManager {
    /// Create a new PCI configuration manager
    pub fn new(scanner: &PciBusScanner) -> Self {
        Self {
            scanner: scanner as *const PciBusScanner,
        }
    }

    /// Get the scanner reference
    fn scanner(&self) -> &PciBusScanner {
        unsafe { &*self.scanner }
    }

    /// Read device BARs and determine their sizes
    pub fn read_bars(&self, device: &PciDevice) -> Vec<BarInfo> {
        let mut bars = Vec::new();
        let mut i = 0;

        while i < 6 {
            let bar_offset = PCI_BAR0 + (i * 4) as u8;
            let original_value = self.scanner().read_config_dword(
                device.bus,
                device.device,
                device.function,
                bar_offset,
            );

            let mut bar_info = BarInfo::from_raw(original_value);

            if bar_info.bar_type != BarType::Unused {
                // Determine BAR size by writing all 1s and reading back
                self.scanner().write_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    bar_offset,
                    0xFFFFFFFF,
                );
                let size_mask = self.scanner().read_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    bar_offset,
                );

                // Restore original value
                self.scanner().write_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    bar_offset,
                    original_value,
                );

                // Calculate size
                let size_bits = if bar_info.is_io() {
                    let bits = size_mask & 0xFFFFFFFC;
                    bar_info.size = if bits != 0 { (!(bits) + 1) as u64 } else { 0 };
                    bits
                } else {
                    let bits = size_mask & 0xFFFFFFF0;
                    bar_info.size = if bits != 0 { (!(bits) + 1) as u64 } else { 0 };
                    bits
                };

                // Handle 64-bit BARs
                if bar_info.bar_type == BarType::Memory64 && i < 5 {
                    let next_bar_offset = PCI_BAR0 + ((i + 1) * 4) as u8;
                    let upper_value = self.scanner().read_config_dword(
                        device.bus,
                        device.device,
                        device.function,
                        next_bar_offset,
                    );
                    bar_info.base_address |= (upper_value as u64) << 32;

                    // Determine upper 32-bit size
                    self.scanner().write_config_dword(
                        device.bus,
                        device.device,
                        device.function,
                        next_bar_offset,
                        0xFFFFFFFF,
                    );
                    let upper_size_mask = self.scanner().read_config_dword(
                        device.bus,
                        device.device,
                        device.function,
                        next_bar_offset,
                    );
                    self.scanner().write_config_dword(
                        device.bus,
                        device.device,
                        device.function,
                        next_bar_offset,
                        upper_value,
                    );

                    if upper_size_mask != 0 {
                        let full_size_mask = ((upper_size_mask as u64) << 32) | (size_bits as u64);
                        bar_info.size = (!full_size_mask) + 1;
                    }

                    bars.push(bar_info);
                    // Add placeholder for upper 32 bits
                    bars.push(BarInfo {
                        bar_type: BarType::Unused,
                        base_address: 0,
                        size: 0,
                        prefetchable: false,
                        raw_value: upper_value,
                    });
                    i += 2;
                } else {
                    bars.push(bar_info);
                    i += 1;
                }
            } else {
                bars.push(bar_info);
                i += 1;
            }
        }

        bars
    }

    /// Enable or disable I/O space access
    pub fn set_io_enable(&self, device: &PciDevice, enable: bool) -> Result<(), &'static str> {
        let mut command = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
        );

        if enable {
            command |= PCI_COMMAND_IO;
        } else {
            command &= !PCI_COMMAND_IO;
        }

        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
            command,
        );
        Ok(())
    }

    /// Enable or disable memory space access
    pub fn set_memory_enable(&self, device: &PciDevice, enable: bool) -> Result<(), &'static str> {
        let mut command = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
        );

        if enable {
            command |= PCI_COMMAND_MEMORY;
        } else {
            command &= !PCI_COMMAND_MEMORY;
        }

        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
            command,
        );
        Ok(())
    }

    /// Enable or disable bus mastering
    pub fn set_bus_master_enable(
        &self,
        device: &PciDevice,
        enable: bool,
    ) -> Result<(), &'static str> {
        let mut command = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
        );

        if enable {
            command |= PCI_COMMAND_MASTER;
        } else {
            command &= !PCI_COMMAND_MASTER;
        }

        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            PCI_COMMAND,
            command,
        );
        Ok(())
    }

    /// Get current command register value
    pub fn get_command(&self, device: &PciDevice) -> u16 {
        self.scanner()
            .read_config_word(device.bus, device.device, device.function, PCI_COMMAND)
    }

    /// Get current status register value
    pub fn get_status(&self, device: &PciDevice) -> u16 {
        self.scanner()
            .read_config_word(device.bus, device.device, device.function, PCI_STATUS)
    }

    /// Set interrupt line
    pub fn set_interrupt_line(&self, device: &PciDevice, irq: u8) -> Result<(), &'static str> {
        self.scanner().write_config_byte(
            device.bus,
            device.device,
            device.function,
            PCI_INTERRUPT_LINE,
            irq,
        );
        Ok(())
    }

    /// Get interrupt line
    pub fn get_interrupt_line(&self, device: &PciDevice) -> u8 {
        self.scanner().read_config_byte(
            device.bus,
            device.device,
            device.function,
            PCI_INTERRUPT_LINE,
        )
    }

    /// Get interrupt pin
    pub fn get_interrupt_pin(&self, device: &PciDevice) -> u8 {
        self.scanner().read_config_byte(
            device.bus,
            device.device,
            device.function,
            PCI_INTERRUPT_PIN,
        )
    }

    /// Read MSI capability if present
    pub fn read_msi_capability(&self, device: &PciDevice) -> Option<MsiConfig> {
        if !device.capabilities.msi {
            return None;
        }

        let cap_ptr = self.find_capability(device, 0x05)?;
        let mut msi_config = MsiConfig::default();

        // Read MSI Control Register
        let control = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
        );
        msi_config.enabled = (control & 0x01) != 0;
        msi_config.multiple_message_capable = ((control >> 1) & 0x07) as u8;
        msi_config.multiple_message_enable = ((control >> 4) & 0x07) as u8;

        // Read Message Address
        let addr_low = self.scanner().read_config_dword(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
        );

        if (control & 0x80) != 0 {
            // 64-bit address
            let addr_high = self.scanner().read_config_dword(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 8,
            );
            msi_config.address = ((addr_high as u64) << 32) | (addr_low as u64);
            msi_config.data = self.scanner().read_config_word(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 12,
            );

            if (control & 0x100) != 0 {
                // Per-vector masking capable
                msi_config.mask_bits = self.scanner().read_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    cap_ptr + 16,
                );
                msi_config.pending_bits = self.scanner().read_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    cap_ptr + 20,
                );
            }
        } else {
            // 32-bit address
            msi_config.address = addr_low as u64;
            msi_config.data = self.scanner().read_config_word(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 8,
            );

            if (control & 0x100) != 0 {
                // Per-vector masking capable
                msi_config.mask_bits = self.scanner().read_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    cap_ptr + 12,
                );
                msi_config.pending_bits = self.scanner().read_config_dword(
                    device.bus,
                    device.device,
                    device.function,
                    cap_ptr + 16,
                );
            }
        }

        Some(msi_config)
    }

    /// Read MSI-X capability if present
    pub fn read_msi_x_capability(&self, device: &PciDevice) -> Option<MsiXConfig> {
        if !device.capabilities.msi_x {
            return None;
        }

        let cap_ptr = self.find_capability(device, 0x11)?;
        let mut msi_x_config = MsiXConfig::default();

        // Read MSI-X Control Register
        let control = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
        );
        msi_x_config.enabled = (control & 0x8000) != 0;
        msi_x_config.function_mask = (control & 0x4000) != 0;
        msi_x_config.table_size = (control & 0x07FF) + 1;

        // Read Table Offset/BIR
        let table_reg = self.scanner().read_config_dword(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
        );
        msi_x_config.table_bar = (table_reg & 0x07) as u8;
        msi_x_config.table_offset = table_reg & 0xFFFFFFF8;

        // Read Pending Offset/BIR
        let pending_reg = self.scanner().read_config_dword(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 8,
        );
        msi_x_config.pending_bar = (pending_reg & 0x07) as u8;
        msi_x_config.pending_offset = pending_reg & 0xFFFFFFF8;

        Some(msi_x_config)
    }

    /// Find a specific capability by ID
    pub fn find_capability(&self, device: &PciDevice, cap_id: u8) -> Option<u8> {
        // Check if device has capabilities
        let status =
            self.scanner()
                .read_config_word(device.bus, device.device, device.function, PCI_STATUS);
        if (status & PCI_STATUS_CAP_LIST) == 0 {
            return None;
        }

        let mut cap_ptr = self.scanner().read_config_byte(
            device.bus,
            device.device,
            device.function,
            PCI_CAPABILITIES_PTR,
        ) & 0xFC;

        // PCI config space is 256 bytes and capabilities are at least 4 bytes apart, so
        // there can be at most 48 of them. Cap the iterations so a cyclic (malformed)
        // capability-pointer chain cannot spin forever.
        let mut iterations = 0;
        while cap_ptr != 0 && cap_ptr != 0xFF {
            if iterations >= 48 {
                return None;
            }
            iterations += 1;

            let current_cap_id = self.scanner().read_config_byte(
                device.bus,
                device.device,
                device.function,
                cap_ptr,
            );
            if current_cap_id == cap_id {
                return Some(cap_ptr);
            }
            cap_ptr = self.scanner().read_config_byte(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 1,
            ) & 0xFC;
        }

        None
    }

    /// Get power management capability and current power state
    pub fn get_power_state(&self, device: &PciDevice) -> Option<PowerState> {
        if !device.capabilities.power_management {
            return None;
        }

        let cap_ptr = self.find_capability(device, 0x01)?;
        let pmcsr = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
        );
        let power_state = pmcsr & 0x03;

        match power_state {
            0 => Some(PowerState::D0),
            1 => Some(PowerState::D1),
            2 => Some(PowerState::D2),
            3 => Some(PowerState::D3Hot),
            _ => None,
        }
    }

    /// Set device power state
    pub fn set_power_state(
        &self,
        device: &PciDevice,
        state: PowerState,
    ) -> Result<(), &'static str> {
        if !device.capabilities.power_management {
            return Err("Device does not support power management");
        }

        let cap_ptr = self
            .find_capability(device, 0x01)
            .ok_or("Power management capability not found")?;
        let mut pmcsr = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
        );

        // Clear current power state bits
        pmcsr &= !0x03;
        // Set new power state
        pmcsr |= state as u16;

        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
            pmcsr,
        );
        Ok(())
    }

    /// Enable MSI interrupts
    pub fn enable_msi(
        &self,
        device: &PciDevice,
        address: u64,
        data: u16,
    ) -> Result<(), &'static str> {
        if !device.capabilities.msi {
            return Err("Device does not support MSI");
        }

        let cap_ptr = self
            .find_capability(device, 0x05)
            .ok_or("MSI capability not found")?;

        // Read control register to check address size
        let control = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
        );

        // Write address
        self.scanner().write_config_dword(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 4,
            address as u32,
        );

        if (control & 0x80) != 0 {
            // 64-bit address
            self.scanner().write_config_dword(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 8,
                (address >> 32) as u32,
            );
            self.scanner().write_config_word(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 12,
                data,
            );
        } else {
            // 32-bit address
            self.scanner().write_config_word(
                device.bus,
                device.device,
                device.function,
                cap_ptr + 8,
                data,
            );
        }

        // Enable MSI
        let new_control = control | 0x01;
        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
            new_control,
        );

        Ok(())
    }

    /// Disable MSI interrupts
    pub fn disable_msi(&self, device: &PciDevice) -> Result<(), &'static str> {
        if !device.capabilities.msi {
            return Err("Device does not support MSI");
        }

        let cap_ptr = self
            .find_capability(device, 0x05)
            .ok_or("MSI capability not found")?;

        // Disable MSI
        let control = self.scanner().read_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
        );
        let new_control = control & !0x01;
        self.scanner().write_config_word(
            device.bus,
            device.device,
            device.function,
            cap_ptr + 2,
            new_control,
        );

        Ok(())
    }

    /// Production device configuration - silent validation only
    pub fn print_device_config(&self, device: &PciDevice) {
        // Production: only report critical configuration problems
        let command = self.get_command(device);
        let status = self.get_status(device);

        // Report critical errors only
        if (status & 0xF900) != 0 {
            // Error bits
            crate::println!("PCI {} error: status 0x{:04x}", device.location(), status);
        }

        // Report if critical device is disabled
        let class_val = device.class_code as u8;
        if (class_val == 0x03 || class_val == 0x02) && // VGA or Network
           (command & (PCI_COMMAND_IO | PCI_COMMAND_MEMORY)) == 0
        {
            crate::println!("Critical device {} disabled", device.location());
        }
    }
}
