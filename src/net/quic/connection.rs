//! QUIC connection state (RFC 9000 §5).
//!
//! Mirrors the connection object that `net/quic/socket.c` and `protocol.c`
//! operate on: it owns the connection IDs, the three packet number spaces, the
//! crypto/key state, congestion control, RTT/timers, the active path, and the
//! stream table, and drives the connection state machine.

use super::cong::Cong;
use super::connid::{CidManager, ConnectionId};
use super::crypto::{CryptoState, EncryptionLevel};
use super::keys::{derive_packet_keys, initial_keys};
use super::path::Path;
use super::pnspace::{PnSpace, PnSpaceKind};
use super::stream::Stream;
use super::timer::RttEstimator;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Endpoint role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Client,
    Server,
}

/// Connection-level state machine (RFC 9000 §5, simplified to the externally
/// observable phases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// Handshake in progress (Initial/Handshake levels).
    Handshaking,
    /// Handshake complete; 1-RTT data may flow.
    Established,
    /// CONNECTION_CLOSE sent/received; draining (RFC 9000 §10.2).
    Closing,
    Draining,
    Closed,
}

pub struct Connection {
    pub role: Role,
    pub state: ConnState,
    pub version: u32,

    /// Our connection IDs (the peer's DCID) and the peer's source CID.
    pub local_cid: ConnectionId,
    pub remote_cid: ConnectionId,

    /// Per-space packet number state.
    pub pn_initial: PnSpace,
    pub pn_handshake: PnSpace,
    pub pn_app: PnSpace,

    pub crypto: CryptoState,
    pub cong: Cong,
    pub rtt: RttEstimator,
    pub path: Path,

    /// Connection-level flow control.
    pub max_data_local: u64,
    pub max_data_remote: u64,
    pub data_sent: u64,
    pub data_recv: u64,

    /// Open streams keyed by stream ID.
    pub streams: BTreeMap<u64, Stream>,
    /// Per-stream initial flow-control limit advertised to the peer.
    pub initial_max_stream_data: u64,

    /// Connection ID set management (issued + peer-advertised CIDs).
    pub cids: CidManager,

    /// Retransmission queue for lost packet payloads.
    pub retransmit_queue: Vec<(EncryptionLevel, Vec<u8>)>,
}

impl Connection {
    pub fn new(
        role: Role,
        version: u32,
        local_cid: ConnectionId,
        remote_cid: ConnectionId,
        path: Path,
    ) -> Self {
        Self {
            role,
            state: ConnState::Handshaking,
            version,
            local_cid,
            remote_cid,
            pn_initial: PnSpace::new(),
            pn_handshake: PnSpace::new(),
            pn_app: PnSpace::new(),
            crypto: CryptoState::default(),
            cong: Cong::new(1452),
            rtt: RttEstimator::new(25),
            path,
            max_data_local: 1 << 20,
            max_data_remote: 0,
            data_sent: 0,
            data_recv: 0,
            streams: BTreeMap::new(),
            initial_max_stream_data: 256 * 1024,
            cids: CidManager::new(),
            retransmit_queue: Vec::new(),
        }
    }

    /// Update the local connection ID.
    pub fn update_local_cid(&mut self, new_cid: ConnectionId) {
        self.local_cid = new_cid;
    }

    /// Periodic tick handler: runs loss detection and moves lost packets to the retransmit queue.
    pub fn tick(&mut self, now: u64) {
        let Self {
            pn_initial,
            pn_handshake,
            pn_app,
            cong,
            rtt,
            retransmit_queue,
            ..
        } = self;

        // Level: Initial
        let lost_initial = super::recovery::detect_lost(pn_initial, cong, now, rtt);
        for pkt in lost_initial {
            if pkt.ack_eliciting && !pkt.frames.is_empty() {
                retransmit_queue.push((pkt.level, pkt.frames));
            }
        }

        // Level: Handshake
        let lost_handshake = super::recovery::detect_lost(pn_handshake, cong, now, rtt);
        for pkt in lost_handshake {
            if pkt.ack_eliciting && !pkt.frames.is_empty() {
                retransmit_queue.push((pkt.level, pkt.frames));
            }
        }

        // Level: OneRtt
        let lost_app = super::recovery::detect_lost(pn_app, cong, now, rtt);
        for pkt in lost_app {
            if pkt.ack_eliciting && !pkt.frames.is_empty() {
                retransmit_queue.push((pkt.level, pkt.frames));
            }
        }
    }

