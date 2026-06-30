//! QUIC packet I/O — build and open fully-protected 1-RTT (short header)
//! packets, and apply received frames to a connection.
//!
//! Ties the header parsing, packet-number spaces, frame codec, and packet
//! protection together into the steady-state data path. Long-header
//! (handshake) packets follow the same protection rules but with the
//! token/length fields parsed first; this module implements the 1-RTT path
//! that carries application data.

use super::common::decode_varint;
use super::connection::{Connection, Role};
use super::frame::{parse_frame, Frame};
use super::keys::PacketKeys;
use super::packet::{parse_header, parse_initial_token, LongPacketType, PacketHeader};
use super::pnspace::{pn_decode, pn_encode_len};
use super::protection::{apply_header_protection, header_protection_mask, open, seal};
use crate::crypto::algapi::CryptoError;
use alloc::vec::Vec;

/// Header-protection sample is taken 4 bytes past the start of the packet
/// number field and is 16 bytes long (RFC 9001 §5.4.2).
const SAMPLE_OFFSET: usize = 4;
const SAMPLE_LEN: usize = 16;

/// Build a protected short-header (1-RTT) packet carrying `payload`
/// (already-serialized frames) for connection `dcid`.
pub fn build_short_packet(
    keys: &PacketKeys,
    dcid: &[u8],
    pn: u64,
    largest_acked: Option<u64>,
    payload: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let pn_len = pn_encode_len(pn, largest_acked); // 1..=4
    let payload = pad_for_sample(payload, pn_len);
    let payload = payload.as_slice();

    let mut pkt = Vec::new();
    // Short header: header form 0, fixed bit set, key phase 0, pn length in the
    // low 2 bits (RFC 9000 §17.3).
    pkt.push(0x40 | ((pn_len as u8 - 1) & 0x03));
    pkt.extend_from_slice(dcid);

    let pn_offset = pkt.len();
    let pn_be = pn.to_be_bytes();
    pkt.extend_from_slice(&pn_be[8 - pn_len..]); // truncated packet number

    // AEAD: the header (through the packet number) is the AAD.
    let header = pkt[..pn_offset + pn_len].to_vec();
    let ciphertext = seal(keys, pn, &header, payload)?;
    pkt.extend_from_slice(&ciphertext);

    // Header protection: sample the ciphertext and mask the first byte + PN.
    let sample_off = pn_offset + SAMPLE_OFFSET;
    if pkt.len() < sample_off + SAMPLE_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }
    let mask = header_protection_mask(&keys.hp, &pkt[sample_off..sample_off + SAMPLE_LEN])?;
    let mut first = pkt[0];
    let mut pn_bytes = pkt[pn_offset..pn_offset + pn_len].to_vec();
    apply_header_protection(&mask, &mut first, &mut pn_bytes, false);
    pkt[0] = first;
    pkt[pn_offset..pn_offset + pn_len].copy_from_slice(&pn_bytes);

    Ok(pkt)
}

/// Pad `payload` with PADDING (0x00) frames so the protected packet can supply
/// the 16-byte header-protection sample at `pn_offset + 4` (RFC 9001 §5.4.2):
/// the ciphertext must be ≥ `4 - pn_len + 16` bytes, i.e. the payload ≥
/// `4 - pn_len`. A PING-only or empty-ACK packet would otherwise be too short.
fn pad_for_sample(payload: &[u8], pn_len: usize) -> Vec<u8> {
    let min = 4usize.saturating_sub(pn_len);
    let mut p = Vec::with_capacity(payload.len().max(min));
    p.extend_from_slice(payload);
    if p.len() < min {
        p.resize(min, 0); // PADDING frames
    }
    p
}

