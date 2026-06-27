//! GTcpConnection matching `gio/gtcpconnection.h`.
//!
//! A `GSocketConnection` subclass for TCP connections. Wraps a remote
//! `InetSocketAddress` and tracks graceful-close state.
//!
//! No_std compatible using `alloc`.

use crate::ginetsocketaddress::InetSocketAddress;
use spin::Mutex;

/// A TCP socket connection (`GTcpConnection`).
pub struct TcpConnection {
    remote_address: InetSocketAddress,
    graceful_disconnect: bool,
    closed: Mutex<bool>,
}

impl TcpConnection {
    /// Creates a new `TcpConnection` to the given remote address.
    ///
    /// Mirrors `g_socket_connection_factory_create_connection` for TCP.
    pub fn new(remote_address: InetSocketAddress) -> Self {
        Self {
            remote_address,
            graceful_disconnect: false,
            closed: Mutex::new(false),
        }
    }

    /// Returns the remote address this connection is to.
    ///
    /// Mirrors `g_socket_connection_get_remote_address`.
    pub fn get_remote_address(&self) -> &InetSocketAddress {
        &self.remote_address
    }

    /// Sets whether to use graceful disconnect (TCP FIN/ACK exchange).
    ///
    /// Mirrors `g_tcp_connection_set_graceful_disconnect`.
    pub fn set_graceful_disconnect(&mut self, graceful: bool) {
        self.graceful_disconnect = graceful;
    }

    /// Gets the graceful disconnect setting.
    ///
    /// Mirrors `g_tcp_connection_get_graceful_disconnect`.
    pub fn get_graceful_disconnect(&self) -> bool {
        self.graceful_disconnect
    }

    /// Closes the connection.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns true if the connection has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};

    fn make_addr(port: u16) -> InetSocketAddress {
        let addr = InetAddress::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4).unwrap();
        InetSocketAddress::new(addr, port)
    }

    #[test]
    fn test_new() {
        let c = TcpConnection::new(make_addr(80));
        assert_eq!(c.get_remote_address().port(), 80);
        assert!(!c.is_closed());
        assert!(!c.get_graceful_disconnect());
    }

    #[test]
    fn test_graceful_disconnect() {
        let mut c = TcpConnection::new(make_addr(443));
        assert!(!c.get_graceful_disconnect());
        c.set_graceful_disconnect(true);
        assert!(c.get_graceful_disconnect());
    }

    #[test]
    fn test_close() {
        let c = TcpConnection::new(make_addr(8080));
        assert!(!c.is_closed());
        c.close();
        assert!(c.is_closed());
    }

    #[test]
    fn test_remote_address_preserved() {
        let c = TcpConnection::new(make_addr(22));
        assert_eq!(c.get_remote_address().port(), 22);
    }
}
