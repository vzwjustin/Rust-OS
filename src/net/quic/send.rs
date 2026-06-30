//! QUIC send path — assemble and protect outbound 1-RTT packets.
//!
//! Collects the frames a connection currently owes (an ACK if one is pending,
//! then queued stream data within the flow-control and congestion windows),
//! assigns a packet number, and produces a protected short-header datagram via
//! [`super::io::build_short_packet`].

use super::connection::Connection;
use super::frame::{encode_ack, encode_stream};
use super::io::build_short_packet;
use super::keys::PacketKeys;
use alloc::vec::Vec;

/// Conservative max UDP payload for a QUIC datagram (RFC 9000 §14.1 floor).
const MAX_DATAGRAM: usize = 1200;
/// Upper bound on a single varint-heavy frame header we stage on the stack.
const FRAME_SCRATCH: usize = 64;

/// Assemble the 1-RTT payload (serialized frames) the connection owes, up to
/// `budget` bytes: an ACK first (if pending), then queued STREAM data.
pub fn build_app_payload(conn: &mut Connection, budget: usize) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut scratch = [0u8; FRAME_SCRATCH];

    // ACK frame first so the peer's loss recovery sees it promptly.
    if conn.pn_app.ack_pending {
        if let Some((largest, first_range, ranges)) = conn.pn_app.ack_fields() {
            if let Some(n) = encode_ack(largest, 0, first_range, &ranges, &mut scratch) {
                if payload.len() + n <= budget {
                    payload.extend_from_slice(&scratch[..n]);
                    conn.pn_app.ack_pending = false;
                }
            }
        }
    }

    // Drain stream send buffers into STREAM frames. The offset of the first
    // buffered byte is `send_offset - send_buf.len()`.
    let stream_ids: Vec<u64> = conn.streams.keys().copied().collect();
    for id in stream_ids {
        let remaining = budget.saturating_sub(payload.len());
        if remaining <= FRAME_SCRATCH {
            break;
        }
        let stream = conn.stream_mut(id);
        if stream.send_buf.is_empty() {
            continue;
        }
        let stream_off = stream.send_offset - stream.send_buf.len() as u64;
        // Reserve room for the frame header (type + 3 varints, ≤ ~20 bytes).
        let max_data = remaining - 24;
        let take = core::cmp::min(stream.send_buf.len(), max_data);
        if take == 0 {
            continue;
        }
        let data: Vec<u8> = stream.send_buf.drain(..take).collect();
        let mut frame = [0u8; MAX_DATAGRAM];
        if let Some(n) = encode_stream(id, stream_off, false, &data, &mut frame) {
            if payload.len() + n <= budget {
                payload.extend_from_slice(&frame[..n]);
            } else {
                // Did not fit after all — put the bytes back at the front.
                for (i, b) in data.into_iter().enumerate() {
                    stream.send_buf.insert(i, b);
                }
            }
        }
    }

    payload
}

/// Produce the next protected 1-RTT datagram, or `None` if nothing is pending,
/// 1-RTT keys are not installed, or the congestion window is full.
pub fn poll_send(conn: &mut Connection) -> Option<Vec<u8>> {
    let km = &conn.crypto.tx_app;
    if !km.installed {
        return None;
    }
    let keys = PacketKeys {
        key: km.key.clone(),
        iv: km.iv.clone(),
        hp: km.hp.clone(),
    };

    // Stay within the congestion window.
    let in_flight = conn.cong.bytes_in_flight;
    let cwnd = conn.cong.cwnd;
    let cwnd_room = cwnd.saturating_sub(in_flight) as usize;
    if cwnd_room == 0 {
        return None;
    }
    let budget = core::cmp::min(MAX_DATAGRAM, cwnd_room);

    let payload = build_app_payload(conn, budget);
    if payload.is_empty() {
        return None;
    }

    let pn = conn.pn_app.take_pn();
    let largest_acked = conn.pn_app.largest_acked;
    let dcid = conn.remote_cid.as_bytes().to_vec();
    let pkt = build_short_packet(&keys, &dcid, pn, largest_acked, &payload).ok()?;
    conn.cong.on_packet_sent(pkt.len() as u64);
    Some(pkt)
}

#[cfg(test)]
mod tests {
    use super::super::common::QUIC_VERSION_1;
    use super::super::connection::{Connection, Role};
    use super::super::connid::ConnectionId;
    use super::super::frame::{parse_frame, Frame};
    use super::super::io::open_short_packet;
    use super::super::keys::initial_keys;
    use super::super::path::Path;
    use super::*;
    use crate::net::NetworkAddress;

    fn test_conn() -> (Connection, PacketKeys) {
        let path = Path::new(
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
        );
        // Local CID is what the peer puts in the DCID; use a 4-byte CID.
        let local = ConnectionId::new(&[1, 2, 3, 4]).unwrap();
        let remote = ConnectionId::new(&[1, 2, 3, 4]).unwrap();
        let mut conn = Connection::new(Role::Server, QUIC_VERSION_1, local, remote, path);
        // Install matching tx keys (sender) so we can open with the same keys.
        let (keys, _) = initial_keys(&[9, 9, 9, 9]);
        conn.crypto
            .tx_app
            .install(keys.key.clone(), keys.iv.clone(), keys.hp.clone());
        conn.on_handshake_complete();
        (conn, keys)
    }

    #[test]
    fn poll_send_emits_ack_and_stream() {
        let (mut conn, keys) = test_conn();
        // Owe an ACK and have stream data queued.
        conn.pn_app.on_received(5);
        conn.stream_mut(0).write(b"hello world over quic");

        let pkt = poll_send(&mut conn).expect("should produce a packet");

        // Open it back with the same keys and check the frames.
        let (_pn, payload) = open_short_packet(&keys, &pkt, 4, None).unwrap();
        let mut saw_ack = false;
        let mut saw_stream = false;
        let mut off = 0;
        while off < payload.len() {
            let (f, used) = parse_frame(&payload[off..]).unwrap();
            match f {
                Frame::Ack { largest, .. } => {
                    assert_eq!(largest, 5);
                    saw_ack = true;
                }
                Frame::Stream { stream_id, data, .. } => {
                    assert_eq!(stream_id, 0);
                    assert_eq!(data, b"hello world over quic");
                    saw_stream = true;
                }
                _ => {}
            }
            off += used.max(1);
        }
        assert!(saw_ack && saw_stream);
        // Nothing left to send.
        assert!(poll_send(&mut conn).is_none());
    }
}