/// Build a protected long-header packet (Initial or Handshake) carrying
/// `payload`. `token` is the Initial token (empty for Handshake/0-RTT and the
/// usual client-Initial-without-Retry case).
pub fn build_long_packet(
    keys: &PacketKeys,
    typ: LongPacketType,
    version: u32,
    dcid: &[u8],
    scid: &[u8],
    token: &[u8],
    pn: u64,
    largest_acked: Option<u64>,
    payload: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let type_bits: u8 = match typ {
        LongPacketType::Initial => 0,
        LongPacketType::ZeroRtt => 1,
        LongPacketType::Handshake => 2,
        LongPacketType::Retry => return Err(CryptoError::AuthenticationFailed),
    };
    let pn_len = pn_encode_len(pn, largest_acked);
    let payload = pad_for_sample(payload, pn_len);
    let payload = payload.as_slice();

    let mut pkt = Vec::new();
    // Long form + fixed bit + type + (reserved 00) + pn length.
    pkt.push(0xc0 | (type_bits << 4) | ((pn_len as u8 - 1) & 0x03));
    pkt.extend_from_slice(&version.to_be_bytes());
    pkt.push(dcid.len() as u8);
    pkt.extend_from_slice(dcid);
    pkt.push(scid.len() as u8);
    pkt.extend_from_slice(scid);
    if typ == LongPacketType::Initial {
        let mut vbuf = [0u8; 8];
        let n = super::common::encode_varint(token.len() as u64, &mut vbuf)
            .ok_or(CryptoError::AuthenticationFailed)?;
        pkt.extend_from_slice(&vbuf[..n]);
        pkt.extend_from_slice(token);
    }

    // Length covers the packet number plus the AEAD ciphertext (payload + tag).
    let length = (pn_len + payload.len() + 16) as u64;
    let mut vbuf = [0u8; 8];
    let n =
        super::common::encode_varint(length, &mut vbuf).ok_or(CryptoError::AuthenticationFailed)?;
    pkt.extend_from_slice(&vbuf[..n]);

    let pn_offset = pkt.len();
    let pn_be = pn.to_be_bytes();
    pkt.extend_from_slice(&pn_be[8 - pn_len..]);

    let header = pkt[..pn_offset + pn_len].to_vec();
    let ciphertext = seal(keys, pn, &header, payload)?;
    pkt.extend_from_slice(&ciphertext);

    let sample_off = pn_offset + SAMPLE_OFFSET;
    if pkt.len() < sample_off + SAMPLE_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }
    let mask = header_protection_mask(&keys.hp, &pkt[sample_off..sample_off + SAMPLE_LEN])?;
    let mut first = pkt[0];
    let mut pn_bytes = pkt[pn_offset..pn_offset + pn_len].to_vec();
    apply_header_protection(&mask, &mut first, &mut pn_bytes, true);
    pkt[0] = first;
    pkt[pn_offset..pn_offset + pn_len].copy_from_slice(&pn_bytes);

    Ok(pkt)
}

/// Open a protected short-header packet, returning `(packet_number, payload)`.
///
/// `dcid_len` is this endpoint's connection-ID length (the short-header DCID
/// carries no length prefix); `largest_received` drives packet-number recovery.
pub fn open_short_packet(
    keys: &PacketKeys,
    datagram: &[u8],
    dcid_len: usize,
    largest_received: Option<u64>,
) -> Result<(u64, Vec<u8>), CryptoError> {
    unprotect(
        keys,
        datagram,
        1 + dcid_len,
        datagram.len(),
        false,
        largest_received,
    )
}

/// Open a protected long-header (Initial / Handshake / 0-RTT) packet, returning
/// `(type, packet_number, payload)`. The Length field bounds this packet, so a
/// coalesced datagram is handled correctly.
pub fn open_long_packet(
    keys: &PacketKeys,
    datagram: &[u8],
    local_cid_len: usize,
    largest_received: Option<u64>,
) -> Result<(LongPacketType, u64, Vec<u8>), CryptoError> {
    let (typ, body_offset) = match parse_header(datagram, local_cid_len) {
        Some(PacketHeader::Long {
            typ, body_offset, ..
        }) => (typ, body_offset),
        _ => return Err(CryptoError::AuthenticationFailed),
    };

    // Skip the type-specific prefix to reach the Length field: Initial carries
    // a length-prefixed Token first; Handshake/0-RTT go straight to Length.
    let after_prefix = match typ {
        LongPacketType::Initial => {
            parse_initial_token(datagram, body_offset)
                .ok_or(CryptoError::AuthenticationFailed)?
                .1
        }
        LongPacketType::Handshake | LongPacketType::ZeroRtt => body_offset,
        LongPacketType::Retry => return Err(CryptoError::AuthenticationFailed),
    };

    let rest = datagram
        .get(after_prefix..)
        .ok_or(CryptoError::AuthenticationFailed)?;
    let (length, ln) = decode_varint(rest).ok_or(CryptoError::AuthenticationFailed)?;
    let pn_offset = after_prefix + ln;
    let packet_end = pn_offset
        .checked_add(length as usize)
        .ok_or(CryptoError::AuthenticationFailed)?;
    if packet_end > datagram.len() {
        return Err(CryptoError::AuthenticationFailed);
    }

    let (pn, payload) = unprotect(
        keys,
        datagram,
        pn_offset,
        packet_end,
        true,
        largest_received,
    )?;
    Ok((typ, pn, payload))
}

