//! UDP (User Datagram Protocol) implementation
//!
//! This module provides comprehensive UDP packet processing and socket operations
//! for connectionless datagram communication.
//!
//! # Features
//!
//! - RFC 768 compliant UDP implementation
//! - Socket options: SO_REUSEADDR, SO_REUSEPORT, SO_BROADCAST
//! - Multicast group management (join/leave)
//! - Configurable send/receive buffer sizes and timeouts
//! - Comprehensive statistics tracking and error reporting
//! - Advanced port allocation with collision avoidance
//! - Wildcard binding and connection semantics
//!
//! # Implementation Status
//!
//! Current implementation supports IPv4 only. IPv6 support is planned for future releases.
//! ICMPv6 checksum calculation requires IPv6 pseudo-header implementation.

use super::{NetworkAddress, NetworkError, NetworkResult, NetworkStack, PacketBuffer};
use alloc::{collections::BTreeMap, vec, vec::Vec};
use spin::RwLock;

/// UDP header size
pub const UDP_HEADER_SIZE: usize = 8;

/// UDP header
#[derive(Debug, Clone)]
pub struct UdpHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    /// Parse UDP header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < UDP_HEADER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let src_port_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let source_port = u16::from_be_bytes([src_port_bytes[0], src_port_bytes[1]]);

        let dst_port_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let dest_port = u16::from_be_bytes([dst_port_bytes[0], dst_port_bytes[1]]);

        let length_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]);

        let checksum_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let checksum = u16::from_be_bytes([checksum_bytes[0], checksum_bytes[1]]);

        Ok(UdpHeader {
            source_port,
            dest_port,
            length,
            checksum,
        })
    }

    /// Calculate UDP checksum
    /// RFC 768 (IPv4) and RFC 2460 Section 8.1 (IPv6)
    ///
    /// Computes the ones-complement sum over the IP pseudo-header (source
    /// address, destination address, protocol, UDP length) concatenated
    /// with the UDP header (checksum field zeroed) and the UDP payload.
    /// The arithmetic is delegated to [`crate::net::internet_checksum`].
    /// Per RFC 768, a computed checksum of 0x0000 is transmitted as 0xFFFF
    /// (a zero checksum means "no checksum" for IPv4 and is forbidden for
    /// IPv6, so 0xFFFF is used in both cases).
    pub fn calculate_checksum(
        &self,
        src_ip: &NetworkAddress,
        dst_ip: &NetworkAddress,
        payload: &[u8],
    ) -> u16 {
        // Upper-layer (UDP) length, carried in the pseudo-header.
        let udp_length = self.length;

        // Build the checksum input: pseudo-header + UDP header (checksum=0) + payload.
        let mut data = Vec::with_capacity(40 + UDP_HEADER_SIZE + payload.len());

        match (src_ip, dst_ip) {
            (NetworkAddress::IPv4(src), NetworkAddress::IPv4(dst)) => {
                // IPv4 pseudo-header (12 bytes): src(4) + dst(4) + zero(1) + proto(1) + length(2)
                data.extend_from_slice(src);
                data.extend_from_slice(dst);
                data.push(0x00);
                data.push(17); // Protocol (UDP)
                data.extend_from_slice(&udp_length.to_be_bytes());
            }
            (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) => {
                // IPv6 pseudo-header (40 bytes): src(16) + dst(16) + length(4) + next-header(4)
                data.extend_from_slice(src);
                data.extend_from_slice(dst);
                data.extend_from_slice(&(udp_length as u32).to_be_bytes());
                // Next header: 24 zero bits + 8-bit protocol (UDP = 17).
                data.extend_from_slice(&[0x00, 0x00, 0x00, 17]);
            }
            _ => return 0, // Mixed address families not supported
        }

        // UDP header with the checksum field zeroed during computation.
        data.extend_from_slice(&self.source_port.to_be_bytes());
        data.extend_from_slice(&self.dest_port.to_be_bytes());
        data.extend_from_slice(&udp_length.to_be_bytes());
        data.extend_from_slice(&[0x00, 0x00]); // checksum field

        // UDP payload.
        data.extend_from_slice(payload);

        let result = super::internet_checksum(&data);

        // UDP checksum of 0 means no checksum (only for IPv4, mandatory for IPv6).
        // Per RFC 768, transmit 0xFFFF instead of 0x0000 in both cases.
        if result == 0 {
            0xFFFF
        } else {
            result
        }
    }

    /// Serialize UDP header to buffer
    pub fn serialize(&self, buffer: &mut PacketBuffer) -> NetworkResult<()> {
        buffer.write(&self.source_port.to_be_bytes())?;
        buffer.write(&self.dest_port.to_be_bytes())?;
        buffer.write(&self.length.to_be_bytes())?;
        buffer.write(&self.checksum.to_be_bytes())?;
        Ok(())
    }
}

