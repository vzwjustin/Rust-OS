//! Standard USB descriptors and parsers (USB 2.0 §9.6).
//!
//! Pure data: structures plus byte (de)serialisation. Enumeration in
//! `super::hub` fills these from `GET_DESCRIPTOR` control transfers and the
//! virtual devices in `super::device` emit them from the same definitions.

use alloc::vec::Vec;

/// USB device class codes used by the stack.
pub mod class {
    pub const PER_INTERFACE: u8 = 0x00;
    pub const HID: u8 = 0x03;
    pub const MASS_STORAGE: u8 = 0x08;
    pub const HUB: u8 = 0x09;
}

/// 18-byte device descriptor.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub usb_version: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_index: u8,
    pub num_configurations: u8,
}

impl DeviceDescriptor {
    pub const SIZE: usize = 18;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.length;
        b[1] = self.descriptor_type;
        b[2..4].copy_from_slice(&self.usb_version.to_le_bytes());
        b[4] = self.device_class;
        b[5] = self.device_subclass;
        b[6] = self.device_protocol;
        b[7] = self.max_packet_size0;
        b[8..10].copy_from_slice(&self.vendor_id.to_le_bytes());
        b[10..12].copy_from_slice(&self.product_id.to_le_bytes());
        b[12..14].copy_from_slice(&self.device_version.to_le_bytes());
        b[14] = self.manufacturer_index;
        b[15] = self.product_index;
        b[16] = self.serial_index;
        b[17] = self.num_configurations;
        b
    }

    pub fn parse(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() < Self::SIZE {
            return Err("device descriptor too short");
        }
        if b[0] as usize != Self::SIZE || b[1] != super::hcd::DESC_DEVICE {
            return Err("malformed device descriptor header");
        }
        let max_packet = b[7];
        if !matches!(max_packet, 8 | 16 | 32 | 64) {
            return Err("invalid ep0 max packet size");
        }
        if b[17] == 0 {
            return Err("device has no configurations");
        }
        Ok(DeviceDescriptor {
            length: b[0],
            descriptor_type: b[1],
            usb_version: u16::from_le_bytes([b[2], b[3]]),
            device_class: b[4],
            device_subclass: b[5],
            device_protocol: b[6],
            max_packet_size0: b[7],
            vendor_id: u16::from_le_bytes([b[8], b[9]]),
            product_id: u16::from_le_bytes([b[10], b[11]]),
            device_version: u16::from_le_bytes([b[12], b[13]]),
            manufacturer_index: b[14],
            product_index: b[15],
            serial_index: b[16],
            num_configurations: b[17],
        })
    }
}

/// 9-byte configuration descriptor header.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigurationDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration_index: u8,
    pub attributes: u8,
    pub max_power: u8,
}

impl ConfigurationDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() < Self::SIZE {
            return Err("config descriptor too short");
        }
        if b[0] as usize != Self::SIZE || b[1] != super::hcd::DESC_CONFIGURATION {
            return Err("malformed config descriptor header");
        }
        let total_length = u16::from_le_bytes([b[2], b[3]]);
        if total_length < Self::SIZE as u16 {
            return Err("config total length too short");
        }
        if b[4] == 0 {
            return Err("config has no interfaces");
        }
        if b[7] & 0x80 == 0 {
            return Err("config attributes missing reserved bit");
        }
        Ok(ConfigurationDescriptor {
            length: b[0],
            descriptor_type: b[1],
            total_length: u16::from_le_bytes([b[2], b[3]]),
            num_interfaces: b[4],
            configuration_value: b[5],
            configuration_index: b[6],
            attributes: b[7],
            max_power: b[8],
        })
    }
}

/// 9-byte interface descriptor.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interface_index: u8,
}

impl InterfaceDescriptor {
    pub const SIZE: usize = 9;

    pub fn parse(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() < Self::SIZE {
            return Err("interface descriptor too short");
        }
        if b[0] as usize != Self::SIZE || b[1] != super::hcd::DESC_INTERFACE {
            return Err("malformed interface descriptor header");
        }
        Ok(InterfaceDescriptor {
            length: b[0],
            descriptor_type: b[1],
            interface_number: b[2],
            alternate_setting: b[3],
            num_endpoints: b[4],
            interface_class: b[5],
            interface_subclass: b[6],
            interface_protocol: b[7],
            interface_index: b[8],
        })
    }
}

