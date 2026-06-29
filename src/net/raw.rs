//! AF_INET SOCK_RAW and AF_PACKET socket support.
//!
//! Raw sockets receive copies of matching IP datagrams (optionally filtered by
//! protocol). AF_PACKET sockets operate at the link layer and can inject or
//! capture Ethernet frames on a specific interface.

use super::{
    internet_checksum, NetworkAddress, NetworkError, NetworkResult, NetworkStack, PacketBuffer,
    Protocol,
};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

/// Linux AF_PACKET
pub const AF_PACKET: i32 = 17;

/// Maximum queued packets per raw/packet socket.
const MAX_QUEUE: usize = 256;

/// Raw socket (AF_INET, SOCK_RAW).
#[derive(Debug, Clone)]
pub struct RawSocket {
    pub id: u32,
    pub protocol: u8,
    pub bound_addr: Option<NetworkAddress>,
    pub recv_queue: VecDeque<Vec<u8>>,
    pub include_ip_header: bool,
    pub stats_rx: u64,
    pub stats_tx: u64,
}

impl RawSocket {
    fn new(id: u32, protocol: u8) -> Self {
        Self {
            id,
            protocol,
            bound_addr: None,
            recv_queue: VecDeque::new(),
            include_ip_header: true,
            stats_rx: 0,
            stats_tx: 0,
        }
    }

    fn enqueue(&mut self, packet: Vec<u8>) -> NetworkResult<()> {
        if self.recv_queue.len() >= MAX_QUEUE {
            self.recv_queue.pop_front();
        }
        self.recv_queue.push_back(packet);
        self.stats_rx += 1;
        Ok(())
    }
}

/// AF_PACKET socket (link layer).
#[derive(Debug, Clone)]
pub struct PacketSocket {
    pub id: u32,
    pub protocol: u16,
    pub ifindex: u32,
    pub interface: Option<alloc::string::String>,
    pub recv_queue: VecDeque<Vec<u8>>,
    pub stats_rx: u64,
    pub stats_tx: u64,
}

impl PacketSocket {
    fn new(id: u32, protocol: u16) -> Self {
        Self {
            id,
            protocol,
            ifindex: 0,
            interface: None,
            recv_queue: VecDeque::new(),
            stats_rx: 0,
            stats_tx: 0,
        }
    }

    fn enqueue(&mut self, frame: Vec<u8>) -> NetworkResult<()> {
        if self.recv_queue.len() >= MAX_QUEUE {
            self.recv_queue.pop_front();
        }
        self.recv_queue.push_back(frame);
        self.stats_rx += 1;
        Ok(())
    }
}

static NEXT_RAW_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_PACKET_ID: AtomicU32 = AtomicU32::new(1);

static RAW_SOCKETS: RwLock<BTreeMap<u32, RawSocket>> = RwLock::new(BTreeMap::new());
static PACKET_SOCKETS: RwLock<BTreeMap<u32, PacketSocket>> = RwLock::new(BTreeMap::new());

/// Map VFS fd → raw socket id.
static RAW_FD_MAP: RwLock<BTreeMap<i32, u32>> = RwLock::new(BTreeMap::new());
/// Map VFS fd → packet socket id.
static PACKET_FD_MAP: RwLock<BTreeMap<i32, u32>> = RwLock::new(BTreeMap::new());

fn alloc_raw_id() -> u32 {
    NEXT_RAW_ID.fetch_add(1, Ordering::Relaxed)
}

fn alloc_packet_id() -> u32 {
    NEXT_PACKET_ID.fetch_add(1, Ordering::Relaxed)
}

/// Create an AF_INET SOCK_RAW socket; returns internal socket id.
pub fn create_raw_socket(protocol: u8) -> NetworkResult<u32> {
    let id = alloc_raw_id();
    let sock = RawSocket::new(id, protocol);
    RAW_SOCKETS.write().insert(id, sock);
    Ok(id)
}

/// Create an AF_PACKET socket; returns internal socket id.
pub fn create_packet_socket(protocol: u16) -> NetworkResult<u32> {
    let id = alloc_packet_id();
    let sock = PacketSocket::new(id, protocol);
    PACKET_SOCKETS.write().insert(id, sock);
    Ok(id)
}

/// Register a VFS fd for a raw socket.
pub fn register_raw_fd(fd: i32, socket_id: u32) {
    RAW_FD_MAP.write().insert(fd, socket_id);
}

/// Register a VFS fd for a packet socket.
pub fn register_packet_fd(fd: i32, socket_id: u32) {
    PACKET_FD_MAP.write().insert(fd, socket_id);
}

