//! GSocketClient matching `gio/gsocketclient.h`.
//!
//! `GSocketClient` is a high-level helper for creating client-side
//! `GSocketConnection`s. In our `no_std` port it resolves a
//! `GSocketConnectable` (via its `SocketAddressEnumerator`) and records
//! which address was chosen; actual I/O is delegated to `GSocket`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginetsocketaddress::InetSocketAddress;
use crate::gsocket::{SocketProtocol, SocketType};
use crate::gsocketconnectable::{SimpleConnectable, SocketConnectable};
use alloc::vec::Vec;

/// Timeout for connection attempts (in milliseconds). 0 means no timeout.
pub type Timeout = u32;

/// TLS mode for `GSocketClient`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsInteraction {
    Default,
    None,
    Required,
}

/// A client-side connection factory (`GSocketClient`).
pub struct SocketClient {
    timeout: Timeout,
    tls: TlsInteraction,
    socket_type: SocketType,
    protocol: SocketProtocol,
    local_address: Option<InetSocketAddress>,
}

impl SocketClient {
    /// Creates a new socket client with defaults.
    ///
    /// Mirrors `g_socket_client_new`.
    pub fn new() -> Self {
        Self {
            timeout: 0,
            tls: TlsInteraction::Default,
            socket_type: SocketType::Stream,
            protocol: SocketProtocol::Default,
            local_address: None,
        }
    }

    /// Sets the connection timeout in seconds. 0 = no timeout.
    ///
    /// Mirrors `g_socket_client_set_timeout`.
    pub fn set_timeout(&mut self, timeout: Timeout) {
        self.timeout = timeout;
    }

    /// Gets the connection timeout.
    pub fn get_timeout(&self) -> Timeout {
        self.timeout
    }

    /// Sets whether to use TLS.
    ///
    /// Mirrors `g_socket_client_set_tls`.
    pub fn set_tls(&mut self, tls: TlsInteraction) {
        self.tls = tls;
    }

    /// Gets the TLS setting.
    pub fn get_tls(&self) -> TlsInteraction {
        self.tls
    }

    /// Overrides the socket type (Stream/Datagram/Seqpacket).
    ///
    /// Mirrors `g_socket_client_set_socket_type`.
    pub fn set_socket_type(&mut self, t: SocketType) {
        self.socket_type = t;
    }

    /// Gets the socket type.
    pub fn get_socket_type(&self) -> SocketType {
        self.socket_type
    }

    /// Sets a local address to bind to before connecting.
    ///
    /// Mirrors `g_socket_client_set_local_address`.
    pub fn set_local_address(&mut self, addr: Option<InetSocketAddress>) {
        self.local_address = addr;
    }

    /// Gets the local address binding.
    pub fn get_local_address(&self) -> Option<&InetSocketAddress> {
        self.local_address.as_ref()
    }

    /// Connects to a `SocketConnectable`, returning the address used.
    ///
    /// Mirrors `g_socket_client_connect`.
    ///
    /// In this `no_std` port we enumerate addresses from the connectable and
    /// return the first one (real I/O is left to the platform layer).
    pub fn connect(
        &self,
        connectable: &dyn SocketConnectable,
        cancellable: Option<&GCancellable>,
    ) -> Result<InetSocketAddress, Error> {
        let enumerator = connectable.enumerate();
        let addr = enumerator.next(cancellable)?.ok_or_else(|| {
            use crate::gioerror::{io_error_quark, IOErrorEnum};
            Error::new(
                io_error_quark(),
                IOErrorEnum::NotFound.to_code(),
                "no addresses to connect to",
            )
        })?;
        Ok(addr)
    }

    /// Connects to a host:port string.
    ///
    /// Mirrors `g_socket_client_connect_to_host`.
    pub fn connect_to_host(
        &self,
        host_and_port: &str,
        default_port: u16,
        cancellable: Option<&GCancellable>,
    ) -> Result<InetSocketAddress, Error> {
        use crate::gnetworkaddress::NetworkAddress;
        let na = NetworkAddress::parse(host_and_port, default_port).map_err(|_| {
            use crate::gioerror::{io_error_quark, IOErrorEnum};
            Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "invalid host:port",
            )
        })?;
        let connectable = SimpleConnectable::new(na.hostname(), na.port(), Vec::new());
        self.connect(&connectable, cancellable)
    }
}

impl Default for SocketClient {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};

    fn make_addr(octets: [u8; 4], port: u16) -> InetSocketAddress {
        let addr = InetAddress::new_from_bytes(&octets, SocketFamily::Ipv4).unwrap();
        InetSocketAddress::new(addr, port)
    }

    #[test]
    fn test_new_defaults() {
        let c = SocketClient::new();
        assert_eq!(c.get_timeout(), 0);
        assert_eq!(c.get_tls(), TlsInteraction::Default);
        assert_eq!(c.get_socket_type(), SocketType::Stream);
        assert!(c.get_local_address().is_none());
    }

    #[test]
    fn test_set_timeout() {
        let mut c = SocketClient::new();
        c.set_timeout(30);
        assert_eq!(c.get_timeout(), 30);
    }

    #[test]
    fn test_set_tls() {
        let mut c = SocketClient::new();
        c.set_tls(TlsInteraction::Required);
        assert_eq!(c.get_tls(), TlsInteraction::Required);
    }

    #[test]
    fn test_set_socket_type() {
        let mut c = SocketClient::new();
        c.set_socket_type(SocketType::Datagram);
        assert_eq!(c.get_socket_type(), SocketType::Datagram);
    }

    #[test]
    fn test_set_local_address() {
        let mut c = SocketClient::new();
        let addr = make_addr([127, 0, 0, 1], 0);
        c.set_local_address(Some(addr.clone()));
        assert!(c.get_local_address().is_some());
        c.set_local_address(None);
        assert!(c.get_local_address().is_none());
    }

    #[test]
    fn test_connect_success() {
        let c = SocketClient::new();
        let addr = make_addr([127, 0, 0, 1], 80);
        let connectable = SimpleConnectable::new("localhost", 80, vec![addr]);
        let result = c.connect(&connectable, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().port(), 80);
    }

    #[test]
    fn test_connect_no_addresses() {
        let c = SocketClient::new();
        let connectable = SimpleConnectable::new("localhost", 80, vec![]);
        assert!(c.connect(&connectable, None).is_err());
    }

    #[test]
    fn test_default() {
        let c = SocketClient::default();
        assert_eq!(c.get_timeout(), 0);
    }
}
