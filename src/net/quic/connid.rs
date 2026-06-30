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

/// Tracks the connection IDs issued by this endpoint and advertised by the
/// peer (RFC 9000 §5.1), including retirement.
#[derive(Debug, Default)]
pub struct CidManager {
    /// CIDs this endpoint has issued (the peer may use any active one as DCID).
    pub local: alloc::vec::Vec<ConnIdEntry>,
    /// CIDs the peer has advertised for us to use as DCID.
    pub remote: alloc::vec::Vec<ConnIdEntry>,
    next_local_seq: u64,
}

impl CidManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue a new local connection ID, returning its sequence number.
    pub fn issue_local(&mut self, cid: ConnectionId, reset_token: Option<[u8; 16]>) -> u64 {
        let seq = self.next_local_seq;
        self.next_local_seq += 1;
        self.local.push(ConnIdEntry {
            seq,
            cid,
            reset_token,
        });
        seq
    }

    /// Record a peer-advertised connection ID (from NEW_CONNECTION_ID), honoring
    /// its `retire_prior_to` by dropping lower-sequence remote CIDs.
    pub fn add_remote(
        &mut self,
        seq: u64,
        cid: ConnectionId,
        reset_token: [u8; 16],
        retire_prior_to: u64,
    ) {
        self.remote.retain(|e| e.seq >= retire_prior_to);
        if !self.remote.iter().any(|e| e.seq == seq) {
            self.remote.push(ConnIdEntry {
                seq,
                cid,
                reset_token: Some(reset_token),
            });
        }
    }

    /// Drop a local connection ID the peer asked us to retire.
    pub fn retire_local(&mut self, seq: u64) {
        self.local.retain(|e| e.seq != seq);
    }

    /// The current connection ID to address the peer with (highest sequence).
    pub fn active_remote(&self) -> Option<&ConnectionId> {
        self.remote.iter().max_by_key(|e| e.seq).map(|e| &e.cid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_retire_prior_to() {
        let mut m = CidManager::new();
        assert_eq!(m.issue_local(ConnectionId::new(b"aaaa").unwrap(), None), 0);
        assert_eq!(m.issue_local(ConnectionId::new(b"bbbb").unwrap(), None), 1);
        m.retire_local(0);
        assert_eq!(m.local.len(), 1);

        m.add_remote(0, ConnectionId::new(b"r0").unwrap(), [0; 16], 0);
        m.add_remote(1, ConnectionId::new(b"r1").unwrap(), [0; 16], 0);
        // retire_prior_to = 1 drops seq 0.
        m.add_remote(2, ConnectionId::new(b"r2").unwrap(), [0; 16], 1);
        assert_eq!(m.active_remote().unwrap().as_bytes(), b"r2");
        assert!(!m.remote.iter().any(|e| e.seq == 0));
    }
}