/// Enhanced UDP socket with advanced features
#[derive(Debug, Clone)]
pub struct UdpSocket {
    pub local_addr: NetworkAddress,
    pub local_port: u16,
    pub remote_addr: Option<NetworkAddress>,
    pub remote_port: Option<u16>,
    pub recv_buffer: Vec<UdpDatagram>,
    pub broadcast: bool,
    pub multicast_groups: Vec<NetworkAddress>,
    pub socket_options: UdpSocketOptions,
    pub statistics: UdpSocketStats,
    pub bind_time: u64,
    pub last_activity: u64,
}

/// UDP socket options
#[derive(Debug, Clone)]
pub struct UdpSocketOptions {
    pub reuse_addr: bool,
    pub reuse_port: bool,
    pub recv_buffer_size: usize,
    pub send_buffer_size: usize,
    pub recv_timeout: Option<u32>,
    pub send_timeout: Option<u32>,
    pub ttl: u8,
    pub multicast_ttl: u8,
    pub multicast_loop: bool,
    pub dscp: u8,
}

impl Default for UdpSocketOptions {
    fn default() -> Self {
        Self {
            reuse_addr: false,
            reuse_port: false,
            recv_buffer_size: 65536,
            send_buffer_size: 65536,
            recv_timeout: None,
            send_timeout: None,
            ttl: 64,
            multicast_ttl: 1,
            multicast_loop: true,
            dscp: 0,
        }
    }
}

/// UDP socket statistics
#[derive(Debug, Clone, Default)]
pub struct UdpSocketStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_errors: u64,
    pub recv_errors: u64,
    pub dropped_packets: u64,
}

/// UDP datagram
#[derive(Debug, Clone)]
pub struct UdpDatagram {
    pub source_addr: NetworkAddress,
    pub source_port: u16,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

impl UdpSocket {
    pub fn new(local_addr: NetworkAddress, local_port: u16) -> Self {
        Self {
            local_addr,
            local_port,
            remote_addr: None,
            remote_port: None,
            recv_buffer: Vec::new(),
            broadcast: false,
            multicast_groups: Vec::new(),
            socket_options: UdpSocketOptions::default(),
            statistics: UdpSocketStats::default(),
            bind_time: current_time_ms(),
            last_activity: current_time_ms(),
        }
    }

    /// Set socket option
    pub fn set_option(&mut self, option: UdpSocketOption) -> NetworkResult<()> {
        match option {
            UdpSocketOption::ReuseAddr(value) => self.socket_options.reuse_addr = value,
            UdpSocketOption::ReusePort(value) => self.socket_options.reuse_port = value,
            UdpSocketOption::Broadcast(value) => self.broadcast = value,
            UdpSocketOption::RecvBufferSize(size) => {
                self.socket_options.recv_buffer_size = size;
                // Limit receive buffer if needed
                while self.recv_buffer.len() > size / 1500 {
                    // Approximate packets
                    self.recv_buffer.remove(0);
                    self.statistics.dropped_packets += 1;
                }
            }
            UdpSocketOption::SendBufferSize(size) => self.socket_options.send_buffer_size = size,
            UdpSocketOption::RecvTimeout(timeout) => self.socket_options.recv_timeout = timeout,
            UdpSocketOption::SendTimeout(timeout) => self.socket_options.send_timeout = timeout,
            UdpSocketOption::Ttl(ttl) => self.socket_options.ttl = ttl,
            UdpSocketOption::MulticastTtl(ttl) => self.socket_options.multicast_ttl = ttl,
            UdpSocketOption::MulticastLoop(enable) => self.socket_options.multicast_loop = enable,
            UdpSocketOption::Dscp(dscp) => self.socket_options.dscp = dscp,
        }
        Ok(())
    }