/// 7-byte endpoint descriptor.
#[derive(Debug, Clone, Copy, Default)]
pub struct EndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl EndpointDescriptor {
    pub const SIZE: usize = 7;

    pub fn parse(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() < Self::SIZE {
            return Err("endpoint descriptor too short");
        }
        if b[0] as usize != Self::SIZE || b[1] != super::hcd::DESC_ENDPOINT {
            return Err("malformed endpoint descriptor header");
        }
        let endpoint_number = b[2] & 0x0f;
        if endpoint_number == 0 || endpoint_number > 15 {
            return Err("invalid endpoint address");
        }
        let transfer_type = b[3] & 0x03;
        if transfer_type == 0 {
            return Err("control endpoint descriptor not allowed in configuration");
        }
        let max_packet_size = u16::from_le_bytes([b[4], b[5]]) & 0x07ff;
        if max_packet_size == 0 {
            return Err("endpoint max packet size is zero");
        }
        Ok(EndpointDescriptor {
            length: b[0],
            descriptor_type: b[1],
            endpoint_address: b[2],
            attributes: b[3],
            max_packet_size: u16::from_le_bytes([b[4], b[5]]),
            interval: b[6],
        })
    }

    /// True if this is an IN endpoint.
    pub fn is_in(&self) -> bool {
        self.endpoint_address & 0x80 != 0
    }

    /// Transfer type: 0=control 1=iso 2=bulk 3=interrupt.
    pub fn transfer_type(&self) -> u8 {
        self.attributes & 0x03
    }
}

/// Parsed interface together with its endpoints.
#[derive(Debug, Clone)]
pub struct ParsedInterface {
    pub descriptor: InterfaceDescriptor,
    pub endpoints: Vec<EndpointDescriptor>,
}

/// Full configuration parsed from a `GET_DESCRIPTOR(CONFIGURATION)` blob.
#[derive(Debug, Clone)]
pub struct ParsedConfiguration {
    pub descriptor: ConfigurationDescriptor,
    pub interfaces: Vec<ParsedInterface>,
}

/// Walk a concatenated configuration blob (config + interfaces + endpoints).
pub fn parse_configuration(blob: &[u8]) -> Result<ParsedConfiguration, &'static str> {
    let config = ConfigurationDescriptor::parse(blob)?;
    if blob.len() < config.total_length as usize {
        return Err("configuration blob shorter than wTotalLength");
    }
    if config.length as usize != ConfigurationDescriptor::SIZE {
        return Err("unexpected configuration descriptor length");
    }
    let mut interfaces: Vec<ParsedInterface> = Vec::new();
    let mut offset = config.length as usize;
    let end = config.total_length as usize;

    while offset + 2 <= end {
        let len = blob[offset] as usize;
        let dtype = blob[offset + 1];
        if len < 2 || offset + len > end {
            return Err("malformed descriptor in configuration");
        }
        match dtype {
            t if t == super::hcd::DESC_INTERFACE => {
                let iface = InterfaceDescriptor::parse(&blob[offset..offset + len])?;
                interfaces.push(ParsedInterface {
                    descriptor: iface,
                    endpoints: Vec::new(),
                });
            }
            t if t == super::hcd::DESC_ENDPOINT => {
                let ep = EndpointDescriptor::parse(&blob[offset..offset + len])?;
                if let Some(last) = interfaces.last_mut() {
                    last.endpoints.push(ep);
                } else {
                    return Err("endpoint before interface");
                }
            }
            _ => {} // class-specific descriptor (e.g. HID) — skipped here
        }
        offset += len;
    }

    if interfaces.len() != config.num_interfaces as usize {
        return Err("interface count mismatch");
    }
    for iface in &interfaces {
        if iface.endpoints.len() != iface.descriptor.num_endpoints as usize {
            return Err("endpoint count mismatch");
        }
    }

    Ok(ParsedConfiguration {
        descriptor: config,
        interfaces,
    })
}
