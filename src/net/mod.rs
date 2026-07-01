//! Network stack implementation for RustOS
//!
//! This module provides a complete TCP/IP networking stack including:
//! - Ethernet frame handling
//! - IP packet processing (IPv4/IPv6)
//! - TCP connection management
//! - UDP datagram handling
//! - Socket interface
//! - Network device abstraction

pub mod arp;
pub mod buffer;
pub mod device;
pub mod dhcp;
pub mod dma;
pub mod dns;
pub mod ethernet;
pub mod icmp;
pub mod ip;
pub mod netfilter;
pub mod quic;
pub mod raw;
pub mod routing;
pub mod socket;
pub mod tcp;
pub mod udp;
pub mod unix;

// Linux-mirror network subsystems
pub mod atm;
pub mod batman_adv;
pub mod bluetooth;
pub mod bpf;
pub mod bridge;
pub mod can;
pub mod ceph;
pub mod core;
pub mod dcb;
pub mod devlink;
pub mod dns_resolver;
pub mod dsa;
pub mod eight02;
pub mod eight021q;
pub mod ethtool;
pub mod handshake;
pub mod hsr;
pub mod ieee802154;
pub mod ife;
pub mod ipv4;
pub mod ipv6;
pub mod iucv;
pub mod kcm;
pub mod key;
pub mod l2tp;
pub mod l3mdev;
pub mod lapb;
pub mod llc;
pub mod mac80211;
pub mod mac802154;
pub mod mctp;
pub mod mpls;
pub mod mptcp;
pub mod ncsi;
pub mod netlabel;
pub mod netlink;
pub mod nfc;
pub mod ninep;
pub mod nsh;
pub mod openvswitch;
pub mod packet;
pub mod phonet;
pub mod psample;
pub mod psp;
pub mod qrtr;
pub mod rds;
pub mod rfkill;
pub mod rxrpc;
pub mod sched;
pub mod sctp;
pub mod shaper;
pub mod sixlowpan;
pub mod smc;
pub mod strparser;
pub mod sunrpc;
pub mod switchdev;
pub mod tipc;
pub mod tls;
pub mod vmw_vsock;
pub mod wireless;
pub mod x25;
pub mod xdp;
pub mod xfrm;

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

/// Type alias for IPv4 address as a 4-byte array
pub type Ipv4Address = [u8; 4];

/// Type alias for MAC address as a 6-byte array
pub type MacAddress = [u8; 6];

/// Type alias for IPv6 address as a 16-byte array
pub type Ipv6Address = [u8; 16];

/// Network address types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetworkAddress {
    /// IPv4 address
    IPv4([u8; 4]),
    /// IPv6 address
    IPv6([u8; 16]),
    /// MAC address
    Mac([u8; 6]),
}

impl NetworkAddress {
    /// Create IPv4 address from octets
    pub fn ipv4(a: u8, b: u8, c: u8, d: u8) -> Self {
        NetworkAddress::IPv4([a, b, c, d])
    }

    /// Create MAC address from bytes
    pub fn mac(bytes: [u8; 6]) -> Self {
        NetworkAddress::Mac(bytes)
    }

    /// Check if address is broadcast
    pub fn is_broadcast(&self) -> bool {
        match self {
            NetworkAddress::IPv4([255, 255, 255, 255]) => true,
            NetworkAddress::Mac([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]) => true,
            _ => false,
        }
    }

    /// Check if address is multicast
    pub fn is_multicast(&self) -> bool {
        match self {
            NetworkAddress::IPv4([a, _, _, _]) => (*a & 0xf0) == 0xe0,
            NetworkAddress::IPv6([a, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _]) => {
                (*a & 0xff) == 0xff
            }
            NetworkAddress::Mac([a, _, _, _, _, _]) => (*a & 0x01) != 0,
        }
    }
}

