//! QUIC connection IDs (RFC 9000 §5.1).
//!
//! Mirrors `net/quic/connid.{c,h}`. A connection ID is an opaque byte string of
//! 0–20 bytes used to route packets to a connection independent of the
//! 4-tuple, enabling connection migration.

use alloc::vec::Vec;

/// Maximum connection ID length permitted by QUIC v1 (RFC 9000 §17.2).
pub const MAX_CONNID_LEN: usize = 20;

/// A QUIC connection ID: 0..=20 opaque bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConnectionId {
    bytes: Vec<u8>,
}

impl ConnectionId {
    /// Construct a connection ID, rejecting anything longer than
    /// [`MAX_CONNID_LEN`].
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_CONNID_LEN {
            return None;
        }
        Some(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// The zero-length connection ID (valid; used when an endpoint does not
    /// need to route by CID).
    pub fn empty() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// An entry in the connection ID set, tracking the sequence number and the
/// stateless-reset token associated with a CID (RFC 9000 §5.1.1).
#[derive(Debug, Clone)]
pub struct ConnIdEntry {
    pub seq: u64,
    pub cid: ConnectionId,
    pub reset_token: Option<[u8; 16]>,
}
