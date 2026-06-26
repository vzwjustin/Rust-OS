//! IP packet processing (IPv4 and IPv6)
//!
//! This module handles Internet Protocol packet parsing, routing, and forwarding.

use super::{NetworkAddress, NetworkError, NetworkResult, NetworkStack, PacketBuffer, Protocol};
use alloc::vec::Vec;

/// IPv4 header minimum size
pub const IPV4_HEADER_MIN_SIZE: usize = 20;

/// IPv6 header size
pub const IPV6_HEADER_SIZE: usize = 40;

/// IPv4 header
#[derive(Debug, Clone)]
pub struct IPv4Header {
    /// Version (4 bits) and Header Length (4 bits)
    pub version_ihl: u8,
    /// Type of Service
    pub tos: u8,
    /// Total Length
    pub total_length: u16,
    /// Identification
    pub identification: u16,
    /// Flags (3 bits) and Fragment Offset (13 bits)
    pub flags_fragment: u16,
    /// Time to Live
    pub ttl: u8,
    /// Protocol
    pub protocol: u8,
    /// Header Checksum
    pub checksum: u16,
    /// Source Address
    pub source: NetworkAddress,
    /// Destination Address
    pub destination: NetworkAddress,
    /// Options (variable length)
    pub options: Vec<u8>,
}

impl IPv4Header {
    /// Parse IPv4 header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < IPV4_HEADER_MIN_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let version_ihl = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];
        let version = (version_ihl >> 4) & 0x0f;
        let ihl = version_ihl & 0x0f;

        if version != 4 {
            return Err(NetworkError::InvalidPacket);
        }

        let header_length = (ihl as usize) * 4;
        if header_length < IPV4_HEADER_MIN_SIZE || buffer.remaining() + 1 < header_length {
            return Err(NetworkError::InvalidPacket);
        }

        let tos = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];

        let total_length_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let total_length = u16::from_be_bytes([total_length_bytes[0], total_length_bytes[1]]);

        let id_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let identification = u16::from_be_bytes([id_bytes[0], id_bytes[1]]);

        let flags_frag_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let flags_fragment = u16::from_be_bytes([flags_frag_bytes[0], flags_frag_bytes[1]]);

        let ttl = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];
        let protocol = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];

        let checksum_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let checksum = u16::from_be_bytes([checksum_bytes[0], checksum_bytes[1]]);

        let src_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let mut src_addr = [0u8; 4];
        src_addr.copy_from_slice(src_bytes);
        let source = NetworkAddress::IPv4(src_addr);

        let dst_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let mut dst_addr = [0u8; 4];
        dst_addr.copy_from_slice(dst_bytes);
        let destination = NetworkAddress::IPv4(dst_addr);

        // Read options if present
        let options_length = header_length - IPV4_HEADER_MIN_SIZE;
        let options = if options_length > 0 {
            let options_bytes = buffer
                .read(options_length)
                .ok_or(NetworkError::InvalidPacket)?;
            options_bytes.to_vec()
        } else {
            Vec::new()
        };

        Ok(IPv4Header {
            version_ihl,
            tos,
            total_length,
            identification,
            flags_fragment,
            ttl,
            protocol,
            checksum,
            source,
            destination,
            options,
        })
    }

    /// Calculate header checksum
    pub fn calculate_checksum(&self) -> u16 {
        let mut sum = 0u32;

        // Add all 16-bit words in header (except checksum field)
        sum += (self.version_ihl as u32) << 8 | (self.tos as u32);
        sum += self.total_length as u32;
        sum += self.identification as u32;
        sum += self.flags_fragment as u32;
        sum += (self.ttl as u32) << 8 | (self.protocol as u32);
        // Skip checksum field

        if let NetworkAddress::IPv4(src) = self.source {
            sum += ((src[0] as u32) << 8) | (src[1] as u32);
            sum += ((src[2] as u32) << 8) | (src[3] as u32);
        }

        if let NetworkAddress::IPv4(dst) = self.destination {
            sum += ((dst[0] as u32) << 8) | (dst[1] as u32);
            sum += ((dst[2] as u32) << 8) | (dst[3] as u32);
        }

        // Add options
        for chunk in self.options.chunks(2) {
            if chunk.len() == 2 {
                sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
            } else {
                sum += (chunk[0] as u32) << 8;
            }
        }

        // Fold 32-bit sum to 16 bits
        while (sum >> 16) != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }

        // One's complement
        !sum as u16
    }

    /// Check if packet is fragmented
    pub fn is_fragmented(&self) -> bool {
        let more_fragments = (self.flags_fragment & 0x2000) != 0;
        let fragment_offset = self.flags_fragment & 0x1FFF;
        more_fragments || fragment_offset != 0
    }
}

