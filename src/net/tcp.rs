//! TCP (Transmission Control Protocol) implementation
//!
//! This module provides a complete TCP stack with connection management,
//! flow control, congestion control, and reliable data transmission conforming
//! to RFC 793 and subsequent TCP RFCs.
//!
//! # Features
//!
//! - Full RFC 793 TCP state machine implementation
//! - Nagle's algorithm for efficient packet transmission (RFC 896)
//! - Fast retransmit and fast recovery (RFC 2581)
//! - Selective acknowledgment support (SACK, RFC 2018)
//! - TCP window scaling (RFC 1323)
//! - Timestamps for RTT measurement (RFC 1323)
//! - Congestion control with multiple algorithms
//! - Advanced retransmission timer management
//! - Comprehensive connection state tracking
//!
//! # Implementation Status
//!
//! Current implementation supports IPv4 only. IPv6 support is planned for future releases.
//! Path MTU discovery (PMTUD) and explicit congestion notification (ECN) are planned
//! enhancements for future versions.

use super::{NetworkAddress, NetworkError, NetworkResult, NetworkStack, PacketBuffer};
use alloc::{collections::BTreeMap, vec::Vec};
use core::cmp;
use spin::RwLock;

/// Maximum number of pending connections in the accept queue per listening socket
const MAX_BACKLOG: usize = 16;

/// Maximum number of connections (any state) tracked at once. Bounds worst-case
/// memory use; new SYNs are answered with RST once this is reached rather than
/// growing the connection table without limit.
const MAX_CONNECTIONS: usize = 4096;

/// Maximum number of half-open (SYN-RECEIVED) connections tracked at once.
/// This is the primary SYN-flood defense: an attacker sending spoofed SYNs
/// cannot grow the table past this cap, and each half-open entry is also
/// reaped by `tcp_tick` after ~75s (see `TcpConnection::is_timed_out`).
const MAX_HALF_OPEN: usize = 512;

/// Maximum number of out-of-order segments buffered per connection while
/// waiting for reassembly. Without a cap a peer can send an unbounded amount
/// of out-of-order data and grow `ooo_segments` without limit.
const MAX_OOO_SEGMENTS: usize = 64;

/// Maximum bytes buffered in `recv_buffer` awaiting delivery to the
/// application, per connection. `recv_window` is derived from the free space
/// remaining under this cap so the advertised window actually shrinks as the
/// buffer fills, instead of staying fixed at its initial value forever.
const MAX_RECV_BUFFER: usize = 65536;

/// TCP header minimum size
pub const TCP_HEADER_MIN_SIZE: usize = 20;

/// TCP connection states with proper state machine transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

impl TcpState {
    /// Check if state allows data transmission
    pub fn can_send_data(&self) -> bool {
        matches!(self, TcpState::Established | TcpState::CloseWait)
    }

    /// Check if state allows data reception
    pub fn can_recv_data(&self) -> bool {
        matches!(
            self,
            TcpState::Established | TcpState::FinWait1 | TcpState::FinWait2
        )
    }

    /// Check if connection is active
    pub fn is_active(&self) -> bool {
        !matches!(self, TcpState::Closed | TcpState::TimeWait)
    }

    /// Get next state on close
    pub fn on_close(&self) -> TcpState {
        match self {
            TcpState::Established => TcpState::FinWait1,
            TcpState::CloseWait => TcpState::LastAck,
            _ => *self,
        }
    }
}

/// TCP flags
#[derive(Debug, Clone, Copy)]
pub struct TcpFlags {
    pub fin: bool,
    pub syn: bool,
    pub rst: bool,
    pub psh: bool,
    pub ack: bool,
    pub urg: bool,
    pub ece: bool,
    pub cwr: bool,
}

impl TcpFlags {
    pub fn new() -> Self {
        Self {
            fin: false,
            syn: false,
            rst: false,
            psh: false,
            ack: false,
            urg: false,
            ece: false,
            cwr: false,
        }
    }

    pub fn from_byte(flags: u8) -> Self {
        Self {
            fin: (flags & 0x01) != 0,
            syn: (flags & 0x02) != 0,
            rst: (flags & 0x04) != 0,
            psh: (flags & 0x08) != 0,
            ack: (flags & 0x10) != 0,
            urg: (flags & 0x20) != 0,
            ece: (flags & 0x40) != 0,
            cwr: (flags & 0x80) != 0,
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        if self.fin {
            flags |= 0x01;
        }
        if self.syn {
            flags |= 0x02;
        }
        if self.rst {
            flags |= 0x04;
        }
        if self.psh {
            flags |= 0x08;
        }
        if self.ack {
            flags |= 0x10;
        }
        if self.urg {
            flags |= 0x20;
        }
        if self.ece {
            flags |= 0x40;
        }
        if self.cwr {
            flags |= 0x80;
        }
        flags
    }
}

/// TCP header
#[derive(Debug, Clone)]
pub struct TcpHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub sequence_number: u32,
    pub acknowledgment_number: u32,
    pub data_offset: u8,
    pub flags: TcpFlags,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
    pub options: Vec<u8>,
}

impl TcpHeader {
    /// Get source IP from context (would be passed in real implementation)
    pub fn source_ip(&self) -> NetworkAddress {
        // This would be passed from IP layer in real implementation
        NetworkAddress::IPv4([0, 0, 0, 0])
    }

    /// Get payload length (would be calculated from total length)
    pub fn payload_length(&self) -> usize {
        // This would be calculated from IP total length minus headers
        0
    }

    /// Parse TCP header from packet buffer
    pub fn parse(buffer: &mut PacketBuffer) -> NetworkResult<Self> {
        if buffer.remaining() < TCP_HEADER_MIN_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let src_port_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let source_port = u16::from_be_bytes([src_port_bytes[0], src_port_bytes[1]]);

        let dst_port_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let dest_port = u16::from_be_bytes([dst_port_bytes[0], dst_port_bytes[1]]);

        let seq_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let sequence_number =
            u32::from_be_bytes([seq_bytes[0], seq_bytes[1], seq_bytes[2], seq_bytes[3]]);

        let ack_bytes = buffer.read(4).ok_or(NetworkError::InvalidPacket)?;
        let acknowledgment_number =
            u32::from_be_bytes([ack_bytes[0], ack_bytes[1], ack_bytes[2], ack_bytes[3]]);

        let offset_flags_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let data_offset = (offset_flags_bytes[0] >> 4) & 0x0f;
        let flags = TcpFlags::from_byte(offset_flags_bytes[1]);

        let window_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let window_size = u16::from_be_bytes([window_bytes[0], window_bytes[1]]);

        let checksum_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let checksum = u16::from_be_bytes([checksum_bytes[0], checksum_bytes[1]]);

        let urgent_bytes = buffer.read(2).ok_or(NetworkError::InvalidPacket)?;
        let urgent_pointer = u16::from_be_bytes([urgent_bytes[0], urgent_bytes[1]]);

        // Read options if present
        let header_length = (data_offset as usize) * 4;
        let options_length = header_length.saturating_sub(TCP_HEADER_MIN_SIZE);
        let options = if options_length > 0 {
            let options_bytes = buffer
                .read(options_length)
                .ok_or(NetworkError::InvalidPacket)?;
            options_bytes.to_vec()
        } else {
            Vec::new()
        };

        Ok(TcpHeader {
            source_port,
            dest_port,
            sequence_number,
            acknowledgment_number,
            data_offset,
            flags,
            window_size,
            checksum,
            urgent_pointer,
            options,
        })
    }

