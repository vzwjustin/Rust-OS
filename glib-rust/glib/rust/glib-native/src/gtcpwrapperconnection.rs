//! GTcpWrapperConnection matching `gio/gtcpwrapperconnection.h`.
//!
//! A `GTcpConnection` subclass that wraps another connection inside a TCP
//! connection's socket. Used to layer protocols (e.g. TLS) over a raw TCP
//! connection while retaining the `GTcpConnection` interface.
//!
//! No_std compatible.

use crate::ginetsocketaddress::InetSocketAddress;
use crate::gtcpconnection::TcpConnection;
use spin::Mutex;

/// A TCP connection that wraps an inner connection (`GTcpWrapperConnection`).
pub struct TcpWrapperConnection {
    inner: TcpConnection,
    /// Protocol name of the wrapped layer (e.g. "tls", "proxy").
    wrapped_protocol: Option<alloc::string::String>,
    closed: Mutex<bool>,
}

impl TcpWrapperConnection {
    /// Creates a new wrapper around a `TcpConnection`.
    ///
    /// Mirrors `g_tcp_wrapper_connection_new`.
    pub fn new(inner: TcpConnection) -> Self {
        Self {
            inner,
            wrapped_protocol: None,
            closed: Mutex::new(false),
        }
    }

    /// Creates a wrapper with a named protocol.
    pub fn new_with_protocol(inner: TcpConnection, protocol: &str) -> Self {
        Self {
            inner,
            wrapped_protocol: Some(protocol.into()),
            closed: Mutex::new(false),
        }
    }

    /// Returns a reference to the wrapped `TcpConnection`.
    ///
    /// Mirrors `g_tcp_wrapper_connection_get_base_io_stream`.
    pub fn get_base_connection(&self) -> &TcpConnection {
        &self.inner
    }

    /// Returns the wrapped protocol name.
    pub fn get_wrapped_protocol(&self) -> Option<&str> {
        self.wrapped_protocol.as_deref()
    }

    /// Returns the remote address of the underlying TCP connection.
    pub fn get_remote_address(&self) -> &InetSocketAddress {
        self.inner.get_remote_address()
    }

    /// Closes the wrapper (and the underlying connection).
    pub fn close(&self) {
        self.inner.close();
        *self.closed.lock() = true;
    }

    /// Returns true if the wrapper has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};
    use crate::ginetsocketaddress::InetSocketAddress;

    fn make_tcp(port: u16) -> TcpConnection {
        let addr = InetAddress::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4).unwrap();
        TcpConnection::new(InetSocketAddress::new(addr, port))
    }

    #[test]
    fn test_new() {
        let w = TcpWrapperConnection::new(make_tcp(443));
        assert!(!w.is_closed());
        assert_eq!(w.get_remote_address().port(), 443);
        assert!(w.get_wrapped_protocol().is_none());
    }

    #[test]
    fn test_with_protocol() {
        let w = TcpWrapperConnection::new_with_protocol(make_tcp(443), "tls");
        assert_eq!(w.get_wrapped_protocol(), Some("tls"));
    }

    #[test]
    fn test_close() {
        let w = TcpWrapperConnection::new(make_tcp(80));
        w.close();
        assert!(w.is_closed());
        assert!(w.get_base_connection().is_closed());
    }

    #[test]
    fn test_base_connection() {
        let w = TcpWrapperConnection::new(make_tcp(8080));
        assert_eq!(w.get_base_connection().get_remote_address().port(), 8080);
    }
}
