//! Simple PCI scanning for storage device detection
//!
//! This module provides basic PCI device scanning functionality
//! specifically for storage device detection.

use alloc::vec::Vec;
use core::arch::asm;

/// PCI device information
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
}

/// PCI configuration space offsets
const PCI_VENDOR_ID: u8 = 0x00;
const PCI_DEVICE_ID: u8 = 0x02;
const PCI_CLASS_CODE: u8 = 0x0B;
const PCI_SUBCLASS: u8 = 0x0A;
const PCI_PROG_IF: u8 = 0x09;
const PCI_BAR0: u8 = 0x10;
const PCI_BAR1: u8 = 0x14;
const PCI_BAR2: u8 = 0x18;
const PCI_BAR3: u8 = 0x1C;
const PCI_BAR4: u8 = 0x20;
const PCI_BAR5: u8 = 0x24;

/// Read from PCI configuration space
fn pci_config_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset & 0xFC) as u32);

    unsafe {
        // Write address to CONFIG_ADDRESS (0xCF8)
        asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") address, options(nomem, nostack, preserves_flags));

        // Read data from CONFIG_DATA (0xCFC)
        let mut data: u32;
        asm!("in eax, dx", out("eax") data, in("dx") 0xCFCu16, options(nomem, nostack, preserves_flags));
        data
    }
}

/// Read 16-bit value from PCI configuration space
fn pci_config_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let data = pci_config_read_u32(bus, device, function, offset & 0xFC);
    ((data >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// Read 8-bit value from PCI configuration space
fn pci_config_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let data = pci_config_read_u32(bus, device, function, offset & 0xFC);
    ((data >> ((offset & 3) * 8)) & 0xFF) as u8
}

/// Check if a PCI device exists
fn pci_device_exists(bus: u8, device: u8, function: u8) -> bool {
    let vendor_id = pci_config_read_u16(bus, device, function, PCI_VENDOR_ID);
    vendor_id != 0xFFFF
}

/// Read PCI device information
fn read_pci_device(bus: u8, device: u8, function: u8) -> Option<PciDevice> {
    if !pci_device_exists(bus, device, function) {
        return None;
    }

    let vendor_id = pci_config_read_u16(bus, device, function, PCI_VENDOR_ID);
    let device_id = pci_config_read_u16(bus, device, function, PCI_DEVICE_ID);
    let class_code = pci_config_read_u8(bus, device, function, PCI_CLASS_CODE);
    let subclass = pci_config_read_u8(bus, device, function, PCI_SUBCLASS);
    let prog_if = pci_config_read_u8(bus, device, function, PCI_PROG_IF);

    let bar0 = pci_config_read_u32(bus, device, function, PCI_BAR0);
    let bar1 = pci_config_read_u32(bus, device, function, PCI_BAR1);
    let bar2 = pci_config_read_u32(bus, device, function, PCI_BAR2);
    let bar3 = pci_config_read_u32(bus, device, function, PCI_BAR3);
    let bar4 = pci_config_read_u32(bus, device, function, PCI_BAR4);
    let bar5 = pci_config_read_u32(bus, device, function, PCI_BAR5);

    Some(PciDevice {
        bus,
        device,
        function,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        bar0,
        bar1,
        bar2,
        bar3,
        bar4,
        bar5,
    })
}

/// Scan all PCI devices
pub fn scan_pci_devices() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    // Scan all possible PCI locations
    for bus in 0..256 {
        for device in 0..32 {
            for function in 0..8 {
                if let Some(pci_device) = read_pci_device(bus as u8, device as u8, function as u8) {
                    devices.push(pci_device);

                    // If this is function 0 and it's not a multi-function device,
                    // skip the other functions
                    if function == 0 {
                        let header_type = pci_config_read_u8(bus as u8, device as u8, 0, 0x0E);
                        if (header_type & 0x80) == 0 {
                            break; // Not multi-function
                        }
                    }
                }
            }
        }
    }

    devices
}
