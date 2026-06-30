//! GSocket abstract interface matching `gio/gsocket.h` / `gio/gsocket.c`.
//!
//! Upstream `GSocket` is a `GObject` subclass wrapping a BSD socket file
//! descriptor. For bare-metal `no_std` we port it as a `Socket` trait plus
//! an in-memory `MockSocket` that exercises the full API without any OS
//! syscalls.
//!
//! Provides:
//! - `SocketType` and `SocketProtocol` enums.
//! - `Socket` trait.
//! - `MockSocket` struct for testing / loopback simulation.
//!
//! `SocketFamily` is re-exported from `crate::ginetaddress` to avoid
//! duplication.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
pub use crate::ginetaddress::SocketFamily;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use alloc::collections::VecDeque;
use spin::Mutex;

// ──────────────────────────── Enums ────────────────────────────────────────

/// The type of a socket (`GSocketType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SocketType {
    /// Not a valid socket type. (`G_SOCKET_TYPE_INVALID`)
    Invalid = 0,
    /// Reliable, ordered, connection-based byte streams. (`G_SOCKET_TYPE_STREAM`)
    Stream = 1,
    /// Connectionless, unreliable datagrams. (`G_SOCKET_TYPE_DATAGRAM`)
    Datagram = 2,
    /// Reliable, ordered, connection-based datagram packets. (`G_SOCKET_TYPE_SEQPACKET`)
    Seqpacket = 3,
}

/// The protocol for a socket (`GSocketProtocol`).
///
/// Negative values indicate "unknown", 0 means "pick the default for this
/// socket type", positive values are standard IANA protocol numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SocketProtocol {
    /// Unknown / unresolvable protocol. (`G_SOCKET_PROTOCOL_UNKNOWN`)
    Unknown = -1,
    /// Let the OS choose the default for the socket type. (`G_SOCKET_PROTOCOL_DEFAULT`)
    Default = 0,
    /// Transmission Control Protocol (6). (`G_SOCKET_PROTOCOL_TCP`)
    Tcp = 6,
    /// User Datagram Protocol (17). (`G_SOCKET_PROTOCOL_UDP`)
    Udp = 17,
    /// Stream Control Transmission Protocol (132). (`G_SOCKET_PROTOCOL_SCTP`)
    Sctp = 132,
}

// ──────────────────────────── Trait ────────────────────────────────────────

/// Abstract socket interface (`GSocket`).
///
/// Implementations wrap either real file descriptors (on hosted platforms)
/// or in-memory buffers (bare-metal / tests).
pub trait Socket {
    /// Returns the socket type.
    ///
    /// Mirrors `g_socket_get_socket_type`.
    fn socket_type(&self) -> SocketType;

    /// Returns the socket protocol.
    ///
    /// Mirrors `g_socket_get_protocol`.
    fn protocol(&self) -> SocketProtocol;

    /// Returns `true` if the socket is currently connected.
    ///
    /// Mirrors `g_socket_is_connected`.
    fn is_connected(&self) -> bool;

    /// Returns `true` if the socket is closed.
    ///
    /// Mirrors `g_socket_is_closed`.
    fn is_closed(&self) -> bool;

    /// Closes the socket.
    ///
    /// Returns `Err` if the socket is already closed or the operation is
    /// cancelled. Mirrors `g_socket_close`.
    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error>;