/// IPv6 header
#[derive(Debug, Clone)]
pub struct IPv6Header {
    /// Version (4 bits), Traffic Class (8 bits), Flow Label (20 bits)
    pub version_tc_fl: u32,
    /// Payload Length
    pub payload_length: u16,
    /// Next Header
    pub next_header: u8,
    /// Hop Limit
    pub hop_limit: u8,
    /// Source Address
    pub source: NetworkAddress,
    /// Destination Address
    pub destination: NetworkAddress,
}

impl IPv6Header {
    /// Parse IPv6 header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < IPV6_HEADER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let vtf_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let version_tc_fl =
            u32::from_be_bytes([vtf_bytes[0], vtf_bytes[1], vtf_bytes[2], vtf_bytes[3]]);

        let version = (version_tc_fl >> 28) & 0x0f;
        if version != 6 {
            return Err(NetworkError::InvalidPacket);
        }

        let pl_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let payload_length = u16::from_be_bytes([pl_bytes[0], pl_bytes[1]]);

        let next_header = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];
        let hop_limit = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];

        let src_bytes = buffer.read(16).ok_or(NetworkError::InvalidPacket)?;
        let mut src_addr = [0u8; 16];
        src_addr.copy_from_slice(src_bytes);
        let source = NetworkAddress::IPv6(src_addr);

        let dst_bytes = buffer.read(16).ok_or(NetworkError::InvalidPacket)?;
        let mut dst_addr = [0u8; 16];
        dst_addr.copy_from_slice(dst_bytes);
        let destination = NetworkAddress::IPv6(dst_addr);

        Ok(IPv6Header {
            version_tc_fl,
            payload_length,
            next_header,
            hop_limit,
            source,
            destination,
        })
    }
}

/// Process IPv4 packet
pub fn process_ipv4_packet(
    network_stack: &NetworkStack,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = IPv4Header::parse(&mut packet)?;

    // Production: log only errors, not every packet

    // Verify checksum
    let calculated_checksum = header.calculate_checksum();
    if calculated_checksum != header.checksum {
        // Checksum mismatch - drop packet silently in production
        return Err(NetworkError::InvalidPacket);
    }

    // Check if packet is for us
    if !is_packet_for_us(&header.destination) {
        // Forward packet if we're a router
        return forward_ipv4_packet(network_stack, header, packet);
    }

    // Handle fragmentation
    if header.is_fragmented() {
        // Fragmented packets not supported - drop silently
        return Ok(());
    }

    // Trim any Ethernet padding so the upper layer (and its checksum) only sees
    // the real IP payload. total_length covers the IP header + payload; anything
    // beyond it in the frame is padding. Guard against a malformed header whose
    // total_length is smaller than the header itself.
    let header_len = ((header.version_ihl & 0x0f) as usize) * 4;
    if (header.total_length as usize) >= header_len {
        let payload_len = header.total_length as usize - header_len;
        let trimmed = packet.position + payload_len;
        if trimmed <= packet.length {
            packet.length = trimmed;
        }
    }

    // Process based on protocol
    let protocol = Protocol::from(header.protocol);
    match protocol {
        Protocol::ICMP => super::icmp::process_icmp_packet(
            network_stack,
            header.source,
            header.destination,
            packet,
        ),
        Protocol::TCP => {
            super::tcp::process_packet(network_stack, header.source, header.destination, packet)
        }
        Protocol::UDP => {
            super::udp::process_packet(network_stack, header.source, header.destination, packet)
        }
        _ => {
            // Unsupported protocol - drop silently
            Ok(())
        }
    }
}