    /// Get socket option
    pub fn get_option(&self, option_type: UdpSocketOptionType) -> UdpSocketOption {
        match option_type {
            UdpSocketOptionType::ReuseAddr => {
                UdpSocketOption::ReuseAddr(self.socket_options.reuse_addr)
            }
            UdpSocketOptionType::ReusePort => {
                UdpSocketOption::ReusePort(self.socket_options.reuse_port)
            }
            UdpSocketOptionType::Broadcast => UdpSocketOption::Broadcast(self.broadcast),
            UdpSocketOptionType::RecvBufferSize => {
                UdpSocketOption::RecvBufferSize(self.socket_options.recv_buffer_size)
            }
            UdpSocketOptionType::SendBufferSize => {
                UdpSocketOption::SendBufferSize(self.socket_options.send_buffer_size)
            }
            UdpSocketOptionType::RecvTimeout => {
                UdpSocketOption::RecvTimeout(self.socket_options.recv_timeout)
            }
            UdpSocketOptionType::SendTimeout => {
                UdpSocketOption::SendTimeout(self.socket_options.send_timeout)
            }
            UdpSocketOptionType::Ttl => UdpSocketOption::Ttl(self.socket_options.ttl),
            UdpSocketOptionType::MulticastTtl => {
                UdpSocketOption::MulticastTtl(self.socket_options.multicast_ttl)
            }
            UdpSocketOptionType::MulticastLoop => {
                UdpSocketOption::MulticastLoop(self.socket_options.multicast_loop)
            }
            UdpSocketOptionType::Dscp => UdpSocketOption::Dscp(self.socket_options.dscp),
        }
    }

    /// Connect UDP socket (sets default destination)
    pub fn connect(&mut self, remote_addr: NetworkAddress, remote_port: u16) {
        self.remote_addr = Some(remote_addr);
        self.remote_port = Some(remote_port);
    }

    /// Disconnect UDP socket
    pub fn disconnect(&mut self) {
        self.remote_addr = None;
        self.remote_port = None;
    }

    /// Add datagram to receive buffer with enhanced management
    pub fn add_datagram(&mut self, datagram: UdpDatagram) {
        self.last_activity = current_time_ms();

        // Calculate buffer size in bytes
        let current_size: usize = self.recv_buffer.iter().map(|d| d.data.len()).sum();
        let incoming_size = datagram.data.len();

        // Check if we need to drop packets to make room
        if current_size + incoming_size > self.socket_options.recv_buffer_size {
            // Drop oldest packets until we have room
            while !self.recv_buffer.is_empty() {
                let oldest_size = self.recv_buffer[0].data.len();
                let new_total = current_size - oldest_size + incoming_size;
                if new_total <= self.socket_options.recv_buffer_size {
                    break;
                }
                self.recv_buffer.remove(0);
                self.statistics.dropped_packets += 1;
            }
        }

        self.recv_buffer.push(datagram);
        self.statistics.packets_received += 1;
        self.statistics.bytes_received += incoming_size as u64;
    }

    /// Check if socket is idle
    pub fn is_idle(&self, timeout_ms: u64) -> bool {
        current_time_ms() - self.last_activity > timeout_ms
    }

    /// Get detailed socket statistics
    pub fn get_stats(&self) -> UdpSocketStats {
        self.statistics.clone()
    }

    /// Get next datagram from receive buffer
    pub fn get_datagram(&mut self) -> Option<UdpDatagram> {
        if !self.recv_buffer.is_empty() {
            Some(self.recv_buffer.remove(0))
        } else {
            None
        }
    }

    /// Check if socket has data available
    pub fn has_data(&self) -> bool {
        !self.recv_buffer.is_empty()
    }

    /// Join multicast group
    pub fn join_multicast(&mut self, group: NetworkAddress) -> NetworkResult<()> {
        if !group.is_multicast() {
            return Err(NetworkError::InvalidAddress);
        }

        if !self.multicast_groups.contains(&group) {
            self.multicast_groups.push(group);
        }
        Ok(())
    }

    /// Leave multicast group
    pub fn leave_multicast(&mut self, group: NetworkAddress) -> NetworkResult<()> {
        self.multicast_groups.retain(|&addr| addr != group);
        Ok(())
    }
}

/// Enhanced UDP socket manager with port management and binding validation
pub struct UdpManager {
    sockets: RwLock<BTreeMap<(NetworkAddress, u16), UdpSocket>>,
    next_port: RwLock<u16>,
    port_usage: RwLock<BTreeMap<u16, PortUsage>>,
    global_stats: RwLock<UdpGlobalStats>,
}

/// Port usage tracking
#[derive(Debug, Clone)]
struct PortUsage {
    count: usize,
    addresses: Vec<NetworkAddress>,
    last_used: u64,
}

/// Global UDP statistics
#[derive(Debug, Clone, Default)]
pub struct UdpGlobalStats {
    pub total_sockets: usize,
    pub active_sockets: usize,
    pub total_packets_sent: u64,
    pub total_packets_received: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub port_allocation_failures: u64,
    pub binding_conflicts: u64,
}

impl UdpManager {
    pub fn new() -> Self {
        Self {
            sockets: RwLock::new(BTreeMap::new()),
            next_port: RwLock::new(32768),
            port_usage: RwLock::new(BTreeMap::new()),
            global_stats: RwLock::new(UdpGlobalStats::default()),
        }
    }

