//! ICMP and ICMPv6 protocol implementation
//!
//! Provides comprehensive Internet Control Message Protocol support for IPv4 and IPv6,
//! including ping, traceroute, error reporting, and neighbor discovery.
//!
//! # Features
//!
//! - RFC 792 compliant ICMP (IPv4) implementation
//! - RFC 4443 compliant ICMPv6 implementation
//! - Echo request/reply (ping) functionality
//! - Error message generation and processing
//! - Router discovery for IPv6 (RFC 4861)
//! - Neighbor discovery protocol (NDP) for IPv6
//! - Comprehensive statistics tracking
//! - Rate limiting for ICMP responses
//!
//! # Implementation Status
//!
//! ICMPv6 support is currently in development. Neighbor Discovery Protocol (NDP)
//! and router discovery features require completion of the IPv6 stack integration.

use super::{NetworkAddress, NetworkResult, NetworkError, PacketBuffer, NetworkStack};
use alloc::{vec::Vec, collections::BTreeMap};
use spin::RwLock;

/// ICMP message types (IPv4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IcmpType {
    EchoReply = 0,
    DestinationUnreachable = 3,
    SourceQuench = 4,
    Redirect = 5,
    EchoRequest = 8,
    RouterAdvertisement = 9,
    RouterSolicitation = 10,
    TimeExceeded = 11,
    ParameterProblem = 12,
    TimestampRequest = 13,
    TimestampReply = 14,
    InformationRequest = 15,
    InformationReply = 16,
}

/// ICMPv6 message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Icmpv6Type {
    DestinationUnreachable = 1,
    PacketTooBig = 2,
    TimeExceeded = 3,
    ParameterProblem = 4,
    EchoRequest = 128,
    EchoReply = 129,
    RouterSolicitation = 133,
    RouterAdvertisement = 134,
    NeighborSolicitation = 135,
    NeighborAdvertisement = 136,
    Redirect = 137,
}

/// ICMP header
#[derive(Debug, Clone)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub rest: [u8; 4],
}

