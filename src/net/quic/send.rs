//! QUIC send path — assemble and protect outbound 1-RTT packets.
//!
//! Collects the frames a connection currently owes (an ACK if one is pending,
//! then queued stream data within the flow-control and congestion windows),
//! assigns a packet number, and produces a protected short-header datagram via
//! [`super::io::build_short_packet`].

use super::connection::Connection;
use super::crypto::EncryptionLevel;
use super::frame::{encode_ack, encode_crypto, encode_stream};
use super::io::{build_long_packet, build_short_packet};
use super::keys::PacketKeys;
use super::packet::LongPacketType;
use alloc::vec::Vec;

/// Conservative max UDP payload for a QUIC datagram (RFC 9000 §14.1 floor).
const MAX_DATAGRAM: usize = 1200;
/// Upper bound on a single varint-heavy frame header we stage on the stack.
const FRAME_SCRATCH: usize = 64;

/// Assemble the 1-RTT payload (serialized frames) the connection owes, up to
/// `budget` bytes: an ACK first (if pending), then queued STREAM data. Returns
/// `(payload, retransmittable, ack_eliciting)` — ack-eliciting iff it contains a non-ACK frame.
pub fn build_app_payload(conn: &mut Connection, budget: usize) -> (Vec<u8>, Vec<u8>, bool) {
    let mut payload = Vec::new();
    let mut retransmittable = Vec::new();
    let mut ack_eliciting = false;
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
                retransmittable.extend_from_slice(&frame[..n]);
                ack_eliciting = true;
            } else {
                // Did not fit after all — put the bytes back at the front.
                for (i, b) in data.into_iter().enumerate() {
                    stream.send_buf.insert(i, b);
                }
            }
        }
    }

    (payload, retransmittable, ack_eliciting)
}

/// Produce the next protected 1-RTT datagram, or `None` if nothing is pending,
/// 1-RTT keys are not installed, or the congestion window is full. `now` (ms)
/// stamps the packet for loss recovery.
pub fn poll_send(conn: &mut Connection, now: u64) -> Option<Vec<u8>> {
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

    let (payload, retransmittable, ack_eliciting) = {
        let mut retransmit_payload = None;
        for i in 0..conn.retransmit_queue.len() {
            if conn.retransmit_queue[i].0 == EncryptionLevel::OneRtt {
                if conn.retransmit_queue[i].1.len() <= budget {
                    let (_, frames) = conn.retransmit_queue.remove(i);
                    retransmit_payload = Some(frames);
                    break;
                }
            }
        }
        if let Some(frames) = retransmit_payload {
            (frames.clone(), frames, true)
        } else {
            build_app_payload(conn, budget)
        }
    };
    if payload.is_empty() {
        return None;
    }

    let pn = conn.pn_app.take_pn();
    let largest_acked = conn.pn_app.largest_acked;
    let dcid = conn.remote_cid.as_bytes().to_vec();
    let pkt = build_short_packet(&keys, &dcid, pn, largest_acked, &payload).ok()?;

    // Only ack-eliciting packets count as in-flight for congestion control and
    // are tracked for loss recovery.
    if ack_eliciting {
        conn.cong.on_packet_sent(pkt.len() as u64);
    }
    if ack_eliciting {
        super::recovery::on_packet_sent(
            &mut conn.pn_app,
            pn,
            now,
            true,
            pkt.len() as u64,
            EncryptionLevel::OneRtt,
            retransmittable,
        );
    }
    Some(pkt)
}