    /// Check if address/port combination can be bound
    fn can_bind(
        &self,
        addr: NetworkAddress,
        port: u16,
        reuse_addr: bool,
        reuse_port: bool,
    ) -> bool {
        let sockets = self.sockets.read();

        // Check for exact match
        if let Some(existing) = sockets.get(&(addr, port)) {
            // Allow binding if both sockets have reuse options set
            return reuse_addr
                && existing.socket_options.reuse_addr
                && reuse_port
                && existing.socket_options.reuse_port;
        }

        // Check for wildcard conflicts
        let wildcard = NetworkAddress::IPv4([0, 0, 0, 0]);
        if addr != wildcard {
            if let Some(wildcard_socket) = sockets.get(&(wildcard, port)) {
                return reuse_addr && wildcard_socket.socket_options.reuse_addr;
            }
        }

        // Check if any specific address is bound to this port when trying to bind wildcard
        if addr == wildcard {
            for ((existing_addr, existing_port), existing_socket) in sockets.iter() {
                if *existing_port == port && *existing_addr != wildcard {
                    if !(reuse_addr && existing_socket.socket_options.reuse_addr) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Allocate port with collision avoidance
    pub fn allocate_port(&self) -> u16 {
        let mut next_port = self.next_port.write();
        let port_usage = self.port_usage.read();
        let mut stats = self.global_stats.write();

        let start_port = *next_port;
        let mut port = start_port;

        // Find an unused port
        loop {
            if !port_usage.contains_key(&port) {
                *next_port = if port == u16::MAX { 32768 } else { port + 1 };
                return port;
            }

            port = if port == u16::MAX { 32768 } else { port + 1 };

            // Prevent infinite loop
            if port == start_port {
                stats.port_allocation_failures += 1;
                // Return the original port even if in use - higher layer will handle conflict
                *next_port = if port == u16::MAX { 32768 } else { port + 1 };
                return port;
            }
        }
    }

    /// Get global UDP statistics
    pub fn get_global_stats(&self) -> UdpGlobalStats {
        let stats = self.global_stats.read();
        let mut current_stats = stats.clone();

        // Update active socket count
        let sockets = self.sockets.read();
        current_stats.active_sockets = sockets.len();

        current_stats
    }

    /// Clean up idle sockets
    pub fn cleanup_idle_sockets(&self, idle_timeout_ms: u64) {
        let mut sockets = self.sockets.write();
        let mut port_usage = self.port_usage.write();
        let mut stats = self.global_stats.write();

        let idle_sockets: Vec<_> = sockets
            .iter()
            .filter(|(_, socket)| socket.is_idle(idle_timeout_ms))
            .map(|((addr, port), _)| (*addr, *port))
            .collect();

        for (addr, port) in idle_sockets {
            if let Some(_socket) = sockets.remove(&(addr, port)) {
                stats.active_sockets -= 1;

                // Update port usage
                if let Some(usage) = port_usage.get_mut(&port) {
                    usage.count -= 1;
                    usage.addresses.retain(|&a| a != addr);
                    if usage.count == 0 {
                        port_usage.remove(&port);
                    }
                }
            }
        }
    }

    /// Bind socket with comprehensive validation
    pub fn bind_socket(&self, local_addr: NetworkAddress, local_port: u16) -> NetworkResult<()> {
        self.bind_socket_with_options(local_addr, local_port, false, false)
    }

    /// Bind socket with reuse options
    pub fn bind_socket_with_options(
        &self,
        local_addr: NetworkAddress,
        local_port: u16,
        reuse_addr: bool,
        reuse_port: bool,
    ) -> NetworkResult<()> {
        // Validate port range
        if local_port == 0 {
            return Err(NetworkError::InvalidArgument);
        }

        // Check binding permissions
        if local_port < 1024 {
            // Privileged port - would need capability check in full implementation
            // For now, allow all binds
        }

        let mut sockets = self.sockets.write();
        let mut port_usage = self.port_usage.write();
        let mut stats = self.global_stats.write();

        // Check if binding is allowed
        if !self.can_bind(local_addr, local_port, reuse_addr, reuse_port) {
            stats.binding_conflicts += 1;
            return Err(NetworkError::AddressInUse);
        }

        let key = (local_addr, local_port);
        let mut socket = UdpSocket::new(local_addr, local_port);
        socket.socket_options.reuse_addr = reuse_addr;
        socket.socket_options.reuse_port = reuse_port;

        sockets.insert(key, socket);

        // Update port usage tracking
        let usage = port_usage.entry(local_port).or_insert(PortUsage {
            count: 0,
            addresses: Vec::new(),
            last_used: current_time_ms(),
        });
        usage.count += 1;
        if !usage.addresses.contains(&local_addr) {
            usage.addresses.push(local_addr);
        }
        usage.last_used = current_time_ms();

        // Update global statistics
        stats.total_sockets += 1;
        stats.active_sockets += 1;

        Ok(())
    }

    /// Unbind socket with cleanup
    pub fn unbind_socket(&self, local_addr: NetworkAddress, local_port: u16) -> NetworkResult<()> {
        let mut sockets = self.sockets.write();
        let mut port_usage = self.port_usage.write();
        let mut stats = self.global_stats.write();

        let key = (local_addr, local_port);

        if let Some(socket) = sockets.remove(&key) {
            // Update global statistics
            stats.active_sockets -= 1;
            stats.total_packets_sent += socket.statistics.packets_sent;
            stats.total_packets_received += socket.statistics.packets_received;
            stats.total_bytes_sent += socket.statistics.bytes_sent;
            stats.total_bytes_received += socket.statistics.bytes_received;

            // Update port usage
            if let Some(usage) = port_usage.get_mut(&local_port) {
                usage.count -= 1;
                usage.addresses.retain(|&addr| addr != local_addr);

                // Remove port usage entry if no longer used
                if usage.count == 0 {
                    port_usage.remove(&local_port);
                }
            }

            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    pub fn get_socket(&self, local_addr: &NetworkAddress, local_port: u16) -> Option<UdpSocket> {
        let sockets = self.sockets.read();
        let key = (*local_addr, local_port);
        sockets.get(&key).cloned()
    }

    pub fn update_socket<F>(
        &self,
        local_addr: NetworkAddress,
        local_port: u16,
        f: F,
    ) -> NetworkResult<()>
    where
        F: FnOnce(&mut UdpSocket),
    {
        let mut sockets = self.sockets.write();
        let key = (local_addr, local_port);

        if let Some(socket) = sockets.get_mut(&key) {
            f(socket);
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Find sockets that should receive a datagram with comprehensive routing
    pub fn find_receiving_sockets(
        &self,
        dest_addr: &NetworkAddress,
        dest_port: u16,
    ) -> Vec<(NetworkAddress, u16)> {
        let sockets = self.sockets.read();
        let mut receivers = Vec::new();
        let mut exact_matches = Vec::new();
        let mut wildcard_matches = Vec::new();

        for ((addr, port), socket) in sockets.iter() {
            if *port != dest_port {
                continue;
            }

            let should_receive = match (addr, dest_addr) {
                // Exact address match (highest priority)
                (a, d) if a == d => {
                    exact_matches.push((*addr, *port));
                    true
                }
                // Wildcard address (0.0.0.0) - lower priority
                (NetworkAddress::IPv4([0, 0, 0, 0]), _) => {
                    wildcard_matches.push((*addr, *port));
                    true
                }
                // Broadcast handling
                (_, d) if d.is_broadcast() && socket.broadcast => {
                    receivers.push((*addr, *port));
                    true
                }
                // Multicast handling
                (_, d) if d.is_multicast() && socket.multicast_groups.contains(dest_addr) => {
                    receivers.push((*addr, *port));
                    true
                }
                // Loopback special case
                (NetworkAddress::IPv4([127, 0, 0, 1]), NetworkAddress::IPv4([127, 0, 0, 1])) => {
                    exact_matches.push((*addr, *port));
                    true
                }
                _ => false,
            };

            // Additional filtering based on socket state
            if should_receive {
                // Check if socket is in valid state to receive
                if socket.is_idle(300000) {
                    // 5 minutes idle timeout
                    continue;
                }

                // Check receive buffer space
                let current_buffer_size: usize =
                    socket.recv_buffer.iter().map(|d| d.data.len()).sum();
                if current_buffer_size >= socket.socket_options.recv_buffer_size {
                    // Buffer full, would drop packet
                    continue;
                }
            }
        }

        // Return exact matches first, then wildcard matches
        // This implements proper socket binding precedence
        if !exact_matches.is_empty() {
            exact_matches
        } else if !wildcard_matches.is_empty() {
            wildcard_matches
        } else {
            receivers
        }
    }

    /// Advanced packet routing with load balancing for multiple sockets
    pub fn route_packet_advanced(
        &self,
        dest_addr: &NetworkAddress,
        dest_port: u16,
        src_addr: &NetworkAddress,
        src_port: u16,
    ) -> Vec<(NetworkAddress, u16)> {
        let receivers = self.find_receiving_sockets(dest_addr, dest_port);

        // If multiple sockets can receive the packet, apply load balancing
        if receivers.len() > 1 {
            // Simple hash-based load balancing using source address and port
            let hash = self.calculate_flow_hash(src_addr, src_port, dest_addr, dest_port);
            let selected_index = (hash as usize) % receivers.len();
            vec![receivers[selected_index]]
        } else {
            receivers
        }
    }

    /// Calculate flow hash for load balancing
    fn calculate_flow_hash(
        &self,
        src_addr: &NetworkAddress,
        src_port: u16,
        dest_addr: &NetworkAddress,
        dest_port: u16,
    ) -> u32 {
        let mut hash = 0u32;

        // Hash source address
        match src_addr {
            NetworkAddress::IPv4(bytes) => {
                for &byte in bytes {
                    hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
                }
            }
            _ => {}
        }

        // Hash destination address
        match dest_addr {
            NetworkAddress::IPv4(bytes) => {
                for &byte in bytes {
                    hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
                }
            }
            _ => {}
        }

        // Hash ports
        hash = hash.wrapping_mul(31).wrapping_add(src_port as u32);
        hash = hash.wrapping_mul(31).wrapping_add(dest_port as u32);

        hash
    }
}

/// UDP socket option types
#[derive(Debug, Clone, Copy)]
pub enum UdpSocketOptionType {
    ReuseAddr,
    ReusePort,
    Broadcast,
    RecvBufferSize,
    SendBufferSize,
    RecvTimeout,
    SendTimeout,
    Ttl,
    MulticastTtl,
    MulticastLoop,
    Dscp,
}

/// UDP socket option values
#[derive(Debug, Clone)]
pub enum UdpSocketOption {
    ReuseAddr(bool),
    ReusePort(bool),
    Broadcast(bool),
    RecvBufferSize(usize),
    SendBufferSize(usize),
    RecvTimeout(Option<u32>),
    SendTimeout(Option<u32>),
    Ttl(u8),
    MulticastTtl(u8),
    MulticastLoop(bool),
    Dscp(u8),
}

static UDP_MANAGER: UdpManager = UdpManager {
    sockets: RwLock::new(BTreeMap::new()),
    next_port: RwLock::new(32768),
    port_usage: RwLock::new(BTreeMap::new()),
    global_stats: RwLock::new(UdpGlobalStats {
        total_sockets: 0,
        active_sockets: 0,
        total_packets_sent: 0,
        total_packets_received: 0,
        total_bytes_sent: 0,
        total_bytes_received: 0,
        port_allocation_failures: 0,
        binding_conflicts: 0,
    }),
};

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    // Use system time for UDP timestamps
    crate::time::get_system_time_ms()
}

/// Process incoming UDP packet
pub fn process_packet(
    _network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = UdpHeader::parse(&mut packet)?;

    // Production: UDP packet processed silently

    // Validate length
    if header.length < UDP_HEADER_SIZE as u16 {
        return Err(NetworkError::InvalidPacket);
    }

    let payload_length = header.length as usize - UDP_HEADER_SIZE;
    if packet.remaining() < payload_length {
        return Err(NetworkError::InvalidPacket);
    }

    // Read payload
    let payload = packet
        .read(payload_length)
        .ok_or(NetworkError::InvalidPacket)?;

    // QUIC runs over UDP: if a QUIC endpoint is bound to this port, hand the
    // datagram to it before the generic socket-delivery path.
    if super::quic::udp::is_bound(header.dest_port)
        && super::quic::udp::deliver(header.dest_port, src_ip, header.source_port, payload)
    {
        return Ok(());
    }

    // Verify checksum (if not zero)
    if header.checksum != 0 {
        let mut checksum_header = header.clone();
        checksum_header.checksum = 0;
        let calculated_checksum = checksum_header.calculate_checksum(&src_ip, &dst_ip, payload);
        if calculated_checksum != header.checksum {
            // Production: checksum validation failed
            return Err(NetworkError::InvalidPacket);
        }
    }

    // Find receiving sockets
    let receivers = UDP_MANAGER.find_receiving_sockets(&dst_ip, header.dest_port);

    if receivers.is_empty() {
        // No listener on port - send ICMP Port Unreachable
        send_icmp_port_unreachable(dst_ip, src_ip, &packet.as_slice()[..packet.position])?;
        return Ok(());
    }

    // Deliver to all matching sockets
    for (local_addr, local_port) in receivers {
        let datagram = UdpDatagram {
            source_addr: src_ip,
            source_port: header.source_port,
            data: payload.to_vec(),
            timestamp: get_current_time(),
        };

        UDP_MANAGER
            .update_socket(local_addr, local_port, |socket| {
                socket.add_datagram(datagram);
            })
            .ok(); // Ignore errors for delivery
    }

    Ok(())
}

/// Send UDP packet
pub fn send_udp_packet(
    src_ip: NetworkAddress,
    src_port: u16,
    dst_ip: NetworkAddress,
    dst_port: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    let length = UDP_HEADER_SIZE + payload.len();
    if length > u16::MAX as usize {
        return Err(NetworkError::BufferOverflow);
    }

    // Create UDP header
    let mut header = UdpHeader {
        source_port: src_port,
        dest_port: dst_port,
        length: length as u16,
        checksum: 0,
    };

    // Calculate checksum
    header.checksum = header.calculate_checksum(&src_ip, &dst_ip, payload);

    // Serialize UDP header and payload
    let mut udp_packet = Vec::with_capacity(8 + payload.len());

    // UDP header serialization
    udp_packet.extend_from_slice(&src_port.to_be_bytes());
    udp_packet.extend_from_slice(&dst_port.to_be_bytes());
    udp_packet.extend_from_slice(&header.length.to_be_bytes());
    udp_packet.extend_from_slice(&header.checksum.to_be_bytes());

    // Add payload
    udp_packet.extend_from_slice(payload);

    // Send through IP layer
    super::ip::send_ipv4_packet(src_ip, dst_ip, 17, &udp_packet)
}

/// UDP socket operations

/// Create UDP socket
pub fn udp_socket() -> NetworkResult<(NetworkAddress, u16)> {
    let local_addr = NetworkAddress::IPv4([0, 0, 0, 0]); // Wildcard address
    let local_port = UDP_MANAGER.allocate_port();

    UDP_MANAGER.bind_socket(local_addr, local_port)?;
    Ok((local_addr, local_port))
}

/// Bind UDP socket to specific address
pub fn udp_bind(local_addr: NetworkAddress, local_port: u16) -> NetworkResult<()> {
    UDP_MANAGER.bind_socket(local_addr, local_port)
}

/// Connect UDP socket
pub fn udp_connect(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
) -> NetworkResult<()> {
    UDP_MANAGER.update_socket(local_addr, local_port, |socket| {
        socket.connect(remote_addr, remote_port);
    })
}

/// Send data through UDP socket
pub fn udp_send(local_addr: NetworkAddress, local_port: u16, data: &[u8]) -> NetworkResult<usize> {
    let socket = UDP_MANAGER
        .get_socket(&local_addr, local_port)
        .ok_or(NetworkError::InvalidAddress)?;

    if let (Some(remote_addr), Some(remote_port)) = (socket.remote_addr, socket.remote_port) {
        send_udp_packet(local_addr, local_port, remote_addr, remote_port, data)?;
        Ok(data.len())
    } else {
        Err(NetworkError::NotSupported)
    }
}

/// Send data to specific address through UDP socket
pub fn udp_send_to(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
    data: &[u8],
) -> NetworkResult<usize> {
    // Verify socket exists
    UDP_MANAGER
        .get_socket(&local_addr, local_port)
        .ok_or(NetworkError::InvalidAddress)?;

    send_udp_packet(local_addr, local_port, remote_addr, remote_port, data)?;
    Ok(data.len())
}

/// Receive data from UDP socket
pub fn udp_recv(
    local_addr: NetworkAddress,
    local_port: u16,
) -> NetworkResult<Option<(Vec<u8>, NetworkAddress, u16)>> {
    let mut result = None;

    UDP_MANAGER.update_socket(local_addr, local_port, |socket| {
        if let Some(datagram) = socket.get_datagram() {
            result = Some((datagram.data, datagram.source_addr, datagram.source_port));
        }
    })?;

    Ok(result)
}

/// Check if UDP socket has data available
pub fn udp_has_data(local_addr: NetworkAddress, local_port: u16) -> bool {
    UDP_MANAGER
        .get_socket(&local_addr, local_port)
        .map(|socket| socket.has_data())
        .unwrap_or(false)
}

/// Close UDP socket
pub fn udp_close(local_addr: NetworkAddress, local_port: u16) -> NetworkResult<()> {
    UDP_MANAGER.unbind_socket(local_addr, local_port)
}

/// Set socket broadcast option
pub fn udp_set_broadcast(
    local_addr: NetworkAddress,
    local_port: u16,
    broadcast: bool,
) -> NetworkResult<()> {
    UDP_MANAGER.update_socket(local_addr, local_port, |socket| {
        socket.broadcast = broadcast;
    })
}

/// Join multicast group
pub fn udp_join_multicast(
    local_addr: NetworkAddress,
    local_port: u16,
    group: NetworkAddress,
) -> NetworkResult<()> {
    UDP_MANAGER.update_socket(local_addr, local_port, |socket| {
        socket.join_multicast(group).ok();
    })
}

/// Leave multicast group
pub fn udp_leave_multicast(
    local_addr: NetworkAddress,
    local_port: u16,
    group: NetworkAddress,
) -> NetworkResult<()> {
    UDP_MANAGER.update_socket(local_addr, local_port, |socket| {
        socket.leave_multicast(group).ok();
    })
}

/// Get current time in milliseconds
fn get_current_time() -> u64 {
    // Use system time for UDP operations
    crate::time::get_system_time_ms()
}

/// Send ICMP Port Unreachable message
fn send_icmp_port_unreachable(
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    original_packet: &[u8],
) -> NetworkResult<()> {
    // ICMP Destination Unreachable format:
    // Type: 3 (Destination Unreachable)
    // Code: 3 (Port Unreachable)
    // Include IP header + first 8 bytes of original datagram

    let mut icmp_payload = Vec::new();

    // Unused field (must be zero)
    icmp_payload.extend_from_slice(&[0u8; 4]);

    // Include IP header + 8 bytes of original datagram
    // Take at most 28 bytes (20 byte IP header + 8 bytes data)
    let include_len = core::cmp::min(original_packet.len(), 28);
    icmp_payload.extend_from_slice(&original_packet[..include_len]);

    // Build ICMP header
    let icmp_type = 3u8; // Destination Unreachable
    let icmp_code = 3u8; // Port Unreachable

    // Calculate checksum
    let mut checksum_data = Vec::new();
    checksum_data.push(icmp_type);
    checksum_data.push(icmp_code);
    checksum_data.extend_from_slice(&[0u8; 2]); // Checksum placeholder
    checksum_data.extend_from_slice(&icmp_payload);

    let checksum = calculate_icmp_checksum(&checksum_data);

    // Build final ICMP packet
    let mut icmp_packet = Vec::new();
    icmp_packet.push(icmp_type);
    icmp_packet.push(icmp_code);
    icmp_packet.extend_from_slice(&checksum.to_be_bytes());
    icmp_packet.extend_from_slice(&icmp_payload);

    // Send through IP layer (protocol 1 = ICMP)
    super::ip::send_ipv4_packet(src_ip, dst_ip, 1, &icmp_packet)
}

/// Calculate ICMP checksum (RFC 1071).
///
/// `data` must contain the full ICMP message with the checksum field
/// already zeroed; the ones-complement sum is delegated to the shared
/// [`crate::net::internet_checksum`] primitive.
fn calculate_icmp_checksum(data: &[u8]) -> u16 {
    super::internet_checksum(data)
}

#[cfg(all(test, feature = "disabled-tests"))]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn process_packet_accepts_valid_checksum() {
        {
            let mut sockets = UDP_MANAGER.sockets.write();
            sockets.clear();
        }
        {
            let mut next_port = UDP_MANAGER.next_port.write();
            *next_port = 32768;
        }

        let stack = NetworkStack::new();
        let src_ip = NetworkAddress::IPv4([192, 168, 1, 1]);
        let dst_ip = NetworkAddress::IPv4([192, 168, 1, 2]);
        let dest_port = 8080;

        UDP_MANAGER.bind_socket(dst_ip, dest_port).unwrap();

        let payload = b"hello";

        let mut header = UdpHeader {
            source_port: 12345,
            dest_port,
            length: (UDP_HEADER_SIZE + payload.len()) as u16,
            checksum: 0,
        };
        header.checksum = header.calculate_checksum(&src_ip, &dst_ip, payload);

        let mut packet_bytes = Vec::new();
        packet_bytes.extend_from_slice(&header.source_port.to_be_bytes());
        packet_bytes.extend_from_slice(&header.dest_port.to_be_bytes());
        packet_bytes.extend_from_slice(&header.length.to_be_bytes());
        packet_bytes.extend_from_slice(&header.checksum.to_be_bytes());
        packet_bytes.extend_from_slice(payload);

        let packet = PacketBuffer::from_data(packet_bytes);

        let result = process_packet(&stack, src_ip, dst_ip, packet);

        assert!(result.is_ok());

        let socket = UDP_MANAGER
            .get_socket(&dst_ip, dest_port)
            .expect("socket should exist");
        assert_eq!(socket.recv_buffer.len(), 1);
        assert_eq!(socket.recv_buffer[0].data.as_slice(), payload);
        assert_eq!(socket.recv_buffer[0].source_port, header.source_port);

        UDP_MANAGER.unbind_socket(dst_ip, dest_port).unwrap();
    }
}