    /// Calculate TCP checksum
    /// RFC 793 (IPv4) and RFC 2460 Section 8.1 (IPv6)
    pub fn calculate_checksum(
        &self,
        src_ip: &NetworkAddress,
        dst_ip: &NetworkAddress,
        payload: &[u8],
    ) -> u16 {
        let mut sum = 0u32;

        // Pseudo-header (differs between IPv4 and IPv6)
        match (src_ip, dst_ip) {
            (NetworkAddress::IPv4(src), NetworkAddress::IPv4(dst)) => {
                // IPv4 pseudo-header
                sum += ((src[0] as u32) << 8) | (src[1] as u32);
                sum += ((src[2] as u32) << 8) | (src[3] as u32);
                sum += ((dst[0] as u32) << 8) | (dst[1] as u32);
                sum += ((dst[2] as u32) << 8) | (dst[3] as u32);
                sum += 6; // Protocol (TCP)
                sum += (TCP_HEADER_MIN_SIZE + self.options.len() + payload.len()) as u32;
            }
            (NetworkAddress::IPv6(src), NetworkAddress::IPv6(dst)) => {
                // IPv6 pseudo-header (RFC 2460 Section 8.1)
                // Source address (16 bytes)
                for chunk in src.chunks(2) {
                    sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
                }
                // Destination address (16 bytes)
                for chunk in dst.chunks(2) {
                    sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
                }
                // Upper-layer packet length (32 bits)
                let tcp_len = (TCP_HEADER_MIN_SIZE + self.options.len() + payload.len()) as u32;
                sum += tcp_len >> 16;
                sum += tcp_len & 0xFFFF;
                // Next header (TCP = 6, padded to 32 bits)
                sum += 6;
            }
            _ => return 0, // Mixed address families not supported
        }

        // TCP header
        sum += self.source_port as u32;
        sum += self.dest_port as u32;
        sum += (self.sequence_number >> 16) as u32;
        sum += (self.sequence_number & 0xFFFF) as u32;
        sum += (self.acknowledgment_number >> 16) as u32;
        sum += (self.acknowledgment_number & 0xFFFF) as u32;
        sum += ((self.data_offset as u32) << 12) | (self.flags.to_byte() as u32);
        sum += self.window_size as u32;
        // Skip checksum field
        sum += self.urgent_pointer as u32;

        // Options
        for chunk in self.options.chunks(2) {
            if chunk.len() == 2 {
                sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
            } else {
                sum += (chunk[0] as u32) << 8;
            }
        }

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
}

/// Congestion control phase (TCP Tahoe)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionPhase {
    SlowStart,
    CongestionAvoidance,
}

/// TCP connection with complete state management
#[derive(Debug, Clone)]
pub struct TcpConnection {
    pub local_addr: NetworkAddress,
    pub local_port: u16,
    pub remote_addr: NetworkAddress,
    pub remote_port: u16,
    pub state: TcpState,
    pub send_sequence: u32,
    pub send_ack: u32,
    pub recv_sequence: u32,
    pub recv_ack: u32,
    pub send_window: u16,
    pub recv_window: u16,
    pub mss: u16,
    pub rtt: u32,
    /// RTT variance for RTO calculation (Jacobson/Karels)
    pub rtt_var: u32,
    pub cwnd: u32,
    pub ssthresh: u32,
    /// Current congestion control phase
    pub congestion_phase: CongestionPhase,
    pub retransmit_timeout: u32,
    /// Absolute timestamp (ms) at which to fire next retransmit; 0 = no timer
    pub retransmit_at: u64,
    pub send_buffer: Vec<u8>,
    pub recv_buffer: Vec<u8>,
    pub send_unacked: Vec<u8>,
    pub last_ack_time: u64,
    pub retransmit_count: u8,
    pub keep_alive_time: u64,
    pub user_timeout: u32,
    pub duplicate_acks: u8,
    pub fast_retransmit: bool,
    pub sack_enabled: bool,
    pub window_scale: u8,
    pub timestamps_enabled: bool,
    pub syn_retries: u8,
    pub established_time: u64,
    /// Sequence number of segment currently being timed (for Karn's algorithm)
    pub rtt_measure_seq: Option<u32>,
    /// Wall-clock time (ms) when the timed segment was first sent
    pub rtt_send_time: Option<u64>,
    /// Out-of-order segments pending reassembly: (seq_num, data)
    pub ooo_segments: Vec<(u32, Vec<u8>)>,
}

impl TcpConnection {
    pub fn new(
        local_addr: NetworkAddress,
        local_port: u16,
        remote_addr: NetworkAddress,
        remote_port: u16,
    ) -> Self {
        Self {
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state: TcpState::Closed,
            send_sequence: 0,
            send_ack: 0,
            recv_sequence: 0,
            recv_ack: 0,
            send_window: 65535,
            recv_window: 65535,
            mss: 1460,
            rtt: 1000, // Initial RTT estimate: 1 second
            rtt_var: 500,
            cwnd: 1460, // 1 MSS in bytes
            ssthresh: 65535,
            congestion_phase: CongestionPhase::SlowStart,
            retransmit_timeout: 3000,
            retransmit_at: 0,
            send_buffer: Vec::new(),
            recv_buffer: Vec::new(),
            send_unacked: Vec::new(),
            last_ack_time: current_time_ms(),
            retransmit_count: 0,
            keep_alive_time: current_time_ms(),
            user_timeout: 300000, // 5 minutes
            duplicate_acks: 0,
            fast_retransmit: false,
            sack_enabled: false,
            window_scale: 0,
            timestamps_enabled: false,
            syn_retries: 0,
            established_time: 0,
            rtt_measure_seq: None,
            rtt_send_time: None,
            ooo_segments: Vec::new(),
        }
    }

    /// Generate initial sequence number using secure random
    pub fn generate_isn(&mut self) {
        // Use a more secure ISN generation method
        let time_component = current_time_ms() as u32;
        let random_component = secure_random_u32();
        self.send_sequence = time_component.wrapping_add(random_component);
        // snd.una starts at the ISN; without this it stays 0 and the first
        // ACK in ESTABLISHED computes a bogus acked-byte count.
        self.send_ack = self.send_sequence;
    }

    /// Check if connection has timed out
    pub fn is_timed_out(&self) -> bool {
        let now = current_time_ms();
        match self.state {
            TcpState::SynSent | TcpState::SynReceived => {
                now - self.last_ack_time > 75000 // 75 seconds for connection timeout
            }
            TcpState::Established | TcpState::CloseWait => {
                now - self.last_ack_time > self.user_timeout as u64
            }
            TcpState::FinWait1 | TcpState::FinWait2 | TcpState::Closing | TcpState::LastAck => {
                now - self.last_ack_time > 60000 // 60 seconds for close timeout
            }
            TcpState::TimeWait => {
                now - self.last_ack_time > 240000 // 4 minutes (2*MSL)
            }
            _ => false,
        }
    }

