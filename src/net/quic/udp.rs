//! QUIC ↔ UDP integration.
//!
//! QUIC runs over UDP. This module owns the registry of QUIC-bound local ports
//! and the entry point the UDP receive path calls to hand a datagram to the
//! matching [`QuicEndpoint`], which demultiplexes it by Destination Connection
//! ID and (for an established connection with installed 1-RTT keys) removes
//! protection and applies the frames.

use super::super::NetworkAddress;
use super::connection::Connection;
use super::endpoint::{DemuxResult, QuicEndpoint};
use super::io::{apply_frames, open_short_packet};
use super::keys::PacketKeys;
use super::Role;
use alloc::collections::BTreeMap;
use lazy_static::lazy_static;
use spin::RwLock;

lazy_static! {
    /// QUIC endpoints keyed by the local UDP port they are bound to.
    static ref QUIC_PORTS: RwLock<BTreeMap<u16, QuicEndpoint>> = RwLock::new(BTreeMap::new());
}

/// Bind a QUIC endpoint to `port`, replacing any existing one.
pub fn bind_port(port: u16, role: Role, local_cid_len: usize) {
    QUIC_PORTS
        .write()
        .insert(port, QuicEndpoint::new(role, local_cid_len));
}

/// Remove the QUIC endpoint bound to `port`.
pub fn unbind_port(port: u16) {
    QUIC_PORTS.write().remove(&port);
}

/// Whether a QUIC endpoint is listening on `port` (the UDP layer uses this to
/// decide whether to route a datagram to QUIC).
pub fn is_bound(port: u16) -> bool {
    QUIC_PORTS.read().contains_key(&port)
}

/// Run a closure against the endpoint bound to `port`, if any.
pub fn with_endpoint<R>(port: u16, f: impl FnOnce(&mut QuicEndpoint) -> R) -> Option<R> {
    QUIC_PORTS.write().get_mut(&port).map(f)
}

/// Deliver an inbound UDP payload carrying a QUIC packet.
///
/// Returns `true` if a QUIC endpoint recognized and consumed the datagram.
pub fn deliver(
    local_port: u16,
    _src: NetworkAddress,
    _src_port: u16,
    payload: &[u8],
) -> bool {
    let mut ports = QUIC_PORTS.write();
    let endpoint = match ports.get_mut(&local_port) {
        Some(ep) => ep,
        None => return false,
    };

    match endpoint.route(payload) {
        DemuxResult::Existing(cid) => {
            if let Some(conn) = endpoint.get_mut(&cid) {
                process_existing(conn, payload);
            }
            true
        }
        DemuxResult::NewConnection => {
            // Server-side Initial for an unknown connection. Initial-key
            // derivation and the userspace handshake hand-off complete this
            // path; for now the datagram is recognized as QUIC and consumed
            // rather than being mistaken for an unbound UDP port.
            true
        }
        DemuxResult::Dropped => false,
    }
}

/// Process a datagram for an established connection: if 1-RTT receive keys are
/// installed, remove protection and apply the frames.
fn process_existing(conn: &mut Connection, datagram: &[u8]) {
    let km = &conn.crypto.rx_app;
    if !km.installed {
        return; // handshake not complete; 1-RTT keys not yet available
    }
    let keys = PacketKeys {
        key: km.key.clone(),
        iv: km.iv.clone(),
        hp: km.hp.clone(),
    };
    let dcid_len = conn.local_cid.len();
    let largest = conn.pn_app.largest_received;

    if let Ok((pn, payload)) = open_short_packet(&keys, datagram, dcid_len, largest) {
        conn.pn_app.on_received(pn);
        let _ = apply_frames(conn, &payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_and_query() {
        bind_port(4433, Role::Server, 8);
        assert!(is_bound(4433));
        // An unparseable datagram on a bound port is not consumed.
        assert!(!deliver(4433, NetworkAddress::ipv4(0, 0, 0, 0), 1, &[0xff]));
        unbind_port(4433);
        assert!(!is_bound(4433));
    }
}