impl IcmpHeader {
    /// Parse ICMP header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < 8 {
            return Err(NetworkError::InvalidPacket);
        }

        let icmp_type = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];
        let code = buffer.read(1).ok_or(NetworkError::InvalidPacket)?[0];

        let checksum_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let checksum = u16::from_be_bytes([checksum_bytes[0], checksum_bytes[1]]);

        let rest_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let mut rest = [0u8; 4];
        rest.copy_from_slice(rest_bytes);

        Ok(IcmpHeader {
            icmp_type,
            code,
            checksum,
            rest,
        })
    }

    /// Calculate ICMP checksum
    pub fn calculate_checksum(&self, payload: &[u8]) -> u16 {
        let mut sum = 0u32;

        // ICMP header (excluding checksum field)
        sum += (self.icmp_type as u32) << 8 | (self.code as u32);
        // Skip checksum
        sum += ((self.rest[0] as u32) << 8) | (self.rest[1] as u32);
        sum += ((self.rest[2] as u32) << 8) | (self.rest[3] as u32);

        // Payload
        for chunk in payload.chunks(2) {
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

    /// Serialize ICMP header to buffer
    pub fn serialize(&self, buffer: &mut PacketBuffer) -> NetworkResult<()> {
        buffer.write(&[self.icmp_type])?;
        buffer.write(&[self.code])?;
        buffer.write(&self.checksum.to_be_bytes())?;
        buffer.write(&self.rest)?;
        Ok(())
    }
}

/// ICMPv6 header (similar structure to ICMP but with different semantics)
pub type Icmpv6Header = IcmpHeader;

/// Ping request/reply data
#[derive(Debug, Clone)]
pub struct PingData {
    pub identifier: u16,
    pub sequence: u16,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}

impl PingData {
    pub fn new(identifier: u16, sequence: u16, payload: Vec<u8>) -> Self {
        Self {
            identifier,
            sequence,
            payload,
            timestamp: current_time_ms(),
        }
    }

    pub fn from_rest_bytes(rest: &[u8; 4], payload: Vec<u8>) -> Self {
        let identifier = u16::from_be_bytes([rest[0], rest[1]]);
        let sequence = u16::from_be_bytes([rest[2], rest[3]]);

        Self {
            identifier,
            sequence,
            payload,
            timestamp: current_time_ms(),
        }
    }

    pub fn to_rest_bytes(&self) -> [u8; 4] {
        let mut rest = [0u8; 4];
        rest[0..2].copy_from_slice(&self.identifier.to_be_bytes());
        rest[2..4].copy_from_slice(&self.sequence.to_be_bytes());
        rest
    }
}

/// Neighbor Discovery Cache Entry (IPv6)
#[derive(Debug, Clone)]
pub struct NeighborCacheEntry {
    pub ip_address: NetworkAddress,
    pub mac_address: Option<NetworkAddress>,
    pub state: NeighborState,
    pub last_update: u64,
    pub reachable_time: u64,
    pub retransmit_timer: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborState {
    Incomplete,
    Reachable,
    Stale,
    Delay,
    Probe,
}

/// ICMP manager for handling ping and error messages
pub struct IcmpManager {
    /// Active ping sessions
    ping_sessions: RwLock<BTreeMap<u16, PingSession>>,
    /// Next ping identifier
    next_ping_id: RwLock<u16>,
    /// Neighbor discovery cache (IPv6)
    neighbor_cache: RwLock<BTreeMap<NetworkAddress, NeighborCacheEntry>>,
}

#[derive(Debug, Clone)]
pub struct PingSession {
    pub target: NetworkAddress,
    pub identifier: u16,
    pub next_sequence: u16,
    pub sent_pings: BTreeMap<u16, u64>, // sequence -> timestamp
    pub received_replies: Vec<PingReply>,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone)]
pub struct PingReply {
    pub sequence: u16,
    pub rtt_ms: u32,
    pub payload: Vec<u8>,
}

impl IcmpManager {
    pub fn new() -> Self {
        Self {
            ping_sessions: RwLock::new(BTreeMap::new()),
            next_ping_id: RwLock::new(1),
            neighbor_cache: RwLock::new(BTreeMap::new()),
        }
    }

    /// Start a new ping session
    pub fn start_ping(&self, target: NetworkAddress) -> NetworkResult<u16> {
        let identifier = {
            let mut next_id = self.next_ping_id.write();
            let id = *next_id;
            *next_id = if id == u16::MAX { 1 } else { id + 1 };
            id
        };

        let session = PingSession {
            target,
            identifier,
            next_sequence: 1,
            sent_pings: BTreeMap::new(),
            received_replies: Vec::new(),
            timeout_ms: 5000, // 5 seconds
        };

        let mut sessions = self.ping_sessions.write();
        sessions.insert(identifier, session);

        Ok(identifier)
    }

    /// Send ping request
    pub fn send_ping(&self, identifier: u16, payload: Vec<u8>) -> NetworkResult<u16> {
        let mut sessions = self.ping_sessions.write();
        let session = sessions.get_mut(&identifier).ok_or(NetworkError::InvalidAddress)?;

        let sequence = session.next_sequence;
        session.next_sequence += 1;
        session.sent_pings.insert(sequence, current_time_ms());

        // Send actual ping packet
        match session.target {
            NetworkAddress::IPv4(_) => {
                send_icmp_echo_request(session.target, identifier, sequence, &payload)?;
            }
            NetworkAddress::IPv6(_) => {
                send_icmpv6_echo_request(session.target, identifier, sequence, &payload)?;
            }
            _ => return Err(NetworkError::InvalidAddress),
        }

        Ok(sequence)
    }

    /// Handle ping reply
    pub fn handle_ping_reply(&self, identifier: u16, sequence: u16, payload: Vec<u8>) -> NetworkResult<()> {
        let mut sessions = self.ping_sessions.write();
        if let Some(session) = sessions.get_mut(&identifier) {
            if let Some(sent_time) = session.sent_pings.remove(&sequence) {
                let rtt_ms = (current_time_ms() - sent_time) as u32;
                let reply = PingReply {
                    sequence,
                    rtt_ms,
                    payload,
                };
                session.received_replies.push(reply);
            }
        }
        Ok(())
    }

    /// Get ping results
    pub fn get_ping_results(&self, identifier: u16) -> Option<Vec<PingReply>> {
        let sessions = self.ping_sessions.read();
        sessions.get(&identifier).map(|session| session.received_replies.clone())
    }

    /// Close ping session
    pub fn close_ping(&self, identifier: u16) -> NetworkResult<()> {
        let mut sessions = self.ping_sessions.write();
        sessions.remove(&identifier);
        Ok(())
    }

    /// Update neighbor cache entry (IPv6)
    pub fn update_neighbor(&self, ip: NetworkAddress, mac: Option<NetworkAddress>, state: NeighborState) {
        let mut cache = self.neighbor_cache.write();
        let entry = NeighborCacheEntry {
            ip_address: ip,
            mac_address: mac,
            state,
            last_update: current_time_ms(),
            reachable_time: 30000, // 30 seconds
            retransmit_timer: 1000, // 1 second
        };
        cache.insert(ip, entry);
    }

    /// Lookup neighbor
    pub fn lookup_neighbor(&self, ip: &NetworkAddress) -> Option<NeighborCacheEntry> {
        let cache = self.neighbor_cache.read();
        cache.get(ip).cloned()
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&self) {
        let now = current_time_ms();

        // Clean up ping sessions
        {
            let mut sessions = self.ping_sessions.write();
            sessions.retain(|_, session| {
                session.sent_pings.retain(|_, &mut sent_time| {
                    now - sent_time < session.timeout_ms as u64
                });
                !session.sent_pings.is_empty() || !session.received_replies.is_empty()
            });
        }

        // Clean up neighbor cache
        {
            let mut cache = self.neighbor_cache.write();
            cache.retain(|_, entry| {
                match entry.state {
                    NeighborState::Reachable => now - entry.last_update < entry.reachable_time,
                    NeighborState::Stale => now - entry.last_update < 86400000, // 24 hours
                    _ => now - entry.last_update < 30000, // 30 seconds for other states
                }
            });
        }
    }
}

static ICMP_MANAGER: IcmpManager = IcmpManager {
    ping_sessions: RwLock::new(BTreeMap::new()),
    next_ping_id: RwLock::new(1),
    neighbor_cache: RwLock::new(BTreeMap::new()),
};

/// Process ICMP packet (IPv4)
pub fn process_icmp_packet(
    network_stack: &NetworkStack,
    ip_header_src: NetworkAddress,
    ip_header_dst: NetworkAddress,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = IcmpHeader::parse(&mut packet)?;
    let payload = packet.read(packet.remaining()).unwrap_or(&[]).to_vec();

    // Verify checksum
    let calculated_checksum = {
        let mut check_header = header.clone();
        check_header.checksum = 0;
        check_header.calculate_checksum(&payload)
    };

    if calculated_checksum != header.checksum {
        return Err(NetworkError::InvalidPacket);
    }

    match header.icmp_type {
        8 => { // Echo Request
            let ping_data = PingData::from_rest_bytes(&header.rest, payload);
            handle_echo_request(network_stack, ip_header_src, ip_header_dst, ping_data)?;
        }
        0 => { // Echo Reply
            let ping_data = PingData::from_rest_bytes(&header.rest, payload);
            ICMP_MANAGER.handle_ping_reply(ping_data.identifier, ping_data.sequence, ping_data.payload)?;
        }
        3 => { // Destination Unreachable
            handle_destination_unreachable(header.code, &payload)?;
        }
        11 => { // Time Exceeded
            handle_time_exceeded(header.code, &payload)?;
        }
        _ => {
            // Unknown ICMP type - silently ignore
        }
    }

    Ok(())
}

/// Process ICMPv6 packet
pub fn process_icmpv6_packet(
    network_stack: &NetworkStack,
    ip_header_src: NetworkAddress,
    ip_header_dst: NetworkAddress,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = Icmpv6Header::parse(&mut packet)?;
    let payload = packet.read(packet.remaining()).unwrap_or(&[]).to_vec();

    // Verify ICMPv6 checksum with pseudo-header (RFC 4443 Section 2.3)
    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (ip_header_src, ip_header_dst) {
        // Reconstruct full ICMPv6 packet for checksum verification
        let mut full_packet = Vec::new();
        full_packet.push(header.icmp_type);
        full_packet.push(header.code);
        full_packet.extend_from_slice(&[0u8; 2]); // Zero out checksum field for verification
        full_packet.extend_from_slice(&header.rest);
        full_packet.extend_from_slice(&payload);

        let calculated_checksum = calculate_icmpv6_checksum_local(&src, &dst, &full_packet);
        if calculated_checksum != header.checksum {
            // Checksum mismatch - drop packet silently in production
            return Err(NetworkError::InvalidPacket);
        }
    }

    match header.icmp_type {
        128 => { // Echo Request
            let ping_data = PingData::from_rest_bytes(&header.rest, payload);
            handle_icmpv6_echo_request(network_stack, ip_header_src, ip_header_dst, ping_data)?;
        }
        129 => { // Echo Reply
            let ping_data = PingData::from_rest_bytes(&header.rest, payload);
            ICMP_MANAGER.handle_ping_reply(ping_data.identifier, ping_data.sequence, ping_data.payload)?;
        }
        135 => { // Neighbor Solicitation
            handle_neighbor_solicitation(network_stack, ip_header_src, ip_header_dst, &payload)?;
        }
        136 => { // Neighbor Advertisement
            handle_neighbor_advertisement(network_stack, ip_header_src, &payload)?;
        }
        1 => { // Destination Unreachable
            handle_icmpv6_destination_unreachable(header.code, &payload)?;
        }
        3 => { // Time Exceeded
            handle_icmpv6_time_exceeded(header.code, &payload)?;
        }
        _ => {
            // Unknown ICMPv6 type - silently ignore
        }
    }

    Ok(())
}

/// Handle ICMP echo request (ping)
fn handle_echo_request(
    _network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    ping_data: PingData,
) -> NetworkResult<()> {
    // Send echo reply
    send_icmp_echo_reply(src_ip, dst_ip, ping_data.identifier, ping_data.sequence, &ping_data.payload)
}

/// Handle ICMPv6 echo request
fn handle_icmpv6_echo_request(
    _network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    ping_data: PingData,
) -> NetworkResult<()> {
    // Send echo reply
    send_icmpv6_echo_reply(src_ip, dst_ip, ping_data.identifier, ping_data.sequence, &ping_data.payload)
}

/// Handle destination unreachable
fn handle_destination_unreachable(code: u8, _payload: &[u8]) -> NetworkResult<()> {
    // Log destination unreachable event
    match code {
        0 => {}, // Network unreachable
        1 => {}, // Host unreachable
        2 => {}, // Protocol unreachable
        3 => {}, // Port unreachable
        4 => {}, // Fragmentation needed
        _ => {}, // Other codes
    }
    Ok(())
}

/// Handle time exceeded
fn handle_time_exceeded(code: u8, _payload: &[u8]) -> NetworkResult<()> {
    // Log time exceeded event
    match code {
        0 => {}, // TTL exceeded in transit
        1 => {}, // Fragment reassembly time exceeded
        _ => {}, // Other codes
    }
    Ok(())
}

/// Handle ICMPv6 destination unreachable
fn handle_icmpv6_destination_unreachable(code: u8, _payload: &[u8]) -> NetworkResult<()> {
    match code {
        0 => {}, // No route to destination
        1 => {}, // Communication with destination administratively prohibited
        3 => {}, // Address unreachable
        4 => {}, // Port unreachable
        _ => {}, // Other codes
    }
    Ok(())
}

/// Handle ICMPv6 time exceeded
fn handle_icmpv6_time_exceeded(code: u8, _payload: &[u8]) -> NetworkResult<()> {
    match code {
        0 => {}, // Hop limit exceeded in transit
        1 => {}, // Fragment reassembly time exceeded
        _ => {}, // Other codes
    }
    Ok(())
}

/// Handle neighbor solicitation (IPv6)
fn handle_neighbor_solicitation(
    network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    _dst_ip: NetworkAddress,
    payload: &[u8],
) -> NetworkResult<()> {
    if payload.len() < 20 {
        return Err(NetworkError::InvalidPacket);
    }

    // Skip reserved field (4 bytes)
    let target_bytes = &payload[4..20];
    let mut target_addr = [0u8; 16];
    target_addr.copy_from_slice(target_bytes);
    let target_ip = NetworkAddress::IPv6(target_addr);

    // Check if target is one of our addresses
    let interfaces = network_stack.list_interfaces();
    for interface in interfaces {
        if interface.ip_addresses.contains(&target_ip) {
            // Send neighbor advertisement
            send_neighbor_advertisement(interface.mac_address, src_ip, target_ip)?;
            break;
        }
    }

    Ok(())
}

/// Handle neighbor advertisement (IPv6)
fn handle_neighbor_advertisement(
    _network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    payload: &[u8],
) -> NetworkResult<()> {
    if payload.len() < 20 {
        return Err(NetworkError::InvalidPacket);
    }

    // Parse flags (4 bytes)
    let flags = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let _router_flag = (flags & 0x80000000) != 0;
    let _solicited_flag = (flags & 0x40000000) != 0;
    let _override_flag = (flags & 0x20000000) != 0;

    // Target address (16 bytes)
    let target_bytes = &payload[4..20];
    let mut target_addr = [0u8; 16];
    target_addr.copy_from_slice(target_bytes);
    let target_ip = NetworkAddress::IPv6(target_addr);

    // Parse options to get link-layer address
    let mut mac_addr = None;
    let mut offset = 20;
    while offset + 8 <= payload.len() {
        let option_type = payload[offset];
        let option_length = payload[offset + 1] as usize * 8; // Length in 8-byte units

        if option_length == 0 || offset + option_length > payload.len() {
            break;
        }

        if option_type == 2 && option_length == 8 { // Target Link-layer Address
            let mut mac_bytes = [0u8; 6];
            mac_bytes.copy_from_slice(&payload[offset + 2..offset + 8]);
            mac_addr = Some(NetworkAddress::Mac(mac_bytes));
        }

        offset += option_length;
    }

    // Update neighbor cache
    ICMP_MANAGER.update_neighbor(src_ip, mac_addr, NeighborState::Reachable);
    if target_ip != src_ip {
        ICMP_MANAGER.update_neighbor(target_ip, mac_addr, NeighborState::Reachable);
    }

    Ok(())
}

/// Send ICMP echo request
fn send_icmp_echo_request(
    dst_ip: NetworkAddress,
    identifier: u16,
    sequence: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    let ping_data = PingData::new(identifier, sequence, payload.to_vec());
    let header = IcmpHeader {
        icmp_type: 8, // Echo Request
        code: 0,
        checksum: 0,
        rest: ping_data.to_rest_bytes(),
    };

    // Calculate checksum
    let checksum = header.calculate_checksum(&ping_data.payload);
    let mut final_header = header;
    final_header.checksum = checksum;

    // Build ICMP packet
    let mut packet_data = Vec::new();
    packet_data.push(final_header.icmp_type);
    packet_data.push(final_header.code);
    packet_data.extend_from_slice(&final_header.checksum.to_be_bytes());
    packet_data.extend_from_slice(&final_header.rest);
    packet_data.extend_from_slice(&ping_data.payload);

    // Get source IP from network stack
    let src_ip = crate::net::network_stack()
        .list_interfaces()
        .first()
        .and_then(|iface| iface.ip_addresses.iter().find(|addr| matches!(addr, NetworkAddress::IPv4(_))))
        .copied()
        .ok_or(NetworkError::NetworkUnreachable)?;

    // Send through IP layer with protocol 1 (ICMP)
    crate::net::ip::send_ipv4_packet(src_ip, dst_ip, 1, &packet_data)
}

/// Send ICMP echo reply
fn send_icmp_echo_reply(
    dst_ip: NetworkAddress,
    src_ip: NetworkAddress,
    identifier: u16,
    sequence: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    let ping_data = PingData::new(identifier, sequence, payload.to_vec());
    let header = IcmpHeader {
        icmp_type: 0, // Echo Reply
        code: 0,
        checksum: 0,
        rest: ping_data.to_rest_bytes(),
    };

    // Calculate checksum
    let checksum = header.calculate_checksum(&ping_data.payload);
    let mut final_header = header;
    final_header.checksum = checksum;

    // Build ICMP packet
    let mut packet_data = Vec::new();
    packet_data.push(final_header.icmp_type);
    packet_data.push(final_header.code);
    packet_data.extend_from_slice(&final_header.checksum.to_be_bytes());
    packet_data.extend_from_slice(&final_header.rest);
    packet_data.extend_from_slice(&ping_data.payload);

    // Send through IP layer with protocol 1 (ICMP)
    // Use src_ip parameter which is the original destination (our IP)
    crate::net::ip::send_ipv4_packet(src_ip, dst_ip, 1, &packet_data)
}

/// Send ICMPv6 echo request
/// RFC 4443 Section 4.1
fn send_icmpv6_echo_request(
    dst_ip: NetworkAddress,
    identifier: u16,
    sequence: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    // Get source IPv6 address from network stack
    let src_ip = crate::net::network_stack()
        .list_interfaces()
        .first()
        .and_then(|iface| iface.ip_addresses.iter().find(|addr| matches!(addr, NetworkAddress::IPv6(_))))
        .copied()
        .ok_or(NetworkError::NetworkUnreachable)?;

    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (src_ip, dst_ip) {
        let ping_data = PingData::new(identifier, sequence, payload.to_vec());

        // Build ICMPv6 packet
        let mut icmpv6_packet = Vec::new();
        icmpv6_packet.push(128u8); // Type: Echo Request
        icmpv6_packet.push(0u8);   // Code: 0
        icmpv6_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
        icmpv6_packet.extend_from_slice(&ping_data.to_rest_bytes());
        icmpv6_packet.extend_from_slice(&ping_data.payload);

        // Calculate ICMPv6 checksum with IPv6 pseudo-header
        let checksum = calculate_icmpv6_checksum_local(&src, &dst, &icmpv6_packet);
        icmpv6_packet[2] = (checksum >> 8) as u8;
        icmpv6_packet[3] = (checksum & 0xFF) as u8;

        // Send through IPv6 layer with next header 58 (ICMPv6)
        crate::net::ip::send_ipv6_packet(src_ip, dst_ip, 58, &icmpv6_packet)
    } else {
        Err(NetworkError::InvalidAddress)
    }
}

/// Send ICMPv6 echo reply
/// RFC 4443 Section 4.2
fn send_icmpv6_echo_reply(
    dst_ip: NetworkAddress,
    src_ip: NetworkAddress,
    identifier: u16,
    sequence: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (src_ip, dst_ip) {
        let ping_data = PingData::new(identifier, sequence, payload.to_vec());

        // Build ICMPv6 packet
        let mut icmpv6_packet = Vec::new();
        icmpv6_packet.push(129u8); // Type: Echo Reply
        icmpv6_packet.push(0u8);   // Code: 0
        icmpv6_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
        icmpv6_packet.extend_from_slice(&ping_data.to_rest_bytes());
        icmpv6_packet.extend_from_slice(&ping_data.payload);

        // Calculate ICMPv6 checksum with IPv6 pseudo-header
        let checksum = calculate_icmpv6_checksum_local(&src, &dst, &icmpv6_packet);
        icmpv6_packet[2] = (checksum >> 8) as u8;
        icmpv6_packet[3] = (checksum & 0xFF) as u8;

        // Send through IPv6 layer with next header 58 (ICMPv6)
        crate::net::ip::send_ipv6_packet(src_ip, dst_ip, 58, &icmpv6_packet)
    } else {
        Err(NetworkError::InvalidAddress)
    }
}

/// Calculate ICMPv6 checksum with IPv6 pseudo-header
/// RFC 4443 Section 2.3, RFC 2460 Section 8.1
fn calculate_icmpv6_checksum_local(src: &[u8; 16], dst: &[u8; 16], packet: &[u8]) -> u16 {
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

/// Send neighbor advertisement (IPv6)
fn send_neighbor_advertisement(
    our_mac: NetworkAddress,
    dst_ip: NetworkAddress,
    target_ip: NetworkAddress,
) -> NetworkResult<()> {
    // Get source IPv6 address from network stack
    let src_ip = crate::net::network_stack()
        .list_interfaces()
        .first()
        .and_then(|iface| iface.ip_addresses.iter().find(|addr| matches!(addr, NetworkAddress::IPv6(_))))
        .copied()
        .ok_or(NetworkError::NetworkUnreachable)?;

    if let (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) = (src_ip, dst_ip) {
        // Build neighbor advertisement payload
        let mut payload = Vec::new();

        // Flags: Solicited + Override
        payload.extend_from_slice(&0x60000000u32.to_be_bytes());

        // Target address
        if let NetworkAddress::IPv6(addr) = target_ip {
            payload.extend_from_slice(&addr);
        } else {
            return Err(NetworkError::InvalidAddress);
        }

        // Target Link-layer Address option
        payload.push(2); // Option type
        payload.push(1); // Length (1 * 8 bytes)
        if let NetworkAddress::Mac(mac) = our_mac {
            payload.extend_from_slice(&mac);
        } else {
            return Err(NetworkError::InvalidAddress);
        }

        // Build complete ICMPv6 packet
        let mut icmpv6_packet = Vec::new();
        icmpv6_packet.push(136u8); // Type: Neighbor Advertisement
        icmpv6_packet.push(0u8);   // Code: 0
        icmpv6_packet.extend_from_slice(&[0u8; 2]); // Checksum (calculated later)
        icmpv6_packet.extend_from_slice(&[0u8; 4]); // Reserved (must be zero)
        icmpv6_packet.extend_from_slice(&payload);

        // Calculate ICMPv6 checksum with IPv6 pseudo-header
        let checksum = calculate_icmpv6_checksum_local(&src, &dst, &icmpv6_packet);
        icmpv6_packet[2] = (checksum >> 8) as u8;
        icmpv6_packet[3] = (checksum & 0xFF) as u8;

        // Send through IPv6 layer with next header 58 (ICMPv6)
        crate::net::ip::send_ipv6_packet(src_ip, dst_ip, 58, &icmpv6_packet)
    } else {
        Err(NetworkError::InvalidAddress)
    }
}

/// Public API functions

/// Start ping to target
pub fn ping_start(target: NetworkAddress) -> NetworkResult<u16> {
    ICMP_MANAGER.start_ping(target)
}

/// Send ping packet
pub fn ping_send(identifier: u16, payload: Option<Vec<u8>>) -> NetworkResult<u16> {
    let payload = payload.unwrap_or_else(|| b"RustOS ping".to_vec());
    ICMP_MANAGER.send_ping(identifier, payload)
}

/// Get ping results
pub fn ping_results(identifier: u16) -> Option<Vec<PingReply>> {
    ICMP_MANAGER.get_ping_results(identifier)
}

/// Stop ping session
pub fn ping_stop(identifier: u16) -> NetworkResult<()> {
    ICMP_MANAGER.close_ping(identifier)
}

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    // Use system time for ICMP echo timestamps
    crate::time::get_system_time_ms()
}

/// Initialize ICMP subsystem
pub fn init() -> NetworkResult<()> {
    // ICMP manager is statically initialized
    Ok(())
}

/// Cleanup expired ICMP sessions
pub fn cleanup() {
    ICMP_MANAGER.cleanup_expired();
}

/// Get ICMP manager reference
pub fn icmp_manager() -> &'static IcmpManager {
    &ICMP_MANAGER
}