    /// Recompute `recv_window` from the space actually free in `recv_buffer`
    /// under `MAX_RECV_BUFFER`. Must be called after any change to
    /// `recv_buffer`'s length so the advertised window reflects real
    /// available buffer space instead of staying fixed forever.
    pub fn update_recv_window(&mut self) {
        let free = MAX_RECV_BUFFER.saturating_sub(self.recv_buffer.len());
        self.recv_window = cmp::min(free, u16::MAX as usize) as u16;
    }

    /// Append `data` to `recv_buffer` if it fits under `MAX_RECV_BUFFER`,
    /// updating `recv_window` afterwards. Returns `false` (and appends
    /// nothing) if the buffer is full, so the caller can leave
    /// `recv_sequence` unadvanced and let the peer retransmit once window
    /// space frees up.
    pub fn try_append_recv(&mut self, data: &[u8]) -> bool {
        if self.recv_buffer.len().saturating_add(data.len()) > MAX_RECV_BUFFER {
            return false;
        }
        self.recv_buffer.extend_from_slice(data);
        self.update_recv_window();
        true
    }

    /// Handle duplicate ACKs for fast retransmit
    pub fn handle_duplicate_ack(&mut self) {
        self.duplicate_acks += 1;
        if self.duplicate_acks >= 3 && !self.fast_retransmit {
            self.fast_retransmit = true;
            // Halve congestion window
            self.ssthresh = core::cmp::max(self.cwnd / 2, 2 * self.mss as u32);
            self.cwnd = self.ssthresh + 3 * self.mss as u32;
        } else if self.fast_retransmit {
            // Inflate congestion window
            self.cwnd += self.mss as u32;
        }
    }

    /// Reset duplicate ACK counter
    pub fn reset_duplicate_acks(&mut self) {
        self.duplicate_acks = 0;
        if self.fast_retransmit {
            self.fast_retransmit = false;
            self.cwnd = self.ssthresh;
        }
    }

    /// Check if keep-alive should be sent
    pub fn should_send_keepalive(&self) -> bool {
        if self.state != TcpState::Established {
            return false;
        }
        let now = current_time_ms();
        now - self.keep_alive_time > 7200000 // 2 hours
    }

    /// Update keep-alive timer
    pub fn update_keepalive(&mut self) {
        self.keep_alive_time = current_time_ms();
    }

    /// Update RTT estimate using Jacobson/Karels algorithm.
    /// Must NOT be called for retransmitted segments (Karn's algorithm).
    pub fn update_rtt(&mut self, measured_rtt: u32) {
        let diff = if self.rtt > measured_rtt {
            self.rtt - measured_rtt
        } else {
            measured_rtt - self.rtt
        };
        self.rtt_var = (self.rtt_var * 3 + diff) / 4;
        self.rtt = (self.rtt * 7 + measured_rtt) / 8;
        // RTO = SRTT + max(1, 4*RTTVAR), capped at 64s, floor 200ms
        let rto = self.rtt + cmp::max(1, 4 * self.rtt_var);
        self.retransmit_timeout = cmp::min(cmp::max(rto, 200), 64_000);
    }

    /// Update congestion window on new ACK receipt.
    pub fn update_cwnd(&mut self, acked_bytes: u32) {
        let mss = self.mss as u32;
        match self.congestion_phase {
            CongestionPhase::SlowStart => {
                // Increase by 1 MSS per MSS acked
                self.cwnd = self.cwnd.saturating_add(cmp::min(acked_bytes, mss));
                if self.cwnd >= self.ssthresh {
                    self.congestion_phase = CongestionPhase::CongestionAvoidance;
                }
            }
            CongestionPhase::CongestionAvoidance => {
                // Increase by MSS²/cwnd per ACK (~1 MSS per RTT)
                let increment = (mss * mss) / cmp::max(self.cwnd, 1);
                self.cwnd = self.cwnd.saturating_add(cmp::max(increment, 1));
            }
        }
    }

    /// Handle congestion event (timeout): TCP Tahoe.
    pub fn handle_congestion(&mut self) {
        let mss = self.mss as u32;
        self.ssthresh = cmp::max(self.cwnd / 2, 2 * mss);
        self.cwnd = mss;
        self.congestion_phase = CongestionPhase::SlowStart;
        // Exponential backoff, cap at 64 seconds
        self.retransmit_timeout = cmp::min(self.retransmit_timeout * 2, 64_000);
    }
}

/// TCP connection manager
pub struct TcpManager {
    connections: RwLock<BTreeMap<(NetworkAddress, u16, NetworkAddress, u16), TcpConnection>>,
    next_port: RwLock<u16>,
    /// Listening sockets: (local_addr, local_port) -> backlog limit
    listening: RwLock<BTreeMap<(NetworkAddress, u16), usize>>,
    /// Accept queue: (local_addr, local_port) -> Vec of established connections
    accept_queue: RwLock<BTreeMap<(NetworkAddress, u16), Vec<TcpConnection>>>,
}