/// Process IPv6 packet
pub fn process_ipv6_packet(
    network_stack: &NetworkStack,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = IPv6Header::parse(&mut packet)?;

    // Production: process packet without debug output

    // Check if packet is for us
    if !is_packet_for_us(&header.destination) {
        // Forward packet if we're a router
        return forward_ipv6_packet(network_stack, header, packet);
    }

    // Process based on next header
    match header.next_header {
        58 => {
            // ICMPv6
            super::icmp::process_icmpv6_packet(
                network_stack,
                header.source,
                header.destination,
                packet,
            )
        }
        6 => {
            // TCP
            super::tcp::process_packet(network_stack, header.source, header.destination, packet)
        }
        17 => {
            // UDP
            super::udp::process_packet(network_stack, header.source, header.destination, packet)
        }
        _ => {
            // Unsupported next header - drop silently
            Ok(())
        }
    }
}

/// Check if packet is destined for us
fn is_packet_for_us(destination: &NetworkAddress) -> bool {
    // Check against all interface addresses
    let interfaces = super::network_stack().list_interfaces();
    for interface in interfaces {
        if interface.ip_addresses.contains(destination) {
            return true;
        }
    }

    // Check for broadcast/multicast
    match destination {
        NetworkAddress::IPv4([255, 255, 255, 255]) => true, // Broadcast
        NetworkAddress::IPv4([a, _, _, _]) if (*a & 0xf0) == 0xe0 => true, // Multicast
        NetworkAddress::IPv6([0xff, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _]) => true, // Multicast
        _ => false,
    }
}

/// Forward IPv4 packet
fn forward_ipv4_packet(
    network_stack: &NetworkStack,
    mut header: IPv4Header,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    // Decrement TTL
    if header.ttl <= 1 {
        // Send ICMP Time Exceeded (type 11, code 0)
        send_icmp_time_exceeded(header.destination, header.source)?;
        return Ok(());
    }
    header.ttl -= 1;

    // Find route to destination
    if let Some(route) = network_stack.find_route(&header.destination) {
        // Recalculate checksum with updated TTL
        header.checksum = header.calculate_checksum();

        // Reconstruct packet with updated header
        let payload = packet.read(packet.remaining()).unwrap_or(&[]).to_vec();

        // Build new packet with updated header
        let mut new_packet_data =
            Vec::with_capacity(IPV4_HEADER_MIN_SIZE + header.options.len() + payload.len());

        // Serialize IPv4 header
        new_packet_data.push(header.version_ihl);
        new_packet_data.push(header.tos);
        new_packet_data.extend_from_slice(&header.total_length.to_be_bytes());
        new_packet_data.extend_from_slice(&header.identification.to_be_bytes());
        new_packet_data.extend_from_slice(&header.flags_fragment.to_be_bytes());
        new_packet_data.push(header.ttl);
        new_packet_data.push(header.protocol);
        new_packet_data.extend_from_slice(&header.checksum.to_be_bytes());

        if let NetworkAddress::IPv4(src) = header.source {
            new_packet_data.extend_from_slice(&src);
        }
        if let NetworkAddress::IPv4(dst) = header.destination {
            new_packet_data.extend_from_slice(&dst);
        }

        // Add options if present
        if !header.options.is_empty() {
            new_packet_data.extend_from_slice(&header.options);
        }

        // Add payload
        new_packet_data.extend_from_slice(&payload);

        // Send via route interface
        let packet_buffer = PacketBuffer::from_data(new_packet_data);
        network_stack.send_packet(&route.interface, packet_buffer)?;

        Ok(())
    } else {
        // No route found - send ICMP Destination Unreachable (type 3, code 0)
        send_icmp_dest_unreachable(header.destination, header.source)?;
        Ok(())
    }
}

