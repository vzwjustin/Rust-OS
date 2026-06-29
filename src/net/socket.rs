//! Socket interface implementation
//!
//! This module provides the socket abstraction layer for network communication,
//! supporting TCP, UDP, and raw sockets with a POSIX-like interface.

use super::{NetworkAddress, NetworkError, NetworkResult, Protocol};
use alloc::collections::VecDeque;
use core::fmt;

/// Socket types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    /// Stream socket (TCP)
    Stream,
    /// Datagram socket (UDP)
    Datagram,
    /// Raw socket
    Raw,
}

/// Socket states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Socket is closed
    Closed,
    /// Socket is listening for connections
    Listening,
    /// Socket is connecting
    Connecting,
    /// Socket is connected
    Connected,
    /// Socket is closing
    Closing,
}

/// Socket address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddress {
    /// IP address
    pub address: NetworkAddress,
    /// Port number
    pub port: u16,
}

impl SocketAddress {
    /// Create a new socket address
    pub fn new(address: NetworkAddress, port: u16) -> Self {
        Self { address, port }
    }

    /// Create IPv4 socket address
    pub fn ipv4(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        Self {
            address: NetworkAddress::ipv4(a, b, c, d),
            port,
        }
    }

    /// Create IPv6 socket address
    pub fn ipv6(addr: [u8; 16], port: u16) -> Self {
        Self {
            address: NetworkAddress::IPv6(addr),
            port,
        }
    }

    /// Validate that address is either IPv4 or IPv6
    pub fn is_valid(&self) -> bool {
        matches!(
            self.address,
            NetworkAddress::IPv4(_) | NetworkAddress::IPv6(_)
        )
    }
}

impl fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.address, self.port)
    }
}

/// Socket options
#[derive(Debug, Clone)]
pub struct SocketOptions {
    /// Reuse address
    pub reuse_addr: bool,
    /// Reuse port
    pub reuse_port: bool,
    /// Keep alive
    pub keep_alive: bool,
    /// No delay (disable Nagle's algorithm)
    pub no_delay: bool,
    /// Receive buffer size
    pub recv_buffer_size: usize,
    /// Send buffer size
    pub send_buffer_size: usize,
    /// Receive timeout (milliseconds)
    pub recv_timeout: Option<u32>,
    /// Send timeout (milliseconds)
    pub send_timeout: Option<u32>,
}

impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            reuse_addr: false,
            reuse_port: false,
            keep_alive: false,
            no_delay: false,
            recv_buffer_size: 8192,
            send_buffer_size: 8192,
            recv_timeout: None,
            send_timeout: None,
        }
    }
}

/// Socket statistics
#[derive(Debug, Clone, Default)]
pub struct SocketStats {
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Packets sent
    pub packets_sent: u64,
    /// Packets received
    pub packets_received: u64,
    /// Send errors
    pub send_errors: u64,
    /// Receive errors
    pub recv_errors: u64,
}

/// Socket implementation
#[derive(Debug, Clone)]
pub struct Socket {
    /// Socket ID
    pub id: u32,
    /// Socket type
    pub socket_type: SocketType,
    /// Protocol
    pub protocol: Protocol,
    /// Current state
    pub state: SocketState,
    /// Local address
    pub local_address: Option<SocketAddress>,
    /// Remote address
    pub remote_address: Option<SocketAddress>,
    /// Socket options
    pub options: SocketOptions,
    /// Receive buffer
    pub recv_buffer: VecDeque<u8>,
    /// Send buffer
    pub send_buffer: VecDeque<u8>,
    /// Pending connections (for listening sockets)
    pub pending_connections: VecDeque<u32>,
    /// Socket statistics
    pub stats: SocketStats,
    /// Receive side has been shut down
    pub recv_shutdown: bool,
    /// Send side has been shut down
    pub send_shutdown: bool,
}

impl Socket {
    /// Create a new socket
    pub fn new(id: u32, socket_type: SocketType, protocol: Protocol) -> Self {
        Self {
            id,
            socket_type,
            protocol,
            state: SocketState::Closed,
            local_address: None,
            remote_address: None,
            options: SocketOptions::default(),
            recv_buffer: VecDeque::new(),
            send_buffer: VecDeque::new(),
            pending_connections: VecDeque::new(),
            stats: SocketStats::default(),
            recv_shutdown: false,
            send_shutdown: false,
        }
    }