impl ::core::fmt::Display for NetworkAddress {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        match self {
            NetworkAddress::IPv4([a, b, c, d]) => write!(f, "{}.{}.{}.{}", a, b, c, d),
            NetworkAddress::IPv6(bytes) => {
                write!(f, "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3],
                    bytes[4], bytes[5], bytes[6], bytes[7],
                    bytes[8], bytes[9], bytes[10], bytes[11],
                    bytes[12], bytes[13], bytes[14], bytes[15])
            }
            NetworkAddress::Mac(bytes) => {
                write!(
                    f,
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                )
            }
        }
    }
}

/// Network protocol types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Protocol {
    /// Internet Control Message Protocol
    ICMP = 1,
    /// Transmission Control Protocol
    TCP = 6,
    /// User Datagram Protocol
    UDP = 17,
    /// IPv6 in IPv4
    IPv6inIPv4 = 41,
    /// Generic Routing Encapsulation
    GRE = 47,
    /// IPv6 Internet Control Message Protocol
    ICMPv6 = 58,
}

/// Compute the Internet checksum (RFC 1071) over `data`.
///
/// This is the standard ones-complement sum of all 16-bit big-endian
/// words: the running 32-bit sum is folded back into 16 bits (end-around
/// carry) and the final ones-complement is returned. If `data` has an odd
/// length, the trailing byte is treated as the high octet of a final
/// 16-bit word whose low octet is zero, as required by RFC 1071 Section 3.
///
/// All IP header, ICMP, ICMPv6 and UDP checksums in the stack are derived
/// from this single primitive so that the checksum field itself is the only
/// thing that must be zeroed by callers before invocation.
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;

    for chunk in data.chunks(2) {
        if chunk.len() == 2 {
            sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
        } else {
            // Odd trailing byte: pad with a zero low octet (network byte order).
            sum += (chunk[0] as u32) << 8;
        }
    }

    // Fold 32-bit sum back into 16 bits (end-around carry).
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

/// Network packet buffer
#[derive(Debug, Clone)]
pub struct PacketBuffer {
    /// Raw packet data
    pub data: Vec<u8>,
    /// Current position in buffer
    pub position: usize,
    /// Packet length
    pub length: usize,
}

impl PacketBuffer {
    /// Create a new packet buffer
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0; capacity],
            position: 0,
            length: 0,
        }
    }

    /// Create packet buffer from existing data
    pub fn from_data(data: Vec<u8>) -> Self {
        let length = data.len();
        Self {
            data,
            position: 0,
            length,
        }
    }

    /// Get remaining bytes in buffer
    pub fn remaining(&self) -> usize {
        self.length.saturating_sub(self.position)
    }

    /// Read bytes from buffer
    pub fn read(&mut self, count: usize) -> Option<&[u8]> {
        if self.position + count <= self.length {
            let start = self.position;
            self.position += count;
            Some(&self.data[start..self.position])
        } else {
            None
        }
    }

    /// Write bytes to buffer
    pub fn write(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        if self.position + data.len() > self.data.len() {
            return Err(NetworkError::BufferOverflow);
        }

        self.data[self.position..self.position + data.len()].copy_from_slice(data);
        self.position += data.len();
        self.length = self.length.max(self.position);
        Ok(())
    }

    /// Reset buffer position
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get slice of current data
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.length]
    }
}

/// Network error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    /// Invalid packet format
    InvalidPacket,
    /// Buffer overflow
    BufferOverflow,
    /// Network unreachable
    NetworkUnreachable,
    /// Host unreachable
    HostUnreachable,
    /// Port unreachable
    PortUnreachable,
    /// Connection refused
    ConnectionRefused,
    /// Connection timeout
    Timeout,
    /// Connection reset
    ConnectionReset,
    /// Invalid address
    InvalidAddress,
    /// Operation not supported
    NotSupported,
    /// Resource busy
    Busy,
    /// No route to host
    NoRoute,
    /// Address already in use
    AddressInUse,
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
    /// Hardware error
    HardwareError,
    /// Invalid state
    InvalidState,
    /// Insufficient memory
    InsufficientMemory,
    /// Buffer too small
    BufferTooSmall,
    /// Protocol error
    ProtocolError,
    /// Not connected
    NotConnected,
    /// Not implemented
    NotImplemented,
    /// Internal error
    InternalError,
    /// Resource not found
    NotFound,
}