/// Forward IPv6 packet
fn forward_ipv6_packet(
    network_stack: &NetworkStack,
    mut header: IPv6Header,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    // Decrement hop limit
    if header.hop_limit <= 1 {
        // Send ICMPv6 Time Exceeded (type 3, code 0)
        send_icmpv6_time_exceeded(header.destination, header.source)?;
        return Ok(());
    }
    header.hop_limit -= 1;

    // Find route to destination
    if let Some(route) = network_stack.find_route(&header.destination) {
        // Reconstruct packet with updated hop limit
        let payload = packet.read(packet.remaining()).unwrap_or(&[]).to_vec();

        // Build new packet with updated header
        let mut new_packet_data = Vec::with_capacity(IPV6_HEADER_SIZE + payload.len());

        // Serialize IPv6 header with updated hop limit
        let version_tc_fl = header.version_tc_fl;
        new_packet_data.extend_from_slice(&version_tc_fl.to_be_bytes());
        new_packet_data.extend_from_slice(&header.payload_length.to_be_bytes());
        new_packet_data.push(header.next_header);
        new_packet_data.push(header.hop_limit);

        if let NetworkAddress::IPv6(src) = header.source {
            new_packet_data.extend_from_slice(&src);
        }
        if let NetworkAddress::IPv6(dst) = header.destination {
            new_packet_data.extend_from_slice(&dst);
        }

        // Add payload
        new_packet_data.extend_from_slice(&payload);

        // Send via route interface
        let packet_buffer = PacketBuffer::from_data(new_packet_data);
        network_stack.send_packet(&route.interface, packet_buffer)?;

        Ok(())
    } else {
        // No route found - send ICMPv6 Destination Unreachable (type 1, code 0)
        send_icmpv6_dest_unreachable(header.destination, header.source)?;
        Ok(())
    }
}

impl From<u8> for Protocol {
    fn from(value: u8) -> Self {
        match value {
            1 => Protocol::ICMP,
            6 => Protocol::TCP,
            17 => Protocol::UDP,
            41 => Protocol::IPv6inIPv4,
            47 => Protocol::GRE,
            58 => Protocol::ICMPv6,
            _ => Protocol::TCP, // Default fallback
        }
    }
}

