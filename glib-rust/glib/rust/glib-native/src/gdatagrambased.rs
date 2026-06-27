//! GDatagramBased matching `gio/gdatagrambased.h`.
//!
//! Interface for socket-like objects with datagram semantics. In this
//! no_std port we model a simple in-memory datagram endpoint with
//! send/receive buffers.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::vec::Vec;
use spin::Mutex;

/// I/O condition flags (subset of `GIOCondition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoCondition(pub u32);

impl IoCondition {
    pub const IN: Self = Self(1);
    pub const OUT: Self = Self(2);
    pub const ERR: Self = Self(4);
    pub const HUP: Self = Self(8);

    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

/// A datagram message.
#[derive(Debug, Clone)]
pub struct Datagram {
    data: Vec<u8>,
    source: Option<Vec<u8>>,
}

impl Datagram {
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            source: None,
        }
    }

    pub fn with_source(data: &[u8], source: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            source: Some(source.to_vec()),
        }
    }

    pub fn get_data(&self) -> &[u8] {
        &self.data
    }

    pub fn get_source(&self) -> Option<&[u8]> {
        self.source.as_deref()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A datagram-based endpoint (`GDatagramBased`).
pub struct DatagramBased {
    rx_queue: Mutex<Vec<Datagram>>,
    tx_log: Mutex<Vec<Datagram>>,
    closed: Mutex<bool>,
}

impl DatagramBased {
    /// Creates a new empty datagram endpoint.
    pub fn new() -> Self {
        Self {
            rx_queue: Mutex::new(Vec::new()),
            tx_log: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Sends a datagram. Returns the number of bytes sent.
    ///
    /// Mirrors `g_datagram_based_send_messages` (simplified).
    pub fn send(&self, data: &[u8]) -> usize {
        if *self.closed.lock() {
            return 0;
        }
        let dgram = Datagram::new(data);
        let len = dgram.len();
        self.tx_log.lock().push(dgram);
        len
    }

    /// Receives a datagram from the queue. Returns `None` if empty.
    ///
    /// Mirrors `g_datagram_based_receive_messages` (simplified).
    pub fn receive(&self) -> Option<Datagram> {
        if *self.closed.lock() {
            return None;
        }
        self.rx_queue.lock().pop()
    }

    /// Injects a datagram into the receive queue (for testing).
    pub fn inject(&self, data: &[u8]) {
        self.rx_queue.lock().push(Datagram::new(data));
    }

    /// Checks which I/O conditions are currently true.
    ///
    /// Mirrors `g_datagram_based_condition_check`.
    pub fn condition_check(&self, condition: IoCondition) -> IoCondition {
        let mut result = 0u32;
        if condition.contains(IoCondition::IN) && !self.rx_queue.lock().is_empty() {
            result |= IoCondition::IN.0;
        }
        if condition.contains(IoCondition::OUT) {
            result |= IoCondition::OUT.0;
        }
        if *self.closed.lock() {
            result |= IoCondition::HUP.0;
        }
        IoCondition(result)
    }

    /// Returns the number of sent datagrams.
    pub fn sent_count(&self) -> usize {
        self.tx_log.lock().len()
    }

    /// Returns the number of pending received datagrams.
    pub fn pending_count(&self) -> usize {
        self.rx_queue.lock().len()
    }

    /// Closes the endpoint.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns whether the endpoint is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Default for DatagramBased {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let db = DatagramBased::new();
        assert!(!db.is_closed());
        assert_eq!(db.pending_count(), 0);
        assert_eq!(db.sent_count(), 0);
    }

    #[test]
    fn test_send_and_receive() {
        let db = DatagramBased::new();
        db.inject(b"hello");
        let dgram = db.receive().unwrap();
        assert_eq!(dgram.get_data(), b"hello");
        assert!(db.receive().is_none());
    }

    #[test]
    fn test_send_logs() {
        let db = DatagramBased::new();
        db.send(b"outgoing");
        assert_eq!(db.sent_count(), 1);
    }

    #[test]
    fn test_condition_check_in() {
        let db = DatagramBased::new();
        db.inject(b"data");
        let cond = db.condition_check(IoCondition::IN);
        assert!(cond.contains(IoCondition::IN));
    }

    #[test]
    fn test_condition_check_empty() {
        let db = DatagramBased::new();
        let cond = db.condition_check(IoCondition::IN);
        assert!(!cond.contains(IoCondition::IN));
    }

    #[test]
    fn test_condition_check_out() {
        let db = DatagramBased::new();
        let cond = db.condition_check(IoCondition::OUT);
        assert!(cond.contains(IoCondition::OUT));
    }

    #[test]
    fn test_close() {
        let db = DatagramBased::new();
        db.close();
        assert!(db.is_closed());
        assert_eq!(db.send(b"data"), 0);
        assert!(db.receive().is_none());
    }

    #[test]
    fn test_datagram_with_source() {
        let d = Datagram::with_source(b"payload", b"127.0.0.1:8080");
        assert_eq!(d.get_data(), b"payload");
        assert_eq!(d.get_source().unwrap(), b"127.0.0.1:8080");
    }

    #[test]
    fn test_datagram_len() {
        let d = Datagram::new(b"hello");
        assert_eq!(d.len(), 5);
        assert!(!d.is_empty());
    }
}
