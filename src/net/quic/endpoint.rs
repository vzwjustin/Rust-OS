//! QUIC endpoint — connection registry and inbound datagram demultiplexing.
//!
//! Mirrors the role of `net/quic/socket.c` + `protocol.c`: a single UDP socket
//! backs many QUIC connections, and inbound datagrams are routed to a
//! connection by their Destination Connection ID rather than the UDP 4-tuple
//! (which lets a connection survive client address migration).
//!
//! This is the routing/lifecycle layer; per-packet AEAD removal (RFC 9001) is a
//! later sub-phase, after which `deliver` will decrypt and feed frames into the
//! matched [`Connection`].

use super::connection::{Connection, Role};
use super::connid::ConnectionId;
use super::packet::{LongPacketType, PacketHeader};
use super::{parse_header, peek_destination};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Result of routing an inbound datagram.
#[derive(Debug, PartialEq, Eq)]
pub enum DemuxResult {
    /// Datagram routed to an existing connection (its local CID).
    Existing(ConnectionId),
    /// An Initial packet for an unknown connection — a server endpoint should
    /// create a new connection for this DCID/SCID.
    NewConnection,
    /// A datagram that could not be parsed or routed (dropped).
    Dropped,
}

/// A QUIC endpoint owning every connection multiplexed over one UDP socket.
pub struct QuicEndpoint {
    role: Role,
    /// Length of the connection IDs this endpoint issues (fixed per endpoint so
    /// short-header DCIDs can be located without a length prefix).
    local_cid_len: usize,
    /// Connections keyed by the local connection ID the peer addresses.
    connections: BTreeMap<Vec<u8>, Connection>,
}

impl QuicEndpoint {
    pub fn new(role: Role, local_cid_len: usize) -> Self {
        Self {
            role,
            local_cid_len,
            connections: BTreeMap::new(),
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Register a connection under its local connection ID.
    pub fn insert(&mut self, conn: Connection) {
        self.connections
            .insert(conn.local_cid.as_bytes().to_vec(), conn);
    }

    /// Look up a connection by its local connection ID.
    pub fn get_mut(&mut self, cid: &ConnectionId) -> Option<&mut Connection> {
        self.connections.get_mut(cid.as_bytes())
    }

    /// Remove a connection (e.g. after CONNECTION_CLOSE drains).
    pub fn remove(&mut self, cid: &ConnectionId) -> Option<Connection> {
        self.connections.remove(cid.as_bytes())
    }

    /// Route an inbound UDP payload to a connection by Destination Connection
    /// ID. Returns how the datagram should be handled; this does not yet remove
    /// AEAD protection or process frames.
    pub fn route(&self, datagram: &[u8]) -> DemuxResult {
        let dcid = match peek_destination(datagram, self.local_cid_len) {
            Some(cid) => cid,
            None => return DemuxResult::Dropped,
        };

        if self.connections.contains_key(dcid.as_bytes()) {
            return DemuxResult::Existing(dcid);
        }

        // No matching connection. A server may accept a new connection from an
        // Initial packet; anything else is dropped (or would trigger a
        // stateless reset, handled elsewhere).
        if self.role == Role::Server {
            if let Some(PacketHeader::Long {
                typ: LongPacketType::Initial,
                ..
            }) = parse_header(datagram, self.local_cid_len)
            {
                return DemuxResult::NewConnection;
            }
        }
        DemuxResult::Dropped
    }

    /// Migrate a connection's routing key in the endpoint registry.
    pub fn migrate_connection_cid(
        &mut self,
        old_cid: &ConnectionId,
        new_cid: ConnectionId,
    ) -> bool {
        if let Some(mut conn) = self.connections.remove(old_cid.as_bytes()) {
            conn.update_local_cid(new_cid.clone());
            self.connections.insert(new_cid.as_bytes().to_vec(), conn);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::QUIC_VERSION_1;
    use super::super::path::Path;
    use super::*;
    use crate::net::NetworkAddress;

    fn dummy_path() -> Path {
        Path::new(
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
        )
    }

    #[test]
    fn routes_to_existing_connection() {
        let mut ep = QuicEndpoint::new(Role::Server, 4);
        let cid = ConnectionId::new(b"aaaa").unwrap();
        ep.insert(Connection::new(
            Role::Server,
            QUIC_VERSION_1,
            cid.clone(),
            ConnectionId::empty(),
            dummy_path(),
        ));
        // Short-header datagram addressed to DCID "aaaa".
        let pkt = alloc::vec![0x40u8, b'a', b'a', b'a', b'a', 0x00];
        assert_eq!(ep.route(&pkt), DemuxResult::Existing(cid));
    }

    #[test]
    fn server_accepts_new_initial() {
        let ep = QuicEndpoint::new(Role::Server, 0);
        // Long Initial (0xC0), version 1, DCID len 4, SCID len 0, token-len 0.
        let pkt = alloc::vec![0xC0u8, 0, 0, 0, 1, 4, b'b', b'b', b'b', b'b', 0, 0x00];
        assert_eq!(ep.route(&pkt), DemuxResult::NewConnection);
    }

    #[test]
    fn unknown_short_header_is_dropped() {
        let ep = QuicEndpoint::new(Role::Server, 4);
        let pkt = alloc::vec![0x40u8, b'z', b'z', b'z', b'z', 0x00];
        assert_eq!(ep.route(&pkt), DemuxResult::Dropped);
    }
}
