//! GSocketListener matching `gio/gsocketlistener.h`.
//!
//! `GSocketListener` is a server-side helper that manages a list of
//! listening sockets and accepts incoming connections. In this `no_std`
//! port we model it as an in-memory queue of "pending" addresses (simulating
//! accepted connections) so the trait surface is fully testable.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginetsocketaddress::InetSocketAddress;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

/// The maximum number of pending connections held in the backlog.
const DEFAULT_BACKLOG: i32 = 10;

/// A server-side connection listener (`GSocketListener`).
pub struct SocketListener {
    backlog: i32,
    /// Listening endpoints (address:port pairs we are bound to).
    listening: Mutex<Vec<InetSocketAddress>>,
    /// Simulated incoming connections queued for `accept`.
    pending: Mutex<VecDeque<InetSocketAddress>>,
    closed: Mutex<bool>,
}

impl SocketListener {
    /// Creates a new socket listener.
    ///
    /// Mirrors `g_socket_listener_new`.
    pub fn new() -> Self {
        Self {
            backlog: DEFAULT_BACKLOG,
            listening: Mutex::new(Vec::new()),
            pending: Mutex::new(VecDeque::new()),
            closed: Mutex::new(false),
        }
    }

    /// Sets the listen backlog.
    ///
    /// Mirrors `g_socket_listener_set_backlog`.
    pub fn set_backlog(&mut self, backlog: i32) {
        self.backlog = backlog;
    }

    /// Gets the listen backlog.
    pub fn get_backlog(&self) -> i32 {
        self.backlog
    }

    /// Adds a port to listen on for all interfaces.
    ///
    /// Mirrors `g_socket_listener_add_inet_port`.
    ///
    /// In this port we simply record the address; no OS socket is created.
    pub fn add_inet_port(
        &self,
        port: u16,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        use crate::ginetaddress::{InetAddress, SocketFamily};
        if *self.closed.lock() {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "listener is closed",
            ));
        }
        let any = InetAddress::new_any(SocketFamily::Ipv4).unwrap();
        self.listening
            .lock()
            .push(InetSocketAddress::new(any, port));
        Ok(())
    }

    /// Accepts the next pending incoming connection (non-blocking).
    ///
    /// Mirrors `g_socket_listener_accept` (synchronous variant).
    ///
    /// Returns the peer address, or `WouldBlock` if the queue is empty.
    pub fn accept(&self, cancellable: Option<&GCancellable>) -> Result<InetSocketAddress, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "listener is closed",
            ));
        }
        if let Some(ref c) = cancellable {
            if c.is_cancelled() {
                return Err(Error::new(
                    io_error_quark(),
                    IOErrorEnum::Cancelled.to_code(),
                    "cancelled",
                ));
            }
        }
        self.pending.lock().pop_front().ok_or_else(|| {
            Error::new(
                io_error_quark(),
                IOErrorEnum::WouldBlock.to_code(),
                "no pending connections",
            )
        })
    }

    /// Returns how many ports are being listened on.
    pub fn n_listening(&self) -> usize {
        self.listening.lock().len()
    }

    /// Returns how many pending connections are queued.
    pub fn n_pending(&self) -> usize {
        self.pending.lock().len()
    }

    /// Simulates an incoming connection (test/platform helper).
    pub fn push_connection(&self, peer: InetSocketAddress) {
        self.pending.lock().push_back(peer);
    }

    /// Closes the listener, refusing further accepts.
    ///
    /// Mirrors `g_socket_listener_close`.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns whether the listener has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Default for SocketListener {
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
        let l = SocketListener::new();
        assert_eq!(l.get_backlog(), 10);
        assert!(!l.is_closed());
        assert_eq!(l.n_listening(), 0);
        assert_eq!(l.n_pending(), 0);
    }

    #[test]
    fn test_add_port() {
        let l = SocketListener::new();
        l.add_inet_port(8080, None).unwrap();
        assert_eq!(l.n_listening(), 1);
    }

    #[test]
    fn test_add_multiple_ports() {
        let l = SocketListener::new();
        l.add_inet_port(80, None).unwrap();
        l.add_inet_port(443, None).unwrap();
        assert_eq!(l.n_listening(), 2);
    }

    #[test]
    fn test_accept_empty() {
        let l = SocketListener::new();
        assert!(l.accept(None).is_err());
    }

    #[test]
    fn test_accept_queued() {
        let l = SocketListener::new();
        l.push_connection(make_addr([192, 168, 1, 1], 12345));
        let peer = l.accept(None).unwrap();
        assert_eq!(peer.port(), 12345);
        assert!(l.accept(None).is_err());
    }

    #[test]
    fn test_accept_order_fifo() {
        let l = SocketListener::new();
        l.push_connection(make_addr([10, 0, 0, 1], 1001));
        l.push_connection(make_addr([10, 0, 0, 2], 1002));
        assert_eq!(l.accept(None).unwrap().port(), 1001);
        assert_eq!(l.accept(None).unwrap().port(), 1002);
    }

    #[test]
    fn test_close_rejects_accept() {
        let l = SocketListener::new();
        l.push_connection(make_addr([127, 0, 0, 1], 80));
        l.close();
        assert!(l.is_closed());
        assert!(l.accept(None).is_err());
    }

    #[test]
    fn test_close_rejects_add_port() {
        let l = SocketListener::new();
        l.close();
        assert!(l.add_inet_port(8080, None).is_err());
    }

    #[test]
    fn test_backlog() {
        let mut l = SocketListener::new();
        l.set_backlog(128);
        assert_eq!(l.get_backlog(), 128);
    }

    #[test]
    fn test_default() {
        let l = SocketListener::default();
        assert_eq!(l.get_backlog(), 10);
    }
}
