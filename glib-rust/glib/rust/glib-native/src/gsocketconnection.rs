//! GSocketConnection matching `gio/gsocketconnection.h`.
//!
//! Wraps a `GSocket` with local/remote address tracking and
//! connect/close lifecycle. In this no_std port we model the state
//! without actual OS sockets.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetsocketaddress::InetSocketAddress;
use crate::gsocket::{MockSocket, Socket as _};
use alloc::string::{String, ToString};
use spin::Mutex;

/// A socket connection (`GSocketConnection`).
pub struct SocketConnection {
    socket: MockSocket,
    local_address: Mutex<Option<InetSocketAddress>>,
    remote_address: Mutex<Option<InetSocketAddress>>,
    connected: Mutex<bool>,
    closed: Mutex<bool>,
}

impl SocketConnection {
    /// Creates a new socket connection from a socket.
    ///
    /// Mirrors `g_socket_connection_connect` (post-connect state).
    pub fn new(socket: MockSocket) -> Self {
        Self {
            socket,
            local_address: Mutex::new(None),
            remote_address: Mutex::new(None),
            connected: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    /// Creates a connected socket connection with addresses.
    pub fn new_connected(
        socket: MockSocket,
        local: InetSocketAddress,
        remote: InetSocketAddress,
    ) -> Self {
        Self {
            socket,
            local_address: Mutex::new(Some(local)),
            remote_address: Mutex::new(Some(remote)),
            connected: Mutex::new(true),
            closed: Mutex::new(false),
        }
    }

    /// Returns whether the connection is connected.
    ///
    /// Mirrors `g_socket_connection_is_connected`.
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }

    /// Connects to the given remote address.
    ///
    /// Mirrors `g_socket_connection_connect` (simplified: no DNS).
    pub fn connect(&self, address: InetSocketAddress) -> Result<(), String> {
        if *self.closed.lock() {
            return Err("connection is closed".to_string());
        }
        *self.remote_address.lock() = Some(address);
        *self.connected.lock() = true;
        Ok(())
    }

    /// Gets the underlying socket.
    ///
    /// Mirrors `g_socket_connection_get_socket`.
    pub fn get_socket(&self) -> &MockSocket {
        &self.socket
    }

    /// Gets the local address.
    ///
    /// Mirrors `g_socket_connection_get_local_address`.
    pub fn get_local_address(&self) -> Option<InetSocketAddress> {
        self.local_address.lock().clone()
    }

    /// Sets the local address.
    pub fn set_local_address(&self, addr: InetSocketAddress) {
        *self.local_address.lock() = Some(addr);
    }

    /// Gets the remote address.
    ///
    /// Mirrors `g_socket_connection_get_remote_address`.
    pub fn get_remote_address(&self) -> Option<InetSocketAddress> {
        self.remote_address.lock().clone()
    }

    /// Closes the connection.
    pub fn close(&self) {
        let _ = self.socket.close(None);
        *self.closed.lock() = true;
        *self.connected.lock() = false;
    }

    /// Returns whether the connection is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};
    use crate::gsocket::{MockSocket, Socket as _, SocketType};

    fn make_addr(port: u16) -> InetSocketAddress {
        let addr = InetAddress::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4).unwrap();
        InetSocketAddress::new(addr, port)
    }

    fn make_socket() -> MockSocket {
        MockSocket::new_stream()
    }

    #[test]
    fn test_new() {
        let conn = SocketConnection::new(make_socket());
        assert!(!conn.is_connected());
        assert!(!conn.is_closed());
        assert!(conn.get_local_address().is_none());
        assert!(conn.get_remote_address().is_none());
    }

    #[test]
    fn test_new_connected() {
        let conn = SocketConnection::new_connected(make_socket(), make_addr(8080), make_addr(443));
        assert!(conn.is_connected());
        assert_eq!(conn.get_local_address().unwrap().port(), 8080);
        assert_eq!(conn.get_remote_address().unwrap().port(), 443);
    }

    #[test]
    fn test_connect() {
        let conn = SocketConnection::new(make_socket());
        conn.connect(make_addr(80)).unwrap();
        assert!(conn.is_connected());
        assert_eq!(conn.get_remote_address().unwrap().port(), 80);
    }

    #[test]
    fn test_close() {
        let conn = SocketConnection::new_connected(make_socket(), make_addr(1234), make_addr(5678));
        conn.close();
        assert!(conn.is_closed());
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connect_after_close() {
        let conn = SocketConnection::new(make_socket());
        conn.close();
        let result = conn.connect(make_addr(80));
        assert!(result.is_err());
    }

    #[test]
    fn test_set_local_address() {
        let conn = SocketConnection::new(make_socket());
        conn.set_local_address(make_addr(3000));
        assert_eq!(conn.get_local_address().unwrap().port(), 3000);
    }

    #[test]
    fn test_get_socket() {
        let socket = make_socket();
        let conn = SocketConnection::new(socket);
        let s = conn.get_socket();
        assert_eq!(s.socket_type(), SocketType::Stream);
    }
}
