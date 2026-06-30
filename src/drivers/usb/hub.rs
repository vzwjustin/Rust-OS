//! Root-hub enumeration.
//!
//! Walks a `HostController`'s root-hub ports and brings each attached device
//! to the configured state following the standard USB sequence: port reset →
//! enable slot → `SET_ADDRESS` → `GET_DESCRIPTOR` (device, then configuration
//! with its interfaces and endpoints) → `SET_CONFIGURATION`. The parsed
//! descriptors are returned so the class binder can match drivers.

use alloc::vec;
use alloc::vec::Vec;

use super::descriptor::{
    parse_configuration, ConfigurationDescriptor, DeviceDescriptor, ParsedConfiguration,
};
use super::hcd::{
    HostController, SetupPacket, DESC_CONFIGURATION, DESC_DEVICE, REQ_GET_DESCRIPTOR,
    REQ_SET_ADDRESS, REQ_SET_CONFIGURATION,
};

/// A device that has been fully enumerated on a port.
#[derive(Debug, Clone)]
pub struct EnumeratedDevice {
    pub port: u8,
    pub slot: u8,
    pub address: u8,
    pub device: DeviceDescriptor,
    pub config: ParsedConfiguration,
}

fn get_descriptor(
    hc: &mut dyn HostController,
    slot: u8,
    desc_type: u8,
    index: u8,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    let setup = SetupPacket {
        request_type: 0x80, // device-to-host, standard, device
        request: REQ_GET_DESCRIPTOR,
        value: ((desc_type as u16) << 8) | index as u16,
        index: 0,
        length: buf.len() as u16,
    };
    let res = hc.control_transfer(slot, setup, Some(buf))?;
    if !res.is_ok() {
        return Err("usb: GET_DESCRIPTOR failed");
    }
    Ok(res.transferred)
}

/// Enumerate the single device (if any) on `port`. Returns `Ok(None)` for an
/// empty port.
pub fn enumerate_port(
    hc: &mut dyn HostController,
    port: u8,
    address: u8,
) -> Result<Option<EnumeratedDevice>, &'static str> {
    let status = hc.port_status(port)?;
    if !status.connected {
        return Ok(None);
    }

    // 1. Reset the port and allocate a device slot.
    hc.reset_port(port)?;
    let slot = hc.enable_slot(port)?;

    // 2. Read the first 8 bytes of the device descriptor to learn EP0 size.
    let mut dev_hdr = [0u8; 8];
    get_descriptor(hc, slot, DESC_DEVICE, 0, &mut dev_hdr)?;

    // 3. Assign the device address.
    let set_addr = SetupPacket {
        request_type: 0x00,
        request: REQ_SET_ADDRESS,
        value: address as u16,
        index: 0,
        length: 0,
    };
    let res = hc.control_transfer(slot, set_addr, None)?;
    if !res.is_ok() {
        return Err("usb: SET_ADDRESS failed");
    }

    // 4. Read the full device descriptor.
    let mut dev_buf = [0u8; DeviceDescriptor::SIZE];
    get_descriptor(hc, slot, DESC_DEVICE, 0, &mut dev_buf)?;
    let device = DeviceDescriptor::parse(&dev_buf)?;

    // 5. Read the configuration descriptor header, then the whole blob.
    let mut cfg_hdr = [0u8; ConfigurationDescriptor::SIZE];
    get_descriptor(hc, slot, DESC_CONFIGURATION, 0, &mut cfg_hdr)?;
    let cfg = ConfigurationDescriptor::parse(&cfg_hdr)?;
    let total = cfg.total_length.max(ConfigurationDescriptor::SIZE as u16) as usize;
    let mut cfg_buf = vec![0u8; total];
    get_descriptor(hc, slot, DESC_CONFIGURATION, 0, &mut cfg_buf)?;
    let config = parse_configuration(&cfg_buf)?;

    // 6. Select the configuration.
    let set_cfg = SetupPacket {
        request_type: 0x00,
        request: REQ_SET_CONFIGURATION,
        value: config.descriptor.configuration_value as u16,
        index: 0,
        length: 0,
    };
    let res = hc.control_transfer(slot, set_cfg, None)?;
    if !res.is_ok() {
        return Err("usb: SET_CONFIGURATION failed");
    }

    Ok(Some(EnumeratedDevice {
        port,
        slot,
        address,
        device,
        config,
    }))
}

/// Enumerate every connected port on the controller, assigning sequential
/// addresses starting at 1.
pub fn enumerate_all(hc: &mut dyn HostController) -> Vec<EnumeratedDevice> {
    let mut devices = Vec::new();
    let mut address = 1u8;
    for port in 1..=hc.port_count() {
        match enumerate_port(hc, port, address) {
            Ok(Some(dev)) => {
                crate::serial_println!(
                    "usb: enumerated port={} slot={} addr={} {:04x}:{:04x} class={}",
                    dev.port,
                    dev.slot,
                    dev.address,
                    dev.device.vendor_id,
                    dev.device.product_id,
                    dev.config
                        .interfaces
                        .first()
                        .map(|i| i.descriptor.interface_class)
                        .unwrap_or(0)
                );
                devices.push(dev);
                address += 1;
            }
            Ok(None) => {}
            Err(e) => crate::serial_println!("usb: enumeration failed on port {}: {}", port, e),
        }
    }
    devices
}
