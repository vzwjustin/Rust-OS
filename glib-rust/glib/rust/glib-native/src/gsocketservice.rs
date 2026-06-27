//! Native Rust port of `GSocketService` from GIO.
//!
//! `SocketService` extends a socket-listener concept to dispatch accepted
//! connections.  In this no_std port there are no real OS sockets; instead the
//! struct maintains an `active` flag and an `incoming` queue so behaviour can
//! be exercised through unit tests.

use crate::prelude::*;
use spin::Mutex;

/// A single accepted connection, keyed by the remote address string.
pub struct IncomingConnection {
    /// Textual representation of the remote endpoint (e.g. `"127.0.0.1:1234"`).
    pub remote_addr: String,
}

/// Mirrors `GSocketService`.
///
/// Call [`start`](SocketService::start) to begin accepting connections and
/// [`accept`](SocketService::accept) to enqueue an `IncomingConnection`.
pub struct SocketService {
    active: Mutex<bool>,
    incoming: Mutex<Vec<IncomingConnection>>,
}

impl SocketService {
    /// Creates a new, **inactive** `SocketService`.
    ///
    /// Mirrors `g_socket_service_new`.
    pub fn new() -> Self {
        Self {
            active: Mutex::new(false),
            incoming: Mutex::new(Vec::new()),
        }
    }

    /// Starts the service so that new connections are accepted.
    ///
    /// Mirrors `g_socket_service_start`.
    pub fn start(&self) {
        *self.active.lock() = true;
    }

    /// Stops the service; subsequent [`accept`](SocketService::accept) calls
    /// return `false`.
    ///
    /// Mirrors `g_socket_service_stop`.
    pub fn stop(&self) {
        *self.active.lock() = false;
    }

    /// Returns `true` if the service is currently active.
    ///
    /// Mirrors `g_socket_service_is_active`.
    pub fn is_active(&self) -> bool {
        *self.active.lock()
    }

    /// Simulates an incoming connection from `remote_addr`.
    ///
    /// If the service is active the connection is enqueued and `true` is
    /// returned (analogous to the `incoming` signal firing).  If the service
    /// is stopped, the connection is silently dropped and `false` is returned.
    pub fn accept(&self, remote_addr: &str) -> bool {
        if !*self.active.lock() {
            return false;
        }
        self.incoming.lock().push(IncomingConnection {
            remote_addr: remote_addr.to_owned(),
        });
        true
    }

    /// Removes and returns all pending connections, leaving the queue empty.
    ///
    /// Useful in tests to inspect what was accepted without keeping the lock.
    pub fn drain_incoming(&self) -> Vec<IncomingConnection> {
        let mut guard = self.incoming.lock();
        core::mem::replace(&mut *guard, Vec::new())
    }

    /// Returns the number of connections currently waiting in the queue.
    pub fn pending_count(&self) -> usize {
        self.incoming.lock().len()
    }
}

impl Default for SocketService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_inactive() {
        let svc = SocketService::new();
        assert!(!svc.is_active());
    }

    #[test]
    fn start_makes_active() {
        let svc = SocketService::new();
        svc.start();
        assert!(svc.is_active());
    }

    #[test]
    fn stop_makes_inactive() {
        let svc = SocketService::new();
        svc.start();
        svc.stop();
        assert!(!svc.is_active());
    }

    #[test]
    fn accept_rejected_when_inactive() {
        let svc = SocketService::new();
        assert!(!svc.accept("10.0.0.1:9999"));
        assert_eq!(svc.pending_count(), 0);
    }

    #[test]
    fn accept_enqueues_when_active() {
        let svc = SocketService::new();
        svc.start();
        assert!(svc.accept("192.168.1.1:443"));
        assert_eq!(svc.pending_count(), 1);
    }

    #[test]
    fn drain_returns_all_and_clears_queue() {
        let svc = SocketService::new();
        svc.start();
        svc.accept("1.2.3.4:80");
        svc.accept("5.6.7.8:443");
        let drained = svc.drain_incoming();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].remote_addr, "1.2.3.4:80");
        assert_eq!(drained[1].remote_addr, "5.6.7.8:443");
        assert_eq!(svc.pending_count(), 0);
    }

    #[test]
    fn multiple_accepts_accumulate() {
        let svc = SocketService::new();
        svc.start();
        for i in 0..5u32 {
            svc.accept(&alloc::format!("10.0.0.{}:1234", i));
        }
        assert_eq!(svc.pending_count(), 5);
    }

    #[test]
    fn stop_does_not_affect_already_queued() {
        let svc = SocketService::new();
        svc.start();
        svc.accept("172.16.0.1:22");
        svc.stop();
        // queue still has the connection accepted before stop
        assert_eq!(svc.pending_count(), 1);
        let drained = svc.drain_incoming();
        assert_eq!(drained[0].remote_addr, "172.16.0.1:22");
    }

    #[test]
    fn default_is_same_as_new() {
        let svc: SocketService = Default::default();
        assert!(!svc.is_active());
        assert_eq!(svc.pending_count(), 0);
    }
}