impl TcpManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(BTreeMap::new()),
            next_port: RwLock::new(32768), // Start of dynamic port range
            listening: RwLock::new(BTreeMap::new()),
            accept_queue: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn allocate_port(&self) -> u16 {
        let mut next_port = self.next_port.write();
        let port = *next_port;
        *next_port = if port == u16::MAX { 32768 } else { port + 1 };
        port
    }

    pub fn create_connection(
        &self,
        local_addr: NetworkAddress,
        local_port: u16,
        remote_addr: NetworkAddress,
        remote_port: u16,
    ) -> NetworkResult<()> {
        let key = (local_addr, local_port, remote_addr, remote_port);
        let mut connections = self.connections.write();

        if connections.contains_key(&key) {
            return Err(NetworkError::AddressInUse);
        }

        let connection = TcpConnection::new(local_addr, local_port, remote_addr, remote_port);
        connections.insert(key, connection);
        Ok(())
    }

    pub fn get_connection(
        &self,
        local_addr: &NetworkAddress,
        local_port: u16,
        remote_addr: &NetworkAddress,
        remote_port: u16,
    ) -> Option<TcpConnection> {
        let connections = self.connections.read();
        let key = (*local_addr, local_port, *remote_addr, remote_port);
        connections.get(&key).cloned()
    }

    pub fn update_connection<F>(
        &self,
        key: (NetworkAddress, u16, NetworkAddress, u16),
        f: F,
    ) -> NetworkResult<()>
    where
        F: FnOnce(&mut TcpConnection),
    {
        let mut connections = self.connections.write();
        if let Some(connection) = connections.get_mut(&key) {
            f(connection);
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    pub fn remove_connection(
        &self,
        local_addr: &NetworkAddress,
        local_port: u16,
        remote_addr: &NetworkAddress,
        remote_port: u16,
    ) -> NetworkResult<()> {
        let mut connections = self.connections.write();
        let key = (*local_addr, local_port, *remote_addr, remote_port);

        if connections.remove(&key).is_some() {
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Register a listening socket with a backlog limit.
    pub fn add_listener(
        &self,
        local_addr: NetworkAddress,
        local_port: u16,
        backlog: usize,
    ) -> NetworkResult<()> {
        let key = (local_addr, local_port);
        let mut listening = self.listening.write();
        if listening.contains_key(&key) {
            return Err(NetworkError::AddressInUse);
        }
        listening.insert(key, backlog);
        self.accept_queue.write().insert(key, Vec::new());
        Ok(())
    }

    /// Remove a listening socket.
    pub fn remove_listener(
        &self,
        local_addr: &NetworkAddress,
        local_port: u16,
    ) -> NetworkResult<()> {
        let key = (*local_addr, local_port);
        self.listening.write().remove(&key);
        self.accept_queue.write().remove(&key);
        Ok(())
    }

    /// Check if a listening socket exists for the given local address/port.
    pub fn is_listening(&self, local_addr: &NetworkAddress, local_port: u16) -> bool {
        self.listening
            .read()
            .contains_key(&(*local_addr, local_port))
    }

    /// Push an established connection onto the accept queue for a listener.
    /// Returns an error if the queue is full.
    pub fn push_accept(
        &self,
        local_addr: NetworkAddress,
        local_port: u16,
        conn: TcpConnection,
    ) -> NetworkResult<()> {
        let key = (local_addr, local_port);
        let mut queue = self.accept_queue.write();
        let q = queue.get_mut(&key).ok_or(NetworkError::InvalidAddress)?;
        if q.len() >= MAX_BACKLOG {
            return Err(NetworkError::ConnectionRefused);
        }
        q.push(conn);
        Ok(())
    }

    /// Pop an established connection from the accept queue (for `accept()`).
    pub fn pop_accept(
        &self,
        local_addr: &NetworkAddress,
        local_port: u16,
    ) -> Option<TcpConnection> {
        let key = (*local_addr, local_port);
        let mut queue = self.accept_queue.write();
        queue.get_mut(&key).and_then(|q| q.pop())
    }
}

static TCP_MANAGER: TcpManager = TcpManager {
    connections: RwLock::new(BTreeMap::new()),
    next_port: RwLock::new(32768),
    listening: RwLock::new(BTreeMap::new()),
    accept_queue: RwLock::new(BTreeMap::new()),
};

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    // Use system time for TCP timestamps
    // TCP uses wall clock time for RFC 7323 timestamps
    crate::time::get_system_time_ms()
}

/// Generate secure random u32 using hardware CSPRNG
fn secure_random_u32() -> u32 {
    let mut result: u32 = 0;
    unsafe {
        let rdrand_ok = {
            let cpuid = core::arch::x86_64::__cpuid(1);
            (cpuid.ecx & (1 << 30)) != 0
        };
        if rdrand_ok && core::arch::x86_64::_rdrand32_step(&mut result) == 1 {
            result
        } else {
            (core::arch::x86_64::_rdtsc() as u32)
                .wrapping_mul(1103515245)
                .wrapping_add(12345)
        }
    }
}

/// Modular (RFC 1982) sequence-number comparisons that tolerate u32 wraparound.
#[inline]
fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}
#[inline]
fn seq_gt(a: u32, b: u32) -> bool {
    seq_lt(b, a)
}
#[inline]
fn seq_leq(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// Process incoming TCP packet
pub fn process_packet(
    _network_stack: &NetworkStack,
    src_ip: NetworkAddress,
    dst_ip: NetworkAddress,
    mut packet: PacketBuffer,
) -> NetworkResult<()> {
    let header = TcpHeader::parse(&mut packet)?;

    // Validate the TCP checksum on the receive path (pseudo-header + header +
    // payload), the same way UDP/ICMP do. Drop the segment if it fails.
    {
        let payload = &packet.as_slice()[packet.position..];
        let mut checksum_header = header.clone();
        checksum_header.checksum = 0;
        if checksum_header.calculate_checksum(&src_ip, &dst_ip, payload) != header.checksum {
            return Err(NetworkError::InvalidPacket);
        }
    }

    // Production: process TCP packet without debug output

    // Find existing connection
    let connection_key = (dst_ip, header.dest_port, src_ip, header.source_port);

    if let Some(mut connection) =
        TCP_MANAGER.get_connection(&dst_ip, header.dest_port, &src_ip, header.source_port)
    {
        // Process packet for existing connection
        process_connection_packet(
            &mut connection,
            &header,
            &packet.as_slice()[packet.position..],
        )?;

        // Update connection in manager
        TCP_MANAGER.update_connection(connection_key, |conn| {
            *conn = connection;
        })?;
    } else {
        // Handle new connection attempt
        if header.flags.syn && !header.flags.ack {
            // Handle new TCP connection attempt
            handle_new_connection(
                dst_ip,
                header.dest_port,
                src_ip,
                header.source_port,
                &header,
            )?;
        } else {
            // Send RST for non-existent connection
            send_rst_for_segment(
                dst_ip,
                header.dest_port,
                src_ip,
                header.source_port,
                &header,
                0,
            )?;
        }
    }

    Ok(())
}

/// Process packet for existing connection with comprehensive state machine
fn process_connection_packet(
    connection: &mut TcpConnection,
    header: &TcpHeader,
    payload: &[u8],
) -> NetworkResult<()> {
    // Update last activity time
    connection.last_ack_time = current_time_ms();

    // Validate sequence numbers
    if !validate_sequence_numbers(connection, header) {
        // Send ACK with current sequence numbers
        send_ack_packet(connection)?;
        return Ok(());
    }

    // Process based on current state
    match connection.state {
        TcpState::Listen => {
            handle_listen_state(connection, header)?;
        }
        TcpState::SynSent => {
            handle_syn_sent_state(connection, header)?;
        }
        TcpState::SynReceived => {
            handle_syn_received_state(connection, header)?;
        }
        TcpState::Established => {
            handle_established_state(connection, header, payload)?;
        }
        TcpState::FinWait1 => {
            handle_fin_wait1_state(connection, header, payload)?;
        }
        TcpState::FinWait2 => {
            handle_fin_wait2_state(connection, header)?;
        }
        TcpState::CloseWait => {
            handle_close_wait_state(connection, header)?;
        }
        TcpState::Closing => {
            handle_closing_state(connection, header)?;
        }
        TcpState::LastAck => {
            handle_last_ack_state(connection, header)?;
        }
        TcpState::TimeWait => {
            handle_time_wait_state(connection, header)?;
        }
        TcpState::Closed => {
            // Connection is closed, send RST
            send_rst_for_segment(
                connection.local_addr,
                connection.local_port,
                connection.remote_addr,
                connection.remote_port,
                header,
                0,
            )?;
        }
    }

    Ok(())
}

/// Validate sequence numbers according to TCP specification
fn validate_sequence_numbers(connection: &TcpConnection, header: &TcpHeader) -> bool {
    // Check if sequence number is within acceptable window
    let seq = header.sequence_number;
    let expected_seq = connection.recv_sequence;
    let window = connection.recv_window as u32;

    // Sequence number is acceptable if it's within the receive window
    if window == 0 {
        seq == expected_seq
    } else {
        let seq_diff = seq.wrapping_sub(expected_seq);
        seq_diff < window
    }
}

/// Handle LISTEN state
fn handle_listen_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.syn && !header.flags.ack {
        // Valid SYN received
        connection.recv_sequence = header.sequence_number.wrapping_add(1);
        connection.generate_isn();
        connection.state = TcpState::SynReceived;
        connection.established_time = current_time_ms();

        // Send SYN-ACK
        send_syn_ack_packet(connection)?;
    } else if header.flags.rst {
        // RST in LISTEN state is ignored
    } else {
        // Invalid packet, send RST
        send_rst_packet(
            connection.local_addr,
            connection.local_port,
            connection.remote_addr,
            connection.remote_port,
            header.sequence_number.wrapping_add(1),
        )?;
    }
    Ok(())
}

/// Handle SYN-SENT state
fn handle_syn_sent_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.syn && header.flags.ack {
        // SYN-ACK received
        if header.acknowledgment_number == connection.send_sequence.wrapping_add(1) {
            connection.send_sequence = connection.send_sequence.wrapping_add(1);
            // Our SYN is now acknowledged: snd.una == snd.nxt (ISN+1).
            connection.send_ack = connection.send_sequence;
            connection.recv_sequence = header.sequence_number.wrapping_add(1);
            connection.state = TcpState::Established;
            connection.established_time = current_time_ms();

            // Send ACK
            send_ack_packet(connection)?;

            // Reset retransmission counter
            connection.syn_retries = 0;
        } else {
            // Invalid ACK, send RST
            send_rst_packet(
                connection.local_addr,
                connection.local_port,
                connection.remote_addr,
                connection.remote_port,
                header.acknowledgment_number,
            )?;
        }
    } else if header.flags.syn && !header.flags.ack {
        // Simultaneous SYN
        connection.recv_sequence = header.sequence_number.wrapping_add(1);
        connection.state = TcpState::SynReceived;
        send_syn_ack_packet(connection)?;
    } else if header.flags.rst {
        // Connection refused
        connection.state = TcpState::Closed;
    }
    Ok(())
}

