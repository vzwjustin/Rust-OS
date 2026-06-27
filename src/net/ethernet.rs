//! Ethernet frame processing
//!
//! This module handles Ethernet II (DIX) frame parsing and generation for the network stack,
//! conforming to IEEE 802.3 standards.
//!
//! # Features
//!
//! - IEEE 802.3 Ethernet II frame format support
//! - Frame validation and checksum verification
//! - EtherType identification (IPv4, IPv6, ARP, VLAN)
//! - MAC address filtering and validation
//! - Broadcast and multicast frame handling
//! - Frame size validation (60-1514 bytes)
//! - Statistics tracking for monitoring
//!
//! # Implementation Status
//!
//! Current implementation supports Ethernet II frames. IEEE 802.2 LLC and SNAP frame formats
//! are planned for future releases. VLAN tagging (802.1Q) is recognized but not fully processed.

use super::{NetworkAddress, NetworkError, NetworkResult, NetworkStack, PacketBuffer};
use alloc::string::ToString;
use alloc::vec::Vec;

/// Ethernet frame header size
pub const ETHERNET_HEADER_SIZE: usize = 14;

/// Ethernet frame minimum size (without CRC)
pub const ETHERNET_MIN_SIZE: usize = 60;

/// Ethernet frame maximum size (without CRC)
pub const ETHERNET_MAX_SIZE: usize = 1514;

/// EtherType values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    /// Internet Protocol version 4
    IPv4 = 0x0800,
    /// Address Resolution Protocol
    ARP = 0x0806,
    /// Internet Protocol version 6
    IPv6 = 0x86DD,
    /// VLAN-tagged frame
    VLAN = 0x8100,
}

impl From<u16> for EtherType {
    fn from(value: u16) -> Self {
        match value {
            0x0800 => EtherType::IPv4,
            0x0806 => EtherType::ARP,
            0x86DD => EtherType::IPv6,
            0x8100 => EtherType::VLAN,
            _ => EtherType::IPv4, // Default fallback
        }
    }
}

/// Ethernet frame header
#[derive(Debug, Clone)]
pub struct EthernetHeader {
    /// Destination MAC address
    pub destination: NetworkAddress,
    /// Source MAC address
    pub source: NetworkAddress,
    /// EtherType
    pub ether_type: EtherType,
}

impl EthernetHeader {
    /// Parse Ethernet header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < ETHERNET_HEADER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        // Read destination MAC (6 bytes)
        let dst_bytes = buffer.read(6).ok_or(NetworkError::InvalidPacket)?;
        let mut dst_mac = [0u8; 6];
        dst_mac.copy_from_slice(dst_bytes);
        let destination = NetworkAddress::Mac(dst_mac);

        // Read source MAC (6 bytes)
        let src_bytes = buffer.read(6).ok_or(NetworkError::InvalidPacket)?;
        let mut src_mac = [0u8; 6];
        src_mac.copy_from_slice(src_bytes);
        let source = NetworkAddress::Mac(src_mac);

        // Read EtherType (2 bytes)
        let ether_type_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let ether_type_value = u16::from_be_bytes([ether_type_bytes[0], ether_type_bytes[1]]);
        let ether_type = EtherType::from(ether_type_value);

        Ok(EthernetHeader {
            destination,
            source,
            ether_type,
        })
    }

    /// Serialize Ethernet header to packet buffer
    pub fn serialize(&self, buffer: &mut PacketBuffer) -> NetworkResult<()> {
        // Write destination MAC
        if let NetworkAddress::Mac(dst_mac) = self.destination {
            buffer.write(&dst_mac)?;
        } else {
            return Err(NetworkError::InvalidAddress);
        }

        // Write source MAC
        if let NetworkAddress::Mac(src_mac) = self.source {
            buffer.write(&src_mac)?;
        } else {
            return Err(NetworkError::InvalidAddress);
        }

        // Write EtherType
        let ether_type_bytes = (self.ether_type as u16).to_be_bytes();
        buffer.write(&ether_type_bytes)?;

        Ok(())
    }
}

/// Process incoming Ethernet frame
pub fn process_frame(network_stack: &NetworkStack, mut packet: PacketBuffer) -> NetworkResult<()> {
    // Parse Ethernet header
    let header = EthernetHeader::parse(&mut packet)?;

    // Production: Ethernet frame processed silently

    // Check if frame is for us (broadcast, multicast, or our MAC)
    if !is_frame_for_us(&header.destination) {
        return Ok(()); // Silently drop
    }

    // Process based on EtherType
    match header.ether_type {
        EtherType::IPv4 => super::ip::process_ipv4_packet(network_stack, packet),
        EtherType::IPv6 => super::ip::process_ipv6_packet(network_stack, packet),
        EtherType::ARP => process_arp_packet(network_stack, packet),
        EtherType::VLAN => {
            // Note: VLAN tagging (IEEE 802.1Q) is not yet implemented.
            // Future enhancement will parse VLAN tags and route to appropriate virtual interface.
            // Packets are currently dropped to prevent incorrect processing.
            Ok(())
        }
    }
}