    /// Mutable access to a packet number space by encryption level.
    pub fn pn_space(&mut self, level: EncryptionLevel) -> &mut PnSpace {
        match level {
            EncryptionLevel::Initial => &mut self.pn_initial,
            EncryptionLevel::Handshake => &mut self.pn_handshake,
            EncryptionLevel::ZeroRtt | EncryptionLevel::OneRtt => &mut self.pn_app,
        }
    }

    /// Map an encryption level to its packet number space kind.
    pub fn pn_kind(level: EncryptionLevel) -> PnSpaceKind {
        match level {
            EncryptionLevel::Initial => PnSpaceKind::Initial,
            EncryptionLevel::Handshake => PnSpaceKind::Handshake,
            EncryptionLevel::ZeroRtt | EncryptionLevel::OneRtt => PnSpaceKind::Application,
        }
    }

    /// Open (or fetch) a stream by ID.
    pub fn stream_mut(&mut self, stream_id: u64) -> &mut Stream {
        let limit = self.initial_max_stream_data;
        self.streams
            .entry(stream_id)
            .or_insert_with(|| Stream::new(stream_id, limit))
    }

    /// Derive and install the Initial-level keys from the client's Destination
    /// Connection ID (RFC 9001 §5.2). The transmit/receive assignment follows
    /// this endpoint's role: a server sends with the server-initial keys and
    /// receives with the client-initial keys, and vice versa.
    pub fn install_initial_keys(&mut self, client_dcid: &[u8]) {
        let (client, server) = initial_keys(client_dcid);
        let (tx, rx) = match self.role {
            Role::Client => (client, server),
            Role::Server => (server, client),
        };
        self.crypto.tx_initial.install(tx.key, tx.iv, tx.hp);
        self.crypto.rx_initial.install(rx.key, rx.iv, rx.hp);
    }

    /// Install a traffic secret for an encryption level and direction (handed
    /// up by the userspace TLS handshake), deriving the AEAD key / IV /
    /// header-protection key (RFC 9001 §5.1).
    pub fn install_secret(&mut self, level: EncryptionLevel, transmit: bool, secret: &[u8]) {
        let derived = derive_packet_keys(secret);
        let km = if transmit {
            self.crypto.tx(level)
        } else {
            self.crypto.rx(level)
        };
        km.install(derived.key, derived.iv, derived.hp);
    }

    /// Mark the handshake complete and transition to 1-RTT data flow.
    pub fn on_handshake_complete(&mut self) {
        if self.state == ConnState::Handshaking {
            self.state = ConnState::Established;
        }
    }

    /// Begin connection close (RFC 9000 §10.2).
    pub fn begin_close(&mut self) {
        self.state = match self.state {
            ConnState::Closed => ConnState::Closed,
            _ => ConnState::Closing,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::NetworkAddress;
    use super::*;

    fn dummy_path() -> Path {
        Path::new(
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
        )
    }

    #[test]
    fn handshake_then_established() {
        let mut c = Connection::new(
            Role::Client,
            super::super::common::QUIC_VERSION_1,
            ConnectionId::empty(),
            ConnectionId::empty(),
            dummy_path(),
        );
        assert_eq!(c.state, ConnState::Handshaking);
        c.on_handshake_complete();
        assert_eq!(c.state, ConnState::Established);
        // PN spaces are independent.
        assert_eq!(c.pn_space(EncryptionLevel::Initial).take_pn(), 0);
        assert_eq!(c.pn_space(EncryptionLevel::OneRtt).take_pn(), 0);
    }
}
