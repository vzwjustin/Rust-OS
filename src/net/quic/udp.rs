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
use super::io::{apply_frames, open_long_packet, open_short_packet};
use super::keys::PacketKeys;
use super::packet::{parse_header, PacketHeader};
use super::path::Path;
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
pub fn deliver(local_port: u16, _src: NetworkAddress, _src_port: u16, payload: &[u8]) -> bool {
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
        DemuxResult::NewConnection => accept_initial(endpoint, _src, _src_port, payload),
        DemuxResult::Dropped => false,
    }
}

/// Accept a server-side Initial packet for an unknown connection: create the
/// connection keyed by the client-chosen DCID, derive the Initial keys from
/// that DCID, remove protection, and feed the CRYPTO frames into the handshake
/// buffer. The TLS handshake itself is driven by the offloaded userspace side,
/// which then installs the Handshake/1-RTT secrets via `install_secret`.
///
/// Returns `true` (consumed) only if a connection was actually created.
fn accept_initial(
    endpoint: &mut QuicEndpoint,
    src: NetworkAddress,
    src_port: u16,
    datagram: &[u8],
) -> bool {
    let (version, dcid, scid) = match parse_header(datagram, 0) {
        Some(PacketHeader::Long {
            version,
            dcid,
            scid,
            ..
        }) => (version, dcid, scid),
        _ => return false,
    };

    // The server's connection is addressed by the DCID the client chose; route
    // subsequent client packets to it under that CID.
    let path = Path::new(NetworkAddress::ipv4(0, 0, 0, 0), 0, src, src_port);
    let mut conn = Connection::new(Role::Server, version, dcid.clone(), scid, path);
    conn.install_initial_keys(dcid.as_bytes());

    let km = &conn.crypto.rx_initial;
    if km.installed {
        let keys = PacketKeys {
            key: km.key.clone(),
            iv: km.iv.clone(),
            hp: km.hp.clone(),
        };
        if let Ok((_typ, pn, plaintext)) = open_long_packet(&keys, datagram, dcid.len(), None) {
            conn.pn_initial.on_received(pn);
            let outcome = apply_frames(&mut conn, &plaintext, super::now_ms());
            // Only ack-eliciting packets oblige us to send an ACK (RFC 9000
            // §13.2.1); acking pure-ACK packets would risk an ACK loop.
            if outcome.ack_eliciting {
                conn.pn_initial.ack_pending = true;
            }
        }
    }

    endpoint.insert(conn);
    true
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
        let outcome = apply_frames(conn, &payload, super::now_ms());
        // Only ack-eliciting packets oblige us to send an ACK (RFC 9000
        // §13.2.1); acking pure-ACK packets would risk an ACK loop.
        if outcome.ack_eliciting {
            conn.pn_app.ack_pending = true;
        }
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