/// Returns true if `fd` is a raw socket.
pub fn is_raw_fd(fd: i32) -> bool {
    RAW_FD_MAP.read().contains_key(&fd)
}

/// Returns true if `fd` is an AF_PACKET socket.
pub fn is_packet_fd(fd: i32) -> bool {
    PACKET_FD_MAP.read().contains_key(&fd)
}

fn raw_id_for_fd(fd: i32) -> Option<u32> {
    RAW_FD_MAP.read().get(&fd).copied()
}

fn packet_id_for_fd(fd: i32) -> Option<u32> {
    PACKET_FD_MAP.read().get(&fd).copied()
}

/// Bind raw socket to a local IP (optional filter).
pub fn raw_bind(socket_id: u32, addr: NetworkAddress) -> NetworkResult<()> {
    let mut socks = RAW_SOCKETS.write();
    let sock = socks
        .get_mut(&socket_id)
        .ok_or(NetworkError::InvalidAddress)?;
    sock.bound_addr = Some(addr);
    Ok(())
}

/// Bind packet socket to interface by index and optional protocol filter.
pub fn packet_bind(socket_id: u32, ifindex: u32, protocol: u16) -> NetworkResult<()> {
    let mut socks = PACKET_SOCKETS.write();
    let sock = socks
        .get_mut(&socket_id)
        .ok_or(NetworkError::InvalidAddress)?;

    sock.ifindex = ifindex;
    sock.protocol = protocol;

    let stack = super::network_stack();
    let interfaces = stack.list_interfaces();
    if ifindex > 0 && (ifindex as usize) <= interfaces.len() {
        sock.interface = Some(interfaces[ifindex as usize - 1].name.clone());
    } else if ifindex == 0 {
        sock.interface = None;
    } else {
        return Err(NetworkError::InvalidAddress);
    }
    Ok(())
}

/// Deliver an IPv4 datagram (header + payload) to matching raw sockets.
pub fn deliver_ipv4(
    protocol: u8,
    header_and_payload: &[u8],
    dst: &NetworkAddress,
) -> NetworkResult<()> {
    let mut socks = RAW_SOCKETS.write();
    for sock in socks.values_mut() {
        if sock.protocol != 0 && sock.protocol != protocol {
            continue;
        }
        if let Some(bound) = sock.bound_addr {
            if bound != *dst {
                continue;
            }
        }
        sock.enqueue(header_and_payload.to_vec())?;
    }
    Ok(())
}

/// Deliver a link-layer frame to matching AF_PACKET sockets.
pub fn deliver_link_frame(interface: &str, frame: &[u8]) -> NetworkResult<()> {
    if frame.len() < 14 {
        return Ok(());
    }

    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    let mut socks = PACKET_SOCKETS.write();

    for sock in socks.values_mut() {
        if sock.protocol != 0 && sock.protocol != ethertype {
            continue;
        }
        if let Some(ref iface) = sock.interface {
            if iface != interface {
                continue;
            }
        }
        sock.enqueue(frame.to_vec())?;
    }
    Ok(())
}

/// Read from a raw socket fd.
pub fn raw_recv(fd: i32, buf: &mut [u8]) -> NetworkResult<usize> {
    let id = raw_id_for_fd(fd).ok_or(NetworkError::InvalidAddress)?;
    let mut socks = RAW_SOCKETS.write();
    let sock = socks.get_mut(&id).ok_or(NetworkError::InvalidAddress)?;

    if let Some(packet) = sock.recv_queue.pop_front() {
        let n = core::cmp::min(buf.len(), packet.len());
        buf[..n].copy_from_slice(&packet[..n]);
        Ok(n)
    } else {
        Ok(0)
    }
}

/// Read from an AF_PACKET socket fd.
pub fn packet_recv(fd: i32, buf: &mut [u8]) -> NetworkResult<usize> {
    let id = packet_id_for_fd(fd).ok_or(NetworkError::InvalidAddress)?;
    let mut socks = PACKET_SOCKETS.write();
    let sock = socks.get_mut(&id).ok_or(NetworkError::InvalidAddress)?;

    if let Some(frame) = sock.recv_queue.pop_front() {
        let n = core::cmp::min(buf.len(), frame.len());
        buf[..n].copy_from_slice(&frame[..n]);
        Ok(n)
    } else {
        Ok(0)
    }
}