/// Shared header-protection removal + AEAD open. `pn_offset` is where the
/// packet number begins; `packet_end` bounds the AEAD ciphertext (the datagram
/// end for short headers, or the Length-delimited packet end for long headers);
/// `long_header` selects the 4-bit vs 5-bit first-byte mask.
fn unprotect(
    keys: &PacketKeys,
    datagram: &[u8],
    pn_offset: usize,
    packet_end: usize,
    long_header: bool,
    largest_received: Option<u64>,
) -> Result<(u64, Vec<u8>), CryptoError> {
    let sample_off = pn_offset + SAMPLE_OFFSET;
    if packet_end > datagram.len() || packet_end < sample_off + SAMPLE_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }

    let mask = header_protection_mask(&keys.hp, &datagram[sample_off..sample_off + SAMPLE_LEN])?;
    let low_mask = if long_header { 0x0f } else { 0x1f };
    let first = datagram[0] ^ (mask[0] & low_mask);
    let pn_len = ((first & 0x03) + 1) as usize;
    if pn_offset + pn_len > packet_end {
        return Err(CryptoError::AuthenticationFailed);
    }

    // Recover the truncated packet number, then expand it.
    let mut truncated = 0u64;
    for i in 0..pn_len {
        truncated = (truncated << 8) | (datagram[pn_offset + i] ^ mask[1 + i]) as u64;
    }
    let pn = pn_decode(
        largest_received.unwrap_or(0),
        truncated,
        (pn_len * 8) as u32,
    );

    // Reconstruct the unprotected header (the AAD).
    let mut header = datagram[..pn_offset + pn_len].to_vec();
    header[0] = first;
    for i in 0..pn_len {
        header[pn_offset + i] ^= mask[1 + i];
    }

    let payload = open(keys, pn, &header, &datagram[pn_offset + pn_len..packet_end])?;
    Ok((pn, payload))
}

/// Outcome of applying a received packet's frames.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FrameOutcome {
    /// An ACK should be sent (an ack-eliciting frame was received).
    pub ack_eliciting: bool,
    /// The peer signalled connection close.
    pub closed: bool,
    /// Bytes of stream data delivered to the application this packet.
    pub stream_bytes: usize,
    /// A protocol violation was detected (malformed frame, or a frame illegal
    /// for this endpoint's role) — the caller should CONNECTION_CLOSE.
    pub error: bool,
}

/// Expand an ACK frame's `(largest, first_range, [(gap, len)])` into inclusive
/// `(low, high)` packet-number ranges (RFC 9000 §19.3.1).
fn expand_ack_ranges(largest: u64, first_range: u64, ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut out = Vec::with_capacity(ranges.len() + 1);
    let mut low = largest.saturating_sub(first_range);
    out.push((low, largest));
    for &(gap, len) in ranges {
        // The next (lower) range's high is gap+2 below the current low.
        let high = match low.checked_sub(gap + 2) {
            Some(h) => h,
            None => break,
        };
        low = high.saturating_sub(len);
        out.push((low, high));
    }
    out
}