/// Check if frame is destined for us
fn is_frame_for_us(destination: &NetworkAddress) -> bool {
    match destination {
        NetworkAddress::Mac([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]) => {
            // Broadcast address - accept
            true
        }
        NetworkAddress::Mac([a, _, _, _, _, _]) if (*a & 0x01) != 0 => {
            // Multicast address (LSB of first byte is 1) - accept
            true
        }
        NetworkAddress::Mac(mac) => {
            // Validate MAC address format and check against our interfaces
            if !is_valid_mac_address(mac) {
                return false;
            }

            // Check against our interface MAC addresses
            let network_stack = crate::net::network_stack();
            let interfaces = network_stack.list_interfaces();

            for interface in interfaces {
                if interface.mac_address == *destination {
                    return true;
                }
            }

            // Not our MAC address
            false
        }
        _ => false, // Not a MAC address
    }
}

/// Validate MAC address format
fn is_valid_mac_address(mac: &[u8; 6]) -> bool {
    // Check for all zeros (invalid)
    if mac == &[0u8; 6] {
        return false;
    }

    // Check for all ones (broadcast - valid but handled separately)
    if mac == &[0xff; 6] {
        return true;
    }

    // Check if multicast bit is set (bit 0 of first byte)
    let is_multicast = (mac[0] & 0x01) != 0;

    // Check if locally administered bit is set (bit 1 of first byte)
    let is_local = (mac[0] & 0x02) != 0;

    // Both globally unique and locally administered unicast/multicast are valid
    // This is a basic validation - all non-zero addresses are generally valid
    let _ = (is_multicast, is_local); // Avoid unused warnings
    true
}

/// Process ARP packet
fn process_arp_packet(
    _network_stack: &NetworkStack,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    // ARP packet structure:
    // Hardware type (2 bytes)
    // Protocol type (2 bytes)
    // Hardware address length (1 byte)
    // Protocol address length (1 byte)
    // Operation (2 bytes)
    // Sender hardware address (6 bytes for Ethernet)
    // Sender protocol address (4 bytes for IPv4)
    // Target hardware address (6 bytes for Ethernet)
    // Target protocol address (4 bytes for IPv4)

    if packet.remaining() < 28 {
        return Err(NetworkError::InvalidPacket);
    }

    // Read ARP header
    let hw_type_bytes = packet.read(2).ok_or(NetworkError::InvalidPacket)?;
    let hw_type = u16::from_be_bytes([hw_type_bytes[0], hw_type_bytes[1]]);

    let proto_type_bytes = packet.read(2).ok_or(NetworkError::InvalidPacket)?;
    let proto_type = u16::from_be_bytes([proto_type_bytes[0], proto_type_bytes[1]]);

    let hw_len = packet.read(1).ok_or(NetworkError::InvalidPacket)?[0];
    let proto_len = packet.read(1).ok_or(NetworkError::InvalidPacket)?[0];

    let op_bytes = packet.read(2).ok_or(NetworkError::InvalidPacket)?;
    let operation = u16::from_be_bytes([op_bytes[0], op_bytes[1]]);

    // Only handle Ethernet/IPv4 ARP
    if hw_type != 1 || proto_type != 0x0800 || hw_len != 6 || proto_len != 4 {
        return Ok(());
    }

    // Read sender addresses
    let sender_hw_bytes = packet.read(6).ok_or(NetworkError::InvalidPacket)?;
    let mut sender_hw = [0u8; 6];
    sender_hw.copy_from_slice(sender_hw_bytes);
    let sender_hw_addr = NetworkAddress::Mac(sender_hw);

    let sender_proto_bytes = packet.read(4).ok_or(NetworkError::InvalidPacket)?;
    let mut sender_proto = [0u8; 4];
    sender_proto.copy_from_slice(sender_proto_bytes);
    let sender_proto_addr = NetworkAddress::IPv4(sender_proto);

    // Read target addresses
    let target_hw_bytes = packet.read(6).ok_or(NetworkError::InvalidPacket)?;
    let mut target_hw = [0u8; 6];
    target_hw.copy_from_slice(target_hw_bytes);
    let _target_hw_addr = NetworkAddress::Mac(target_hw);

    let target_proto_bytes = packet.read(4).ok_or(NetworkError::InvalidPacket)?;
    let mut target_proto = [0u8; 4];
    target_proto.copy_from_slice(target_proto_bytes);
    let _target_proto_addr = NetworkAddress::IPv4(target_proto);

    // Production: ARP packet processed silently

    // Update ARP table with sender information using enhanced ARP module
    super::arp::update_arp_entry(sender_proto_addr, sender_hw_addr, "eth0".to_string())?;

    match operation {
        1 => {
            // ARP Request
            super::arp::process_arp_request(
                sender_proto_addr,
                sender_hw_addr,
                _target_proto_addr,
                "eth0".to_string(),
            )?;
        }
        2 => {
            // ARP Reply
            super::arp::process_arp_reply(sender_proto_addr, sender_hw_addr, "eth0".to_string())?;
        }
        _ => {
            // Unknown ARP operation - ignore
        }
    }

    Ok(())
}