    /// Sends `buf` over the socket.
    ///
    /// Returns the number of bytes sent, or an error. Mirrors `g_socket_send`.
    fn send(&self, buf: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error>;

    /// Receives data from the socket into `buf`.
    ///
    /// Returns the number of bytes actually placed into `buf`, or an error.
    /// Mirrors `g_socket_receive`.
    fn receive(&self, buf: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error>;

    /// Gets the I/O timeout in seconds (0 = blocking).
    ///
    /// Mirrors `g_socket_get_timeout`.
    fn get_timeout(&self) -> u32;

    /// Sets the I/O timeout in seconds (0 = blocking).
    ///
    /// Mirrors `g_socket_set_timeout`.
    fn set_timeout(&self, timeout_secs: u32);
}

// ──────────────────────────── MockSocket ───────────────────────────────────

/// Internal mutable state for `MockSocket`.
struct MockSocketState {
    /// Bytes available for `receive()`.
    rx_buf: VecDeque<u8>,
    connected: bool,
    closed: bool,
    timeout_secs: u32,
}

/// An in-memory loopback socket for testing.
///
/// - `send()` discards bytes (they go nowhere, simulating a /dev/null peer).
/// - `receive()` drains bytes previously injected with `inject()`.
/// - `close()` marks the socket closed; further I/O returns an error.
pub struct MockSocket {
    socket_type: SocketType,
    protocol: SocketProtocol,
    state: Mutex<MockSocketState>,
}

impl MockSocket {
    /// Creates a new connected `Stream` / `Tcp` mock socket.
    pub fn new_stream() -> Self {
        Self {
            socket_type: SocketType::Stream,
            protocol: SocketProtocol::Tcp,
            state: Mutex::new(MockSocketState {
                rx_buf: VecDeque::new(),
                connected: true,
                closed: false,
                timeout_secs: 0,
            }),
        }
    }

    /// Injects `data` into the receive buffer.
    ///
    /// Subsequent `receive()` calls will drain these bytes. Intended for
    /// test setup: call this to simulate the peer sending data.
    pub fn inject(&self, data: &[u8]) {
        let mut st = self.state.lock();
        st.rx_buf.extend(data.iter().copied());
    }

    /// Returns how many bytes are currently sitting in the receive buffer.
    pub fn rx_available(&self) -> usize {
        self.state.lock().rx_buf.len()
    }
}

impl Socket for MockSocket {
    fn socket_type(&self) -> SocketType {
        self.socket_type
    }

    fn protocol(&self) -> SocketProtocol {
        self.protocol
    }

    fn is_connected(&self) -> bool {
        let st = self.state.lock();
        st.connected && !st.closed
    }

    fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let mut st = self.state.lock();
        if st.closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Socket is already closed",
            ));
        }
        st.closed = true;
        st.connected = false;
        Ok(())
    }

    fn send(&self, buf: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let st = self.state.lock();
        if st.closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Socket is closed",
            ));
        }
        if !st.connected {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::NotConnected.to_code(),
                "Socket is not connected",
            ));
        }
        // In the mock, all bytes are "sent" immediately (discarded).
        Ok(buf.len())
    }

    fn receive(&self, buf: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let mut st = self.state.lock();
        if st.closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Socket is closed",
            ));
        }
        if !st.connected {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::NotConnected.to_code(),
                "Socket is not connected",
            ));
        }
        if st.rx_buf.is_empty() {
            // No data available — return 0 (EOF / would-block in non-blocking mode).
            return Ok(0);
        }

        let n = buf.len().min(st.rx_buf.len());
        for byte in buf[..n].iter_mut() {
            *byte = st.rx_buf.pop_front().unwrap();
        }
        Ok(n)
    }

    fn get_timeout(&self) -> u32 {
        self.state.lock().timeout_secs
    }

    fn set_timeout(&self, timeout_secs: u32) {
        self.state.lock().timeout_secs = timeout_secs;
    }
}

// ──────────────────────────── Tests ────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_type_and_protocol() {
        let sock = MockSocket::new_stream();
        assert_eq!(sock.socket_type(), SocketType::Stream);
        assert_eq!(sock.protocol(), SocketProtocol::Tcp);
    }

    #[test]
    fn test_initial_state_connected_not_closed() {
        let sock = MockSocket::new_stream();
        assert!(sock.is_connected());
        assert!(!sock.is_closed());
    }

    #[test]
    fn test_close_transitions_state() {
        let sock = MockSocket::new_stream();
        sock.close(None).expect("close should succeed");
        assert!(sock.is_closed());
        assert!(!sock.is_connected());
    }

    #[test]
    fn test_double_close_returns_error() {
        let sock = MockSocket::new_stream();
        sock.close(None).unwrap();
        let err = sock.close(None).unwrap_err();
        assert_eq!(err.code(), IOErrorEnum::Closed.to_code());
    }

    #[test]
    fn test_send_returns_byte_count() {
        let sock = MockSocket::new_stream();
        let data = b"hello world";
        let n = sock.send(data, None).unwrap();
        assert_eq!(n, data.len());
    }

    #[test]
    fn test_inject_and_receive() {
        let sock = MockSocket::new_stream();
        sock.inject(b"ping");
        let mut buf = [0u8; 16];
        let n = sock.receive(&mut buf, None).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"ping");
    }

    #[test]
    fn test_receive_empty_returns_zero() {
        let sock = MockSocket::new_stream();
        let mut buf = [0u8; 8];
        let n = sock.receive(&mut buf, None).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_timeout_get_set() {
        let sock = MockSocket::new_stream();
        assert_eq!(sock.get_timeout(), 0);
        sock.set_timeout(30);
        assert_eq!(sock.get_timeout(), 30);
    }
}