/// Drive the handshake send side: emit the next protected long-header packet
/// carrying queued CRYPTO data, preferring the Initial level over Handshake.
/// Returns `None` if no handshake CRYPTO is queued at a level whose transmit
/// keys are installed. CRYPTO frames are always ack-eliciting and are tracked
/// for loss recovery in their own packet number space (RFC 9002 §A).
pub fn poll_handshake(conn: &mut Connection, now: u64) -> Option<Vec<u8>> {
    for level in [EncryptionLevel::Initial, EncryptionLevel::Handshake] {
        let km = conn.crypto.tx(level);
        if !km.installed {
            continue;
        }
        let keys = PacketKeys {
            key: km.key.clone(),
            iv: km.iv.clone(),
            hp: km.hp.clone(),
        };

        let mut retransmit_payload = None;
        for i in 0..conn.retransmit_queue.len() {
            if conn.retransmit_queue[i].0 == level {
                let (_, frames) = conn.retransmit_queue.remove(i);
                retransmit_payload = Some(frames);
                break;
            }
        }

        let payload = if let Some(frames) = retransmit_payload {
            frames
        } else {
            // Reserve room for the long header and the CRYPTO frame header + tag.
            let budget = MAX_DATAGRAM.saturating_sub(FRAME_SCRATCH);
            let (offset, data) = match conn.crypto.tx_crypto(level).and_then(|s| s.take(budget)) {
                Some(v) => v,
                None => continue,
            };

            let mut frame = [0u8; MAX_DATAGRAM];
            let n = encode_crypto(offset, &data, &mut frame)?;
            frame[..n].to_vec()
        };

        let typ = match level {
            EncryptionLevel::Initial => LongPacketType::Initial,
            EncryptionLevel::Handshake => LongPacketType::Handshake,
            // Filtered out above; nothing else carries CRYPTO frames.
            _ => continue,
        };
        let version = conn.version;
        let dcid = conn.remote_cid.as_bytes().to_vec();
        let scid = conn.local_cid.as_bytes().to_vec();

        let (pn, largest_acked) = {
            let space = conn.pn_space(level);
            (space.take_pn(), space.largest_acked)
        };

        let pkt = build_long_packet(
            &keys,
            typ,
            version,
            &dcid,
            &scid,
            &[],
            pn,
            largest_acked,
            &payload,
        )
        .ok()?;

        conn.cong.on_packet_sent(pkt.len() as u64);
        super::recovery::on_packet_sent(
            conn.pn_space(level),
            pn,
            now,
            true,
            pkt.len() as u64,
            level,
            payload,
        );
        return Some(pkt);
    }
    None
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
        // Owe an ACK (an ack-eliciting packet 5 was received) and have stream
        // data queued.
        conn.pn_app.on_received(5);
        conn.pn_app.ack_pending = true;
        conn.stream_mut(0).write(b"hello world over quic");

        let pkt = poll_send(&mut conn, 0).expect("should produce a packet");

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
                Frame::Stream {
                    stream_id, data, ..
                } => {
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
        assert!(poll_send(&mut conn, 0).is_none());
    }

    #[test]
    fn poll_handshake_emits_crypto_initial() {
        use super::super::io::open_long_packet;
        use super::super::packet::LongPacketType;

        let path = Path::new(
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
            NetworkAddress::ipv4(0, 0, 0, 0),
            0,
        );
        let dcid = ConnectionId::new(&[0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08]).unwrap();
        let mut conn = Connection::new(
            Role::Client,
            QUIC_VERSION_1,
            ConnectionId::empty(),
            dcid.clone(),
            path,
        );
        // Install Initial keys derived from the (client-chosen) DCID, then queue
        // a ClientHello-ish CRYPTO payload.
        conn.install_initial_keys(dcid.as_bytes());
        conn.crypto
            .queue_crypto(EncryptionLevel::Initial, b"\x01\x00\x00\x04ABCD");

        let pkt = poll_handshake(&mut conn, 0).expect("should emit an Initial packet");

        // Open it with the matching client-initial keys and confirm a CRYPTO
        // frame at offset 0 with our payload.
        let (client, _server) = initial_keys(dcid.as_bytes());
        let (typ, _pn, payload) = open_long_packet(&client, &pkt, 0, None).unwrap();
        assert_eq!(typ, LongPacketType::Initial);
        let (f, _used) = parse_frame(&payload).unwrap();
        match f {
            Frame::Crypto { offset, data } => {
                assert_eq!(offset, 0);
                assert_eq!(data, b"\x01\x00\x00\x04ABCD");
            }
            _ => panic!("expected a CRYPTO frame"),
        }
        // The sent packet is tracked for loss recovery in the Initial space.
        assert!(conn.pn_initial.sent.contains_key(&0));
        // Nothing left queued.
        assert!(poll_handshake(&mut conn, 0).is_none());
    }
}
