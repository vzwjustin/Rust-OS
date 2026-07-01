//! QUIC packet header parsing (RFC 9000 §17).
//!
//! Mirrors `net/quic/packet.{c,h}`. Parses the version-independent and v1
//! header fields needed to route a datagram to a connection. The protected
//! payload and packet number are recovered later by the crypto layer using
//! header protection (RFC 9001 §5.4), so this layer stops at the public
//! header fields.

use super::common::decode_varint;
use super::connid::{ConnectionId, MAX_CONNID_LEN};

/// Long header packet types in QUIC v1 (RFC 9000 §17.2, table in the type bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongPacketType {
    Initial,
    ZeroRtt,
    Handshake,
    Retry,
}

impl LongPacketType {
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => LongPacketType::Initial,
            1 => LongPacketType::ZeroRtt,
            2 => LongPacketType::Handshake,
            _ => LongPacketType::Retry,
        }
    }
}

/// A parsed QUIC public header.
#[derive(Debug, Clone)]
pub enum PacketHeader {
    /// Long header (used during the handshake and for version negotiation).
    Long {
        typ: LongPacketType,
        version: u32,
        dcid: ConnectionId,
        scid: ConnectionId,
        /// Offset into the datagram where type-specific fields (token/length/
        /// packet number/payload) begin.
        body_offset: usize,
    },
    /// Short header (1-RTT) — carries only the destination connection ID, whose
    /// length is fixed by the receiver and so must be supplied by the caller.
    Short {
        spin: bool,
        key_phase: bool,
        dcid: ConnectionId,
        body_offset: usize,
    },
    /// Version Negotiation packet (long header form, version == 0).
    VersionNegotiation {
        dcid: ConnectionId,
        scid: ConnectionId,
        body_offset: usize,
    },
}

const HEADER_FORM_LONG: u8 = 0x80;
const FIXED_BIT: u8 = 0x40;

/// Parse the public header from the front of `buf`.
///
/// `local_cid_len` is the length of this endpoint's connection IDs, required to
/// locate the DCID in a short header (which carries no length prefix).
pub fn parse_header(buf: &[u8], local_cid_len: usize) -> Option<PacketHeader> {
    let first = *buf.first()?;

    if first & HEADER_FORM_LONG == 0 {
        // Short header (RFC 9000 §17.3).
        if first & FIXED_BIT == 0 {
            return None; // QUIC bit must be set for valid 1-RTT packets.
        }
        if local_cid_len > MAX_CONNID_LEN {
            return None;
        }
        let dcid_start = 1;
        let dcid_end = dcid_start + local_cid_len;
        if buf.len() < dcid_end {
            return None;
        }
        let dcid = ConnectionId::new(&buf[dcid_start..dcid_end])?;
        return Some(PacketHeader::Short {
            spin: first & 0x20 != 0,
            key_phase: first & 0x04 != 0,
            dcid,
            body_offset: dcid_end,
        });
    }

    // Long header (RFC 9000 §17.2): first byte, 4-byte version, then
    // length-prefixed DCID and SCID.
    if buf.len() < 6 {
        return None;
    }
    let version = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);

    let mut off = 5;
    let dcid_len = buf[off] as usize;
    off += 1;
    if dcid_len > MAX_CONNID_LEN || buf.len() < off + dcid_len + 1 {
        return None;
    }
    let dcid = ConnectionId::new(&buf[off..off + dcid_len])?;
    off += dcid_len;

    let scid_len = buf[off] as usize;
    off += 1;
    if scid_len > MAX_CONNID_LEN || buf.len() < off + scid_len {
        return None;
    }
    let scid = ConnectionId::new(&buf[off..off + scid_len])?;
    off += scid_len;

    if version == super::common::QUIC_VERSION_NEGOTIATION {
        return Some(PacketHeader::VersionNegotiation {
            dcid,
            scid,
            body_offset: off,
        });
    }

    // The fixed bit must be set on v1 long-header packets.
    if first & FIXED_BIT == 0 {
        return None;
    }

    Some(PacketHeader::Long {
        typ: LongPacketType::from_bits(first >> 4),
        version,
        dcid,
        scid,
        body_offset: off,
    })
}

/// For an Initial packet, parse the Token (length-prefixed) that follows the
/// SCID, returning `(token, offset_after_token)` (RFC 9000 §17.2.2).
pub fn parse_initial_token(buf: &[u8], body_offset: usize) -> Option<(&[u8], usize)> {
    let (token_len, n) = decode_varint(buf.get(body_offset..)?)?;
    let start = body_offset + n;
    let end = start.checked_add(token_len as usize)?;
    if end > buf.len() {
        return None;
    }
    Some((&buf[start..end], end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_long_initial() {
        // first byte 0xC0 = long form + fixed bit + Initial(00); version 1;
        // DCID len 4 ["aaaa"]; SCID len 0.
        let mut pkt = alloc::vec![0xC0u8, 0, 0, 0, 1, 4, b'a', b'a', b'a', b'a', 0];
        pkt.push(0x00); // start of token-length varint (0)
        let hdr = parse_header(&pkt, 0).unwrap();
        match hdr {
            PacketHeader::Long {
                typ,
                version,
                dcid,
                scid,
                body_offset,
            } => {
                assert_eq!(typ, LongPacketType::Initial);
                assert_eq!(version, 1);
                assert_eq!(dcid.as_bytes(), b"aaaa");
                assert!(scid.is_empty());
                assert_eq!(body_offset, 11);
            }
            _ => panic!("expected long header"),
        }
    }

    #[test]
    fn parse_short_header() {
        // 0x40 = short form + fixed bit; DCID len fixed at 2.
        let pkt = alloc::vec![0x40u8, 0xAB, 0xCD, 0x00];
        let hdr = parse_header(&pkt, 2).unwrap();
        match hdr {
            PacketHeader::Short {
                dcid, body_offset, ..
            } => {
                assert_eq!(dcid.as_bytes(), &[0xAB, 0xCD]);
                assert_eq!(body_offset, 3);
            }
            _ => panic!("expected short header"),
        }
    }
}