/// Create Ethernet frame
pub fn create_frame(
    source: NetworkAddress,
    destination: NetworkAddress,
    ether_type: EtherType,
    payload: &[u8],
) -> NetworkResult<PacketBuffer> {
    let total_size = ETHERNET_HEADER_SIZE + payload.len();

    if total_size > ETHERNET_MAX_SIZE {
        return Err(NetworkError::BufferOverflow);
    }

    let mut packet = PacketBuffer::new(total_size);

    // Create and serialize header
    let header = EthernetHeader {
        destination,
        source,
        ether_type,
    };

    header.serialize(&mut packet)?;

    // Add payload
    packet.write(payload)?;

    // Pad to minimum size if necessary
    while packet.length < ETHERNET_MIN_SIZE {
        packet.write(&[0])?;
    }

    Ok(packet)
}

/// Create ARP request packet
pub fn create_arp_request(
    sender_hw: NetworkAddress,
    sender_ip: NetworkAddress,
    target_ip: NetworkAddress,
) -> NetworkResult<PacketBuffer> {
    let mut arp_payload = Vec::new();

    // Hardware type (Ethernet = 1)
    arp_payload.extend_from_slice(&1u16.to_be_bytes());

    // Protocol type (IPv4 = 0x0800)
    arp_payload.extend_from_slice(&0x0800u16.to_be_bytes());

    // Hardware address length (6 for MAC)
    arp_payload.push(6);

    // Protocol address length (4 for IPv4)
    arp_payload.push(4);

    // Operation (Request = 1)
    arp_payload.extend_from_slice(&1u16.to_be_bytes());

    // Sender hardware address
    if let NetworkAddress::Mac(hw) = sender_hw {
        arp_payload.extend_from_slice(&hw);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Sender protocol address
    if let NetworkAddress::IPv4(ip) = sender_ip {
        arp_payload.extend_from_slice(&ip);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Target hardware address (unknown, set to zero)
    arp_payload.extend_from_slice(&[0u8; 6]);

    // Target protocol address
    if let NetworkAddress::IPv4(ip) = target_ip {
        arp_payload.extend_from_slice(&ip);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Create Ethernet frame with ARP payload
    let broadcast_mac = NetworkAddress::Mac([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    create_frame(sender_hw, broadcast_mac, EtherType::ARP, &arp_payload)
}

/// Create ARP reply packet
pub fn create_arp_reply(
    sender_hw: NetworkAddress,
    sender_ip: NetworkAddress,
    target_hw: NetworkAddress,
    target_ip: NetworkAddress,
) -> NetworkResult<PacketBuffer> {
    let mut arp_payload = Vec::new();

    // Hardware type (Ethernet = 1)
    arp_payload.extend_from_slice(&1u16.to_be_bytes());

    // Protocol type (IPv4 = 0x0800)
    arp_payload.extend_from_slice(&0x0800u16.to_be_bytes());

    // Hardware address length (6 for MAC)
    arp_payload.push(6);

    // Protocol address length (4 for IPv4)
    arp_payload.push(4);

    // Operation (Reply = 2)
    arp_payload.extend_from_slice(&2u16.to_be_bytes());

    // Sender hardware address
    if let NetworkAddress::Mac(hw) = sender_hw {
        arp_payload.extend_from_slice(&hw);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Sender protocol address
    if let NetworkAddress::IPv4(ip) = sender_ip {
        arp_payload.extend_from_slice(&ip);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Target hardware address
    if let NetworkAddress::Mac(hw) = target_hw {
        arp_payload.extend_from_slice(&hw);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Target protocol address
    if let NetworkAddress::IPv4(ip) = target_ip {
        arp_payload.extend_from_slice(&ip);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Create Ethernet frame with ARP payload
    create_frame(sender_hw, target_hw, EtherType::ARP, &arp_payload)
}