/// Send IPv4 packet with specified protocol and payload
pub fn send_ipv4_packet(
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    protocol: u8,
    payload: &[u8],
) -> NetworkResult<()> {
    use crate::net::network_stack;

    // Create IPv4 header
    let total_length = (IPV4_HEADER_MIN_SIZE + payload.len()) as u16;

    let mut header_bytes = Vec::with_capacity(IPV4_HEADER_MIN_SIZE + payload.len());

    // Version (4) and IHL (5 = 20 bytes)
    header_bytes.push(0x45);
    // TOS
    header_bytes.push(0x00);
    // Total length
    header_bytes.push((total_length >> 8) as u8);
    header_bytes.push((total_length & 0xFF) as u8);
    // Identification
    header_bytes.push(0x00);
    header_bytes.push(0x00);
    // Flags and Fragment Offset (don't fragment)
    header_bytes.push(0x40);
    header_bytes.push(0x00);
    // TTL
    header_bytes.push(64);
    // Protocol
    header_bytes.push(protocol);
    // Checksum (calculate after)
    let checksum_offset = header_bytes.len();
    header_bytes.push(0x00);
    header_bytes.push(0x00);

    // Source IP
    if let NetworkAddress::IPv4(src) = src_ip {
        header_bytes.extend_from_slice(&src);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Destination IP
    if let NetworkAddress::IPv4(dst) = dst_ip {
        header_bytes.extend_from_slice(&dst);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Calculate header checksum
    let checksum = calculate_ip_checksum(&header_bytes);
    header_bytes[checksum_offset] = (checksum >> 8) as u8;
    header_bytes[checksum_offset + 1] = (checksum & 0xFF) as u8;

    // Add payload
    header_bytes.extend_from_slice(payload);

    // Wrap in Ethernet frame with proper MAC addresses
    let stack = network_stack();
    let interfaces = stack.list_interfaces();

    let interface = interfaces.first().ok_or(NetworkError::NetworkUnreachable)?;

    // Resolve destination MAC address
    let dest_mac = if dst_ip.is_broadcast() {
        // Broadcast address
        NetworkAddress::Mac([0xFF; 6])
    } else {
        // Try to resolve MAC via ARP
        match super::arp::lookup_arp(&dst_ip) {
            Some(mac) => mac,
            None => {
                // MAC not in ARP cache - send ARP request and use broadcast as fallback
                // In production, packet would be queued pending ARP resolution
                let _ = super::arp::send_arp_request(dst_ip, interface.name.clone());
                NetworkAddress::Mac([0xFF; 6])
            }
        }
    };

    // Get source MAC from interface
    let src_mac = interface.mac_address;

    // Build Ethernet frame: [dest MAC (6)] [src MAC (6)] [EtherType (2)] [IP packet]
    let mut eth_frame = Vec::with_capacity(14 + header_bytes.len());

    // Destination MAC (6 bytes)
    if let NetworkAddress::Mac(mac) = dest_mac {
        eth_frame.extend_from_slice(&mac);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Source MAC (6 bytes)
    if let NetworkAddress::Mac(mac) = src_mac {
        eth_frame.extend_from_slice(&mac);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // EtherType: 0x0800 for IPv4 (2 bytes, big-endian)
    eth_frame.extend_from_slice(&[0x08, 0x00]);

    // IP packet
    eth_frame.extend_from_slice(&header_bytes);

    // Send through network stack
    let packet = PacketBuffer::from_data(eth_frame);
    stack.send_packet(&interface.name, packet)
}

/// Calculate IP header checksum
fn calculate_ip_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;

    for chunk in data.chunks(2) {
        if chunk.len() == 2 {
            sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
        } else {
            sum += (chunk[0] as u32) << 8;
        }
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

/// Calculate ICMPv6 checksum with IPv6 pseudo-header
/// RFC 4443 Section 2.3, RFC 2460 Section 8.1
fn calculate_icmpv6_checksum(src: &[u8; 16], dst: &[u8; 16], packet: &[u8]) -> u16 {
    let mut sum = 0u32;

    // IPv6 pseudo-header checksum
    // Source address (16 bytes)
    for chunk in src.chunks(2) {
        sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
    }

    // Destination address (16 bytes)
    for chunk in dst.chunks(2) {
        sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
    }

    // Upper-layer packet length (32 bits)
    let packet_len = packet.len() as u32;
    sum += packet_len >> 16;
    sum += packet_len & 0xFFFF;

    // Next header (ICMPv6 = 58, padded to 32 bits)
    sum += 58;

    // ICMPv6 packet data
    for chunk in packet.chunks(2) {
        if chunk.len() == 2 {
            sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
        } else {
            sum += (chunk[0] as u32) << 8;
        }
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !sum as u16
}

/// Send IPv6 packet with specified next header and payload
pub fn send_ipv6_packet(
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    next_header: u8,
    payload: &[u8],
) -> NetworkResult<()> {
    use crate::net::network_stack;

    // Create IPv6 header
    let payload_length = payload.len() as u16;

    let mut header_bytes = Vec::with_capacity(IPV6_HEADER_SIZE + payload.len());

    // Version (4 bits = 6), Traffic Class (8 bits = 0), Flow Label (20 bits = 0)
    let version_tc_fl = 0x60000000u32; // Version 6, TC 0, FL 0
    header_bytes.extend_from_slice(&version_tc_fl.to_be_bytes());

    // Payload Length (16 bits)
    header_bytes.extend_from_slice(&payload_length.to_be_bytes());

    // Next Header (8 bits)
    header_bytes.push(next_header);

    // Hop Limit (8 bits) - default to 64
    header_bytes.push(64);

    // Source IPv6 address (128 bits)
    if let NetworkAddress::IPv6(src) = src_ip {
        header_bytes.extend_from_slice(&src);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Destination IPv6 address (128 bits)
    if let NetworkAddress::IPv6(dst) = dst_ip {
        header_bytes.extend_from_slice(&dst);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Add payload
    header_bytes.extend_from_slice(payload);

    // Wrap in Ethernet frame with proper MAC addresses
    let stack = network_stack();
    let interfaces = stack.list_interfaces();

    let interface = interfaces.first().ok_or(NetworkError::NetworkUnreachable)?;

    // Resolve destination MAC address for IPv6
    // For IPv6, we use NDP (Neighbor Discovery Protocol) instead of ARP
    // For now, use broadcast/multicast MAC for IPv6
    let dest_mac = if dst_ip.is_broadcast() || dst_ip.is_multicast() {
        // IPv6 multicast MAC: 33:33:xx:xx:xx:xx where xx:xx:xx:xx are lower 32 bits of IPv6 address
        if let NetworkAddress::IPv6(addr) = dst_ip {
            NetworkAddress::Mac([0x33, 0x33, addr[12], addr[13], addr[14], addr[15]])
        } else {
            NetworkAddress::Mac([0xFF; 6]) // Fallback to broadcast
        }
    } else {
        // Try neighbor discovery table lookup (similar to ARP for IPv4)
        // For now, use broadcast as fallback
        NetworkAddress::Mac([0xFF; 6])
    };

    // Get source MAC from interface
    let src_mac = interface.mac_address;

    // Build Ethernet frame: [dest MAC (6)] [src MAC (6)] [EtherType (2)] [IPv6 packet]
    let mut eth_frame = Vec::with_capacity(14 + header_bytes.len());

    // Destination MAC (6 bytes)
    if let NetworkAddress::Mac(mac) = dest_mac {
        eth_frame.extend_from_slice(&mac);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Source MAC (6 bytes)
    if let NetworkAddress::Mac(mac) = src_mac {
        eth_frame.extend_from_slice(&mac);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // EtherType: 0x86DD for IPv6 (2 bytes, big-endian)
    eth_frame.extend_from_slice(&[0x86, 0xDD]);

    // IPv6 packet
    eth_frame.extend_from_slice(&header_bytes);

    // Send through network stack
    let packet = PacketBuffer::from_data(eth_frame);
    stack.send_packet(&interface.name, packet)
}

/// Send ICMP Time Exceeded message
fn send_icmp_time_exceeded(src_ip: NetworkAddress, dst_ip: NetworkAddress) -> NetworkResult<()> {
    // ICMP Time Exceeded: Type 11, Code 0 (TTL exceeded in transit)
    let mut icmp_packet = Vec::new();

    icmp_packet.push(11u8); // Type: Time Exceeded
    icmp_packet.push(0u8); // Code: TTL exceeded in transit
    icmp_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
    icmp_packet.extend_from_slice(&[0u8; 4]); // Unused

    // Calculate checksum
    let checksum = calculate_ip_checksum(&icmp_packet);
    icmp_packet[2] = (checksum >> 8) as u8;
    icmp_packet[3] = (checksum & 0xFF) as u8;

    // Send through IP layer (protocol 1 = ICMP)
    send_ipv4_packet(src_ip, dst_ip, 1, &icmp_packet)
}

/// Send ICMP Destination Unreachable message
fn send_icmp_dest_unreachable(src_ip: NetworkAddress, dst_ip: NetworkAddress) -> NetworkResult<()> {
    // ICMP Destination Unreachable: Type 3, Code 0 (Network unreachable)
    let mut icmp_packet = Vec::new();

    icmp_packet.push(3u8); // Type: Destination Unreachable
    icmp_packet.push(0u8); // Code: Network unreachable
    icmp_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
    icmp_packet.extend_from_slice(&[0u8; 4]); // Unused

    // Calculate checksum
    let checksum = calculate_ip_checksum(&icmp_packet);
    icmp_packet[2] = (checksum >> 8) as u8;
    icmp_packet[3] = (checksum & 0xFF) as u8;

    // Send through IP layer (protocol 1 = ICMP)
    send_ipv4_packet(src_ip, dst_ip, 1, &icmp_packet)
}

/// Send ICMPv6 Time Exceeded message
fn send_icmpv6_time_exceeded(src_ip: NetworkAddress, dst_ip: NetworkAddress) -> NetworkResult<()> {
    // ICMPv6 Time Exceeded: Type 3, Code 0 (Hop limit exceeded in transit)
    // RFC 4443 Section 3.3
    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (src_ip, dst_ip) {
        let mut icmpv6_packet = Vec::new();

        // ICMPv6 Type 3 (Time Exceeded), Code 0 (Hop limit exceeded in transit)
        icmpv6_packet.push(3u8);
        icmpv6_packet.push(0u8);
        icmpv6_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
        icmpv6_packet.extend_from_slice(&[0u8; 4]); // Unused (must be zero)

        // Note: RFC 4443 Section 2.4 recommends including as much of the invoking packet
        // as possible without exceeding the minimum IPv6 MTU (1280 bytes).
        // Full implementation would include:
        // - Original IPv6 header (40 bytes)
        // - As much of the original payload as fits (up to 1232 bytes after headers)
        // Current implementation sends minimal error message without original packet data.
        //
        // Future enhancement: Pass original packet buffer to error functions and append:
        //   let max_excerpt = 1280 - 40 (IPv6) - 8 (ICMPv6) = 1232 bytes
        //   icmpv6_packet.extend_from_slice(&original_packet[..max_excerpt.min(len)])

        // Calculate ICMPv6 checksum with IPv6 pseudo-header
        let checksum = calculate_icmpv6_checksum(&src, &dst, &icmpv6_packet);
        icmpv6_packet[2] = (checksum >> 8) as u8;
        icmpv6_packet[3] = (checksum & 0xFF) as u8;

        // Send via IPv6 layer with next header 58 (ICMPv6)
        send_ipv6_packet(src_ip, dst_ip, 58, &icmpv6_packet)
    } else {
        Ok(()) // Not IPv6, silently ignore
    }
}

/// Send ICMPv6 Destination Unreachable message
fn send_icmpv6_dest_unreachable(
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
) -> NetworkResult<()> {
    // ICMPv6 Destination Unreachable: Type 1, Code 0 (No route to destination)
    // RFC 4443 Section 3.1
    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (src_ip, dst_ip) {
        let mut icmpv6_packet = Vec::new();

        // ICMPv6 Type 1 (Destination Unreachable), Code 0 (No route to destination)
        icmpv6_packet.push(1u8);
        icmpv6_packet.push(0u8);
        icmpv6_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
        icmpv6_packet.extend_from_slice(&[0u8; 4]); // Unused (must be zero)

        // Note: RFC 4443 Section 2.4 recommends including as much of the invoking packet
        // as possible without exceeding the minimum IPv6 MTU (1280 bytes).
        // Full implementation would include:
        // - Original IPv6 header (40 bytes)
        // - As much of the original payload as fits (up to 1232 bytes after headers)
        // Current implementation sends minimal error message without original packet data.
        //
        // Future enhancement: Pass original packet buffer to error functions and append:
        //   let max_excerpt = 1280 - 40 (IPv6) - 8 (ICMPv6) = 1232 bytes
        //   icmpv6_packet.extend_from_slice(&original_packet[..max_excerpt.min(len)])

        // Calculate ICMPv6 checksum with IPv6 pseudo-header
        let checksum = calculate_icmpv6_checksum(&src, &dst, &icmpv6_packet);
        icmpv6_packet[2] = (checksum >> 8) as u8;
        icmpv6_packet[3] = (checksum & 0xFF) as u8;

        // Send via IPv6 layer with next header 58 (ICMPv6)
        send_ipv6_packet(src_ip, dst_ip, 58, &icmpv6_packet)
    } else {
        Ok(()) // Not IPv6, silently ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn process_ipv4_packet_accepts_valid_checksum() {
        let network_stack = NetworkStack::new();

        let mut packet_bytes = vec![
            0x45, 0x00, 0x00, 0x1c, // version/IHL, TOS, total length (28)
            0x00, 0x00, // identification
            0x00, 0x00, // flags/fragment offset
            0x40, 0x01, // TTL, protocol (ICMP)
            0x00, 0x00, // checksum placeholder
            0xC0, 0x00, 0x02, 0x01, // source IP 192.0.2.1
            0xFF, 0xFF, 0xFF, 0xFF, // destination IP broadcast
        ];

        let known_checksum = 0xB8E0;
        packet_bytes[10] = (known_checksum >> 8) as u8;
        packet_bytes[11] = (known_checksum & 0xFF) as u8;

        // Minimal 8-byte ICMP payload to exercise the success path
        packet_bytes.extend_from_slice(&[0u8; 8]);

        let packet = PacketBuffer::from_data(packet_bytes);
        assert!(process_ipv4_packet(&network_stack, packet).is_ok());
    }
}