impl ::core::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        match self {
            NetworkError::InvalidPacket => write!(f, "Invalid packet format"),
            NetworkError::BufferOverflow => write!(f, "Buffer overflow"),
            NetworkError::NetworkUnreachable => write!(f, "Network unreachable"),
            NetworkError::HostUnreachable => write!(f, "Host unreachable"),
            NetworkError::PortUnreachable => write!(f, "Port unreachable"),
            NetworkError::ConnectionRefused => write!(f, "Connection refused"),
            NetworkError::Timeout => write!(f, "Connection timeout"),
            NetworkError::ConnectionReset => write!(f, "Connection reset"),
            NetworkError::InvalidAddress => write!(f, "Invalid address"),
            NetworkError::NotSupported => write!(f, "Operation not supported"),
            NetworkError::Busy => write!(f, "Resource busy"),
            NetworkError::NoRoute => write!(f, "No route to host"),
            NetworkError::AddressInUse => write!(f, "Address already in use"),
            NetworkError::PermissionDenied => write!(f, "Permission denied"),
            NetworkError::InvalidArgument => write!(f, "Invalid argument"),
            NetworkError::HardwareError => write!(f, "Hardware error"),
            NetworkError::InvalidState => write!(f, "Invalid state"),
            NetworkError::InsufficientMemory => write!(f, "Insufficient memory"),
            NetworkError::BufferTooSmall => write!(f, "Buffer too small"),
            NetworkError::ProtocolError => write!(f, "Protocol error"),
            NetworkError::NotConnected => write!(f, "Not connected"),
            NetworkError::NotImplemented => write!(f, "Not implemented"),
            NetworkError::InternalError => write!(f, "Internal error"),
            NetworkError::NotFound => write!(f, "Resource not found"),
        }
    }
}

/// Network result type
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Network interface configuration
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    /// Interface name
    pub name: String,
    /// MAC address
    pub mac_address: NetworkAddress,
    /// IP addresses assigned to this interface
    pub ip_addresses: Vec<NetworkAddress>,
    /// Network mask for the primary IP address
    pub netmask: NetworkAddress,
    /// Maximum transmission unit
    pub mtu: u16,
    /// Interface flags
    pub flags: InterfaceFlags,
    /// Interface statistics
    pub stats: InterfaceStats,
}

/// Interface flags
#[derive(Debug, Clone, Copy)]
pub struct InterfaceFlags {
    /// Interface is up
    pub up: bool,
    /// Interface supports broadcast
    pub broadcast: bool,
    /// Interface is loopback
    pub loopback: bool,
    /// Interface supports multicast
    pub multicast: bool,
    /// Interface is point-to-point
    pub point_to_point: bool,
}

impl Default for InterfaceFlags {
    fn default() -> Self {
        Self {
            up: false,
            broadcast: true,
            loopback: false,
            multicast: true,
            point_to_point: false,
        }
    }
}

/// Interface statistics
#[derive(Debug, Clone, Default)]
pub struct InterfaceStats {
    /// Packets received
    pub rx_packets: u64,
    /// Bytes received
    pub rx_bytes: u64,
    /// Receive errors
    pub rx_errors: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Transmit errors
    pub tx_errors: u64,
    /// Packets dropped
    pub dropped: u64,
}

pub use routing::RouteEntry;

/// Network stack manager
pub struct NetworkStack {
    /// Network interfaces
    interfaces: RwLock<BTreeMap<String, NetworkInterface>>,
    /// IPv4 routing table
    routing_table: routing::RoutingTable,
    /// ARP table (IP -> MAC mapping)
    arp_table: RwLock<BTreeMap<NetworkAddress, NetworkAddress>>,
    /// Socket registry
    sockets: RwLock<BTreeMap<u32, socket::Socket>>,
    /// Next socket ID
    next_socket_id: Mutex<u32>,
}