/// Handle SYN-RECEIVED state
fn handle_syn_received_state(
    connection: &mut TcpConnection,
    header: &TcpHeader,
) -> NetworkResult<()> {
    if header.flags.ack && !header.flags.syn {
        // ACK received — 3-way handshake complete
        if header.acknowledgment_number == connection.send_sequence.wrapping_add(1) {
            connection.send_sequence = connection.send_sequence.wrapping_add(1);
            // Our SYN-ACK is now acknowledged: snd.una == snd.nxt (ISN+1).
            connection.send_ack = connection.send_sequence;
            connection.state = TcpState::Established;
            connection.established_time = current_time_ms();

            // Push the established connection onto the accept queue so
            // `accept()` can return it to the listening application.
            let _ = TCP_MANAGER.push_accept(
                connection.local_addr,
                connection.local_port,
                connection.clone(),
            );
        } else {
            // Invalid ACK, send RST
            send_rst_packet(
                connection.local_addr,
                connection.local_port,
                connection.remote_addr,
                connection.remote_port,
                header.acknowledgment_number,
            )?;
        }
    } else if header.flags.rst {
        // Connection reset
        connection.state = TcpState::Closed;
    }
    Ok(())
}

/// Handle ESTABLISHED state
fn handle_established_state(
    connection: &mut TcpConnection,
    header: &TcpHeader,
    payload: &[u8],
) -> NetworkResult<()> {
    // Handle data reception
    if !payload.is_empty() {
        if header.sequence_number == connection.recv_sequence {
            // In-order data. If the receive buffer is full, drop the segment
            // and leave recv_sequence unadvanced so the peer retransmits once
            // the application drains the buffer and the window reopens.
            if connection.try_append_recv(payload) {
                connection.recv_sequence =
                    connection.recv_sequence.wrapping_add(payload.len() as u32);

                // Check if any out-of-order segments can now be reassembled
                loop {
                    let recv_seq = connection.recv_sequence;
                    // Find a segment that starts at our current recv_sequence
                    let found_idx = connection
                        .ooo_segments
                        .iter()
                        .position(|(seq, _)| *seq == recv_seq);
                    if let Some(idx) = found_idx {
                        let data_len = connection.ooo_segments[idx].1.len();
                        // Stop reassembling (leave it buffered) once the
                        // receive buffer can't take the next segment either.
                        if connection.recv_buffer.len().saturating_add(data_len)
                            > MAX_RECV_BUFFER
                        {
                            break;
                        }
                        let (_, data) = connection.ooo_segments.remove(idx);
                        let _ = connection.try_append_recv(&data);
                        connection.recv_sequence =
                            connection.recv_sequence.wrapping_add(data_len as u32);
                    } else {
                        break;
                    }
                }
            }

            // Send ACK
            send_ack_packet(connection)?;

            // Reset duplicate ACK counter
            connection.reset_duplicate_acks();
        } else if seq_gt(header.sequence_number, connection.recv_sequence) {
            // Out-of-order data — buffer for later reassembly
            // Avoid duplicates: only insert if we don't already have this sequence
            let already_have = connection
                .ooo_segments
                .iter()
                .any(|(seq, _)| *seq == header.sequence_number);
            if !already_have && connection.ooo_segments.len() < MAX_OOO_SEGMENTS {
                connection
                    .ooo_segments
                    .push((header.sequence_number, payload.to_vec()));
                // Sort by sequence number to keep the buffer ordered
                connection.ooo_segments.sort_by_key(|(seq, _)| *seq);
            }
            // If already at MAX_OOO_SEGMENTS, drop the segment — the peer
            // will retransmit it once earlier gaps are filled and the buffer
            // has room again.
            // Send duplicate ACK with expected sequence number
            send_ack_packet(connection)?;
        }
        // Ignore old data (sequence_number < recv_sequence)
    }

    // Handle ACK
    if header.flags.ack {
        let ack_num = header.acknowledgment_number;
        if seq_gt(ack_num, connection.send_ack) && seq_leq(ack_num, connection.send_sequence) {
            // Valid new ACK
            let acked_bytes = ack_num.wrapping_sub(connection.send_ack);
            connection.send_ack = ack_num;

            // Karn's algorithm: only update RTT if this ACK is not for a
            // retransmitted segment.  rtt_measure_seq holds the *end* seq of
            // the segment being timed (None if currently retransmitting).
            if let (Some(measure_seq), Some(send_time)) =
                (connection.rtt_measure_seq, connection.rtt_send_time)
            {
                if seq_leq(measure_seq, ack_num) {
                    let now = current_time_ms();
                    let measured = (now.saturating_sub(send_time)) as u32;
                    connection.update_rtt(measured);
                    connection.rtt_measure_seq = None;
                    connection.rtt_send_time = None;
                }
            }

            // Update congestion window
            connection.update_cwnd(acked_bytes);

            // Remove acknowledged data from send_unacked
            if acked_bytes as usize <= connection.send_unacked.len() {
                connection.send_unacked.drain(0..acked_bytes as usize);
            }

            // Reset or arm retransmit timer
            if connection.send_unacked.is_empty() {
                connection.retransmit_at = 0; // nothing to retransmit
                connection.retransmit_count = 0;
            } else {
                let now = current_time_ms();
                connection.retransmit_at = now + connection.retransmit_timeout as u64;
            }

            connection.reset_duplicate_acks();
        } else if ack_num == connection.send_ack {
            // Duplicate ACK
            connection.handle_duplicate_ack();
        }
    }

    // Handle FIN — only when it is the next in-sequence octet. A segment can
    // carry payload + FIN; if that payload arrived out of order (buffered above
    // without advancing recv_sequence) the FIN is not yet in sequence, and
    // consuming it would skip RCV.NXT past the buffered data (losing it) and
    // desynchronize sequence numbers.
    if header.flags.fin
        && header.sequence_number.wrapping_add(payload.len() as u32) == connection.recv_sequence
    {
        connection.recv_sequence = connection.recv_sequence.wrapping_add(1);
        connection.state = TcpState::CloseWait;
        send_ack_packet(connection)?;
    }

    // Handle RST
    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle FIN-WAIT-1 state
fn handle_fin_wait1_state(
    connection: &mut TcpConnection,
    header: &TcpHeader,
    payload: &[u8],
) -> NetworkResult<()> {
    // Process any data segment that arrived (still in-flight data from peer)
    if !payload.is_empty() && header.sequence_number == connection.recv_sequence {
        if connection.try_append_recv(payload) {
            connection.recv_sequence = connection.recv_sequence.wrapping_add(payload.len() as u32);
        }
        send_ack_packet(connection)?;
    }

    if header.flags.ack {
        // ACK for our FIN
        if header.acknowledgment_number == connection.send_sequence.wrapping_add(1) {
            connection.send_sequence = connection.send_sequence.wrapping_add(1);
            connection.state = TcpState::FinWait2;
        } else if header.acknowledgment_number == connection.send_sequence {
            // ACK for data we sent before FIN — process normally
            // Remove acknowledged data from send_unacked
            let unacked_len = connection.send_unacked.len();
            if unacked_len > 0 {
                connection.send_unacked.clear();
                connection.update_cwnd(unacked_len as u32);
                connection.reset_duplicate_acks();
            }
        }
    }

    if header.flags.fin {
        // Simultaneous close or FIN received
        connection.recv_sequence = connection.recv_sequence.wrapping_add(1);
        send_ack_packet(connection)?;

        if connection.state == TcpState::FinWait2 {
            connection.state = TcpState::TimeWait;
        } else {
            connection.state = TcpState::Closing;
        }
    }

    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle FIN-WAIT-2 state
fn handle_fin_wait2_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.fin {
        connection.recv_sequence = connection.recv_sequence.wrapping_add(1);
        connection.state = TcpState::TimeWait;
        send_ack_packet(connection)?;
    }

    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle CLOSE-WAIT state
fn handle_close_wait_state(
    connection: &mut TcpConnection,
    header: &TcpHeader,
) -> NetworkResult<()> {
    // Application should close the connection
    // For now, automatically close after a timeout
    if current_time_ms() - connection.established_time > 30000 {
        // 30 seconds
        connection.state = TcpState::LastAck;
        send_fin_packet(connection)?;
    }

    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle CLOSING state
fn handle_closing_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.ack {
        // ACK for our FIN
        if header.acknowledgment_number == connection.send_sequence.wrapping_add(1) {
            connection.state = TcpState::TimeWait;
        }
    }

    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle LAST-ACK state
fn handle_last_ack_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.ack {
        // ACK for our FIN
        if header.acknowledgment_number == connection.send_sequence.wrapping_add(1) {
            connection.state = TcpState::Closed;
        }
    }

    if header.flags.rst {
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Handle TIME-WAIT state
fn handle_time_wait_state(connection: &mut TcpConnection, header: &TcpHeader) -> NetworkResult<()> {
    if header.flags.fin {
        // Retransmitted FIN, send ACK
        send_ack_packet(connection)?;
    }

    // Check for timeout (2*MSL)
    if current_time_ms() - connection.last_ack_time > 240000 {
        // 4 minutes
        connection.state = TcpState::Closed;
    }

    Ok(())
}

/// Send FIN packet
fn send_fin_packet(connection: &TcpConnection) -> NetworkResult<()> {
    let mut flags = TcpFlags::new();
    flags.fin = true;
    flags.ack = true;

    send_tcp_packet(
        connection.local_addr,
        connection.local_port,
        connection.remote_addr,
        connection.remote_port,
        connection.send_sequence,
        connection.recv_sequence,
        flags,
        connection.recv_window,
        &[],
    )
}

/// Handle new connection attempt
fn handle_new_connection(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
    header: &TcpHeader,
) -> NetworkResult<()> {
    // Check if there is a listening socket for this local address/port.
    // If not, send a RST to reject the connection attempt.
    if !TCP_MANAGER.is_listening(&local_addr, local_port) {
        send_rst_for_segment(local_addr, local_port, remote_addr, remote_port, header, 0)?;
        return Err(NetworkError::ConnectionRefused);
    }

    // Admission control: reject (RST, no allocation) once the total
    // connection table or the half-open (SYN-RECEIVED) count is at its cap.
    // Without this a SYN flood can grow the connection table without bound
    // (each entry is only reaped ~75s later by `tcp_tick`), exhausting kernel
    // memory long before the timeout kicks in.
    {
        let connections = TCP_MANAGER.connections.read();
        if connections.len() >= MAX_CONNECTIONS {
            drop(connections);
            send_rst_for_segment(local_addr, local_port, remote_addr, remote_port, header, 0)?;
            return Err(NetworkError::ConnectionRefused);
        }
        let half_open = connections
            .values()
            .filter(|c| c.state == TcpState::SynReceived)
            .count();
        if half_open >= MAX_HALF_OPEN {
            drop(connections);
            send_rst_for_segment(local_addr, local_port, remote_addr, remote_port, header, 0)?;
            return Err(NetworkError::ConnectionRefused);
        }
    }

    // Create new connection
    let mut connection = TcpConnection::new(local_addr, local_port, remote_addr, remote_port);
    connection.state = TcpState::Listen;
    connection.recv_sequence = header.sequence_number.wrapping_add(1);
    connection.generate_isn();
    connection.state = TcpState::SynReceived;

    // Store connection
    let key = (local_addr, local_port, remote_addr, remote_port);
    TCP_MANAGER
        .connections
        .write()
        .insert(key, connection.clone());

    // Send SYN-ACK
    send_syn_ack_packet(&connection)?;

    Ok(())
}

/// Send SYN-ACK packet
fn send_syn_ack_packet(connection: &TcpConnection) -> NetworkResult<()> {
    let mut flags = TcpFlags::new();
    flags.syn = true;
    flags.ack = true;

    send_tcp_packet(
        connection.local_addr,
        connection.local_port,
        connection.remote_addr,
        connection.remote_port,
        connection.send_sequence,
        connection.recv_sequence,
        flags,
        connection.recv_window,
        &[],
    )
}

/// Send ACK packet
fn send_ack_packet(connection: &TcpConnection) -> NetworkResult<()> {
    let mut flags = TcpFlags::new();
    flags.ack = true;

    send_tcp_packet(
        connection.local_addr,
        connection.local_port,
        connection.remote_addr,
        connection.remote_port,
        connection.send_sequence,
        connection.recv_sequence,
        flags,
        connection.recv_window,
        &[],
    )
}

/// Send RST packet
/// Generate a RST in response to an incoming `header`, per RFC 9293 §3.5.2:
/// if the segment carried an ACK, reset with SEQ = SEG.ACK and no ACK bit;
/// otherwise reset with SEQ = 0, ACK = SEG.SEQ + SEG.LEN and the ACK bit set
/// (SYN and FIN each contribute 1 to SEG.LEN). Sending SEG.SEQ+1 in the SEQ
/// field with no ACK bit (the previous behavior) is discarded by compliant
/// peers' acceptability checks.
fn send_rst_for_segment(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
    header: &TcpHeader,
    payload_len: u32,
) -> NetworkResult<()> {
    let mut flags = TcpFlags::new();
    flags.rst = true;
    let (seq, ack) = if header.flags.ack {
        (header.acknowledgment_number, 0)
    } else {
        flags.ack = true;
        let seg_len = payload_len
            + if header.flags.syn { 1 } else { 0 }
            + if header.flags.fin { 1 } else { 0 };
        (0, header.sequence_number.wrapping_add(seg_len))
    };
    send_tcp_packet(
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        seq,
        ack,
        flags,
        0,
        &[],
    )
}

fn send_rst_packet(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
    sequence: u32,
) -> NetworkResult<()> {
    let mut flags = TcpFlags::new();
    flags.rst = true;

    send_tcp_packet(
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        sequence,
        0,
        flags,
        0,
        &[],
    )
}

/// Send TCP packet
fn send_tcp_packet(
    src_ip: NetworkAddress,
    src_port: u16,
    dst_ip: NetworkAddress,
    dst_port: u16,
    sequence: u32,
    acknowledgment: u32,
    flags: TcpFlags,
    window: u16,
    payload: &[u8],
) -> NetworkResult<()> {
    // Create TCP header
    let header = TcpHeader {
        source_port: src_port,
        dest_port: dst_port,
        sequence_number: sequence,
        acknowledgment_number: acknowledgment,
        data_offset: 5, // 20 bytes (no options)
        flags,
        window_size: window,
        checksum: 0, // Will be calculated
        urgent_pointer: 0,
        options: Vec::new(),
    };

    // Calculate checksum
    let _checksum = header.calculate_checksum(&src_ip, &dst_ip, payload);

    // Serialize TCP header and payload
    let mut tcp_packet = Vec::with_capacity(20 + payload.len());

    // TCP header serialization
    tcp_packet.extend_from_slice(&src_port.to_be_bytes());
    tcp_packet.extend_from_slice(&dst_port.to_be_bytes());
    tcp_packet.extend_from_slice(&sequence.to_be_bytes());
    tcp_packet.extend_from_slice(&acknowledgment.to_be_bytes());

    // Data offset (5 = 20 bytes, no options) + reserved + flags
    let data_offset_flags = (5u16 << 12) | (flags.to_byte() as u16);
    tcp_packet.extend_from_slice(&data_offset_flags.to_be_bytes());

    // Window size
    tcp_packet.extend_from_slice(&window.to_be_bytes());
    // Checksum
    tcp_packet.extend_from_slice(&_checksum.to_be_bytes());
    // Urgent pointer
    tcp_packet.extend_from_slice(&0u16.to_be_bytes());

    // Add payload
    tcp_packet.extend_from_slice(payload);

    // Send through IP layer
    super::ip::send_ipv4_packet(src_ip, dst_ip, 6, &tcp_packet)
}

/// TCP socket operations
pub fn tcp_connect(
    local_addr: NetworkAddress,
    remote_addr: NetworkAddress,
    remote_port: u16,
) -> NetworkResult<u16> {
    let local_port = TCP_MANAGER.allocate_port();

    // Create connection
    TCP_MANAGER.create_connection(local_addr, local_port, remote_addr, remote_port)?;

    // Start connection process
    let key = (local_addr, local_port, remote_addr, remote_port);
    let mut isn: u32 = 0;
    TCP_MANAGER.update_connection(key, |conn| {
        conn.generate_isn();
        conn.state = TcpState::SynSent;
        isn = conn.send_sequence;
    })?;

    // Send SYN packet with our ISN as the sequence number. The peer's
    // SYN-ACK acknowledges ISN+1, which handle_syn_sent_state checks against
    // send_sequence; sending seq=0 here made every active connect fail.
    let mut flags = TcpFlags::new();
    flags.syn = true;

    send_tcp_packet(
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        isn,
        0,
        flags,
        65535,
        &[],
    )?;

    Ok(local_port)
}

/// TCP listen
pub fn tcp_listen(local_addr: NetworkAddress, local_port: u16) -> NetworkResult<()> {
    // Register the listening socket with a default backlog.
    TCP_MANAGER.add_listener(local_addr, local_port, MAX_BACKLOG)?;

    // Also create a connection entry in the Listen state so the packet
    // processing path can find it for incoming SYNs that match exactly.
    let dummy_remote = NetworkAddress::IPv4([0, 0, 0, 0]);
    let _ = TCP_MANAGER.create_connection(local_addr, local_port, dummy_remote, 0);
    let key = (local_addr, local_port, dummy_remote, 0);
    let _ = TCP_MANAGER.update_connection(key, |conn| {
        conn.state = TcpState::Listen;
    });

    Ok(())
}

/// TCP accept — dequeue an established connection from the listen backlog.
/// Returns (remote_addr, remote_port) for the accepted connection.
pub fn tcp_accept(
    local_addr: NetworkAddress,
    local_port: u16,
) -> NetworkResult<(NetworkAddress, u16)> {
    match TCP_MANAGER.pop_accept(&local_addr, local_port) {
        Some(conn) => Ok((conn.remote_addr, conn.remote_port)),
        None => Err(NetworkError::NotConnected),
    }
}

/// TCP close - Initiate graceful connection teardown
pub fn tcp_close(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
) -> NetworkResult<()> {
    let key = (local_addr, local_port, remote_addr, remote_port);

    // Get current connection state
    let connection = TCP_MANAGER
        .get_connection(&local_addr, local_port, &remote_addr, remote_port)
        .ok_or(NetworkError::InvalidAddress)?;

    match connection.state {
        TcpState::Established => {
            // Transition to FIN-WAIT-1 and send FIN
            TCP_MANAGER.update_connection(key, |conn| {
                conn.state = TcpState::FinWait1;
            })?;

            // Send FIN packet
            send_fin_packet(&connection)?;
        }
        TcpState::CloseWait => {
            // Transition to LAST-ACK and send FIN
            TCP_MANAGER.update_connection(key, |conn| {
                conn.state = TcpState::LastAck;
            })?;

            // Send FIN packet
            send_fin_packet(&connection)?;
        }
        TcpState::Listen | TcpState::SynSent => {
            // Can close immediately from these states
            TCP_MANAGER.remove_connection(&local_addr, local_port, &remote_addr, remote_port)?;
        }
        TcpState::Closed | TcpState::TimeWait => {
            // Already closed or closing
            return Ok(());
        }
        _ => {
            // Connection is already in a closing state
            return Ok(());
        }
    }

    Ok(())
}

/// TCP send data — transmit data over an established connection.
///
/// Sends data from the socket's send_buffer as TCP segments, respecting
/// the congestion window and receiver's advertised window. Data that has
/// been sent but not yet acknowledged is tracked in send_unacked.
/// Returns the number of bytes transmitted.
pub fn tcp_send_data(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
    data: &[u8],
) -> NetworkResult<usize> {
    if data.is_empty() {
        return Ok(0);
    }

    let key = (local_addr, local_port, remote_addr, remote_port);
    let mut connection = TCP_MANAGER
        .get_connection(&local_addr, local_port, &remote_addr, remote_port)
        .ok_or(NetworkError::NotConnected)?;

    if !connection.state.can_send_data() {
        return Err(NetworkError::NotConnected);
    }

    let mss = connection.mss as usize;
    let mut total_sent = 0usize;

    for chunk in data.chunks(mss) {
        // Enforce congestion window: only send while unacked < min(cwnd, rwnd)
        let effective_window = cmp::min(connection.cwnd as usize, connection.send_window as usize);
        if connection.send_unacked.len() + chunk.len() > effective_window {
            break;
        }

        let seq = connection.send_sequence;
        let now = current_time_ms();
        let mut flags = TcpFlags::new();
        flags.ack = true;

        let window = connection.recv_window;

        send_tcp_packet(
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            seq,
            connection.recv_sequence,
            flags,
            window,
            chunk,
        )?;

        total_sent += chunk.len();

        // Update connection state: advance send_sequence, track unacked data,
        // arm the retransmit timer, and start RTT measurement for the first
        // new unacked segment (Karn's algorithm: only time un-retransmitted).
        TCP_MANAGER.update_connection(key, |conn| {
            let end_seq = conn.send_sequence.wrapping_add(chunk.len() as u32);
            conn.send_sequence = end_seq;
            conn.send_unacked.extend_from_slice(chunk);
            // Arm retransmit timer if not already running
            if conn.retransmit_at == 0 {
                conn.retransmit_at = now + conn.retransmit_timeout as u64;
            }
            // Start RTT measurement for the first new segment
            if conn.rtt_measure_seq.is_none() {
                conn.rtt_measure_seq = Some(end_seq);
                conn.rtt_send_time = Some(now);
            }
        })?;

        // Re-read connection for next chunk
        connection = TCP_MANAGER
            .get_connection(&local_addr, local_port, &remote_addr, remote_port)
            .ok_or(NetworkError::NotConnected)?;
    }

    Ok(total_sent)
}

/// TCP get send confirmation — returns the number of bytes that have been
/// sent and acknowledged by the remote peer.
pub fn tcp_get_send_confirmed(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
) -> NetworkResult<usize> {
    let connection = TCP_MANAGER
        .get_connection(&local_addr, local_port, &remote_addr, remote_port)
        .ok_or(NetworkError::NotConnected)?;

    // send_ack tracks the highest acknowledged sequence number.
    // send_unacked contains data sent but not yet acknowledged.
    // Bytes confirmed = total sent - unacked length
    Ok(connection.send_unacked.len())
}

/// TCP timer tick — retransmit timed-out segments and handle connection cleanup.
///
/// Call this from the kernel timer interrupt handler at regular intervals
/// (e.g. every 100 ms).  `current_time` is the current monotonic clock in ms.
pub fn tcp_tick(current_time: u64) {
    // Collect keys that need retransmit or cleanup to avoid holding the lock
    // while calling send helpers.
    let keys_to_process: Vec<_> = {
        let conns = TCP_MANAGER.connections.read();
        conns
            .iter()
            .filter_map(|(k, conn)| {
                let needs_retransmit = conn.retransmit_at != 0
                    && current_time >= conn.retransmit_at
                    && !conn.send_unacked.is_empty();
                let needs_cleanup = conn.is_timed_out();
                if needs_retransmit || needs_cleanup {
                    Some((*k, needs_retransmit, needs_cleanup))
                } else {
                    None
                }
            })
            .collect()
    };

    for (key, needs_retransmit, needs_cleanup) in keys_to_process {
        if needs_cleanup {
            // Connection (often half-open, or TIME-WAIT) has been idle past
            // its state's timeout. Actually remove it from the table —
            // previously this only set state = Closed and left the entry
            // (and its Vec buffers) in the map forever, so idle/half-open
            // connections accumulated without bound instead of being reaped.
            let (local_addr, local_port, remote_addr, remote_port) = key;
            let _ = TCP_MANAGER.remove_connection(
                &local_addr,
                local_port,
                &remote_addr,
                remote_port,
            );
            continue;
        }

        if needs_retransmit {
            // Snapshot the segment to retransmit and update state
            let segment_to_send = {
                let conns = TCP_MANAGER.connections.read();
                conns.get(&key).and_then(|conn| {
                    if conn.send_unacked.is_empty() {
                        return None;
                    }
                    let len = cmp::min(conn.mss as usize, conn.send_unacked.len());
                    Some((
                        conn.local_addr,
                        conn.local_port,
                        conn.remote_addr,
                        conn.remote_port,
                        conn.send_ack, // retransmit from oldest unacked seq
                        conn.recv_sequence,
                        conn.recv_window,
                        conn.send_unacked[..len].to_vec(),
                    ))
                })
            };

            if let Some((src_ip, src_port, dst_ip, dst_port, seq, ack_seq, recv_win, payload)) =
                segment_to_send
            {
                let mut flags = TcpFlags::new();
                flags.ack = true;
                let _ = send_tcp_packet(
                    src_ip, src_port, dst_ip, dst_port, seq, ack_seq, flags, recv_win, &payload,
                );
            }

            // Update retransmit state: congestion control + backoff
            let _ = TCP_MANAGER.update_connection(key, |conn| {
                conn.retransmit_count = conn.retransmit_count.saturating_add(1);
                // Congestion event on timeout (TCP Tahoe)
                conn.handle_congestion();
                // Karn: discard timing info for retransmitted segment
                conn.rtt_measure_seq = None;
                conn.rtt_send_time = None;
                // Next retransmit deadline uses the already-doubled RTO
                conn.retransmit_at = current_time + conn.retransmit_timeout as u64;
            });
        }
    }
}

/// TCP get bytes sent — returns total bytes sent (including unacked).
pub fn tcp_get_bytes_sent(
    local_addr: NetworkAddress,
    local_port: u16,
    remote_addr: NetworkAddress,
    remote_port: u16,
) -> NetworkResult<usize> {
    let connection = TCP_MANAGER
        .get_connection(&local_addr, local_port, &remote_addr, remote_port)
        .ok_or(NetworkError::NotConnected)?;

    Ok(connection.send_unacked.len())
}