/// Walk the frames in a decrypted payload and apply them to `conn`. `now` (ms)
/// drives RTT/loss recovery when an ACK frame is processed.
pub fn apply_frames(conn: &mut Connection, payload: &[u8], now: u64) -> FrameOutcome {
    let mut outcome = FrameOutcome::default();
    let mut off = 0;
    while off < payload.len() {
        let (frame, used) = match parse_frame(&payload[off..]) {
            Some(v) => v,
            None => {
                // A malformed/unsupported frame inside an authenticated payload
                // is a FRAME_ENCODING_ERROR, not end-of-data: reject the packet
                // rather than silently accepting the prefix already applied.
                outcome.error = true;
                conn.begin_close();
                break;
            }
        };
        match frame {
            Frame::Padding(_) => {}
            Frame::Ack {
                largest,
                delay,
                first_range,
                ranges,
            } => {
                // ACK frames are not themselves ack-eliciting. Feed them to
                // loss recovery against the application PN space.
                let acked = expand_ack_ranges(largest, first_range, &ranges);
                super::recovery::on_ack_received(
                    &mut conn.pn_app,
                    &mut conn.cong,
                    &mut conn.rtt,
                    largest,
                    delay,
                    now,
                    &acked,
                );
                let _ =
                    super::recovery::detect_lost(&mut conn.pn_app, &mut conn.cong, now, &conn.rtt);
            }
            Frame::Ping => outcome.ack_eliciting = true,
            Frame::Crypto { offset, data } => {
                outcome.ack_eliciting = true;
                let _ = conn.crypto.recv_crypto(offset, data);
            }
            Frame::Stream {
                stream_id,
                offset,
                fin,
                data,
            } => {
                outcome.ack_eliciting = true;
                // Reassemble: buffer out-of-order, deliver in order.
                let delivered = conn.stream_mut(stream_id).recv(offset, data, fin);
                outcome.stream_bytes += delivered.len();
            }
            Frame::MaxData(v) => {
                conn.max_data_remote = conn.max_data_remote.max(v);
                outcome.ack_eliciting = true;
            }
            Frame::MaxStreamData { stream_id, max } => {
                outcome.ack_eliciting = true;
                let stream = conn.stream_mut(stream_id);
                stream.send_max_data = stream.send_max_data.max(max);
            }
            Frame::ResetStream { .. } | Frame::StopSending { .. } => {
                outcome.ack_eliciting = true;
            }
            Frame::ConnectionClose { .. } => {
                outcome.closed = true;
                conn.begin_close();
            }
            Frame::NewConnectionId {
                seq,
                retire_prior_to,
                cid,
                reset_token,
            } => {
                outcome.ack_eliciting = true;
                if let Some(cid) = super::connid::ConnectionId::new(cid) {
                    conn.cids.add_remote(seq, cid, reset_token, retire_prior_to);
                }
            }
            Frame::RetireConnectionId(seq) => {
                outcome.ack_eliciting = true;
                conn.cids.retire_local(seq);
            }
            Frame::HandshakeDone => {
                outcome.ack_eliciting = true;
                // HANDSHAKE_DONE is sent only by the server; receiving it on a
                // server connection is a protocol violation (RFC 9000 §19.20).
                if conn.role == Role::Client {
                    conn.on_handshake_complete();
                } else {
                    outcome.error = true;
                    conn.begin_close();
                    break;
                }
            }
        }
        off += used.max(1);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::super::keys::initial_keys;
    use super::*;

    #[test]
    fn short_packet_round_trip() {
        let (keys, _server) = initial_keys(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let dcid = [0xAAu8, 0xBB, 0xCC, 0xDD];
        let payload = b"\x01\x01\x01frame-bytes-go-here-padding-too"; // PINGs + data

        let pkt = build_short_packet(&keys, &dcid, 7, Some(0), payload).unwrap();
        // First byte is header-protected, so its low bits are masked on the wire.
        let (pn, recovered) = open_short_packet(&keys, &pkt, dcid.len(), Some(0)).unwrap();
        assert_eq!(pn, 7);
        assert_eq!(recovered, payload);
    }

    #[test]
    fn long_initial_packet_round_trip() {
        use super::super::common::QUIC_VERSION_1;
        let dcid = [0x83u8, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08];
        let (client, _server) = initial_keys(&dcid);
        let payload = b"\x06\x00\x10crypto-handshake-data-here!!"; // CRYPTO-ish bytes

        let pkt = build_long_packet(
            &client,
            LongPacketType::Initial,
            QUIC_VERSION_1,
            &dcid,
            &[],
            &[],
            1,
            None,
            payload,
        )
        .unwrap();

        // local_cid_len here is the DCID length the receiver parses from the
        // long header itself (carried on the wire), so any value works for the
        // long-header path — parse_header reads the length prefix.
        let (typ, pn, recovered) = open_long_packet(&client, &pkt, 0, None).unwrap();
        assert_eq!(typ, LongPacketType::Initial);
        assert_eq!(pn, 1);
        assert_eq!(recovered, payload);
    }

    #[test]
    fn tampered_packet_fails_auth() {
        let (keys, _server) = initial_keys(&[9, 9, 9, 9]);
        let dcid = [0u8; 4];
        let mut pkt =
            build_short_packet(&keys, &dcid, 1, Some(0), b"hello-quic-payload!!").unwrap();
        let n = pkt.len();
        pkt[n - 1] ^= 0x01; // flip a tag bit
        assert!(open_short_packet(&keys, &pkt, dcid.len(), Some(0)).is_err());
    }
}