impl NetworkStack {
    /// Create a new network stack
    pub fn new() -> Self {
        Self {
            interfaces: RwLock::new(BTreeMap::new()),
            routing_table: routing::RoutingTable::new(),
            arp_table: RwLock::new(BTreeMap::new()),
            sockets: RwLock::new(BTreeMap::new()),
            next_socket_id: Mutex::new(1),
        }
    }

    /// Add a network interface
    pub fn add_interface(&self, interface: NetworkInterface) -> NetworkResult<()> {
        let mut interfaces = self.interfaces.write();

        if interfaces.contains_key(&interface.name) {
            return Err(NetworkError::AddressInUse);
        }

        interfaces.insert(interface.name.clone(), interface);
        Ok(())
    }

    /// Remove a network interface
    pub fn remove_interface(&self, name: &str) -> NetworkResult<()> {
        let mut interfaces = self.interfaces.write();

        if interfaces.remove(name).is_some() {
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Get network interface by name
    pub fn get_interface(&self, name: &str) -> Option<NetworkInterface> {
        let interfaces = self.interfaces.read();
        interfaces.get(name).cloned()
    }

    /// List all network interfaces
    pub fn list_interfaces(&self) -> Vec<NetworkInterface> {
        let interfaces = self.interfaces.read();
        interfaces.values().cloned().collect()
    }

    /// Return the number of registered network interfaces without iterating
    /// or cloning entries.
    pub fn interface_count(&self) -> usize {
        self.interfaces.read().len()
    }

    /// Set interface up/down
    pub fn set_interface_state(&self, name: &str, up: bool) -> NetworkResult<()> {
        let mut interfaces = self.interfaces.write();

        if let Some(interface) = interfaces.get_mut(name) {
            interface.flags.up = up;
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Add IP address to interface
    pub fn add_ip_address(
        &self,
        interface_name: &str,
        address: NetworkAddress,
    ) -> NetworkResult<()> {
        let mut interfaces = self.interfaces.write();

        if let Some(interface) = interfaces.get_mut(interface_name) {
            if !interface.ip_addresses.contains(&address) {
                interface.ip_addresses.push(address);
            }
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Add route to routing table
    pub fn add_route(&self, route: RouteEntry) -> NetworkResult<()> {
        self.routing_table.add(route)
    }

    /// Find route for destination address with longest prefix matching
    pub fn find_route(&self, destination: &NetworkAddress) -> Option<RouteEntry> {
        self.routing_table.find(destination)
    }

    /// Delete a route from the routing table.
    pub fn delete_route(
        &self,
        destination: &NetworkAddress,
        netmask: &NetworkAddress,
        interface: &str,
    ) -> NetworkResult<()> {
        self.routing_table.delete(destination, netmask, interface)
    }

    /// List all routes in the routing table.
    pub fn list_routes(&self) -> alloc::vec::Vec<RouteEntry> {
        self.routing_table.list()
    }

    /// Handle a netlink-style route request.
    pub fn handle_route_request(
        &self,
        req: routing::RouteRequest,
    ) -> NetworkResult<alloc::vec::Vec<RouteEntry>> {
        self.routing_table.handle_request(
            req,
            |iface| self.interfaces.read().contains_key(iface),
            |iface, gw| self.gateway_reachable_on_interface(iface, gw),
        )
    }

    /// Check whether `gateway` is reachable via `interface`.
    pub fn gateway_reachable_on_interface(
        &self,
        interface: &str,
        gateway: &NetworkAddress,
    ) -> bool {
        let interfaces = self.interfaces.read();
        if let Some(iface) = interfaces.get(interface) {
            for interface_ip in &iface.ip_addresses {
                if self.is_same_subnet(interface_ip, gateway) {
                    return true;
                }
            }
        }
        false
    }

    /// Reference to the routing table (for ioctl handlers).
    pub fn routing_table(&self) -> &routing::RoutingTable {
        &self.routing_table
    }

    /// Update ARP table with real address resolution
    pub fn update_arp(&self, ip: NetworkAddress, mac: NetworkAddress) {
        // Update local ARP table
        let mut arp_table = self.arp_table.write();
        arp_table.insert(ip, mac);

        // Also update enhanced ARP module
        arp::update_arp_entry(ip, mac, "default".to_string()).ok();
    }

    /// Lookup MAC address for IP with real resolution
    pub fn lookup_arp(&self, ip: &NetworkAddress) -> Option<NetworkAddress> {
        // First check local cache
        {
            let arp_table = self.arp_table.read();
            if let Some(mac) = arp_table.get(ip) {
                return Some(*mac);
            }
        }

        // Check enhanced ARP module
        if let Some(mac) = arp::lookup_arp(ip) {
            // Cache the result locally
            self.update_arp(*ip, mac);
            return Some(mac);
        }

        // If not found, initiate ARP request for IPv4 addresses
        match ip {
            NetworkAddress::IPv4(_) => {
                self.send_arp_request(ip).ok();
                None
            }
            _ => None, // IPv6 uses Neighbor Discovery Protocol
        }
    }

    /// Send ARP request for address resolution
    pub fn send_arp_request(&self, target_ip: &NetworkAddress) -> NetworkResult<()> {
        // Find appropriate interface for this IP
        let interface_name = self.find_interface_for_ip(target_ip)?;

        // Get interface details
        let interfaces = self.interfaces.read();
        let interface = interfaces
            .get(&interface_name)
            .ok_or(NetworkError::InvalidAddress)?;

        let src_mac = interface.mac_address;
        let src_ip = *interface
            .ip_addresses
            .first()
            .ok_or(NetworkError::InvalidAddress)?;

        drop(interfaces);

        // Create ARP request packet
        let arp_packet = self.create_arp_request_packet(src_mac, src_ip, *target_ip)?;

        // Send through interface
        self.send_packet(&interface_name, arp_packet)?;

        Ok(())
    }

    /// Find interface that can reach the given IP address
    fn find_interface_for_ip(&self, target_ip: &NetworkAddress) -> NetworkResult<String> {
        let interfaces = self.interfaces.read();

        // First, check if target is on same subnet as any interface
        for (name, interface) in interfaces.iter() {
            if !interface.flags.up {
                continue;
            }

            for interface_ip in &interface.ip_addresses {
                if self.is_same_subnet(interface_ip, target_ip) {
                    return Ok(name.clone());
                }
            }
        }

        // If not on same subnet, use routing table longest-prefix match
        if let Some(route) = self.routing_table.find(target_ip) {
            return Ok(route.interface.clone());
        }

        // Fallback to first up interface
        for (name, interface) in interfaces.iter() {
            if interface.flags.up {
                return Ok(name.clone());
            }
        }

        Err(NetworkError::NoRoute)
    }

    /// Check if two IP addresses are on the same subnet
    fn is_same_subnet(&self, ip1: &NetworkAddress, ip2: &NetworkAddress) -> bool {
        match (ip1, ip2) {
            (NetworkAddress::IPv4(a), NetworkAddress::IPv4(b)) => {
                // Use routing table to find the netmask for this interface
                // Fall back to /24 (255.255.255.0) if no route found
                let routes = self.routing_table.list();
                let netmask = routes
                    .iter()
                    .find(|r| {
                        if let (NetworkAddress::IPv4(dest), NetworkAddress::IPv4(mask)) =
                            (&r.destination, &r.netmask)
                        {
                            (a[0] & mask[0]) == dest[0]
                                && (a[1] & mask[1]) == dest[1]
                                && (a[2] & mask[2]) == dest[2]
                                && (a[3] & mask[3]) == dest[3]
                        } else {
                            false
                        }
                    })
                    .map(|r| {
                        if let NetworkAddress::IPv4(m) = r.netmask {
                            m
                        } else {
                            [255, 255, 255, 0]
                        }
                    })
                    .unwrap_or([255, 255, 255, 0]);

                (a[0] & netmask[0]) == (b[0] & netmask[0])
                    && (a[1] & netmask[1]) == (b[1] & netmask[1])
                    && (a[2] & netmask[2]) == (b[2] & netmask[2])
                    && (a[3] & netmask[3]) == (b[3] & netmask[3])
            }
            _ => false,
        }
    }

    /// Create ARP request packet
    fn create_arp_request_packet(
        &self,
        src_mac: NetworkAddress,
        src_ip: NetworkAddress,
        target_ip: NetworkAddress,
    ) -> NetworkResult<PacketBuffer> {
        let mut packet_data = Vec::new();

        // Ethernet header
        packet_data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Broadcast MAC
        if let NetworkAddress::Mac(mac_bytes) = src_mac {
            packet_data.extend_from_slice(&mac_bytes);
        }
        packet_data.extend_from_slice(&[0x08, 0x06]); // ARP EtherType

        // ARP header
        packet_data.extend_from_slice(&[0x00, 0x01]); // Hardware type (Ethernet)
        packet_data.extend_from_slice(&[0x08, 0x00]); // Protocol type (IPv4)
        packet_data.push(6); // Hardware address length
        packet_data.push(4); // Protocol address length
        packet_data.extend_from_slice(&[0x00, 0x01]); // Operation (request)

        // Sender hardware address
        if let NetworkAddress::Mac(mac_bytes) = src_mac {
            packet_data.extend_from_slice(&mac_bytes);
        }

        // Sender protocol address
        if let NetworkAddress::IPv4(ip_bytes) = src_ip {
            packet_data.extend_from_slice(&ip_bytes);
        }

        // Target hardware address (unknown, all zeros)
        packet_data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // Target protocol address
        if let NetworkAddress::IPv4(ip_bytes) = target_ip {
            packet_data.extend_from_slice(&ip_bytes);
        }

        Ok(PacketBuffer::from_data(packet_data))
    }

    /// Create a new socket
    pub fn create_socket(
        &self,
        socket_type: socket::SocketType,
        protocol: Protocol,
    ) -> NetworkResult<u32> {
        let socket_id = {
            let mut next_id = self.next_socket_id.lock();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let socket = socket::Socket::new(socket_id, socket_type, protocol);

        let mut sockets = self.sockets.write();
        sockets.insert(socket_id, socket);

        Ok(socket_id)
    }

    /// Close a socket
    pub fn close_socket(&self, socket_id: u32) -> NetworkResult<()> {
        let mut sockets = self.sockets.write();

        if sockets.remove(&socket_id).is_some() {
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Get socket by ID
    pub fn get_socket(&self, socket_id: u32) -> Option<socket::Socket> {
        let sockets = self.sockets.read();
        sockets.get(&socket_id).cloned()
    }

    /// Update an existing socket in place
    pub fn update_socket(&self, socket_id: u32, socket: socket::Socket) -> NetworkResult<()> {
        let mut sockets = self.sockets.write();
        if sockets.contains_key(&socket_id) {
            sockets.insert(socket_id, socket);
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Process incoming packet
    pub fn process_packet(&self, interface_name: &str, packet: PacketBuffer) -> NetworkResult<()> {
        // Update interface statistics
        {
            let mut interfaces = self.interfaces.write();
            if let Some(interface) = interfaces.get_mut(interface_name) {
                interface.stats.rx_packets += 1;
                interface.stats.rx_bytes += packet.length as u64;
            }
        }

        // Process Ethernet frame
        ethernet::process_frame(self, interface_name, packet)
    }

    /// Send packet through interface
    pub fn send_packet(&self, interface_name: &str, packet: PacketBuffer) -> NetworkResult<()> {
        let interfaces = self.interfaces.read();
        let interface = interfaces
            .get(interface_name)
            .ok_or(NetworkError::InvalidAddress)?;

        if !interface.flags.up {
            return Err(NetworkError::NetworkUnreachable);
        }

        // Validate packet before transmission
        let packet_data = packet.as_slice();
        if packet_data.is_empty() {
            return Err(NetworkError::InvalidPacket);
        }

        // Check MTU
        if packet_data.len() > interface.mtu as usize {
            return Err(NetworkError::BufferOverflow);
        }

        let packet_len = packet_data.len();

        drop(interfaces);

        // Send through device manager
        let result = device::device_manager().send_packet(interface_name, packet);

        // Update interface statistics
        {
            let mut interfaces = self.interfaces.write();
            if let Some(interface) = interfaces.get_mut(interface_name) {
                match &result {
                    Ok(_) => {
                        interface.stats.tx_packets += 1;
                        interface.stats.tx_bytes += packet_len as u64;
                    }
                    Err(_) => {
                        interface.stats.tx_errors += 1;
                    }
                }
            }
        }

        result
    }

    /// Get network statistics
    pub fn get_stats(&self) -> NetworkStats {
        let interfaces = self.interfaces.read();
        let sockets = self.sockets.read();
        let routes = self.routing_table.len();

        let mut total_rx_packets = 0;
        let mut total_rx_bytes = 0;
        let mut total_tx_packets = 0;
        let mut total_tx_bytes = 0;

        for interface in interfaces.values() {
            total_rx_packets += interface.stats.rx_packets;
            total_rx_bytes += interface.stats.rx_bytes;
            total_tx_packets += interface.stats.tx_packets;
            total_tx_bytes += interface.stats.tx_bytes;
        }

        NetworkStats {
            interfaces: interfaces.len(),
            sockets: sockets.len(),
            routes,
            arp_entries: self.arp_table.read().len(),
            total_rx_packets,
            total_rx_bytes,
            total_tx_packets,
            total_tx_bytes,
            packets_sent: total_tx_packets,
            packets_received: total_rx_packets,
            bytes_sent: total_tx_bytes,
            bytes_received: total_rx_bytes,
            send_errors: 0,
            receive_errors: 0,
            dropped_packets: 0,
        }
    }
}

/// Network stack statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub interfaces: usize,
    pub sockets: usize,
    pub routes: usize,
    pub arp_entries: usize,
    pub total_rx_packets: u64,
    pub total_rx_bytes: u64,
    pub total_tx_packets: u64,
    pub total_tx_bytes: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_errors: u64,
    pub receive_errors: u64,
    pub dropped_packets: u64,
}

lazy_static! {
    static ref NETWORK_STACK: NetworkStack = NetworkStack::new();
}

/// Initialize the network stack
pub fn init() -> NetworkResult<()> {
    netfilter::init();

    // Initialize Linux-mirror net subsystems
    sixlowpan::init().ok();
    eight02::init().ok();
    eight021q::init().ok();
    ninep::init().ok();
    atm::init().ok();
    batman_adv::init().ok();
    bluetooth::init().ok();
    bpf::init().ok();
    bridge::init().ok();
    can::init().ok();
    ceph::init().ok();
    core::init().ok();
    dcb::init().ok();
    devlink::init().ok();
    dns_resolver::init().ok();
    dsa::init().ok();
    ethtool::init().ok();
    handshake::init().ok();
    hsr::init().ok();
    ieee802154::init().ok();
    ife::init().ok();
    ipv4::init().ok();
    ipv6::init().ok();
    iucv::init().ok();
    kcm::init().ok();
    key::init().ok();
    l2tp::init().ok();
    l3mdev::init().ok();
    lapb::init().ok();
    llc::init().ok();
    mac80211::init().ok();
    mac802154::init().ok();
    mctp::init().ok();
    mpls::init().ok();
    mptcp::init().ok();
    ncsi::init().ok();
    netlabel::init().ok();
    netlink::init().ok();
    nfc::init().ok();
    nsh::init().ok();
    openvswitch::init().ok();
    packet::init().ok();
    phonet::init().ok();
    psample::init().ok();
    psp::init().ok();
    qrtr::init().ok();
    rds::init().ok();
    rfkill::init().ok();
    rxrpc::init().ok();
    sched::init().ok();
    sctp::init().ok();
    shaper::init().ok();
    smc::init().ok();
    strparser::init().ok();
    sunrpc::init().ok();
    switchdev::init().ok();
    tipc::init().ok();
    tls::init().ok();
    vmw_vsock::init().ok();
    wireless::init().ok();
    x25::init().ok();
    xdp::init().ok();
    xfrm::init().ok();

    // Create loopback interface
    let loopback = NetworkInterface {
        name: "lo".to_string(),
        mac_address: NetworkAddress::mac([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        ip_addresses: vec![NetworkAddress::ipv4(127, 0, 0, 1)],
        netmask: NetworkAddress::ipv4(255, 0, 0, 0),
        mtu: 65535,
        flags: InterfaceFlags {
            up: true,
            broadcast: false,
            loopback: true,
            multicast: false,
            point_to_point: false,
        },
        stats: InterfaceStats::default(),
    };

    NETWORK_STACK.add_interface(loopback)?;
    NETWORK_STACK.set_interface_state("lo", true)?;

    // Add loopback route
    let loopback_route = RouteEntry {
        destination: NetworkAddress::ipv4(127, 0, 0, 0),
        netmask: NetworkAddress::ipv4(255, 0, 0, 0),
        gateway: None,
        interface: "lo".to_string(),
        metric: 0,
    };
    NETWORK_STACK.add_route(loopback_route)?;

    // Load hardware NIC drivers via PCI scanning
    match crate::drivers::network::init_global_network_drivers() {
        Ok(()) => {
            crate::drivers::network::with_network_driver_manager(|mgr| {
                let drivers = mgr.list_drivers();
                for (id, name, dtype) in drivers.iter() {
                    crate::serial_println!("net: driver #{} '{}' ({:?})", id, name, dtype);
                }
            });
        }
        Err(e) => {
            crate::serial_println!("net: hardware driver load failed: {:?}", e);
        }
    }

    Ok(())
}

/// Get the global network stack
pub fn network_stack() -> &'static NetworkStack {
    &NETWORK_STACK
}

/// Poll all network devices for incoming packets and process them through
/// the network stack. Should be called periodically from the main loop.
pub fn poll_network() {
    let packets = device::device_manager().poll_devices();
    for (name, packet) in packets {
        let _ = NETWORK_STACK.process_packet(&name, packet);
    }
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Get aggregate interface statistics.
/// Returns (rx_packets, tx_packets, rx_bytes, tx_bytes)
pub fn get_interface_stats() -> Result<(u64, u64, u64, u64), &'static str> {
    // Get stats from the default interface or aggregate all interfaces
    let stack = network_stack();
    let interfaces = stack.interfaces.read();

    if interfaces.is_empty() {
        return Ok((0, 0, 0, 0));
    }

    // Aggregate stats from all interfaces
    let (mut total_rx_packets, mut total_tx_packets, mut total_rx_bytes, mut total_tx_bytes) =
        (0, 0, 0, 0);

    for interface in interfaces.values() {
        total_rx_packets += interface.stats.rx_packets;
        total_tx_packets += interface.stats.tx_packets;
        total_rx_bytes += interface.stats.rx_bytes;
        total_tx_bytes += interface.stats.tx_bytes;
    }

    Ok((
        total_rx_packets,
        total_tx_packets,
        total_rx_bytes,
        total_tx_bytes,
    ))
}