/// Send a raw IP datagram (with IP header) via the routing table.
pub fn raw_send(fd: i32, data: &[u8]) -> NetworkResult<usize> {
    if data.len() < 20 {
        return Err(NetworkError::InvalidPacket);
    }

    let id = raw_id_for_fd(fd).ok_or(NetworkError::InvalidAddress)?;
    let version = (data[0] >> 4) & 0x0F;
    if version != 4 {
        return Err(NetworkError::NotSupported);
    }

    let dst = NetworkAddress::ipv4(data[16], data[17], data[18], data[19]);
    let stack = super::network_stack();

    let route = stack.find_route(&dst).ok_or(NetworkError::NoRoute)?;

    let packet = PacketBuffer::from_data(data.to_vec());
    stack.send_packet(&route.interface, packet)?;

    if let Some(sock) = RAW_SOCKETS.write().get_mut(&id) {
        sock.stats_tx += 1;
    }
    Ok(data.len())
}

/// Send a link-layer frame on the bound interface.
pub fn packet_send(fd: i32, frame: &[u8]) -> NetworkResult<usize> {
    if frame.len() < 14 {
        return Err(NetworkError::InvalidPacket);
    }

    let id = packet_id_for_fd(fd).ok_or(NetworkError::InvalidAddress)?;
    let iface = {
        let socks = PACKET_SOCKETS.read();
        let sock = socks.get(&id).ok_or(NetworkError::InvalidAddress)?;
        sock.interface.clone().ok_or(NetworkError::InvalidAddress)?
    };

    let stack = super::network_stack();
    let packet = PacketBuffer::from_data(frame.to_vec());
    stack.send_packet(&iface, packet)?;

    if let Some(sock) = PACKET_SOCKETS.write().get_mut(&id) {
        sock.stats_tx += 1;
    }
    Ok(frame.len())
}

/// Unified send for raw or packet fd.
pub fn send(fd: i32, data: &[u8]) -> NetworkResult<usize> {
    if is_raw_fd(fd) {
        raw_send(fd, data)
    } else if is_packet_fd(fd) {
        packet_send(fd, data)
    } else {
        Err(NetworkError::InvalidAddress)
    }
}

/// Unified recv for raw or packet fd.
pub fn recv(fd: i32, buf: &mut [u8]) -> NetworkResult<usize> {
    if is_raw_fd(fd) {
        raw_recv(fd, buf)
    } else if is_packet_fd(fd) {
        packet_recv(fd, buf)
    } else {
        Err(NetworkError::InvalidAddress)
    }
}

/// Build and transmit a minimal IPv4 raw packet with payload.
pub fn send_raw_ip(
    stack: &NetworkStack,
    src: NetworkAddress,
    dst: NetworkAddress,
    protocol: Protocol,
    payload: &[u8],
    interface: &str,
) -> NetworkResult<()> {
    let total_len = (20 + payload.len()) as u16;
    let mut packet = Vec::with_capacity(20 + payload.len());

    packet.push(0x45); // version 4, IHL 5
    packet.push(0);
    packet.extend_from_slice(&total_len.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes()); // id
    packet.extend_from_slice(&0u16.to_be_bytes()); // flags/frag
    packet.push(64); // TTL
    packet.push(protocol as u8);
    packet.extend_from_slice(&0u16.to_be_bytes()); // checksum placeholder

    if let NetworkAddress::IPv4(s) = src {
        packet.extend_from_slice(&s);
    } else {
        return Err(NetworkError::InvalidAddress);
    }
    if let NetworkAddress::IPv4(d) = dst {
        packet.extend_from_slice(&d);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    packet.extend_from_slice(payload);

    let csum = internet_checksum(&packet[..20]);
    packet[10..12].copy_from_slice(&csum.to_be_bytes());

    stack.send_packet(interface, PacketBuffer::from_data(packet))
}

pub fn raw_id_for_fd_internal(fd: i32) -> Option<u32> {
    raw_id_for_fd(fd)
}

pub fn packet_id_for_fd_internal(fd: i32) -> Option<u32> {
    packet_id_for_fd(fd)
}

/// Close and remove a raw socket.
pub fn close_raw(socket_id: u32) {
    RAW_SOCKETS.write().remove(&socket_id);
    RAW_FD_MAP.write().retain(|_, id| *id != socket_id);
}

/// Close and remove a packet socket.
pub fn close_packet(socket_id: u32) {
    PACKET_SOCKETS.write().remove(&socket_id);
    PACKET_FD_MAP.write().retain(|_, id| *id != socket_id);
}

/// Number of active raw sockets.
pub fn raw_socket_count() -> usize {
    RAW_SOCKETS.read().len()
}

/// Number of active packet sockets.
pub fn packet_socket_count() -> usize {
    PACKET_SOCKETS.read().len()
}