    /// Bind socket to local address
    pub fn bind(&mut self, address: SocketAddress) -> NetworkResult<()> {
        if self.state != SocketState::Closed {
            return Err(NetworkError::InvalidAddress);
        }

        // Privileged port check: ports < 1024 require CAP_NET_BIND_SERVICE (euid == 0)
        if address.port < 1024 {
            let pid = crate::process::current_pid();
            let pm = crate::process::get_process_manager();
            let is_root = pm
                .get_process(pid)
                .map(|pcb| pcb.euid == 0)
                .unwrap_or(false);
            if !is_root {
                return Err(NetworkError::PermissionDenied);
            }
        }

        // For UDP sockets, actually bind in the UDP stack
        if self.socket_type == SocketType::Datagram {
            crate::net::udp::udp_bind(address.address, address.port)?;
        }

        self.local_address = Some(address);
        Ok(())
    }

    /// Listen for incoming connections (TCP only)
    pub fn listen(&mut self, _backlog: u32) -> NetworkResult<()> {
        if self.socket_type != SocketType::Stream {
            return Err(NetworkError::NotSupported);
        }

        if let Some(local_addr) = self.local_address {
            // Call TCP listen function
            crate::net::tcp::tcp_listen(local_addr.address, local_addr.port)?;

            self.state = SocketState::Listening;
            self.pending_connections.clear();

            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Connect to remote address
    pub fn connect(&mut self, address: SocketAddress) -> NetworkResult<()> {
        if self.state != SocketState::Closed {
            return Err(NetworkError::InvalidAddress);
        }

        self.remote_address = Some(address);
        self.state = SocketState::Connecting;

        match self.socket_type {
            SocketType::Stream => {
                // TCP connection - use real TCP stack
                if let Some(local_addr) = self.local_address {
                    // Call TCP connect function
                    let _local_port = crate::net::tcp::tcp_connect(
                        local_addr.address,
                        address.address,
                        address.port,
                    )?;
                    self.state = SocketState::Connected;
                } else {
                    return Err(NetworkError::InvalidAddress);
                }
            }
            SocketType::Datagram => {
                // UDP "connection" (just sets default destination)
                if let Some(local_addr) = self.local_address {
                    crate::net::udp::udp_connect(
                        local_addr.address,
                        local_addr.port,
                        address.address,
                        address.port,
                    )?;
                }
                self.state = SocketState::Connected;
            }
            SocketType::Raw => {
                return Err(NetworkError::NotSupported);
            }
        }

        Ok(())
    }

    /// Accept incoming connection (TCP only)
    pub fn accept(&mut self) -> NetworkResult<Option<u32>> {
        if self.socket_type != SocketType::Stream || self.state != SocketState::Listening {
            return Err(NetworkError::NotSupported);
        }

        if let Some(connection_id) = self.pending_connections.pop_front() {
            Ok(Some(connection_id))
        } else {
            Ok(None)
        }
    }

    /// Send data through socket
    pub fn send(&mut self, data: &[u8]) -> NetworkResult<usize> {
        if self.state != SocketState::Connected {
            return Err(NetworkError::ConnectionRefused);
        }
        if self.send_shutdown {
            return Err(NetworkError::ConnectionRefused);
        }

        // Check send buffer space
        if self.send_buffer.len() + data.len() > self.options.send_buffer_size {
            return Err(NetworkError::BufferOverflow);
        }

        let bytes_sent = match self.socket_type {
            SocketType::Stream => {
                // TCP send — buffer the data and transmit via TCP layer
                self.send_buffer.extend(data.iter());
                // Transmit the data over the TCP connection
                if let (Some(local_addr), Some(remote_addr)) =
                    (self.local_address, self.remote_address)
                {
                    match crate::net::tcp::tcp_send_data(
                        local_addr.address,
                        local_addr.port,
                        remote_addr.address,
                        remote_addr.port,
                        data,
                    ) {
                        Ok(sent) => sent,
                        Err(_) => {
                            // If transmission fails, still return buffered length
                            // so the caller can retry. The data remains in send_buffer.
                            data.len()
                        }
                    }
                } else {
                    data.len()
                }
            }
            SocketType::Datagram => {
                // UDP send - use real UDP stack
                if let (Some(local_addr), Some(_remote_addr)) =
                    (self.local_address, self.remote_address)
                {
                    crate::net::udp::udp_send(local_addr.address, local_addr.port, data)?
                } else {
                    return Err(NetworkError::InvalidAddress);
                }
            }
            SocketType::Raw => {
                return Err(NetworkError::NotSupported);
            }
        };

        self.stats.bytes_sent += bytes_sent as u64;
        self.stats.packets_sent += 1;

        Ok(bytes_sent)
    }

    /// Receive data from socket
    pub fn recv(&mut self, buffer: &mut [u8]) -> NetworkResult<usize> {
        if self.state != SocketState::Connected && self.state != SocketState::Listening {
            return Err(NetworkError::ConnectionRefused);
        }
        if self.recv_shutdown {
            return Ok(0); // EOF: read side shut down
        }

        let bytes_to_read = core::cmp::min(buffer.len(), self.recv_buffer.len());

        if bytes_to_read == 0 {
            return Ok(0); // No data available
        }

        // Copy data from receive buffer
        for i in 0..bytes_to_read {
            buffer[i] = self.recv_buffer.pop_front().unwrap();
        }

        self.stats.bytes_received += bytes_to_read as u64;
        self.stats.packets_received += 1;

        // Production: data received successfully
        Ok(bytes_to_read)
    }

    /// Peek at data in the receive buffer without consuming it (MSG_PEEK).
    pub fn recv_peek(&self, buffer: &mut [u8]) -> NetworkResult<usize> {
        if self.state != SocketState::Connected && self.state != SocketState::Listening {
            return Err(NetworkError::ConnectionRefused);
        }
        if self.recv_shutdown {
            return Ok(0);
        }

        let bytes_to_read = core::cmp::min(buffer.len(), self.recv_buffer.len());
        if bytes_to_read == 0 {
            return Ok(0);
        }

        // Copy without removing from the buffer
        for (i, byte) in self.recv_buffer.iter().take(bytes_to_read).enumerate() {
            buffer[i] = *byte;
        }

        Ok(bytes_to_read)
    }

    /// Send data to specific address (UDP only)
    pub fn send_to(&mut self, data: &[u8], address: SocketAddress) -> NetworkResult<usize> {
        if self.socket_type != SocketType::Datagram {
            return Err(NetworkError::NotSupported);
        }

        // Real UDP packet transmission
        let bytes_sent = if let Some(local_addr) = self.local_address {
            crate::net::udp::udp_send_to(
                local_addr.address,
                local_addr.port,
                address.address,
                address.port,
                data,
            )?
        } else {
            return Err(NetworkError::InvalidAddress);
        };

        self.stats.bytes_sent += bytes_sent as u64;
        self.stats.packets_sent += 1;

        Ok(bytes_sent)
    }

    /// Receive data from any address (UDP only)
    pub fn recv_from(&mut self, buffer: &mut [u8]) -> NetworkResult<(usize, SocketAddress)> {
        if self.socket_type != SocketType::Datagram {
            return Err(NetworkError::NotSupported);
        }

        // Real UDP packet reception
        if let Some(local_addr) = self.local_address {
            if let Some((data, src_addr, src_port)) =
                crate::net::udp::udp_recv(local_addr.address, local_addr.port)?
            {
                let bytes_to_copy = core::cmp::min(buffer.len(), data.len());
                buffer[..bytes_to_copy].copy_from_slice(&data[..bytes_to_copy]);

                let source = SocketAddress::new(src_addr, src_port);

                self.stats.bytes_received += bytes_to_copy as u64;
                self.stats.packets_received += 1;

                Ok((bytes_to_copy, source))
            } else {
                // No data available
                Err(NetworkError::Timeout)
            }
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Shut down part or all of the socket.
    pub fn shutdown(&mut self, how: i32) -> NetworkResult<()> {
        const SHUT_RD: i32 = 0;
        const SHUT_WR: i32 = 1;
        const SHUT_RDWR: i32 = 2;

        match how {
            SHUT_RD => self.recv_shutdown = true,
            SHUT_WR => self.send_shutdown = true,
            SHUT_RDWR => {
                self.recv_shutdown = true;
                self.send_shutdown = true;
                self.close()?;
            }
            _ => return Err(NetworkError::InvalidArgument),
        }
        Ok(())
    }

    /// Close the socket
    pub fn close(&mut self) -> NetworkResult<()> {
        match self.state {
            SocketState::Closed => return Ok(()),
            SocketState::Connected => {
                self.state = SocketState::Closing;

                // Implement proper TCP connection teardown
                if self.socket_type == SocketType::Stream {
                    if let (Some(local_addr), Some(remote_addr)) =
                        (self.local_address, self.remote_address)
                    {
                        // Initiate TCP close sequence (FIN handshake)
                        crate::net::tcp::tcp_close(
                            local_addr.address,
                            local_addr.port,
                            remote_addr.address,
                            remote_addr.port,
                        )
                        .ok(); // Ignore errors during close
                    }
                }

                self.state = SocketState::Closed;
            }
            _ => {
                self.state = SocketState::Closed;
            }
        }

        self.recv_buffer.clear();
        self.send_buffer.clear();
        self.pending_connections.clear();

        // Production: socket closed successfully
        Ok(())
    }

    /// Set socket option
    pub fn set_option(&mut self, option: SocketOption) -> NetworkResult<()> {
        match option {
            SocketOption::ReuseAddr(value) => self.options.reuse_addr = value,
            SocketOption::ReusePort(value) => self.options.reuse_port = value,
            SocketOption::KeepAlive(value) => self.options.keep_alive = value,
            SocketOption::NoDelay(value) => self.options.no_delay = value,
            SocketOption::RecvBufferSize(size) => {
                self.options.recv_buffer_size = size;
                // Resize buffer if needed
                if self.recv_buffer.len() > size {
                    self.recv_buffer.truncate(size);
                }
            }
            SocketOption::SendBufferSize(size) => {
                self.options.send_buffer_size = size;
                // Resize buffer if needed
                if self.send_buffer.len() > size {
                    self.send_buffer.truncate(size);
                }
            }
            SocketOption::RecvTimeout(timeout) => self.options.recv_timeout = timeout,
            SocketOption::SendTimeout(timeout) => self.options.send_timeout = timeout,
        }

        Ok(())
    }

    /// Get socket option
    pub fn get_option(&self, option_type: SocketOptionType) -> NetworkResult<SocketOption> {
        let option = match option_type {
            SocketOptionType::ReuseAddr => SocketOption::ReuseAddr(self.options.reuse_addr),
            SocketOptionType::ReusePort => SocketOption::ReusePort(self.options.reuse_port),
            SocketOptionType::KeepAlive => SocketOption::KeepAlive(self.options.keep_alive),
            SocketOptionType::NoDelay => SocketOption::NoDelay(self.options.no_delay),
            SocketOptionType::RecvBufferSize => {
                SocketOption::RecvBufferSize(self.options.recv_buffer_size)
            }
            SocketOptionType::SendBufferSize => {
                SocketOption::SendBufferSize(self.options.send_buffer_size)
            }
            SocketOptionType::RecvTimeout => SocketOption::RecvTimeout(self.options.recv_timeout),
            SocketOptionType::SendTimeout => SocketOption::SendTimeout(self.options.send_timeout),
        };

        Ok(option)
    }

    /// Check if socket has data available for reading
    pub fn has_data(&self) -> bool {
        !self.recv_buffer.is_empty()
    }

    /// Check if socket can accept more data for sending
    pub fn can_send(&self) -> bool {
        self.send_buffer.len() < self.options.send_buffer_size
    }

    /// Get number of bytes available for reading
    pub fn available_bytes(&self) -> usize {
        self.recv_buffer.len()
    }

    /// Add data to receive buffer (used by network stack)
    pub fn add_received_data(&mut self, data: &[u8]) -> NetworkResult<()> {
        if self.recv_buffer.len() + data.len() > self.options.recv_buffer_size {
            return Err(NetworkError::BufferOverflow);
        }

        self.recv_buffer.extend(data.iter());
        Ok(())
    }
}

/// Socket option types
#[derive(Debug, Clone, Copy)]
pub enum SocketOptionType {
    ReuseAddr,
    ReusePort,
    KeepAlive,
    NoDelay,
    RecvBufferSize,
    SendBufferSize,
    RecvTimeout,
    SendTimeout,
}

/// Socket option values
#[derive(Debug, Clone)]
pub enum SocketOption {
    ReuseAddr(bool),
    ReusePort(bool),
    KeepAlive(bool),
    NoDelay(bool),
    RecvBufferSize(usize),
    SendBufferSize(usize),
    RecvTimeout(Option<u32>),
    SendTimeout(Option<u32>),
}

/// Socket domain (address family)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDomain {
    /// Internet Protocol version 4
    IPv4,
    /// Internet Protocol version 6
    IPv6,
    /// Unix domain sockets
    Unix,
}

/// High-level socket operations
pub struct SocketManager;

impl SocketManager {
    /// Create a TCP socket
    pub fn tcp_socket() -> NetworkResult<u32> {
        super::network_stack().create_socket(SocketType::Stream, Protocol::TCP)
    }

    /// Create a UDP socket
    pub fn udp_socket() -> NetworkResult<u32> {
        super::network_stack().create_socket(SocketType::Datagram, Protocol::UDP)
    }

    /// Create a raw socket
    pub fn raw_socket(protocol: Protocol) -> NetworkResult<u32> {
        super::network_stack().create_socket(SocketType::Raw, protocol)
    }

    /// Close a socket
    pub fn close_socket(socket_id: u32) -> NetworkResult<()> {
        super::network_stack().close_socket(socket_id)
    }
}
